import { describe, expect, it } from "vitest";
import { page } from "./page";
import type { Status } from "./setup";

const shown = (status: Status, supported = true) => page(status, supported);

describe("card", () => {
  it("asks for Chrome or Edge without WebUSB, and offers nothing to press", () => {
    const { card } = shown({ kind: "idle" }, false);
    expect(card.title).toBe("Use Chrome or Edge");
    expect(card.action).toBeUndefined();
    expect(card.grille).toBe("off");
  });

  it("waits with the outlined card while the headset asks", () => {
    const { card } = shown({ kind: "asking", step: "allow" });
    expect(card.look).toBe("waiting");
    expect(card.grille).toBe("full");
    expect(card.sentence).toContain("Always allow from this computer");
  });

  it("lights the grille with the install's progress instead of sweeping it", () => {
    expect(shown({ kind: "working", step: "install", progress: 0.5 }).card.grille).toBe(0.5);
    expect(shown({ kind: "working", step: "setup" }).card.grille).toBe("sweep");
  });

  it("offers setting up another headset as the quieter button once done", () => {
    expect(shown({ kind: "done", model: "Quest 3" }).card.action).toEqual({
      label: "Set up another headset",
      style: "secondary",
    });
  });

  it("says what went wrong as a sentence, and offers to try again", () => {
    const { card } = shown({ kind: "failed", step: "install", reason: { kind: "error", message: "couldn't download Setu (404)" } });
    expect(card).toMatchObject({
      look: "problem",
      title: "Couldn't install",
      sentence: "Couldn't download Setu (404).",
      action: { label: "Try again", style: "primary" },
    });
  });
});

describe("steps", () => {
  it("says why each step is there", () => {
    expect(shown({ kind: "idle" }).steps.every((row) => row.detail.length > 0)).toBe(true);
  });

  it("marks the steps before the current one done, with a check", () => {
    const { steps } = shown({ kind: "asking", step: "allow" });
    expect(steps.map((s) => s.state)).toEqual(["done", "done", "done", "current"]);
    expect(steps.map((s) => s.icon)).toEqual(["check", "check", "check", "headset"]);
  });

  it("opens the asking step with the prompt, and says to choose Always allow", () => {
    const help = shown({ kind: "asking", step: "allow" }).steps[3].help;
    expect(help?.prompt).toBe(true);
    expect(help?.items.join(" ")).toContain("Always allow from this computer");
  });

  it("opens only the current step", () => {
    expect(shown({ kind: "asking", step: "connect" }).steps.filter((s) => s.help)).toHaveLength(1);
  });

  it("keeps a failed step current, with how to fix it", () => {
    const { steps } = shown({ kind: "failed", step: "connect", reason: { kind: "busy" } });
    expect(steps.map((s) => s.state)).toEqual(["current", "todo", "todo", "todo"]);
    expect(steps[0].note).toBe("Stopped");
    expect(steps[0].help?.items.join(" ")).toContain("adb kill-server");
  });

  it("shows the install's progress beside its step", () => {
    expect(shown({ kind: "working", step: "install", progress: 0.42 }).steps[1].note).toBe("42%");
  });

  it("marks every step done when setup finished", () => {
    expect(shown({ kind: "done", model: "Quest 3" }).steps.every((s) => s.state === "done")).toBe(true);
  });
});

describe("section", () => {
  it("lists what to have ready before connecting", () => {
    expect(shown({ kind: "idle" }).section?.title).toBe("Before you start");
  });

  it("asks for Chrome or Edge first without WebUSB", () => {
    expect(shown({ kind: "idle" }, false).section?.rows[0].title).toBe("Chrome or Edge on a computer");
  });

  it("says what to do next once set up, leading to the desktop app", () => {
    const next = shown({ kind: "done", model: "Quest 3" }).section;
    expect(next?.rows.map((r) => r.title)).toContain("Tap Start");
    expect(next?.rows.some((r) => r.href)).toBe(true);
  });

  it("keeps out of the way while setup runs", () => {
    expect(shown({ kind: "working", step: "install", progress: 0.1 }).section).toBeUndefined();
  });
});
