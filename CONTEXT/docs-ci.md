# docs-ci

Owner: docs-ci-dev.

## Purpose
CI and release workflows, scripts, changelog, and mechanical doc edits.

## Owned paths
- `.github/**`, `scripts/**`, `CHANGELOG.md`, docs files a task names.

## Depends on
- The gate commands below; no code interfaces.

## Gate (every PR and push to workbench)
1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace` (includes the bindings drift test and DB migration tests)
4. `cargo run -p opendeck-desktop --example export-bindings && git diff --exit-code ui/src/ipc/bindings.ts`
5. `pnpm -C ui install --frozen-lockfile && pnpm -C ui typecheck && pnpm -C ui lint && pnpm -C ui test`
6. Storybook build (from wave 0b)
7. `cargo deny check` (licences: MIT, Apache-2.0, BSD, Zlib, ISC, CC0, Unicode, MPL-2.0 unmodified; deny GPL, LGPL, AGPL)

## Jobs
- `ci.yml`: Linux (gate + `pnpm -C ui tauri build --bundles deb,appimage`), Windows (`cargo test`, `pnpm -C ui tauri build --no-bundle`; primary target), macOS (`cargo test`).
- `release.yml`: on tag, build nsis + msi (Windows), deb + appimage (Linux), universal dmg (macOS, `universal-apple-darwin`), upload artifacts.
- Linux runners need `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev` (ALSA for cpal from M1).
- Cache: `Swatinem/rust-cache`, pnpm store.

## Acceptance
- Wave 0b: `ci.yml` green on the workbench branch with all gate steps; `deny.toml` committed.
- M8: `release.yml` produces all five bundle types.

## Open questions
- Code signing for Windows and macOS (certificates, secrets) before public releases.
- A dependency-graph check that enforces the ADR 0002 module map (e.g. `cargo metadata` script).
