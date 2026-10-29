// Usage: node scripts/bump-version.mjs <version>
import { readFileSync, writeFileSync } from "node:fs";
import { files, patterns } from "./version-files.mjs";

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("usage: node scripts/bump-version.mjs <major.minor.patch[-pre]>");
  process.exit(1);
}

for (const [name, file] of Object.entries(files)) {
  const text = readFileSync(file, "utf8");
  if (!patterns[name].test(text)) {
    if (name === "shell" || name === "tauri") continue; // optional: Tauri falls back to the Cargo version
    throw new Error(`no version found in ${file}`);
  }
  writeFileSync(file, text.replace(patterns[name], `$1${version}$3`));
  console.log(`${file} -> ${version}`);
}
console.log("\nNext: run `cargo update -w` to refresh Cargo.lock, then commit.");
