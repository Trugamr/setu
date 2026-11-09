import { describe, expect, it } from "vitest";
import { card, steps } from "./card";

describe("card", () => {
  it("asks for Chrome or Edge without WebUSB, and offers nothing to press", () => {
    const shown = card({ kind: "idle" }, false);
    expect(shown.title).toBe("Use Chrome or Edge");
    expect(shown.action).toBeUndefined();
  });

  it("waits with the outlined card while the headset asks", () => {
    const shown = card({ kind: "asking", step: "allow" }, true);
    expect(shown.look).toBe("waiting");
    expect(shown.sentence).toContain("Always allow from this computer");
  });

  it("lights the grille with the install's progress instead of sweeping it", () => {
    const shown = card({ kind: "working", step: "install", progress: 0.5 }, true);
    expect(shown.busy).toBe(false);
    expect(shown.progress).toBe(0.5);
  });

  it("says what went wrong as a sentence, and offers to try again", () => {
    const shown = card({ kind: "failed", step: "install", reason: { kind: "error", message: "couldn't download Setu (404)" } }, true);
    expect(shown).toMatchObject({ title: "Couldn't install", sentence: "Couldn't download Setu (404).", problem: true, action: "Try again" });
  });
});

describe("steps", () => {
  it("marks the steps before the current one done", () => {
    expect(steps({ kind: "asking", step: "allow" }).map((s) => s.state)).toEqual(["done", "done", "done", "current"]);
  });

  it("keeps a failed step current", () => {
    expect(steps({ kind: "failed", step: "setup", reason: { kind: "notAllowed" } }).map((s) => s.state)).toEqual([
      "done",
      "done",
      "current",
      "todo",
    ]);
  });

  it("marks every step done when setup finished", () => {
    expect(steps({ kind: "done", model: "Quest 3" }).every((s) => s.state === "done")).toBe(true);
  });
});
