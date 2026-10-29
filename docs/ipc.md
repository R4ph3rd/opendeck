# IPC command and event plan

Planned Tauri commands and events for milestones 1 to 8 (ADR 0004). Implemented entries are marked **done**. Types are Rust names; DTOs come from `crates/contracts` unless marked *shell*. Every fallible command returns `Result<T, CommandError>`. Adding or changing a row is a contract change: update this file, the Rust command and the regenerated `ui/src/ipc/bindings.ts` in the same commit.

Milestone plan assumed here (confirm with the project brief):
M1 engine and decks, M2 library and waveforms, M3 analysis and sync, M4 cues, loops and pad modes, M5 effects, M6 MIDI, M7 sampler, saved sets and rekordbox import, M8 settings, layout and release.

## Commands

### App and settings

| Command | Args | Returns | M |
|---|---|---|---|
| `app_info` **done** | | `AppInfo` *shell* | 0 |
| `list_audio_hosts` **done** (stub) | | `Vec<AudioHostInfo>` | 0, real in 1 |
| `list_audio_devices` | `host: AudioHostKind` | `Vec<AudioDeviceInfo>` (name, channels, rates, buffer sizes) | 1 |
| `get_audio_settings` | | `AudioSettings` | 1 |
| `set_audio_settings` | `settings: AudioSettings` | `()`; restarts the stream | 1 |
| `get_setting` | `key: String` | `Option<serde_json::Value>` | 8 |
| `set_setting` | `key: String, value: Value` | `()` | 8 |
| `get_shortcuts` / `set_shortcuts` | `ShortcutMap` | `ShortcutMap` / `()` | 8 |
| `get_layout` / `set_layout` | `LayoutState` | `LayoutState` / `()` | 8 |

### Decks and transport

| Command | Args | Returns | M |
|---|---|---|---|
| `get_deck_state` | `deck: DeckId` | `DeckState` | 1 |
| `load_track` | `deck: DeckId, track: TrackId` | `()`; progress via `deck-state-changed` | 1 |
| `eject_track` | `deck: DeckId` | `()` | 1 |
| `set_playing` | `deck, playing: bool` | `()` | 1 |
| `cue` | `deck, pressed: bool` | `()` | 1 |
| `seek` | `deck, position: Seconds` | `()` | 1 |
| `set_tempo_fader` | `deck, position: f32` | `()` | 1 |
| `set_tempo_range` | `deck, range: TempoRange` | `()` | 1 |
| `set_key_lock` | `deck, on: bool` | `()` | 1 |
| `pitch_bend` | `deck, amount: f32` | `()` | 1 |
| `set_vinyl_mode` | `deck, on: bool` | `()` | 1 |
| `set_sync` | `deck, mode: SyncMode` | `()` | 3 |
| `set_leader` | `deck: Option<DeckId>` | `()` | 3 |
| `set_quantize` | `deck, on: bool` | `()` | 3 |
| `set_key_shift` | `deck, semitones: i8` | `()` | 7 |

### Mixer

| Command | Args | Returns | M |
|---|---|---|---|
| `get_mixer_state` | | `MixerState` | 1 |
| `set_trim` | `deck, gain: Gain` | `()` | 1 |
| `set_eq` | `deck, band: EqBand, gain: Gain` | `()` | 1 |
| `set_filter` | `deck, amount: f32` | `()` | 1 |
| `set_channel_fader` | `deck, position: f32` | `()` | 1 |
| `set_pfl` | `deck, on: bool` | `()` | 1 |
| `set_crossfader` | `position: f32` | `()` | 1 |
| `set_crossfader_curve` | `curve: CrossfaderCurve` | `()` | 1 |
| `set_crossfader_assign` | `deck, assign: CrossfaderAssign` | `()` | 1 |
| `set_master_level` / `set_headphone_level` | `gain: Gain` | `()` | 1 |
| `set_headphone_mix` | `mix: f32` | `()` | 1 |
| `set_split_cue` | `on: bool` | `()` | 1 |

### Library and import

| Command | Args | Returns | M |
|---|---|---|---|
| `query_tracks` | `query: TrackQuery` (text, filters, sort, offset, limit) | `TrackPage` (rows, total) | 2 |
| `get_track` | `track: TrackId` | `TrackDetail` | 2 |
| `update_track` | `track, patch: TrackPatch` (rating, color, comment, tags) | `()` | 2 |
| `remove_tracks` | `tracks: Vec<TrackId>` | `()` | 2 |
| `get_artwork` | `track: TrackId` | raw bytes (`tauri::ipc::Response`) | 2 |
| `get_track_peaks` | `track: TrackId, level: u8` | raw `contracts::peaks` blob (`Response`) | 2 |
| `import_folder` | `path: String, playlist: Option<PlaylistId>` | `JobId` | 2 |
| `import_files` | `paths: Vec<String>, playlist: Option<PlaylistId>` | `JobId` | 2 |
| `import_playlist_file` | `path: String` | `JobId` | 2 |
| `import_rekordbox_xml` | `path: String` | `JobId` | 7 |
| `list_jobs` | | `Vec<JobProgress>` | 2 |
| `pause_job` / `resume_job` / `cancel_job` | `job: JobId` | `()` | 2 |
| `reanalyze_tracks` | `tracks: Vec<TrackId>, features: AnalysisFeatures` | `JobId` | 3 |
| `list_playlists` | | `Vec<PlaylistNode>` (tree) | 2 |
| `create_playlist` | `parent: Option<PlaylistId>, name: String, folder: bool` | `PlaylistId` | 2 |
| `rename_playlist` / `delete_playlist` | `playlist, name?` | `()` | 2 |
| `move_playlist` | `playlist, parent: Option<PlaylistId>, position: u32` | `()` | 2 |
| `get_playlist_tracks` | `playlist: PlaylistId` | `Vec<TrackRow>` | 2 |
| `add_to_playlist` | `playlist, tracks: Vec<TrackId>, position: Option<u32>` | `()` | 2 |
| `remove_from_playlist` / `reorder_playlist` | `playlist, positions` | `()` | 2 |

### Analysis and beat grid

| Command | Args | Returns | M |
|---|---|---|---|
| `get_analysis` | `track: TrackId` | `Option<TrackAnalysis>` (bpm, key, grid, loudness) | 3 |
| `set_beat_grid` | `track, grid: BeatGrid` | `()`; locks the grid | 3 |
| `nudge_beat_grid` | `track, offset: Seconds` | `BeatGrid` | 3 |

### Cues, loops, jumps, pads

| Command | Args | Returns | M |
|---|---|---|---|
| `list_cues` | `track: TrackId` | `Vec<Cue>` | 4 |
| `set_hot_cue` | `deck, index: HotCueIndex` | `Cue` (at playhead) | 4 |
| `trigger_hot_cue` | `deck, index, pressed: bool` | `()` | 4 |
| `update_cue` | `cue: CueId, color: Option<CueColor>, name: String` | `()` | 4 |
| `delete_cue` | `cue: CueId` | `()` | 4 |
| `add_memory_cue` | `deck` | `Cue` | 4 |
| `call_memory_cue` | `deck, direction: i8` | `()` | 4 |
| `loop_in` / `loop_out` / `loop_toggle` | `deck` | `()` | 4 |
| `auto_loop` | `deck, length: Beats` | `()` | 4 |
| `loop_roll` | `deck, length: Beats, active: bool` | `()` | 4 |
| `resize_loop` | `deck, factor: f32` | `()` | 4 |
| `save_loop` | `deck` | `Cue` | 4 |
| `beat_jump` | `deck, beats: Beats` | `()` | 4 |
| `set_slip` | `deck, on: bool` | `()` | 4 |
| `get_pad_mode` **done** | `deck: DeckId` | `PadMode` | 0 |
| `set_pad_mode` | `deck, mode: PadMode` | `()`; emits `pad-mode-changed` | 4 |
| `press_pad` | `deck, index: u8, pressed: bool, shifted: bool` | `()` (interpreted in the active mode) | 4 |

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
| `list_saved_sets` | | `Vec<SavedSetInfo>` | 7 |
| `save_current_set` | `name: String` | `SavedSetId` | 7 |
| `get_saved_set` | `set: SavedSetId` | `SavedSet` (tracks, snapshot) | 7 |
| `delete_saved_set` | `set` | `()` | 7 |

## Events

| Event | Payload | Rate | M |
|---|---|---|---|
| `engine-telemetry` **declared** | `EngineTelemetry` | ~60 Hz | 1 |
| `engine-event` | `EngineEvent` | on change | 1 |
| `deck-state-changed` | `DeckState` | on change | 1 |
| `mixer-state-changed` | `MixerState` | on change (MIDI-driven moves) | 1 |
| `audio-device-changed` | `AudioStatus` (running, device, rate, buffer, error) | on change | 1 |
| `job-progress` | `JobProgress` | <= 4 Hz per job | 2 |
| `library-changed` | `LibraryChange` (tracks or playlists touched) | on change, coalesced | 2 |
| `analysis-ready` | `TrackId` | per track | 3 |
| `cues-changed` | `TrackId` | on change | 4 |
| `pad-mode-changed` | `{ deck: DeckId, mode: PadMode }` | on change | 4 |
| `fx-state-changed` | `FxState` | on change | 5 |
| `midi-message` | `MidiMonitorEntry` (timestamp, bytes, decoded control id, action) | monitor on only, coalesced at 60 Hz | 6 |
| `midi-learned` | `MidiLearnResult` (message, suggested binding) | once | 6 |
| `midi-connection-changed` | `MidiStatus` | on change | 6 |
| `sampler-state-changed` | `SamplerState` | on change | 7 |

Types not yet in contracts (`TrackQuery`, `TrackRow`, `PlaylistNode`, `FxState`, `MidiPorts`, ...) are added to `crates/contracts` by the architect when their milestone starts.
