//! Audio and MIDI device settings DTOs.

/// Audio host API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum AudioHostKind {
    /// Windows WASAPI (default on Windows).
    Wasapi,
    /// Steinberg ASIO (Windows, only in builds with the `asio` feature).
    Asio,
    /// macOS Core Audio.
    CoreAudio,
    /// Linux ALSA.
    Alsa,
    /// JACK (Linux/macOS, only in builds with the `jack` feature).
    Jack,
}

/// One audio host as reported to the settings screen.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AudioHostInfo {
    /// Host API.
    pub kind: AudioHostKind,
    /// Display name.
    pub name: String,
    /// Compiled in and usable on this machine.
    pub available: bool,
    /// The platform default host.
    pub is_default: bool,
    /// The host exposes at least one input device (mic and aux inputs).
    #[cfg_attr(feature = "serde", serde(default))]
    pub input: bool,
    /// Some output device offers four channels or more (separate master,
    /// headphone and booth pairs).
    #[cfg_attr(feature = "serde", serde(default))]
    pub multichannel_output: bool,
    /// Some device reports a buffer size range, so the buffer size setting
    /// takes effect. Without it the device default is used.
    #[cfg_attr(feature = "serde", serde(default))]
    pub configurable_buffer: bool,
}

/// A pair of output channels (zero-based) on the output device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ChannelPair {
    /// Left channel index.
    pub left: u16,
    /// Right channel index.
    pub right: u16,
}

/// Most channels of one device the channel pickers offer.
pub const MAX_DEVICE_CHANNELS: u16 = 32;

/// Input channels of the input device that carry one source: a single channel
/// (mono, heard on both sides) or a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InputChannels {
    /// Left (or only) channel index, zero-based.
    pub left: u16,
    /// Right channel index, `None` for a mono source.
    pub right: Option<u16>,
}

impl InputChannels {
    /// A mono source on `channel`.
    pub const fn mono(channel: u16) -> Self {
        Self {
            left: channel,
            right: None,
        }
    }

    /// A stereo source on a pair.
    pub const fn pair(left: u16, right: u16) -> Self {
        Self {
            left,
            right: Some(right),
        }
    }
}

/// Talk-over: the program (decks, sampler, aux) is ducked while the mic is
/// louder than a threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TalkOverSettings {
    /// Duck the program while the mic is active.
    pub enabled: bool,
    /// How far the program is lowered, in dB (positive, `0..=40`).
    pub depth_db: f32,
    /// Mic level in dBFS above which the program ducks (`-80..=0`).
    pub threshold_db: f32,
}

impl Default for TalkOverSettings {
    // TODO(verify): product defaults, not a spec.
    fn default() -> Self {
        Self {
            enabled: false,
            depth_db: 12.0,
            threshold_db: -40.0,
        }
    }
}

/// Persisted audio settings (`settings` table, key `audio`).
///
/// Default routing is one multichannel device: master on channels 1/2 and
/// headphones on 3/4, as on the DDJ-FLX4 (ADR 0001, deviation 5). Every field
/// has a serde default, so settings stored by an older build still load.
///
/// Output pairs index the output device's channels (up to
/// [`MAX_DEVICE_CHANNELS`]). A device that cannot carry them drops the booth,
/// then the headphones, then falls back to stereo on channels 1/2
/// (`stereo_fallback` in the status). A mono output plays the sum of left and
/// right on both channels of its pair.
///
/// Inputs are opened only when `mic` or `aux` is set. `input_device` `None`
/// means the input of the output device when it has one, else the host
/// default input.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AudioSettings {
    /// Host API, `None` for the platform default.
    pub host: Option<AudioHostKind>,
    /// Output device name as reported by the host, `None` for the default device.
    pub device: Option<String>,
    /// Requested sample rate in Hz, `None` for the device default.
    pub sample_rate: Option<u32>,
    /// Requested buffer size in frames, `None` for the device default.
    pub buffer_frames: Option<u32>,
    /// Master output channels.
    pub master: ChannelPair,
    /// Play the master as the mono sum of left and right.
    pub master_mono: bool,
    /// Headphone output channels, `None` to disable headphone output.
    pub headphones: Option<ChannelPair>,
    /// Play the headphones as the mono sum of left and right.
    pub headphones_mono: bool,
    /// Booth output channels, `None` to disable the booth output. The booth
    /// plays a copy of the master with its own level.
    pub booth: Option<ChannelPair>,
    /// Play the booth as the mono sum of left and right.
    pub booth_mono: bool,
    /// Booth level, linear gain (`1.0` = same as the master).
    pub booth_level: f32,
    /// Input device name, `None` for the output device or the host default.
    pub input_device: Option<String>,
    /// Microphone channels on the input device, `None` for no mic.
    pub mic: Option<InputChannels>,
    /// Aux / line input pair on the input device, `None` for no aux.
    pub aux: Option<ChannelPair>,
    /// Talk-over (ducking) settings.
    pub talkover: TalkOverSettings,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            host: None,
            device: None,
            sample_rate: None,
            buffer_frames: None,
            master: ChannelPair { left: 0, right: 1 },
            master_mono: false,
            headphones: Some(ChannelPair { left: 2, right: 3 }),
            headphones_mono: false,
            booth: None,
            booth_mono: false,
            booth_level: 1.0,
            input_device: None,
            mic: None,
            aux: None,
            talkover: TalkOverSettings::default(),
        }
    }
}

impl AudioSettings {
    /// An input stream is needed: a mic or an aux pair is routed.
    pub fn wants_input(&self) -> bool {
        self.mic.is_some() || self.aux.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_routing_is_four_channel() {
        let s = AudioSettings::default();
        assert_eq!(s.master, ChannelPair { left: 0, right: 1 });
        assert_eq!(s.headphones, Some(ChannelPair { left: 2, right: 3 }));
        assert_eq!(s.booth, None);
        assert!(!s.wants_input());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn settings_stored_before_m8_still_load() {
        let old = r#"{"host":null,"device":"DDJ-FLX4","sample_rate":48000,
            "buffer_frames":256,"master":{"left":0,"right":1},
            "headphones":{"left":2,"right":3}}"#;
        let s: AudioSettings = serde_json::from_str(old).expect("old settings");
        assert_eq!(s.device.as_deref(), Some("DDJ-FLX4"));
        assert_eq!(s.buffer_frames, Some(256));
        assert_eq!(s.booth_level, 1.0);
        assert_eq!(s.mic, None);
        assert_eq!(s.talkover, TalkOverSettings::default());
        let empty: AudioSettings = serde_json::from_str("{}").expect("empty");
        assert_eq!(empty, AudioSettings::default());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn new_settings_round_trip() {
        let s = AudioSettings {
            booth: Some(ChannelPair { left: 4, right: 5 }),
            booth_mono: true,
            booth_level: 0.5,
            mic: Some(InputChannels::mono(2)),
            aux: Some(ChannelPair { left: 0, right: 1 }),
            talkover: TalkOverSettings {
                enabled: true,
                ..TalkOverSettings::default()
            },
            ..AudioSettings::default()
        };
        let json = serde_json::to_string(&s).expect("serialize");
        assert_eq!(
            serde_json::from_str::<AudioSettings>(&json).expect("parse"),
            s
        );
    }
}
