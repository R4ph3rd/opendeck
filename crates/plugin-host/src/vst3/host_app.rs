//! The host context handed to plugins: `IHostApplication`, with the
//! `IMessage` and `IAttributeList` objects plugins create through it to
//! talk between their component and controller halves.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Mutex;

use vst3::Steinberg::Vst::{
    IAttributeList, IAttributeListTrait, IHostApplication, IHostApplicationTrait, IMessage,
    IMessageTrait, String128, TChar,
};
use vst3::Steinberg::{
    FUnknown, TUID, int64, kInvalidArgument, kNoInterface, kResultFalse, kResultOk, tresult, uint32,
};
use vst3::{Class, ComWrapper, Interface};

use super::util::write_utf16;

/// The name plugins see through `IHostApplication::getName`.
pub(super) const HOST_NAME: &str = "OpenDeck";

/// `IHostApplication`. Creates `IMessage` and `IAttributeList` only.
pub(super) struct HostApp;

impl Class for HostApp {
    type Interfaces = (IHostApplication,);
}

impl HostApp {
    pub(super) fn new() -> ComWrapper<HostApp> {
        ComWrapper::new(HostApp)
    }

    /// The context pointer for `initialize` and `setHostContext`. Valid
    /// while `wrapper` lives.
    pub(super) fn context_ptr(wrapper: &ComWrapper<HostApp>) -> *mut FUnknown {
        wrapper
            .as_com_ref::<FUnknown>()
            .map_or(std::ptr::null_mut(), |r| r.as_ptr())
    }
}

impl IHostApplicationTrait for HostApp {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        if name.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: non-null out pointer to a String128 from the plugin.
        write_utf16(HOST_NAME, unsafe { &mut *name });
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        cid: *mut TUID,
        iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if cid.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: non-null TUID pointers from the plugin.
        let (cid, iid) = unsafe { (*cid, *iid) };
        let bytes = |t: TUID| t.map(|c| c as u8);
        let unknown = if bytes(cid) == IMessage::IID {
            ComWrapper::new(Message::new()).to_com_ptr::<FUnknown>()
        } else if bytes(cid) == IAttributeList::IID {
            ComWrapper::new(AttributeList::new()).to_com_ptr::<FUnknown>()
        } else {
            return kNoInterface;
        };
        let Some(unknown) = unknown else {
            return kNoInterface;
        };
        let ptr = unknown.as_ptr();
        let mut iid = iid;
        // SAFETY: `ptr` is a live FUnknown; `queryInterface` adds the
        // reference the plugin owns, ours is released when `unknown` drops.
        unsafe { ((*(*ptr).vtbl).queryInterface)(ptr, &raw mut iid, obj) }
    }
}

/// One attribute value.
enum Value {
    Int(i64),
    Float(f64),
    Str(Vec<TChar>),
    Bin(Vec<u8>),
}

/// `IAttributeList` over a map. Thread-safe; plugins normally use it on
/// the main thread.
pub(super) struct AttributeList {
    map: Mutex<HashMap<String, Value>>,
}

impl Class for AttributeList {
    type Interfaces = (IAttributeList,);
}

impl AttributeList {
    fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }

    fn key(id: *const c_char) -> Option<String> {
        if id.is_null() {
            return None;
        }
        // SAFETY: attribute ids are NUL-terminated strings from the plugin.
        Some(unsafe { CStr::from_ptr(id) }.to_string_lossy().into_owned())
    }

    fn set(&self, id: *const c_char, v: Value) -> tresult {
        let (Some(key), Ok(mut map)) = (Self::key(id), self.map.lock()) else {
            return kInvalidArgument;
        };
        map.insert(key, v);
        kResultOk
    }
}

impl IAttributeListTrait for AttributeList {
    unsafe fn setInt(&self, id: *const c_char, value: int64) -> tresult {
        self.set(id, Value::Int(value))
    }

    unsafe fn getInt(&self, id: *const c_char, value: *mut int64) -> tresult {
        let (Some(key), Ok(map)) = (Self::key(id), self.map.lock()) else {
            return kInvalidArgument;
        };
        match map.get(&key) {
            Some(Value::Int(v)) if !value.is_null() => {
                // SAFETY: non-null out pointer from the plugin.
                unsafe { *value = *v };
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setFloat(&self, id: *const c_char, value: f64) -> tresult {
        self.set(id, Value::Float(value))
    }

    unsafe fn getFloat(&self, id: *const c_char, value: *mut f64) -> tresult {
        let (Some(key), Ok(map)) = (Self::key(id), self.map.lock()) else {
            return kInvalidArgument;
        };
        match map.get(&key) {
            Some(Value::Float(v)) if !value.is_null() => {
                // SAFETY: non-null out pointer from the plugin.
                unsafe { *value = *v };
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setString(&self, id: *const c_char, string: *const TChar) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let mut units = Vec::new();
        let mut p = string;
        // SAFETY: the plugin passes a NUL-terminated UTF-16 string.
        unsafe {
            while *p != 0 {
                units.push(*p);
                p = p.add(1);
            }
        }
        self.set(id, Value::Str(units))
    }

    unsafe fn getString(
        &self,
        id: *const c_char,
        string: *mut TChar,
        size_in_bytes: uint32,
    ) -> tresult {
        let (Some(key), Ok(map)) = (Self::key(id), self.map.lock()) else {
            return kInvalidArgument;
        };
        let capacity = size_in_bytes as usize / std::mem::size_of::<TChar>();
        match map.get(&key) {
            Some(Value::Str(units)) if !string.is_null() && capacity > 0 => {
                let n = units.len().min(capacity - 1);
                // SAFETY: the plugin's buffer holds `capacity` units; we
                // write `n + 1 <= capacity`.
                unsafe {
                    std::ptr::copy_nonoverlapping(units.as_ptr(), string, n);
                    *string.add(n) = 0;
                }
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setBinary(
        &self,
        id: *const c_char,
        data: *const c_void,
        size_in_bytes: uint32,
    ) -> tresult {
        let bytes = if data.is_null() || size_in_bytes == 0 {
            Vec::new()
        } else {
            // SAFETY: the plugin passes `size_in_bytes` readable bytes.
            unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size_in_bytes as usize) }
                .to_vec()
        };
        self.set(id, Value::Bin(bytes))
    }

    unsafe fn getBinary(
        &self,
        id: *const c_char,
        data: *mut *const c_void,
        size_in_bytes: *mut uint32,
    ) -> tresult {
        let (Some(key), Ok(map)) = (Self::key(id), self.map.lock()) else {
            return kInvalidArgument;
        };
        match map.get(&key) {
            Some(Value::Bin(b)) if !data.is_null() && !size_in_bytes.is_null() => {
                // SAFETY: non-null out pointers from the plugin. The data
                // pointer stays valid until the attribute is set again, as
                // the interface documents.
                unsafe {
                    *data = b.as_ptr().cast();
                    *size_in_bytes = b.len() as uint32;
                }
                kResultOk
            }
            _ => kResultFalse,
        }
    }
}

/// `IMessage`: an id and an attribute list.
pub(super) struct Message {
    id: Mutex<CString>,
    attributes: ComWrapper<AttributeList>,
}

impl Class for Message {
    type Interfaces = (IMessage,);
}

impl Message {
    fn new() -> Self {
        Self {
            id: Mutex::new(CString::default()),
            attributes: ComWrapper::new(AttributeList::new()),
        }
    }
}

impl IMessageTrait for Message {
    unsafe fn getMessageID(&self) -> *const c_char {
        // The pointer stays valid until the id is set again.
        self.id.lock().map_or(std::ptr::null(), |id| id.as_ptr())
    }

    unsafe fn setMessageID(&self, id: *const c_char) {
        if id.is_null() {
            return;
        }
        // SAFETY: the plugin passes a NUL-terminated string.
        let value = unsafe { CStr::from_ptr(id) }.to_owned();
        if let Ok(mut slot) = self.id.lock() {
            *slot = value;
        }
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        self.attributes
            .as_com_ref::<IAttributeList>()
            .map_or(std::ptr::null_mut(), |r| r.as_ptr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vst3::ComPtr;

    fn create<I: Interface>(host: &ComWrapper<HostApp>) -> Option<ComPtr<I>> {
        let h = host.as_com_ref::<IHostApplication>().expect("host");
        let mut cid: TUID = I::IID.map(|b| b as c_char);
        let mut iid = cid;
        let mut obj = std::ptr::null_mut();
        // SAFETY: valid pointers for the call.
        let r = unsafe { h.createInstance(&mut cid, &mut iid, &mut obj) };
        // SAFETY: on success `obj` is an owned reference to an `I`.
        (r == kResultOk)
            .then(|| unsafe { ComPtr::from_raw(obj.cast::<I>()) })
            .flatten()
    }

    #[test]
    fn the_host_names_itself_and_refuses_unknown_classes() {
        let host = HostApp::new();
        let h = host.as_com_ref::<IHostApplication>().expect("host");
        let mut name: String128 = [0; 128];
        // SAFETY: valid out pointer.
        assert_eq!(unsafe { h.getName(&mut name) }, kResultOk);
        assert_eq!(super::super::util::utf16_to_string(&name), HOST_NAME);
        let mut cid: TUID = [7; 16];
        let mut iid = cid;
        let mut obj = std::ptr::null_mut();
        // SAFETY: valid pointers.
        assert_eq!(
            unsafe { h.createInstance(&mut cid, &mut iid, &mut obj) },
            kNoInterface
        );
    }

    #[test]
    fn messages_carry_typed_attributes() {
        let host = HostApp::new();
        let msg = create::<IMessage>(&host).expect("message");
        // SAFETY: valid pointers and NUL-terminated strings for every call.
        unsafe {
            msg.setMessageID(c"state".as_ptr());
            assert_eq!(CStr::from_ptr(msg.getMessageID()), c"state");
            let attrs = vst3::ComRef::from_raw(msg.getAttributes()).expect("attributes");
            assert_eq!(attrs.setInt(c"n".as_ptr(), 42), kResultOk);
            assert_eq!(attrs.setFloat(c"f".as_ptr(), 0.5), kResultOk);
            let text: Vec<TChar> = "hi".encode_utf16().chain([0]).map(|u| u as TChar).collect();
            assert_eq!(attrs.setString(c"s".as_ptr(), text.as_ptr()), kResultOk);
            let blob = [1u8, 2, 3];
            assert_eq!(
                attrs.setBinary(c"b".as_ptr(), blob.as_ptr().cast(), 3),
                kResultOk
            );

            let (mut n, mut f) = (0, 0.0);
            assert_eq!(attrs.getInt(c"n".as_ptr(), &mut n), kResultOk);
            assert_eq!(attrs.getFloat(c"f".as_ptr(), &mut f), kResultOk);
            assert_eq!((n, f), (42, 0.5));
            // A value of another type is not found.
            assert_eq!(attrs.getInt(c"f".as_ptr(), &mut n), kResultFalse);
            let mut out: [TChar; 8] = [0; 8];
            assert_eq!(
                attrs.getString(c"s".as_ptr(), out.as_mut_ptr(), 16),
                kResultOk
            );
            assert_eq!(super::super::util::utf16_to_string(&out), "hi");
            // A 2-unit buffer truncates to one character and a NUL.
            assert_eq!(
                attrs.getString(c"s".as_ptr(), out.as_mut_ptr(), 4),
                kResultOk
            );
            assert_eq!(super::super::util::utf16_to_string(&out), "h");
            let (mut data, mut len) = (std::ptr::null(), 0);
            assert_eq!(
                attrs.getBinary(c"b".as_ptr(), &mut data, &mut len),
                kResultOk
            );
            assert_eq!(
                std::slice::from_raw_parts(data.cast::<u8>(), len as usize),
                [1, 2, 3]
            );
            assert_eq!(attrs.getInt(c"missing".as_ptr(), &mut n), kResultFalse);
        }
    }
}
