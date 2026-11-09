import { describe, expect, it } from "vitest";
import {
  HeadsetBusy,
  LEGACY_PORT,
  SETUP_COMMAND,
  SETUP_DONE,
  TCP_PORT_PROP,
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
    this.device.log.push("install");
    progress(1);
  }

  async shell(command: string) {
    this.check();
    this.device.log.push(command);
    return command === SETUP_COMMAND ? this.device.setupOutput : "";
  }

  async getProp(key: string) {
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
    async reconnect() {
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
