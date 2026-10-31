//! Diagnostics report for Settings, Advanced: version, OS, audio and MIDI
//! devices, file locations and the last problems.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::midi_types::MidiStatusDto;
use crate::types::AudioStatus;

/// Where the app keeps its files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AppPathsDto {
    /// Log directory.
    pub logs: Option<String>,
    /// Library database file.
    pub database: Option<String>,
    /// Config directory.
    pub config: Option<String>,
}

impl AppPathsDto {
    /// Paths from optional locations.
    pub fn new(logs: Option<&Path>, database: Option<&Path>, config: Option<&Path>) -> Self {
        let text = |p: Option<&Path>| p.map(|p| p.display().to_string());
        Self {
            logs: text(logs),
            database: text(database),
            config: text(config),
        }
    }
}

/// Everything the report shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DiagnosticsDto {
    /// App version.
    pub version: String,
    /// Target OS.
    pub os: String,
    /// Target architecture.
    pub arch: String,
    /// Audio host API in use.
    pub audio_host: Option<String>,
    /// Audio device in use.
    pub audio_device: Option<String>,
    /// Audio error, if the output is off.
    pub audio_error: Option<String>,
    /// MIDI input port in use.
    pub midi_device: Option<String>,
    /// MIDI error, if any.
    pub midi_error: Option<String>,
    /// File locations.
    pub paths: AppPathsDto,
    /// Startup messages (for example a replaced database).
    pub notices: Vec<String>,
    /// Last warnings, errors and panics, oldest first.
    pub recent_errors: Vec<String>,
    /// The report as plain text, ready to copy.
    pub text: String,
}

/// Builds the report. `recent_errors` and `notices` come from the caller so
/// the function stays pure.
pub fn build_diagnostics(
    audio: Option<&AudioStatus>,
    midi: Option<&MidiStatusDto>,
    paths: AppPathsDto,
    notices: Vec<String>,
    recent_errors: Vec<String>,
) -> DiagnosticsDto {
    let mut report = DiagnosticsDto {
        version: crate::VERSION.to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        audio_host: audio.and_then(|a| a.host.clone()),
        audio_device: audio.and_then(|a| a.device.clone()),
        audio_error: audio.and_then(|a| a.error.clone()),
        midi_device: midi.and_then(|m| m.input.clone()),
        midi_error: midi.and_then(|m| m.last_error.clone()),
        paths,
        notices,
        recent_errors,
        text: String::new(),
    };
    report.text = render(&report);
    report
}

fn render(r: &DiagnosticsDto) -> String {
    let or_none = |v: &Option<String>| v.clone().unwrap_or_else(|| "none".to_owned());
    let mut out = String::new();
    out.push_str(&format!("OpenDeck {}\n", r.version));
    out.push_str(&format!("OS: {} {}\n", r.os, r.arch));
    out.push_str(&format!(
        "Audio: host {}, device {}\n",
        or_none(&r.audio_host),
        or_none(&r.audio_device)
    ));
    if let Some(error) = &r.audio_error {
        out.push_str(&format!("Audio error: {error}\n"));
    }
    out.push_str(&format!("MIDI: {}\n", or_none(&r.midi_device)));
    if let Some(error) = &r.midi_error {
        out.push_str(&format!("MIDI error: {error}\n"));
    }
    out.push_str(&format!("Logs: {}\n", or_none(&r.paths.logs)));
    out.push_str(&format!("Database: {}\n", or_none(&r.paths.database)));
    for notice in &r.notices {
        out.push_str(&format!("Notice: {notice}\n"));
    }
    if r.recent_errors.is_empty() {
        out.push_str("Recent errors: none\n");
    } else {
        out.push_str("Recent errors:\n");
        for line in &r.recent_errors {
            out.push_str(&format!("  {line}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_lists_devices_and_errors() {
        let audio = AudioStatus {
            running: true,
            host: Some("ALSA".into()),
            device: Some("DDJ-FLX4".into()),
            ..AudioStatus::default()
        };
        let report = build_diagnostics(
            Some(&audio),
            None,
            AppPathsDto::new(
                Some(Path::new("/logs")),
                Some(Path::new("/data/library.db")),
                None,
            ),
            vec!["database replaced".into()],
            vec!["ERROR midi: port lost".into()],
        );
        assert!(report.text.contains("OpenDeck "));
        assert!(report.text.contains("host ALSA, device DDJ-FLX4"));
        assert!(report.text.contains("MIDI: none"));
        assert!(report.text.contains("Logs: /logs"));
        assert!(report.text.contains("Notice: database replaced"));
        assert!(report.text.contains("ERROR midi: port lost"));
    }

    #[test]
    fn report_without_problems_says_so() {
        let report = build_diagnostics(None, None, AppPathsDto::default(), vec![], vec![]);
        assert!(report.text.contains("Recent errors: none"));
        assert!(report.text.contains("Audio: host none, device none"));
    }
}
