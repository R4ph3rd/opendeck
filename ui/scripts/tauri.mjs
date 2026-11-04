// Runs the Tauri CLI from ui/ against ../src-tauri.
// The CLI only searches the current folder for tauri.conf.json, and src-tauri
// is a sibling of ui/, so point it there explicitly. Cross-platform, no shell
// env syntax. Usage: `pnpm tauri <command> [args]`.
//
// `build` and `bundle` also merge src-tauri/tauri.sidecars.conf.json, which
// lists the sidecars (`bundle.externalBin`) that `beforeBuildCommand` builds
// with scripts/build-sidecars.mjs. It stays out of tauri.conf.json because
// tauri-build fails any cargo build of src-tauri (clippy, tests) when a listed
// sidecar file is missing.
import { run } from "@tauri-apps/cli/main.js";
import path from "node:path";
import { fileURLToPath } from "node:url";

const uiDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const appDir = path.join(uiDir, "..", "src-tauri");
process.env["TAURI_APP_PATH"] ??= appDir;
process.env["TAURI_FRONTEND_PATH"] ??= uiDir;

const args = process.argv.slice(2);
if (args[0] === "build" || args[0] === "bundle") {
  // Right after the command: arguments after `--` go to cargo.
  args.splice(1, 0, "--config", path.join(appDir, "tauri.sidecars.conf.json"));
}

try {
  await run(args, "pnpm tauri");
} catch (error) {
  console.error(error);
  process.exit(1);
}
