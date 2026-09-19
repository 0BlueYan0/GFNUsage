# GFNUsage

Keep an eye on your GeForce NOW monthly playtime quota, from the Windows
system tray or the macOS menu bar.

[繁體中文說明](README.zh-TW.md)

> **Status:** milestone 1 is done. GFNUsage imports credentials from a
> local GeForce NOW install, refreshes them on its own, and shows the
> remaining hours in the Windows tray or the macOS menu bar. Pace
> thresholds and prediction land in milestone 2.

## Why

GeForce NOW Performance and Ultimate memberships come with 100 hours of
monthly playtime, plus up to 15 hours of unused time carried over — so a
single period tops out at 115 hours. The official client shows you a
number buried in a settings page. It will not tell you whether you are
burning through it too fast.

## Features

- ✅ **Remaining hours, always visible** — no need to open the GFN client
- ✅ **Rollover and purchased time** — shown as part of the period's total
- **Pace threshold** — how much *should* you have used by now? Go past it
  and the display turns red
- **Overrun prediction** — which day you run out, and by how much
- **Blackout windows** — mark the hours you cannot play: work, sleep,
  anything. Predictions then divide by *available* time rather than
  wall-clock time. A weekend holds more than twice the free time of a
  weekday, so wall-clock arithmetic flags a perfectly normal Saturday
  evening as overspending
- **"How long can I play today"** — spreads the remaining quota across
  the available time left in the period
- **Rollover waste warning** — how many hours you are on track to leave
  unused, and how many of those will expire past the 15-hour cap

## How it works

GFNUsage reads the remaining time from your NVIDIA account rather than
counting seconds on the local machine, so the figure stays correct when
you play across several computers.

Credentials come from one of two places: an OAuth login over a localhost
loopback, or an import from a machine that already has the GeForce NOW
client installed. They live in the operating system's credential store —
Windows Credential Manager or the macOS Keychain — never in a plain file.

## Disclaimer

This project talks to private, undocumented interfaces of the GeForce NOW
client. Those can change or disappear at any time, and GFNUsage will stop
working when they do.

This project is not affiliated with, endorsed by, or sponsored by NVIDIA
Corporation. "NVIDIA", "GeForce", and "GeForce NOW" are trademarks of
NVIDIA Corporation.

## License

[Apache License 2.0](LICENSE)
