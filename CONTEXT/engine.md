# engine

Owner: engine-dev.

## Purpose
Real-time audio engine: decks (2 shown, 4 supported), transport, CUE behaviour, tempo and key lock, sync, jog scratch and nudge, loops and loop roll, hot cues, beat jump, slip, mixer (trim, 3-band EQ, filter, fader, PFL, crossfader curves), master and headphone outputs (split cue, cue/master mix), FX slot host, sampler, offline renderer.

## Owned paths
- `crates/engine/**`

## Depends on
- `opendeck-contracts`: `EngineCommand`, `EngineTelemetry`, `EngineEvent`, `Effect`, `ProcessContext`, `StereoBufferMut`, `BeatGrid`, `CrossfaderCurve`, `TempoRange`, ids, units.
- `opendeck-media`: decoding on the loader thread only.

## External crates (add when used)
- `cpal` 0.18 (WASAPI default; `asio` cargo feature), `rtrb` 0.4, `signalsmith-stretch` 0.1 (key lock, key shift), `thiserror` 2, `assert_no_alloc` 1 (optional, dev). Later: `rubato` (async resampler for dual-device routing), `atomic_float` 1.

## RT rules (ADR 0003)
- Callback never allocates, frees, locks, blocks, does I/O, logs (`tracing`) or panics. No `unwrap`, no indexing that can fail: use validated ids and `get`.
- All mutation through the command ring; heap objects arrive on the hand-off ring and leave on the return ring.
- Smooth every continuous parameter per block. Clamp out-of-range values.
- Offline render tests wrap the render call in `assert_no_alloc`.
- Effects: host owns wet/dry, bypass crossfade, tail handling and latency compensation (ADR 0006).

## Acceptance per milestone
- M1: one deck plays a file, device selection, offline render tests. Track in as `TrackBuffer` over the hand-off ring (no decoding in the engine); play/pause, CUE (CDJ behaviour), seek, tempo fader and ranges, pitch bend, key lock; cpal output with device, rate and buffer selection, 4-channel routing (master 1/2, headphones 3/4 silent) with stereo fallback; telemetry at ~60 Hz; `OfflineRenderer`; zero allocations in the callback. The engine already allocates N decks (2 by default).
- M2: two decks, mixer (trim, 3-band EQ, filter, fader, crossfader curves), routing with cue (PFL, headphone mix, split cue), beat and tempo sync with leader election, quantize, loops and loop roll, hot cues, beat jump, slip, jog scratch and nudge.
- M5: FX units (deck/master/send), chains of `FX_SLOTS_PER_UNIT`, beat-synced context.
- M7: sampler slots, key shift via stretcher.

## M1 implementation notes
- `engine(config)` returns `(EngineCore, EngineHandle)`. The core goes to the cpal callback (`device::open_output`) or to `OfflineRenderer`; the handle stays on the control thread.
- Tracks: `EngineHandle::load_track` pushes a `TrackBuffer` (`Arc<[f32]>`, interleaved stereo at engine rate) on the hand-off ring. Replaced or ejected buffers return on the return ring; call `reclaim()` to drop them off the audio thread.
- Jumps (seek, cue return, pause, key lock toggle) render a 2 ms fade-out tail of the old state over a 2 ms fade-in of the new one, so they do not click.
- Key lock: Signalsmith stretcher, primed on every start or jump so the audible playhead matches `pos` despite its ~120 ms latency at 48 kHz. Off: varispeed with linear or cubic (`EngineConfig::interpolation`).
- Key lock priming is amortised: a 1440-frame warm-up (~30 ms at 48 kHz) with the stretcher running at up to 4x real time, so a callback does at most 4 blocks of extra stretcher work. The deck plays varispeed meanwhile, then crossfades to the stretcher over 10 ms. When rate != 1 the first ~30 ms are off-pitch.
- Backend errors are reported by `AudioOutput::poll_event` as `EngineEvent::DeviceLost`, `DeviceChanged` or `StreamError` (control thread; not on the audio event ring).
- cargo features: `asio` (forwards `cpal/asio`, Windows), `alloc-guard` (wraps the callback in `assert_no_alloc`; the binary installs `AllocDisabler` in debug).

## M2 implementation notes
- Track load: `EngineHandle::load_track_with_grid(deck, track, cue_point, Option<BeatGrid>)`; `load_track` is the same with no grid. `set_beat_grid(deck, Option<BeatGrid>) -> bool` swaps the grid of a loaded track (analysis finishing late). `BeatGrid` is `Copy`, so it rides the hand-off ring; no contract change. Grid anchor is seconds from the first frame of the buffer. A load clears loops, hot cues, slip and scheduled jumps; app-core re-sends `SetHotCue` after it. Sync, quantize, slip, vinyl, key lock and tempo settings survive a load.
- Module layout: `deck/` (playback, `cues`, `loops`, `jog`, `sync`), `sync.rs` (leader election, follower rates), `mixer.rs` (strips, crossfader, master, headphone bus), `dsp.rs` (biquads, Linkwitz-Riley, state-variable filter).
- Render entry points: `EngineCore::render(master)` and `render_with_headphones(master, headphones)`. The device `Router` writes the headphone bus to the configured pair; with no pair it is dropped. `OfflineRenderer::render_both` does the same offline.
- Quantize (default on, no-op without a grid): snaps stored positions (CUE set, hot cue store, loop in/out, auto loop start) to the nearest beat. A hot cue triggered while playing is scheduled for the next beat and lands exactly on the cue (jump with the 2 ms tail). Beat jump is already phase preserving. Seek, jog search and CUE-return are immediate.
- Loops: the last `declick` frames before the loop end blend into the audio before the loop start, then the playhead steps back by the loop length. No tail, no fade-in, so the loop start keeps its transient and a beat-aligned loop of a steady beat is sample exact. Minimum loop is 3 x 2 ms. Playhead already past the loop end when it activates is folded into the loop modulo its length; `LoopOut` and re-loop jump to the start. With key lock the stretcher input is read through the loop with the same blend, so no re-prime per repeat; leaving a loop re-aligns the stretcher once.
- Loop roll always uses slip: it saves the previous loop and restores it on release. Auto loop, roll and beat jump need a grid (no-op without). Roll start with quantize on is the previous multiple of the roll length on the grid.
- Slip: shadow playhead at the natural tempo rate. Reasons: scratch, loop, hot cue hold (only with slip on) and roll (always). Telemetry `slip_position` shows it.
- Hot cues: 8 slots. Empty slot press stores the playhead and sends `HotCueSet`. Playing: jump (hot loop also starts looping); stopped: preview while held, return on release.
- Jog: vinyl mode and touched = scratch (record held, wheel moves it, transport keeps running underneath; paused decks scrub). Wheel motion is accumulated as distance owed and delivered through a 20 ms first-order low pass, then the speed ramps linearly across each block, so any block or event rate gives the same smooth speed. Speed capped at 6x. Key lock is bypassed while scratching and re-primed on release. Otherwise the wheel nudges: 0.10 per revolution per second, capped at `MAX_PITCH_BEND`, 40 ms smoothing (`TODO(verify)`).
- Sync: see the module docs of `sync.rs`. Leader: `SetLeader` deck while playing with a grid; else the previous leader; else first playing deck with sync off; else first playing deck. Followers run at `leader_bpm * leader_rate / own_bpm` (x0.5 or x2 when closer). Beat sync aligns phase once (engage, play start, after jumps) and then only trims drift under 10 ms, so a nudge or scratch is not undone. Without a grid sync does nothing; `DeckStatus::sync` reports `NoGrid`, `NoLeader`, `Leader`, `Following`; telemetry carries `sync_mode` and `is_leader` per deck. A follower whose leader goes away holds its synced tempo until its fader moves.
- Mixer (product choices, not specs):
  - Order: trim, EQ, filter, PFL tap, channel fader (linear), meter, crossfader side, master level, headphones. Every gain ramps linearly over the block.
  - EQ: isolator, Linkwitz-Riley 8th order (48 dB/oct) crossovers at 200 Hz and 2.5 kHz, low band all-passed at 2.5 kHz so unity gains sum flat. Band gain 0 (kill) to +6 dB (`EQ_MAX_GAIN_DB`). LR4 only reaches about 36 dB at the band centres, so LR8 is used: kills measure -77 to -82 dB at 63 Hz, 707 Hz and 7 kHz. At unity the EQ is bypassed bit-exactly after a one block crossfade.
  - Filter: one knob, dead zone +/- 2 % (bit-exact bypass), left low pass (20 kHz down to 100 Hz), right high pass (20 Hz up to 8 kHz), exponential sweep, 2-pole state-variable, Q 1.2 (about +1.6 dB at cutoff). Coefficients update every 32 frames.
  - Trim clamps at +12 dB, EQ at +6 dB. NaN and infinity are ignored.
  - Headphones: PFL taps pre-fader after EQ and filter. Blend is equal power, 0 = cue, 1 = master; the master tap is after the master level. Split cue ignores the blend: cue mono left, master mono right. No PFL and blend 0 is silence.
  - Meters: per channel post fader, master after master level, headphone bus. Peak holds with 0.85 decay per snapshot.
- CPU (release, 48 kHz, 512 frames): 2 decks key lock + EQ + filter + PFL: 0.61 ms per block (5.7 % of realtime), worst 2.9 ms; 1 deck key lock +8 %: 2.9 %.
- Tests: `tests_deck.rs`, `tests_mixer.rs`, `tests_support.rs`; all through `EngineCore::render` under `assert_no_alloc`, plus a 3000 command fuzz on 4 decks.

## M5 implementation notes (FX host)
- Code: `fx.rs` (host, module docs have the signal flow), tests in `tests_fx.rs` plus `fx_host_makes_no_heap_calls` in `tests_heap.rs`.
- Units: `MAX_DECKS` deck units, one master, `FX_SEND_BUSES` send units, `FX_SLOTS_PER_UNIT` serial slots each. Deck insert: after EQ and filter, before the PFL tap and fader (cue hears it). Master insert: after send returns are summed, before master level. Sends: per bus pre-fader (after insert) or post-fader (what the channel adds to master, after fader and crossfader side); default post; `EngineHandle::set_fx_send_tap`.
- Install: `EngineHandle::install_effect(FxSlotId, Box<dyn Effect>) -> Result<(), Box<dyn Effect>>` prepares the effect (`prepare(sample_rate, max_block)`), allocates the slot buffers, hands the box over the FX hand-off ring (16). `clear_effect(slot) -> bool`. Replaced boxes return on the FX return ring (32); `reclaim()` drops them. Set initial params on the effect before installing. A swap is a hard cut; disable and wait ~10 ms first.
- Commands: `SetFxEnabled`, `SetFxParam` (normalized, clamped to `[0, 1]`, unknown ids and NaN ignored, effect smooths), `SetFxMix` (clamped), `SetFxBeatDivision` (clamped to 1/64..64, non-positive ignored), `SetFxSend` (clamped to +6 dB). Defaults: wet 0.5 on deck and master slots, 1.0 on send slots.
- Slot host: equal-power wet/dry ramped over the block; 10 ms bypass crossfade; tail effects are fed a faded-out input and keep running until -90 dBFS holds for 4096 frames (cap 10 s); `Effect::reset()` on every engage (also while a tail still rings), on the engage frame; latency read once at install and applied to the dry path (cap 16384 frames), the delay line stays in the path while an effect with latency is installed so toggling never shifts timing; compensation is inside the slot only. Send units: dry path is silent while nothing in the chain is enabled, so an idle send returns nothing.
- Context: deck units get their deck's BPM (grid BPM x rate), beat position at block start, play state; master and send units the sync leader (else first playing deck with a grid); `playing` is any deck for those. Per unit `FxTempoSource`: `Auto`, `Deck(DeckId)`, `Tap(Bpm)` (own beat counter from 0); `EngineHandle::set_fx_tempo_source`.
- Controller features (DDJ-FLX4 Beat FX): unit to channel 1, 2 or 1&2 maps to the deck chains; for 1&2 app-core keeps one instance per channel, mirrors the `SetFx*` commands and sets both units to `FxTempoSource::Deck(d)`. Pad FX: `schedule_fx_enabled(slot, on, at: Frames, fast)` starts on the exact frame (1 ms ramp if `fast`; the effect is split and reset at that frame). Release FX: `release_fx(from, release, at, hold)` bypasses `from`, runs `release` for `hold` frames, then lets its tail ring. `at` is on the `EngineTelemetry::engine_time` clock; 16 timed events wait at once.
- Status for tests and tools (no telemetry yet): `EngineCore::fx_slot_status`, `fx_unit_status`.
- CPU (release, 48 kHz, 512 frames): 2 decks x 3 slots + master x 3 slots of a gain effect: 1.8 % of realtime, worst block 1.7 ms (on top of the M2 mixer figure).
- Open: FX telemetry needs a contract addition (per unit enabled, active, level); commands for timed engage, release, tempo source and send tap are engine handle calls until the contracts get them.

## Open questions
- Jog nudge sensitivity and `MAX_PITCH_BEND`: product tuning. M2 uses 0.10 per rev/s and 8 % (`TODO(verify)`).
- `DeckTelemetry` has `sync_mode`, `is_leader`, `loop_region` and `loop_active`. The finer `SyncStatus` (no grid, no leader) is still engine-local.
- A quantized hot cue waits for the next beat (up to one beat). Some products land at once on the cue with phase offset instead; decide with a user test.
- Channel fader law is linear. A log taper is a one line change if wanted.
