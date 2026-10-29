# Palette source

The OpenDeck palette is extracted from gittrunk, the author's own project, and reused as-is where it fits.

- Repository: https://github.com/R4ph3rd/gittrunk
- Commit: `128c1ad728981ac744486608926cef0b2ba6a4d9`
- Files read:
  - `src/design/tokens.css` (all tokens below)
  - `src/index.css` (fonts, Tailwind `@theme inline` mapping, base focus rule, `bg-chrome` utility)
  - `docs/DESIGN.md` (intent, contrast rules for the backdrop)

Nothing here is guessed. Values in the table are copied from `tokens.css` at that commit. Where OpenDeck differs, the change is listed under Deviations.

## Other extracted facts

| Item | Value | Source | OpenDeck |
|---|---|---|---|
| Sans font | Inter Variable via `@fontsource-variable/inter` | `src/index.css` | kept (OFL-1.1) |
| Mono font | JetBrains Mono Variable via `@fontsource-variable/jetbrains-mono` | `src/index.css` | kept (OFL-1.1) |
| Base text size | 13px | `--text-base`, body rule | kept |
| Focus rule | `:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 1px; }` | `src/index.css` | kept in `base.css` |
| Chrome utility | `bg-chrome` = `var(--gradient-chrome)` | `src/index.css` | kept; adds `bg-mesh`, `bg-mesh-page` |
| Keyframes | `ds-fade-in`, `ds-pop-in` | `tokens.css` | kept |
| Reduced motion | `--duration-fast` and `--duration-base` become `0.01ms` | `tokens.css` | kept |
| Tailwind mapping | `@theme inline` maps `--color-*`, `--radius-*`, `--shadow-*`, `--text-*`, `--ease-standard` | `src/index.css` | kept in `theme.css`, plus DJ colors |
| Custom variants `compact`, `coarse`, `short` | mobile layout | `src/index.css` | dropped (desktop app) |
| Direction | Dark first plus light, 13px text, 28px control, 1px borders, 120 to 180ms motion | `docs/DESIGN.md` | kept |

## Extracted tokens

Columns: dark value, light value (`same` means one value for both themes, set on `:root`), and what OpenDeck did with it.

| Token | Dark | Light | OpenDeck |
|---|---|---|---|
| `--font-sans` | `"Inter Variable", "Inter", ui-sans-serif, system-ui, sans-serif` | same | kept |
| `--font-mono` | `"JetBrains Mono Variable", "JetBrains Mono", ui-monospace, "Cascadia Code", monospace` | same | kept |
| `--text-xs` | `11px` | same | kept |
| `--text-sm` | `12px` | same | kept |
| `--text-base` | `13px` | same | kept |
| `--text-lg` | `15px` | same | kept |
| `--text-xl` | `18px` | same | kept |
| `--text-2xl` | `24px` | same | kept |
| `--space-1` | `4px` | same | kept |
| `--space-2` | `8px` | same | kept |
| `--space-3` | `12px` | same | kept |
| `--space-4` | `16px` | same | kept |
| `--space-6` | `24px` | same | kept |
| `--space-8` | `32px` | same | kept |
| `--control-sm` | `24px` | same | kept |
| `--control-md` | `28px` | same | kept |
| `--control-lg` | `32px` | same | kept |
| `--z-base` | `0` | same | kept |
| `--z-sticky` | `10` | same | kept |
| `--z-dropdown` | `50` | same | kept |
| `--z-overlay` | `100` | same | kept |
| `--z-modal` | `110` | same | kept |
| `--z-popover` | `120` | same | kept |
| `--z-toast` | `130` | same | kept |
| `--z-tooltip` | `140` | same | kept |
| `--radius-sm` | `4px` | same | kept |
| `--radius-md` | `6px` | same | kept |
| `--radius-lg` | `10px` | same | kept |
| `--duration-fast` | `120ms` | same | kept |
| `--duration-base` | `180ms` | same | kept |
| `--ease-standard` | `cubic-bezier(0.2, 0, 0, 1)` | same | kept |
| `--bp-compact` | `768px` | same | dropped (git or mobile specific) |
| `--touch-target` | `44px` | same | dropped (git or mobile specific) |
| `--touch-target-row` | `52px` | same | dropped (git or mobile specific) |
| `--appbar-h` | `48px` | same | dropped (git or mobile specific) |
| `--bottomnav-h` | `56px` | same | dropped (git or mobile specific) |
| `--navrail-w` | `72px` | same | dropped (git or mobile specific) |
| `--sheet-radius` | `var(--radius-lg)` | same | dropped (git or mobile specific) |
| `--sheet-handle` | `32px` | same | dropped (git or mobile specific) |
| `--sheet-duration` | `var(--duration-base)` | same | dropped (git or mobile specific) |
| `--swipe-duration` | `150ms` | same | dropped (git or mobile specific) |
| `--safe-top` | `env(safe-area-inset-top, 0px)` | same | dropped (git or mobile specific) |
| `--safe-bottom` | `env(safe-area-inset-bottom, 0px)` | same | dropped (git or mobile specific) |
| `--safe-left` | `env(safe-area-inset-left, 0px)` | same | dropped (git or mobile specific) |
| `--safe-right` | `env(safe-area-inset-right, 0px)` | same | dropped (git or mobile specific) |
| `--kb-inset` | `0px` | same | dropped (git or mobile specific) |
| `--appbar-h-short` | `40px` | same | dropped (git or mobile specific) |
| `--sheet-handle-h` | `4px` | same | dropped (git or mobile specific) |
| `--sheet-max-h` | `85dvh` | same | dropped (git or mobile specific) |
| `--progress-h` | `2px` | same | dropped (git or mobile specific) |
| `--bg` | `#0c0c0d` | `#fafafa` | kept |
| `--bg-subtle` | `#111113` | `#f4f4f5` | kept |
| `--surface` | `#161618` | `#ffffff` | kept |
| `--surface-raised` | `#1c1c1f` | `#ffffff` | kept |
| `--surface-hover` | `#232326` | `#efeff1` | kept |
| `--border` | `#2a2a2e` | `#e4e4e7` | kept |
| `--border-strong` | `#3a3a3f` | `#d0d0d6` | kept |
| `--fg` | `#ececef` | `#18181b` | kept |
| `--fg-muted` | `#a3a3ab` | `#52525b` | kept |
| `--fg-subtle` | `#8a8a92` | `#6b6b74` | kept |
| `--accent` | `#ff4f7b` | `#d6195a` | kept |
| `--accent-hover` | `#ff6b90` | `#bf1650` | kept |
| `--accent-fg` | `#1a070d` | `#ffffff` | kept |
| `--accent-muted` | `rgb(255 79 123 / 0.14)` | `rgb(214 25 90 / 0.1)` | kept |
| `--focus-ring` | `rgb(255 79 123 / 0.6)` | `rgb(214 25 90 / 0.45)` | kept, alpha raised for 3:1 (see below) |
| `--danger` | `#ff6a4d` | `#c2410c` | kept |
| `--danger-fg` | `#1f0a05` | `#ffffff` | kept |
| `--success` | `#3ecf8e` | `#15803d` | kept |
| `--warning` | `#f5b83d` | `#b45309` | kept |
| `--overlay` | `rgb(0 0 0 / 0.6)` | `rgb(24 24 27 / 0.35)` | kept |
| `--selection` | `rgb(255 79 123 / 0.28)` | `rgb(214 25 90 / 0.18)` | kept |
| `--scrollbar-thumb` | `#2e2e33` | `#d0d0d6` | kept |
| `--scrollbar-thumb-hover` | `#45454c` | `#b0b0b8` | kept |
| `--diff-add-bg` | `rgb(62 207 142 / 0.12)` | `rgb(21 128 61 / 0.12)` | dropped (git or mobile specific) |
| `--diff-del-bg` | `rgb(255 106 77 / 0.12)` | `rgb(194 65 12 / 0.1)` | dropped (git or mobile specific) |
| `--shadow-sm` | `0 1px 2px rgb(0 0 0 / 0.4)` | `0 1px 2px rgb(16 24 40 / 0.06)` | kept |
| `--shadow-md` | `0 4px 16px rgb(0 0 0 / 0.35), 0 1px 2px rgb(0 0 0 / 0.4)` | `0 4px 16px rgb(16 24 40 / 0.08), 0 1px 2px rgb(16 24 40 / 0.06)` | kept |
| `--shadow-lg` | `0 12px 40px rgb(0 0 0 / 0.5)` | `0 12px 40px rgb(16 24 40 / 0.14)` | kept |
| `--gradient-chrome` | `radial-gradient(60% 90% at 100% 0%, rgb(255 79 123 / 0.07) 0%, transparent 60%), linear-gradient(180deg, #111113 0%, #0c0c0d 100%)` | `radial-gradient(60% 90% at 100% 0%, rgb(214 25 90 / 0.06) 0%, transparent 60%), linear-gradient(180deg, #f4f4f5 0%, #fafafa 100%)` | kept |
| `--backdrop-red` | `#e11d48` | `#fb7185` | kept |
| `--backdrop-blue` | `#1d4ed8` | `#60a5fa` | kept |
| `--backdrop-alpha-subtle` | `0.136` | `0.08` | kept |
| `--backdrop-alpha-page` | `0.347` | `0.392` | kept |
| `--backdrop-blur-subtle` | `24px` | `24px` | kept |
| `--backdrop-blur-page` | `40px` | `40px` | kept |
| `--lane-0` | `#ff4f7b` | `#d6195a` | renamed `--cue-1` |
| `--lane-1` | `#5cc8ff` | `#0369a1` | renamed `--cue-2` |
| `--lane-2` | `#f5b83d` | `#b45309` | renamed `--cue-3` |
| `--lane-3` | `#a78bfa` | `#7c3aed` | renamed `--cue-4` |
| `--lane-4` | `#3ecf8e` | `#15803d` | renamed `--cue-5` |
| `--lane-5` | `#ff8a4c` | `#c2410c` | renamed `--cue-6` |
| `--lane-6` | `#2dd4bf` | `#0f766e` | renamed `--cue-7` |
| `--lane-7` | `#e879f9` | `#a21caf` | renamed `--cue-8` |
| `--tabbar-bg` | `#09090a` | `#e9e9ec` | kept |
| `--toolbar-bg` | `#19191b` | `#fafafa` | kept |
| `--tab-active-bg` | `var(--toolbar-bg)` | `var(--toolbar-bg)` | kept |
| `--tab-hover-bg` | `#141416` | `#f1f1f3` | kept |
| `--panel-header-bg` | `var(--bg-subtle)` | `var(--bg-subtle)` | kept |
| `--lane-fg` | `#111113` | `#ffffff` | renamed `--cue-fg` |
| `--avatar-ring` | `var(--surface)` | `var(--surface)` | dropped (git or mobile specific) |
| `--staged-accent` | `var(--success)` | `var(--success)` | dropped (git or mobile specific) |
| `--staged-bg` | `rgb(62 207 142 / 0.06)` | `rgb(21 128 61 / 0.06)` | dropped (git or mobile specific) |
| `--unstaged-accent` | `var(--warning)` | `var(--warning)` | dropped (git or mobile specific) |
| `--unstaged-bg` | `transparent` | `transparent` | dropped (git or mobile specific) |
| `--terminal-bg` | `#0e0e10` | `#ffffff` | dropped (git or mobile specific) |
| `--terminal-fg` | `var(--fg)` | `var(--fg)` | dropped (git or mobile specific) |
| `--terminal-cursor` | `var(--accent)` | `var(--accent)` | dropped (git or mobile specific) |
| `--terminal-selection` | `rgb(255 79 123 / 0.3)` | `rgb(214 25 90 / 0.2)` | dropped (git or mobile specific) |

## Deviations from gittrunk

| Change | Reason |
|---|---|
| `--focus-ring` alpha raised: dark 0.6 to 0.8, light 0.45 to 0.85 | The gittrunk values give 2.8:1 (dark) and 2.1:1 (light) against `--bg`, below the 3:1 non-text requirement. Same hue. |
| `--lane-0..7` renamed `--cue-1..8`, `--lane-fg` renamed `--cue-fg` | Hot cues use the same eight hues. The git graph lane concept is dropped. |
| Git tokens dropped: diff, staged, unstaged, terminal, avatar ring | No equivalent in a DJ app. |
| Mobile tokens dropped: breakpoints, touch targets, sheets, safe areas | OpenDeck is a desktop app. |
| `--backdrop-blur-*` moved to the theme-independent block | Same values in both themes in gittrunk. |
| Added `--row-h` (24px), `--wave-played-opacity`, `--wave-unplayed-opacity` | Dense tables and waveform dimming. New values, not from gittrunk. |
| Added `--gradient-mesh`, `--gradient-mesh-page` | Soft mesh built from the gittrunk `--backdrop-*` tokens (colors, alphas). |

## DJ tokens (derived)

All DJ tokens reference gittrunk palette values. Each has one derivation note.

| Token | Dark | Light | Derivation |
|---|---|---|---|
| `--deck-a` | `var(--cue-2)` | same ref | Cyan, the cool hue, for the left deck. |
| `--deck-b` | `var(--cue-1)` | same ref | The accent pink-red, for the right deck. |
| `--deck-c` | `var(--cue-3)` | same ref | Gold from the lane palette. |
| `--deck-d` | `var(--cue-4)` | same ref | Purple from the lane palette. |
| `--cue-1..8` | the eight `--lane-0..7` hex values | the eight light `--lane-0..7` hex values | Renamed lane palette; light values are the darker variants gittrunk already defines. |
| `--wave-low` | `var(--cue-6)` | same ref | Orange (bass, warm end of the palette). |
| `--wave-mid` | `var(--cue-5)` | same ref | Green. |
| `--wave-high` | `var(--cue-2)` | same ref | Cyan (treble, cool end). |
| `--wave-played-opacity` | `0.45` | same | New. Dims the part of the waveform already played; `--wave-unplayed-opacity` is `1`. |
| `--beat-grid` | `rgb(236 236 239 / 0.16)` | `rgb(24 24 27 / 0.14)` | `--fg` at low alpha in each theme (fg values from the table above). |
| `--beat-grid-downbeat` | `rgb(236 236 239 / 0.45)` | `rgb(24 24 27 / 0.45)` | Same base as the grid line, stronger alpha. |
| `--playhead` | `var(--fg)` | same ref | Highest contrast neutral. |
| `--loop-region` | `rgb(245 184 61 / 0.18)` | `rgb(180 83 9 / 0.16)` | `--warning` color at low alpha (gittrunk dark `#f5b83d`, light `#b45309`). |
| `--loop-edge` | `var(--warning)` | same ref | Solid edge for the loop fill. |
| `--meter-green` | `var(--success)` | same ref | Gittrunk status green. |
| `--meter-amber` | `var(--warning)` | same ref | Gittrunk status amber. |
| `--meter-red` | `var(--danger)` | same ref | Gittrunk coral danger. |
| `--meter-clip` | `var(--backdrop-red)` (`#e11d48`) | `var(--accent-hover)` (`#bf1650`) | Deeper rose than meter red so clip is distinct. Light uses accent-hover because light `--backdrop-red` (`#fb7185`) is below 3:1. |
| `--meter-off` | `var(--surface-hover)` | `var(--border)` | Unlit segment. |
| `--sync-on` | `var(--cue-7)` | same ref | Teal, kept apart from the deck colors. |
| `--master-on` | `var(--cue-3)` | same ref | Gold. |
| `--gradient-mesh` | two radial gradients | same | `--backdrop-red` top left and `--backdrop-blue` bottom right at `--backdrop-alpha-subtle`. Chrome and empty states only. |
| `--gradient-mesh-page` | two radial gradients | same | Same at `--backdrop-alpha-page`. |

Deck colors are tested at 3:1 against `--bg` in both themes. See `ui/packages/design-system/src/tokens/contrast.test.ts`.
