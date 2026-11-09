import type { ReactNode } from "react";
import type { Icon as IconName } from "../card";
import { Icon } from "../icons";

/** A list's label, inset to line up with the text in its rows, as in the apps. */
export function SectionLabel({ children }: { children: ReactNode }) {
  return <h3 className="mt-6 mb-2 ml-4 text-label text-cream-muted">{children}</h3>;
}

/** Rows on one raised surface, divided by lines, as the apps group them. */
export function Group({ children, as: Tag = "ul" }: { children: ReactNode; as?: "ul" | "ol" }) {
  return (
    <Tag className="list-none divide-y divide-ink-high overflow-hidden rounded-group bg-ink-raised">
      {children}
    </Tag>
  );
}

export type TileLook = "plain" | "done" | "current" | "todo";

/** An icon tile, a title over a secondary line, and something at the end. */
export function RowLine({
  icon,
  title,
  detail,
  end,
  tile = "plain",
  muted = false,
}: {
  icon: IconName;
  title: string;
  detail: string;
  end?: ReactNode;
  tile?: TileLook;
  muted?: boolean;
}) {
  return (
    <div className="flex min-h-16 items-center gap-3 px-4 py-3">
      <Tile icon={icon} look={tile} />
      <span className="flex min-w-0 flex-1 flex-col gap-1">
        <span className={`text-row ${muted ? "text-cream-muted" : "text-cream"}`}>{title}</span>
        <span className="text-label text-pretty text-cream-muted">{detail}</span>
      </span>
      {end}
    </div>
  );
}

function Tile({ icon, look }: { icon: IconName; look: TileLook }) {
  const looks: Record<TileLook, string> = {
    plain: "bg-ink-high text-cream",
    done: "bg-cream text-ink",
    current: "bg-ink-high text-cream inset-ring-2 inset-ring-cream",
    todo: "bg-ink-high text-cream-muted",
  };
  return (
    <span className={`grid size-10 flex-none place-items-center rounded-control transition-colors duration-200 ${looks[look]}`}>
      <Icon name={icon} />
    </span>
  );
}
