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

- [ ] Waveform scroll stays smooth at 60 fps with two decks playing on the owner's PC (open the `Viz/Perf` story and the app; record p95 frame time and late frames).
- [ ] Waveforms, knobs, jog and meters stay sharp on a high-DPI display (150% and 200% scaling) and after moving the window between screens.
- [ ] Jog feel: platter touch stops the deck, turning scratches and nudges at a natural rate, Shift search speed is usable (`SEARCH_BEATS_PER_REV` TODO(verify)).

## M5 FX

- [ ] Beat FX unit on channel 1, then 2, then 1&2: only the assigned decks change; on 1&2 both decks stay in time with each other.
- [ ] Multi unit with three effects on a channel: Sound Color FX, a held pad FX and Release FX still work on that channel.
- [ ] Release FX (SHIFT+ON/OFF and a release pad): the Beat FX stops, the release effect runs for its hold time, then rings out.
- [ ] Tap BPM: four taps set the unit tempo and the effect follows it; back to Auto follows the track again.
- [ ] CFX knob feel for each type: centre is transparent, no zipper noise, both sides sweep evenly.
- [ ] Echo and reverb tails ring out after ON/OFF and after a pad release, with no click.
- [ ] Chains and presets survive restart; no clicks when toggling.

## M6 MIDI (DDJ-FLX4)

Never run on a real FLX4 yet. Open the MIDI monitor from the controller indicator in the top bar. Channels below are 1-based as in the message list (channel 1 is status 0x90 or 0xB0). Record results against the question id in `docs/midi-open-questions.md`.

Connection
- [ ] Settings, MIDI: device on Automatic, Connect at launch on. Start the app with the FLX4 plugged in: the indicator shows the controller with no click on Connect.
- [ ] Unplug the FLX4 while a deck plays: the indicator changes and playback continues. Replug: it reconnects within about 2 s and the LEDs come back (vinyl mode, loaded, play, pads).

Controls
- [ ] Move every control once, section by section (deck 1, deck 2, Beat FX, mixer, browse, pads in each of the 8 modes, each with and without SHIFT). For each row in the monitor, the bytes match the message list and "Mapped to" names the expected control. Note rows without a mapping and rows with a `verify` badge.
- [ ] Each control does what the manual says in the app (play, CUE, sync, tempo, loops, hot cues, beat jump, beat loop, pad FX, EQ, CFX, faders, crossfader, CH CUE, browse and load).

LEDs
- [ ] Each pad mode button: the mode LED lights and the 8 pads show the state for that mode (hot cue set, loop length active, pad FX held). Repeat with SHIFT for the shifted modes.
- [ ] PLAY blinks while paused with a track loaded, lights while playing, is off with no track; CUE, SYNC, CH CUE, loop IN and OUT follow the app.
- [ ] Hold SHIFT: LEDs keep the same state and clear correctly when SHIFT is released.

Jog and meters
- [ ] Jog feel: touch stops the deck in vinyl mode, scratch and nudge rates feel natural, SHIFT + jog search is usable (search speed 32 beats per revolution is a placeholder).
- [ ] Q02 jog scale: pause a deck, mark the platter, turn it exactly one revolution. Filter the monitor on the deck's channel and count messages on CC 0x22 (vinyl on), CC 0x23 (vinyl off), CC 0x29 (SHIFT) and CC 0x21 (side ring). Record ticks per revolution for each and whether turning faster changes the count or the value (0x41 or more up, 0x3F or less down).
- [ ] Q04 VU curve: play a steady tone at the levels the master meter in the app reads 0, -6, -12, -18, -24 and -36 dB. Record which channel meter segments light on the FLX4 at each level (bands per list 3-15: Green1 0x26, Green2 0x41, Orange1 0x57, Orange2 0x65, Red 0x77).

Open questions
- [ ] Q03 14-bit order: move the deck 1 tempo fader slowly. Record whether CC 0x00 (MSB) or CC 0x20 (LSB) arrives first, and repeat on a channel fader and an EQ knob.
- [ ] Q05 sampler pads: in SAMPLER mode press pads 1 to 8 on each deck. Record the status bytes (channel 8 and 10 per list 5-5). Behaviour waits for M7.
- [ ] Q06 audio routing: see M1 (channels 1/2 master, 3/4 headphones).
- [ ] Q07 Beat FX LEVEL/DEPTH: turn the knob. Record the status byte: 0xB4 (channel 5, as mapped) or 0xB5 (channel 6, as the channel column prints).
- [ ] Q08 crossfader start: turn Fader start on in Settings, MIDI. Move the crossfader off each edge and back. Record whether messages arrive on channel 7 (0x96) or as 0x90/0x91 notes 0x66 and 0x52, and compare with moving the channel faders off the bottom.
- [ ] Q09 BEAT SYNC: press and hold SYNC on deck 1, then release. Record when the message arrives (press or release) and which values (0x7F then 0x00, or 0x7F only). Hold for 2 s and record the long-press note 0x5C.
- [ ] Q10 FX ON/OFF LED: assign the Beat FX to CH1 and turn it on, then off. Record whether the button blinks while on and is steady lit while off, as note *4 says. If not, record what it shows for 0x7F and 0x00 on note 0x47.
- [ ] Q11 LOADED: connect with both decks empty, then load a track on each. Record that the LOADED indicator is off when empty (0x00 assumed) and on after load (0x7F).
- [ ] Q12 IN and OUT LEDs: set a loop with IN then OUT, exit it, then use 4 BEAT. Record which LEDs light at each step against manual p.15-16 (the mapping lights both while a loop is active).
- [ ] Q13 SHIFT LED bank: with play, cue, loop, sync and CH CUE on, hold SHIFT. Record whether the LEDs show the +SHIFT bank (play 0x0E, cue 0x48, IN 0x4C, OUT 0x4E, sync 0x60, CH CUE 0x68) with the same state.
- [ ] Q14 pad colours: in each pad mode, record the colour each lit pad shows and whether it differs per mode.

## M7 Pad modes and sampler

- [ ] Sampler slot sharing between decks: slots are shared or per deck. TODO(verify)
- [ ] Middle panel follows the active pad mode on both decks.

## M8 Hardening

- [ ] Round-trip latency measured and recorded per device.
- [ ] First-run flow on a clean user profile.
- [ ] Installer upgrade keeps the library.
