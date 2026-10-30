// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { contrast, over, parseColor, resolvedTokens, type Theme } from "./parse";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");
const themes: Theme[] = ["dark", "light"];

function ratio(theme: Theme, fg: string, bg: string): number {
  const t = resolvedTokens(css, theme);
  const f = t[fg];
  const b = t[bg];
  if (f === undefined || b === undefined) throw new Error(`missing token ${fg} or ${bg}`);
  return contrast(parseColor(f), parseColor(b));
}

describe.each(themes)("contrast (%s)", (theme) => {
  describe("text", () => {
    for (const bg of ["bg", "surface"]) {
      for (const fg of ["fg", "fg-muted"]) {
        it(`${fg} on ${bg} >= 4.5`, () => {
          expect(ratio(theme, fg, bg)).toBeGreaterThanOrEqual(4.5);
        });
      }
      // fg-subtle is for placeholders, disabled hints and tertiary labels only.
      it(`fg-subtle on ${bg} >= 3`, () => {
        expect(ratio(theme, "fg-subtle", bg)).toBeGreaterThanOrEqual(3);
      });
    }
    it("accent-fg on accent >= 4.5", () => {
      expect(ratio(theme, "accent-fg", "accent")).toBeGreaterThanOrEqual(4.5);
    });
    it("accent-fg on accent-hover >= 4.5", () => {
      expect(ratio(theme, "accent-fg", "accent-hover")).toBeGreaterThanOrEqual(4.5);
    });
    it("danger-fg on danger >= 4.5", () => {
      expect(ratio(theme, "danger-fg", "danger")).toBeGreaterThanOrEqual(4.5);
    });
    it("cue-fg on every cue color >= 4.5", () => {
      for (let i = 1; i <= 8; i++) {
        expect(ratio(theme, "cue-fg", `cue-${i}`)).toBeGreaterThanOrEqual(4.5);
      }
    });
  });

  describe("essential UI", () => {
    for (const bg of ["bg", "surface", "surface-raised"]) {
      it(`focus-ring visible on ${bg} >= 3`, () => {
        expect(ratio(theme, "focus-ring", bg)).toBeGreaterThanOrEqual(3);
      });
    }
    it("accent on bg >= 3", () => {
      expect(ratio(theme, "accent", "bg")).toBeGreaterThanOrEqual(3);
    });
    for (const deck of ["a", "b", "c", "d"]) {
      it(`deck-${deck} on bg >= 3`, () => {
        expect(ratio(theme, `deck-${deck}`, "bg")).toBeGreaterThanOrEqual(3);
      });
    }
    for (let i = 1; i <= 8; i++) {
      it(`cue-${i} on bg >= 3`, () => {
        expect(ratio(theme, `cue-${i}`, "bg")).toBeGreaterThanOrEqual(3);
      });
    }
    for (const m of ["green", "amber", "red", "clip"]) {
      it(`meter-${m} on bg >= 3`, () => {
        expect(ratio(theme, `meter-${m}`, "bg")).toBeGreaterThanOrEqual(3);
      });
    }
    // Callout, LED and status colours (icons, borders, dots).
    for (const c of ["info", "success", "warning", "danger", "sync-on", "master-on"]) {
      for (const bg of ["bg", "surface"]) {
        it(`${c} on ${bg} >= 3`, () => {
          expect(ratio(theme, c, bg)).toBeGreaterThanOrEqual(3);
        });
      }
    }
    // Callout titles use the status colour as text.
    for (const c of ["info", "success", "warning", "danger"]) {
      it(`${c} text on surface >= 4.5`, () => {
        expect(ratio(theme, c, "surface")).toBeGreaterThanOrEqual(4.5);
      });
    }
    it("border-strong (off LED ring) on surface >= 1.4 (off state, not essential)", () => {
      expect(ratio(theme, "border-strong", "surface")).toBeGreaterThanOrEqual(1.4);
    });
    for (const w of ["low", "mid", "high"]) {
      it(`wave-${w} on bg >= 3`, () => {
        expect(ratio(theme, `wave-${w}`, "bg")).toBeGreaterThanOrEqual(3);
      });
    }
    it("playhead on bg >= 3", () => {
      expect(ratio(theme, "playhead", "bg")).toBeGreaterThanOrEqual(3);
    });
    it("beat-grid-downbeat on bg >= 1.5 (decorative guide)", () => {
      expect(ratio(theme, "beat-grid-downbeat", "bg")).toBeGreaterThanOrEqual(1.5);
    });
  });

  describe("backdrop mesh", () => {
    const cases = [
      ["subtle", "surface"],
      ["page", "bg"],
    ] as const;
    for (const [strength, base] of cases) {
      for (const end of ["red", "blue"]) {
        for (const fg of ["fg", "fg-muted"]) {
          it(`${fg} on ${base} under backdrop-${end} (${strength}) >= 4.5`, () => {
            const t = resolvedTokens(css, theme);
            const value = (name: string): string => {
              const v = t[name];
              if (v === undefined) throw new Error(`missing token ${name}`);
              return v;
            };
            const wave = parseColor(value(`backdrop-${end}`));
            const alpha = Number(value(`backdrop-alpha-${strength}`));
            const stacked = over({ ...wave, a: alpha }, parseColor(value(base)));
            expect(contrast(parseColor(value(fg)), stacked)).toBeGreaterThanOrEqual(4.5);
          });
        }
      }
    }
  });

  describe("completeness", () => {
    it("every dark token has a resolved value in this theme", () => {
      const dark = resolvedTokens(css, "dark");
      const t = resolvedTokens(css, theme);
      for (const name of Object.keys(dark)) expect(t[name], name).toBeDefined();
    });
  });
});
