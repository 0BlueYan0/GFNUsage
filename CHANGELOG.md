# Changelog

[繁體中文](CHANGELOG.zh-TW.md)

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the version numbers follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Every released version needs a section here. The release workflow reads the
one matching the tag into the release notes and refuses to build without it.

## [Unreleased]

## [0.1.3] - 2026-09-22

### Fixed

- The panel really does reload when it is shown. 0.1.2 sent the event but the
  window was never granted permission to listen for it, so nothing arrived and
  the panel still sat on the numbers it read at startup.

## [0.1.2] - 2026-09-22

### Fixed

- The panel shows the current numbers. It read them once at startup and never
  again, so the tray could carry a number while the panel beside it said there
  was no data.
- Checking for updates gives up after 12 seconds and, if the system proxy
  failed, tries once more without it. A check could sit there for almost two
  minutes, and a proxy that reaches NVIDIA does not necessarily reach GitHub.
- Checking for updates no longer disables the buttons on the main panel.

## [0.1.1] - 2026-09-22

### Fixed

- Checking for updates says what it found: that you are on the latest version,
  or why it could not tell. Both used to leave the screen unchanged.
- On macOS, installing an update finishes and restarts. It used to leave the
  About page on "installing" for good. Windows was never affected.
- The update banner's text lines up with its buttons.
- The settings page keeps its title and back button in view while the list
  scrolls, the way the play history page does.

## [0.1.0] - 2026-09-21

First public release. Windows only in practice — macOS is built by CI and has
never been run.

### Added

- Remaining monthly hours in the Windows system tray and the macOS menu bar.
- A pace threshold and an overrun projection measured against the time you can
  actually play, not the wall clock.
- How much you can still play today, and how many hours you are on track to
  leave unused.
- Your recent play sessions: each game, when, and for how long.
- Blackout windows you set in the panel, with export and import.
- Sign-in with an NVIDIA account in a webview, renewed in the background.
- An update check, with the install one click away.
- Start with Windows.

[Unreleased]: https://github.com/0BlueYan0/GFNUsage/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/0BlueYan0/GFNUsage/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/0BlueYan0/GFNUsage/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/0BlueYan0/GFNUsage/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/0BlueYan0/GFNUsage/releases/tag/v0.1.0
