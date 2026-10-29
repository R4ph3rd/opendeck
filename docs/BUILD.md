# Building OpenDeck

Primary target is Windows x64. macOS and Linux build from the same sources.

## Windows (clean machine to installer)

1. Install Visual Studio Build Tools 2022 with the "Desktop development with C++" workload (MSVC, Windows SDK). `signalsmith-stretch` compiles C++, so a C++ compiler is required.
2. Install Rust with [rustup](https://rustup.rs), host `x86_64-pc-windows-msvc`. The repo pins the toolchain in `rust-toolchain.toml`; rustup installs it on the first `cargo` call.
3. Install Node 22, then enable pnpm 10 through corepack:
   ```
   corepack enable
   corepack prepare pnpm@10.28.0 --activate
   ```
4. WebView2 is preinstalled on Windows 10 (recent) and Windows 11. If missing, install the [Evergreen bootstrapper](https://developer.microsoft.com/microsoft-edge/webview2/#download).
5. From the repo root:
   ```
   pnpm install
   pnpm -C ui install
   pnpm -C ui tauri build
   ```
6. Output:
   - `target\release\bundle\nsis\*.exe` (installer)
   - `target\release\bundle\msi\*.msi`
   - `target\release\opendeck.exe` (portable binary, no installer)

For a quick compile check without bundling: `pnpm -C ui tauri build --no-bundle`. For development: `pnpm -C ui tauri dev`.

## ASIO (optional, local builds only)

The Steinberg ASIO SDK licence forbids redistributing it, so it is never committed (`.gitignore` excludes it) and release binaries use WASAPI only.

1. Download the ASIO SDK from [Steinberg](https://www.steinberg.net/developers/) and unzip it outside the repo.
2. Install LLVM (clang) for bindgen and set `LIBCLANG_PATH` to its `bin` folder if it is not found.
3. Set `CPAL_ASIO_DIR` to the SDK folder (the one containing `common` and `host`).
4. Build with the cargo feature: `cargo build --release --features asio`.
   TODO: the `asio` feature is not wired into `opendeck-engine` / `opendeck-desktop` yet (ADR 0001 plans it as a cargo feature forwarding to `cpal/asio`). Until then the steps above do nothing.

ASIO in CI would need the SDK from a private location: store a zip as a private artifact or release asset, expose its download token as a repository secret (`ASIO_SDK_URL`), and add a CI step that downloads and unzips it before setting `CPAL_ASIO_DIR`. Do not upload such a build to a public release.

## macOS

```
xcode-select --install
rustup target add aarch64-apple-darwin x86_64-apple-darwin
corepack enable && pnpm install && pnpm -C ui install
pnpm -C ui tauri build --target universal-apple-darwin --bundles dmg
```

Output: `target/universal-apple-darwin/release/bundle/dmg/*.dmg`.

## Linux

Debian/Ubuntu build dependencies:

```
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libasound2-dev libudev-dev patchelf build-essential
```

Then `pnpm install && pnpm -C ui install && pnpm -C ui tauri build --bundles appimage,deb`. Output is in `target/release/bundle/appimage` and `deb`.

Runtime dependencies of the installed app: `libwebkit2gtk-4.1`, `libasound2`, `libayatana-appindicator3`.

## Unsigned binaries

Releases are not code-signed yet.

- Windows: SmartScreen shows "Windows protected your PC". Click "More info", then "Run anyway".
- macOS: Gatekeeper blocks the first launch. Right-click the app, choose "Open", confirm. Or run `xattr -dr com.apple.quarantine /Applications/OpenDeck.app`.

## Checks and bindings

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm -C ui typecheck && pnpm -C ui lint && pnpm -C ui test
pnpm -C ui build-storybook
cargo deny check
```

Regenerate the TypeScript IPC bindings after changing any command or contract type (ADR 0004):

```
cargo run -p opendeck-desktop --bin export-bindings
```

Commit `ui/src/ipc/bindings.ts` with the change. CI fails if it is stale.
