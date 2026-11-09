import { motion } from "motion/react";
import { ALWAYS_ALLOW_LABEL } from "../card";
import { Icon } from "../icons";
import { EASE } from "./motion";

/**
 * Horizon OS's "Allow USB debugging?" panel, drawn to be recognised when it appears: its wording,
 * and the three stacked buttons it has where Android has a checkbox. The panel is 480 wide with 40
 * padding, a 60 icon and 55-high buttons 10 apart; the drawing is that at about 0.6, snapped to
 * the 4 grid, in the headset's greys (the `horizon` tokens) rather than Setu's colours. The
 * button to choose is ringed.
 */
export function HeadsetPrompt() {
  return (
    <figure
      aria-hidden="true"
      className="flex w-full max-w-72 flex-col items-center rounded-horizon bg-horizon-panel p-6 text-center shadow-horizon"
    >
      <span className="grid size-9 place-items-center rounded-full bg-horizon-control text-horizon-text">
        <Icon name="plug" className="size-5" />
      </span>
      <p className="mt-3 text-horizon-title font-semibold text-horizon-text">Allow USB debugging?</p>
      <p className="mt-2 text-horizon-body text-horizon-muted">
        Your device is not secure in developer mode and may be vulnerable to malicious software installed using ADB.
        The computer's RSA key fingerprint is: 4F:C2:19:8A:…
      </p>
      <div className="mt-4 flex w-full flex-col gap-2 text-horizon-body font-medium">
        <span className="grid h-8 place-items-center rounded-full bg-horizon-text text-horizon-on-primary">Allow</span>
        <span className="grid h-8 place-items-center rounded-full bg-horizon-control text-horizon-text">Cancel</span>
        <span className="relative grid h-8 place-items-center rounded-full bg-horizon-control text-horizon-text">
          {ALWAYS_ALLOW_LABEL}
          <motion.span
            className="absolute -inset-1 rounded-full ring-2 ring-cream"
            initial={{ opacity: 0, scale: 1.08 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ duration: 0.4, delay: 0.35, ease: EASE }}
          />
        </span>
      </div>
    </figure>
  );
}
