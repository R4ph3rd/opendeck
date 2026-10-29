// Fails when the versions in Cargo.toml, ui/package.json (and tauri.conf.json if it sets one) differ.
import { readVersions } from "./version-files.mjs";

const versions = readVersions();
const unique = new Set(Object.values(versions));
console.log(versions);
if (unique.size > 1) {
  console.error("Version mismatch. Run `node scripts/bump-version.mjs <version>`.");
  process.exit(1);
}
console.log(`versions in sync: ${[...unique][0]}`);
