# Session Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remember which mascots were live and their scale/speed across app restarts, so relaunching the app automatically restores the previous session.

**Architecture:** A new `shimeji::session` module (data-only, no Win32 dependency) provides `load_session`/`save_session` over a `session.json` file, mirroring the existing `shimeji::importer::catalog` module. `App` calls `save_session` after every mutation to its live mascot list (spawn/close/close_all/set_scale/set_speed) and calls a one-time `restore_session` at the end of `App::new`, which re-spawns each saved mascot by catalog slug and reapplies its saved scale/speed.

**Tech Stack:** Rust, `serde`/`serde_json` (already a dependency), `thiserror` (already a dependency), `tempfile` (already a dev-dependency, used in tests).

## Global Constraints

- Persist only: catalog slug, scale, speed. Position is explicitly NOT persisted — restored mascots always spawn at the default point (100, 100), same as `spawn`'s existing hardcoded default.
- `session.json` lives at `<app_root>/session.json` — a sibling of `animations.log`, NOT inside `library_root` (the `<app_root>/mascots` catalog directory).
- Every persistence failure (read or write) is logged via the existing `crate::logging::log_error("session", ...)` path and otherwise ignored — never a panic, never a propagated error, never a dialog. This matches every other fallible `App` operation (see `import_from_bytes`, `spawn`).
- An entry whose slug no longer resolves to a spawnable bundle is silently dropped from the restored session and pruned from `session.json` on the next save — never retried forever.

---

### Task 1: `shimeji::session` module

**Files:**
- Create: `src/session.rs`
- Modify: `src/lib.rs` (register the new module)

**Interfaces:**
- Produces (consumed by Task 2):
  - `pub struct SessionEntry { pub slug: String, pub scale: f64, pub speed: f64 }` (derives `Debug, Clone, PartialEq, Serialize, Deserialize`)
  - `pub enum SessionError { Read(std::io::Error), Write(std::io::Error), Parse(serde_json::Error) }` (derives `Debug`, `thiserror::Error`)
  - `pub fn load_session(path: &std::path::Path) -> Result<Vec<SessionEntry>, SessionError>` — returns `Ok(vec![])` if the file doesn't exist.
  - `pub fn save_session(path: &std::path::Path, entries: &[SessionEntry]) -> Result<(), SessionError>` — creates the file's parent directory if needed, overwrites the whole file with pretty-printed JSON.

- [ ] **Step 1: Write `src/session.rs` with types, stub functions, and the full test module**

```rust
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

pub fn load_session(_path: &Path) -> Result<Vec<SessionEntry>, SessionError> {
    todo!()
}

pub fn save_session(_path: &Path, _entries: &[SessionEntry]) -> Result<(), SessionError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_loads_as_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.json");
        assert_eq!(load_session(&path).unwrap(), Vec::new());
    }

    #[test]
    fn round_trips_session_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.json");
        let entries = vec![
            SessionEntry { slug: "usagi".into(), scale: 1.0, speed: 1.0 },
            SessionEntry { slug: "neko".into(), scale: 1.5, speed: 0.8 },
        ];
        save_session(&path, &entries).unwrap();
        assert_eq!(load_session(&path).unwrap(), entries);
    }

    #[test]
    fn corrupt_json_is_a_parse_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.json");
        std::fs::write(&path, "not json").unwrap();
        let err = load_session(&path).unwrap_err();
        assert!(matches!(err, SessionError::Parse(_)));
    }

    #[test]
    fn save_creates_missing_parent_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("dir").join("session.json");
        save_session(&path, &[]).unwrap();
        assert!(path.exists());
    }
}
```

- [ ] **Step 2: Register the module in `src/lib.rs`**

Change:
```rust
pub mod environment;
pub mod format;
pub mod importer;
pub mod state_machine;
pub mod tray;
pub mod window;
```
to:
```rust
pub mod environment;
pub mod format;
pub mod importer;
pub mod session;
pub mod state_machine;
pub mod tray;
pub mod window;
```

- [ ] **Step 3: Run the new tests and confirm they fail on the `todo!()` stubs**

Run (PowerShell, not Bash — cargo is not on Bash's PATH): `& "$env:USERPROFILE\.cargo\bin\cargo.exe" test --lib session::`
Expected: compiles, then FAILs with `not yet implemented` panics from `todo!()` (in `missing_file_loads_as_empty` and `round_trips_session_entries`; `corrupt_json_is_a_parse_error` and `save_creates_missing_parent_directory` will also hit the `save_session`/`load_session` stubs).

- [ ] **Step 4: Implement `load_session` and `save_session`**

Replace:
```rust
pub fn load_session(_path: &Path) -> Result<Vec<SessionEntry>, SessionError> {
    todo!()
}

pub fn save_session(_path: &Path, _entries: &[SessionEntry]) -> Result<(), SessionError> {
    todo!()
}
```
with:
```rust
pub fn load_session(path: &Path) -> Result<Vec<SessionEntry>, SessionError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).map_err(SessionError::Read)?;
    serde_json::from_str(&text).map_err(SessionError::Parse)
}

pub fn save_session(path: &Path, entries: &[SessionEntry]) -> Result<(), SessionError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(SessionError::Write)?;
    }
    let text = serde_json::to_string_pretty(entries).expect("SessionEntry always serializes");
    std::fs::write(path, text).map_err(SessionError::Write)
}
```

- [ ] **Step 5: Run the tests again and confirm they pass**

Run: `& "$env:USERPROFILE\.cargo\bin\cargo.exe" test --lib session::`
Expected: all 4 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/session.rs src/lib.rs
git commit -m "Add shimeji::session module for session.json persistence"
```

---

### Task 2: Wire session save/restore into `App`

**Files:**
- Modify: `src/app.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes (from Task 1): `shimeji::session::{SessionEntry, SessionError, load_session, save_session}`.
- Produces: `App::new` signature grows a `session_path: std::path::PathBuf` parameter (second positional argument, after `library_root`); `MascotInstance` grows a `pub slug: String` field.

- [ ] **Step 1: Add `slug` to `MascotInstance` and capture it in `spawn`**

In `src/app.rs`, change:
```rust
pub struct MascotInstance {
    pub id: u32,
    pub bundle: MascotBundle,
```
to:
```rust
pub struct MascotInstance {
    pub id: u32,
    pub slug: String,
    pub bundle: MascotBundle,
```

Then change the `spawn` method's push (still in `src/app.rs`):
```rust
        self.mascots.push(MascotInstance {
            id,
            bundle,
```
to:
```rust
        self.mascots.push(MascotInstance {
            id,
            slug: slug.to_string(),
            bundle,
```

- [ ] **Step 2: Run the existing test suite to confirm this compiles clean so far**

Run: `& "$env:USERPROFILE\.cargo\bin\cargo.exe" build`
Expected: builds with no new errors (the `slug` field is unused as a read so far, which is fine — it's about to be consumed in Step 3 onward).

- [ ] **Step 3: Add `session_path` to `App`, and `save_session`/`restore_session` methods**

In `src/app.rs`, change the `App` struct:
```rust
pub struct App {
    pub library_root: std::path::PathBuf,
    pub catalog: Vec<CatalogEntry>,
    pub mascots: Vec<MascotInstance>,
    pub environment: EnvironmentTracker<Win32MonitorSource, Win32WindowSource>,
    pub tray: TrayIcon,
    pub owner: HWND,
    next_instance_id: u32,
    settings_tx: Option<std::sync::mpsc::Sender<crate::settings_ui::SettingsCommand>>,
}
```
to:
```rust
pub struct App {
    pub library_root: std::path::PathBuf,
    pub catalog: Vec<CatalogEntry>,
    pub mascots: Vec<MascotInstance>,
    pub environment: EnvironmentTracker<Win32MonitorSource, Win32WindowSource>,
    pub tray: TrayIcon,
    pub owner: HWND,
    session_path: std::path::PathBuf,
    next_instance_id: u32,
    settings_tx: Option<std::sync::mpsc::Sender<crate::settings_ui::SettingsCommand>>,
}
```

Change `App::new`:
```rust
    pub fn new(library_root: std::path::PathBuf, owner: HWND, tray: TrayIcon) -> Self {
        let catalog = shimeji::importer::catalog::load_catalog(&library_root).unwrap_or_default();
        let environment = EnvironmentTracker::new(
            Win32MonitorSource,
            Win32WindowSource { exclude: Vec::new() },
            Duration::from_millis(150),
        );
        tray.set_menu(shimeji::tray::build_menu(&catalog, &[]));
        App {
            library_root,
            catalog,
            mascots: Vec::new(),
            environment,
            tray,
            owner,
            next_instance_id: 1,
            settings_tx: None,
        }
    }
```
to:
```rust
    pub fn new(library_root: std::path::PathBuf, session_path: std::path::PathBuf, owner: HWND, tray: TrayIcon) -> Self {
        let catalog = shimeji::importer::catalog::load_catalog(&library_root).unwrap_or_default();
        let environment = EnvironmentTracker::new(
            Win32MonitorSource,
            Win32WindowSource { exclude: Vec::new() },
            Duration::from_millis(150),
        );
        tray.set_menu(shimeji::tray::build_menu(&catalog, &[]));
        let mut app = App {
            library_root,
            catalog,
            mascots: Vec::new(),
            environment,
            tray,
            owner,
            session_path,
            next_instance_id: 1,
            settings_tx: None,
        };
        app.restore_session();
        app
    }
```

Add two new private methods right after `refresh_tray_menu` (which currently reads):
```rust
    fn refresh_tray_menu(&mut self) {
        let live: Vec<(u32, String)> = self.mascots.iter().map(|m| (m.id, m.bundle.manifest.name.clone())).collect();
        self.tray.set_menu(shimeji::tray::build_menu(&self.catalog, &live));
    }
```
Insert immediately after it (before `pub fn settings_window_opened`):
```rust

    fn save_session(&self) {
        let entries: Vec<shimeji::session::SessionEntry> = self
            .mascots
            .iter()
            .map(|m| shimeji::session::SessionEntry { slug: m.slug.clone(), scale: m.scale, speed: m.speed })
            .collect();
        if let Err(err) = shimeji::session::save_session(&self.session_path, &entries) {
            crate::logging::log_error("session", &err.to_string());
        }
    }

    /// Restores last session's live mascots (by catalog slug) and their scale/speed, run once
    /// from `App::new`. Position is intentionally not restored -- every restored mascot spawns at
    /// the same default point a fresh manual spawn would. An entry whose slug no longer resolves
    /// to a loadable bundle (removed/renamed/corrupt since last run) is silently dropped; the
    /// re-save after the loop prunes it from session.json instead of retrying it forever.
    fn restore_session(&mut self) {
        let entries = match shimeji::session::load_session(&self.session_path) {
            Ok(entries) => entries,
            Err(err) => {
                crate::logging::log_error("session", &err.to_string());
                return;
            }
        };
        for entry in &entries {
            let before = self.mascots.len();
            let _ = self.spawn(&entry.slug);
            if self.mascots.len() > before {
                if let Some(mascot) = self.mascots.last_mut() {
                    mascot.scale = entry.scale;
                    mascot.speed = entry.speed;
                }
            }
        }
        self.save_session();
    }
```

- [ ] **Step 4: Call `save_session()` after every mutation to the live mascot list**

In `src/app.rs`, in `spawn`, change:
```rust
        });
        self.refresh_tray_menu();
        Ok(())
    }
```
to:
```rust
        });
        self.refresh_tray_menu();
        self.save_session();
        Ok(())
    }
```

Change `set_scale` and `set_speed`:
```rust
    pub fn set_scale(&mut self, instance_id: u32, pct: i32) {
        if let Some(mascot) = self.mascots.iter_mut().find(|m| m.id == instance_id) {
            mascot.scale = pct as f64 / 100.0;
        }
    }

    pub fn set_speed(&mut self, instance_id: u32, pct: i32) {
        if let Some(mascot) = self.mascots.iter_mut().find(|m| m.id == instance_id) {
            mascot.speed = pct as f64 / 100.0;
        }
    }
```
to:
```rust
    pub fn set_scale(&mut self, instance_id: u32, pct: i32) {
        if let Some(mascot) = self.mascots.iter_mut().find(|m| m.id == instance_id) {
            mascot.scale = pct as f64 / 100.0;
        }
        self.save_session();
    }

    pub fn set_speed(&mut self, instance_id: u32, pct: i32) {
        if let Some(mascot) = self.mascots.iter_mut().find(|m| m.id == instance_id) {
            mascot.speed = pct as f64 / 100.0;
        }
        self.save_session();
    }
```

Change `close` and `close_all`:
```rust
    pub fn close(&mut self, instance_id: u32) {
        if let Some(pos) = self.mascots.iter().position(|m| m.id == instance_id) {
            let mascot = self.mascots.remove(pos);
            self.environment.window_source.exclude.retain(|h| *h != mascot.window.hwnd);
            unsafe {
                let _ = DestroyWindow(mascot.window.hwnd);
            }
            self.refresh_tray_menu();
        }
    }

    pub fn close_all(&mut self) {
        self.environment.window_source.exclude.clear();
        for mascot in self.mascots.drain(..) {
            unsafe {
                let _ = DestroyWindow(mascot.window.hwnd);
            }
        }
        self.refresh_tray_menu();
    }
```
to:
```rust
    pub fn close(&mut self, instance_id: u32) {
        if let Some(pos) = self.mascots.iter().position(|m| m.id == instance_id) {
            let mascot = self.mascots.remove(pos);
            self.environment.window_source.exclude.retain(|h| *h != mascot.window.hwnd);
            unsafe {
                let _ = DestroyWindow(mascot.window.hwnd);
            }
            self.refresh_tray_menu();
            self.save_session();
        }
    }

    pub fn close_all(&mut self) {
        self.environment.window_source.exclude.clear();
        for mascot in self.mascots.drain(..) {
            unsafe {
                let _ = DestroyWindow(mascot.window.hwnd);
            }
        }
        self.refresh_tray_menu();
        self.save_session();
    }
```

- [ ] **Step 5: Update the `App::new` call site in `src/main.rs`**

Change:
```rust
    let app_root = dirs_next::data_dir().expect("APPDATA must be resolvable on Windows").join("ShimejiRust");
    let library_root = app_root.join("mascots");
    logging::init(app_root.join("animations.log"));
```
to:
```rust
    let app_root = dirs_next::data_dir().expect("APPDATA must be resolvable on Windows").join("ShimejiRust");
    let library_root = app_root.join("mascots");
    let session_path = app_root.join("session.json");
    logging::init(app_root.join("animations.log"));
```

Change:
```rust
        let tray = TrayIcon::create().expect("failed to create tray icon");
        let mut app = App::new(library_root, owner, tray);
```
to:
```rust
        let tray = TrayIcon::create().expect("failed to create tray icon");
        let mut app = App::new(library_root, session_path, owner, tray);
```

- [ ] **Step 6: Build and run the full test suite**

Run: `& "$env:USERPROFILE\.cargo\bin\cargo.exe" build`
Expected: builds with no errors.

Run: `& "$env:USERPROFILE\.cargo\bin\cargo.exe" test`
Expected: all tests pass (the pre-existing suite plus Task 1's 4 new `session::` tests — no test in this task's scope touches `App` directly, since `App` has no existing unit-test harness and standing one up for a Win32-backed struct is out of scope here per the design spec).

- [ ] **Step 7: Manual smoke test**

Do NOT simulate mouse/cursor input for this — build the release binary and have a human (or the requesting user) perform this check:

1. `& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release` (fails with an "Access is denied" file-lock error if an old build of `shimeji.exe` is still running — ask the user to close it via the tray icon's Exit item first if so).
2. Launch `target\release\shimeji.exe`, spawn a mascot from the tray menu, open Settings and change its scale to something other than 100%.
3. Confirm `%APPDATA%\ShimejiRust\session.json` now contains one entry with that mascot's slug and the new scale.
4. Exit the app via the tray's Exit item, relaunch it, and confirm the mascot reappears automatically at the same scale (position may differ — that's expected, per this task's scope).

- [ ] **Step 8: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "Restore live mascots and their scale/speed from session.json on launch"
```
