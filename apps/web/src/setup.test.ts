import { describe, expect, it } from "vitest";
import {
  HeadsetBusy,
  LEGACY_PORT,
  SETUP_COMMAND,
  SETUP_DONE,
  TCP_PORT_PROP,
  installFailure,
  runSetup,
  type Apk,
  type Headset,
  type Status,
  type Usb,
} from "./setup";

const APK: Apk = { size: 4, stream: () => new Blob([new Uint8Array(4)]).stream() };
const FAST = { allowMs: 200, pollMs: 5 };

/**
 * A headset whose adbd behaves like the real one: `tcpip` and `usb` restart it, ending the
 * connection. [allowAfter] is how many polls of the port pass before the agent, approved,
 * returns adbd to USB; null never.
 */
class FakeDevice {
  port = "";
  log: string[] = [];
  polls = 0;
  connections = 0;
  current?: FakeHeadset;
  /** What `pm install` printed when it refused the APK, or nothing when it took it. */
  installRefused?: string;
  /** Whether a read on an ended connection never answers, instead of failing. */
  hangsWhenGone = false;
  /** Whether adbd restarts while the setup routine runs, its answer never arriving. */
  dropsDuringSetup = false;
  /** Whether the headset asks to allow the browser again after adbd restarts. */
  asksOnReconnect = false;

  constructor(
    readonly allowAfter: number | null = 2,
    readonly setupOutput = `setup: pm grant …\n${SETUP_DONE}\n`,
  ) {}

  connect(): FakeHeadset {
    this.connections++;
    this.current = new FakeHeadset(this);
    return this.current;
  }

  restart() {
    this.current?.drop();
  }
}

class FakeHeadset implements Headset {
  readonly serial = "2G97C5ZH2T013J";
  readonly model = "Quest 3";
  readonly disconnected: Promise<void>;
  private end!: () => void;
  private gone = false;

  constructor(private readonly device: FakeDevice) {
    this.disconnected = new Promise((resolve) => (this.end = resolve));
  }

  drop() {
    this.gone = true;
    this.end();
  }

  private check() {
    if (this.gone) throw new Error("disconnected");
  }

  async install(_apk: Apk, progress: (sent: number) => void) {
    this.check();
    if (this.device.installRefused) throw new Error(this.device.installRefused);
    this.device.log.push("install");
    progress(1);
  }

  async shell(command: string) {
    this.check();
    this.device.log.push(command);
    if (command === SETUP_COMMAND && this.device.dropsDuringSetup) {
      this.device.restart();
      return new Promise<string>(() => {});
    }
    return command === SETUP_COMMAND ? this.device.setupOutput : "";
  }

  async getProp(key: string) {
    if (this.gone && this.device.hangsWhenGone) return new Promise<string>(() => {});
    this.check();
    expect(key).toBe(TCP_PORT_PROP);
    const { device } = this;
    device.polls++;
    if (device.port === String(LEGACY_PORT) && device.allowAfter !== null && device.polls > device.allowAfter) {
      device.log.push("agent: usb");
      device.port = "0";
      device.restart();
      this.check();
    }
    return device.port;
  }

  async openTcpPort(port: number) {
    this.check();
    this.device.log.push(`tcpip ${port}`);
    this.device.port = String(port);
    // adbd answers, then restarts.
    queueMicrotask(() => this.device.restart());
  }

  async closeTcpPort() {
    this.check();
    this.device.log.push("usb");
    this.device.port = "0";
    queueMicrotask(() => this.device.restart());
  }

  async close() {}
}

function usb(device: FakeDevice, choose: "pick" | "cancel" | "busy" = "pick"): Usb {
  return {
    async choose(authorizing) {
      if (choose === "cancel") return undefined;
      if (choose === "busy") throw new HeadsetBusy();
      authorizing();
      return device.connect();
    },
    async reconnect(_previous, _timeoutMs, authorizing) {
      if (device.asksOnReconnect) authorizing();
      return device.connect();
    },
  };
}

async function run(device: FakeDevice, choose?: "pick" | "cancel" | "busy") {
  const statuses: Status[] = [];
  await runSetup(usb(device, choose), async () => APK, (s) => statuses.push(s), FAST);
  return statuses;
}

describe("runSetup", () => {
  it("installs, runs the APK's setup, opens the port and waits for the agent to close it", async () => {
    const device = new FakeDevice();
    const statuses = await run(device);
    expect(device.log).toEqual(["install", SETUP_COMMAND, "tcpip 5555", "agent: usb"]);
    expect(statuses.at(-1)).toEqual({ kind: "done", model: "Quest 3" });
    expect(statuses).toContainEqual({ kind: "asking", step: "connect" });
    expect(statuses).toContainEqual({ kind: "asking", step: "allow" });
    // Once after tcpip restarted adbd, once after the agent's usb did.
    expect(device.connections).toBe(3);
  });

  it("reconnects when a read on the ended connection never answers", async () => {
    const device = new FakeDevice();
    device.hangsWhenGone = true;
    const statuses = await run(device);
    expect(statuses.at(-1)).toEqual({ kind: "done", model: "Quest 3" });
  });

  it("closes a port an earlier setup left open before running setup", async () => {
    const device = new FakeDevice();
    device.port = String(LEGACY_PORT);
    const statuses = await run(device);
    expect(device.log).toEqual(["install", "usb", SETUP_COMMAND, "tcpip 5555", "agent: usb"]);
    expect(statuses.at(-1)).toEqual({ kind: "done", model: "Quest 3" });
  });

  it("says the headset asks again when closing that port forgets this browser", async () => {
    const device = new FakeDevice();
    device.port = String(LEGACY_PORT);
    device.asksOnReconnect = true;
    const statuses = await run(device);
    expect(statuses).toContainEqual({ kind: "asking", step: "setup" });
    expect(statuses.at(-1)).toEqual({ kind: "done", model: "Quest 3" });
  });

  it("stops instead of waiting when the connection ends during a command", async () => {
    const device = new FakeDevice();
    device.dropsDuringSetup = true;
    const statuses = await run(device);
    expect(statuses.at(-1)).toMatchObject({ kind: "failed", step: "setup", reason: { kind: "error" } });
  });

  it("stops at install with why the headset refused it", async () => {
    const device = new FakeDevice();
    device.installRefused = "Failure [INSTALL_FAILED_INSUFFICIENT_STORAGE]";
    const statuses = await run(device);
    expect(device.log).toEqual([]);
    expect(statuses.at(-1)).toEqual({
      kind: "failed",
      step: "install",
      reason: { kind: "error", message: "the headset is out of storage. Free some space, then try again" },
    });
  });

  it("finishes when the agent was faster than the reconnect", async () => {
    const device = new FakeDevice(0);
    const statuses = await run(device);
    expect(statuses.at(-1)).toEqual({ kind: "done", model: "Quest 3" });
  });

  it("closes the port itself when the headset never allows", async () => {
    const device = new FakeDevice(null);
    const statuses = await run(device);
    expect(device.log.at(-1)).toBe("usb");
    expect(statuses.at(-1)).toEqual({ kind: "failed", step: "allow", reason: { kind: "notAllowed" } });
  });

  it("stops at setup when the routine fails, with its reason", async () => {
    const device = new FakeDevice(2, "setup: failed: pm grant dev.trugamr.setu …: Unknown package\n");
    const statuses = await run(device);
    expect(device.log).not.toContain("tcpip 5555");
    expect(statuses.at(-1)).toEqual({
      kind: "failed",
      step: "setup",
      reason: { kind: "error", message: "failed: pm grant dev.trugamr.setu …: Unknown package" },
    });
  });

  it("says when another program holds the headset", async () => {
    const statuses = await run(new FakeDevice(), "busy");
    expect(statuses.at(-1)).toEqual({ kind: "failed", step: "connect", reason: { kind: "busy" } });
  });

  it("goes back to the start when no headset is picked", async () => {
    const statuses = await run(new FakeDevice(), "cancel");
    expect(statuses.at(-1)).toEqual({ kind: "idle" });
  });
});

describe("installFailure", () => {
  it("tells the user to uninstall a Setu signed by another build", () => {
    const output =
      "Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Package dev.trugamr.setu signatures do not match newer version; ignoring!]";
    expect(installFailure(output)).toContain("Uninstall Setu");
  });

  it("names a refusal it has no words for", () => {
    expect(installFailure("Failure [INSTALL_FAILED_TEST_ONLY: installPackageLI]")).toBe(
      "the headset refused Setu (INSTALL_FAILED_TEST_ONLY)",
    );
  });

  it("passes on output that isn't pm's", () => {
    expect(installFailure("device offline")).toBe("device offline");
  });
});
