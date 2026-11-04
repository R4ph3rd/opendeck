//! `IBStream` over a byte vector, for plugin state. Main thread only.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use vst3::Steinberg::{
    IBStream, IBStreamTrait, int32, int64, kInvalidArgument, kResultFalse, kResultOk, tresult,
};
use vst3::{Class, ComWrapper};

/// `IBStream::kIBSeekSet`, `kIBSeekCur`, `kIBSeekEnd`.
const SEEK_SET: int32 = 0;
const SEEK_CUR: int32 = 1;
const SEEK_END: int32 = 2;

/// A growable in-memory stream. Writes past the end extend it; seeking
/// past the end is allowed (a later write pads with zeros).
pub(super) struct MemStream {
    data: RefCell<Vec<u8>>,
    pos: Cell<usize>,
}

impl Class for MemStream {
    type Interfaces = (IBStream,);
}

impl MemStream {
    /// An empty stream to write into.
    pub(super) fn empty() -> ComWrapper<MemStream> {
        Self::with_bytes(Vec::new())
    }

    /// A stream to read `bytes` from, positioned at the start.
    pub(super) fn with_bytes(bytes: Vec<u8>) -> ComWrapper<MemStream> {
        ComWrapper::new(MemStream {
            data: RefCell::new(bytes),
            pos: Cell::new(0),
        })
    }

    /// Takes the written bytes and resets the stream.
    pub(super) fn take(&self) -> Vec<u8> {
        self.pos.set(0);
        std::mem::take(&mut *self.data.borrow_mut())
    }

    /// Back to the start, to read what was written.
    pub(super) fn rewind(&self) {
        self.pos.set(0);
    }

    /// The raw interface pointer for plugin calls. Valid while `wrapper` lives.
    pub(super) fn as_ptr(wrapper: &ComWrapper<MemStream>) -> *mut IBStream {
        wrapper
            .as_com_ref::<IBStream>()
            .map_or(std::ptr::null_mut(), |r| r.as_ptr())
    }
}

impl IBStreamTrait for MemStream {
    unsafe fn read(
        &self,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_read: *mut int32,
    ) -> tresult {
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let data = self.data.borrow();
        let pos = self.pos.get().min(data.len());
        let n = (num_bytes as usize).min(data.len() - pos);
        // SAFETY: the caller passes a buffer of at least `num_bytes` bytes;
        // `n <= num_bytes` and the source range is inside `data`.
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr().add(pos), buffer.cast::<u8>(), n) };
        self.pos.set(pos + n);
        if !num_bytes_read.is_null() {
            // SAFETY: non-null out pointer from the caller.
            unsafe { *num_bytes_read = n as int32 };
        }
        kResultOk
    }

    unsafe fn write(
        &self,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_written: *mut int32,
    ) -> tresult {
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let mut data = self.data.borrow_mut();
        let pos = self.pos.get();
        let end = pos + num_bytes as usize;
        if data.len() < end {
            data.resize(end, 0);
        }
        // SAFETY: the caller passes `num_bytes` readable bytes; the
        // destination range `pos..end` is inside `data` after the resize.
        unsafe {
            std::ptr::copy_nonoverlapping(
                buffer.cast::<u8>().cast_const(),
                data.as_mut_ptr().add(pos),
                num_bytes as usize,
            )
        };
        self.pos.set(end);
        if !num_bytes_written.is_null() {
            // SAFETY: non-null out pointer from the caller.
            unsafe { *num_bytes_written = num_bytes };
        }
        kResultOk
    }

    unsafe fn seek(&self, pos: int64, mode: int32, result: *mut int64) -> tresult {
        let base = match mode {
            SEEK_SET => 0,
            SEEK_CUR => self.pos.get() as i64,
            SEEK_END => self.data.borrow().len() as i64,
            _ => return kInvalidArgument,
        };
        let Some(target) = base.checked_add(pos).filter(|t| *t >= 0) else {
            return kResultFalse;
        };
        self.pos.set(target as usize);
        if !result.is_null() {
            // SAFETY: non-null out pointer from the caller.
            unsafe { *result = target };
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut int64) -> tresult {
        if pos.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: non-null out pointer from the caller.
        unsafe { *pos = self.pos.get() as int64 };
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_seek_and_read_back() {
        let s = MemStream::empty();
        let p = s.as_com_ref::<IBStream>().expect("IBStream");
        let mut written = 0;
        let mut bytes = *b"hello world";
        // SAFETY: valid buffers and out pointers for the calls.
        unsafe {
            assert_eq!(
                p.write(bytes.as_mut_ptr().cast(), 11, &mut written),
                kResultOk
            );
            assert_eq!(written, 11);
            let mut at = 0;
            assert_eq!(p.seek(-5, SEEK_END, &mut at), kResultOk);
            assert_eq!(at, 6);
            let mut out = [0u8; 16];
            let mut read = 0;
            assert_eq!(p.read(out.as_mut_ptr().cast(), 16, &mut read), kResultOk);
            assert_eq!((read, &out[..5]), (5, &b"world"[..]));
            // At the end: zero bytes, still ok.
            assert_eq!(p.read(out.as_mut_ptr().cast(), 4, &mut read), kResultOk);
            assert_eq!(read, 0);
            assert_eq!(p.seek(-1, SEEK_SET, std::ptr::null_mut()), kResultFalse);
            assert_eq!(p.seek(0, 9, std::ptr::null_mut()), kInvalidArgument);
            let mut t = 0;
            assert_eq!(p.tell(&mut t), kResultOk);
            assert_eq!(t, 11);
        }
        assert_eq!(s.take(), b"hello world");
    }

    #[test]
    fn seeking_past_the_end_pads_with_zeros() {
        let s = MemStream::empty();
        let p = s.as_com_ref::<IBStream>().expect("IBStream");
        let mut byte = 7u8;
        // SAFETY: valid buffers for the calls.
        unsafe {
            assert_eq!(p.seek(3, SEEK_SET, std::ptr::null_mut()), kResultOk);
            assert_eq!(
                p.write((&raw mut byte).cast(), 1, std::ptr::null_mut()),
                kResultOk
            );
        }
        assert_eq!(s.take(), [0, 0, 0, 7]);
    }
}
