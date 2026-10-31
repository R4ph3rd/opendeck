//! File logging with `tracing` and a short memory of recent problems.
//!
//! Logs go to a daily rolling file in the app log directory through
//! `tracing-appender`'s non-blocking writer, so a log call only pushes onto a
//! channel. The audio thread never logs (ADR 0003); this is for the control,
//! loader, MIDI and shell threads. Warnings and errors are also kept in a
//! small ring for the diagnostics report.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

/// File name prefix of the log files (`opendeck.<date>.log`).
pub const LOG_PREFIX: &str = "opendeck";
/// Daily files kept before the oldest is removed.
pub const LOG_FILES_KEPT: usize = 7;
/// Problems remembered for the diagnostics report.
pub const RECENT_ERRORS: usize = 20;

static RECENT: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();

fn recent() -> &'static Mutex<VecDeque<String>> {
    RECENT.get_or_init(|| Mutex::new(VecDeque::with_capacity(RECENT_ERRORS)))
}

/// Remembers a problem for the diagnostics report.
pub fn record_error(message: impl Into<String>) {
    if let Ok(mut ring) = recent().lock() {
        if ring.len() == RECENT_ERRORS {
            ring.pop_front();
        }
        ring.push_back(message.into());
    }
}

/// The last warnings, errors and panics, oldest first.
pub fn recent_errors() -> Vec<String> {
    recent()
        .lock()
        .map(|ring| ring.iter().cloned().collect())
        .unwrap_or_default()
}

/// Keeps the log writer alive; dropping it flushes and stops the writer thread.
pub struct LogGuard {
    _worker: WorkerGuard,
    dir: PathBuf,
}

impl LogGuard {
    /// Directory the log files are written to.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.fields, " {}={value:?}", field.name());
        }
    }
}

/// Copies warnings and errors into the ring.
struct RecentLayer;

impl<S: Subscriber> Layer<S> for RecentLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = *event.metadata().level();
        if level > Level::WARN {
            return;
        }
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        record_error(format!(
            "{level} {}: {}{}",
            event.metadata().target(),
            visitor.message,
            visitor.fields
        ));
    }
}

/// Starts logging to `dir` (created if missing). `RUST_LOG` overrides the
/// default `info` filter. Safe to call once; a second call returns an error.
pub fn init(dir: &Path) -> Result<LogGuard, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_PREFIX)
        .filename_suffix("log")
        .max_log_files(LOG_FILES_KEPT)
        .build(dir)
        .map_err(|e| format!("open log file: {e}"))?;
    let (writer, worker) = tracing_appender::non_blocking(appender);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false),
        )
        .with(RecentLayer)
        .try_init()
        .map_err(|e| format!("install logger: {e}"))?;
    tracing::info!(version = crate::VERSION, "OpenDeck started");
    Ok(LogGuard {
        _worker: worker,
        dir: dir.to_path_buf(),
    })
}

/// Text of a panic payload.
fn panic_text(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned());
    match info.location() {
        Some(l) => format!("{message} at {}:{}", l.file(), l.line()),
        None => message,
    }
}

/// Makes panics write to the log and the recent-problems ring, then run the
/// previous hook (which prints to stderr).
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = panic_text(info);
        tracing::error!(panic = %text, "panic");
        // `tracing::error!` already reached the ring when a logger is installed.
        if tracing::dispatcher::has_been_set() {
            // The non-blocking writer flushes on a worker thread; give it a moment.
            std::thread::sleep(std::time::Duration::from_millis(50));
        } else {
            record_error(format!("PANIC {text}"));
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_the_newest_entries() {
        for i in 0..(RECENT_ERRORS + 5) {
            record_error(format!("problem {i}"));
        }
        let errors = recent_errors();
        assert!(errors.len() <= RECENT_ERRORS);
        assert_eq!(
            errors.last().map(String::as_str),
            Some(format!("problem {}", RECENT_ERRORS + 4).as_str())
        );
    }

    #[test]
    fn init_writes_a_log_file() {
        let dir = std::env::temp_dir().join(format!("opendeck-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let guard = init(&dir).expect("init");
        tracing::warn!("test warning");
        assert_eq!(guard.dir(), dir.as_path());
        assert!(recent_errors().iter().any(|e| e.contains("test warning")));
        drop(guard);
        let files: Vec<_> = std::fs::read_dir(&dir).expect("dir").flatten().collect();
        assert!(!files.is_empty());
        let text = std::fs::read_to_string(files[0].path()).expect("read");
        assert!(text.contains("OpenDeck started"));
    }
}
