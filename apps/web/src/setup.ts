// The installer's flow, free of WebUSB so it can be tested with a fake headset: install the
// agent, run the setup routine inside its APK, then open adbd's TCP port so the headset asks to
// allow the agent's key. `task agent:setup` runs the same steps with adb.

/** The setup routine shipped in the agent's APK (`server/Setup.java`). */
export const SETUP_COMMAND =
  "CLASSPATH=$(pm path dev.trugamr.setu | cut -d: -f2) app_process / dev.trugamr.setu.server.Server setup";
/** The routine's last line when every step succeeded. */
export const SETUP_DONE = "setup: done";
/** The port the agent waits for; it connects there so the headset asks to allow its key. */
export const LEGACY_PORT = 5555;
/** adbd's TCP port: the port while it listens, 0 or empty once the agent returned it to USB. */
export const TCP_PORT_PROP = "service.adb.tcp.port";

/** One headset, connected and authorised over USB. */
export interface Headset {
  /** The USB serial number, which finds the headset again after adbd restarts. */
  readonly serial: string;
  /** The model, such as "Quest 3". */
  readonly model: string;
  /** Resolves when the connection ends, such as when adbd restarts. */
  readonly disconnected: Promise<void>;
  /** Installs an APK, reporting how much of it has been sent, from 0 to 1. */
  install(apk: Apk, progress: (sent: number) => void): Promise<void>;
  /** Runs a shell command and returns what it printed. */
  shell(command: string): Promise<string>;
  getProp(key: string): Promise<string>;
  /** `adb tcpip <port>`: adbd restarts listening on the port, which ends this connection. */
  openTcpPort(port: number): Promise<void>;
  /** `adb usb`: adbd restarts on USB only. */
  closeTcpPort(): Promise<void>;
  close(): Promise<void>;
}

export interface Usb {
  /**
   * Asks the user to pick the headset, then connects. [authorizing] runs when the headset may be
   * asking to allow this browser. Resolves undefined when the user picks nothing.
   */
  choose(authorizing: () => void): Promise<Headset | undefined>;
  /**
   * Connects again to the headset [previous] was, once it is back on USB after adbd restarted.
   * [authorizing] runs when the headset may be asking to allow this browser again, as it does
   * after a restart when it was allowed only once.
   */
  reconnect(previous: Headset, timeoutMs: number, authorizing: () => void): Promise<Headset>;
}

export interface Apk {
  readonly size: number;
  stream(): ReadableStream<Uint8Array>;
}

/** Another program holds the headset's USB interface, such as adb or Meta Quest Link. */
export class HeadsetBusy extends Error {}

export type Step = "connect" | "install" | "setup" | "allow";
export const STEPS: readonly Step[] = ["connect", "install", "setup", "allow"];

export type Status =
  | { kind: "idle" }
  | { kind: "working"; step: Step; progress?: number }
  /** Waiting for the user to choose Allow on the headset. */
  | { kind: "asking"; step: "connect" | "setup" | "allow" }
  | { kind: "done"; model: string }
  | { kind: "failed"; step: Step; reason: Failure };

export type Failure =
  | { kind: "busy" }
  | { kind: "notAllowed" }
  | { kind: "error"; message: string };

export interface Timing {
  /** How long the headset has to allow the agent once the port is open. */
  allowMs: number;
  /** How often to check whether it has. */
  pollMs: number;
}

/**
 * Outlasts the agent's approval (LocalAdb.kt): up to MAX_ASKS (3) prompts of APPROVE_TIMEOUT_S
 * (60 s) each, plus REMEMBER_WAIT_MS (2 s) for the key to be saved, inside APPROVE_WINDOW_MS
 * (180 s). Change them together.
 */
export const DEFAULT_TIMING: Timing = { allowMs: 200_000, pollMs: 1000 };

/** Runs setup from the start, reporting each change. Call it straight from a click: it picks the device first. */
export async function runSetup(
  usb: Usb,
  loadApk: () => Promise<Apk>,
  report: (status: Status) => void,
  timing: Timing = DEFAULT_TIMING,
): Promise<void> {
  let step: Step = "connect";
  let headset: Headset | undefined;
  try {
    report({ kind: "working", step });
    // Before any other await: the browser shows its device list only during a click.
    headset = await usb.choose(() => report({ kind: "asking", step: "connect" }));
    if (!headset) {
      report({ kind: "idle" });
      return;
    }

    step = "install";
    report({ kind: "working", step, progress: 0 });
    const apk = await loadApk();
    const installing = headset;
    await whileConnected(
      installing,
      installing.install(apk, (progress) => report({ kind: "working", step: "install", progress })).catch((e) => {
        throw new Error(installFailure(e instanceof Error ? e.message : String(e)));
      }),
    );

    step = "setup";
    report({ kind: "working", step });
    // A port an earlier setup left open lets the agent approve and restart adbd before this one
    // opens it, ending the connection mid-command. Close it first, so the steps run in order.
    if (isOpen(await whileConnected(headset, headset.getProp(TCP_PORT_PROP)))) {
      await headset.closeTcpPort();
      headset = await reconnect(usb, headset, timing.allowMs, () => report({ kind: "asking", step: "setup" }));
      report({ kind: "working", step });
    }
    const output = (await whileConnected(headset, headset.shell(SETUP_COMMAND))).trim();
    if (!output.endsWith(SETUP_DONE)) {
      const last = output.split("\n").pop()?.replace(/^setup: /, "") || "the setup routine printed nothing";
      throw new Error(last);
    }

    step = "allow";
    await headset.openTcpPort(LEGACY_PORT);
    report({ kind: "asking", step });
    headset = await awaitAllowed(usb, headset, timing);
    report({ kind: "done", model: headset.model });
  } catch (e) {
    report({ kind: "failed", step, reason: failure(e) });
  } finally {
    await headset?.close().catch(() => {});
  }
}

/**
 * Waits for the agent to return adbd to USB, which it does once the headset allowed its key.
 * adbd restarts twice on the way, for `tcpip` and then for `usb`, so this reconnects as needed.
 * Returns the headset as last connected. On timeout, closes the port itself.
 */
async function awaitAllowed(usb: Usb, headset: Headset, timing: Timing): Promise<Headset> {
  const deadline = Date.now() + timing.allowMs;
  while (true) {
    const left = deadline - Date.now();
    if (left <= 0) {
      await headset.closeTcpPort().catch(() => {});
      throw new NotAllowed();
    }
    let port: string;
    try {
      // A read on a connection that adbd's restart ended may never answer.
      port = (await whileConnected(headset, headset.getProp(TCP_PORT_PROP))).trim();
    } catch {
      // The page already asks the user to allow on the headset.
      headset = await reconnect(usb, headset, left, () => {});
      continue;
    }
    if (port !== String(LEGACY_PORT)) return headset;
    await Promise.race([headset.disconnected, sleep(timing.pollMs)]);
  }
}

class NotAllowed extends Error {}

function isOpen(port: string): boolean {
  const value = port.trim();
  return value !== "" && value !== "0";
}

/** Closes [headset] and connects again once adbd, restarting, is back on USB. */
async function reconnect(usb: Usb, headset: Headset, timeoutMs: number, authorizing: () => void): Promise<Headset> {
  await headset.close().catch(() => {});
  return usb.reconnect(headset, timeoutMs, authorizing);
}

/** [work], failing instead of waiting forever when the connection ends first. */
function whileConnected<T>(headset: Headset, work: Promise<T>): Promise<T> {
  return Promise.race([
    work,
    headset.disconnected.then(() => {
      throw new Error("the headset disconnected. Check the cable, then try again");
    }),
  ]);
}

/**
 * Says why `pm install` refused the APK, from what it printed, such as
 * "Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Package … signatures do not match …]".
 */
export function installFailure(output: string): string {
  const code = /\[?(INSTALL_[A-Z_]+)/.exec(output)?.[1];
  switch (code) {
    case undefined:
      return output.trim() || "the headset refused Setu";
    case "INSTALL_FAILED_UPDATE_INCOMPATIBLE":
      return (
        "Setu on the headset was installed from another build, so it can't be updated. " +
        "Uninstall Setu from the headset's Library, under Unknown sources, then try again"
      );
    case "INSTALL_FAILED_VERSION_DOWNGRADE":
      return "the headset has a newer Setu than this page. Uninstall it from the headset, then try again";
    case "INSTALL_FAILED_INSUFFICIENT_STORAGE":
      return "the headset is out of storage. Free some space, then try again";
    default:
      return `the headset refused Setu (${code})`;
  }
}

function failure(e: unknown): Failure {
  if (e instanceof HeadsetBusy) return { kind: "busy" };
  if (e instanceof NotAllowed) return { kind: "notAllowed" };
  return { kind: "error", message: e instanceof Error ? e.message : String(e) };
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
