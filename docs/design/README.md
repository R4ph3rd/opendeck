# OpenDeck design system

Package: `ui/packages/design-system` (`@opendeck/design-system`). Palette origin: [palette-source.md](palette-source.md).

Dark is the default theme. Light is set with `data-theme="light"` on `<html>` (or any wrapper).

## Use it

The app imports one stylesheet (already done in `ui/src/main.tsx`):

```ts
import "@opendeck/design-system/styles.css";
```

Components come from the package index: `import { Button, Panel } from "@opendeck/design-system"`. Style with token classes (`bg-surface`, `text-fg-muted`, `border-border`, `text-deck-a`) or `var(--token)` arbitrary values (`h-[var(--control-md)]`).

## Files

| File | Role |
|---|---|
| `src/tokens/tokens.css` | The only place for raw colors, sizes and durations. Dark default, `[data-theme="light"]` overrides. |
| `src/tokens/theme.css` | Tailwind v4 `@theme inline` mapping from tokens to utilities. |
| `src/tokens/base.css` | Global rules (body, focus ring, scrollbar, selection). |
| `src/styles.css` | Entry: fonts, Tailwind, the three files above. |
| `src/components/*` | shadcn/ui on Radix, restyled with tokens, one story per file. |
| `src/tokens/contrast.test.ts` | WCAG contrast tests that parse `tokens.css`. |

## Components

Base (shadcn/ui on Radix): Badge, Button, ContextMenu, Dialog, DropdownMenu, EmptyState, IconButton, Input, Kbd, Panel, ScrollArea, Select, Separator, Slider, Switch, Table, Tabs, Toast, Toggle and ToggleGroup, Tooltip.

Added in milestone 4:

| Component | Use | Built on |
|---|---|---|
| `ResizablePanelGroup`, `ResizablePanel`, `ResizableHandle` | Deck, middle panel and collection split. Persist with `onLayoutChanged`, restore with `defaultLayout`. | `react-resizable-panels` (MIT) |
| `Popover`, `HoverCard` | Click-opened panel; hover and focus preview. | Radix |
| `Command`, `CommandPalette`, `Combobox` | Quick search and commands (Mod+K), searchable select. | `cmdk` (MIT) |
| `NumberField` | BPM entry: type, arrows, wheel (focused), drag-to-scrub, precision. | native input |
| `ColorSwatchPicker` | Hot cue colour from the 8 `--cue-N` tokens. | Radix RadioGroup |
| `SegmentedControl` | One of a few values (tempo range, beat size). | Radix RadioGroup |
| `StatusDot` | LED for SYNC, MASTER, loop: on, off, blink. | native |
| `ProgressBar`, `Skeleton` | Import progress (determinate or not); loading placeholders. | Radix Progress |
| `Breadcrumb` | File explorer path. | native |
| `KeyValueList` | Info and settings summaries. | native |
| `Callout` | Inline info, success, warning, danger message. | native |
| `SettingsLayout`, `SettingsNav`, `SettingsSection`, `SettingsRow` | Frame for the settings dialog. | native |

Storybook "Patterns" section: dense panel header, empty state, settings page skeleton.

## Add a token

1. Add the variable to `tokens.css`. If it is a color or shadow, define it for dark in the `:root, [data-theme="dark"]` block and for light in `[data-theme="light"]`. A token that only references other tokens (`var(--x)`) goes in the shared block at the end so nested theme wrappers re-resolve it.
2. If utilities should exist (`bg-*`, `text-*`), map it in `theme.css` under `@theme inline`.
3. Add a contrast case to `contrast.test.ts` if it is text or essential UI.
4. Add a derivation note to `palette-source.md` if it is new.
5. The Tokens page in Storybook lists it automatically (it parses `tokens.css`).

## Add a component

1. Create `src/components/<name>.tsx`. Start from the Radix primitive (`import { X } from "radix-ui"`) or a plain element.
2. Style with token classes only. Use `cn()` and `focusRing` from `src/lib/cn.ts`. Every interactive element needs a visible `:focus-visible` style.
3. Variants go through `cva`. Icons come from `lucide-react`.
4. Export from `src/index.ts`.
5. Add `<name>.stories.tsx` with the default state, variants and disabled/invalid states.

## Lint rules

- Stylelint (`ui/stylelint.config.js`, `stylelint-declaration-strict-value`): color properties, spacing, gap, radius, font-size, box-shadow and z-index must use `var(--token)` or a neutral keyword. `tokens.css` is exempt.
- ESLint (`opendeck-tokens/no-raw-style-literals`, source in `eslint/no-raw-style-literals.js`): string and template literals in TS/TSX must not contain hex colors, `rgb()`/`hsl()`/`oklch()` or Tailwind arbitrary lengths such as `h-[28px]`. Use a token or a scale class (`h-7`).
- Proof: `src/lint/lint.test.ts` feeds raw values to both linters and expects failures.
- Escape hatch for a false positive (for example a URL fragment that looks like hex): `// eslint-disable-next-line opendeck-tokens/no-raw-style-literals` with a reason.

## Contrast policy

Tested in both themes: `fg`, `fg-muted` at 4.5:1 on `bg` and `surface`; `fg-subtle` at 3:1 (placeholders, disabled hints, tertiary labels only); `accent-fg` on `accent` at 4.5:1; `focus-ring` at 3:1 on `bg`, `surface`, `surface-raised`; deck, cue, waveform and meter colors at 3:1 on `bg`; backdrop mesh at peak alpha keeps `fg` and `fg-muted` at 4.5:1.

## Commands (run from the repo root)

```sh
pnpm -C ui storybook          # dev server on :6006
pnpm -C ui build-storybook    # static build, run by CI
pnpm -C ui lint               # eslint + stylelint
pnpm -C ui test               # app tests + design-system tests (contrast, lint proof)
pnpm -C ui typecheck
```
