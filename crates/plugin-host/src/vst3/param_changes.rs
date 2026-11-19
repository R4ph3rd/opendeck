//! Fixed-capacity `IParameterChanges` and `IParamValueQueue`.
//!
//! The VST3 SDK's own host classes grow `std::vector`s while a block runs.
//! These do not: every queue object is created at activation, so the audio
//! thread only writes into memory that already exists. One [`ParamChanges`]
//! serves as the input list (the host writes, the plugin reads) and another
//! as the output list (the plugin adds queues and points, the host reads).
//!
//! Thread rule: a list is used by one thread at a time. The host fills it,
//! calls `IAudioProcessor::process`, then reads it, all on the audio
//! thread, so plain `Cell`s are enough.

use std::cell::Cell;

use vst3::Steinberg::Vst::{
    IParamValueQueue, IParamValueQueueTrait, IParameterChanges, IParameterChangesTrait, ParamID,
    ParamValue,
};
use vst3::Steinberg::{int32, kResultFalse, kResultOk, kResultTrue, tresult};
use vst3::{Class, ComWrapper};

/// Points one queue keeps per block. A further point replaces the last
/// one (the newest value wins), so no change is lost, only intermediate
/// steps of a long ramp.
pub(super) const QUEUE_POINTS: usize = 16;

/// One parameter's points for the block, sorted by sample offset.
pub(super) struct Queue {
    id: Cell<ParamID>,
    len: Cell<usize>,
    points: [Cell<(i32, f64)>; QUEUE_POINTS],
}

impl Class for Queue {
    type Interfaces = (IParamValueQueue,);
}

impl Queue {
    fn new() -> Self {
        Self {
            id: Cell::new(0),
            len: Cell::new(0),
            points: std::array::from_fn(|_| Cell::new((0, 0.0))),
        }
    }

    /// Adds a point keeping offsets sorted; a point at an existing offset
    /// replaces it. Returns its index.
    fn insert(&self, offset: i32, value: f64) -> usize {
        let len = self.len.get();
        let mut at = len;
        for i in 0..len {
            let (o, _) = self.points[i].get();
            if o == offset {
                self.points[i].set((offset, value));
                return i;
            }
            if o > offset {
                at = i;
                break;
            }
        }
        if len == QUEUE_POINTS {
            // Full: the newest value takes the last slot.
            self.points[QUEUE_POINTS - 1]
                .set((offset.max(self.points[QUEUE_POINTS - 2].get().0), value));
            return QUEUE_POINTS - 1;
        }
        let mut i = len;
        while i > at {
            self.points[i].set(self.points[i - 1].get());
            i -= 1;
        }
        self.points[at].set((offset, value));
        self.len.set(len + 1);
        at
    }
}

impl IParamValueQueueTrait for Queue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.get()
    }

    unsafe fn getPointCount(&self) -> int32 {
        self.len.get() as int32
    }

    unsafe fn getPoint(
        &self,
        index: int32,
        sample_offset: *mut int32,
        value: *mut ParamValue,
    ) -> tresult {
        let Some(cell) = usize::try_from(index)
            .ok()
            .filter(|i| *i < self.len.get())
            .and_then(|i| self.points.get(i))
        else {
            return kResultFalse;
        };
        let (o, v) = cell.get();
        if !sample_offset.is_null() {
            // SAFETY: non-null out pointer from the plugin.
            unsafe { *sample_offset = o };
        }
        if !value.is_null() {
            // SAFETY: non-null out pointer from the plugin.
            unsafe { *value = v };
        }
        kResultTrue
    }

    unsafe fn addPoint(
        &self,
        sample_offset: int32,
        value: ParamValue,
        index: *mut int32,
    ) -> tresult {
        let i = self.insert(sample_offset.max(0), value);
        if !index.is_null() {
            // SAFETY: non-null out pointer from the plugin.
            unsafe { *index = i as int32 };
        }
        kResultOk
    }
}

/// A list of parameter queues with room for `capacity` distinct ids.
pub(super) struct ParamChanges {
    queues: Vec<ComWrapper<Queue>>,
    ptrs: Vec<*mut IParamValueQueue>,
    used: Cell<usize>,
}

impl Class for ParamChanges {
    type Interfaces = (IParameterChanges,);
}

impl ParamChanges {
    /// Allocates every queue. Control or main thread.
    pub(super) fn new(capacity: usize) -> ComWrapper<ParamChanges> {
        let queues: Vec<ComWrapper<Queue>> = (0..capacity)
            .map(|_| ComWrapper::new(Queue::new()))
            .collect();
        let ptrs = queues
            .iter()
            .map(|q| {
                q.as_com_ref::<IParamValueQueue>()
                    .map_or(std::ptr::null_mut(), |r| r.as_ptr())
            })
            .collect();
        ComWrapper::new(ParamChanges {
            queues,
            ptrs,
            used: Cell::new(0),
        })
    }

    /// Empties the list. RT-safe.
    pub(super) fn clear(&self) {
        self.used.set(0);
    }

    #[cfg(test)]
    /// Number of queues in use.
    pub(super) fn len(&self) -> usize {
        self.used.get()
    }

    fn queue_for(&self, id: ParamID) -> Option<(usize, &Queue)> {
        let used = self.used.get();
        for (i, q) in self.queues.iter().take(used).enumerate() {
            if q.id.get() == id {
                return Some((i, q));
            }
        }
        let q = self.queues.get(used)?;
        q.id.set(id);
        q.len.set(0);
        self.used.set(used + 1);
        Some((used, q))
    }

    /// Adds a point for `id`. False if no queue is left for a new id.
    /// RT-safe.
    pub(super) fn push(&self, id: ParamID, offset: u32, value: f64) -> bool {
        match self.queue_for(id) {
            Some((_, q)) => {
                q.insert(i32::try_from(offset).unwrap_or(i32::MAX), value);
                true
            }
            None => false,
        }
    }

    /// Calls `f(id, offset, value)` for every point, queue by queue. RT-safe.
    pub(super) fn for_each(&self, mut f: impl FnMut(ParamID, i32, f64)) {
        for q in self.queues.iter().take(self.used.get()) {
            for cell in q.points.iter().take(q.len.get()) {
                let (o, v) = cell.get();
                f(q.id.get(), o, v);
            }
        }
    }

    /// The interface pointer for `ProcessData`. Valid while `wrapper` lives.
    pub(super) fn as_ptr(wrapper: &ComWrapper<ParamChanges>) -> *mut IParameterChanges {
        wrapper
            .as_com_ref::<IParameterChanges>()
            .map_or(std::ptr::null_mut(), |r| r.as_ptr())
    }
}

impl IParameterChangesTrait for ParamChanges {
    unsafe fn getParameterCount(&self) -> int32 {
        self.used.get() as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        usize::try_from(index)
            .ok()
            .filter(|i| *i < self.used.get())
            .and_then(|i| self.ptrs.get(i).copied())
            .unwrap_or(std::ptr::null_mut())
    }

    unsafe fn addParameterData(
        &self,
        id: *const ParamID,
        index: *mut int32,
    ) -> *mut IParamValueQueue {
        if id.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: non-null id pointer from the plugin.
        let id = unsafe { *id };
        match self.queue_for(id) {
            Some((i, _)) => {
                if !index.is_null() {
                    // SAFETY: non-null out pointer from the plugin.
                    unsafe { *index = i as int32 };
                }
                self.ptrs.get(i).copied().unwrap_or(std::ptr::null_mut())
            }
            None => std::ptr::null_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vst3::ComRef;

    fn points(list: &ComWrapper<ParamChanges>) -> Vec<(u32, i32, f64)> {
        let mut v = Vec::new();
        list.for_each(|id, o, x| v.push((id, o, x)));
        v
    }

    #[test]
    fn points_are_grouped_by_id_and_sorted_by_offset() {
        let list = ParamChanges::new(4);
        assert!(list.push(7, 50, 0.5));
        assert!(list.push(3, 0, 0.1));
        assert!(list.push(7, 10, 0.2));
        assert!(list.push(7, 10, 0.25)); // same offset replaces
        assert_eq!(points(&list), [(7, 10, 0.25), (7, 50, 0.5), (3, 0, 0.1)]);
        list.clear();
        assert_eq!(list.len(), 0);
        assert!(points(&list).is_empty());
    }

    #[test]
    fn new_ids_past_the_capacity_are_refused_and_long_ramps_keep_the_last_value() {
        let list = ParamChanges::new(2);
        assert!(list.push(1, 0, 0.0));
        assert!(list.push(2, 0, 0.0));
        assert!(!list.push(3, 0, 0.0));
        assert!(list.push(1, 5, 0.5), "existing id still accepted");
        for i in 0..40 {
            list.push(2, 10 + i, f64::from(i));
        }
        let mut last = None;
        list.for_each(|id, _, v| {
            if id == 2 {
                last = Some(v);
            }
        });
        assert_eq!(last, Some(39.0));
    }

    #[test]
    fn the_plugin_side_sees_the_same_data_through_com() {
        let list = ParamChanges::new(4);
        list.push(9, 12, 0.75);
        let p = list.as_com_ref::<IParameterChanges>().expect("interface");
        // SAFETY: valid pointers for every call; the list outlives them.
        unsafe {
            assert_eq!(p.getParameterCount(), 1);
            let q = ComRef::from_raw(p.getParameterData(0)).expect("queue");
            assert_eq!((q.getParameterId(), q.getPointCount()), (9, 1));
            let (mut o, mut v) = (0, 0.0);
            assert_eq!(q.getPoint(0, &mut o, &mut v), kResultOk);
            assert_eq!((o, v), (12, 0.75));
            assert_eq!(q.getPoint(1, &mut o, &mut v), kResultFalse);
            assert!(p.getParameterData(1).is_null());

            // Output side: the plugin adds a queue and a point.
            let out = ParamChanges::new(2);
            let po = out.as_com_ref::<IParameterChanges>().expect("interface");
            let (id, mut index) = (4u32, -1);
            let q = ComRef::from_raw(po.addParameterData(&id, &mut index)).expect("queue");
            assert_eq!(index, 0);
            let mut at = -1;
            assert_eq!(q.addPoint(3, 0.5, &mut at), kResultOk);
            assert_eq!(at, 0);
            assert_eq!(points(&out), [(4, 3, 0.5)]);
            // The same id finds the same queue.
            let again = po.addParameterData(&id, &mut index);
            assert_eq!(again, q.as_ptr());
        }
    }
}
