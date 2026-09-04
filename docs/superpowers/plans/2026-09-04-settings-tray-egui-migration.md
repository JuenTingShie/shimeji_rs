# Settings Window & Tray Icon Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-rolled raw-Win32 settings window with `egui`/`eframe` (glow backend) and the hand-rolled `src/tray/` (icon + menu) with the `tray-icon` crate (which re-exports `muda` for menus), while leaving mascot rendering, physics, and the animation state machine entirely on raw Win32.

**Architecture:** The main thread keeps its existing custom `PeekMessageW` loop untouched. `tray-icon` runs on that same main thread (it only needs an active Win32 message pump, which the loop already provides) and is polled via its event-receiver channels once per loop iteration. The settings window can't share that thread — `eframe::run_native()` blocks until its window closes — so `App::open_settings` spawns a dedicated OS thread per open that runs the settings UI for just that window's lifetime, communicating back to `App` via `PostMessageW` to the owner HWND, reusing the same message-passing pattern the raw settings window already used.

**Tech Stack:** `tray-icon` 0.24 (Windows: `Shell_NotifyIcon`-based, handles `TaskbarCreated` re-registration internally), `eframe` 0.36 with the `glow` renderer (not the default `wgpu`, to avoid pulling in a GPU-pipeline dependency tree for two sliders and a button), `raw-window-handle` 0.6 (to extract the settings window's `HWND` from `eframe::Frame`).

## Global Constraints

- Mascot rendering (`src/window/mascot_window.rs`), physics (`step_one_mascot` in `src/app.rs`), and the animation state machine are **out of scope** — do not modify them beyond the message-routing changes this plan explicitly calls for.
- Balloon/toast import-success/import-failure notifications are being **dropped**, not replaced — `tray-icon` has no notification API and cannot piggyback on our own `Shell_NotifyIcon` calls (it owns its own internal `HWND`/`uID`). This is a deliberate, disclosed behavior change, decided during brainstorming — do not attempt to add toast notifications as a replacement.
- `eframe` must be built with `default-features = false` and an explicit feature list that **excludes `wgpu`** and includes `glow` — this is the whole point of choosing glow over the crate's default renderer.
- No new automated tests are expected for the tray/menu/settings glue code in `main.rs`/`app.rs`/`settings_ui.rs` (binary crate, no existing test coverage there, per project convention). Verification for those pieces is `cargo build`/`cargo check` plus a manual test checklist. `filter_zip_paths`'s existing unit test moves with it and must still pass.
- The full existing lib-crate test suite (69 lib + 4 bundle_validation + 3 importer tests as of this plan) must still pass unchanged after every task — nothing in this plan touches lib-crate physics/animation/import code.
- The release binary may be locked by an already-running `shimeji.exe` during development; use `cargo check` (or `cargo build`, debug profile) to verify compilation when that happens, and only attempt `cargo build --release` once the running instance is closed.

---

### Task 1: Migrate the tray icon and its menu to `tray-icon` + `muda`

**Files:**
- Modify: `Cargo.toml`
- Delete: `src/tray/menu.rs`
- Modify (full rewrite): `src/tray/mod.rs`
- Modify: `src/app.rs` (drop balloon notifications, wire menu rebuild on catalog change)
- Modify: `src/main.rs` (replace `show_tray_menu`, `WM_TRAY_CALLBACK` handling, and `TaskbarCreated` handling with `tray-icon`/`muda` event draining)
- Test: `src/tray/mod.rs` (inline `#[cfg(test)]`, carries over `filter_zip_paths`'s existing test unchanged)

**Interfaces:**
- Consumes: `shimeji::importer::catalog::CatalogEntry { slug: String, name: String, dir: PathBuf }` (existing type, unchanged).
- Produces:
  - `pub struct TrayIcon` with `pub fn create() -> tray_icon::Result<TrayIcon>` and `pub fn set_menu(&self, menu: tray_icon::menu::Menu)` — used by `app.rs` and `main.rs`.
  - `pub fn build_menu(catalog: &[CatalogEntry]) -> tray_icon::menu::Menu` — used by `app.rs`.
  - `pub fn filter_zip_paths(paths: &[PathBuf]) -> Vec<PathBuf>` — relocated from the deleted `src/tray/menu.rs`, signature unchanged, used by `main.rs`'s `handle_drop`.

- [ ] **Step 1: Add the `tray-icon` dependency**

Edit `Cargo.toml`'s `[dependencies]` section — add this line (keep everything else unchanged):

```toml
tray-icon = "0.24"
```

- [ ] **Step 2: Run `cargo check` to confirm the dependency resolves**

Run: `cargo check`
Expected: succeeds (no source code uses the new crate yet, so this only confirms the dependency itself resolves and builds).

- [ ] **Step 3: Delete `src/tray/menu.rs` and rewrite `src/tray/mod.rs`**

Delete `src/tray/menu.rs` entirely (its `filter_zip_paths` moves into the new `src/tray/mod.rs` below; its `build_spawn_items`/`SPAWN_MENU_ID_BASE`/`IMPORT_MENU_ID`/`CLOSE_ALL_MENU_ID`/`EXIT_MENU_ID` are replaced by `build_menu` below using `muda`'s string-based `MenuId`s instead of numeric Win32 menu-command IDs).

Replace the full contents of `src/tray/mod.rs` with:

```rust
use crate::importer::catalog::CatalogEntry;
use std::path::PathBuf;
use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{Icon, TrayIconBuilder};

pub struct TrayIcon {
    inner: tray_icon::TrayIcon,
}

impl TrayIcon {
    pub fn create() -> tray_icon::Result<TrayIcon> {
        let inner = TrayIconBuilder::new()
            .with_icon(placeholder_icon())
            .with_tooltip("Shimeji")
            .with_menu_on_left_click(false)
            .build()?;
        Ok(TrayIcon { inner })
    }

    /// Replaces the tray icon's menu -- called once after the catalog first loads and again
    /// every time it changes (an import), rather than rebuilt on every right-click as the old
    /// raw Win32 code did, since tray-icon shows whatever menu is currently set automatically.
    pub fn set_menu(&self, menu: Menu) {
        self.inner.set_menu(Some(Box::new(menu)));
    }
}

/// A plain solid-color placeholder icon -- this was already a generic, unbranded OS icon
/// (`IDI_APPLICATION`) before this migration, not a real app icon, so a flat color square is not
/// a visual regression. Swap for a real embedded .ico/.png later if desired; out of scope here.
fn placeholder_icon() -> Icon {
    const SIZE: u32 = 32;
    let mut img = image::RgbaImage::new(SIZE, SIZE);
    for px in img.pixels_mut() {
        *px = image::Rgba([70, 130, 180, 255]);
    }
    Icon::from_rgba(img.into_raw(), SIZE, SIZE).expect("fixed 32x32 opaque buffer is always a valid icon")
}

pub fn build_menu(catalog: &[CatalogEntry]) -> Menu {
    let menu = Menu::new();
    for entry in catalog {
        let item = MenuItem::with_id(format!("spawn:{}", entry.slug), &entry.name, true, None);
        let _ = menu.append(&item);
    }
    let _ = menu.append(&MenuItem::with_id("import", "Import Mascot...", true, None));
    let _ = menu.append(&MenuItem::with_id("close_all", "Close All", true, None));
    let _ = menu.append(&MenuItem::with_id("exit", "Exit", true, None));
    menu
}

pub fn filter_zip_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| p.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("zip")))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_zip_paths_case_insensitively() {
        let paths = vec![
            PathBuf::from("C:/drop/usagi.zip"),
            PathBuf::from("C:/drop/readme.txt"),
            PathBuf::from("C:/drop/OTHER.ZIP"),
        ];
        let kept = filter_zip_paths(&paths);
        assert_eq!(kept, vec![PathBuf::from("C:/drop/usagi.zip"), PathBuf::from("C:/drop/OTHER.ZIP")]);
    }

    #[test]
    fn builds_a_menu_item_per_catalog_entry_plus_the_three_fixed_items() {
        let catalog = vec![
            CatalogEntry { slug: "usagi".into(), name: "usagi".into(), dir: PathBuf::from("usagi") },
            CatalogEntry { slug: "neko".into(), name: "neko".into(), dir: PathBuf::from("neko") },
        ];
        let menu = build_menu(&catalog);
        assert_eq!(menu.items().len(), 5); // 2 catalog entries + import + close_all + exit
    }
}
```

If `menu.items()` isn't the exact accessor `muda::Menu` exposes for inspecting appended items in the resolved 0.19.3 API, check `cargo doc --open -p muda` (or the compiler error from this test) for the correct accessor and use that instead — the assertion's intent (menu ends up with 5 items) is what matters, not this exact call.

- [ ] **Step 4: Run `cargo check` to confirm `src/tray/mod.rs` compiles, then run its test**

Run: `cargo check`
Expected: compiles (fix any exact-API mismatches against the installed `tray-icon`/`muda` 0.19.3 source now, per Step 3's note).

Run: `cargo test --lib tray::`
Expected: both tests in `src/tray/mod.rs` pass.

- [ ] **Step 5: Drop balloon notifications and wire menu rebuilds in `src/app.rs`**

In `src/app.rs`:

1. Remove the `self.tray.notify(...)` calls inside `import_from_bytes` (both the `Ok(entry)` and `Err(err)` branches) — leave the rest of each branch (`self.catalog = ...` on success) unchanged. Import still works identically; it's silent instead of showing a balloon.
2. After `self.catalog = shimeji::importer::catalog::load_catalog(&self.library_root).unwrap_or_default();` inside `import_from_bytes`, add:

```rust
self.tray.set_menu(shimeji::tray::build_menu(&self.catalog));
```

3. In `App::new`, after the existing `let catalog = shimeji::importer::catalog::load_catalog(&library_root).unwrap_or_default();` line and before constructing the final `App { ... }` struct literal, add:

```rust
tray.set_menu(shimeji::tray::build_menu(&catalog));
```

(`tray` is the `TrayIcon` parameter `App::new` already takes — this just seeds its menu once the catalog is known, since `TrayIcon::create()` no longer takes a catalog and starts with no menu at all.)

- [ ] **Step 6: Run `cargo check` on the lib+app changes**

Run: `cargo check`
Expected: fails at this point specifically in `main.rs` (not yet updated) — confirms the compiler is now flagging exactly the call sites Step 7 needs to fix, and nothing else.

- [ ] **Step 7: Rewire `src/main.rs`**

1. Update imports: replace

```rust
use shimeji::tray::menu::{build_spawn_items, filter_zip_paths, CLOSE_ALL_MENU_ID, EXIT_MENU_ID, IMPORT_MENU_ID};
use shimeji::tray::{TrayIcon, WM_TRAY_CALLBACK};
```

with:

```rust
use shimeji::tray::{filter_zip_paths, TrayIcon};
use tray_icon::menu::MenuEvent;
use tray_icon::TrayIconEvent;
```

2. Remove the `RegisterWindowMessageW` import (no longer used anywhere) and delete this block entirely (tray-icon re-registers itself with Explorer after a `TaskbarCreated` broadcast internally — this crate-level fix landed in tray-icon's own Windows backend, not just in the unrelated `tao` crate):

```rust
// Explorer broadcasts this registered message to every top-level window when the
// taskbar is (re)created, e.g. after Explorer crashes and restarts — see
// TrayIcon::readd's doc comment.
let wm_taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
```

3. Delete the match arm `m if m == wm_taskbar_created => app.tray.readd(),` (the whole `TrayIcon::readd` method and the `WM_TASKBARCREATED`-driven re-add it implemented are gone — confirm no other reference to `readd` remains anywhere in the codebase before finishing this task).

4. Delete the `WM_TRAY_CALLBACK => { ... show_tray_menu(owner, &mut app); ... }` match arm, and delete the whole `show_tray_menu` function at the bottom of the file.

5. Change the tray creation line from:

```rust
let tray = TrayIcon::create(owner)?;
```

to:

```rust
let tray = TrayIcon::create().expect("failed to create tray icon");
```

(`tray_icon::Result`'s error type doesn't convert via `?` into this function's `windows::core::Result<()>` — matches this file's existing style of `.expect(...)` for other infallible-in-practice startup calls, e.g. `dirs_next::data_dir().expect(...)`.)

6. Immediately after the main `loop { if PeekMessageW(...) { match msg.message { ... } } else { ... } }` block's closing brace, but still inside the `loop { ... }`, add:

```rust
while let Ok(_event) = TrayIconEvent::receiver().try_recv() {
    // Not acted on individually (with_menu_on_left_click(false) plus a menu already being set
    // makes tray-icon show it automatically on right-click) -- still drained every iteration
    // because the channel is otherwise unbounded and Move fires continuously while hovering.
}
while let Ok(event) = MenuEvent::receiver().try_recv() {
    let id = event.id.0.as_str();
    if let Some(slug) = id.strip_prefix("spawn:") {
        let _ = app.spawn(slug);
    } else if id == "import" {
        if let Some(path) = rfd::FileDialog::new().add_filter("Mascot bundle", &["zip"]).pick_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                app.import_from_bytes(&bytes);
            }
        }
    } else if id == "close_all" {
        app.close_all();
    } else if id == "exit" {
        app.close_all();
        PostQuitMessage(0);
    }
}
```

7. `CatalogEntry` is very likely now unused in `main.rs` (it was only needed for `show_tray_menu`'s `labels: Vec<(u16, CatalogEntry)>` bookkeeping, which no longer exists) — if `cargo check` in the next step flags it as an unused import, remove it.

- [ ] **Step 8: Build, run the full test suite, and fix any remaining compile errors**

Run: `cargo check`
Expected: succeeds. Fix any remaining mismatches against the actual installed `tray-icon`/`muda` 0.19.3 API (e.g. exact `TrayIconBuilder`/`MenuItem` method names) using the compiler's error output and `cargo doc --open -p tray-icon -p muda` as ground truth over this plan's text if they've drifted.

Run: `cargo test`
Expected: `69 passed` (lib) + `4 passed` (bundle_validation) + `3 passed` (importer) — same totals as before this task, plus the 2 new tests in `src/tray/mod.rs` (so lib should now read `71 passed`).

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml src/tray/mod.rs src/app.rs src/main.rs
git rm src/tray/menu.rs
git commit -m "feat: migrate tray icon and menu to tray-icon + muda

Replaces src/tray/'s hand-rolled Shell_NotifyIcon/AppendMenuW/TrackPopupMenu
code with the tray-icon crate (which re-exports muda for the menu). Drops
balloon notifications entirely -- tray-icon has no notification API and
owns its own internal HWND, so there's no way to piggyback a Shell_NotifyIcon
balloon call onto it without showing a second, unwanted tray icon; this was
a deliberate, disclosed decision made during brainstorming, not an oversight.

TaskbarCreated re-registration (fixed by hand earlier this session) is now
handled internally by tray-icon itself, so that machinery is removed from
main.rs along with it. Menu items use tray-icon/muda's string-based MenuIds
(\"spawn:<slug>\", \"import\", \"close_all\", \"exit\") instead of the old
numeric Win32 menu-command ID scheme, and the menu is rebuilt once when the
catalog actually changes rather than freshly on every right-click, since
tray-icon shows whatever menu is currently set rather than requiring one
to be built just-in-time."
```

---

### Task 2: Migrate the settings window to `egui`/`eframe`

**Files:**
- Modify: `Cargo.toml`
- Delete: `src/window/settings_window.rs`
- Modify: `src/window/mod.rs`
- Create: `src/settings_ui.rs` (binary-crate module, alongside `src/app.rs`/`src/logging.rs`)
- Modify: `src/app.rs` (settings-open guard, exclude-list wiring, message handling)
- Modify: `src/main.rs` (`mod settings_ui;`, message routing)

**Interfaces:**
- Consumes: `WM_MASCOT_CLOSE` (from `shimeji::window::mascot_window`, unchanged, `= WM_APP + 13`); `App::set_scale(&mut self, instance_id: u32, pct: i32)` and `App::set_speed(&mut self, instance_id: u32, pct: i32)` (existing, unchanged, from Task 4 of the earlier settings-window session work).
- Produces:
  - `pub fn open_settings_window(owner: HWND, instance_id: u32, scale_pct: i32, speed_pct: i32)` in `src/settings_ui.rs` — spawns the settings thread; used by `App::open_settings`.
  - `pub const WM_MASCOT_SET_SCALE: u32`, `WM_MASCOT_SET_SPEED: u32`, `WM_SETTINGS_OPENED: u32`, `WM_SETTINGS_CLOSED: u32` in `src/settings_ui.rs` (replacing the ones previously defined in the deleted `src/window/settings_window.rs`) — used by `main.rs`'s message loop.
  - `App::settings_window_opened(&mut self, instance_id: u32, hwnd_raw: isize)` and a revised `App::settings_window_closed(&mut self, instance_id: u32)` (payload changes from raw hwnd to instance_id) — used by `main.rs`.

- [ ] **Step 1: Add the `eframe` and `raw-window-handle` dependencies**

Edit `Cargo.toml`'s `[dependencies]` section — add these two lines:

```toml
eframe = { version = "0.36", default-features = false, features = ["glow", "default_fonts", "winit", "accesskit"] }
raw-window-handle = "0.6"
```

`default-features = false` plus this explicit feature list is what excludes `wgpu` (eframe's default renderer, a much heavier GPU-pipeline dependency than this window needs) while still pulling in native windowing (`winit`), text rendering (`default_fonts`), and accessibility (`accesskit`) support.

- [ ] **Step 2: Run `cargo check` to confirm the dependencies resolve**

Run: `cargo check`
Expected: succeeds (no source code uses either crate yet).

- [ ] **Step 3: Delete the raw settings window**

Delete `src/window/settings_window.rs`.

In `src/window/mod.rs`, remove the line `pub mod settings_window;`, leaving:

```rust
pub mod alpha;
pub mod hit_test;
pub mod input;
pub mod mascot_window;
```

- [ ] **Step 4: Run `cargo check` to see what breaks**

Run: `cargo check`
Expected: fails in `src/app.rs` and `src/main.rs`, which still reference the deleted module — confirms exactly the call sites Steps 5-7 need to fix.

- [ ] **Step 5: Create `src/settings_ui.rs`**

```rust
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use shimeji::window::mascot_window::WM_MASCOT_CLOSE;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_MASCOT_SET_SCALE: u32 = WM_APP + 16;
pub const WM_MASCOT_SET_SPEED: u32 = WM_APP + 17;
pub const WM_SETTINGS_OPENED: u32 = WM_APP + 18;
pub const WM_SETTINGS_CLOSED: u32 = WM_APP + 19;

/// Spawns a dedicated thread that runs the settings UI for exactly one mascot's lifetime.
/// eframe::run_native() blocks its calling thread until the window closes, which is incompatible
/// with sharing the main mascot loop's thread -- so this runs on its own thread instead and talks
/// back to `App` purely via `PostMessageW` to the owner HWND, which is documented safe to call
/// cross-thread and needs no shared/locked state with the main thread.
pub fn open_settings_window(owner: HWND, instance_id: u32, scale_pct: i32, speed_pct: i32) {
    let owner_addr = owner.0 as isize;
    std::thread::spawn(move || {
        let owner = HWND(owner_addr as *mut _);
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([260.0, 190.0])
                .with_resizable(false),
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        };
        let app = SettingsApp { owner, instance_id, scale_pct, speed_pct, hwnd_reported: false };
        let _ = eframe::run_native("Mascot Settings", options, Box::new(move |_cc| Ok(Box::new(app))));
        // run_native blocks until the window closes, however it closes -- including an internal
        // startup failure. Post this unconditionally after it returns so App's duplicate-open
        // guard (App::open_settings_instances) can never wedge on a thread that silently died.
        unsafe {
            let _ = PostMessageW(Some(owner), WM_SETTINGS_CLOSED, WPARAM(instance_id as usize), LPARAM(0));
        }
    });
}

struct SettingsApp {
    owner: HWND,
    instance_id: u32,
    scale_pct: i32,
    speed_pct: i32,
    hwnd_reported: bool,
}

impl SettingsApp {
    fn post(&self, msg: u32, value: i32) {
        unsafe {
            let _ = PostMessageW(Some(self.owner), msg, WPARAM(self.instance_id as usize), LPARAM(value as isize));
        }
    }

    fn post_raw(&self, msg: u32, value: isize) {
        unsafe {
            let _ = PostMessageW(Some(self.owner), msg, WPARAM(self.instance_id as usize), LPARAM(value));
        }
    }
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if !self.hwnd_reported {
            self.hwnd_reported = true;
            if let Ok(handle) = frame.window_handle() {
                if let RawWindowHandle::Win32(h) = handle.as_raw() {
                    self.post_raw(WM_SETTINGS_OPENED, h.hwnd.get());
                }
            }
        }

        ui.heading("Mascot Settings");
        if ui.add(egui::Slider::new(&mut self.scale_pct, 50..=200).text("Scale %")).changed() {
            self.post(WM_MASCOT_SET_SCALE, self.scale_pct);
        }
        if ui.add(egui::Slider::new(&mut self.speed_pct, 25..=300).text("Speed %")).changed() {
            self.post(WM_MASCOT_SET_SPEED, self.speed_pct);
        }
        ui.separator();
        if ui.button("Remove Mascot").clicked() {
            self.post(WM_MASCOT_CLOSE, 0);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
```

If any exact method/builder name here (`with_resizable`, `ViewportCommand::Close`, `send_viewport_cmd`, `window_handle`) doesn't match the installed `egui`/`eframe` 0.36.1, resolve it from the compiler error plus `cargo doc --open -p eframe -p egui -p raw-window-handle` — the architecture (thread-per-open, `Frame::window_handle()` for the HWND, `PostMessageW` for all cross-thread communication) is what this task is actually validating, not any one exact call name.

- [ ] **Step 6: Run `cargo check` on `settings_ui.rs` in isolation**

Run: `cargo check`
Expected: fails only in `main.rs` (`mod settings_ui;` not yet declared) — confirms `settings_ui.rs` itself is syntactically self-contained before wiring it in.

- [ ] **Step 7: Wire `settings_ui` into `src/main.rs` and `src/app.rs`**

In `src/main.rs`:

1. Add near the other `mod` declarations at the top:

```rust
mod settings_ui;
```

2. Replace:

```rust
use shimeji::window::mascot_window::{WM_MASCOT_CLOSE, WM_MASCOT_DRAG_START, WM_MASCOT_FLING, WM_MASCOT_JUMP, WM_MASCOT_OPEN_SETTINGS, WM_MASCOT_TAP};
use shimeji::window::settings_window::{WM_MASCOT_SET_SCALE, WM_MASCOT_SET_SPEED, WM_SETTINGS_CLOSED};
```

with:

```rust
use shimeji::window::mascot_window::{WM_MASCOT_CLOSE, WM_MASCOT_DRAG_START, WM_MASCOT_FLING, WM_MASCOT_JUMP, WM_MASCOT_OPEN_SETTINGS, WM_MASCOT_TAP};
use settings_ui::{WM_MASCOT_SET_SCALE, WM_MASCOT_SET_SPEED, WM_SETTINGS_CLOSED, WM_SETTINGS_OPENED};
```

3. Update the message-loop match arms from:

```rust
WM_MASCOT_OPEN_SETTINGS => app.open_settings(msg.wParam.0 as u32),
WM_MASCOT_SET_SCALE => app.set_scale(msg.wParam.0 as u32, msg.lParam.0 as i32),
WM_MASCOT_SET_SPEED => app.set_speed(msg.wParam.0 as u32, msg.lParam.0 as i32),
WM_SETTINGS_CLOSED => app.settings_window_closed(msg.wParam.0),
```

to:

```rust
WM_MASCOT_OPEN_SETTINGS => app.open_settings(msg.wParam.0 as u32),
WM_MASCOT_SET_SCALE => app.set_scale(msg.wParam.0 as u32, msg.lParam.0 as i32),
WM_MASCOT_SET_SPEED => app.set_speed(msg.wParam.0 as u32, msg.lParam.0 as i32),
WM_SETTINGS_OPENED => app.settings_window_opened(msg.wParam.0 as u32, msg.lParam.0),
WM_SETTINGS_CLOSED => app.settings_window_closed(msg.wParam.0 as u32),
```

In `src/app.rs`:

1. Remove `use shimeji::window::settings_window::SettingsWindow;` (or wherever it's imported — it may already be referenced only via a fully-qualified path inside `open_settings`; either way, no more `SettingsWindow` references should remain).

2. Add `use std::collections::HashSet;` alongside the existing `use std::time::{Duration, Instant};` (keep both).

3. On `App`, replace the single-purpose exclude-push in `open_settings` with two tracking fields. Add to the `App` struct definition:

```rust
    open_settings_instances: HashSet<u32>,
    open_settings_hwnds: std::collections::HashMap<u32, HWND>,
```

and initialize both as empty in `App::new`'s constructed struct literal (`open_settings_instances: HashSet::new(), open_settings_hwnds: std::collections::HashMap::new(),`).

4. Replace the whole `open_settings` method with:

```rust
    pub fn open_settings(&mut self, instance_id: u32) {
        if self.open_settings_instances.contains(&instance_id) {
            return;
        }
        let Some(mascot) = self.mascots.iter().find(|m| m.id == instance_id) else { return };
        let scale_pct = (mascot.scale * 100.0).round() as i32;
        let speed_pct = (mascot.speed * 100.0).round() as i32;
        self.open_settings_instances.insert(instance_id);
        crate::settings_ui::open_settings_window(self.owner, instance_id, scale_pct, speed_pct);
    }

    pub fn settings_window_opened(&mut self, instance_id: u32, hwnd_raw: isize) {
        let hwnd = HWND(hwnd_raw as *mut _);
        self.open_settings_hwnds.insert(instance_id, hwnd);
        // A settings window is itself a real, visible, titled top-level window -- excluded from
        // the desktop window list for the same reason mascot windows are (see spawn's comment).
        self.environment.window_source.exclude.push(hwnd);
    }
```

5. Replace the existing `settings_window_closed` method (which took a raw hwnd) with:

```rust
    pub fn settings_window_closed(&mut self, instance_id: u32) {
        self.open_settings_instances.remove(&instance_id);
        if let Some(hwnd) = self.open_settings_hwnds.remove(&instance_id) {
            self.environment.window_source.exclude.retain(|h| *h != hwnd);
        }
    }
```

- [ ] **Step 8: Build, run the full test suite, and fix any remaining compile errors**

Run: `cargo check`
Expected: succeeds. Fix any remaining API-name mismatches the same way as Task 1's Step 8.

Run: `cargo test`
Expected: same totals as after Task 1 (`71 passed` lib + `4` + `3`) — nothing in this task touches lib-crate code.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml src/window/mod.rs src/settings_ui.rs src/app.rs src/main.rs
git rm src/window/settings_window.rs
git commit -m "feat: migrate settings window to egui/eframe (glow backend)

Replaces src/window/settings_window.rs's raw Win32 scrollbars/GWLP_USERDATA
state/manual WM_HSCROLL plumbing with a small eframe app (glow renderer, not
the default wgpu, to avoid a GPU-pipeline dependency tree for two sliders
and a button).

eframe::run_native() blocks its calling thread until the window closes, so
it can't share the main mascot loop's thread -- App::open_settings now
spawns a dedicated thread per open instead, which talks back to App purely
via PostMessageW to the owner HWND (documented safe cross-thread), reusing
the same message-passing contract the raw version already used. Duplicate
opens for the same mascot are now guarded via open_settings_instances, and
the settings window's own HWND (extracted via raw-window-handle from
eframe::Frame on its first rendered frame) is tracked separately in
open_settings_hwnds so it can be excluded from -- and later removed from --
the desktop window list mascots use for floor/ceiling detection, matching
the mascot-window exclude fix from earlier this session."
```

---

### Task 3: Final verification and cleanup

**Files:**
- Read-only survey: `src/`, `Cargo.toml`, `README.md`
- Possible small edits: anywhere a leftover reference to deleted code is found

**Interfaces:**
- Consumes: everything produced by Tasks 1-2.
- Produces: nothing new — this task is a regression pass, not a feature.

- [ ] **Step 1: Grep for dead references to deleted code**

Run each of these and confirm zero matches (besides this plan file and the design spec, which are expected to mention them historically):

```bash
grep -rn "readd\|WM_TRAY_CALLBACK\|show_tray_menu\|SettingsWindow\b" src/
grep -rn "SPAWN_MENU_ID_BASE\|IMPORT_MENU_ID\|CLOSE_ALL_MENU_ID\|EXIT_MENU_ID" src/
```

Expected: no matches in `src/`. If any turn up, remove them.

- [ ] **Step 2: Full build and test pass**

Run: `cargo check`
Expected: succeeds with no warnings about unused imports/dead code beyond the project's existing pre-migration `unsafe_op_in_unsafe_fn` style warnings.

Run: `cargo test`
Expected: `71 passed` (lib) + `4 passed` (bundle_validation) + `3 passed` (importer), `0 failed` across all.

If `shimeji.exe` isn't currently running (check with `tasklist /FI "IMAGENAME eq shimeji.exe"` or the PowerShell equivalent), also run:

Run: `cargo build --release`
Expected: succeeds, producing `target/release/shimeji.exe`.

- [ ] **Step 3: Manual test checklist (report results, don't just assume)**

This plan's changes are Win32/UI glue code with no automated coverage, consistent with the rest of this project's binary crate — manual verification is the real test here. Using the freshly built `shimeji.exe`:

1. Launch it. Confirm the tray icon appears (a flat steel-blue square — expected, see Task 1 Step 3's comment on `placeholder_icon`) and that Explorer doesn't show a second, stray icon anywhere.
2. Right-click the tray icon. Confirm the menu shows: any spawnable mascots by name, then "Import Mascot...", "Close All", "Exit" — same items as before, just no numeric-ID artifacts.
3. Left-click the tray icon. Confirm nothing happens (matches pre-migration behavior — `with_menu_on_left_click(false)`).
4. Spawn a mascot from the menu. Confirm it appears and behaves normally (walk/climb/fall/drag), unaffected by this plan.
5. Import a `.zip` via the menu's "Import Mascot...". Confirm the newly imported mascot appears in the tray menu on the next right-click, and that there's no balloon/toast (expected — dropped deliberately).
6. Right-click a spawned mascot and choose "Settings...". Confirm a small window titled "Mascot Settings" opens with two sliders and a "Remove Mascot" button.
7. Drag the Scale slider. Confirm the mascot visibly resizes in real time.
8. Drag the Speed slider. Confirm the mascot's movement/animation visibly speeds up or slows down.
9. Click "Remove Mascot". Confirm both the mascot and the settings window close together.
10. Open "Settings..." on a second mascot, then click it again before closing the first one's window (if still open) — confirm a second click on the same mascot's "Settings..." while one is already open does nothing (the duplicate-open guard), and confirm two *different* mascots can each have their own settings window open at once.
11. With a settings window open, drag a different mascot underneath where the settings window sits on screen — confirm the mascot does **not** treat the settings window as floor/ceiling geometry (walks/falls through that screen region normally, not landing on top of the settings window).
12. Close the settings window via its titlebar X (not the Remove button) — confirm the mascot is unaffected (still there, sliders' last values already applied) and that reopening "Settings..." on it afterward works normally (guard was released).

- [ ] **Step 4: Commit any Step 1 cleanup, or confirm nothing to commit**

If Step 1 found and removed any dead references:

```bash
git add -A -- src
git commit -m "chore: remove dead references left over from tray/settings migration"
```

If Step 1 found nothing, no commit is needed for this task.
