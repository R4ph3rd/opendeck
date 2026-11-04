# Palette source

## Palette notes (tables below are the previous palette)

The palette follows the colour language of a classic DAW dark theme: flat neutral grey surfaces, light grey text, one red accent for selection, lit states and transport. The waveform is a red ramp (deep red, red-orange, warm light) on a near-black ground. Deck A and B differ by tone, not hue. Hot cue colours are muted and appear only on the pads. Meters stay green, amber, red, desaturated. Current values: `tokens.css` and [theming.md](theming.md); the tables below list the previous orange palette and are kept for history. No name, logo, icon or asset from any DAW is used in the app or the package.

**Every value below is an approximation from general knowledge.** The owner could not provide an extraction and the vendor site was not reachable from the build environment, so nothing was sampled from a real UI. The owner may supply screenshots; exact values can then be sampled and these tokens adjusted in `tokens.css` alone (the contrast tests will say which pairs need tuning).

Rules that hold for the whole palette: flat fills only, no gradients, no blur or `backdrop-filter` (banned by stylelint and ESLint), no translucent panels, menus, dialogs or toasts. Only the dim scrim (`--overlay`), text selection and the waveform and beat grid overlays use alpha.

Fonts are unchanged: Inter Variable for UI text, JetBrains Mono Variable for numbers (both OFL-1.1). Radii are small: 2, 3 and 4 px. Shadows are a hard 1px drop plus a 1px top highlight.

### Surfaces and text

| Token | Dark | Light | Rationale |
|---|---|---|---|
| `--bg` | `#1f272c` | `#e2e9ec` | App canvas, the darkest step, slate with a blue-green cast. |
| `--bg-subtle` | `#242d32` | `#eaf0f2` | Panel headers and inputs, one step up. |
| `--surface` | `#2b353b` | `#f5f8f9` | Panels, a step lighter than the canvas. |
| `--surface-raised` | `#36424a` | `#ffffff` | Menus, dialogs, raised controls. |
| `--surface-hover` | `#3d4b53` | `#d7e0e5` | Hover and pressed fill. |
| `--border` | `#161c20` | `#bcc8cf` | Thin separator, darker than every surface in dark. |
| `--border-strong` | `#566670` | `#8a9aa5` | Control outlines, off-LED ring. |
| `--fg` | `#e3e9ec` | `#1b2429` | Primary light grey text. |
| `--fg-muted` | `#b4c0c7` | `#44535d` | Secondary text, 4.5:1 on every surface. |
| `--fg-subtle` | `#94a3ad` | `#5a6a75` | Placeholders and disabled hints, 3:1. |

### Accent and status

| Token | Dark | Light | Rationale |
|---|---|---|---|
| `--accent` | `#ff9a2e` | `#a24900` | Warm orange: selection, active transport. Light is darkened for 3:1 on the canvas. |
| `--accent-hover` | `#ffb04f` | `#873c00` | One step brighter (dark) or deeper (light). |
| `--accent-fg` | `#1f1304` | `#ffffff` | Text on accent fills, 4.5:1. |
| `--accent-muted` | `#43321d` | `#fbe3c8` | Solid on-state fill for toggles; accent text on it is 4.5:1. |
| `--focus-ring` | `#ffb04f` | `#8f4000` | Solid, 3:1 on every surface. |
| `--selection` | `rgb(255 154 46 / 0.35)` | `rgb(162 73 0 / 0.25)` | Text selection only. |
| `--danger` / `--danger-fg` | `#ff7260` / `#2a0a05` | `#b52b17` / `#ffffff` | Coral red. |
| `--success` | `#92d94a` | `#36700a` | Green for success and the meter low segment. |
| `--warning` | `#f5c542` | `#8a5d00` | Yellow for warnings and the meter mid segment. |
| `--info` | `#6fa0ff` | `#2a56c6` | Blue for information. |
| `--danger-bg`, `--success-bg`, `--warning-bg`, `--info-bg` | `#442a29`, `#2d3c25`, `#4a4026`, `#273755` | `#fbe0dc`, `#e1f0d2`, `#f8ecc8`, `#dce6fb` | Solid callout fills. Status text and `fg` on them are 4.5:1. |
| `--overlay` | `rgb(8 12 14 / 0.72)` | `rgb(27 36 41 / 0.4)` | Solid dim scrim behind modals. |
| `--scrollbar-thumb`, `-hover` | `#4a5963`, `#627480` | `#b4c1c9`, `#93a3ae` | Quiet slate thumb. |
| `--shadow-sm`, `-md`, `-lg` | hard 1px drop, plus `inset 0 1px 0` highlight on md and lg | same shape, lighter | Minimal, bevel-like. |

### Shell

| Token | Dark | Light | Rationale |
|---|---|---|---|
| `--tabbar-bg` | `#181f23` | `#d3dce1` | Darker than the canvas. |
| `--toolbar-bg` | `#2b353b` | `#f5f8f9` | Same step as panels. |
| `--tab-hover-bg` | `#202930` | `#e0e8ec` | Between tab bar and canvas. |
| `--tab-active-bg`, `--panel-header-bg` | `var(--toolbar-bg)`, `var(--bg-subtle)` | same refs | Aliases. |

### DJ tokens

| Token | Dark | Light | Rationale |
|---|---|---|---|
| `--cue-1` red | `#ff6b6b` | `#c8303a` | Hot cue hues: red, orange, yellow, green, cyan, blue, purple, pink. |
| `--cue-2` orange | `#ff9a2e` | `#a24900` | Same hue as the accent. |
| `--cue-3` yellow | `#f2d03b` | `#8a6a00` | |
| `--cue-4` green | `#7ed957` | `#3a7a1a` | |
| `--cue-5` cyan | `#38c6e8` | `#00758f` | |
| `--cue-6` blue | `#6f9bff` | `#2a56c6` | |
| `--cue-7` purple | `#b78cff` | `#7040d0` | |
| `--cue-8` pink | `#ff7ac8` | `#b52f85` | |
| `--cue-fg` | `#14191c` | `#ffffff` | Text on a cue fill, 4.5:1 on all eight. |
| `--led-on` | `#b6e94a` | `#3f7a0e` | Lime for lit LEDs, SYNC, pads. |
| `--sync-on` | `var(--led-on)` | same ref | Lit SYNC. |
| `--master-on` | `var(--cue-3)` | same ref | Yellow MASTER. |
| `--deck-a` | `var(--cue-2)` | same ref | Orange. |
| `--deck-b` | `var(--cue-5)` | same ref | Cyan. Orange against cyan differs on the blue-yellow axis, so it holds for the common colour-vision deficiencies. |
| `--deck-c`, `--deck-d` | `var(--cue-7)`, `var(--cue-4)` | same refs | Purple, green. |
| `--wave-low`, `-mid`, `-high` | `var(--cue-2)`, `var(--cue-4)`, `var(--cue-5)` | same refs | Orange, green, cyan: three well separated hues on slate. |
| `--wave-played-opacity` | `0.45` | same | Dims the played part. |
| `--beat-grid` | `rgb(227 233 236 / 0.14)` | `rgb(27 36 41 / 0.14)` | Low-contrast grey line. |
| `--beat-grid-downbeat` | `rgb(227 233 236 / 0.45)` | `rgb(27 36 41 / 0.45)` | Brighter downbeat. |
| `--playhead` | `var(--fg)` | same ref | Highest contrast neutral. |
| `--loop-region`, `--loop-edge` | `rgb(242 208 59 / 0.2)`, `var(--cue-3)` | `rgb(138 106 0 / 0.18)`, same ref | Yellow loop fill and edge. |
| `--meter-green`, `-amber`, `-red` | `var(--success)`, `var(--warning)`, `var(--danger)` | same refs | Green, yellow, red. |
| `--meter-clip` | `#ff3b4e` | `#c4162a` | Hotter red than the meter red segment. |
| `--meter-off` | `#3a4750` | `#b4c1c9` | Unlit segment. |

### Unchanged scales

Type sizes (11 to 24 px), spacing (4 px scale), control heights (24, 28, 32), `--row-h`, z-index layers, durations (120 ms, 180 ms, plus `--duration-blink` 1000 ms and `--duration-sweep` 1400 ms), `--ease-standard`, `--slider-track-length`. Radii changed to 2, 3, 4 px.

### Removed tokens

`--gradient-chrome`, `--gradient-mesh`, `--gradient-mesh-page`, `--backdrop-red`, `--backdrop-blue`, `--backdrop-alpha-subtle`, `--backdrop-alpha-page`, `--backdrop-blur-subtle`, `--backdrop-blur-page`, and the utilities `bg-chrome`, `bg-mesh`, `bg-mesh-page`. Gradient backgrounds and the empty state backdrop are gone; `EmptyState` is flat.

## Previous source, superseded

The first palette was extracted from the author's project gittrunk (`https://github.com/R4ph3rd/gittrunk`, commit `128c1ad728981ac744486608926cef0b2ba6a4d9`, files `src/design/tokens.css`, `src/index.css`, `docs/DESIGN.md`): near-black neutrals, pink-red accent, a mesh backdrop. It is replaced by the table above and kept only in git history. The structure it set (token names, theme blocks, Tailwind mapping, focus rule, motion tokens, fonts) is unchanged.
