# Releasing

A release is a milestone promotion: `workbench` merges into `main`, and the release workflow builds and publishes. Milestone N ships as `v0.N.0`.

## Steps

1. **Gate.** On `workbench`, CI is green and the checker gate passes (fmt, clippy, tests, bindings diff, UI checks, Storybook, `cargo deny check`). Complete the relevant section of `docs/hardware-test-checklist.md` if hardware is available.
2. **Bump.** `node scripts/bump-version.mjs 0.N.0`, then `cargo update -w` to refresh `Cargo.lock`. `node scripts/check-version-sync.mjs` must pass.
3. **Changelog.** Move the `[Unreleased]` entries in `CHANGELOG.md` under a new `[0.N.0] - YYYY-MM-DD` heading and start a fresh `[Unreleased]`. Update `docs/STATUS.md`.
4. **Commit** on `workbench`: `chore(release): milestone N, v0.N.0`.
5. **Promote.** Merge `workbench` into `main` (fast-forward or merge commit, no squash, so commit history feeds the release notes).
6. **Release workflow.** The push to `main` starts `release.yml`: it reads the version, skips if tag `v0.N.0` already exists, creates a draft pre-release with notes from conventional commits, builds Windows (nsis, msi), macOS (universal dmg) and Linux (appimage, deb), then un-drafts the release. If any build fails the release stays a draft and nothing is published.

## When a release fails

- Read the failed job log. If the cause is in the code, fix it on a `feat/*` branch, merge to `workbench`, bump the patch version (`0.N.1`), and promote again.
- If the cause is the runner or a flaky download, re-run the failed jobs. The draft release is reused.
- If a draft was left behind and the version is being replaced, delete the draft release before pushing again, otherwise the workflow skips (the tag or release exists).
- Never edit a published release's assets by hand.

## Windows installer identity

`bundle.windows.wix.upgradeCode` in `src-tauri/tauri.conf.json` is `6bcb09fc-a2c9-5bb7-a0aa-15cb2d440f6f`. It must never change: Windows Installer uses it to find an installed OpenDeck and upgrade it in place. A new code makes the next MSI install beside the old one. The value is the code Tauri derives from the binary name (UUID v5 of `opendeck.app.x64` in the DNS namespace), so the MSIs of v0.1.0 to v0.7.0 carry it too and upgrade cleanly. Keep `identifier` (`app.opendeck.desktop`) and the NSIS `installMode` stable for the same reason.

## Signing (later)

Builds are unsigned. To enable signing, add the secrets named in the commented `env` block of `release.yml` (`WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`, `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, `TAURI_SIGNING_PRIVATE_KEY`) and uncomment those lines. Remove the unsigned warning from `.github/release-notes-template.md` once both platforms are signed.
