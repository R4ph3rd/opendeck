// Builds the binaries Tauri ships next to the app (`bundle.externalBin` in
// src-tauri/tauri.sidecars.conf.json) and copies them to
// src-tauri/binaries/<name>-<target triple>[.exe], the names Tauri expects.
//
// Run by `beforeBuildCommand`: the Tauri CLI sets TAURI_ENV_TARGET_TRIPLE
// (`--target`, else the host) and TAURI_ENV_DEBUG (`--debug`).
// Manual use: node scripts/build-sidecars.mjs [--target <triple>] [--debug]
//
// `universal-apple-darwin` builds both darwin triples and joins them with
// `lipo`. The per-arch files are kept: tauri-build checks them during each
// arch's cargo build, the bundler takes the universal one.
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outDir = path.join(root, "src-tauri", "binaries");

/** Cargo package and bin target of each sidecar. */
const SIDECARS = [{ pkg: "opendeck-plugin-host", bin: "opendeck-plugin-scan" }];
const UNIVERSAL = "universal-apple-darwin";
const UNIVERSAL_PARTS = ["aarch64-apple-darwin", "x86_64-apple-darwin"];

function parseArgs(argv) {
  const out = { target: undefined, debug: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--target" || arg === "-t") out.target = argv[++i];
    else if (arg.startsWith("--target=")) out.target = arg.slice("--target=".length);
    else if (arg === "--debug" || arg === "-d") out.debug = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  return out;
}

function hostTriple() {
  const info = execFileSync("rustc", ["-vV"], { cwd: root, encoding: "utf8" });
  const host = /^host: (\S+)$/m.exec(info)?.[1];
  if (!host) throw new Error("could not read the host triple from `rustc -vV`");
  return host;
}

/** Builds one bin for one triple and returns the path of the executable. */
function cargoBuild({ pkg, bin }, triple, host, debug) {
  const args = ["build", "-p", pkg, "--bin", bin, "--message-format=json-render-diagnostics"];
  if (!debug) args.push("--release");
  // Without --target, cargo reuses target/<profile> from the app build.
  if (triple !== host) args.push("--target", triple);
  console.error(`build-sidecars: cargo ${args.join(" ")}`);
  const stdout = execFileSync("cargo", args, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
    maxBuffer: 256 * 1024 * 1024,
  });
  let exe;
  for (const line of stdout.split("\n")) {
    if (!line.startsWith("{")) continue;
    const msg = JSON.parse(line);
    if (msg.reason === "compiler-artifact" && msg.target?.name === bin && msg.executable) {
      exe = msg.executable;
    }
  }
  if (!exe) throw new Error(`cargo did not report the ${bin} executable`);
  return exe;
}

function install(src, name, triple) {
  const ext = triple.includes("windows") ? ".exe" : "";
  const dest = path.join(outDir, `${name}-${triple}${ext}`);
  copyFileSync(src, dest);
  if (process.platform !== "win32") chmodSync(dest, 0o755);
  console.error(`build-sidecars: ${path.relative(root, dest)}`);
  return dest;
}

const opts = parseArgs(process.argv.slice(2));
const host = hostTriple();
const target = opts.target ?? process.env["TAURI_ENV_TARGET_TRIPLE"] ?? host;
const debug = opts.debug || process.env["TAURI_ENV_DEBUG"] === "true";
mkdirSync(outDir, { recursive: true });

for (const sidecar of SIDECARS) {
  if (target === UNIVERSAL) {
    const parts = UNIVERSAL_PARTS.map((triple) =>
      install(cargoBuild(sidecar, triple, host, debug), sidecar.bin, triple),
    );
    const dest = path.join(outDir, `${sidecar.bin}-${UNIVERSAL}`);
    execFileSync("lipo", ["-create", "-output", dest, ...parts], { stdio: "inherit" });
    console.error(`build-sidecars: ${path.relative(root, dest)}`);
  } else {
    install(cargoBuild(sidecar, target, host, debug), sidecar.bin, target);
  }
}
