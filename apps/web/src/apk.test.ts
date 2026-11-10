import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchApk } from "./apk";

function serve(body: BodyInit, status = 200) {
  vi.stubGlobal("fetch", async () => new Response(body, { status }));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("fetchApk", () => {
  it("returns the APK with its size", async () => {
    serve(new Uint8Array([0x50, 0x4b, 0x03, 0x04, 0, 0]));
    expect((await fetchApk()).size).toBe(6);
  });

  it("refuses a page served in the APK's place", async () => {
    serve("<!doctype html>");
    await expect(fetchApk()).rejects.toThrow("the page has no APK next to it");
  });

  it("refuses an empty file", async () => {
    serve(new Uint8Array());
    await expect(fetchApk()).rejects.toThrow("the page has no APK next to it");
  });

  it("says when the download failed", async () => {
    serve("Build the agent first", 404);
    await expect(fetchApk()).rejects.toThrow("couldn't download Setu (404)");
  });
});
