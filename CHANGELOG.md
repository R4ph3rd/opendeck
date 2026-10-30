# Changelog

All notable changes are recorded here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/). Milestone N releases as v0.N.0.

## [Unreleased]

## [0.5.0] - 2026-10-30

Milestone 5: FX library, FX host, FX UI with presets and chains.

### Added

- FX library (`dsp-fx`): 17 built-in effects (delay, echo, ping pong, spiral, reverb, helix, flanger, phaser, filter, trans, roll, reverse roll, pitch, vinyl brake, noise, bit crusher, colour filter) with catalog data, factory presets, chain presets and a reference slot host. Parameter ranges are `TODO(verify)` against the rekordbox manual.
- FX host in the engine: deck, master and send units of six serial slots each; equal-power wet/dry, 10 ms bypass, tail spill, slot latency compensation, sample-accurate timed engage, Release FX, tapped tempo source, per-unit FX telemetry, no heap calls in the callback.
- FX control model in app-core: two FLX4-style Beat FX units (channel 1, 2 or 1&2, Multi and Single, BEAT stepper, tap BPM, Release FX), Sound Color FX per channel, two send buses (pre or post fader) and 32 pad FX. Each deck chain has dedicated Beat FX, Sound Color FX, pad FX and Release FX slots. State persists in the library settings.
- Presets and chains: user effect presets in `fx_presets`, chain presets per unit, factory presets per effect.
- Tauri FX commands and the `fx-state-changed` event.
- UI: effects panel with Beat FX units and send units, CFX knobs and send levels on the channel strips, pad FX editor, preset and chain menus.

### Changed

- `FX_SLOTS_PER_UNIT` is 6 (contract change), so a three-effect Multi unit no longer disables Sound Color FX, pad FX or Release FX on its channel.

## [0.4.0] - 2026-10-30

Milestone 4: waveforms, deck visual components, design system components, flat re-theme.

### Added

- Waveforms: zoomed and overview waveforms from analysis peaks (`get_track_peaks`) on Canvas 2D, with 3-band colours, beat grid, hot and memory cues, loops, a playhead extrapolated from telemetry, zoom from 2 to 64 s and scrub. Peak decoder and per-track cache.
- Viz components: canvas knob, jog wheel (touch, turn, nudge ring, Shift search), performance pads and stereo level meter with peak hold and clip lamp, wired into the deck and mixer panels. Viz Storybook with synthetic tracks, a perf story and a two-deck layout story.
- Design system: resizable, popover, command, number field, status dot, segmented control, settings layout, breadcrumb, callout, key-value, skeleton, progress bar and colour swatch picker, with stories and pattern docs; info, blink and sweep tokens with contrast tests.

### Changed

- FL-style flat re-theme on a slate palette. No glass: gradients, mesh backgrounds and backdrop blur are removed, and stylelint and ESLint reject `backdrop-filter` and blur.

## [0.3.0] - 2026-10-30

Milestone 3: library, database, import pipeline, analysis, collection view.

### Added

- `analysis`: tempo and beat grid (spectral-flux onsets, comb filter, octave correction), key detection (chroma + Krumhansl-Schmuckler, Camelot), 3-band waveform peaks at several zoom levels, EBU R128 loudness. stratum-dsp was evaluated and rejected on accuracy.
- `library`: SQLite schema v2 (playlist entries with their own ids, folder guards, FTS5 search, indexed sort columns, backup on upgrade); tracks, analysis, cues, playlists, folders, saved sets, settings.
- Import pipeline: folder, files, M3U/M3U8 and rekordbox XML; dedupe by path and content fingerprint; background workers with progress, pause, cancel and resume after restart.
- Collection view: virtualized table with sorting, filters, search, multi-select, context menu and drag to deck; playlists and folders tree, saved sets, in-app file explorer; import progress panel.
- Decks load from the library with beat grid, BPM and key; cues persist in the database.

### Fixed

- Windows CI: app manifest embedded in test binaries, LF checkouts, platform-neutral path tests.
- Licence check allows BSD-1-Clause (allocation guard used in tests).

## [0.2.0] - 2026-10-29

Milestone 2: two decks and the mixer, with cue routing, sync, loops and hot cues.

### Added

- `engine`: second deck with jog scratch and nudge, tempo and beat sync with leader election, quantize, loops with click-free wraps, loop roll, 8 hot cues, beat jump and slip. Tracks carry an optional beat grid (`load_track_with_grid`, `set_beat_grid`).
- `engine`: mixer with trim, 3-band isolator EQ with kill, one-knob filter, channel fader, PFL, crossfader with constant power, linear and sharp curves, master level, and a headphone bus with cue/master blend and split cue on the headphone pair.
- Telemetry: channel, master and headphone meters, beat phase, and per deck sync mode, leader and loop state.
- `app-core`: deck, mixer, loop and cue control; hot cues and memory cues in memory, re-sent after every load; `render_offline` for headless audio tests.
- Tauri commands for decks, loops, cues and the mixer.
- UI: deck B, mixer strip, loop controls, hot cue pads, SYNC/MASTER/QUANT/SLIP, BPM readout, two-deck keyboard shortcuts.
- Design system: Toaster with an imperative toast API; slider thumb labels, track length and centre detent.
- Offline tests through the engine and through app-core proving EQ kill, crossfader, PFL, loops and beat sync change the audio.

### Changed

- `DeckTelemetry::active_loop` is replaced by `loop_region` and `loop_active`.

### Fixed

- `DeckState::track_bpm` reports the grid tempo, not the effective tempo.
- The version test checks the semver shape instead of a hard-coded value.

## [0.1.0] - 2026-10-29

Milestone 1: audio engine. One deck plays a file on a chosen audio device.

### Added

- `media`: decode MP3, AAC/M4A, ALAC, FLAC, WAV, AIFF and Ogg Vorbis with symphonia, resample to the engine rate with rubato, read tags and artwork with lofty.
- `engine`: real-time deck with play/pause, CDJ-style CUE, seek, tempo fader with ranges, pitch bend and key lock (signalsmith-stretch); cpal device layer with host/device listing, 4-channel master/headphone routing with stereo fallback, optional `asio` feature; offline renderer; telemetry at 60 Hz.
- Real-time safety tests: callback runs under `assert_no_alloc`; a Linux test counts C and C++ heap calls on the key lock path.
- `app-core`: control and loader threads, audio settings persisted as JSON, headless mode via `OPENDECK_OFFLINE_AUDIO=1`.
- Tauri commands and events for audio settings, track loading and deck transport.
- UI: app layout, deck A panel (transport, tempo, key lock, pitch bend, click-to-seek progress), audio settings dialog, toasts, keyboard shortcuts.

### Fixed

- Key lock start no longer spikes CPU: stretcher priming is spread over a fixed 30 ms warm-up.
- macOS release bundling: the bindings exporter is an example, not an extra binary.
- Release workflow replaces a stale draft from a failed run.

## [0.0.0] - 2026-10-29

Milestone 0: architecture, contracts, skeleton, CI and design tokens.

### Added

- Architecture decision records 0001 to 0007: stack, crate boundaries, real-time rules, IPC, DB schema v1, FX trait, MIDI mapping format.
- Cargo workspace with crates `contracts`, `media`, `engine`, `dsp-fx`, `analysis`, `library`, `midi`, `app-core`.
- Contracts crate with the cross-crate types and traits.
- Library schema v1 migration with in-memory tests.
- MIDI mapping JSON schema with a drift test against the contracts.
- Tauri 2 shell with typed commands and generated TypeScript bindings.
- Vite and React UI shell calling `app_info` through the bindings.
- Design system package: tokens, Tailwind v4 theme, shadcn/Radix components, Storybook, and stylelint and ESLint rules that reject raw style values.
- CI workflow (Rust, UI, Windows build, commitlint, version sync, cargo-deny) and a release workflow that builds draft releases for Windows, macOS and Linux.
- `scripts/bump-version.mjs`, `scripts/check-version-sync.mjs` and `scripts/release-notes.mjs`.
- Build, status, release, hardware test and MIDI open-question documents.

### Fixed

- MIDI mapping modifier keys are restricted to identifiers.
- Analysis BPM and key are indexed for collection sorting.
