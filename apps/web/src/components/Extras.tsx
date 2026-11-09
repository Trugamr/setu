import { motion } from "motion/react";
import { useState } from "react";
import type { Section } from "../card";
import { Icon } from "../icons";
import { Collapse, QUICK } from "./motion";
import { Group, RowLine, SectionLabel } from "./Rows";

/** What to have ready before setup, or what to do once it finished. */
export function SectionRows({ section }: { section: Section }) {
  return (
    <motion.div
      key={section.title}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={QUICK}
    >
      <SectionLabel>{section.title}</SectionLabel>
      <Group>
        {section.rows.map((row) => (
          <li key={row.title}>
            {row.href ? (
              <a href={row.href} className="inset-focus block no-underline transition-colors hover:bg-ink-hover">
                <RowLine {...row} end={<Icon name="chevron" className="size-5 flex-none text-cream-muted" />} />
              </a>
            ) : (
              <RowLine {...row} />
            )}
          </li>
        ))}
      </Group>
    </motion.div>
  );
}

/** Why setup asks for what it does, for anyone who wants to know before they plug in. */
export function HowItWorks() {
  const [open, setOpen] = useState(false);
  return (
    <div className="mt-6 overflow-hidden rounded-group bg-ink-raised">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className={`inset-focus flex min-h-13 w-full cursor-pointer items-center justify-between px-4 text-left text-row transition-colors hover:bg-ink-hover ${open ? "rounded-t-group" : "rounded-group"}`}
      >
        How setup works
        <motion.span animate={{ rotate: open ? 90 : 0 }} transition={QUICK} className="text-cream-muted">
          <Icon name="chevron" />
        </motion.span>
      </button>
      <Collapse open={open}>
        <div className="flex flex-col gap-3 border-t border-ink-high px-4 pt-3 pb-4 text-sentence text-pretty text-cream-muted">
          <p>
            Setu captures your Quest's sound with a small helper that only adb, Android's debugging tool, can
            start. Setup lets the Setu app start that helper itself, over the headset's own wireless debugging,
            so you need the cable only this once.
          </p>
          <p>
            That's why the headset asks twice: first to let this browser install Setu, then to let Setu use
            debugging. Nothing is installed on this computer.
          </p>
          <p>After the headset restarts, it may ask once to allow wireless debugging on your network. Choose Allow.</p>
        </div>
      </Collapse>
    </div>
  );
}
