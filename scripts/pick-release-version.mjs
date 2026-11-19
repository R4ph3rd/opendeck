// Chooses the version a release workflow run builds. Used by release.yml and
// testable locally. Env: EVENT (github.event_name), INPUT_VERSION
// (workflow_dispatch), REF_NAME (tag name on push). Prints the chosen version
// and a notice; writes `version=<v>` to $GITHUB_OUTPUT when set.
//
// - workflow_dispatch: the input version.
// - push of a v* tag: the tag without the `v`.
// - otherwise (automatic run on main): the Cargo workspace version, or, when
//   tag v<that> already exists, the next patch after the highest released
//   x.y.z tag (pre-release tags ignored). That patch is stamped into the
//   version files at build time and never committed.
import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { readVersions } from "./version-files.mjs";

const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/;

const cmp = (a, b) => {
  const [x, y] = [a, b].map((v) => v.split(".").map(Number));
  return x[0] - y[0] || x[1] - y[1] || x[2] - y[2];
};

/** Pure: picks the version from the event and the existing tag names. */
export function pickVersion({ event, inputVersion, refName, base, tags }) {
  if (event === "workflow_dispatch") return { version: inputVersion ?? "" };
  if (event === "push") return { version: (refName ?? "").replace(/^v/, "") };
  if (!tags.includes(`v${base}`)) return { version: base };
  const released = tags
    .map((t) => t.replace(/^v/, ""))
    .filter((v) => /^\d+\.\d+\.\d+$/.test(v))
    .concat(base)
    .sort(cmp);
  const [major, minor, patch] = released[released.length - 1].split(".").map(Number);
  const version = `${major}.${minor}.${patch + 1}`;
  return {
    version,
    notice: `v${base} is already released; releasing ${version} (stamped at build time, not committed).`,
  };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const tags = execFileSync("git", ["tag", "-l", "v*"], { encoding: "utf8" })
    .split("\n")
    .filter(Boolean);
  const { version, notice } = pickVersion({
    event: process.env.EVENT,
    inputVersion: process.env.INPUT_VERSION,
    refName: process.env.REF_NAME,
    base: readVersions().cargo,
    tags,
  });
  if (!SEMVER.test(version)) {
    console.log(`::error::Version '${version}' is not valid semver without a leading v (e.g. 0.8.0).`);
    process.exit(1);
  }
  if (notice) console.log(`::notice::${notice}`);
  console.log(version);
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\n`);
}
