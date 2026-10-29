// Usage: node scripts/release-notes.mjs <version> [previous-tag]
// Prints release notes from conventional commits since the previous tag.
// No network, no dependencies; needs git history (fetch-depth: 0).
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const version = process.argv[2];
if (!version) {
  console.error("usage: node scripts/release-notes.mjs <version> [previous-tag]");
  process.exit(1);
}

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();

let prev = process.argv[3];
if (prev === undefined) {
  try {
    prev = git("describe", "--tags", "--abbrev=0", "--match", "v*", "HEAD");
  } catch {
    prev = "";
  }
}

const range = prev ? `${prev}..HEAD` : "HEAD";
const SEP = "\u001f";
const log = git("log", range, "--no-merges", `--format=%h${SEP}%s`);

const groups = [
  ["feat", "Features"],
  ["fix", "Fixes"],
  ["perf", "Performance"],
  ["refactor", "Refactoring"],
  ["build", "Build"],
  ["ci", "CI"],
  ["docs", "Documentation"],
  ["test", "Tests"],
  ["chore", "Chores"],
  ["style", "Style"],
];
const buckets = new Map(groups.map(([type]) => [type, []]));
const other = [];
const re = /^(\w+)(?:\(([^)]+)\))?(!)?:\s*(.+)$/;

for (const line of log.split("\n").filter(Boolean)) {
  const [hash, subject] = line.split(SEP);
  const m = re.exec(subject);
  if (m && buckets.has(m[1])) {
    const scope = m[2] ? `**${m[2]}:** ` : "";
    buckets.get(m[1]).push(`- ${scope}${m[4]}${m[3] ? " (breaking)" : ""} (${hash})`);
  } else if (!/^chore\(release\)/.test(subject)) {
    other.push(`- ${subject} (${hash})`);
  }
}

const out = [`# OpenDeck v${version}`, ""];
if (prev) out.push(`Changes since ${prev}.`, "");
for (const [type, title] of groups) {
  const items = buckets.get(type);
  if (items.length) out.push(`## ${title}`, "", ...items, "");
}
if (other.length) out.push("## Other", "", ...other, "");

const template = path.join(here, "..", ".github", "release-notes-template.md");
if (existsSync(template)) out.push(readFileSync(template, "utf8").trimEnd(), "");

process.stdout.write(out.join("\n"));
