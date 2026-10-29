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

/// Persisted audio output settings (`settings` table, key `audio`).
///
/// Default routing is one multichannel device: master on channels 1/2 and
/// headphones on 3/4, as on the DDJ-FLX4 (ADR 0001, deviation 5).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    /// Headphone output channels, `None` to disable headphone output.
    pub headphones: Option<ChannelPair>,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            host: None,
            device: None,
            sample_rate: None,
            buffer_frames: None,
            master: ChannelPair { left: 0, right: 1 },
            headphones: Some(ChannelPair { left: 2, right: 3 }),
        }
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
    }
}
