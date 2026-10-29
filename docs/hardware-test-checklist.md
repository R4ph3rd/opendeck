# Hardware test checklist

Manual tests that need a real controller or audio device. Tick items with the date, OS and device. Items marked TODO(verify) wait on a source check (see `docs/midi-open-questions.md`).

## M0 Foundations

- [ ] App starts on Windows 10/11 with WebView2 present.
- [ ] Unsigned installer opens following the steps in `docs/BUILD.md`.

## M1 Audio engine

All on Windows 10/11 unless noted.

- [ ] Device list shows WASAPI devices; selection survives restart.
- [ ] Open an audio file on deck A; title, duration and progress bar appear.
- [ ] Play/pause, CUE (CDJ behaviour) and seek work without clicks.
- [ ] Tempo +8 % and -8 % with key lock off: pitch follows tempo.
- [ ] Tempo +8 % and -8 % with key lock on: pitch holds. Listen for artifacts at start and after seeks (the first ~30 ms are varispeed by design).
- [ ] Buffer sizes 64, 128, 256 and 512: note xruns over 10 minutes for each.
- [ ] FLX4 audio interface selected as output device: master plays on channels 1/2. TODO(verify)
- [ ] FLX4 audio output routing: 4-channel device, channels 1/2 master, 3/4 headphones. TODO(verify)
- [ ] Unplug the output device while playing: a toast reports the loss and the app keeps running.
- [ ] ASIO device list and playback (only on a local build with the `asio` feature).

## M2 Decks and mixer

On a 4-channel device (FLX4 or similar) with headphones on 3/4.

- [ ] Two-deck mix on headphones: PFL on deck B reaches headphones only, master keeps deck A. Sweep the cue/master blend end to end.
- [ ] Split cue: cue mono on the left ear, master mono on the right; the blend has no effect.
- [ ] EQ kill on each band: the band drops out with no click; returning to unity sounds identical to EQ untouched.
- [ ] Filter sweep left (low pass) and right (high pass): smooth, no zipper noise, centre dead zone is transparent.
- [ ] Crossfader curves: constant power has no dip in the centre, linear dips, sharp cuts in the last few percent at each edge.
- [ ] Loop in/out, auto loops 1/4 to 32 beats, halve and double: wraps are click-free on vocals and pads.
- [ ] Loop roll while held, slip on and off: playback resumes where it would have been.
- [ ] Hot cues: set, jump while playing (lands on the next beat with quantize), preview while paused.
- [ ] Sync on real tracks once M3 grids exist: tempo and beat sync hold over 5 minutes, half and double time pick the closer tempo. TODO(verify) after M3.
- [ ] Jog scratch and nudge feel (sensitivity and pitch bend range are `TODO(verify)`).

## M3 Library

- [ ] Import a large folder from an external drive; unplug and replug.

## M4 Waveforms and deck UI

- [ ] Waveform scroll stays smooth at 60 fps on the target laptop.

## M5 FX

- [ ] Chains and presets survive restart; no clicks when toggling.

## M6 MIDI (DDJ-FLX4)

- [ ] Shift-layer pad mode pairs: which pad mode each shifted pad button selects. TODO(verify)
- [ ] Jog ticks per revolution (touch-on and touch-off, platter vs side). TODO(verify)
- [ ] 14-bit faders and knobs: MSB/LSB send order (MSB first or LSB first). TODO(verify)
- [ ] VU meter LED curve: value to LED segment mapping. TODO(verify)
- [ ] Shift-layer LEDs light and clear correctly.
- [ ] Hot unplug and replug recovers without restart.

## M7 Pad modes and sampler

- [ ] Sampler slot sharing between decks: slots are shared or per deck. TODO(verify)
- [ ] Middle panel follows the active pad mode on both decks.

## M8 Hardening

- [ ] Round-trip latency measured and recorded per device.
- [ ] First-run flow on a clean user profile.
- [ ] Installer upgrade keeps the library.
