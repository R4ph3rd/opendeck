//! Loading a `.vst3` module and reading its plugin factory.
//!
//! Loading follows the VST 3 module format (SDK hosting code, `module_win32.cpp`,
//! `module_mac.mm`, `module_linux.cpp`; <https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Locations+Format/Plugin+Format.html>):
//!
//! | OS | Library | Entry | Exit |
//! |---|---|---|---|
//! | Windows | `<bundle>/Contents/<arch>-win/<name>.vst3` (or the `.vst3` file itself) | `InitDll()` | `ExitDll()` |
//! | macOS | `<bundle>/Contents/MacOS/<name>` | `bundleEntry(CFBundleRef)` | `bundleExit()` |
//! | Linux | `<bundle>/Contents/<arch>-linux/<name>.so` | `ModuleEntry(void* handle)` | `ModuleExit()` |
//!
//! then `GetPluginFactory()`. A module is loaded once per path and shared
//! by the instances made from it (the entry function runs once per load, as
//! in the SDK's own host classes); the last user runs the exit function and
//! unloads it. Main thread only: the cache is thread-local.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Weak;

use libloading::Library;
use vst3::Steinberg::{
    IPluginFactory, IPluginFactory2, IPluginFactory2Trait, IPluginFactory3, IPluginFactory3Trait,
    IPluginFactoryTrait, PClassInfo, PClassInfo2, PClassInfoW, PFactoryInfo, TUID,
};
use vst3::{ComPtr, ComWrapper, Interface};

use super::host_app::HostApp;
use super::util::{cchars_to_string, is_ok, utf16_to_string};
use crate::PluginError;
use crate::scan::{TargetOs, vst3_binary_path};

/// The category of audio plugin classes.
pub(super) const AUDIO_MODULE_CLASS: &str = "Audio Module Class";

type GetFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
type ExitFn = unsafe extern "system" fn() -> bool;

/// One audio class a factory lists.
#[derive(Debug, Clone)]
pub(super) struct ClassInfo {
    pub cid: TUID,
    pub name: String,
    pub vendor: String,
    pub version: String,
    /// `subCategories` split at `|` (`Fx`, `Reverb`, `Instrument`, `Synth`, ...).
    pub subcategories: Vec<String>,
}

/// A loaded VST3 module. Not `Send`: plugin code runs on the main thread.
pub(super) struct Module {
    factory: Option<ComPtr<IPluginFactory>>,
    host: ComWrapper<HostApp>,
    exit: Option<ExitFn>,
    #[cfg(target_os = "macos")]
    bundle: *mut c_void,
    path: PathBuf,
    // Last: the library unloads after everything above is released.
    _lib: Library,
}

thread_local! {
    static MODULES: RefCell<HashMap<PathBuf, Weak<Module>>> = RefCell::new(HashMap::new());
}

fn load_err(path: &Path, reason: impl Into<String>) -> PluginError {
    PluginError::Load {
        path: path.display().to_string(),
        reason: reason.into(),
    }
}

/// The library file of a bundle on this OS. A `.vst3` that is a plain file
/// (single-DLL plugins of early SDKs on Windows) is the library itself.
fn binary_path(bundle: &Path) -> PathBuf {
    if bundle.is_file() {
        return bundle.to_owned();
    }
    vst3_binary_path(bundle, TargetOs::current(), std::env::consts::ARCH)
}

impl Module {
    /// Loads `bundle` (or returns the module already loaded for it). Runs
    /// plugin code (library initialisation). Main thread, or the scanner's
    /// only thread.
    pub(super) fn load(bundle: &Path) -> Result<Arc<Module>, PluginError> {
        let cached = MODULES.with(|m| m.borrow().get(bundle).and_then(Weak::upgrade));
        if let Some(m) = cached {
            return Ok(m);
        }
        let module = Arc::new(Self::open(bundle)?);
        MODULES.with(|m| {
            let mut m = m.borrow_mut();
            m.retain(|_, w| w.strong_count() > 0);
            m.insert(bundle.to_owned(), Arc::downgrade(&module));
        });
        Ok(module)
    }

    fn open(bundle: &Path) -> Result<Module, PluginError> {
        let binary = binary_path(bundle);
        if !binary.is_file() {
            return Err(load_err(
                bundle,
                format!("no library at {} for this OS and CPU", binary.display()),
            ));
        }
        // SAFETY: loading a library runs its initialisers. Only plugins that
        // passed a scan in the scanner subprocess get here in the app.
        let (lib, raw_handle) =
            unsafe { open_library(&binary) }.map_err(|e| load_err(bundle, e))?;

        #[cfg(target_os = "macos")]
        let bundle_ref = cf::create_bundle(bundle);
        #[cfg(not(target_os = "macos"))]
        let bundle_ref: *mut c_void = std::ptr::null_mut();

        // SAFETY: the symbols are looked up by the names the format defines
        // and have the signatures in the format documentation.
        let exit = unsafe { run_entry(&lib, raw_handle, bundle_ref) }.map_err(|e| {
            #[cfg(target_os = "macos")]
            cf::release(bundle_ref);
            load_err(bundle, e)
        })?;

        // SAFETY: `GetPluginFactory` is required by the format and takes no
        // arguments; it returns an owned factory reference.
        let factory = unsafe {
            let get = lib
                .get::<GetFactory>(b"GetPluginFactory\0")
                .map_err(|e| load_err(bundle, format!("no GetPluginFactory: {e}")))?;
            ComPtr::from_raw(get())
        }
        .ok_or_else(|| load_err(bundle, "GetPluginFactory returned null"))?;

        let host = HostApp::new();
        if let Some(f3) = factory.cast::<IPluginFactory3>() {
            // SAFETY: valid factory and host context pointers.
            let _ = unsafe { f3.setHostContext(HostApp::context_ptr(&host)) };
        }
        Ok(Module {
            factory: Some(factory),
            host,
            exit,
            #[cfg(target_os = "macos")]
            bundle: bundle_ref,
            path: bundle.to_owned(),
            _lib: lib,
        })
    }

    /// The bundle path this module was loaded from.
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// The host context for `initialize` calls. Valid while the module lives.
    pub(super) fn host_context(&self) -> *mut vst3::Steinberg::FUnknown {
        HostApp::context_ptr(&self.host)
    }

    fn factory(&self) -> Result<&ComPtr<IPluginFactory>, PluginError> {
        self.factory
            .as_ref()
            .ok_or_else(|| load_err(&self.path, "factory released"))
    }

    /// Lists the audio classes. Uses `IPluginFactory3` (unicode names),
    /// then `IPluginFactory2` (sub-categories, vendor, version), then the
    /// plain factory.
    pub(super) fn classes(&self) -> Result<Vec<ClassInfo>, PluginError> {
        let factory = self.factory()?;
        // SAFETY: plain interface calls on a live factory; the info structs
        // are plain data the plugin fills.
        unsafe {
            let count = factory.countClasses();
            let mut factory_vendor = String::new();
            let mut finfo: PFactoryInfo = std::mem::zeroed();
            if is_ok(factory.getFactoryInfo(&mut finfo)) {
                factory_vendor = cchars_to_string(&finfo.vendor);
            }
            let f3 = factory.cast::<IPluginFactory3>();
            let f2 = factory.cast::<IPluginFactory2>();
            let mut out = Vec::new();
            for index in 0..count {
                let info = if let Some(f3) = &f3 {
                    let mut w: PClassInfoW = std::mem::zeroed();
                    is_ok(f3.getClassInfoUnicode(index, &mut w)).then(|| {
                        (
                            w.cid,
                            cchars_to_string(&w.category),
                            utf16_to_string(&w.name),
                            cchars_to_string(&w.subCategories),
                            utf16_to_string(&w.vendor),
                            utf16_to_string(&w.version),
                        )
                    })
                } else if let Some(f2) = &f2 {
                    let mut i: PClassInfo2 = std::mem::zeroed();
                    is_ok(f2.getClassInfo2(index, &mut i)).then(|| {
                        (
                            i.cid,
                            cchars_to_string(&i.category),
                            cchars_to_string(&i.name),
                            cchars_to_string(&i.subCategories),
                            cchars_to_string(&i.vendor),
                            cchars_to_string(&i.version),
                        )
                    })
                } else {
                    let mut i: PClassInfo = std::mem::zeroed();
                    is_ok(factory.getClassInfo(index, &mut i)).then(|| {
                        (
                            i.cid,
                            cchars_to_string(&i.category),
                            cchars_to_string(&i.name),
                            String::new(),
                            String::new(),
                            String::new(),
                        )
                    })
                };
                let Some((cid, category, name, subs, vendor, version)) = info else {
                    continue;
                };
                if category != AUDIO_MODULE_CLASS {
                    continue;
                }
                out.push(ClassInfo {
                    cid,
                    name,
                    vendor: if vendor.is_empty() {
                        factory_vendor.clone()
                    } else {
                        vendor
                    },
                    version,
                    subcategories: subs
                        .split('|')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect(),
                });
            }
            Ok(out)
        }
    }

    /// Creates an object of class `cid` and queries interface `I`.
    pub(super) fn create<I: Interface>(&self, cid: &TUID) -> Result<ComPtr<I>, PluginError> {
        let factory = self.factory()?;
        let mut obj: *mut c_void = std::ptr::null_mut();
        // SAFETY: `cid` and the interface id are 16-byte ids that outlive
        // the call; `obj` receives an owned reference on success.
        let result = unsafe {
            factory.createInstance(
                cid.as_ptr().cast::<c_char>(),
                I::IID.as_ptr().cast::<c_char>(),
                &mut obj,
            )
        };
        if !is_ok(result) {
            return Err(PluginError::Plugin(format!(
                "createInstance failed (result {result})"
            )));
        }
        // SAFETY: on success `obj` is an owned `I` reference.
        unsafe { ComPtr::from_raw(obj.cast::<I>()) }
            .ok_or_else(|| PluginError::Plugin("createInstance returned null".into()))
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        // Release the factory, then run the exit function, then the library
        // unloads (field order).
        drop(self.factory.take());
        if let Some(exit) = self.exit {
            // SAFETY: the exit function belongs to the library still loaded.
            let _ = unsafe { exit() };
        }
        #[cfg(target_os = "macos")]
        cf::release(self.bundle);
    }
}

/// Opens the library. On Unix also returns the raw `dlopen` handle, which
/// Linux plugins want in `ModuleEntry`.
///
/// # Safety
/// Runs the library's initialisers.
#[cfg(unix)]
unsafe fn open_library(path: &Path) -> Result<(Library, *mut c_void), String> {
    // SAFETY: forwarded from the caller.
    let lib = unsafe { libloading::os::unix::Library::new(path) }.map_err(|e| e.to_string())?;
    let raw = lib.into_raw();
    // SAFETY: `raw` is the handle just released by `into_raw`.
    let lib = unsafe { libloading::os::unix::Library::from_raw(raw) };
    Ok((lib.into(), raw))
}

/// Opens the library.
///
/// # Safety
/// Runs the library's initialisers.
#[cfg(windows)]
unsafe fn open_library(path: &Path) -> Result<(Library, *mut c_void), String> {
    // SAFETY: forwarded from the caller.
    let lib = unsafe { Library::new(path) }.map_err(|e| e.to_string())?;
    Ok((lib, std::ptr::null_mut()))
}

/// Calls the platform entry function if the module has one and returns its
/// exit function.
///
/// # Safety
/// Runs plugin code; the symbols must have the documented signatures.
unsafe fn run_entry(
    lib: &Library,
    raw_handle: *mut c_void,
    bundle_ref: *mut c_void,
) -> Result<Option<ExitFn>, String> {
    #[cfg(windows)]
    let (entry_names, exit_names): (&[&[u8]], &[&[u8]]) = (&[b"InitDll\0"], &[b"ExitDll\0"]);
    #[cfg(target_os = "macos")]
    let (entry_names, exit_names): (&[&[u8]], &[&[u8]]) = (
        &[b"bundleEntry\0", b"BundleEntry\0"],
        &[b"bundleExit\0", b"BundleExit\0"],
    );
    #[cfg(all(unix, not(target_os = "macos")))]
    let (entry_names, exit_names): (&[&[u8]], &[&[u8]]) = (&[b"ModuleEntry\0"], &[b"ModuleExit\0"]);

    let arg = if cfg!(windows) {
        std::ptr::null_mut()
    } else if cfg!(target_os = "macos") {
        bundle_ref
    } else {
        raw_handle
    };
    for name in entry_names {
        // SAFETY: documented signature: `bool entry(void* arg)`; `InitDll`
        // takes no argument and ignores the extra one in the C ABI.
        if let Ok(entry) =
            unsafe { lib.get::<unsafe extern "system" fn(*mut c_void) -> bool>(name) }
        {
            // SAFETY: as above.
            if !unsafe { entry(arg) } {
                return Err("the module's entry function failed".into());
            }
            break;
        }
    }
    for name in exit_names {
        // SAFETY: documented signature: `bool exit()`.
        if let Ok(exit) = unsafe { lib.get::<ExitFn>(name) } {
            return Ok(Some(*exit));
        }
    }
    Ok(None)
}

#[cfg(target_os = "macos")]
mod cf {
    //! The bits of CoreFoundation `bundleEntry` needs: a `CFBundleRef` for
    //! the bundle. `TODO(verify)` on a Mac: not run in this wave's CI.
    use std::ffi::c_void;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: *const c_void,
            buffer: *const u8,
            length: isize,
            is_directory: u8,
        ) -> *mut c_void;
        fn CFBundleCreate(allocator: *const c_void, url: *mut c_void) -> *mut c_void;
        fn CFRelease(cf: *const c_void);
    }

    /// A retained `CFBundleRef` for `bundle`, or null.
    pub(super) fn create_bundle(bundle: &Path) -> *mut c_void {
        let bytes = bundle.as_os_str().as_bytes();
        // SAFETY: valid buffer and length; null allocator is the default.
        unsafe {
            let url = CFURLCreateFromFileSystemRepresentation(
                std::ptr::null(),
                bytes.as_ptr(),
                bytes.len() as isize,
                1,
            );
            if url.is_null() {
                return std::ptr::null_mut();
            }
            let b = CFBundleCreate(std::ptr::null(), url);
            CFRelease(url);
            b
        }
    }

    /// Releases a bundle from [`create_bundle`].
    pub(super) fn release(bundle: *mut c_void) {
        if !bundle.is_null() {
            // SAFETY: an owned reference from `CFBundleCreate`.
            unsafe { CFRelease(bundle) };
        }
    }
}
