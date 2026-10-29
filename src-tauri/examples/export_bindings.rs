//! Writes `ui/src/ipc/bindings.ts` without starting the app.
//!
//! Usage: `cargo run -p opendeck-desktop --example export-bindings [OUTPUT]`.
//! CI runs it and then `git diff --exit-code ui/src/ipc/bindings.ts`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let out = std::env::args_os().nth(1).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join(opendeck_desktop::BINDINGS_PATH),
        PathBuf::from,
    );
    match opendeck_desktop::export_bindings(&out) {
        Ok(()) => {
            println!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("failed to export bindings: {err}");
            ExitCode::FAILURE
        }
    }
}
