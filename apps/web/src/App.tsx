import { AnimatePresence, MotionConfig } from "motion/react";
import { useState } from "react";
import { SOURCE_URL, card, section, steps } from "./card";
import { HowItWorks, SectionRows } from "./components/Extras";
import { StatusCard } from "./components/StatusCard";
import { Steps } from "./components/Steps";
import { runSetup, type Status } from "./setup";
import { fetchApk, webUsb } from "./webusb";

const usb = webUsb();
const supported = usb !== undefined;

// In development, `?status={"kind":"asking","step":"allow"}` shows any state without a headset.
const preview = import.meta.env.DEV ? new URLSearchParams(location.search).get("status") : null;

export function App() {
  const [status, setStatus] = useState<Status>(preview ? (JSON.parse(preview) as Status) : { kind: "idle" });
  const [running, setRunning] = useState(false);

  // Straight from the click: the browser shows its device list only during one.
  async function start() {
    if (!usb || running) return;
    setRunning(true);
    try {
      await runSetup(usb, fetchApk, setStatus);
    } finally {
      setRunning(false);
    }
  }

  const shown = card(status, supported);
  const extra = section(status, supported);
  return (
    <MotionConfig reducedMotion="user">
      <main className="mx-auto w-full max-w-column px-4 pt-10 pb-12">
        <header className="mb-5 flex items-baseline justify-between px-4">
          <h1 className="font-display text-app-title">Setu</h1>
          <p className="text-label text-cream-muted">Headset setup</p>
        </header>

        <StatusCard card={shown} onAction={running ? undefined : start} />
        <Steps rows={steps(status)} />
        <AnimatePresence mode="wait" initial={false}>
          {extra && <SectionRows key={extra.title} section={extra} />}
        </AnimatePresence>
        <HowItWorks />

        <footer className="mt-8 px-4 text-label text-cream-muted">
          Setu sends your Quest's audio to your PC. <a href={SOURCE_URL}>Source on GitHub</a>
        </footer>
      </main>
    </MotionConfig>
  );
}
