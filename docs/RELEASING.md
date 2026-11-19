# Releasing

Releases come only from the **Release** workflow (`.github/workflows/release.yml`), from a commit on `main` whose CI run passed. Milestone N ships as `v0.N.0`. Releases are pre-releases while the major version is 0.

## Steps

1. **Gate.** On `workbench`, CI is green and the checker gate passes (fmt, clippy, tests, bindings diff, UI checks, Storybook, `cargo deny check`). Complete the relevant section of `docs/hardware-test-checklist.md` if hardware is available.
2. **Bump.** `node scripts/bump-version.mjs 0.N.0`, then `cargo update -w` to refresh `Cargo.lock`. `node scripts/check-version-sync.mjs` must pass.
3. **Changelog.** Move the `[Unreleased]` entries in `CHANGELOG.md` under a new `[0.N.0] - YYYY-MM-DD` heading and start a fresh `[Unreleased]`. Update `docs/STATUS.md`.
4. **Commit** on `workbench`: `chore(release): milestone N, v0.N.0`.
5. **Promote.** Merge `workbench` into `main` (fast-forward or merge commit, no squash, so commit history feeds the release notes).
6. **Release workflow.** CI completing on `main` starts `release.yml`. It runs only if CI succeeded for that exact commit.

## Version

- Taken from the Cargo workspace version (`tauri.conf.json` has no version field, so Tauri uses it too). `scripts/check-version-sync.mjs` keeps `Cargo.toml` and `ui/package.json` equal.
- If `v<version>` is already released, the workflow releases the next patch after the highest `v*` tag (`0.8.0` to `0.8.1`) and stamps it into the version files at build time with `scripts/bump-version.mjs`. Nothing is committed back. The choice lives in `scripts/pick-release-version.mjs`.
- For a minor or major release, bump in a commit (steps above).

## Manual release

Actions, **Release**, **Run workflow** on `main`, with the version and `platforms`. Pushing a `v<version>` tag on a commit of `main` also works (always all platforms). Both need CI green for the commit and the version files equal to the version (`node scripts/check-version-sync.mjs <version>`), so bump first.

`platforms` (dispatch only): `all`, `windows`, or `windows,linux`. Use a reduced set to cut a cheap pre-release. The publish job then expects only the assets of the chosen platforms.

## What the workflow does

1. **verify**: the commit must be on `main`; ci.yml must have a successful run for it (automatic runs stop quietly otherwise, manual runs fail); pick and validate the version; refuse a version whose release is already published; delete a stale draft from an earlier failed run; choose the build legs; create the draft release `OpenDeck v<version>` with notes from `scripts/release-notes.mjs` (conventional commits since the previous tag plus `.github/release-notes-template.md`).
2. **build**: Windows (NSIS, MSI), macOS (universal DMG), Linux (deb, rpm, AppImage), all through `pnpm -C ui tauri`, which merges `src-tauri/tauri.sidecars.conf.json` and runs `scripts/build-sidecars.mjs` (plugin scanner sidecar, universal via `lipo` on macOS). Each leg uploads to the draft.
3. **publish**: only if every leg succeeded. Checks that the expected assets exist (`-setup.exe`, `.msi`, `.dmg`, `.deb`, `.rpm`, `.AppImage`), then publishes the draft (pre-release while major is 0, latest otherwise). If a leg fails the draft stays unpublished; run the workflow again to rebuild (the stale draft is deleted).

## Actions cost

The repository is private, so Actions minutes are billed with multipliers: Linux x1, Windows x2, macOS x10. Choices to keep the bill down:

- `ci.yml` skips docs-only changes (`docs/**`, `**.md`, `CONTEXT/**`) and cancels superseded runs.
- `feat/**` pushes run only the Linux Rust and UI jobs. The Windows job runs on `workbench` pushes and on pull requests into `main`.
- Release builds run only from `main`. macOS is the expensive leg: use `platforms` to skip it for pre-releases.
- Because docs-only commits skip CI, a `main` commit that is docs-only has no CI run and will not release. The release bump commit touches `Cargo.toml`, so normal promotions are unaffected.

## Code signing

Signing is automatic when secrets exist. Without them the build is unsigned and the job log shows one `::notice::` per platform.

### Windows

Repository secrets:

- `WINDOWS_CERTIFICATE`: the `.pfx`, base64-encoded (`[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))`)
- `WINDOWS_CERTIFICATE_PASSWORD`: its password

The workflow imports the certificate into the runner store, writes a Tauri config override (`certificateThumbprint`, `digestAlgorithm: sha256`, `timestampUrl: http://timestamp.digicert.com`) and builds with an extra `--config`; Tauri merges all `--config` values in order, so it layers on the sidecars config the wrapper adds. It then checks `opendeck.exe` and every installer with `Get-AuthenticodeSignature` and fails unless the status is `Valid`. The `opendeck-plugin-scan.exe` sidecar is checked but only warns: whether Tauri signs `externalBin` on Windows is unconfirmed, TODO(verify) with a real certificate. Verification runs after upload, so a failure leaves an unpublished draft.

Test certificate: `./scripts/new-test-certificate.ps1` writes a self-signed PFX (base64) and password to `test-cert/` (ignored by Git; never commit). It is not trusted, so the `Valid` check fails with it; use it only to see import and signing work, never for real releases.

### macOS

Secrets (Tauri reads them natively): `APPLE_CERTIFICATE` (base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`. To notarize also set `APPLE_ID`, `APPLE_PASSWORD` (app-specific password) and `APPLE_TEAM_ID`. Tauri signs the app bundle including the `opendeck-plugin-scan` sidecar (`externalBin`); the workflow then runs `codesign --verify --deep --strict` on the app and on the sidecar. Without the three signing secrets the build is unsigned; without the notarization trio it is signed but not notarized.

### Linux

No signing.

## What users see unsigned

- Windows: SmartScreen "Windows protected your PC". More info, then Run anyway.
- macOS: Gatekeeper blocks the first launch. Right-click the app, Open, confirm, or `xattr -dr com.apple.quarantine /Applications/OpenDeck.app`.

The release notes template carries this text. Reword it in `.github/release-notes-template.md` once both platforms are signed.

## When a release fails

- Read the failed job log. If the cause is in the code, fix it on a `feat/*` branch, merge to `workbench`, promote to `main`; the next patch is picked automatically.
- If the cause is the runner or a flaky download, re-run the failed jobs (the draft is reused), or run the workflow manually for a clean rebuild.
- Never edit a published release's assets by hand. A published version is never rebuilt.

## Windows installer identity

`bundle.windows.wix.upgradeCode` in `src-tauri/tauri.conf.json` is `6bcb09fc-a2c9-5bb7-a0aa-15cb2d440f6f`. It must never change: Windows Installer uses it to find an installed OpenDeck and upgrade it in place. A new code makes the next MSI install beside the old one. The value is the code Tauri derives from the binary name (UUID v5 of `opendeck.app.x64` in the DNS namespace), so the MSIs of v0.1.0 to v0.7.0 carry it too and upgrade cleanly. Keep `identifier` (`app.opendeck.desktop`) and the NSIS `installMode` stable for the same reason.
