# Windows Native Shimeji — Design

## Goal

A desktop mascot ("Shimeji") app for Windows that:

- Requires **no installed runtime** — no JVM, no .NET runtime, nothing the user has to
  separately install. The distributed artifact is a single native `.exe`.
- Lets users **import custom mascots from a `.zip` bundle**, via drag-and-drop or a file
  picker.
- Replicates classic Shimeji-ee desktop behavior: mascots walk around the screen, climb
  walls and ceilings, and can walk/climb along the edges of **other running applications'
  windows**, not just the screen bounds.
- Supports **multiple simultaneous mascots** on screen at once.

Test fixture: `8ge8jqm7.zip` (checked into the repo root) — a real exported mascot bundle
used to validate the importer and animation interpreter end-to-end.

## Input format (from the test fixture)

The zip is a modern JSON-based export format, not the legacy Java Shimeji XML format:

```
manifest.json       — bundle metadata, sprite sheet description
animation.json       — animation state machine (schema_id "legacy_default_v1")
sprites/%04d.webp    — individual sprite frames (70 in the sample, 512x512 each)
thumbnail.webp
```

`manifest.json` declares `schemaVersion`, sprite count/pattern/size, and points at
`animation.json` via `animationSchema.{path, schemaId, version}`.

`animation.json` defines:

- `animations[]`: each has a `key`, `type` (`GROUND`/`WALL`/`CEILING`/`AIR`/`USER`),
  `subtype`, `level` (unlock tier, 1-4 in the sample), `loop` (`ONESHOT`/`LOOP`),
  `direction`, a `frames[]` list (`sprite` index, `dx`/`dy` per-frame movement,
  `durationTicks`), and optional `auto.onTimer`/`auto.onFinish` weighted random
  transitions plus `borderTransitions` (what to do when hitting the edge of the current
  surface, keyed by `LEFT`/`RIGHT`/`TOP`/`BOTTOM`).
- `events[]`: transitions triggered by user/engine events — `DRAG_START`/`DRAG_END`,
  `FLING_START`/`FLING_END`, `JUMP`, `IDLE`, `TAP` — matched against the mascot's current
  animation type/level and optionally which edge it's touching.

This is fully data-driven, so the engine only needs to implement a generic interpreter for
this schema — no per-mascot code.

## Architecture

Single Rust process, single Win32 message loop, hosting N per-mascot **layered windows**:
borderless, always-on-top, `WS_EX_LAYERED`, rendered via `UpdateLayeredWindow` with true
per-pixel alpha from the decoded WebP frames. This is the same technique the original Java
Shimeji uses to get a transparent, non-rectangular window.

A central **Environment** module tracks the "world" mascots walk on: the virtual screen
bounds across all monitors, plus the rects of every other visible top-level window on the
desktop (`EnumWindows`, filtered to visible/non-cloaked/non-tool windows, excluding the
app's own windows). It refreshes on a throttled timer (~150ms), not every frame — enumerating
every window every tick would be wasteful and other apps' windows don't move that often.
Each mascot's state machine queries this each tick to resolve `GROUND`/`WALL`/`CEILING`
border transitions against whatever surface — screen edge or another app's window edge — it
is currently on or approaching.

A single ~60Hz timer drives all mascots each tick:

1. Advance the current animation's frame timer.
2. Evaluate `onTimer`/`onFinish`/`borderTransitions` (weighted random choice) against the
   Environment query for the mascot's current position/surface.
3. Apply the resulting `dx`/`dy`, move the window, blit the new frame.

## Components

- **mascot-format** — parses/validates `manifest.json` + `animation.json` against the
  schema above, decodes the WebP sprite sequence into raw RGBA frame buffers.
- **importer** — takes zip bytes (from file picker or drag-drop), validates fully into a
  temp directory, and only on success unpacks into
  `%APPDATA%\ShimejiRust\mascots\<slug>\` and records it in a small catalog file there.
  Nothing is written to the permanent library on a failed import.
- **environment** — monitor enumeration (virtual screen rect) + other-window rect
  tracking; exposes a query API answering "what surface, if any, is at/adjacent to point
  (x, y) in direction d".
- **state-machine** — generic interpreter for the animation.json schema: current
  animation key + tick count + Environment query result → next frame/position, handling
  weighted transitions and the `DRAG`/`FLING`/`JUMP`/`IDLE`/`TAP` events. Independent of
  any specific mascot — it works for any bundle matching the schema.
- **mascot-window** — the layered window for one live mascot instance: creation,
  per-pixel `WM_NCHITTEST` (clicks pass through transparent pixels, hit the character
  where opaque), mouse input (drag start/move/release → drag/fling/tap), position
  tracking, blitting the current frame.
- **tray** — one system tray icon (`Shell_NotifyIcon`) for the whole app, with a context
  menu: `Spawn ▸ [imported mascots]`, `Import Mascot...` (file picker), `Close All`,
  `Exit`. Accepts drag-dropped `.zip` files on both the tray icon and any live mascot
  window (`DragAcceptFiles`/`WM_DROPFILES`).
- **app** — wires it together: message loop, tick timer, the live mascot instance list,
  the library catalog.

## Data flow

```
.zip (drag-drop or file picker)
  → importer validates & unpacks
  → catalog entry
  → listed in tray's "Spawn" menu
  → user picks it
  → new mascot window spawned, state-machine seeded at animation.json's default_animation
  → each tick: state-machine.step(environment_query) → new frame/position → window repaints/moves
```

## Interaction

Driven directly by the `events[]` the sample defines, so these are required, not optional:

- **Drag**: mouse-down on an opaque mascot pixel and move → `DRAG_START`, animation
  switches to the `drag` state and follows the cursor; mouse-up → `DRAG_END` (or `FLING`
  if released above a velocity threshold, carrying the release velocity into the fling
  animation).
- **Tap**: mouse-down/up without meaningful movement → `TAP`, resolved against the
  mascot's current type/level per the event table.
- **Idle**: no interaction for a period → `IDLE` event.

## Error handling

- Invalid zip (schema/version mismatch, missing `manifest.json`/`animation.json`, missing
  a referenced sprite file, corrupt WebP) → import rejected with a tray notification
  explaining why; nothing written to the library.
- A transition in `animation.json` referencing an unknown animation key → defensively
  logged and falls back to `default_animation`, rather than crashing (not expected with
  well-formed bundles, but the interpreter shouldn't panic on bad data).
- Windows the app can't inspect (e.g. elevated processes) are skipped as climbable
  surfaces — the mascot treats them as not present rather than erroring.

## Testing

- Unit tests for `mascot-format` against `8ge8jqm7.zip` as a fixture, plus a few
  hand-broken variants (bad schema version, missing sprite file, malformed JSON).
- Unit tests for `state-machine` transition resolution using a seeded RNG, covering
  `onFinish` vs `onTimer` vs `borderTransitions` precedence, independent of any real
  window or Environment.
- Manual/integration verification: run the app, drag-and-drop `8ge8jqm7.zip` in, spawn
  it, and visually confirm walk/fall/climb/ceiling-hang/drag/fling behaviors against real
  windows (e.g. a browser, Notepad) and, if available, across multiple monitors.
- No automated visual/UI testing is planned — asserting on a transparent desktop overlay
  programmatically isn't practical. This is a deliberate scope limit, not an oversight.

## Distribution

Single portable `.exe`, no installer. Imported mascots and the catalog live in
`%APPDATA%\ShimejiRust\`.

## Explicitly out of scope for this version

- Auto-start on Windows login.
- Restoring previously-spawned mascots across app restarts (a fresh launch starts with
  zero mascots on screen until the user spawns one from the tray).
- Any settings/config UI beyond the tray context menu.
