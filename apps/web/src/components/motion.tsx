import { AnimatePresence, motion, type Transition } from "motion/react";
import type { ReactNode } from "react";

// The page's motion: one easing, a quick fade or turn, and a slightly longer open and close.

/** Starts fast and settles, as the apps' panels do. */
export const EASE = [0.2, 0, 0, 1] as const;
/** A fade, a swap of text, a chevron turning. */
export const QUICK: Transition = { duration: 0.18, ease: EASE };
/** Something opening or closing. */
export const EXPAND: Transition = { duration: 0.22, ease: EASE };

/** Opens [children] to their height while [open], and closes them away. */
export function Collapse({ open, children }: { open: boolean; children: ReactNode }) {
  return (
    <AnimatePresence initial={false}>
      {open && (
        <motion.div
          key="collapse"
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          transition={EXPAND}
          className="overflow-hidden"
        >
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  );
}
