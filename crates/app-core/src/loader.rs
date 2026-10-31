//! The loader thread: decodes files at the engine rate, off the audio and UI
//! threads (ADR 0003). One request at a time; a newer request for a deck
//! cancels the older one through its [`CancelFlag`].

use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use opendeck_contracts::analysis::CancelFlag;
use opendeck_contracts::{DeckId, SampleRate};
use opendeck_engine::Reclaimed;
use opendeck_media::{MediaError, decode_file};

use crate::control::Msg;
use crate::types::{CoreError, TrackInfo};

/// Decode `path` for `deck` at `rate`.
pub(crate) struct LoadRequest {
    pub(crate) deck: DeckId,
    /// Matches the control thread's counter; stale results are dropped there.
    pub(crate) generation: u64,
    pub(crate) path: PathBuf,
    pub(crate) info: TrackInfo,
    pub(crate) rate: SampleRate,
    pub(crate) cancel: CancelFlag,
}

/// What the loader found.
pub(crate) struct DecodeResult {
    pub(crate) deck: DeckId,
    pub(crate) generation: u64,
    /// Track info with the duration measured from the decoded audio.
    pub(crate) info: TrackInfo,
    /// Interleaved stereo samples at the request rate.
    pub(crate) outcome: Result<Vec<f32>, CoreError>,
}

/// Work for the loader thread.
pub(crate) enum LoaderJob {
    /// Decode a file.
    Load(Box<LoadRequest>),
    /// Drop a buffer or effect the audio thread gave back, so a large free
    /// never runs on the control thread (ADR 0003).
    Free(Reclaimed),
}

pub(crate) fn spawn(
    jobs: Receiver<LoaderJob>,
    results: Sender<Msg>,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("opendeck-loader".into())
        .spawn(move || {
            while let Ok(job) = jobs.recv() {
                match job {
                    LoaderJob::Load(request) => {
                        let request = *request;
                        let result = run(request);
                        if results.send(Msg::Decoded(result)).is_err() {
                            break;
                        }
                    }
                    LoaderJob::Free(old) => {
                        drop(old);
                        #[cfg(test)]
                        FREED.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        })
}

/// Buffers and effects the loader thread has dropped since the process
/// started. Lets tests check that frees happen off the control thread.
#[cfg(test)]
pub(crate) static FREED: AtomicUsize = AtomicUsize::new(0);

fn run(request: LoadRequest) -> DecodeResult {
    let LoadRequest {
        deck,
        generation,
        path,
        mut info,
        rate,
        cancel,
    } = request;
    let outcome = if cancel.is_cancelled() {
        Err(CoreError::Cancelled)
    } else {
        match decode_file(&path, rate, &cancel) {
            Ok(audio) => {
                info.duration_secs = audio.frames as f64 / f64::from(rate.0);
                Ok(audio.samples)
            }
            Err(MediaError::Cancelled) => Err(CoreError::Cancelled),
            Err(e) => Err(CoreError::Media(e.to_string())),
        }
    };
    DecodeResult {
        deck,
        generation,
        info,
        outcome,
    }
}
