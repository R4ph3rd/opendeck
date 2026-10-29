// Shared helpers for bump-version.mjs and check-version-sync.mjs.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const files = {
  cargo: path.join(root, "Cargo.toml"),
  tauri: path.join(root, "src-tauri", "tauri.conf.json"),
  ui: path.join(root, "ui", "package.json"),
  shell: path.join(root, "src-tauri", "Cargo.toml"),
};

export const patterns = {
  cargo: /(\[workspace\.package\][^[]*?\nversion\s*=\s*")([^"]+)(")/,
  tauri: /("version"\s*:\s*")([^"]+)(")/,
  ui: /("version"\s*:\s*")([^"]+)(")/,
  // Only matches a literal version; `version.workspace = true` does not match.
  shell: /(\[package\][^[]*?\nversion\s*=\s*")([^"]+)(")/,
};

/** Returns { name: version } for every file that carries its own version. */
export function readVersions() {
  const out = {};
  for (const [name, file] of Object.entries(files)) {
    const m = patterns[name].exec(readFileSync(file, "utf8"));
    if (!m) {
      if (name === "shell" || name === "tauri") continue; // shell inherits; tauri.conf.json falls back to Cargo
      throw new Error(`no version found in ${file}`);
    }
    out[name] = m[2];
  }
  return out;
}
