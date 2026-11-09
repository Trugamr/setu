// Lays out the installer's page from card.ts and runs setup.ts over WebUSB.

import mark from "../../../design/brand/setu-mark-dark.svg";
import { card, steps, type Card } from "./card";
import { runSetup, type Status } from "./setup";
import { fetchApk, webUsb } from "./webusb";

const COLUMNS = 5;
const ROWS = 3;
const DOTS = COLUMNS * ROWS;
/** One sweep of the busy grille, as on the agent. */
const SWEEP_MS = 1600;

const usb = webUsb();
const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");

const el = {
  card: byId("card"),
  title: byId("card-title"),
  sentence: byId("card-sentence"),
  action: byId("card-action") as HTMLButtonElement,
  grille: byId("grille"),
  steps: byId("steps"),
};

(byId("mark") as HTMLImageElement).src = mark;

// Column by column, bottom to top, the order a level meter fills.
const dots = Array.from({ length: DOTS }, () => document.createElement("span"));
for (let col = 0; col < COLUMNS; col++) {
  for (let row = 0; row < ROWS; row++) el.grille.append(dots[col * ROWS + (ROWS - 1 - row)]);
}

let status: Status = { kind: "idle" };
let running = false;
let shown: Card;

el.action.addEventListener("click", async () => {
  if (!usb || running) return;
  running = true;
  try {
    await runSetup(usb, fetchApk, render);
  } finally {
    running = false;
    render(status);
  }
});

function render(next: Status) {
  status = next;
  shown = card(status, usb !== undefined);
  el.card.dataset.look = shown.look;
  el.title.textContent = shown.title;
  el.sentence.textContent = shown.sentence;
  el.sentence.toggleAttribute("data-problem", shown.problem);
  el.action.hidden = !shown.action || running;
  el.action.textContent = shown.action ?? "";
  el.action.toggleAttribute("data-quiet", status.kind === "done");

  el.steps.replaceChildren(
    ...steps(status).map(({ name, state }) => {
      const li = document.createElement("li");
      li.dataset.state = state;
      const dot = document.createElement("span");
      dot.className = "dot";
      const label = document.createElement("span");
      label.textContent = name;
      const note = document.createElement("span");
      note.className = "status";
      note.textContent = state === "done" ? "Done" : state === "current" ? currentNote() : "";
      li.append(dot, label, note);
      if (state === "current") li.setAttribute("aria-current", "step");
      return li;
    }),
  );
  paintGrille(performance.now());
}

function currentNote(): string {
  if (status.kind === "failed") return "Stopped";
  if (status.kind === "working" && status.progress !== undefined) return `${Math.round(status.progress * 100)}%`;
  if (status.kind === "asking") return "On the headset";
  return "Now";
}

/** Lights the grille: the install's progress, a sweep while busy, all lit while the headset asks. */
function paintGrille(now: number) {
  let lit = 0;
  if (shown.look === "waiting") lit = DOTS;
  else if (shown.progress !== undefined) lit = Math.round(shown.progress * DOTS);
  else if (shown.busy && !reducedMotion.matches) lit = Math.floor(((now % SWEEP_MS) / SWEEP_MS) * (DOTS + 1));
  else if (status.kind === "done") lit = DOTS;
  dots.forEach((dot, i) => dot.toggleAttribute("data-lit", i < lit));
}

function frame(now: number) {
  if (shown.busy) paintGrille(now);
  requestAnimationFrame(frame);
}

function byId(id: string): HTMLElement {
  const found = document.getElementById(id);
  if (!found) throw new Error(`#${id} is missing`);
  return found;
}

// In development, `?status={"kind":"asking","step":"allow"}` shows any state without a headset.
const preview = import.meta.env.DEV && new URLSearchParams(location.search).get("status");
render(preview ? (JSON.parse(preview) as Status) : status);
requestAnimationFrame(frame);
