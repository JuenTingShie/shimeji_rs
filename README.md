# Shimeji (native Windows, no runtime install)

A desktop mascot app. Import a mascot from a `.zip` (the `legacy_default_v1`
manifest.json + animation.json + sprites/ schema) and it walks, falls, climbs,
and hangs around your desktop — including on top of your other open windows.

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

## Test

    cargo test

Runs the unit and integration tests covering format parsing, bundle
validation, the animation state machine, and the environment surface
geometry. Win32 window/tray/input code is not covered by automated tests —
it was verified manually by running the app (see
`docs/superpowers/plans/2026-09-03-windows-native-shimeji.md`, Task 18/19,
for the checklist and what was found).
