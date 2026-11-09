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
  /** Connects again to the headset [previous] was, once it is back on USB after adbd restarted. */
  reconnect(previous: Headset, timeoutMs: number): Promise<Headset>;
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
  | { kind: "asking"; step: "connect" | "allow" }
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

export const DEFAULT_TIMING: Timing = { allowMs: 90_000, pollMs: 1000 };

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
    await headset.install(apk, (progress) => report({ kind: "working", step: "install", progress }));

    step = "setup";
    report({ kind: "working", step });
    const output = (await headset.shell(SETUP_COMMAND)).trim();
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
      port = (await headset.getProp(TCP_PORT_PROP)).trim();
    } catch {
      await headset.close().catch(() => {});
      headset = await usb.reconnect(headset, left);
      continue;
    }
    if (port !== String(LEGACY_PORT)) return headset;
    await Promise.race([headset.disconnected, sleep(timing.pollMs)]);
  }
}

class NotAllowed extends Error {}

function failure(e: unknown): Failure {
  if (e instanceof HeadsetBusy) return { kind: "busy" };
  if (e instanceof NotAllowed) return { kind: "notAllowed" };
  return { kind: "error", message: e instanceof Error ? e.message : String(e) };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
