# ADR 0001: Technology stack

- Status: proposed (frozen on approval)
- Date: 2026-10-29
- Deciders: project owner, architect

## Context

OpenDeck is a desktop DJ application with a real-time audio engine, a large track library, MIDI controller support (DDJ-FLX4 first) and a dense, design-system-driven UI. Primary target is Windows x64; macOS and Linux builds ship from CI. The audio path must be real-time safe: no allocation, locks, I/O, logging or panics in the audio callback.

Versions below are the latest stable on crates.io / npm on the ADR date. Exact versions are pinned in `Cargo.lock` and `pnpm-lock.yaml`.

## Decision

### Shell and IPC

| Choice | Version | Licence | Notes |
|---|---|---|---|
| Tauri | 2.12.x | MIT / Apache-2.0 | Stay on 2.x. 3.0 is alpha, not used. |
| tauri-specta + specta | 2.0.0-rc (pinned exact) | MIT | Generates TS types **and** typed command/event wrappers. Still RC upstream, so the version is pinned with `=` and bumped deliberately. Fallback if it blocks us: `ts-rs` 12 (types only) plus a thin generated command layer. |
| Binary IPC | `tauri::ipc::Response` raw bytes | | Waveform peaks and meters are sent as binary, never JSON arrays. |

### Audio engine

| Concern | Crate | Version | Licence | Notes |
|---|---|---|---|---|
| Audio I/O | `cpal` | 0.18 | Apache-2.0 | WASAPI default on Windows, `asio` feature behind a cargo feature flag (Steinberg SDK not in repo). CoreAudio on macOS, ALSA on Linux, JACK behind a feature. |
| Decoding | `symphonia` | 0.6 | MPL-2.0 | File-level copyleft. Used unmodified as a dependency, compatible with any project licence. Formats: MP3, AAC/M4A, ALAC, FLAC, WAV, AIFF, Ogg Vorbis. |
| Resampling | `rubato` | 5.x | MIT | File rate to engine rate at load time; async resampler reserved for device clock drift. |
| Time-stretch / key lock | `signalsmith-stretch` | 0.1.x | MIT (wrapper and C++ library) | Builds C++ via `cc`, works with MSVC. **SoundTouch rejected**: it is LGPL-2.1, which complicates static linking. Rubber Band rejected (GPL / commercial). |
| Lock-free queues | `rtrb` | 0.4 | MIT / Apache-2.0 | SPSC command and telemetry rings. Chosen over `ringbuf` for a smaller, RT-focused API. |
| RT allocation guard (tests) | `assert_no_alloc` | 1.x | BSD-2-Clause | **Addition.** Wraps the callback in debug and test builds and aborts on allocation. Gives the checker an automated RT-safety proof. |
| Atomics | `atomic_float` | 1.x | MIT / Apache-2.0 | **Addition.** Lock-free parameter values shared with the UI side. |

### Analysis

| Concern | Choice | Licence | Notes |
|---|---|---|---|
| FFT | `rustfft` 6.4 | MIT / Apache-2.0 | |
| Loudness | `ebur128` 0.1 | MIT | EBU R128 integrated loudness and true peak. |
| BPM, beat grid, key | `stratum-dsp` 1.0 evaluated first | MIT / Apache-2.0 | Young crate (low download count). Milestone 3 starts with a spike: run it against a labelled test set (BPM within 0.1, key exact or relative). If it fails the bar, fall back to the custom implementation already specified in the brief: onset envelope + autocorrelation/comb filter for tempo, chroma + Krumhansl-Schmuckler for key, Camelot output. Both sit behind one `Analyzer` trait, so the swap is local. |
| Rejected | `aubio-rs` | GPL-3.0 | Not linked. |

### Library, metadata, MIDI

| Concern | Crate | Version | Licence | Notes |
|---|---|---|---|---|
| DB | `rusqlite` (bundled SQLite) | 0.41 | MIT | WAL mode, `synchronous=NORMAL`, foreign keys on. |
| Migrations | `rusqlite_migration` | 2.6 | Apache-2.0 | Uses `user_version`, embeds SQL files. Chosen over `refinery` for zero extra tables and a smaller footprint. |
| Tags and artwork | `lofty` | 0.25 | MIT / Apache-2.0 | |
| Content hash | `blake3` | 1.x | CC0 / Apache-2.0 | **Addition.** Dedupe key: file size plus BLAKE3 of the first and last 1 MiB. Fast on large imports; a full-file hash runs only on collision. |
| Rekordbox XML import | `quick-xml` | 0.x | MIT | **Addition.** Documented XML export only. |
| Job pool | `rayon` (analysis) + `crossbeam-channel` (job queue) | | MIT / Apache-2.0 | **Addition.** Worker pool sized to cores minus one, leaving a core for audio. |
| MIDI | `midir` | 0.11 | MIT | WinMM on Windows, CoreMIDI, ALSA. |
| Serialization | `serde`, `serde_json` | 1.x | MIT / Apache-2.0 | Mapping files, presets, settings. |
| Errors / logging | `thiserror`, `tracing` | | MIT | `tracing` is never called from the audio thread. |

### Frontend

| Concern | Choice | Notes |
|---|---|---|
| Framework | React 19 + TypeScript (strict) + Vite | |
| Styling | Tailwind CSS v4 (`@theme`) | Tokens from `ui/packages/design-system`. |
| Components | shadcn/ui on Radix primitives | Copied into the design-system package, restyled with tokens. |
| Animation | Motion | Respect `prefers-reduced-motion`. |
| State | Zustand | Engine telemetry goes to a separate store updated per animation frame, never through React state per sample. |
| Tables | TanStack Table + TanStack Virtual | |
| Custom viz | Canvas 2D for knobs, jog, pads; WebGL2 for waveforms | |
| Docs | Storybook 9 (React + Vite) | |
| Lint | ESLint (flat config), Stylelint, Prettier | No raw hex or magic numbers outside tokens: `stylelint-declaration-strict-value` + a custom ESLint rule for TS/TSX. |
| Tests | Vitest + Testing Library; Playwright for a smoke test later | |
| Package manager | pnpm 10 workspaces | |

### Tooling

- Rust edition 2024, toolchain pinned in `rust-toolchain.toml` (stable, MSVC on Windows).
- Commit rules: commitlint (conventional) + lefthook (`commit-msg`, `pre-commit` fmt/lint on staged files).
- CI: GitHub Actions `ci.yml` and `release.yml` as specified in the brief.

## Deviations from the brief and why

1. **Two extra crates.** `crates/contracts` holds every cross-crate type and trait (IDs, `Effect`, `Analyzer`, engine commands and telemetry, pad modes). Without it, `engine`, `dsp-fx`, `midi` and `app-core` would depend on each other in cycles, and the future VST3/CLAP host would have to depend on built-in effects. `crates/media` holds decoding, resampling and tag reading, shared by the engine loader and the analysis pipeline.
2. **Tauri 2, not 3.** 3.0 exists only as an alpha.
3. **SoundTouch excluded** on licence grounds (LGPL), not just Rubber Band.
4. **Tracks are decoded fully into RAM** (f32, engine rate) on a loader thread before play, as most DJ software does. About 110 MB per 5-minute stereo track. Gives sample-exact seeking, scratching and reverse with no disk I/O on the RT path. Buffers are freed off the audio thread through a return ring.
5. **Single multichannel device is the default routing.** The FLX4 exposes one 4-channel output (1/2 master, 3/4 headphones). Master and cue to two *different* devices needs clock-drift compensation and is a later option, behind the async resampler.
6. **Windows installer cannot be built in the agent environment** (Linux container). The checker gate runs `pnpm tauri build` on Linux (AppImage/.deb) locally; the Windows NSIS/MSI build is proven by the `ci.yml` Windows job and `release.yml`. A `windows-latest` job running `cargo test` and `tauri build --no-bundle` is added to `ci.yml` because Windows is the primary target.
7. **macOS x64.** GitHub retired Intel macOS runners, so macOS ships one universal dmg (`universal-apple-darwin`) built on `macos-latest`.
8. **No `gh` CLI auth in the agent environment.** The checker watches workflow runs through the GitHub API tools instead of `gh run watch`.

## Consequences

- The `contracts` crate is the change-control point: any edit to it is a contract change and goes through the architect.
- tauri-specta RC pinning means occasional manual upgrades.
- ASIO users build locally with the SDK; release binaries are WASAPI only on Windows.
