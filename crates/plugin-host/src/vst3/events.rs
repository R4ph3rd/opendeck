//! Fixed-capacity `IEventList` for note input and output.

use std::cell::Cell;

use opendeck_contracts::instrument::{MAX_NOTE_EVENTS, NoteEvent, NoteKind};
use vst3::Steinberg::Vst::{Event, Event_, IEventList, IEventListTrait, NoteOffEvent, NoteOnEvent};
use vst3::Steinberg::{int32, kInvalidArgument, kResultFalse, kResultOk, tresult};
use vst3::{Class, ComWrapper};

/// A list of at most [`MAX_NOTE_EVENTS`] events, allocated up front.
pub(super) struct EventList {
    events: Vec<Cell<Event>>,
    len: Cell<usize>,
}

impl Class for EventList {
    type Interfaces = (IEventList,);
}

fn blank() -> Event {
    // SAFETY: `Event` is a plain `repr(C)` struct of integers, floats and a
    // union of such structs; all-zero is a valid value.
    unsafe { std::mem::zeroed() }
}

impl EventList {
    /// Allocates the list. Control or main thread.
    pub(super) fn new() -> ComWrapper<EventList> {
        ComWrapper::new(EventList {
            events: (0..MAX_NOTE_EVENTS).map(|_| Cell::new(blank())).collect(),
            len: Cell::new(0),
        })
    }

    /// Empties the list. RT-safe.
    pub(super) fn clear(&self) {
        self.len.set(0);
    }

    #[cfg(test)]
    /// Number of events. RT-safe.
    pub(super) fn len(&self) -> usize {
        self.len.get()
    }

    /// Appends a note event on bus 0. The note id pairs on and off of the
    /// same channel and key. RT-safe; false if the list is full.
    pub(super) fn push_note(&self, n: &NoteEvent) -> bool {
        let Some(slot) = self.events.get(self.len.get()) else {
            return false;
        };
        let channel = i16::from(n.channel());
        let pitch = i16::from(n.key());
        let note_id = (i32::from(n.channel()) << 8) | i32::from(n.key());
        let mut e = blank();
        e.busIndex = 0;
        e.sampleOffset = i32::try_from(n.offset()).unwrap_or(i32::MAX);
        match n.kind() {
            NoteKind::On => {
                e.r#type = Event_::EventTypes_::kNoteOnEvent as u16;
                e.__field0.noteOn = NoteOnEvent {
                    channel,
                    pitch,
                    tuning: 0.0,
                    velocity: n.velocity(),
                    length: 0,
                    noteId: note_id,
                };
            }
            NoteKind::Off => {
                e.r#type = Event_::EventTypes_::kNoteOffEvent as u16;
                e.__field0.noteOff = NoteOffEvent {
                    channel,
                    pitch,
                    velocity: n.velocity(),
                    noteId: note_id,
                    tuning: 0.0,
                };
            }
        }
        slot.set(e);
        self.len.set(self.len.get() + 1);
        true
    }

    /// Event `index`, if any. RT-safe.
    pub(super) fn get(&self, index: usize) -> Option<Event> {
        (index < self.len.get())
            .then(|| self.events.get(index).map(Cell::get))
            .flatten()
    }

    /// The interface pointer for `ProcessData`. Valid while `wrapper` lives.
    pub(super) fn as_ptr(wrapper: &ComWrapper<EventList>) -> *mut IEventList {
        wrapper
            .as_com_ref::<IEventList>()
            .map_or(std::ptr::null_mut(), |r| r.as_ptr())
    }
}

impl IEventListTrait for EventList {
    unsafe fn getEventCount(&self) -> int32 {
        self.len.get() as int32
    }

    unsafe fn getEvent(&self, index: int32, e: *mut Event) -> tresult {
        if e.is_null() {
            return kInvalidArgument;
        }
        match usize::try_from(index).ok().and_then(|i| self.get(i)) {
            Some(event) => {
                // SAFETY: non-null out pointer from the plugin.
                unsafe { *e = event };
                kResultOk
            }
            None => kResultFalse,
        }
    }

    unsafe fn addEvent(&self, e: *mut Event) -> tresult {
        if e.is_null() {
            return kInvalidArgument;
        }
        match self.events.get(self.len.get()) {
            Some(slot) => {
                // SAFETY: non-null event pointer from the plugin.
                slot.set(unsafe { *e });
                self.len.set(self.len.get() + 1);
                kResultOk
            }
            None => kResultFalse,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_round_trip_through_the_com_interface() {
        let list = EventList::new();
        let on = NoteEvent::on(12, 1, 60, 0.5).expect("note");
        let off = NoteEvent::off(40, 1, 60, 0.0).expect("note");
        assert!(list.push_note(&on) && list.push_note(&off));
        let p = list.as_com_ref::<IEventList>().expect("interface");
        // SAFETY: valid pointers for the calls.
        unsafe {
            assert_eq!(p.getEventCount(), 2);
            let mut e = blank();
            assert_eq!(p.getEvent(0, &mut e), kResultOk);
            assert_eq!((e.sampleOffset, e.r#type), (12, 0));
            assert_eq!(
                (
                    e.__field0.noteOn.pitch,
                    e.__field0.noteOn.channel,
                    e.__field0.noteOn.velocity
                ),
                (60, 1, 0.5)
            );
            let on_id = e.__field0.noteOn.noteId;
            assert_eq!(p.getEvent(1, &mut e), kResultOk);
            assert_eq!((e.sampleOffset, e.r#type), (40, 1));
            assert_eq!(e.__field0.noteOff.noteId, on_id, "off pairs with on");
            assert_eq!(p.getEvent(2, &mut e), kResultFalse);
        }
        list.clear();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn a_full_list_refuses_more() {
        let list = EventList::new();
        let on = NoteEvent::on(0, 0, 60, 1.0).expect("note");
        for _ in 0..MAX_NOTE_EVENTS {
            assert!(list.push_note(&on));
        }
        assert!(!list.push_note(&on));
        let mut e = blank();
        // SAFETY: valid pointer.
        assert_eq!(unsafe { list.addEvent(&mut e) }, kResultFalse);
    }
}
