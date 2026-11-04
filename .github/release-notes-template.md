## Known limitations

- Binaries are code-signed only when the release had signing certificates configured; if your OS warns about an unknown publisher:
  - Windows: SmartScreen shows "Windows protected your PC". Click "More info", then "Run anyway".
  - macOS: Gatekeeper blocks the first launch. Right-click the app, choose "Open", then confirm. Or run `xattr -dr com.apple.quarantine /Applications/OpenDeck.app`.
- ASIO is not included in CI builds (the Steinberg SDK cannot be redistributed). Windows builds use WASAPI. To use ASIO, build locally: see docs/BUILD.md.
- Pre-1.0: settings and library data may change between releases.
