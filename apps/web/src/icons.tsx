import {
  AppWindow,
  ArrowDownToLine,
  Cable,
  Check,
  ChevronRight,
  CodeXml,
  type LucideIcon,
  MonitorSpeaker,
  Plug,
  Power,
  RectangleGoggles,
  ShieldCheck,
} from "lucide-react";
import type { Icon as IconName } from "./page";

/** Lucide's line icons, at the weight of the apps' drawn ones. */
const ICONS: Record<IconName | "chevron", LucideIcon> = {
  plug: Plug,
  download: ArrowDownToLine,
  key: ShieldCheck,
  check: Check,
  code: CodeXml,
  cable: Cable,
  apps: AppWindow,
  headset: RectangleGoggles,
  power: Power,
  monitor: MonitorSpeaker,
  chevron: ChevronRight,
};

export function Icon({ name, className = "size-5" }: { name: IconName | "chevron"; className?: string }) {
  const Drawn = ICONS[name];
  return <Drawn className={className} strokeWidth={1.75} aria-hidden="true" />;
}
