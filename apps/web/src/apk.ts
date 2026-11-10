// The agent's APK, served next to the page: by Vite from the agent's build (vite.config.ts).

import type { Apk } from "./setup";

/** Downloads the APK, refusing anything that isn't one. */
export async function fetchApk(): Promise<Apk> {
  const response = await fetch(`${import.meta.env.BASE_URL}setu-agent.apk`);
  if (!response.ok) throw new Error(`couldn't download Setu (${response.status})`);
  const blob = await response.blob();
  if (!isApk(new Uint8Array(await blob.slice(0, 4).arrayBuffer()))) {
    throw new Error("couldn't download Setu: the page has no APK next to it");
  }
  return { size: blob.size, stream: () => blob.stream() };
}

/**
 * Whether a file starts as an APK does, with a zip's local header. A server without the APK may
 * answer its URL with a page instead, as Vite's does.
 */
function isApk(start: Uint8Array): boolean {
  return start[0] === 0x50 && start[1] === 0x4b && start[2] === 0x03 && start[3] === 0x04;
}
