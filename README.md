# GFNUsage

Keep an eye on your GeForce NOW monthly playtime quota, from the Windows
system tray or the macOS menu bar.

[繁體中文說明](README.zh-TW.md) · [Changelog](CHANGELOG.md)

> **Status:** the v1 feature set is done on Windows. GFNUsage signs in with
> your NVIDIA account in a webview, keeps the session alive on its own, and
> shows the remaining hours in the tray alongside a pace threshold, an
> overrun projection, your per-session play history and the blackout
> windows you set up in the panel.
>
> **macOS is untested.** The sign-in flow exists precisely so a Mac can
> get its own session, but the development machine is a Windows box and
> nobody has run it on a Mac yet.

## Why

GeForce NOW Performance and Ultimate memberships come with 100 hours of
monthly playtime, plus up to 15 hours of unused time carried over — so a
single period tops out at 115 hours. The official client shows you a
number buried in a settings page. It will not tell you whether you are
burning through it too fast.

## Features

- ✅ **Remaining hours, always visible** — no need to open the GFN client
- ✅ **Rollover and purchased time** — shown as part of the period's total
- ✅ **Pace threshold** — how much *should* you have used by now? Go past it
  and the display turns red
- ✅ **Overrun prediction** — which day you run out, and by how much
- ✅ **Period chart** — how the quota has run down so far, and where the last
  seven days of play take it by month-end
- ✅ **Refreshes when a session ends** — on Windows, the numbers update a few
  minutes after you leave a game. Pick how often to refresh otherwise, or turn
  scheduled refreshes off
- ✅ **Blackout windows** — mark the hours you cannot play: work, sleep,
  anything. Predictions then divide by *available* time rather than
  wall-clock time. A weekend holds more than twice the free time of a
  weekday, so wall-clock arithmetic flags a perfectly normal Saturday
  evening as overspending
- ✅ **"How long can I play today"** — spreads the remaining quota across
  the available time left in the period
- ✅ **Rollover waste warning** — how many hours you are on track to leave
  unused, and how many of those will expire past the 15-hour cap
- ✅ **Sign in with your NVIDIA account** — OAuth in a webview, so a
  machine without the GeForce NOW client can get its own session
- ✅ **Recent sessions** — each game you played, when, and for how long
- ✅ **Settings export and import** — move one set of blackout windows
  between machines as a JSON file

## Install

Download the latest `GFNUsage_<version>_x64-setup.exe` from
[Releases](https://github.com/0BlueYan0/GFNUsage/releases) and run it.

It installs into your user folder, so Windows will not ask for
administrator rights. There is no code signing certificate behind this
build, so SmartScreen will say "Windows protected your PC" and name an
unknown publisher: choose **More info** → **Run anyway**.

The app checks for new versions and tells you when one is out. Nothing
installs until you click it.

**macOS is not guaranteed to work.** The `.dmg` is built by CI but nobody
has run it. It is neither signed nor notarised, so Gatekeeper blocks it on
first launch — right-click the app and choose **Open**, or allow it under
**System Settings → Privacy & Security**.

Uninstalling leaves three things behind, by design, so a reinstall picks up
where you left off: `%APPDATA%\tw.iosclub.gfnusage\`,
`%LOCALAPPDATA%\tw.iosclub.gfnusage\`, and the `GFNUsage` entries in
Windows Credential Manager.

## Build from source

Needs Node 22, a stable Rust toolchain, and the WebView2 runtime (Windows
11 has it already).

```
npm ci
npm run tauri dev      # run it, against the real NVIDIA API
npm run tauri build    # produce the installer
```

## How it works

GFNUsage reads the remaining time from your NVIDIA account rather than
counting seconds on the local machine, so the figure stays correct when
you play across several computers.

You sign in once, in a webview. What GFNUsage keeps is a one-hour token,
stored in the operating system's credential store — Windows Credential
Manager or the macOS Keychain — never in a plain file. When it expires the
app renews it in the background, without asking you again, for as long as
the webview's cookie lasts.

## Disclaimer

This project talks to private, undocumented interfaces of the GeForce NOW
client. Those can change or disappear at any time, and GFNUsage will stop
working when they do.

This project is not affiliated with, endorsed by, or sponsored by NVIDIA
Corporation. "NVIDIA", "GeForce", and "GeForce NOW" are trademarks of
NVIDIA Corporation.

## License

[Apache License 2.0](LICENSE)
