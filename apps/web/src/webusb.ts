// The Usb and Headset adapters: WebUSB through Tango (@yume-chan/adb). The browser keeps its own
// adb key in IndexedDB, which the headset asks to allow the first time.

import { Adb, AdbDaemonTransport } from "@yume-chan/adb";
import AdbWebCredentialStore from "@yume-chan/adb-credential-web";
import { AdbDaemonWebUsbDevice, AdbDaemonWebUsbDeviceManager } from "@yume-chan/adb-daemon-webusb";
import { PackageManager } from "@yume-chan/android-bin";
import type { ReadableStream as TangoReadableStream } from "@yume-chan/stream-extra";
import { HeadsetBusy, isApk, sleep, type Apk, type Headset, type Usb } from "./setup";

const credentials = new AdbWebCredentialStore("Setu installer");
/** How long authentication runs before the headset is taken to be asking to allow this browser. */
const ASKING_AFTER_MS = 1000;

/** WebUSB, or undefined when the browser has none. */
export function webUsb(): Usb | undefined {
  const manager = AdbDaemonWebUsbDeviceManager.BROWSER;
  if (!manager) return undefined;
  return {
    async choose(authorizing) {
      const device = await manager.requestDevice();
      return device && connect(device, authorizing);
    },
    async reconnect(previous, timeoutMs, authorizing) {
      const deadline = Date.now() + timeoutMs;
      while (Date.now() < deadline) {
        const device = (await manager.getDevices()).find((d) => d.serial === previous.serial);
        if (device) {
          try {
            return await connect(device, authorizing);
          } catch {
            // Back on the bus with adbd not up yet, or the browser still holding the old
            // connection: try again.
          }
        }
        await sleep(500);
      }
      throw new Error("The headset didn't come back on USB. Check the cable, then try again.");
    },
  };
}

async function connect(device: AdbDaemonWebUsbDevice, authorizing: () => void): Promise<UsbHeadset> {
  let connection;
  try {
    connection = await device.connect();
  } catch (e) {
    if (e instanceof AdbDaemonWebUsbDevice.DeviceBusyError) throw new HeadsetBusy();
    throw e;
  }
  // The headset shows "Allow USB debugging?" for a key it hasn't allowed, and authentication waits
  // for the answer. An allowed key finishes at once, so only a slow one means the headset asks.
  const asking = setTimeout(authorizing, ASKING_AFTER_MS);
  let transport;
  try {
    transport = await AdbDaemonTransport.authenticate({
      serial: device.serial,
      connection,
      credentialStore: credentials,
    });
  } finally {
    clearTimeout(asking);
  }
  const adb = new Adb(transport);
  const model = (await adb.getProp("ro.product.model")).trim();
  return new UsbHeadset(device.serial, adb, model);
}

class UsbHeadset implements Headset {
  constructor(
    readonly serial: string,
    private readonly adb: Adb,
    readonly model: string,
  ) {}

  get disconnected() {
    return this.adb.disconnected;
  }

  async install(apk: Apk, progress: (sent: number) => void) {
    let sent = 0;
    const counted = apk.stream().pipeThrough(
      new TransformStream<Uint8Array, Uint8Array>({
        transform(chunk, controller) {
          sent += chunk.byteLength;
          progress(sent / apk.size);
          controller.enqueue(chunk);
        },
      }),
    );
    // Tango types its streams with async iteration, which the browser's streams have at runtime.
    await new PackageManager(this.adb).installStream(apk.size, counted as unknown as TangoReadableStream<Uint8Array>);
  }

  shell(command: string) {
    return this.adb.subprocess.noneProtocol.spawnWaitText(command);
  }

  getProp(key: string) {
    return this.adb.getProp(key);
  }

  async openTcpPort(port: number) {
    await this.adb.tcpip.setPort(port);
  }

  async closeTcpPort() {
    await this.adb.tcpip.disable();
  }

  close() {
    return this.adb.close();
  }
}

/** The agent's APK, served next to the page. */
export async function fetchApk(): Promise<Apk> {
  const response = await fetch(`${import.meta.env.BASE_URL}setu-agent.apk`);
  if (!response.ok) throw new Error(`couldn't download Setu (${response.status})`);
  const blob = await response.blob();
  if (!isApk(new Uint8Array(await blob.slice(0, 4).arrayBuffer()))) {
    throw new Error("couldn't download Setu: the page has no APK next to it");
  }
  return { size: blob.size, stream: () => blob.stream() };
}
