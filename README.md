# Internet Speed Meter

A tiny Windows 11 taskbar widget that shows live download/upload speed and counts how much
data you use. Written in Rust against the raw Win32 API: no GUI framework, no runtime,
one small exe that idles at ~0% CPU.

```
 ...  ↓ 12.4 MB/s
      ↑  1.1 MB/s   ^  🔊  10:42
```

## How it works

- **Speed:** once a second (configurable) it reads the byte counters of every connected
  *hardware* adapter with `GetIfTable2` and divides the delta by the elapsed time. VPN,
  Hyper-V and tunnel adapters are skipped so traffic is not counted twice.
- **Taskbar display:** Windows 11 no longer allows third-party widgets inside the taskbar,
  so the meter is a click-through, always-on-top layered window placed on the taskbar just
  left of the notification area. It hides while a fullscreen app or game is running.
- **Usage:** daily download/upload totals are stored in `%APPDATA%\InternetSpeedMeter\usage.csv`
  (last 400 days). Hover the tray icon, or right-click it, to see today and this month.

## Settings

Right-click the tray icon: units (MB/s or Mbps), show download / upload / both, update
interval, text size, text color (auto follows the Windows theme), which network adapter to
watch, position nudge, start with Windows. Settings live in
`%APPDATA%\InternetSpeedMeter\settings.ini`.

## Building

CI builds everything: push to `main` and download the `InternetSpeedMeter` artifact
(installer + portable exe). Pushing a tag like `v0.1.0` also publishes a GitHub release.

Locally (needs the Rust MSVC toolchain): `cargo build --release`, then compile
`installer/speedmeter.iss` with [Inno Setup](https://jrsoftware.org/isinfo.php).
