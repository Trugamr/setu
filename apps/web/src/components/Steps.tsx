import type { Help, StepRow } from "../page";
import { HeadsetPrompt } from "./HeadsetPrompt";
import { Collapse } from "./motion";
import { Group, PAST_TILE, RowLine, SectionLabel } from "./Rows";

/** The four steps, each saying why it is there. The current one opens to say what to do now. */
export function Steps({ rows }: { rows: StepRow[] }) {
  return (
    <>
      <SectionLabel>Steps</SectionLabel>
      <Group as="ol">
        {rows.map((row) => (
          <li key={row.step} aria-current={row.state === "current" ? "step" : undefined}>
            <RowLine
              icon={row.icon}
              tile={row.state}
              title={row.title}
              detail={row.detail}
              muted={row.state === "todo"}
              end={
                <span className={`flex-none text-label text-cream-muted tabular-nums ${row.help ? "max-sm:hidden" : ""}`}>
                  {row.note}
                </span>
              }
            />
            <Collapse open={row.help !== undefined}>{row.help && <HelpPanel help={row.help} />}</Collapse>
          </li>
        ))}
      </Group>
    </>
  );
}

/**
 * Under the current step: what to do, lined up with the step's text, then what the headset shows
 * when it asks, centred on a stage of the page's colour, as if seen through the headset.
 */
function HelpPanel({ help }: { help: Help }) {
  const List = help.ordered ? "ol" : "ul";
  return (
    <div className="flex flex-col gap-4 px-4 pb-4">
      <List className={`flex flex-col gap-2 ${PAST_TILE} text-sentence text-pretty`}>
        {help.items.map((item, i) => (
          <li key={item} className="flex gap-3">
            {help.ordered ? (
              <span className="grid size-5 flex-none place-items-center rounded-full bg-ink-high text-caption font-semibold text-cream tabular-nums">
                {i + 1}
              </span>
            ) : (
              <span className="mt-2 size-1 flex-none rounded-full bg-cream-muted" />
            )}
            <span>{item}</span>
          </li>
        ))}
      </List>
      {help.prompt && (
        <div className="flex justify-center rounded-control bg-ink px-4 py-6 sm:py-8">
          <HeadsetPrompt />
        </div>
      )}
    </div>
  );
}
