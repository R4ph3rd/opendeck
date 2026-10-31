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
| `set_pad_mode` **done** | `deck, mode: PadMode` | `bool` (changed); emits `pad-mode-changed`. Choosing KEYBOARD forgets its hot cue | 7 |
| `pad_press` **done** | `deck, mode: PadMode, pad: u8, shift: bool, pressed: bool` | `()`; same interpreter as the controller's pads, ignored if `mode` is not the deck's mode | 7 |
| `get_pad_state` **done** | | `PadStateDto` (per deck: mode, signed beat jump sizes, loop lengths, key shift and keyboard semitones, keyboard cue) | 7 |
| `set_key_shift` **done** | `deck, semitones: i8` | `()`; `-12..=12`, else `invalid` | 7 |
| `rename_hot_cue` **done** | `deck, index: HotCueIndex, name: String` | `()`; stored with the track, `hot-cues-changed` follows | 7 |
| `get_sampler_state` **done** | | `SamplerStateDto` (16 slots, layout, level, PFL) | 7 |
| `sampler_load_track` / `sampler_load_path` **done** | `slot: u8, track: TrackId` / `slot: u8, path: String` | `()`; decoded off-thread at the engine rate, then `sampler-state-changed` | 7 |
| `sampler_clear` **done** | `slot: u8` | `()` | 7 |
| `sampler_set_slot` **done** | `slot: u8, patch: SamplerSlotPatch` (name, colour, gain `0..=2`, play mode `one_shot`/`gate`/`loop`, sync; `null` keeps a field) | `()` | 7 |
| `sampler_set_layout` **done** | `layout: SamplerLayout` (`split`: deck 1 slots 1-8, deck 2 slots 9-16; `shared`: both 1-8) | `()` | 7 |
| `sampler_set_level` / `sampler_set_pfl` **done** | `level: f32` / `on: bool` | `()` | 7 |
| `sampler_trigger` / `sampler_stop` **done** | `slot: u8, pressed: bool` / `slot: u8` | `()` | 7 |

### Effects

The FX model sits on the engine FX units (ADR 0006): two FLX4-style Beat FX units (each on channel 1, 2 or 1&2), one Sound Color FX per channel, 32 pad effects and two send buses. DTOs (`Fx*Dto`, `FxOp`) live in `crates/app-core/src/fx_types.rs`. The model is persisted in the library settings (`fx.state`, `fx.chains`) and user effect presets in `fx_presets`. Every edit is one `FxOp` through `fx_apply`; the new model arrives as `fx-state-changed`. Engine slot layout and its limits: `crates/app-core/src/fx.rs` module docs.

| Command | Args | Returns | M |
|---|---|---|---|
| `fx_list_effects` **done** | | `Vec<FxEffectDto>` (key, name, routing, params, primary parameter, release flag, accepted beat divisions) | 5 |
| `fx_get_state` **done** | | `FxStateDto` (units, CFX, sends, send levels, pads) | 5 |
| `fx_apply` **done** | `op: FxOp` | `()`; ops: `set_assign`, `set_mode`, `set_effect`, `set_param`, `set_level` (primary parameter), `set_wet`, `set_division`, `step_division` (BEAT left/right), `set_slot_enabled`, `set_unit_on`, `release`, `set_bpm_source`, `tap`, `set_release_effect`, `set_cfx_kind`, `set_cfx`, `set_send_level`, `set_send_tap`, `apply_preset`, `apply_chain`, `set_pad`. Fails with `invalid` for an unknown effect, slot or parameter | 5 |
| `fx_pad` **done** | `deck, bank: u8, shift: bool, pad: u8, pressed: bool` | `()`; hold engages the assigned pad effect at the next engine frame (next beat when the deck has quantize on), release stops it; a release pad fires Release FX | 5 |
| `fx_get_presets` **done** | | `FxPresetsDto` (factory presets, user presets, chains) | 5 |
| `fx_save_preset` **done** | `slot: FxSlotRef, name: String` | `FxPresetsDto`; stores the slot's effect, parameters, wet and division | 5 |
| `fx_delete_preset` **done** | `effect: EffectKey, name: String` | `FxPresetsDto` | 5 |
| `fx_save_chain` **done** | `unit: FxUnitRef, name: String` | `FxPresetsDto`; stores the unit's effects | 5 |
| `fx_delete_chain` **done** | `name: String` | `FxPresetsDto` | 5 |

### MIDI

*shell* types live in `crates/app-core/src/midi_types.rs`. Without a MIDI service the commands return `invalid`.

| Command | Args | Returns | M |
|---|---|---|---|
| `midi_list_ports` **done** | | `Vec<MidiPortDto>` (name, has input, has output, matches the mapping's device patterns) | 6 |
| `midi_get_status` **done** | | `MidiStatusDto` (mapping, worker running, connected ports, last error, auto-connect, chosen device, fader start, dropped input, `verify` count) | 6 |
| `midi_connect` **done** | `device: Option<String>` | `MidiStatusDto`; starts the worker. `Some(name)` selects a port by name and is remembered, `None` follows the mapping's patterns. The worker rescans every second, so a device plugged in later connects | 6 |
| `midi_disconnect` **done** | | `MidiStatusDto`; stops the worker and closes the ports | 6 |
| `midi_set_auto_connect` **done** | `on: bool` | `MidiStatusDto`; start the worker at launch (default on) | 6 |
| `midi_set_fader_start` **done** | `on: bool` | `MidiStatusDto`; hardware fader start plays and pauses decks (default off) | 6 |
| `midi_set_monitor` **done** | `on: bool` | `()`; streams `midi-monitor` | 6 |
| `midi_learn_start` **done** | `control_id: String` | `()`; the next gesture on the controller becomes the control's input and emits `midi-learned` | 6 |
| `midi_learn_cancel` **done** | | `()` | 6 |
| `midi_get_mapping` **done** | | `Vec<MidiMappingRowDto>` (id, label, source ref, binding, `ControlAction`, layer, value kind, `verify`, overridden) | 6 |
| `midi_set_override` **done** | `control_id: String, binding: MidiBindingDto` | `Vec<MidiMappingRowDto>`; refused with `invalid` when the input is used by another control. Stored in the library settings (key `midi.settings`) and applied with `Mapping::apply_input_overrides` | 6 |
| `midi_clear_override` **done** | `control_id: String` | `Vec<MidiMappingRowDto>` | 6 |
| `midi_reset_overrides` **done** | | `Vec<MidiMappingRowDto>` | 6 |
| `midi_export_mapping` **done** | `path: String` | `()`; writes the effective mapping JSON | 6 |
| `midi_import_mapping` **done** | `path: String` | `Vec<MidiMappingRowDto>`; same mapping id only, bindings that differ from the shipped file become overrides | 6 |
| `browse_set_context` **done** | `context: BrowseContextDto` (collection query or playlist, selected row) | `()`; tells the browse encoder which list is on screen | 6 |
| `browse_select` **done** | `index: u32` | `()`; the pane selected a row | 6 |

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
| `get_ui_pref` **done** | `name: String` | `Option<String>` (JSON text) | 8 |
| `set_ui_pref` **done** | `name: String`, `json: String` | `()` | 8 |
| `reset_ui_prefs` **done** | none | `u32` (removed) | 8 |
| `get_diagnostics` **done** | none | `DiagnosticsDto` (version, OS, devices, paths, recent errors, text) | 8 |
| `open_logs_folder` **done** | none | `()` | 8 |
| `get_startup_notices` **done** | none | `Vec<String>` (for example a replaced database) | 8 |
| `play_test_tone` **done** | `host`, `device`, `pair: ChannelPair` | `()` (blocks about 1 s; own stream, see `tone.rs`) | 8 |

## Events

| Event | Payload | Rate | M |
|---|---|---|---|
| `engine-telemetry` **done** | `EngineTelemetry` | ~60 Hz | 1 |
| `engine-event` **done** | `EngineEvent` (audio thread events plus `device_lost`, `device_changed`, `stream_error`) | on change | 1 |
| `track-loaded` **done** | `TrackLoaded` *shell* (deck, `TrackInfo` with library id, BPM and key, frames) | once per load | 1 |
| `track-load-failed` **done** | `TrackLoadFailed` *shell* (deck, path, message) | once per failed load | 1 |
| `deck-state-changed` **done** | `DeckState` | when the controller changed it (read at 4 Hz and after button presses); UI-driven changes are read with `get_deck_state` | 6 |
| `mixer-state-changed` **done** | `MixerState` | on change, MIDI-driven moves only (at most 4 Hz) | 6 |
| `audio-device-changed` **done** | `AudioStatus` (running, device, rate, buffer, error) | on change | 1 |
| `import-progress` **done** | `JobProgress` | <= 10 Hz per job; a state change is sent at once | 3 |
| `library-changed` **done** | `LibraryChange` (`tracks`, `playlists`) | <= 10 Hz, coalesced; also after every library command that edits rows | 3 |
| `analysis-ready` **done** | `TrackId` | once per track whose analysis was stored; a deck holding it already has the new grid | 3 |
| `hot-cues-changed` **done** | `HotCues` *shell* (deck, 8 slots) | on change | 2 |
| `cues-changed` **done** | `TrackId` | on every stored cue change | 3 |
| `pad-mode-changed` **done** | `PadModeChanged` *shell* (deck, mode) | when a pad mode button on the controller or `set_pad_mode` changed it | 6 |
| `fx-state-changed` **done** | `FxStateDto` | on every FX model change | 5 |
| `midi-monitor` **done** | `Vec<MidiMonitorEntryDto>` (timestamp, direction, bytes, text, mapped control and action, `verify`) | monitor on only, batched every 50 ms | 6 |
| `midi-learned` **done** | `MidiLearnResultDto` (control id, accepted binding or error, description) | once per gesture | 6 |
| `midi-connection-changed` **done** | `MidiStatusDto` | on change | 6 |
| `browse-cursor-moved` **done** | `BrowseCursorDto` (row index, track) | per encoder step | 6 |
| `sampler-state-changed` **done** | `SamplerStateDto` | on change (load progress, edits, layout, level) | 7 |
| `pad-state-changed` **done** | `PadStateDto` | when a beat jump scale, pitch layout or the keyboard cue changed (controller or UI) | 7 |

Mixer values cross IPC as linear `Gain` (trim, EQ band, master, headphone) or normalised positions (`filter` in `[-1, 1]`, channel fader and headphone mix in `[0, 1]`, crossfader in `[-1, 1]`). The UI owns the knob-to-gain curves (`ui/src/lib/mixer.ts`, constants mirror `contracts::mixer`); app-core validates ranges and the engine clamps.

Library DTOs (`TrackQueryDto`, `TrackRowDto`, `PlaylistNodeDto`, ...) live in `crates/app-core/src/dto.rs`, not in contracts. Types still missing (`MidiPorts`, ...) are added to `crates/contracts` by the architect when their milestone starts.
