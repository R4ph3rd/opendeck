# OpenDeck

OpenDeck is a desktop DJ application: a real-time audio engine, a track library with analysis, effects, and MIDI controller support. It supports the Pioneer DJ DDJ-FLX4 controller and imports rekordbox XML exports.

Windows x64 is the primary target. macOS and Linux builds come from CI.

## Status

MVP. Two decks with sync, loops, hot cues and key lock, a mixer with three-band EQ, filter and headphone cue, FX, sampler and pad modes, a library with analysis and rekordbox XML import, DDJ-FLX4 mapping with LED feedback, and full audio routing (master, headphones, booth, mic, aux, talk-over). See [docs/STATUS.md](docs/STATUS.md) for what is done and what comes next.

## Build

See [docs/BUILD.md](docs/BUILD.md). In short: Rust (pinned in `rust-toolchain.toml`), Node 22, pnpm 10, then `pnpm install && pnpm -C ui install && pnpm -C ui tauri build`.

## Contributing

Work happens on `feat/*` branches off `workbench`. Commits follow Conventional Commits and are checked by commitlint. Architecture decisions are in [docs/adr](docs/adr). Release process: [docs/RELEASING.md](docs/RELEASING.md).

## Licence

PolyForm Noncommercial 1.0.0, plus an additional permission that allows commercial use of unmodified copies (for example, performing as a paid DJ). The full terms are in [LICENSE](LICENSE). Dependencies are permissive or used unmodified under MPL-2.0; `cargo deny` enforces this.
