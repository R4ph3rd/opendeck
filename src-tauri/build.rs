//! Tauri build script.
//!
//! On Windows MSVC the app manifest (Common-Controls v6 dependency) is embedded
//! through linker arguments instead of tauri-build's resource, so every binary
//! the crate produces gets it, including `cargo test` harnesses. Without it the
//! test executables abort with STATUS_ENTRYPOINT_NOT_FOUND.

fn main() {
    let attrs = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    if let Err(error) = tauri_build::try_build(attrs) {
        panic!("tauri build script failed: {error:#}");
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
