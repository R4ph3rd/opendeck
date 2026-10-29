// Runs the Tauri CLI from ui/ against ../src-tauri.
// The CLI only searches the current folder for tauri.conf.json, and src-tauri
// is a sibling of ui/, so point it there explicitly. Cross-platform, no shell
// env syntax. Usage: `pnpm tauri <command> [args]`.
import { run } from "@tauri-apps/cli/main.js";
import path from "node:path";
import { fileURLToPath } from "node:url";

const uiDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
process.env["TAURI_APP_PATH"] ??= path.join(uiDir, "..", "src-tauri");
process.env["TAURI_FRONTEND_PATH"] ??= uiDir;

try {
  await run(process.argv.slice(2), "pnpm tauri");
} catch (error) {
  console.error(error);
  process.exit(1);
}
