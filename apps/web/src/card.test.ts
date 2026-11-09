import { describe, expect, it } from "vitest";
import { card, section, steps } from "./card";

describe("card", () => {
  it("asks for Chrome or Edge without WebUSB, and offers nothing to press", () => {
    const shown = card({ kind: "idle" }, false);
    expect(shown.title).toBe("Use Chrome or Edge");
    expect(shown.action).toBeUndefined();
    expect(shown.grille).toBe("off");
  });

  it("waits with the outlined card while the headset asks", () => {
    const shown = card({ kind: "asking", step: "allow" }, true);
    expect(shown.look).toBe("waiting");
    expect(shown.grille).toBe("full");
    expect(shown.sentence).toContain("Always allow from this computer");
  });

  it("lights the grille with the install's progress instead of sweeping it", () => {
    expect(card({ kind: "working", step: "install", progress: 0.5 }, true).grille).toBe(0.5);
    expect(card({ kind: "working", step: "setup" }, true).grille).toBe("sweep");
  });

  it("offers setting up another headset as the quieter button once done", () => {
    expect(card({ kind: "done", model: "Quest 3" }, true).action).toEqual({ label: "Set up another headset", style: "secondary" });
  });

  it("says what went wrong as a sentence, and offers to try again", () => {
    const shown = card({ kind: "failed", step: "install", reason: { kind: "error", message: "couldn't download Setu (404)" } }, true);
    expect(shown).toMatchObject({
      title: "Couldn't install",
      sentence: "Couldn't download Setu (404).",
      problem: true,
      action: { label: "Try again", style: "primary" },
    });
  });
});

describe("steps", () => {
  it("says why each step is there", () => {
    expect(steps({ kind: "idle" }).every((row) => row.detail.length > 0)).toBe(true);
  });

  it("marks the steps before the current one done, with a check", () => {
    const shown = steps({ kind: "asking", step: "allow" });
    expect(shown.map((s) => s.state)).toEqual(["done", "done", "done", "current"]);
    expect(shown.map((s) => s.icon)).toEqual(["check", "check", "check", "headset"]);
  });

  it("opens the asking step with the prompt, and says to choose Always allow", () => {
    const help = steps({ kind: "asking", step: "allow" })[3].help;
    expect(help?.prompt).toBe(true);
    expect(help?.items.join(" ")).toContain("Always allow from this computer");
  });

  it("opens only the current step", () => {
    expect(steps({ kind: "asking", step: "connect" }).filter((s) => s.help)).toHaveLength(1);
  });

  it("keeps a failed step current, with how to fix it", () => {
    const shown = steps({ kind: "failed", step: "connect", reason: { kind: "busy" } });
    expect(shown.map((s) => s.state)).toEqual(["current", "todo", "todo", "todo"]);
    expect(shown[0].note).toBe("Stopped");
    expect(shown[0].help?.items.join(" ")).toContain("adb kill-server");
  });

  it("shows the install's progress beside its step", () => {
    expect(steps({ kind: "working", step: "install", progress: 0.42 })[1].note).toBe("42%");
  });

  it("marks every step done when setup finished", () => {
    expect(steps({ kind: "done", model: "Quest 3" }).every((s) => s.state === "done")).toBe(true);
  });
});

describe("section", () => {
  it("lists what to have ready before connecting", () => {
    expect(section({ kind: "idle" }, true)?.title).toBe("Before you start");
  });

  it("says what to do next once set up, leading to the desktop app", () => {
    const next = section({ kind: "done", model: "Quest 3" }, true);
    expect(next?.rows.map((r) => r.title)).toContain("Tap Start");
    expect(next?.rows.some((r) => r.href)).toBe(true);
  });

  it("keeps out of the way while setup runs", () => {
    expect(section({ kind: "working", step: "install", progress: 0.1 }, true)).toBeUndefined();
  });
});
