# CLAUDE.md

Guidance for Claude Code when working in this repo.

## What this is

A native Windows desktop "Shimeji" mascot app in Rust — no installer, no
runtime, single `.exe`. Imports mascot bundles from `.zip` (manifest +
animation.json + sprites), spawns them as layered, click-through-free,
topmost Win32 windows that walk/fall/climb around the desktop.

## Build / test / run

    cargo build --release   # -> target/release/shimeji.exe
    cargo test               # unit + integration tests
    cargo run --release

If `cargo` isn't on PATH in your shell, use the full path to
`cargo.exe` under the user's `.cargo/bin`.

`cargo build --release` fails with an "Access is denied" error if a
previously built `shimeji.exe` is still running — kill it first (and tell
the user, don't silently force-kill their running app without saying so).

## Architecture

- **`src/main.rs`** — process entry point. Owns a custom `PeekMessageW`
  message loop (not `eframe`'s/winit's loop) driven by a 60Hz `SetTimer`,
  because the app is fundamentally a Win32 app; egui is used only for the
  settings window (see below), on its own thread.
- **`src/app.rs`** — `App`/`MascotInstance`: the live-mascot registry,
  spawn/close/scale/speed logic, session save/restore, tray menu refresh.
- **`src/window/`** — per-mascot layered Win32 windows (`mascot_window.rs`:
  `UpdateLayeredWindow`, hit-testing, drag/fling input).
- **`src/environment/`** — desktop surface geometry (monitors, taskbar
  edges) the physics/state machine falls and climbs against.
- **`src/format/`** — mascot bundle schema: manifest, animation, sprites.
- **`src/importer/`** — `.zip` import + on-disk catalog of installed
  mascots (`%APPDATA%\ShimejiRust\mascots\catalog.json`).
- **`src/state_machine/`** — weighted animation state transitions.
- **`src/tray/`** — system tray icon + menu (`tray-icon`/`muda`).
- **`src/settings_ui.rs`** — the one app-wide settings window (`eframe`/
  `egui`, glow renderer). See "winit one-event-loop" below.
- **`src/session.rs`** — persists live mascots (slug/scale/speed, not
  window position) to `%APPDATA%\ShimejiRust\session.json`; restored on
  next launch. Mirrors the `importer::catalog` module's plain
  `serde_json` + `std::fs` + `thiserror` pattern.
- **`src/logging.rs`** — no-ops unless `logging::init()` is called; gated
  behind the `--log` CLI flag in `main.rs` (off by default).

## Load-bearing constraints

- **winit allows exactly one `EventLoop` per process** (see
  `winit-*/src/event_loop.rs`'s `EVENT_LOOP_CREATED` static — never reset
  outside wasm). The settings window is therefore spawned once and never
  destroyed: its OS close button is intercepted
  (`ViewportCommand::CancelClose` + `Visible(false)`) to hide instead of
  close, and later "open" requests are delivered over an `mpsc` channel to
  the still-running window thread. Do not "fix" this by spawning a fresh
  window/thread per open — it will panic (or silently fail) on the second
  call.
- **Never simulate cursor/mouse input** (`SetCursorPos`, `SendInput`,
  etc.) to test UI — the user tests interactively themselves. Automated
  tests cover format parsing, bundle validation, the state machine, and
  environment geometry; Win32 window/tray/input code is verified manually.
- `App::close_all()` (tray "Close All") intentionally persists an empty
  session — that's a real user action. Process shutdown (tray "Exit")
  must use `App::destroy_all_windows()` instead, which does *not* touch
  `session.json`, so the current session survives a restart.

## Releases

CI (`.github/workflows/release.yml`) auto-publishes a GitHub Release with
`shimeji.exe` attached whenever the `version` in `Cargo.toml` changes on
`main`. Bump the version before merging to cut a release; otherwise the
workflow is a no-op.
