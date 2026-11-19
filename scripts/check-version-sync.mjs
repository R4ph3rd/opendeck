// Fails when the versions in Cargo.toml, ui/package.json (and tauri.conf.json if it sets one) differ.
// With an argument, they must also all equal it: node scripts/check-version-sync.mjs [expected].
import { readVersions } from "./version-files.mjs";

const expected = process.argv[2];
const versions = readVersions();
const unique = new Set(Object.values(versions));
if (expected) unique.add(expected);
console.log(versions);
if (unique.size > 1) {
  console.error(
    expected
      ? `Version files do not all equal ${expected}. Run \`node scripts/bump-version.mjs ${expected}\`.`
      : "Version mismatch. Run `node scripts/bump-version.mjs <version>`.",
  );
  process.exit(1);
}
console.log(`versions in sync: ${[...unique][0]}`);
