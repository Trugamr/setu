// What the installer's page shows, worked out from a Status without the DOM, so the wording can
// be tested. main.ts only lays it out. The card follows the apps' status card (design/brand/README.md).

import { STEPS, type Status, type Step } from "./setup";

/** Plain: nothing to do on the headset. Waiting: the headset is asking the user something. */
export type Look = "plain" | "waiting";

export interface Card {
  look: Look;
  title: string;
  sentence: string;
  /** Whether the sentence says what went wrong. */
  problem: boolean;
  /** Whether something is in progress with nothing to show for it: the grille sweeps. */
  busy: boolean;
  /** How much of the APK has been sent, from 0 to 1, lighting the grille. */
  progress?: number;
  /** The button's label, or none. */
  action?: string;
}

export const STEP_NAMES: Record<Step, string> = {
  connect: "Connect the headset",
  install: "Install Setu",
  setup: "Let Setu turn on wireless debugging",
  allow: "Allow Setu on the headset",
};

export function card(status: Status, supported: boolean): Card {
  if (!supported) {
    return {
      look: "plain",
      title: "Use Chrome or Edge",
      sentence: "Setup reaches the headset over USB, which only Chrome and Edge on a computer can do.",
      problem: false,
      busy: false,
    };
  }
  switch (status.kind) {
    case "idle":
      return {
        look: "plain",
        title: "Set up your headset",
        sentence: "Connect your Quest to this computer with a USB cable, then choose Connect.",
        problem: false,
        busy: false,
        action: "Connect",
      };
    case "working":
      return {
        look: "plain",
        title: { connect: "Connecting", install: "Installing", setup: "Setting up", allow: "Setting up" }[status.step],
        sentence: {
          connect: "Choose your headset in the browser's list.",
          install: "Sending Setu to the headset…",
          setup: "Letting Setu turn on wireless debugging…",
          allow: "Opening adb's network port…",
        }[status.step],
        problem: false,
        busy: status.progress === undefined,
        progress: status.progress,
      };
    case "asking":
      return {
        look: "waiting",
        title: "Allow on the headset",
        sentence:
          status.step === "connect"
            ? "Put on the headset and choose Allow, so this browser can install Setu."
            : "Put on the headset, tick “Always allow from this computer”, then choose Allow. " +
              "This lets Setu start without the cable.",
        problem: false,
        busy: false,
      };
    case "done":
      return {
        look: "plain",
        title: "Ready",
        sentence: `Unplug the cable. On the ${status.model || "headset"}, open Setu and tap Start.`,
        problem: false,
        busy: false,
        action: "Set up another headset",
      };
    case "failed": {
      const { reason, step } = status;
      return {
        look: "plain",
        title: {
          connect: "Couldn't connect",
          install: "Couldn't install",
          setup: "Couldn't set up",
          allow: "Not allowed",
        }[step],
        sentence:
          reason.kind === "busy"
            ? "Another program is using the headset's USB connection. Close SideQuest and Meta Quest Link, " +
              "and run “adb kill-server” if you use adb, then try again."
            : reason.kind === "notAllowed"
              ? "The headset didn't allow Setu in time. Try again, and choose Allow when the headset asks."
              : sentenceCase(reason.message),
        problem: true,
        busy: false,
        action: "Try again",
      };
    }
  }
}

export type StepState = "done" | "current" | "todo";

/** Where each step stands. A failed step stays current, so the list shows where it stopped. */
export function steps(status: Status): { step: Step; name: string; state: StepState }[] {
  const at =
    status.kind === "done" ? STEPS.length : status.kind === "idle" ? -1 : STEPS.indexOf(status.step);
  return STEPS.map((step, i) => ({
    step,
    name: STEP_NAMES[step],
    state: i < at ? "done" : i === at ? "current" : "todo",
  }));
}

function sentenceCase(message: string): string {
  const text = message.trim() || "Something went wrong.";
  const capital = text[0].toUpperCase() + text.slice(1);
  return /[.!?]$/.test(capital) ? capital : `${capital}.`;
}
