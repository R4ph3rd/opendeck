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
| `set_audio_settings` **done** | `settings: AudioSettings` | `AudioStatus`; restarts the stream, reloads loaded tracks at the new rate, persists to the library's `settings` table (key `app.settings`); a `settings.json` from an older install is migrated on the first start | 1 |
| `get_setting` | `key: String` | `Option<serde_json::Value>` | 8 |
| `set_setting` | `key: String, value: Value` | `()` | 8 |
| `get_shortcuts` / `set_shortcuts` | `ShortcutMap` | `ShortcutMap` / `()` | 8 |
| `get_layout` / `set_layout` | `LayoutState` | `LayoutState` / `()` | 8 |

### Decks and transport

| Command | Args | Returns | M |
|---|---|---|---|
| `get_deck_state` **done** | `deck: DeckId` | `DeckState`; control state only (tempo, sync, quantize, slip, vinyl, leader, loop, pad mode). `track`, `cue_point`, `beat_grid`, `track_key` stay `None`: a deck's track, BPM and key come from `TrackInfo` (`track-loaded`) | 2 |
| `load_track` **done** | `deck: DeckId, track: TrackId` | `TrackInfo` *shell* (library tags, `id`, analysed `bpm`, `key`, `key_musical`); the stored beat grid, hot cues and memory cues go with the load, the track counts as played and joins the session's saved set; decode runs on the loader thread, then `track-loaded` or `track-load-failed` | 3 |
| `load_track_path` **done** | `deck: DeckId, path: String` | `TrackInfo`; for files not yet imported (drag from the file explorer): imports the file, then loads it by id. A file the importer refuses loads straight from the path, without analysis. The grid reaches the deck with `analysis-ready` when analysis finishes after the load | 3 |
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

DTOs live in `crates/app-core/src/dto.rs` (plain copies of the `opendeck-library` types with a `Dto` suffix) and are exported with the commands. A library command fails with `internal` when the database fails and with `invalid` for a row that does not exist or a rule the library enforces (playlists only nest in folders, no cycles, rating 0 to 5).

| Command | Args | Returns | M |
|---|---|---|---|
| `query_tracks` **done** | `query: TrackQueryDto` (text, BPM range, keys, genre, min rating, status, sort, descending, offset, limit) | `TrackPageDto` (rows with BPM, key in Camelot and musical notation, comment; total) | 3 |
| `get_track` **done** | `track: TrackId` | `TrackDetailDto` | 3 |
| `update_track_meta` **done** | `track, patch: TrackMetaDto` (`None` keeps a field, an empty string clears it) | `()` | 3 |
| `set_rating` **done** | `track, rating: u8` (0 to 5) | `()` | 3 |
| `delete_tracks` **done** | `tracks: Vec<TrackId>` | rows removed; the files stay on disk | 3 |
| `get_artwork` **done** | `track: TrackId` | raw bytes, empty when the track has none. `ArtworkBytes` is a binary payload on the wire and typed `number[]` in the bindings: read it as `new Uint8Array(buffer as unknown as ArrayBuffer)` | 3 |
| `get_track_peaks` **done** | `track: TrackId, level: u8` (0 to 2 detail, 3 overview) | raw `contracts::peaks` blob, empty when not analysed. Typed `number[]` like `get_artwork`: read as `ArrayBuffer`, decode with `ui/src/viz/peaks.ts` | 4 |
| `import_folder` **done** | `path: String, options: Option<ImportOptionsDto>` (features, target playlist) | `JobId` | 3 |
| `import_files` **done** | `paths: Vec<String>, options` | `JobId` | 3 |
| `import_playlist_file` **done** | `path: String, options` | `JobId`; makes a playlist from the M3U or M3U8 | 3 |
| `import_rekordbox_xml` **done** | `path: String, options` | `JobId` | 3 |
| `list_jobs` **done** | | `Vec<JobInfo>` (progress, source, error, created at), newest first, at most 50 | 3 |
| `pause_job` / `resume_job` / `cancel_job` **done** | `job: JobId` | `()` | 3 |
| `reanalyze_tracks` **done** | `tracks: Vec<TrackId>, features: AnalysisFeatures` | `JobId`; a locked grid survives | 3 |
| `list_playlists` **done** | | `Vec<PlaylistNodeDto>` (flat, parent ids, sibling order, track counts) | 3 |
| `create_playlist` **done** | `parent: Option<PlaylistId>, name: String, folder: bool` | `PlaylistId` | 3 |
| `rename_playlist` / `delete_playlist` **done** | `playlist, name?` | `()`; a delete takes the subtree, the tracks stay | 3 |
| `move_playlist` **done** | `playlist, parent: Option<PlaylistId>, position: u32` | `()`; `position` is the sibling index in the list without the moved node | 3 |
| `get_playlist_tracks` **done** | `playlist: PlaylistId` | `Vec<PlaylistEntryDto>` (entry id, position, track row) | 3 |
| `add_to_playlist` **done** | `playlist, tracks: Vec<TrackId>, position: Option<u32>` | `()` | 3 |
| `remove_from_playlist` **done** | `playlist, entries: Vec<i64>` (entry ids) | `()` | 3 |
| `reorder_playlist` **done** | `playlist, entries: Vec<i64>, to_index: u32` | `()` | 3 |
| `list_directory` **done** | `path: Option<String>` (`None` is the home folder, an empty path lists the roots) | `DirListing`; folders first, then audio files and `m3u`, `m3u8`, `xml` files; hidden entries are left out | 3 |
| `reveal_in_explorer` **done** | `path: String` | `()`; opens the system file manager | 3 |

### Analysis and beat grid

| Command | Args | Returns | M |
|---|---|---|---|
| `get_analysis` **done** | `track: TrackId` | `Option<TrackAnalysisDto>` (BPM, key in both notations, grid, grid locked, loudness) | 3 |
| `set_beat_grid` **done** | `track, grid: BeatGrid` | `()`; locks the grid and sends it to the decks holding the track | 3 |
| `nudge_beat_grid` | `track, offset: Seconds` | `BeatGrid` | 3 |

### Cues, loops, jumps, pads

| Command | Args | Returns | M |
|---|---|---|---|
| `list_cues` **done** | `track: TrackId` | `Vec<Cue>` (hot, memory, loop) | 3 |
| `set_hot_cue` **done** | `deck, index: HotCueIndex` | `Cue` (at playhead, with its row id when the track is in the library); stored in the library; fails with `invalid` when no track is loaded | 2 |
| `trigger_hot_cue` **done** | `deck, index, pressed: bool` | `()` | 2 |
| `get_hot_cues` **done** | `deck` | `HotCues` *shell* (deck, 8 slots); not in the original plan, hydrates the UI | 2 |
| `delete_hot_cue` **done** | `deck, index: HotCueIndex` | `()`; deletes the stored cue | 2 |
| `update_cue` | `cue: CueId, color: Option<CueColor>, name: String` | `()` | 4 |
| `delete_cue` | `cue: CueId` | `()` | 3 |
| `add_memory_cue` **done** | `deck` | `Cue` (at playhead), stored in the library | 2 |
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
| `list_saved_sets` **done** | | `Vec<SavedSetDto>`, newest first. The session's set is made on the first load and every loaded track is appended (title and artist snapshot, deck, time), so there is no `save_current_set` | 3 |
| `get_saved_set` **done** | `set: i64` | `SavedSetDetailDto` (header, entries in play order) | 3 |
| `rename_saved_set` **done** | `set: i64, name: String` | `()` | 3 |
| `delete_saved_set` **done** | `set: i64` | `()` | 3 |

## Events

| Event | Payload | Rate | M |
|---|---|---|---|
| `engine-telemetry` **done** | `EngineTelemetry` | ~60 Hz | 1 |
| `engine-event` **done** | `EngineEvent` (audio thread events plus `device_lost`, `device_changed`, `stream_error`) | on change | 1 |
| `track-loaded` **done** | `TrackLoaded` *shell* (deck, `TrackInfo` with library id, BPM and key, frames) | once per load | 1 |
| `track-load-failed` **done** | `TrackLoadFailed` *shell* (deck, path, message) | once per failed load | 1 |
| `deck-state-changed` | `DeckState` | on change | 1 |
| `mixer-state-changed` | `MixerState` | on change (MIDI-driven moves) | 2 |
| `audio-device-changed` **done** | `AudioStatus` (running, device, rate, buffer, error) | on change | 1 |
| `import-progress` **done** | `JobProgress` | <= 10 Hz per job; a state change is sent at once | 3 |
| `library-changed` **done** | `LibraryChange` (`tracks`, `playlists`) | <= 10 Hz, coalesced; also after every library command that edits rows | 3 |
| `analysis-ready` **done** | `TrackId` | once per track whose analysis was stored; a deck holding it already has the new grid | 3 |
| `hot-cues-changed` **done** | `HotCues` *shell* (deck, 8 slots) | on change | 2 |
| `cues-changed` **done** | `TrackId` | on every stored cue change | 3 |
| `pad-mode-changed` | `{ deck: DeckId, mode: PadMode }` | on change | 7 |
| `fx-state-changed` | `FxState` | on change | 5 |
| `midi-message` | `MidiMonitorEntry` (timestamp, bytes, decoded control id, action) | monitor on only, coalesced at 60 Hz | 6 |
| `midi-learned` | `MidiLearnResult` (message, suggested binding) | once | 6 |
| `midi-connection-changed` | `MidiStatus` | on change | 6 |
| `sampler-state-changed` | `SamplerState` | on change | 7 |

Mixer values cross IPC as linear `Gain` (trim, EQ band, master, headphone) or normalised positions (`filter` in `[-1, 1]`, channel fader and headphone mix in `[0, 1]`, crossfader in `[-1, 1]`). The UI owns the knob-to-gain curves (`ui/src/lib/mixer.ts`, constants mirror `contracts::mixer`); app-core validates ranges and the engine clamps.

Library DTOs (`TrackQueryDto`, `TrackRowDto`, `PlaylistNodeDto`, ...) live in `crates/app-core/src/dto.rs`, not in contracts. Types still missing (`FxState`, `MidiPorts`, ...) are added to `crates/contracts` by the architect when their milestone starts.
