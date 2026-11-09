import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// The page is styled only with the tokens in style.css and Tailwind's 4px steps. An arbitrary
// value such as `text-[11px]`, `bg-[#3b3c42]` or `rounded-[12px]` would bypass them, so a value
// the page needs becomes a token there instead.

const SRC = new URL(".", import.meta.url).pathname;

function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sources(path);
    return entry.name.endsWith(".tsx") ? [path] : [];
  });
}

/** Utility classes with a bracketed value: `p-[3px]`, `-outline-offset-[1.5px]`, `[&>li]:x`. */
const ARBITRARY = /(?:^|[\s"'`])-?[a-z][\w-]*-\[[^\]]+\]|(?:^|[\s"'`])\[[^\]]+\]:/g;

describe("design tokens", () => {
  it("styles the page without arbitrary values", () => {
    const found = sources(SRC).flatMap((file) =>
      [...readFileSync(file, "utf8").matchAll(ARBITRARY)].map((m) => `${file.slice(SRC.length)}: ${m[0].trim()}`),
    );
    expect(found).toEqual([]);
  });
});
