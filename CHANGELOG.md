# Changelog

All notable changes are recorded here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/). Milestone N releases as v0.N.0.

## [Unreleased]

Milestone 0, in progress.

### Added

- Architecture decision records 0001 to 0007: stack, crate boundaries, real-time rules, IPC, DB schema v1, FX trait, MIDI mapping format.
- Cargo workspace with crates `contracts`, `media`, `engine`, `dsp-fx`, `analysis`, `library`, `midi`, `app-core`.
- Contracts crate with the cross-crate types and traits.
- Library schema v1 migration with in-memory tests.
- MIDI mapping JSON schema with a drift test against the contracts.
- Tauri 2 shell with typed commands and generated TypeScript bindings.
- Vite and React UI shell calling `app_info` through the bindings.
- CI workflow (Rust, UI, Windows build, commitlint, version sync, cargo-deny) and a release workflow that builds draft releases for Windows, macOS and Linux.
- `scripts/bump-version.mjs`, `scripts/check-version-sync.mjs` and `scripts/release-notes.mjs`.
- Build, status, release, hardware test and MIDI open-question documents.

### Fixed

- MIDI mapping modifier keys are restricted to identifiers.
- Analysis BPM and key are indexed for collection sorting.
