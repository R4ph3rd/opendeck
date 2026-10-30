import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import { rawTokens, resolvedTokens } from "./parse";
import css from "./tokens.css?raw";

// Names come from tokens.css itself, so this page cannot drift from the tokens.
const raw = rawTokens(css, "dark");
const resolved = resolvedTokens(css, "dark");
const names = Object.keys(raw);

const isColor = (n: string): boolean => /^#|^rgb\(/.test(resolved[n] ?? "");
const pick = (re: RegExp): string[] => names.filter((n) => re.test(n) && isColor(n));

const DJ = /^(deck-|cue-|wave-|beat-|playhead|loop-|meter-|sync-|master-)/;
const groups: { title: string; note: string; names: string[] }[] = [
  {
    title: "Surfaces and borders",
    note: "bg is the app canvas, surface holds panels, surface-raised holds menus and dialogs. All surfaces are flat and opaque. Use border for hairlines.",
    names: pick(/^(bg|bg-subtle|surface|surface-raised|surface-hover|border|border-strong|overlay)$/),
  },
  {
    title: "Text",
    note: "fg and fg-muted pass 4.5:1 on every surface step. fg-subtle passes 3:1 only: placeholders, disabled hints, tertiary labels.",
    names: pick(/^fg/),
  },
  {
    title: "Accent and status",
    note: "Warm orange accent. accent-fg is the text on accent fills. focus-ring is the :focus-visible outline. The *-bg tokens are solid callout fills; status text on them passes 4.5:1.",
    names: pick(/^(accent|focus-ring|selection|danger|success|warning|info)/),
  },
  {
    title: "Shell",
    note: "Tab strip, toolbar and panel headers.",
    names: pick(/^(tabbar-bg|toolbar-bg|tab-|panel-header-bg|scrollbar)/),
  },
  {
    title: "DJ: decks",
    note: "One accent per deck. Use for deck headers, waveform outline and load indicators.",
    names: pick(/^deck-/),
  },
  {
    title: "DJ: hot cues",
    note: "Eight palette colors. Cues stored as RGB in the DB map to the nearest token. cue-fg is the text on a cue fill.",
    names: pick(/^cue-/),
  },
  {
    title: "DJ: waveform",
    note: "Low, mid, high bands. Dim the played part with --wave-played-opacity. Beat grid lines are decorative; downbeat is stronger.",
    names: pick(/^(wave-|beat-|playhead|loop-)/),
  },
  {
    title: "DJ: meters and indicators",
    note: "Meter segments run green, yellow, red, then clip. led-on (lime) lights LEDs, SYNC and pads; master-on is yellow.",
    names: pick(/^(meter-|sync-|master-|led-)/),
  },
];

function Swatch({ name }: { name: string }) {
  return (
    <div className="flex items-center gap-2">
      <span
        className="size-8 shrink-0 rounded-md border border-border"
        style={{ background: `var(--${name})` }}
      />
      <span className="min-w-0">
        <code className="block truncate text-xs text-fg">--{name}</code>
        <code className="block truncate text-xs text-fg-subtle">{raw[name]}</code>
      </span>
    </div>
  );
}

function Section({ title, note, children }: { title: string; note?: string; children: ReactNode }) {
  return (
    <section className="mb-8">
      <h2 className="mb-1 text-xl font-semibold text-fg">{title}</h2>
      {note ? <p className="mb-3 max-w-2xl text-sm text-fg-muted">{note}</p> : null}
      {children}
    </section>
  );
}

function Colors() {
  return (
    <>
      {groups.map((g) => (
        <Section key={g.title} title={g.title} note={g.note}>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(14rem,1fr))] gap-3">
            {g.names.map((n) => (
              <Swatch key={n} name={n} />
            ))}
          </div>
        </Section>
      ))}
    </>
  );
}

const textSizes = ["xs", "sm", "base", "lg", "xl", "2xl"];
const spaces = names.filter((n) => /^space-/.test(n));
const radii = names.filter((n) => /^radius-/.test(n));
const shadows = names.filter((n) => /^shadow-/.test(n));
const durations = names.filter((n) => /^duration-/.test(n));
const zs = names.filter((n) => /^z-/.test(n));

function Tokens() {
  return (
    <div className="p-6">
      <h1 className="mb-1 text-2xl font-semibold text-fg">Design tokens</h1>
      <p className="mb-6 max-w-2xl text-sm text-fg-muted">
        Tokens live in <code>src/tokens/tokens.css</code>. Dark is the default; use the toolbar to
        preview light. Use token classes (<code>bg-surface</code>, <code>text-fg-muted</code>) or{" "}
        <code>var(--token)</code>. Raw hex and pixel values fail lint. The palette is flat slate with an
        orange accent, approximated from general knowledge of a classic DAW dark theme and not sampled
        (see <code>docs/design/palette-source.md</code>). No gradients, blur or translucent surfaces.
      </p>

      <Colors />

      <Section
        title="Type scale"
        note="Inter Variable for UI, JetBrains Mono Variable for numbers, BPM, time and keys. Base is 13px."
      >
        <div className="flex flex-col gap-2">
          {textSizes.map((s) => (
            <div key={s} className="flex items-baseline gap-4">
              <code className="w-24 text-xs text-fg-subtle">
                --text-{s} {raw[`text-${s}`]}
              </code>
              <span style={{ fontSize: `var(--text-${s})` }}>Deck A 128.00 BPM</span>
              <span className="font-mono" style={{ fontSize: `var(--text-${s})` }}>
                03:42.1
              </span>
            </div>
          ))}
        </div>
      </Section>

      <Section title="Spacing" note="4px scale. Tailwind spacing classes map to it (p-2 is 8px).">
        <div className="flex flex-col gap-2">
          {spaces.map((n) => (
            <div key={n} className="flex items-center gap-3">
              <code className="w-32 text-xs text-fg-subtle">
                --{n} {raw[n]}
              </code>
              <span className="h-3 bg-accent" style={{ width: `var(--${n})` }} />
            </div>
          ))}
        </div>
      </Section>

      <Section title="Radii" note="sm for chips and inputs, md for controls, lg for panels and dialogs.">
        <div className="flex gap-4">
          {radii.map((n) => (
            <div key={n} className="flex flex-col items-center gap-1">
              <span
                className="size-12 border border-border-strong bg-surface-raised"
                style={{ borderRadius: `var(--${n})` }}
              />
              <code className="text-xs text-fg-subtle">
                --{n} {raw[n]}
              </code>
            </div>
          ))}
        </div>
      </Section>

      <Section title="Shadows" note="Menus use md, dialogs use lg. Panels are flat with a border.">
        <div className="flex gap-6">
          {shadows.map((n) => (
            <div key={n} className="flex flex-col items-center gap-2">
              <span
                className="size-16 rounded-lg bg-surface-raised"
                style={{ boxShadow: `var(--${n})` }}
              />
              <code className="text-xs text-fg-subtle">--{n}</code>
            </div>
          ))}
        </div>
      </Section>

      <Section
        title="Motion"
        note="Hover the boxes. Durations collapse to near zero under prefers-reduced-motion."
      >
        <div className="flex gap-4">
          {durations.map((n) => (
            <div key={n} className="flex flex-col items-start gap-2">
              <span
                className="size-12 rounded-md bg-surface-hover hover:translate-x-4 hover:bg-accent"
                style={{
                  transition: `transform var(--${n}) var(--ease-standard), background-color var(--${n}) var(--ease-standard)`,
                }}
              />
              <code className="text-xs text-fg-subtle">
                --{n} {raw[n]}
              </code>
            </div>
          ))}
          <code className="self-end text-xs text-fg-subtle">--ease-standard {raw["ease-standard"]}</code>
        </div>
      </Section>

      <Section title="Z-index" note="Always use a named layer. Never a literal.">
        <code className="block text-xs text-fg-muted">
          {zs.map((n) => `--${n}: ${raw[n]}`).join("   ")}
        </code>
      </Section>

      <Section title="DJ token list" note="Everything matched by the DJ groups above, for reference.">
        <code className="block text-xs text-fg-muted">
          {names.filter((n) => DJ.test(n)).map((n) => `--${n}`).join("  ")}
        </code>
      </Section>
    </div>
  );
}

const meta = { title: "Foundations/Tokens", component: Tokens } satisfies Meta<typeof Tokens>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Overview: Story = {};
