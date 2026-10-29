# ADR 0002: Workspace and crate boundaries

- Status: accepted
- Date: 2026-10-29
- Deciders: architect

## Context

ADR 0001 fixed the stack and added two crates (`contracts`, `media`) to break dependency cycles. Agents work in parallel on separate crates, so every boundary must be explicit and enforced by Cargo, not by convention.

## Decision

One Cargo workspace at the repo root. Shared metadata (`edition = 2024`, `rust-version = 1.97`, `license-file`, `version = 0.0.0`) and all external versions live in the root `Cargo.toml` (`[workspace.package]`, `[workspace.dependencies]`). Members use `workspace = true`; a crate adds an external dependency to its own manifest only when its code uses it.

### Module map

| Crate | Package | May depend on | Owner |
|---|---|---|---|
| `crates/contracts` | `opendeck-contracts` | nothing internal (serde, specta optional) | architect |
| `crates/media` | `opendeck-media` | contracts | analysis-dev |
| `crates/engine` | `opendeck-engine` | contracts, media | engine-dev |
| `crates/dsp-fx` | `opendeck-dsp-fx` | contracts | dsp-fx-dev |
| `crates/analysis` | `opendeck-analysis` | contracts, media | analysis-dev |
| `crates/library` | `opendeck-library` | contracts, media, analysis | library-dev |
| `crates/midi` | `opendeck-midi` | contracts | midi-dev |
| `crates/app-core` | `opendeck-app-core` | all crates above | architect, then ui-shell-dev for wiring |
| `src-tauri` | `opendeck-desktop` | app-core, contracts (`specta` feature) | ui-shell-dev |

```
contracts  <-  media  <-  engine
    ^   ^        ^   \
    |   |        |    analysis  <-  library
    |   dsp-fx   |
    |   midi     |
    +--------- app-core (everything)  <-  src-tauri
```

Rules:

1. `contracts` is the only crate every other crate may see. Cross-crate types and traits go there and nowhere else.
2. The engine never depends on `dsp-fx`, `midi`, `library` or `app-core`. Effects reach it as `Box<dyn Effect>`; commands as `EngineCommand`.
3. `midi` never depends on the engine: it emits `ControlEvent`s; app-core turns them into `EngineCommand`s.
4. `library` may call `media` and `analysis` for the import pipeline; it never touches the engine.
5. `app-core` is the only crate that wires crates together. `src-tauri` is a thin IPC adapter over `app-core`.
6. A future plugin host (`crates/plugin-host`, VST3/CLAP) sits at the level of `dsp-fx`: depends on `contracts` only (ADR 0006).

### Features of `contracts`

- `serde`: serde derives for persisted, mapping-file and IPC types.
- `specta`: `specta::Type` derives for IPC DTOs; implies `serde`. Only `src-tauri` enables it.

The engine builds `contracts` with no features.

### Lints

Workspace lints: `unsafe_code = "deny"`, `missing_docs = "warn"` (an error under `clippy -D warnings`). A crate that needs `unsafe` (ASIO, FFI) opts out locally with a comment and a safety review.

## Consequences

- Cycles are impossible by construction; adding an edge not in the table is an ADR change.
- `contracts` changes ripple to every crate, so they go through the architect (ADR 0001).
- `cargo deny` (CI, docs-ci) checks licences; a dependency-graph check against this table is a follow-up for docs-ci.
