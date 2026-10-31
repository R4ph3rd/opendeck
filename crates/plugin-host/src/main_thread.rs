//! The plugin main thread.
//!
//! CLAP `[main-thread]` functions and VST3 controller and UI calls must run
//! on one thread, and editors need it to be the platform UI thread (Win32
//! window owner thread, Cocoa main thread, X11 event thread). The app
//! provides that thread through [`MainThread`]: src-tauri implements it with
//! Tauri's `run_on_main_thread`; tests, the scanner and offline tools use
//! [`ThreadMainThread`].
//!
//! Plugin instances never leave that thread: they live in a thread-local
//! table (`instances`), and other threads reach them by queuing jobs.
//!
//! Deadlock rule: the main thread must never block waiting for the control
//! thread while the control thread waits in [`run_blocking`]. Every wait has
//! a timeout ([`DEFAULT_TIMEOUT`]).

use std::sync::mpsc;
use std::thread::{self, JoinHandle, ThreadId};
use std::time::Duration;

use crate::PluginError;

/// A job for the main thread.
pub type MainJob = Box<dyn FnOnce() + Send + 'static>;

/// Default wait for a main-thread job (instantiate, activate, state).
/// Product choice: long enough for slow plugin loads, short enough that a
/// hung plugin does not freeze the control thread for long.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs jobs on the plugin main thread.
///
/// Invariants: every job runs on the same OS thread, in submission order,
/// at most once. [`MainThread::is_main_thread`] is true exactly on that
/// thread.
pub trait MainThread: Send + Sync {
    /// Queues `job`. Returns at once; fails only if the executor is gone.
    fn spawn(&self, job: MainJob) -> Result<(), PluginError>;

    /// True when called on the main thread.
    fn is_main_thread(&self) -> bool;
}

/// Runs `f` on the main thread and waits up to `timeout` for its result.
/// Runs inline when already on the main thread. Never call from the audio
/// thread.
pub fn run_blocking<T, F>(main: &dyn MainThread, timeout: Duration, f: F) -> Result<T, PluginError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    if main.is_main_thread() {
        return Ok(f());
    }
    let (tx, rx) = mpsc::sync_channel(1);
    main.spawn(Box::new(move || {
        // The receiver is gone after a timeout; the value drops here.
        let _ = tx.send(f());
    }))?;
    rx.recv_timeout(timeout).map_err(|e| match e {
        mpsc::RecvTimeoutError::Timeout => PluginError::MainThreadTimeout(timeout),
        mpsc::RecvTimeoutError::Disconnected => PluginError::MainThreadGone,
    })
}

/// A dedicated thread acting as the plugin main thread. For tests, the
/// scanner and offline rendering; the app uses the platform UI thread.
/// Dropping it runs the queued jobs, then joins the thread.
pub struct ThreadMainThread {
    tx: Option<mpsc::Sender<MainJob>>,
    id: ThreadId,
    thread: Option<JoinHandle<()>>,
}

impl ThreadMainThread {
    /// Starts the thread.
    pub fn new(name: &str) -> Result<Self, PluginError> {
        let (tx, rx) = mpsc::channel::<MainJob>();
        let thread = thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                for job in rx {
                    job();
                }
            })
            .map_err(|e| PluginError::Scanner(format!("main thread: {e}")))?;
        Ok(Self {
            tx: Some(tx),
            id: thread.thread().id(),
            thread: Some(thread),
        })
    }
}

impl MainThread for ThreadMainThread {
    fn spawn(&self, job: MainJob) -> Result<(), PluginError> {
        self.tx
            .as_ref()
            .ok_or(PluginError::MainThreadGone)?
            .send(job)
            .map_err(|_| PluginError::MainThreadGone)
    }

    fn is_main_thread(&self) -> bool {
        thread::current().id() == self.id
    }
}

impl Drop for ThreadMainThread {
    fn drop(&mut self) {
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_run_in_order_on_one_thread() {
        let main = ThreadMainThread::new("test-main").expect("thread");
        assert!(!main.is_main_thread());
        let a = run_blocking(&main, DEFAULT_TIMEOUT, || thread::current().id()).expect("a");
        let b = run_blocking(&main, DEFAULT_TIMEOUT, || thread::current().id()).expect("b");
        assert_eq!(a, b);
        assert_ne!(a, thread::current().id());
    }

    #[test]
    fn a_hung_job_times_out() {
        let main = ThreadMainThread::new("test-main").expect("thread");
        let short = Duration::from_millis(20);
        let r = run_blocking(&main, short, || thread::sleep(Duration::from_millis(200)));
        assert_eq!(r, Err(PluginError::MainThreadTimeout(short)));
    }
}
