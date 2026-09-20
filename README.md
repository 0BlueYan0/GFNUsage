# GFNUsage

Keep an eye on your GeForce NOW monthly playtime quota, from the Windows
system tray or the macOS menu bar.

[繁體中文說明](README.zh-TW.md)

> **Status:** milestone 3 is done, which completes the v1 feature set on
> Windows. GFNUsage signs in with your NVIDIA account over a localhost
> loopback — or imports credentials from a machine that already has the
> GeForce NOW client — refreshes them on its own, and shows the remaining
> hours in the tray alongside a pace threshold, an overrun projection and
> the blackout windows you set up in the panel. Settings export and
> import, a snapshot history and a warning before the credential expires
> landed here too.
>
> **macOS is untested.** The sign-in flow exists precisely so a Mac can
> get credentials without the GFN client, but the development machine is
> a Windows box and nobody has run it on a Mac yet.

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
- ✅ **Blackout windows** — mark the hours you cannot play: work, sleep,
  anything. Predictions then divide by *available* time rather than
  wall-clock time. A weekend holds more than twice the free time of a
  weekday, so wall-clock arithmetic flags a perfectly normal Saturday
  evening as overspending
- ✅ **"How long can I play today"** — spreads the remaining quota across
  the available time left in the period
- ✅ **Rollover waste warning** — how many hours you are on track to leave
  unused, and how many of those will expire past the 15-hour cap
- ✅ **Sign in with your NVIDIA account** — a localhost loopback OAuth
  flow, so a machine without the GeForce NOW client can get credentials
  on its own
- ✅ **Settings export and import** — move one set of blackout windows
  between machines as a JSON file
- ✅ **Credential expiry warning** — the refresh token lasts 90 days;
  the panel says so a week before it runs out

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
