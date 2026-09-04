#![windows_subsystem = "windows"]

mod app;
mod logging;
mod settings_ui;

use app::App;
use shimeji::tray::{filter_zip_paths, TrayIcon};
use shimeji::window::mascot_window::{WM_MASCOT_CLOSE, WM_MASCOT_DRAG_START, WM_MASCOT_FLING, WM_MASCOT_JUMP, WM_MASCOT_OPEN_SETTINGS, WM_MASCOT_TAP};
use settings_ui::{WM_MASCOT_SET_SCALE, WM_MASCOT_SET_SPEED, WM_SETTINGS_CLOSED, WM_SETTINGS_OPENED};
use std::time::{Duration, Instant};
use tray_icon::menu::MenuEvent;
use tray_icon::TrayIconEvent;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW,
    PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassW, SetTimer,
    TranslateMessage, MSG, PM_REMOVE,
    WM_DESTROY, WM_DROPFILES, WM_QUIT, WM_TIMER, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::core::w;

const TICK_TIMER_ID: usize = 1;
const TICK_INTERVAL_MS: u32 = 1000 / 60;

// Explorer delivers WM_DROPFILES via SendMessage rather than PostMessage. A SendMessage from
// another process is dispatched straight into this WndProc as a side effect of the receiving
// thread pumping messages (PeekMessageW/GetMessageW) — it never becomes a MSG the main loop's
// PeekMessageW call returns, so the loop's `match msg.message` never sees it. Re-post it so the
// existing queue-based handling in main()'s loop still runs it exactly once.
unsafe extern "system" fn owner_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_DROPFILES {
        let _ = unsafe { PostMessageW(Some(hwnd), msg, wparam, lparam) };
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn main() -> windows::core::Result<()> {
    let app_root = dirs_next::data_dir().expect("APPDATA must be resolvable on Windows").join("ShimejiRust");
    let library_root = app_root.join("mascots");
    logging::init(app_root.join("animations.log"));

    unsafe {
        // Without an app manifest, Win32 processes default to DPI-unaware, which makes Windows
        // silently virtualize (scale) screen coordinates for this process. On a mixed-DPI or
        // mismatched-orientation multi-monitor setup, that virtualization is a well-known cause
        // of exactly this class of bug: cursor position (GetCursorPos, used to place the tray
        // context menu) and the OS's actual click hit-testing end up in different coordinate
        // spaces, so a popup menu can be drawn right under the cursor while every click on it
        // is silently swallowed. Declaring real per-monitor DPI awareness turns that
        // virtualization off. Must happen before any window is created.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("ShimejiOwnerWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(owner_window_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
        let owner = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name,
            w!("Shimeji"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance.into()),
            None,
        )?;
        DragAcceptFiles(owner, true);

        let tray = TrayIcon::create().expect("failed to create tray icon");
        let mut app = App::new(library_root, owner, tray);
        SetTimer(Some(owner), TICK_TIMER_ID, TICK_INTERVAL_MS, None);

        let mut msg = MSG::default();
        let mut last_tick = Instant::now();
        loop {
            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                match msg.message {
                    // WM_QUIT is what PostQuitMessage (called by "Exit") actually enqueues; a
                    // PeekMessageW loop must check for it explicitly to terminate — unlike
                    // GetMessageW, PeekMessageW never returns false to signal it. WM_DESTROY is
                    // kept as a defensive fallback in case the owner window is ever destroyed
                    // directly, though nothing does that in normal operation.
                    WM_QUIT => break,
                    WM_DESTROY => {
                        PostQuitMessage(0);
                        break;
                    }
                    WM_TIMER => app.tick(Instant::now()),
                    WM_MASCOT_TAP => app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::Tap, None),
                    WM_MASCOT_DRAG_START => app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::DragStart, None),
                    WM_MASCOT_FLING => {
                        let packed = msg.lParam.0 as u32;
                        let vx = (packed & 0xFFFF) as i16 as f64;
                        let vy = ((packed >> 16) & 0xFFFF) as i16 as f64;
                        app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::FlingStart, Some((vx, vy)));
                    }
                    WM_MASCOT_JUMP => app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::Jump, None),
                    WM_MASCOT_CLOSE => app.close(msg.wParam.0 as u32),
                    WM_MASCOT_OPEN_SETTINGS => app.open_settings(msg.wParam.0 as u32),
                    WM_MASCOT_SET_SCALE => app.set_scale(msg.wParam.0 as u32, msg.lParam.0 as i32),
                    WM_MASCOT_SET_SPEED => app.set_speed(msg.wParam.0 as u32, msg.lParam.0 as i32),
                    WM_SETTINGS_OPENED => app.settings_window_opened(msg.wParam.0 as u32, msg.lParam.0),
                    WM_SETTINGS_CLOSED => app.settings_window_closed(msg.wParam.0 as u32),
                    WM_DROPFILES => handle_drop(&mut app, HDROP(msg.wParam.0 as *mut _)),
                    _ => {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            } else if last_tick.elapsed() >= Duration::from_millis(TICK_INTERVAL_MS as u64) {
                app.tick(Instant::now());
                last_tick = Instant::now();
            }

            while let Ok(_event) = TrayIconEvent::receiver().try_recv() {
                // Not acted on individually (with_menu_on_left_click(false) plus a menu already
                // being set makes tray-icon show it automatically on right-click) -- still drained
                // every iteration because the channel is otherwise unbounded and Move fires
                // continuously while hovering.
            }
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                let id = event.id.0.as_str();
                if let Some(slug) = id.strip_prefix("spawn:") {
                    let _ = app.spawn(slug);
                } else if let Some(id_str) = id.strip_prefix("settings:") {
                    if let Ok(instance_id) = id_str.parse::<u32>() {
                        app.open_settings(instance_id);
                    }
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
        }
    }
    Ok(())
}

unsafe fn handle_drop(app: &mut App, hdrop: HDROP) {
    let count = unsafe { DragQueryFileW(hdrop, 0xFFFFFFFF, None) };
    let mut paths = Vec::new();
    for i in 0..count {
        let mut buf = [0u16; 260];
        let len = unsafe { DragQueryFileW(hdrop, i, Some(&mut buf)) };
        paths.push(std::path::PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])));
    }
    unsafe { DragFinish(hdrop) };
    for path in filter_zip_paths(&paths) {
        if let Ok(bytes) = std::fs::read(&path) {
            app.import_from_bytes(&bytes);
        }
    }
}
