//! Input and feedback engines of one mapping, wired together.
//!
//! [`Pipeline`] is the whole controller logic without any I/O: bytes in,
//! [`ControlEvent`]s out; state in, bytes out. The MIDI worker thread and the
//! replay harness both drive it, so what the tests prove is what runs.

use opendeck_contracts::control::{ControlEvent, FeedbackTarget, FeedbackValue};

use crate::error::MappingError;
use crate::feedback::{FeedbackConfig, FeedbackEngine, OutMessage};
use crate::input::{Handled, MappingEngine};
use crate::mapping::Mapping;

/// Input engine plus feedback engine for one mapping.
#[derive(Debug, Clone)]
pub struct Pipeline {
    input: MappingEngine,
    feedback: FeedbackEngine,
}

impl Pipeline {
    /// Builds both engines. Validates the mapping.
    pub fn new(mapping: Mapping, config: FeedbackConfig) -> Result<Self, MappingError> {
        let feedback = FeedbackEngine::new(&mapping, config);
        Ok(Self {
            input: MappingEngine::new(mapping)?,
            feedback,
        })
    }

    /// The mapping.
    pub fn mapping(&self) -> &Mapping {
        self.input.mapping()
    }

    /// The input engine.
    pub fn input(&self) -> &MappingEngine {
        &self.input
    }

    /// Mutable access to the input engine (e.g. to change the MSB timeout).
    pub fn input_mut(&mut self) -> &mut MappingEngine {
        &mut self.input
    }

    /// The feedback engine.
    pub fn feedback(&self) -> &FeedbackEngine {
        &self.feedback
    }

    /// Processes one received message. A modifier change also re-evaluates
    /// shift-layer LEDs, so `out` can receive messages here.
    pub fn on_midi(
        &mut self,
        ts_us: u64,
        bytes: &[u8],
        events: &mut Vec<ControlEvent>,
        out: &mut Vec<OutMessage>,
    ) -> Handled {
        let handled = self.input.process(ts_us, bytes, events);
        if matches!(handled, Handled::Modifier { .. }) {
            self.feedback
                .set_modifiers(self.input.modifier_states(), ts_us, out);
        }
        handled
    }

    /// Sets an application state and appends the messages that are due.
    pub fn set_feedback(
        &mut self,
        target: FeedbackTarget,
        value: FeedbackValue,
        now_us: u64,
        out: &mut Vec<OutMessage>,
    ) {
        self.feedback.set(target, value, now_us, out);
    }

    /// Runs both engines' timers.
    pub fn tick(&mut self, now_us: u64, events: &mut Vec<ControlEvent>, out: &mut Vec<OutMessage>) {
        self.input.tick(now_us, events);
        self.feedback.tick(now_us, out);
    }

    /// Earliest time at which [`Pipeline::tick`] has work, if any.
    pub fn next_deadline_us(&self) -> Option<u64> {
        match (
            self.input.next_deadline_us(),
            self.feedback.next_deadline_us(),
        ) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// The mapping's `on_connect` messages, to send when the output opens.
    pub fn connect_messages(&self) -> Vec<OutMessage> {
        self.mapping()
            .on_connect
            .iter()
            .map(|m| OutMessage {
                bytes: m.bytes.clone(),
            })
            .collect()
    }

    /// Resends all cached LED state (after an output reconnect).
    pub fn resend_all(&mut self, now_us: u64, out: &mut Vec<OutMessage>) {
        self.feedback.resend_all(now_us, out);
    }
}
