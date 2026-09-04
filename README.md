# Shimeji (native Windows, no runtime install)

A desktop mascot app. Import a mascot from a `.zip` (the `legacy_default_v1`
manifest.json + animation.json + sprites/ schema) and it walks, falls, climbs,
and hangs around your desktop — including on top of your other open windows.

- **No install** — a single native `.exe`, no runtime or installer.
- **Multiple mascots at once**, each independently draggable, flingable, and
  closable.
- **One Settings window** for the whole app — pick any live mascot from a
  dropdown and adjust its scale/speed, no matter how many are open.
- **Session persistence** — whichever mascots are open (and their
  scale/speed) are saved to `session.json` and restored automatically the
  next time you launch the app.
- **Drag-and-drop import** — drop a `.zip` onto the tray icon, or use the
  tray menu's "Import Mascot..." picker.

## Build

    cargo build --release

Output: `target/release/shimeji.exe`. That single file is the whole app — no
installer, no runtime to install alongside it.

## Run

Double-click `shimeji.exe`, or `cargo run --release`. Use the system tray icon
to import a mascot (drag a `.zip` onto the tray icon, or use "Import
Mascot...") and spawn it.

**First run:** Windows hides new tray icons in the "Show hidden icons" `^`
overflow area next to the clock by default. If right-clicking where you'd
expect the icon does nothing, click the `^` chevron first — the icon (and
its menu) is in there. Drag it out onto the visible taskbar, or set it to
always show, from that same flyout to keep it visible next time.

### Tray menu

Right-click the tray icon:

- One entry per imported mascot — click to spawn another instance of it.
- **Import Mascot...** — pick a `.zip` from a file dialog.
- **Settings** (shown once at least one mascot is open) — opens the single
  settings window described below.
- **Close All** — closes every live mascot (and saves that empty state, so
  they won't come back on next launch unless you spawn them again).
- **Exit** — quits the app. Whatever mascots are open at the time are left
  in `session.json` and restored automatically next launch.

Right-click a mascot itself for a per-mascot menu: **Jump**, **Settings...**
(opens the settings window focused on that mascot), and **Close**.

### Settings window

One window manages every live mascot — pick which one to edit from the
dropdown at the top, then adjust:

- **Scale** — 10% to 400% of the mascot's native sprite size.
- **Speed** — 25% to 300% of its default animation speed.

Closing the window (the OS close button) just hides it; reopening it from
the tray or a mascot's right-click menu shows it again instantly.

### Session persistence

On every spawn, close, scale/speed change, and clean Exit, the current set
of live mascots (slug, scale, speed — not window position) is written to
`%APPDATA%\ShimejiRust\session.json`. On the next launch, that file is
read back and every mascot in it is respawned with its saved scale/speed.
Deleting `session.json` (or exiting via **Close All** first) starts the
next launch with no mascots.

### Logging

The app does not log by default. Pass `--log` on the command line
(`shimeji.exe --log`) to write animation/error diagnostics to
`%APPDATA%\ShimejiRust\animations.log`.

## Test

    cargo test

Runs the unit and integration tests covering format parsing, bundle
validation, the animation state machine, and the environment surface
geometry. Win32 window/tray/input code is not covered by automated tests —
it was verified manually by running the app (see
`docs/superpowers/plans/2026-09-03-windows-native-shimeji.md`, Task 18/19,
for the checklist and what was found).

## License

MIT — see [LICENSE](LICENSE).
