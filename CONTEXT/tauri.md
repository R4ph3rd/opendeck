# tauri (src-tauri)

Owner: ui-shell-dev.

## Purpose
Thin Tauri 2 shell: window, typed commands and events over `opendeck_app_core`, bindings export, bundling.

## Owned paths
- `src-tauri/**` (icons regenerated with `pnpm -C ui tauri icon src-tauri/icons/source.svg`)
- `ui/src/ipc/bindings.ts` is generated from here; editing commands is a contract change (ADR 0004, `docs/ipc.md`).

## Depends on
- `opendeck-app-core`, `opendeck-contracts` (`specta` feature).

## External crates
- `tauri` 2.12, `tauri-build` 2.7, `tauri-specta` `=2.0.0-rc.26`, `specta` `=2.0.0-rc.26`, `specta-typescript` `=0.0.13`, `serde`.

## How things work
- Commands: `#[tauri::command] #[specta::specta]`, registered in `specta_builder()`. Fallible ones return `Result<T, CommandError>`.
- Events: derive `tauri_specta::Event` with `#[tauri_specta(event_name = "...")]`.
- Regenerate bindings: `cargo run -p opendeck-desktop --example export-bindings`. Test `bindings_are_up_to_date` fails on drift; `contract_dtos_export` checks every planned DTO exports.
- Raw byte commands (peaks, artwork) return `tauri::ipc::Response` through a dispatcher next to the specta handler; TS wrappers in `ui/src/ipc/raw.ts` (ADR 0004).
- Build: `pnpm -C ui tauri build` (wrapper sets `TAURI_APP_PATH`); `beforeBuildCommand` runs `pnpm build` in `ui/`. Bundles: nsis, msi, appimage, deb, dmg.
- Plain `cargo build`/`clippy`/`test` work without `ui/dist` (debug builds use `devUrl`).

## Acceptance per milestone
- M0: 3 commands, 1 event, bindings export and drift test, deb bundle builds on Linux. **Done in wave 0a.**
- M1 onward: every command in `docs/ipc.md` for the milestone, each with a unit test calling the command function against an `AppCore`; telemetry emitter at ~60 Hz.
- M8: release config (updater decision, signing notes), Windows NSIS/MSI from CI.

## Open questions
- CSP may need tightening once asset protocol use (artwork) is known.
- App data directory layout (DB, logs, mappings overrides) per OS.
