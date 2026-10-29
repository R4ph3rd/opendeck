# IPC command and event plan

Planned Tauri commands and events for milestones 1 to 8 (ADR 0004). Implemented entries are marked **done**. Types are Rust names; DTOs come from `crates/contracts` unless marked *shell*. Every fallible command returns `Result<T, CommandError>` (`internal`, `audio`, `media`, `invalid`; `invalid` covers a non-finite or out-of-range argument and cue actions on an empty deck). Adding or changing a row is a contract change: update this file, the Rust command and the regenerated `ui/src/ipc/bindings.ts` in the same commit.

The `M` column follows the milestone table in `docs/STATUS.md`: M1 engine with one deck and device selection, M2 two decks, mixer, cue routing, sync, loops and hot cues, M3 library, DB, import, analysis and collection view, M4 waveforms and deck UI, M5 FX, M6 MIDI, M7 pad modes and sampler, M8 hardening and settings.

## Commands

### App and settings

| Command | Args | Returns | M |
|---|---|---|---|
| `app_info` **done** | | `AppInfo` *shell* | 0 |
| `list_audio_hosts` **done** | | `Vec<AudioHostInfo>` (cpal hosts the settings enum models) | 1 |
| `list_audio_devices` **done** | `host: Option<AudioHostKind>` (`None` = platform default) | `Vec<AudioDeviceInfo>` *shell* (name, channels, rate ranges, buffer range) | 1 |
| `get_audio_settings` **done** | | `AudioSettings` | 1 |
| `get_audio_status` **done** | | `AudioStatus` *shell* (running, device, rate, buffer, latency, routing, error) | 1 |
| `supported_audio_extensions` **done** | | `Vec<String>` (decoder extensions, for the file picker) | 1 |
| `set_audio_settings` **done** | `settings: AudioSettings` | `AudioStatus`; restarts the stream, reloads loaded tracks at the new rate, persists to `settings.json` in the app config dir (SQLite in M3) | 1 |
| `get_setting` | `key: String` | `Option<serde_json::Value>` | 8 |
| `set_setting` | `key: String, value: Value` | `()` | 8 |
| `get_shortcuts` / `set_shortcuts` | `ShortcutMap` | `ShortcutMap` / `()` | 8 |
| `get_layout` / `set_layout` | `LayoutState` | `LayoutState` / `()` | 8 |

### Decks and transport

| Command | Args | Returns | M |
|---|---|---|---|
| `get_deck_state` **done** | `deck: DeckId` | `DeckState`; control state only (tempo, sync, quantize, slip, vinyl, leader, loop, pad mode). `track`, `cue_point`, `beat_grid`, `track_key` stay `None` until M3 | 2 |
| `load_track` **done** (path form) | `deck: DeckId, path: String` | `TrackInfo` *shell* (tags, read synchronously); decode runs on the loader thread, then `track-loaded` or `track-load-failed`. Becomes `track: TrackId` when the library lands (M3, contract change) | 1 |
| `eject_track` **done** | `deck: DeckId` | `()` | 1 |
| `set_playing` **done** | `deck, playing: bool` | `()` | 1 |
| `toggle_playing` **done** | `deck` | `()`; flips on the control thread's newest telemetry (space bar, PLAY) | 1 |
| `cue` **done** | `deck, pressed: bool` | `()` | 1 |
| `seek` **done** | `deck, position: Seconds` | `()` | 1 |
| `set_tempo_fader` **done** | `deck, position: f32` | `()` | 1 |
| `set_tempo_range` **done** | `deck, range: TempoRange` | `()` | 1 |
| `set_key_lock` **done** | `deck, on: bool` | `()` | 1 |
| `pitch_bend` **done** | `deck, amount: f32` | `()`; `0` releases | 1 |
| `set_vinyl_mode` **done** | `deck, on: bool` | `()` | 2 |
| `jog_touch` **done** | `deck, touched: bool` | `()`; platter touched or released | 2 |
| `jog_turn` **done** | `deck, revolutions: f32` | `()`; signed revolutions since the last call | 2 |
| `jog_search` **done** | `deck, beats: Beats` | `()`; SHIFT + jog fast search | 2 |
| `set_sync` **done** | `deck, mode: SyncMode` | `()` | 2 |
| `set_leader` **done** | `deck: Option<DeckId>` | `()` | 2 |
| `set_quantize` **done** | `deck, on: bool` | `()` | 2 |
| `set_key_shift` | `deck, semitones: i8` | `()` | 7 |

### Mixer

| Command | Args | Returns | M |
|---|---|---|---|
| `get_mixer_state` **done** | | `MixerState`; what app-core last sent to the engine, replayed after an engine restart | 2 |
| `set_trim` **done** | `deck, gain: Gain` | `()` | 2 |
| `set_eq` **done** | `deck, band: EqBand, gain: Gain` | `()` | 2 |
| `set_filter` **done** | `deck, amount: f32` | `()` | 2 |
| `set_channel_fader` **done** | `deck, position: f32` | `()` | 2 |
| `set_pfl` **done** | `deck, on: bool` | `()` | 2 |
| `set_crossfader` **done** | `position: f32` | `()` | 2 |
| `set_crossfader_curve` **done** | `curve: CrossfaderCurve` | `()` | 2 |
| `set_crossfader_assign` **done** | `deck, assign: CrossfaderAssign` | `()` | 2 |
| `set_master_level` / `set_headphone_level` **done** | `gain: Gain` | `()` | 2 |
| `set_headphone_mix` **done** | `mix: f32` | `()` | 2 |
| `set_split_cue` **done** | `on: bool` | `()` | 2 |

### Library and import

| Command | Args | Returns | M |
|---|---|---|---|
| `query_tracks` | `query: TrackQuery` (text, filters, sort, offset, limit) | `TrackPage` (rows, total) | 3 |
| `get_track` | `track: TrackId` | `TrackDetail` | 3 |
| `update_track` | `track, patch: TrackPatch` (rating, color, comment, tags) | `()` | 3 |
| `remove_tracks` | `tracks: Vec<TrackId>` | `()` | 3 |
| `get_artwork` | `track: TrackId` | raw bytes (`tauri::ipc::Response`) | 3 |
| `get_track_peaks` | `track: TrackId, level: u8` | raw `contracts::peaks` blob (`Response`) | 4 |
| `import_folder` | `path: String, playlist: Option<PlaylistId>` | `JobId` | 3 |
| `import_files` | `paths: Vec<String>, playlist: Option<PlaylistId>` | `JobId` | 3 |
| `import_playlist_file` | `path: String` | `JobId` | 3 |
| `import_rekordbox_xml` | `path: String` | `JobId` | 3 |
| `list_jobs` | | `Vec<JobProgress>` | 3 |
| `pause_job` / `resume_job` / `cancel_job` | `job: JobId` | `()` | 3 |
| `reanalyze_tracks` | `tracks: Vec<TrackId>, features: AnalysisFeatures` | `JobId` | 3 |
| `list_playlists` | | `Vec<PlaylistNode>` (tree) | 3 |
| `create_playlist` | `parent: Option<PlaylistId>, name: String, folder: bool` | `PlaylistId` | 3 |
| `rename_playlist` / `delete_playlist` | `playlist, name?` | `()` | 3 |
| `move_playlist` | `playlist, parent: Option<PlaylistId>, position: u32` | `()` | 3 |
| `get_playlist_tracks` | `playlist: PlaylistId` | `Vec<TrackRow>` | 3 |
| `add_to_playlist` | `playlist, tracks: Vec<TrackId>, position: Option<u32>` | `()` | 3 |
| `remove_from_playlist` / `reorder_playlist` | `playlist, positions` | `()` | 3 |

### Analysis and beat grid

| Command | Args | Returns | M |
|---|---|---|---|
| `get_analysis` | `track: TrackId` | `Option<TrackAnalysis>` (bpm, key, grid, loudness) | 3 |
| `set_beat_grid` | `track, grid: BeatGrid` | `()`; locks the grid | 3 |
| `nudge_beat_grid` | `track, offset: Seconds` | `BeatGrid` | 3 |

### Cues, loops, jumps, pads

| Command | Args | Returns | M |
|---|---|---|---|
| `list_cues` | `track: TrackId` | `Vec<Cue>` | 3 |
| `set_hot_cue` **done** | `deck, index: HotCueIndex` | `Cue` (at playhead); in memory until persistence (M3), fails with `invalid` when no track is loaded | 2 |
| `trigger_hot_cue` **done** | `deck, index, pressed: bool` | `()` | 2 |
| `get_hot_cues` **done** | `deck` | `HotCues` *shell* (deck, 8 slots); not in the original plan, hydrates the UI | 2 |
| `delete_hot_cue` **done** | `deck, index: HotCueIndex` | `()`; replaces `delete_cue` until cues have ids (M3) | 2 |
| `update_cue` | `cue: CueId, color: Option<CueColor>, name: String` | `()` | 4 |
| `delete_cue` | `cue: CueId` | `()` | 3 |
| `add_memory_cue` **done** | `deck` | `Cue` (at playhead, in memory) | 2 |
| `call_memory_cue` **done** | `deck, direction: i8` | `()`; sets the cue point and seeks to the next (`>= 0`) or previous memory cue | 2 |
| `loop_in` / `loop_out` / `loop_toggle` **done** | `deck` | `()` | 2 |
| `auto_loop` **done** | `deck, length: Beats` | `()` | 2 |
| `loop_roll` **done** | `deck, length: Beats, active: bool` | `()` | 2 |
| `resize_loop` **done** | `deck, factor: f32` | `()` | 2 |
| `save_loop` | `deck` | `Cue` | 2 |
| `beat_jump` **done** | `deck, beats: Beats` | `()`; `0` is refused | 2 |
| `set_slip` **done** | `deck, on: bool` | `()` | 2 |
| `get_pad_mode` **done** | `deck: DeckId` | `PadMode` | 0 |
| `set_pad_mode` | `deck, mode: PadMode` | `()`; emits `pad-mode-changed` | 7 |
| `press_pad` | `deck, index: u8, pressed: bool, shifted: bool` | `()` (interpreted in the active mode) | 7 |

### Effects

| Command | Args | Returns | M |
|---|---|---|---|
| `list_effects` | | `Vec<EffectInfo>` | 5 |
| `get_fx_state` | | `FxState` (per slot: effect key, enabled, wet, division, params) | 5 |
| `load_effect` | `slot: FxSlotId, effect: Option<EffectKey>` | `()` | 5 |
| `set_fx_enabled` | `slot, on: bool` | `()` | 5 |
| `set_fx_param` | `slot, param: ParamId, value: f32` | `()` | 5 |
| `set_fx_mix` | `slot, wet: f32` | `()` | 5 |
| `set_fx_beat_division` | `slot, beats: Beats` | `()` | 5 |
| `set_fx_send` | `deck, bus: u8, level: Gain` | `()` | 5 |
| `list_fx_presets` | `effect: EffectKey` | `Vec<FxPreset>` | 5 |
| `save_fx_preset` / `delete_fx_preset` | `preset: FxPreset` / `effect, name` | `()` | 5 |
| `apply_fx_preset` | `slot, name: String` | `()` | 5 |

### MIDI

| Command | Args | Returns | M |
|---|---|---|---|
| `list_midi_ports` | | `MidiPorts` (inputs, outputs) | 6 |
| `connect_midi` | `input: String, output: Option<String>, mapping: String` | `()` | 6 |
| `disconnect_midi` | | `()` | 6 |
| `list_mappings` | | `Vec<MappingInfo>` (id, version, name, verify count) | 6 |
| `set_midi_monitor` | `on: bool` | `()`; streams `midi-message` | 6 |
| `start_midi_learn` | `control_id: String` | `()`; next message emits `midi-learned` | 6 |
| `cancel_midi_learn` | | `()` | 6 |
| `set_mapping_override` | `mapping: String, control_id: String, binding: Option<Value>` | `()` | 6 |
| `clear_mapping_overrides` | `mapping: String` | `()` | 6 |

### Sampler and saved sets

| Command | Args | Returns | M |
|---|---|---|---|
| `load_sampler_slot` | `slot: SamplerSlotId, track: TrackId` | `()` | 7 |
| `trigger_sampler` | `slot, pressed: bool` | `()` | 7 |
| `stop_sampler` | `slot` | `()` | 7 |
| `set_sampler_level` | `gain: Gain` | `()` | 7 |
| `list_saved_sets` | | `Vec<SavedSetInfo>` | 3 |
| `save_current_set` | `name: String` | `SavedSetId` | 3 |
| `get_saved_set` | `set: SavedSetId` | `SavedSet` (tracks, snapshot) | 3 |
| `delete_saved_set` | `set` | `()` | 3 |

## Events

| Event | Payload | Rate | M |
|---|---|---|---|
| `engine-telemetry` **done** | `EngineTelemetry` | ~60 Hz | 1 |
| `engine-event` **done** | `EngineEvent` (audio thread events plus `device_lost`, `device_changed`, `stream_error`) | on change | 1 |
| `track-loaded` **done** | `TrackLoaded` *shell* (deck, `TrackInfo`, frames) | once per load | 1 |
| `track-load-failed` **done** | `TrackLoadFailed` *shell* (deck, path, message) | once per failed load | 1 |
| `deck-state-changed` | `DeckState` | on change | 1 |
| `mixer-state-changed` | `MixerState` | on change (MIDI-driven moves) | 2 |
| `audio-device-changed` **done** | `AudioStatus` (running, device, rate, buffer, error) | on change | 1 |
| `job-progress` | `JobProgress` | <= 4 Hz per job | 3 |
| `library-changed` | `LibraryChange` (tracks or playlists touched) | on change, coalesced | 3 |
| `analysis-ready` | `TrackId` | per track | 3 |
| `hot-cues-changed` **done** | `HotCues` *shell* (deck, 8 slots) | on change | 2 |
| `cues-changed` | `TrackId` | on change | 3 |
| `pad-mode-changed` | `{ deck: DeckId, mode: PadMode }` | on change | 7 |
| `fx-state-changed` | `FxState` | on change | 5 |
| `midi-message` | `MidiMonitorEntry` (timestamp, bytes, decoded control id, action) | monitor on only, coalesced at 60 Hz | 6 |
| `midi-learned` | `MidiLearnResult` (message, suggested binding) | once | 6 |
| `midi-connection-changed` | `MidiStatus` | on change | 6 |
| `sampler-state-changed` | `SamplerState` | on change | 7 |

Mixer values cross IPC as linear `Gain` (trim, EQ band, master, headphone) or normalised positions (`filter` in `[-1, 1]`, channel fader and headphone mix in `[0, 1]`, crossfader in `[-1, 1]`). The UI owns the knob-to-gain curves (`ui/src/lib/mixer.ts`, constants mirror `contracts::mixer`); app-core validates ranges and the engine clamps.

Types not yet in contracts (`TrackQuery`, `TrackRow`, `PlaylistNode`, `FxState`, `MidiPorts`, ...) are added to `crates/contracts` by the architect when their milestone starts.
