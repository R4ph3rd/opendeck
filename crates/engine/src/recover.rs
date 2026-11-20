//! Opening the output robustly: readable error text, one retry and a fall
//! back to the system default output.
//!
//! Everything here is pure (the device is a closure), so the ordering of
//! open, retry and fallback is tested without hardware. The real device is
//! plugged in by [`crate::device::open_output_recovering`].
//!
//! Policy, in order:
//! 1. Open the configured output.
//! 2. On a failure that can be transient ([`AudioError::is_retryable`]: the
//!    device is busy or the stream would not build or start), wait
//!    [`RETRY_DELAY`] and open it once more. The failed attempt has already
//!    dropped its stream when its call returns, so the device is released
//!    before the retry (cpal's WASAPI `Stream::drop` joins the render thread).
//! 3. If it still fails and the configured device is not the system default,
//!    open the system default output with the host, device, rate and buffer
//!    left to the defaults. The caller learns why through [`FallbackInfo`].
//!
//! A device that is simply absent ([`AudioError::DeviceNotFound`]) is not
//! retried and does not fall back: app-core's reopen loop waits for it to be
//! plugged in again.
//!
//! HRESULT text: WASAPI errors reach us as a cpal error whose message ends in
//! `OS Error <signed hresult>`. [`hresult_in_message`] reads the code and
//! [`describe_failure`] turns it into text for the user. The code values are
//! the `AUDCLNT_E_*` constants of the Windows API (`windows` crate,
//! `Win32::Media::Audio`); their meaning is from Microsoft's "Core Audio
//! Error Codes" and `IAudioClient::Initialize` documentation.

use std::time::Duration;

use cpal::ErrorKind;
use opendeck_contracts::settings::AudioSettings;

use crate::device::AudioError;

/// Wait before the single retry of a failed open. Long enough for the audio
/// service to finish releasing a stream we just dropped (`TODO(verify)` on
/// Windows hardware), short enough that loading a track does not stall.
pub const RETRY_DELAY: Duration = Duration::from_millis(200);

/// `AUDCLNT_E_DEVICE_IN_USE`: the endpoint is held in exclusive mode.
pub const AUDCLNT_E_DEVICE_IN_USE: i32 = 0x8889_000A_u32 as i32;
const AUDCLNT_E_DEVICE_INVALIDATED: i32 = 0x8889_0004_u32 as i32;
const AUDCLNT_E_UNSUPPORTED_FORMAT: i32 = 0x8889_0008_u32 as i32;
const AUDCLNT_E_ENDPOINT_CREATE_FAILED: i32 = 0x8889_000F_u32 as i32;
const AUDCLNT_E_SERVICE_NOT_RUNNING: i32 = 0x8889_0010_u32 as i32;
const AUDCLNT_E_EXCLUSIVE_MODE_ONLY: i32 = 0x8889_0012_u32 as i32;
const AUDCLNT_E_BUFFER_SIZE_ERROR: i32 = 0x8889_0016_u32 as i32;
const AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED: i32 = 0x8889_0019_u32 as i32;
const AUDCLNT_E_INVALID_DEVICE_PERIOD: i32 = 0x8889_0020_u32 as i32;
const AUDCLNT_E_RESOURCES_INVALIDATED: i32 = 0x8889_0026_u32 as i32;

/// Which side of the device an error is about, for the wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Playback.
    Output,
    /// Capture (mic and aux).
    Input,
}

impl Direction {
    fn word(self) -> &'static str {
        match self {
            Self::Output => "output",
            Self::Input => "input",
        }
    }
}

/// The signed HRESULT a cpal WASAPI message carries, if any. The message
/// reads like `... OS Error -2004287478 (FormatMessageW() returned error 317)
/// (os error -2004287478)`.
pub fn hresult_in_message(message: &str) -> Option<i32> {
    let lower = message.to_ascii_lowercase();
    for marker in ["os error ", "os error: "] {
        let mut from = 0;
        while let Some(at) = lower[from..].find(marker) {
            let start = from + at + marker.len();
            let digits: String = lower[start..]
                .chars()
                .enumerate()
                .take_while(|(i, c)| c.is_ascii_digit() || (*i == 0 && *c == '-'))
                .map(|(_, c)| c)
                .collect();
            if let Ok(code) = digits.parse::<i64>()
                && let Ok(code) = i32::try_from(code)
            {
                return Some(code);
            }
            from = start;
        }
    }
    None
}

/// `0x8889000A`-style text of an HRESULT.
pub fn hresult_hex(code: i32) -> String {
    format!("0x{:08X}", code as u32)
}

/// The device is held by something else: cpal says busy, or the HRESULT is
/// `AUDCLNT_E_DEVICE_IN_USE`.
pub fn is_busy(kind: ErrorKind, message: &str) -> bool {
    kind == ErrorKind::DeviceBusy || hresult_in_message(message) == Some(AUDCLNT_E_DEVICE_IN_USE)
}

/// Text for the user for a backend failure. Known Windows codes and cpal
/// kinds get a sentence that says what to do; anything else keeps the
/// backend's own message.
pub fn describe_failure(kind: ErrorKind, message: &str, direction: Direction) -> String {
    let code = hresult_in_message(message);
    let what = direction.word();
    let text = match code {
        Some(AUDCLNT_E_DEVICE_IN_USE) => Some(busy_text(what)),
        Some(AUDCLNT_E_DEVICE_INVALIDATED) => Some(format!(
            "The {what} device was unplugged or disabled while it was being opened."
        )),
        Some(AUDCLNT_E_SERVICE_NOT_RUNNING) => Some(
            "The Windows Audio service is not running. Start it in Windows Services, then try again."
                .to_owned(),
        ),
        Some(
            AUDCLNT_E_UNSUPPORTED_FORMAT
            | AUDCLNT_E_BUFFER_SIZE_ERROR
            | AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED
            | AUDCLNT_E_INVALID_DEVICE_PERIOD,
        ) => Some(unsupported_text(what)),
        Some(AUDCLNT_E_EXCLUSIVE_MODE_ONLY) => Some(format!(
            "The {what} device only works in exclusive mode, which OpenDeck does not use. Pick another device."
        )),
        Some(AUDCLNT_E_ENDPOINT_CREATE_FAILED) => Some(format!(
            "Windows could not open the {what} device. Reconnect it and try again."
        )),
        Some(AUDCLNT_E_RESOURCES_INVALIDATED) => Some(format!(
            "The {what} device changed while it was being opened. Try again."
        )),
        _ => None,
    };
    let text = text.or_else(|| match kind {
        ErrorKind::DeviceBusy => Some(busy_text(what)),
        ErrorKind::DeviceNotAvailable => Some(format!(
            "The {what} device is not available. It may be unplugged or disabled."
        )),
        ErrorKind::HostUnavailable => {
            Some("The audio system is not available. Start the audio service and try again.".into())
        }
        ErrorKind::UnsupportedConfig => Some(unsupported_text(what)),
        ErrorKind::PermissionDenied => Some(format!(
            "The system denied access to the {what} device. Check the privacy and audio permissions."
        )),
        _ => None,
    });
    match (text, code) {
        (Some(text), Some(code)) => format!("{text} (code {})", hresult_hex(code)),
        (Some(text), None) => text,
        (None, _) => message.to_owned(),
    }
}

fn busy_text(what: &str) -> String {
    format!(
        "The {what} device is used exclusively by another application. Close other DJ or audio software, or turn off \"Allow applications to take exclusive control of this device\" in the Windows sound settings."
    )
}

fn unsupported_text(what: &str) -> String {
    format!(
        "The {what} device does not accept the requested sample rate, channels or buffer size. Use the device defaults in Settings."
    )
}

/// Why the engine runs on the default output instead of the chosen one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackInfo {
    /// Device the settings asked for (`None` when only the host was set).
    pub wanted: Option<String>,
    /// Why opening it failed, as text for the user.
    pub reason: String,
}

impl FallbackInfo {
    /// One notice for a toast: what failed, why, and what plays now.
    pub fn notice(&self, using: &str) -> String {
        let wanted = self
            .wanted
            .as_deref()
            .map_or_else(|| "the selected output".to_owned(), |d| format!("\"{d}\""));
        let reason = self.reason.trim_end_matches('.');
        format!(
            "Could not open {wanted}: {reason}. Playing on the system default output \"{using}\" instead. Pick a device in Settings, Audio to change it."
        )
    }
}

impl AudioError {
    /// A failure worth one more try after [`RETRY_DELAY`]: the device is busy
    /// or the stream would not build or start.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Busy(_) | Self::Build(_) | Self::Play(_))
    }

    /// The device exists but could not be opened, so the default output is a
    /// sensible substitute. A missing device is not (it may come back).
    pub fn allows_fallback(&self) -> bool {
        matches!(
            self,
            Self::Busy(_) | Self::Build(_) | Self::Play(_) | Self::NoConfig
        )
    }
}

/// Settings for the system default output, keeping the routing and inputs.
/// `None` when `settings` already name no host and no device, so there is
/// nothing else to try.
pub fn fallback_settings(settings: &AudioSettings) -> Option<AudioSettings> {
    if settings.host.is_none() && settings.device.is_none() {
        return None;
    }
    Some(AudioSettings {
        host: None,
        device: None,
        sample_rate: None,
        buffer_frames: None,
        ..settings.clone()
    })
}

/// Opens with `open`, retries once, then falls back to the default output.
/// `pause` waits between the attempt and its retry.
///
/// Returns the opened value and, when the fallback was used, why. When
/// everything fails the error of the first attempt is returned (it names the
/// device the user chose).
pub fn open_with_recovery<T>(
    settings: &AudioSettings,
    mut open: impl FnMut(&AudioSettings) -> Result<T, AudioError>,
    mut pause: impl FnMut(Duration),
) -> Result<(T, Option<FallbackInfo>), AudioError> {
    let mut error = match open(settings) {
        Ok(opened) => return Ok((opened, None)),
        Err(error) => error,
    };
    if error.is_retryable() {
        pause(RETRY_DELAY);
        match open(settings) {
            Ok(opened) => return Ok((opened, None)),
            Err(retry) => error = retry,
        }
    }
    if error.allows_fallback()
        && let Some(default) = fallback_settings(settings)
        && let Ok(opened) = open(&default)
    {
        let info = FallbackInfo {
            wanted: settings.device.clone(),
            reason: error.to_string(),
        };
        return Ok((opened, Some(info)));
    }
    Err(error)
}

/// Drops what `slot` holds, then runs `next`. The audio stream of the old
/// engine must be gone before the device is opened again: on WASAPI a stream
/// that is still alive keeps the endpoint client open. Use this wherever an
/// output is replaced.
pub fn release_then<T, R>(slot: &mut Option<T>, next: impl FnOnce() -> R) -> R {
    drop(slot.take());
    next()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use super::*;

    /// The exact text from a Windows report (`AUDCLNT_E_DEVICE_IN_USE`).
    const OWNER_MESSAGE: &str = "Failed to initialize audio client: OS Error -2004287478 (FormatMessageW() returned error 317) (os error -2004287478)";

    #[test]
    fn the_hresult_is_read_from_the_cpal_message() {
        let code = hresult_in_message(OWNER_MESSAGE).expect("code");
        assert_eq!(code, AUDCLNT_E_DEVICE_IN_USE);
        assert_eq!(hresult_hex(code), "0x8889000A");
        assert_eq!(hresult_in_message("no code here"), None);
        assert_eq!(hresult_in_message("os error "), None);
        assert_eq!(hresult_in_message("OS Error 5 (access)"), Some(5));
        assert_eq!(hresult_in_message("os error 99999999999"), None);
    }

    #[test]
    fn device_in_use_explains_exclusive_mode_and_keeps_the_code() {
        let text = describe_failure(ErrorKind::DeviceBusy, OWNER_MESSAGE, Direction::Output);
        assert!(
            text.contains("used exclusively by another application"),
            "{text}"
        );
        assert!(
            text.contains("Allow applications to take exclusive control"),
            "{text}"
        );
        assert!(text.contains("0x8889000A"), "{text}");
        assert!(!text.contains("FormatMessageW"), "{text}");
        assert!(!text.contains('\u{2014}'), "no em dash");
    }

    #[test]
    fn the_code_wins_over_a_vague_kind() {
        // cpal may report BackendError for a code it does not map.
        let text = describe_failure(ErrorKind::BackendError, OWNER_MESSAGE, Direction::Output);
        assert!(text.contains("used exclusively"), "{text}");
        assert!(is_busy(ErrorKind::BackendError, OWNER_MESSAGE));
        assert!(is_busy(ErrorKind::DeviceBusy, "EBUSY"));
        assert!(!is_busy(ErrorKind::BackendError, "something else"));
    }

    #[test]
    fn known_codes_and_kinds_get_actionable_text() {
        let cases = [
            ("os error -2004287484", "unplugged or disabled"),
            ("os error -2004287472", "Windows Audio service"),
            ("os error -2004287480", "does not accept"),
            ("os error -2004287463", "does not accept"),
            ("os error -2004287470", "exclusive mode"),
        ];
        for (message, expect) in cases {
            let text = describe_failure(ErrorKind::BackendError, message, Direction::Output);
            assert!(text.contains(expect), "{message}: {text}");
            assert!(text.contains("(code 0x"), "{message}: {text}");
        }
        let kind = describe_failure(ErrorKind::DeviceNotAvailable, "gone", Direction::Input);
        assert!(
            kind.starts_with("The input device is not available"),
            "{kind}"
        );
        let busy = describe_failure(ErrorKind::DeviceBusy, "EBUSY", Direction::Output);
        assert!(busy.contains("another application"), "{busy}");
        assert!(
            !busy.contains("(code"),
            "no code on other platforms: {busy}"
        );
    }

    #[test]
    fn unknown_failures_keep_the_backend_message() {
        assert_eq!(
            describe_failure(ErrorKind::BackendError, "weird failure", Direction::Output),
            "weird failure"
        );
        assert_eq!(
            describe_failure(ErrorKind::BackendError, "os error 1234", Direction::Output),
            "os error 1234"
        );
    }

    #[test]
    fn retry_and_fallback_rules() {
        assert!(AudioError::Busy("b".into()).is_retryable());
        assert!(AudioError::Build("b".into()).is_retryable());
        assert!(AudioError::Play("p".into()).is_retryable());
        assert!(!AudioError::DeviceNotFound("x".into()).is_retryable());
        assert!(!AudioError::NoDevice.is_retryable());
        assert!(AudioError::NoConfig.allows_fallback());
        assert!(!AudioError::NoConfig.is_retryable());
        assert!(!AudioError::DeviceNotFound("x".into()).allows_fallback());
        assert!(!AudioError::HostUnavailable("x".into()).allows_fallback());
    }

    #[test]
    fn fallback_settings_use_the_default_device_and_keep_routing() {
        let chosen = AudioSettings {
            device: Some("FLX4".into()),
            sample_rate: Some(44_100),
            buffer_frames: Some(128),
            ..AudioSettings::default()
        };
        let fallback = fallback_settings(&chosen).expect("a fallback");
        assert_eq!(fallback.device, None);
        assert_eq!(fallback.host, None);
        assert_eq!(fallback.sample_rate, None);
        assert_eq!(fallback.buffer_frames, None);
        assert_eq!(fallback.headphones, chosen.headphones);
        assert_eq!(fallback.master, chosen.master);
        assert_eq!(fallback_settings(&AudioSettings::default()), None);
    }

    // ----- ordering with a fake device -----

    type Log = Rc<RefCell<Vec<String>>>;

    /// A stream that logs when it is dropped.
    struct FakeStream {
        name: String,
        log: Log,
    }

    impl Drop for FakeStream {
        fn drop(&mut self) {
            self.log.borrow_mut().push(format!("drop {}", self.name));
        }
    }

    /// Scripted device: each `open` pops the next outcome. A `Err` outcome
    /// first builds a stream and drops it, like `open_output` does when
    /// `play` fails after `build`.
    struct FakeDevice {
        log: Log,
        outcomes: VecDeque<Result<(), AudioError>>,
    }

    impl FakeDevice {
        fn new(outcomes: impl IntoIterator<Item = Result<(), AudioError>>) -> (Self, Log) {
            let log: Log = Rc::default();
            (
                Self {
                    log: Rc::clone(&log),
                    outcomes: outcomes.into_iter().collect(),
                },
                log,
            )
        }

        fn open(&mut self, settings: &AudioSettings) -> Result<FakeStream, AudioError> {
            let name = settings.device.clone().unwrap_or_else(|| "default".into());
            self.log.borrow_mut().push(format!("open {name}"));
            let stream = FakeStream {
                name,
                log: Rc::clone(&self.log),
            };
            self.outcomes
                .pop_front()
                .expect("scripted outcome")
                .map(|()| stream)
        }
    }

    fn flx4() -> AudioSettings {
        AudioSettings {
            device: Some("FLX4".into()),
            ..AudioSettings::default()
        }
    }

    fn busy() -> AudioError {
        AudioError::Busy("busy".into())
    }

    fn run(
        settings: &AudioSettings,
        device: &mut FakeDevice,
        log: &Log,
    ) -> Result<(FakeStream, Option<FallbackInfo>), AudioError> {
        let pause_log = Rc::clone(log);
        open_with_recovery(
            settings,
            |s| device.open(s),
            move |d| {
                pause_log
                    .borrow_mut()
                    .push(format!("pause {}ms", d.as_millis()))
            },
        )
    }

    #[test]
    fn a_working_device_opens_once_without_waiting() {
        let (mut device, log) = FakeDevice::new([Ok(())]);
        let (stream, fallback) = run(&flx4(), &mut device, &log).expect("opens");
        assert_eq!(stream.name, "FLX4");
        assert_eq!(fallback, None);
        assert_eq!(*log.borrow(), ["open FLX4"]);
    }

    #[test]
    fn a_busy_device_is_retried_once_after_the_first_stream_is_gone() {
        let (mut device, log) = FakeDevice::new([Err(busy()), Ok(())]);
        let (_stream, fallback) = run(&flx4(), &mut device, &log).expect("retry works");
        assert_eq!(fallback, None, "the retry succeeded, no fallback");
        assert_eq!(
            *log.borrow(),
            ["open FLX4", "drop FLX4", "pause 200ms", "open FLX4",],
            "the failed stream is dropped before the pause and the retry"
        );
    }

    #[test]
    fn a_device_that_stays_busy_falls_back_to_the_default_output() {
        let (mut device, log) = FakeDevice::new([Err(busy()), Err(busy()), Ok(())]);
        let (stream, fallback) = run(&flx4(), &mut device, &log).expect("fallback opens");
        assert_eq!(stream.name, "default");
        let info = fallback.expect("fallback reported");
        assert_eq!(info.wanted.as_deref(), Some("FLX4"));
        assert_eq!(info.reason, "busy");
        assert_eq!(
            *log.borrow(),
            [
                "open FLX4",
                "drop FLX4",
                "pause 200ms",
                "open FLX4",
                "drop FLX4",
                "open default",
            ]
        );
        let notice = info.notice("Speakers");
        assert!(notice.contains("\"FLX4\""), "{notice}");
        assert!(notice.contains("\"Speakers\""), "{notice}");
        assert!(notice.contains("Settings"), "{notice}");
    }

    #[test]
    fn a_missing_device_is_neither_retried_nor_replaced() {
        let (mut device, log) = FakeDevice::new([Err(AudioError::DeviceNotFound("FLX4".into()))]);
        let error = run(&flx4(), &mut device, &log).err().expect("fails");
        assert_eq!(error, AudioError::DeviceNotFound("FLX4".into()));
        assert_eq!(
            *log.borrow(),
            ["open FLX4", "drop FLX4"],
            "one open, no pause"
        );
    }

    #[test]
    fn the_default_device_has_nowhere_to_fall_back_to() {
        let (mut device, log) = FakeDevice::new([Err(busy()), Err(busy())]);
        let error = run(&AudioSettings::default(), &mut device, &log)
            .err()
            .expect("fails");
        assert_eq!(error, busy());
        assert_eq!(
            *log.borrow(),
            [
                "open default",
                "drop default",
                "pause 200ms",
                "open default",
                "drop default"
            ]
        );
    }

    #[test]
    fn when_the_fallback_fails_too_the_first_error_is_reported() {
        let first = AudioError::Busy("flx4 is busy".into());
        let (mut device, log) = FakeDevice::new([
            Err(first.clone()),
            Err(first.clone()),
            Err(AudioError::NoConfig),
        ]);
        let error = run(&flx4(), &mut device, &log).err().expect("fails");
        assert_eq!(error, first, "names the device the user chose");
    }

    #[test]
    fn a_no_config_device_falls_back_without_a_retry() {
        let (mut device, log) = FakeDevice::new([Err(AudioError::NoConfig), Ok(())]);
        let (stream, fallback) = run(&flx4(), &mut device, &log).expect("fallback opens");
        assert_eq!(stream.name, "default");
        assert!(fallback.is_some());
        assert!(!log.borrow().iter().any(|e| e.starts_with("pause")));
    }

    #[test]
    fn the_old_output_is_released_before_the_next_open() {
        let log: Log = Rc::default();
        let mut slot = Some(FakeStream {
            name: "old".into(),
            log: Rc::clone(&log),
        });
        let next_log = Rc::clone(&log);
        let opened = release_then(&mut slot, || {
            next_log.borrow_mut().push("open new".into());
            1
        });
        assert_eq!(opened, 1);
        assert!(slot.is_none());
        assert_eq!(*log.borrow(), ["drop old", "open new"]);
    }
}
