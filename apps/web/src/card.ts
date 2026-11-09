// What the installer's page shows, worked out from a Status without the DOM, so the wording and
// every choice of what to show can be tested; components/ only lays it out. The card and rows
// follow the apps' (design/brand/README.md).

import { STEPS, type Status, type Step } from "./setup";

/** Plain: nothing to do on the headset. Waiting: the headset is asking the user something. */
export type Look = "plain" | "waiting";

/**
 * The status card's grille. Off; sweeping while something runs with nothing to measure; full
 * while the headset asks and once set up; or lit to a fraction from 0 to 1, such as how much of
 * the APK has been sent.
 */
export type Grille = "off" | "sweep" | "full" | number;

export interface Card {
  look: Look;
  title: string;
  sentence: string;
  /** Whether the sentence says what went wrong. */
  problem: boolean;
  grille: Grille;
  /** The button, or none. Primary is Signal, the action that starts things; secondary an outline. */
  action?: { label: string; style: "primary" | "secondary" };
}

export const SOURCE_URL = "https://github.com/Trugamr/setu";
/** The headset's button that remembers a computer's key, as the headset labels it. */
export const ALWAYS_ALLOW_LABEL = "Always allow from this computer";
const ALWAYS_ALLOW = `“${ALWAYS_ALLOW_LABEL}”`;

export function card(status: Status, supported: boolean): Card {
  const plain = { look: "plain" as const, problem: false, grille: "off" as const };
  if (!supported) {
    return {
      ...plain,
      title: "Use Chrome or Edge",
      sentence: "Setup reaches the headset over USB, which only Chrome and Edge on a computer can do.",
    };
  }
  switch (status.kind) {
    case "idle":
      return {
        ...plain,
        title: "Set up your Quest",
        sentence: "Connect your Quest to this computer with a USB cable, then choose Connect. It takes about a minute.",
        action: { label: "Connect", style: "primary" },
      };
    case "working":
      return {
        ...plain,
        title: { connect: "Connecting", install: "Installing", setup: "Setting up", allow: "Setting up" }[status.step],
        sentence: {
          connect: "Choose your Quest in the browser's list.",
          install: "Sending Setu to the headset. Keep the cable plugged in.",
          setup: "Giving Setu permission…",
          allow: "Opening adb's network port…",
        }[status.step],
        grille: status.progress ?? "sweep",
      };
    case "asking":
      return {
        ...plain,
        look: "waiting",
        grille: "full",
        title: "Allow on the headset",
        sentence: `Put on the headset and choose ${ALWAYS_ALLOW}.`,
      };
    case "done":
      return {
        ...plain,
        title: "Ready",
        grille: "full",
        sentence: `Setu is set up on the ${status.model || "headset"}. You can unplug the cable.`,
        action: { label: "Set up another headset", style: "secondary" },
      };
    case "failed": {
      const { reason, step } = status;
      return {
        ...plain,
        title: { connect: "Couldn't connect", install: "Couldn't install", setup: "Couldn't set up", allow: "Not allowed" }[step],
        sentence:
          reason.kind === "busy"
            ? "Another program is using the headset's USB connection."
            : reason.kind === "notAllowed"
              ? "The headset didn't allow Setu to start without the cable."
              : sentenceCase(reason.message),
        problem: true,
        action: { label: "Try again", style: "primary" },
      };
    }
  }
}

/** The pictures in the rows' icon tiles. */
export type Icon = "plug" | "download" | "key" | "check" | "code" | "cable" | "apps" | "headset" | "power" | "monitor";

export type StepState = "done" | "current" | "todo";

/** What opens under the current step: what to do now, and the headset's prompt when it asks. */
export interface Help {
  items: string[];
  /** Whether the items are steps to take in order. */
  ordered: boolean;
  /** Draws the headset's "Allow USB debugging?" panel, with the button to choose. */
  prompt: boolean;
}

export interface StepRow {
  step: Step;
  icon: Icon;
  title: string;
  /** Why the step is there. */
  detail: string;
  state: StepState;
  /** Where it stands, at the row's end. */
  note: string;
  help?: Help;
}

const STEP_ROWS: Record<Step, { icon: Icon; title: string; detail: string }> = {
  connect: { icon: "plug", title: "Connect the headset", detail: "This browser reaches it over USB." },
  install: { icon: "download", title: "Install Setu", detail: "Sends the Setu app to the headset." },
  setup: { icon: "key", title: "Give Setu permission", detail: "So it can turn on wireless debugging itself." },
  allow: { icon: "headset", title: "Allow Setu on the headset", detail: "So it can start without the cable." },
};

const ONLY_ONCE = "Allow lets Setu in only once, and it couldn't start later.";
const TRY_AGAIN = "Choose Try again.";

const ON_THE_HEADSET = (why: string): Help => ({
  items: ["Put on the headset.", `Choose ${ALWAYS_ALLOW}, the last button. ${why}`],
  ordered: true,
  prompt: true,
});

/** Where each step stands. A failed step stays current, so the list shows where it stopped. */
export function steps(status: Status): StepRow[] {
  const at = status.kind === "done" ? STEPS.length : status.kind === "idle" ? -1 : STEPS.indexOf(status.step);
  return STEPS.map((step, i) => {
    const state: StepState = i < at ? "done" : i === at ? "current" : "todo";
    const row = STEP_ROWS[step];
    return {
      step,
      ...row,
      icon: state === "done" ? "check" : row.icon,
      state,
      note: note(status, state),
      help: state === "current" ? help(status) : undefined,
    };
  });
}

function note(status: Status, state: StepState): string {
  if (state === "done") return "Done";
  if (state === "todo") return "";
  if (status.kind === "failed") return "Stopped";
  if (status.kind === "asking") return "On the headset";
  if (status.kind === "working" && status.progress !== undefined) return `${Math.round(status.progress * 100)}%`;
  return "Now";
}

function help(status: Status): Help | undefined {
  switch (status.kind) {
    case "working":
      return status.step === "connect"
        ? {
            items: [
              "Quest not in the list? Wake the headset, check that Developer Mode is on, and try another cable: many only charge.",
            ],
            ordered: false,
            prompt: false,
          }
        : undefined;
    case "asking":
      // Before the allow step, the headset asks about this browser's key; then, the agent's.
      return ON_THE_HEADSET(status.step === "allow" ? ONLY_ONCE : "Then this browser can reconnect without asking.");
    case "failed":
      if (status.reason.kind === "busy") {
        return {
          items: [
            "Close SideQuest, Meta Quest Developer Hub and Meta Quest Link.",
            "If you use adb, run “adb kill-server”.",
            TRY_AGAIN,
          ],
          ordered: true,
          prompt: false,
        };
      }
      if (status.reason.kind === "notAllowed") {
        return ON_THE_HEADSET(ONLY_ONCE);
      }
      return {
        items: ["Unplug the headset and plug it back in, or try another cable.", TRY_AGAIN],
        ordered: true,
        prompt: false,
      };
    default:
      return undefined;
  }
}

export interface Row {
  icon: Icon;
  title: string;
  detail: string;
  /** Where the row leads, shown with a chevron. */
  href?: string;
}

/** The group under the steps: what to have ready, or what to do once set up. */
export interface Section {
  title: string;
  rows: Row[];
}

export function section(status: Status, supported: boolean): Section | undefined {
  if (!supported || status.kind === "idle") {
    return {
      title: "Before you start",
      rows: [
        ...(supported
          ? []
          : [{ icon: "monitor" as const, title: "Chrome or Edge on a computer", detail: "Windows, macOS or Linux." }]),
        { icon: "code", title: "Turn on Developer Mode", detail: "In the Meta Horizon app on your phone, in the headset's settings." },
        { icon: "cable", title: "Use a data cable", detail: "A USB-C cable that carries data, not only charge." },
        {
          icon: "apps",
          title: "Close other headset tools",
          detail: "SideQuest and Meta Quest Link, and “adb kill-server” if you use adb. Only one program can use the USB connection.",
        },
        { icon: "headset", title: "Keep the headset nearby", detail: "It asks you to allow things twice." },
      ],
    };
  }
  if (status.kind === "done") {
    return {
      title: "Next",
      rows: [
        { icon: "headset", title: "Open Setu on the headset", detail: "It's in your Library, under Unknown sources." },
        { icon: "power", title: "Tap Start", detail: "The headset starts sending its sound." },
        { icon: "monitor", title: "Open Setu on your PC", detail: "To hear the headset there.", href: SOURCE_URL },
      ],
    };
  }
  return undefined;
}

function sentenceCase(message: string): string {
  const text = message.trim() || "Something went wrong.";
  const capital = text[0].toUpperCase() + text.slice(1);
  return /[.!?]$/.test(capital) ? capital : `${capital}.`;
}
