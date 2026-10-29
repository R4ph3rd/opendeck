# design-system

Owner: design-system-dev.

## Purpose
Single source of style: design tokens (colour, type, spacing, radius, motion, z-index), Tailwind CSS v4 `@theme`, shadcn/ui components on Radix restyled with tokens, Storybook, and lint rules that forbid raw hex and magic numbers.

## Owned paths
- `ui/packages/design-system/**` (stub `package.json` created in wave 0a)
- `docs/design/**`

## Depends on
- Nothing from Rust. Consumed by `ui/src` (ui-shell) and `ui/src/viz` (ui-viz).

## Libraries (ADR 0001)
- Tailwind CSS v4, shadcn/ui + Radix, Motion, Storybook 9 (React + Vite), Stylelint with `stylelint-declaration-strict-value`, a custom ESLint rule for TS/TSX literals, Vitest.

## Rules
- Dark theme first, light theme ready (token layer, not component forks).
- Contrast checks run as tests (WCAG AA for text and essential UI).
- Deck colours, cue colour palette and waveform band colours are tokens; hot cue colours stored as RGB in the DB map to the nearest palette token for display.
- Docs and UI copy: concise, no em-dashes.

## Acceptance per milestone
- Wave 0b: token package, Tailwind theme export, Storybook building, lint rules active in `ui/`, 5-10 base components (button, toggle, slider, knob shell, tabs, dialog, tooltip, table shell).
- M1-M4: components needed by deck, mixer, collection and pad panels.
- M8: theme switching.

## Open questions
- Brand identity (logo, primary colour); the app icon is a placeholder from `src-tauri/icons/source.svg`.
- Font choice and licence.
