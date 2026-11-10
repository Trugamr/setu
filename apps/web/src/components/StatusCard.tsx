import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import type { Card, Grille as GrilleState } from "../page";
import { QUICK } from "./motion";

const COLUMNS = 5;
const ROWS = 3;
const DOTS = COLUMNS * ROWS;
/** One sweep of the busy grille, as on the agent. */
const SWEEP_MS = 1600;

/** The apps' status card: a title and a sentence, the grille, and one action. */
export function StatusCard({ card, onAction }: { card: Card; onAction?: () => void }) {
  const lit = useLit(card.grille);
  return (
    <section
      aria-live="polite"
      className={`flex flex-col rounded-card bg-ink-raised p-4 outline-2 -outline-offset-2 transition-colors duration-200 ${
        card.look === "waiting" ? "outline-cream" : "outline-transparent"
      }`}
    >
      <div className="flex items-start justify-between gap-4">
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={card.title}
            className="flex min-w-0 flex-col gap-1"
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={QUICK}
          >
            <h2 className="font-display text-status-title">{card.title}</h2>
            <p className={`text-sentence text-pretty ${card.look === "problem" ? "text-signal" : "text-cream-muted"}`}>
              {card.sentence}
            </p>
          </motion.div>
        </AnimatePresence>
        <Grille lit={lit} />
      </div>
      {card.action && onAction && (
        <button
          type="button"
          onClick={onAction}
          className={`mt-4 h-10 w-full cursor-pointer rounded-control text-row font-semibold transition-colors ${
            card.action.style === "secondary"
              ? "bg-transparent text-cream inset-ring inset-ring-edge hover:bg-ink-hover"
              : "bg-signal text-ink hover:bg-signal-hover active:bg-signal-pressed"
          }`}
        >
          {card.action.label}
        </button>
      )}
    </section>
  );
}

/** How many dots the grille lights, sweeping them in turn when it sweeps. */
function useLit(grille: GrilleState): number {
  const reduced = useReducedMotion();
  const [sweep, setSweep] = useState(0);
  const sweeping = grille === "sweep" && !reduced;
  useEffect(() => {
    if (!sweeping) return;
    let frame = requestAnimationFrame(function tick(now) {
      setSweep(Math.floor(((now % SWEEP_MS) / SWEEP_MS) * (DOTS + 1)));
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [sweeping]);

  if (grille === "full") return DOTS;
  if (typeof grille === "number") return Math.round(grille * DOTS);
  return sweeping ? sweep : 0;
}

/** The grille from the icon, five dots by three, filled column by column from the bottom left. */
function Grille({ lit }: { lit: number }) {
  // Column by column, bottom to top, the order a level meter fills.
  const order = (col: number, row: number) => col * ROWS + (ROWS - 1 - row);
  return (
    <div className="grid flex-none grid-flow-col grid-rows-3 gap-1" aria-hidden="true">
      {Array.from({ length: COLUMNS }, (_, col) =>
        Array.from({ length: ROWS }, (_, row) => (
          <span
            key={`${col}-${row}`}
            className={`size-2 rounded-full transition-colors duration-150 ${order(col, row) < lit ? "bg-cream" : "bg-unlit"}`}
          />
        )),
      )}
    </div>
  );
}
