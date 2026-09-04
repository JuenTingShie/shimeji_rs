# Settings Window & Tray Icon Migration Design

## Goal

Replace the hand-rolled raw-Win32 UI for two subsystems — the per-mascot settings window and the system tray — with well-tested Rust crates, while leaving mascot rendering, physics, and the animation state machine entirely untouched on raw Win32:

- **Settings window** → `egui` (via `eframe`, `glow` rendering backend)
- **Tray icon + its menu** → `tray-icon` (+ its companion `muda` crate for the menu)

## Motivation

- This session found and fixed five real Win32 bugs in the hand-rolled tray/menu code: `SendMessage`-vs-`PostMessage` delivery bridging, missing DPI awareness, a missing trailing `WM_NULL` after `TrackPopupMenu`, an `HMENU` leak that eventually crashed the app, and missing `TaskbarCreated` re-registration after Explorer restarts. `tray-icon`/`muda` are mature, widely deployed (Tauri uses them) and have almost certainly already solved all of these.
- The raw settings window (native scrollbars, `GWLP_USERDATA` state, manual `WM_HSCROLL` plumbing) took real, error-prone effort to hand-build for two sliders and a button. `egui`'s immediate-mode model is dramatically faster to build and modify UI in.
- Mascot rendering has an exact, proven, zero-extra-dependency solution today (`WM_NCHITTEST` + `UpdateLayeredWindow`) that neither `egui` nor `iced` replicate natively for per-pixel click-through — full reasoning and comparison table already covered in conversation, not repeated here. It stays on raw Win32.

## Architecture

The main thread keeps its existing custom `PeekMessageW` loop untouched — mascot ticking (60Hz timer) and all `WM_MASCOT_*` message routing are unaffected. Two things change what runs on/around it:

- **Tray icon**: `tray-icon` + `muda` replace `src/tray/` entirely, but stay on the **same main thread**. The crate needs an active Win32 message pump on its thread — which the main loop already provides — so no new thread is needed here. Each loop iteration, after the existing `PeekMessageW` poll, the loop additionally drains `TrayIconEvent::receiver()` and `MenuEvent::receiver()` (non-blocking `try_iter()`), replacing `WM_TRAY_CALLBACK` handling and `show_tray_menu()`.
- **Settings window**: `eframe::run_native()` blocks whatever thread calls it until its window closes, which is incompatible with sharing the main loop's thread. `App::open_settings` instead spawns a **new OS thread per open** that runs `eframe::run_native()` (glow backend) for just that window's lifetime, exiting when the window closes — matching how the current settings window is already opened and destroyed on demand.

The settings thread talks back to `App` the same way the current raw settings window already does: by `PostMessageW`-ing the owner HWND. `PostMessageW` is documented safe to call cross-thread, so the egui thread needs no shared/locked state with the main thread — just the owner HWND and the instance_id it was opened for, captured at spawn time.

## Components

### Removed

- `src/tray/mod.rs`, `src/tray/menu.rs`
- `src/window/settings_window.rs`

### Changed

- `src/tray/mod.rs` is rewritten in place (module path kept) as a thin wrapper around `tray-icon`. Its public surface (`TrayIcon::create`, `TrayIcon::notify`) is preserved as far as practical so `main.rs`/`app.rs` call sites need minimal churn. `TrayIcon::readd()` and the `WM_TASKBARCREATED` handling in `main.rs` are deleted if `tray-icon` already re-registers automatically after Explorer restarts (to confirm during implementation — see Open Risks); otherwise kept as-is.
- `main.rs`'s tray-menu construction (`show_tray_menu`, the dynamic "spawn X" list, Import/Close All/Exit) moves to building a `muda::Menu` once per tray-menu-open, using the same catalog data `App` already exposes.

### Added

- `src/settings_ui.rs` (new binary-crate module): the `eframe::App` implementation (two sliders — Scale 50–200%, Speed 25–300% — and a Remove Mascot button) and a `open_settings_window(owner: HWND, instance_id: u32, scale_pct: i32, speed_pct: i32)` function that spawns the dedicated thread.

### Message contract

All existing `WM_APP`-based constants keep their current shape; two are added/repurposed:

- `WM_MASCOT_OPEN_SETTINGS` (existing, unchanged trigger: mascot's own right-click "Settings..." menu item) — `App::open_settings(instance_id)` now spawns the settings thread instead of creating a window synchronously in-thread. Guarded (see below).
- `WM_MASCOT_SET_SCALE` / `WM_MASCOT_SET_SPEED` (existing, unchanged shape: wParam = instance_id, lParam = percentage) — posted from the settings thread on slider change.
- `WM_MASCOT_CLOSE` (existing, unchanged) — still posted by the Remove button.
- `WM_SETTINGS_OPENED` (**new**) — posted once from the settings thread after its native window exists, carrying instance_id (wParam) and the window's raw `HWND` value (lParam, as `isize`), so `App` can push it onto `environment.window_source.exclude` — same reasoning as the mascot-window exclude fix earlier this session (a visible, titled top-level window must not be treated as floor/ceiling geometry for mascots).
- `WM_SETTINGS_CLOSED` (existing, **repurposed payload**) — posted from the settings thread when its window closes. Now carries **instance_id** (not the raw hwnd, as it did with the raw Win32 version) so `App` can both remove the recorded hwnd from the exclude list and clear the duplicate-open guard below in one lookup.

### Duplicate-open guard

`App` gains `open_settings_instances: HashSet<u32>`. `open_settings(id)` is a no-op if `id` is already in the set — this prevents opening a second settings thread/window for the same mascot while one is already active. Inserted on `WM_MASCOT_OPEN_SETTINGS`, removed on `WM_SETTINGS_CLOSED`.

### Extracting the HWND from eframe

`eframe`'s `CreationContext` exposes the native window via the `raw-window-handle` crate; extracting a Win32 `HWND` from a `RawWindowHandle::Win32` variant is a supported, documented pattern. The exact call is pinned to the `egui`/`eframe` version chosen during implementation.

## Error handling / edge cases

- **Balloon notifications**: `TrayIcon::notify()` today shows a Windows balloon via `Shell_NotifyIcon(NIM_MODIFY, NIF_INFO, ...)` for import success/failure. Whether `tray-icon` exposes an equivalent is unconfirmed (see Open Risks). Fallback: keep that one raw `Shell_NotifyIcon` call scoped to `notify()` only; everything else still moves to the crate.
- **eframe startup failure** (e.g., no usable GL context on the settings thread): must not crash the main mascot process. A panic on a spawned (non-main) Rust thread does not tear down other threads, but the settings thread should not silently vanish without cleanup — it must still ensure `WM_SETTINGS_CLOSED` reaches the owner (e.g. via a scope guard/`Drop` that posts it even on early return) so the duplicate-open guard doesn't permanently wedge that instance_id.
- **Mascot closed while its settings window is open** (e.g., via "Close All" or the mascot's own Close menu item): out of scope for this change. The settings window stays open; its slider/Remove messages become no-ops since `App::set_scale`/`set_speed`/`close` already do a `find` and no-op on a missing instance_id. This is an accepted limitation, not a silent gap.
- **Process exit with a settings thread still running**: not joined explicitly. When `main()` returns, the process exits and the OS reclaims all threads regardless — standard behavior for any detached thread, not a leak.

## Testing

- The existing lib-crate test suite (state machine, surface/physics, importer, bundle validation — 69+4+3 tests as of this session) is entirely unaffected; nothing in this change touches lib-crate physics or animation code.
- Cross-thread `egui` UI behavior and tray-icon click routing are not meaningfully unit-testable; verification is manual (build + user testing), consistent with how prior UI-behavior fixes this session were verified.
- The new message-contract pieces (`WM_SETTINGS_OPENED`, the guard set, `WM_SETTINGS_CLOSED`'s repurposed payload) are glue code in the binary crate, which has no unit tests today (matching existing project convention — `app.rs`/`main.rs` are covered by manual testing, not `cargo test`); no new automated tests are planned for them.

## Out of scope

- No changes to mascot windows, physics, the animation state machine, the importer, or bundle formats.
- No redesign of the settings UI's look or new menu items — this change preserves current behavior on a new stack, not a feature change.

## Open risks (to resolve during implementation, not blocking this design)

1. Whether `tray-icon` exposes balloon/toast notifications is unconfirmed from documentation alone.
2. Whether `tray-icon` already handles `TaskbarCreated` re-registration internally is unconfirmed — if so, `TrayIcon::readd()` and its `main.rs` call site can be deleted entirely, simplifying `main.rs` further.
3. The exact `raw-window-handle` → `HWND` extraction call depends on the pinned `egui`/`eframe` version at implementation time.
