# Theming guide

Every colour and size of the performance layout is a CSS variable in one file:
`ui/packages/design-system/src/tokens/tokens.css`. Change a value there and the app, the canvas waveforms and Storybook follow. Components never hold a hex value or a size literal (ESLint and Stylelint fail the build if they do).

The file is grouped and commented by area. Dark values are in `:root, [data-theme="dark"]`, light overrides in `[data-theme="light"]`. Tokens that are the same in both themes (waveform, deck tones) are in the last shared block.

## Change the palette

Look and feel: neutral grey surfaces, one red accent family, everything else muted.

| Area | Tokens | Notes |
|---|---|---|
| Surfaces | `--bg`, `--bg-subtle`, `--surface`, `--surface-raised`, `--surface-hover`, `--border`, `--border-strong` | Darkest to lightest in that order. `--border` is darker than every surface. |
| Text | `--fg`, `--fg-muted`, `--fg-subtle` | `fg` and `fg-muted` must stay 4.5:1 on every surface. |
| Accent | `--accent`, `--accent-hover`, `--accent-fg`, `--accent-muted`, `--focus-ring`, `--selection` | `--accent-fg` is text on `--accent` (4.5:1). `--focus-ring` needs 3:1 on every surface. |
| Status | `--danger`, `--success`, `--warning`, `--info` and their `-bg` fills | Callouts and badges only. |
| Shell | `--toolbar-bg`, `--tabbar-bg`, `--tab-hover-bg` | Menu row and tabs. |
| Deck tones | `--deck-a` to `--deck-d` | Decks differ by tone and by the 1/2 label. They reference `--fg`, `--fg-muted`, `--fg-subtle`. |
| Cues | `--cue-1` to `--cue-8`, `--cue-fg` | Hot cue pad colours. Only the pads use them. `--cue-fg` is the text on a pad (4.5:1). |
| LEDs | `--led-on`, `--sync-on`, `--master-on` | Follow the accent by default. |

To retint the whole app red to blue, change the six accent tokens in both theme blocks. Run `pnpm -C ui/packages/design-system test`: the contrast tests tell you if a pair fell below its minimum.

## Dark surface scale

Dark theme values, near-black and flat. Contrast tests stay green (text 4.5:1, focus ring 3:1).

| Token | Value |
|---|---|
| `--bg` | `#0b0c0d` |
| `--bg-subtle` | `#101214` |
| `--surface` | `#15171a` |
| `--surface-raised` | `#1d2023` |
| `--surface-hover` | `#292d31` |
| `--border` | `#060707` |
| `--border-strong` | `#4a5157` |
| `--toolbar-bg`, `--tabbar-bg`, `--tab-hover-bg` | `#131517`, `#08090a`, `#181a1d` |
| `--accent-muted` | `#2a1618` |

## Restrained controls

- Pads and sampler slots are neutral (`--surface-raised`). A set cue or sample shows a 3px left edge in its muted `--cue-1` to `--cue-8` colour (low chroma). Pressed or sounding: `--surface-hover` plus a 1px accent inset.
- Latching buttons (Q, MT, VINYL, SYNC, MASTER, mode tabs) are neutral; when on they get accent text and a 1px accent outline, never a solid fill.
- Labels use the `ds-label` utility: `--text-label` (10.5px), `--weight-label` (500), `--tracking-label` (0.04em), uppercase.
- Slider caps are thin rectangles: `--slider-thumb-along` (5px, travel direction) by `--slider-thumb-across` (12px).

## Waveform drawing

Canvases are sized to their CSS box times `devicePixelRatio` by a `ResizeObserver` and redraw at the new size. Columns are one device pixel wide with whole-pixel heights, taken from the finest peak level that has at least one point per column; a column takes the max of the points it covers and never interpolates. Beat grid lines are 1 device pixel, the playhead 2.

## Change the waveform colours

The waveform is a red ramp on a near-black ground. Bands are drawn low, mid, high on top of each other, so `--wave-low` is the outer envelope.

| Token | Default | Role |
|---|---|---|
| `--wave-bg` | `#08090a` | Ground behind zoomed and overview waveforms. |
| `--wave-low` | `#b52b33` | Bass: deep red, outer shape. |
| `--wave-mid` | `#ea5a3c` | Mids: red-orange. |
| `--wave-high` | `#ffcfb0` | Highs: warm light core. |
| `--wave-played-opacity` | `0.45` | How much the played part dims (0 to 1). |
| `--wave-unplayed-opacity` | `1` | Opacity of the part still to play. |
| `--playhead` | `#ffffff` | Playhead line. |
| `--beat-grid`, `--beat-grid-downbeat` | 10% and 38% light | Subtle grid, stronger downbeat. |
| `--loop-region`, `--loop-edge` | accent at 18%, `--accent-hover` | Active loop. |

A single-hue look: set all three band tokens to one red and keep their lightness steps apart (dark, mid, light) so the bands stay readable.

Meters keep green, amber, red, desaturated: `--meter-green`, `--meter-amber`, `--meter-red`, `--meter-clip`, `--meter-off`.

Canvas components read tokens through `ui/src/viz/colors.ts` (`TOKEN_NAMES`). A new token the canvas needs must be added to that list.

## Change sizes

LAYOUT SIZES group in `tokens.css`. Use them in components as `h-[var(--deck-header-h)]`.

Size tokens are rem; the table gives px at UI scale 1 (1rem = 16px). The UI scale (Settings, Interface: auto or manual) sets the root font size to `16px * --ui-scale`, so every rem token and Tailwind utility scales with it. Do not use CSS `zoom` for this: Radix menus, selects and tooltips are positioned wrong under a zoomed root in Chromium and WebKitGTK.

| Token | Default | What it sizes |
|---|---|---|
| `--toolbar-h` | 28px | Top menu row. |
| `--deck-header-h` | 46px | Deck number, title, clocks, key, SYNC and MASTER. |
| `--deck-overview-h` | 28px | Overview waveform under the deck header. |
| `--deck-tabs-h` | 24px | Pad mode tabs above the hot cue grid, and the pane header in the middle row. |
| `--deck-strip-h` | 28px | Reserved height hint for the beat loop and jump strip. |
| `--mixer-w` | 312px | Centre mixer column. |
| `--jog-max` | 520px | Largest jog diameter. The jog also shrinks to the deck height. |
| `--transport-w` | 52px | CUE and PLAY column. |
| `--modes-w` | 40px | SLIP, Q, MT, VINYL column. |
| `--tempo-w` | 36px | Tempo fader column at the outer deck edge. |
| `--pad-min-w` | 56px | Smallest hot cue pad height. |
| `--fx-unit-w` | 280px | Beat FX block on each side of the middle row. |
| `--preview-col-w`, `--artwork-col-w` | 96px, 28px | Browser Preview and Artwork columns (starting widths are in `trackColumns.tsx`). |
| `--row-h` | 24px | Browser row height. |
| `--text-xs` to `--text-2xl`, `--space-*`, `--radius-*` | | Type scale, spacing, corner radii. |

Band heights (waveforms, decks, middle row, browser) are draggable and stored per user. Their defaults are percentages in `ui/src/state/prefs.ts` (`DEFAULT_LAYOUT`) and their minimum heights are px at scale 1 in `ui/src/lib/layout.ts` (`BAND_MIN_PX`), passed to the resizable panels as rem so they follow the UI scale. "Reset layout" in the View menu restores the defaults.

## Rules

- Raw colours and size literals live only in `tokens.css` (and the Storybook preview). Everything else uses a token class (`bg-surface`, `text-fg-muted`) or `var(--token)`.
- A new colour token needs: a dark and a light value, a mapping in `theme.css` for utilities, and a contrast case in `contrast.test.ts` when it carries text.
- Keep it flat: no gradients, blur or translucent panels.
