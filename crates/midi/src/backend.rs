//! MIDI port access behind a small trait, with the `midir` implementation and
//! a mock for tests.
//!
//! The trait exists so hot-plug, reconnect and feedback behaviour can be
//! tested without hardware. The controller worker creates its backend on its
//! own thread (the factory is the only thing that crosses threads), so
//! connection handles never need to be `Send`.

use std::any::Any;
use std::sync::{Arc, Mutex};

use midir::{Ignore, MidiInput, MidiOutput, MidiOutputConnection};

use crate::error::MidiError;

/// A MIDI port as the backend names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortInfo {
    /// Backend id; stable while the device stays connected.
    pub id: String,
    /// Display name, matched against the mapping's device patterns.
    pub name: String,
}

/// Receives `(driver timestamp in microseconds, message bytes)` on the
/// backend's input thread. It must not block.
pub type RawSink = Box<dyn FnMut(u64, &[u8]) + Send + 'static>;

/// An open input. Dropping it closes the port.
pub type InputHandle = Box<dyn Any>;

/// An open output.
pub trait OutputPort {
    /// Sends one message.
    fn send(&mut self, bytes: &[u8]) -> Result<(), MidiError>;
}

/// Port discovery and opening.
pub trait MidiBackend {
    /// Current input ports.
    fn input_ports(&mut self) -> Result<Vec<PortInfo>, MidiError>;
    /// Current output ports.
    fn output_ports(&mut self) -> Result<Vec<PortInfo>, MidiError>;
    /// Opens an input; `sink` runs on the backend's thread for each message.
    fn open_input(&mut self, port: &PortInfo, sink: RawSink) -> Result<InputHandle, MidiError>;
    /// Opens an output.
    fn open_output(&mut self, port: &PortInfo) -> Result<Box<dyn OutputPort>, MidiError>;
}

/// Lists the input ports of the system MIDI backend.
pub fn list_input_ports() -> Result<Vec<PortInfo>, MidiError> {
    MidirBackend::default().input_ports()
}

/// Lists the output ports of the system MIDI backend.
pub fn list_output_ports() -> Result<Vec<PortInfo>, MidiError> {
    MidirBackend::default().output_ports()
}

/// Case-insensitive substring match of a port name against mapping patterns.
pub fn matches_any(name: &str, patterns: &[String]) -> bool {
    let name = name.to_lowercase();
    patterns
        .iter()
        .any(|p| !p.is_empty() && name.contains(&p.to_lowercase()))
}

/// The `midir` backend (ALSA, CoreMIDI, WinMM).
#[derive(Debug, Clone)]
pub struct MidirBackend {
    client_name: String,
}

impl Default for MidirBackend {
    fn default() -> Self {
        Self::new("OpenDeck")
    }
}

impl MidirBackend {
    /// A backend whose midir clients are named `client_name`.
    pub fn new(client_name: &str) -> Self {
        Self {
            client_name: client_name.to_owned(),
        }
    }

    fn input(&self) -> Result<MidiInput, MidiError> {
        let mut input =
            MidiInput::new(&self.client_name).map_err(|e| MidiError::Backend(e.to_string()))?;
        // Clock and active sensing would flood the worker; mappings use neither.
        input.ignore(Ignore::TimeAndActiveSense);
        Ok(input)
    }

    fn output(&self) -> Result<MidiOutput, MidiError> {
        MidiOutput::new(&self.client_name).map_err(|e| MidiError::Backend(e.to_string()))
    }
}

struct MidirOutput(MidiOutputConnection);

impl OutputPort for MidirOutput {
    fn send(&mut self, bytes: &[u8]) -> Result<(), MidiError> {
        self.0
            .send(bytes)
            .map_err(|e| MidiError::Send(e.to_string()))
    }
}

impl MidiBackend for MidirBackend {
    fn input_ports(&mut self) -> Result<Vec<PortInfo>, MidiError> {
        let input = self.input()?;
        Ok(input
            .ports()
            .iter()
            .filter_map(|p| {
                Some(PortInfo {
                    id: p.id(),
                    name: input.port_name(p).ok()?,
                })
            })
            .collect())
    }

    fn output_ports(&mut self) -> Result<Vec<PortInfo>, MidiError> {
        let output = self.output()?;
        Ok(output
            .ports()
            .iter()
            .filter_map(|p| {
                Some(PortInfo {
                    id: p.id(),
                    name: output.port_name(p).ok()?,
                })
            })
            .collect())
    }

    fn open_input(&mut self, port: &PortInfo, mut sink: RawSink) -> Result<InputHandle, MidiError> {
        let input = self.input()?;
        let found = input
            .find_port_by_id(&port.id)
            .ok_or_else(|| MidiError::PortNotFound(port.name.clone()))?;
        let connection = input
            .connect(
                &found,
                "opendeck-in",
                move |ts, bytes, ()| sink(ts, bytes),
                (),
            )
            .map_err(|e| MidiError::Open {
                port: port.name.clone(),
                reason: e.to_string(),
            })?;
        Ok(Box::new(connection))
    }

    fn open_output(&mut self, port: &PortInfo) -> Result<Box<dyn OutputPort>, MidiError> {
        let output = self.output()?;
        let found = output
            .find_port_by_id(&port.id)
            .ok_or_else(|| MidiError::PortNotFound(port.name.clone()))?;
        let connection = output
            .connect(&found, "opendeck-out")
            .map_err(|e| MidiError::Open {
                port: port.name.clone(),
                reason: e.to_string(),
            })?;
        Ok(Box::new(MidirOutput(connection)))
    }
}

/// In-memory backend for tests: ports can appear and vanish, input is pushed
/// by hand and output is recorded.
pub mod mock {
    use super::*;

    #[derive(Default)]
    struct State {
        inputs: Vec<PortInfo>,
        outputs: Vec<PortInfo>,
        sinks: Vec<(String, RawSink)>,
        sent: Vec<(String, Vec<u8>)>,
        fail_open: bool,
    }

    /// Test control over a [`MockBackend`]; cloneable and thread-safe.
    #[derive(Clone, Default)]
    pub struct MockHandle {
        state: Arc<Mutex<State>>,
    }

    impl MockHandle {
        fn lock(&self) -> std::sync::MutexGuard<'_, State> {
            self.state.lock().unwrap_or_else(|e| e.into_inner())
        }

        /// A backend that shares this handle's state.
        pub fn backend(&self) -> MockBackend {
            MockBackend {
                handle: self.clone(),
            }
        }

        /// Plugs in a device with an input and an output port of this name.
        pub fn plug(&self, name: &str) {
            let mut s = self.lock();
            let port = PortInfo {
                id: name.to_owned(),
                name: name.to_owned(),
            };
            if !s.inputs.contains(&port) {
                s.inputs.push(port.clone());
            }
            if !s.outputs.contains(&port) {
                s.outputs.push(port);
            }
        }

        /// Unplugs it: ports vanish and open inputs stop receiving.
        pub fn unplug(&self, name: &str) {
            let mut s = self.lock();
            s.inputs.retain(|p| p.name != name);
            s.outputs.retain(|p| p.name != name);
            s.sinks.retain(|(n, _)| n != name);
        }

        /// Makes the next `open_*` calls fail (or succeed again).
        pub fn set_fail_open(&self, fail: bool) {
            self.lock().fail_open = fail;
        }

        /// Delivers a message to the open input of `name`. Returns false if
        /// that input is not open.
        pub fn push(&self, name: &str, ts_us: u64, bytes: &[u8]) -> bool {
            let mut s = self.lock();
            match s.sinks.iter_mut().find(|(n, _)| n == name) {
                Some((_, sink)) => {
                    sink(ts_us, bytes);
                    true
                }
                None => false,
            }
        }

        /// Messages sent to outputs so far, as `(port name, bytes)`.
        pub fn sent(&self) -> Vec<(String, Vec<u8>)> {
            self.lock().sent.clone()
        }

        /// Forgets recorded output.
        pub fn clear_sent(&self) {
            self.lock().sent.clear();
        }

        /// True while an input of that name is open.
        pub fn input_open(&self, name: &str) -> bool {
            self.lock().sinks.iter().any(|(n, _)| n == name)
        }
    }

    /// See the module docs.
    pub struct MockBackend {
        handle: MockHandle,
    }

    /// Closes the mock input when dropped, like a real connection.
    struct MockInput {
        name: String,
        handle: MockHandle,
    }

    impl Drop for MockInput {
        fn drop(&mut self) {
            self.handle.lock().sinks.retain(|(n, _)| n != &self.name);
        }
    }

    struct MockOutput {
        name: String,
        handle: MockHandle,
    }

    impl OutputPort for MockOutput {
        fn send(&mut self, bytes: &[u8]) -> Result<(), MidiError> {
            let mut s = self.handle.lock();
            if !s.outputs.iter().any(|p| p.name == self.name) {
                return Err(MidiError::Send("port is gone".into()));
            }
            s.sent.push((self.name.clone(), bytes.to_vec()));
            Ok(())
        }
    }

    impl MidiBackend for MockBackend {
        fn input_ports(&mut self) -> Result<Vec<PortInfo>, MidiError> {
            Ok(self.handle.lock().inputs.clone())
        }

        fn output_ports(&mut self) -> Result<Vec<PortInfo>, MidiError> {
            Ok(self.handle.lock().outputs.clone())
        }

        fn open_input(&mut self, port: &PortInfo, sink: RawSink) -> Result<InputHandle, MidiError> {
            let mut s = self.handle.lock();
            if s.fail_open || !s.inputs.contains(port) {
                return Err(MidiError::Open {
                    port: port.name.clone(),
                    reason: "mock open failure".into(),
                });
            }
            s.sinks.push((port.name.clone(), sink));
            Ok(Box::new(MockInput {
                name: port.name.clone(),
                handle: self.handle.clone(),
            }))
        }

        fn open_output(&mut self, port: &PortInfo) -> Result<Box<dyn OutputPort>, MidiError> {
            let s = self.handle.lock();
            if s.fail_open || !s.outputs.contains(port) {
                return Err(MidiError::Open {
                    port: port.name.clone(),
                    reason: "mock open failure".into(),
                });
            }
            Ok(Box::new(MockOutput {
                name: port.name.clone(),
                handle: self.handle.clone(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_match_is_case_insensitive_substring() {
        let pats = vec!["DDJ-FLX4".to_owned()];
        assert!(matches_any("DDJ-FLX4:DDJ-FLX4 MIDI 1 20:0", &pats));
        assert!(matches_any("ddj-flx4", &pats));
        assert!(!matches_any("DDJ-400", &pats));
        assert!(!matches_any("anything", &[String::new()]));
    }

    #[test]
    fn listing_ports_does_not_panic_without_hardware() {
        // CI machines have no MIDI devices and may have no sequencer; both
        // outcomes are fine, panicking is not.
        let _ = list_input_ports();
        let _ = list_output_ports();
    }
}
