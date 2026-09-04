# Session Persistence Design

**Goal:** Remember which mascots were live and their scale/speed across app restarts, so relaunching the app restores the previous session automatically.

## Background

There is currently no settings persistence in the app. `App::set_scale`/`set_speed` only mutate the in-memory `MascotInstance`, and `App::new` always starts with `mascots: Vec::new()` — nothing is restored on relaunch. The catalog (`catalog.json`, which mascot bundles are importable) already persists via `shimeji::importer::catalog::{load_catalog, save_catalog}`; this spec adds a parallel, independent persistence path for *live session state* (which mascots are currently spawned, and their scale/speed).

## Scope

- Persist: which mascots are spawned (by catalog slug) and their scale/speed.
- Not persisted: on-screen position. A restored mascot always spawns at the default spawn point (100, 100), same as a fresh manual spawn today.
- Not in scope: persisting anything about the settings window itself (open/closed, selected mascot) — that's ephemeral UI state, not session state.

## Architecture

A new `shimeji::session` module, structurally mirroring the existing `shimeji::importer::catalog` module:

```rust
// src/session.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub slug: String,
    pub scale: f64,
    pub speed: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("could not read session.json: {0}")]
    Read(#[source] std::io::Error),
    #[error("could not write session.json: {0}")]
    Write(#[source] std::io::Error),
    #[error("session.json is corrupt: {0}")]
    Parse(#[source] serde_json::Error),
}

pub fn load_session(path: &Path) -> Result<Vec<SessionEntry>, SessionError>;
pub fn save_session(path: &Path, entries: &[SessionEntry]) -> Result<(), SessionError>;
```

`load_session` returns `Ok(Vec::new())` when the file doesn't exist yet (same convention as `load_catalog`). `save_session` overwrites the file wholesale with `serde_json::to_string_pretty` (same convention as `save_catalog`) — no partial/merge writes.

The file lives at `<app_root>/session.json` (a sibling of `animations.log`, not inside `library_root`/the mascot bundle library) since it describes this app instance's live state, not the mascot library itself.

## `App` integration

- `App` gains a `session_path: PathBuf` field, and `App::new`'s signature grows a `session_path: PathBuf` parameter (computed in `main.rs` the same way `animations.log`'s path already is: `app_root.join("session.json")`).
- `MascotInstance` gains a `slug: String` field, set at spawn time from the `CatalogEntry` that was spawned. This is required because `SessionEntry` identifies mascots by slug (instance ids are per-process and meaningless across a restart), and today `MascotInstance` has no way to recover which catalog entry it came from.
- A private `App::save_session(&self)` method builds a `Vec<SessionEntry>` from `self.mascots` (`slug`, `scale`, `speed`) and calls `session::save_session`, logging (not propagating) any error via the existing `crate::logging::log_error` path — consistent with every other fallible operation in `App` (import, spawn, settings-window creation).
- `save_session` is called at the end of: `spawn` (after a successful push), `close`, `close_all`, `set_scale`, `set_speed`. These are exactly the operations that change what a restored session would look like.
- A private `App::restore_session(&mut self)` method runs once, at the end of `App::new`: it calls `session::load_session`, and for each `SessionEntry`, calls `self.spawn(&entry.slug)` and then — only if `self.mascots.len()` actually grew (an entry's slug may no longer exist in the catalog, or its bundle may now fail to load) — writes `entry.scale`/`entry.speed` directly onto the newly-pushed `MascotInstance`. After the loop, `self.save_session()` is called once more, so that any entries that failed to restore (missing/broken bundle) are pruned from the persisted file rather than being retried forever on every future launch.

## Error handling

Both `load_session` and `save_session` return a typed `Result`; `App` never propagates these errors or shows a dialog — every failure (missing permissions, disk full, corrupt JSON) is logged via `crate::logging::log_error("session", ...)` and the app continues with whatever in-memory state it has. This matches the existing error-handling convention throughout `App` (see `import_from_bytes`, `spawn`).

## Testing

- Unit tests on `shimeji::session`, following the exact shape of `catalog.rs`'s existing tests: round-trips a `Vec<SessionEntry>` through `save_session`/`load_session` in a `tempfile::tempdir()`; missing file → `Ok(vec![])`; corrupt JSON → `Err(SessionError::Parse(_))`.
- No new UI/integration test for restore-on-startup specifically — `App::new` isn't currently under integration test, and standing up a Win32 owner window + tray icon in a test harness is out of scope here. Restore logic is exercised indirectly through the `session` module's own tests (which cover the round-trip this depends on) plus manual verification (spawn a couple of mascots, adjust scale/speed, restart the app, confirm they come back).
