mod app;

use app::App;
use shimeji::importer::catalog::CatalogEntry;
use shimeji::tray::menu::{build_spawn_items, filter_zip_paths, CLOSE_ALL_MENU_ID, EXIT_MENU_ID, IMPORT_MENU_ID};
use shimeji::tray::{TrayIcon, WM_TRAY_CALLBACK};
use shimeji::window::mascot_window::{WM_MASCOT_CLOSE, WM_MASCOT_DRAG_START, WM_MASCOT_FLING, WM_MASCOT_JUMP, WM_MASCOT_TAP};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetCursorPos,
    PeekMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, SetTimer,
    TrackPopupMenu, TranslateMessage, MF_STRING, MSG, PM_REMOVE, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    WM_DESTROY, WM_DROPFILES, WM_QUIT, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_POPUP,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::core::w;

const TICK_TIMER_ID: usize = 1;
const TICK_INTERVAL_MS: u32 = 1000 / 60;

unsafe extern "system" fn owner_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn main() -> windows::core::Result<()> {
    let library_root = dirs_next::data_dir()
        .expect("APPDATA must be resolvable on Windows")
        .join("ShimejiRust")
        .join("mascots");

    unsafe {
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

        let tray = TrayIcon::create(owner)?;
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
                    WM_MASCOT_TAP => app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::DragEnd, None),
                    WM_MASCOT_DRAG_START => app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::DragStart, None),
                    WM_MASCOT_FLING => {
                        let packed = msg.lParam.0 as u32;
                        let vx = (packed & 0xFFFF) as i16 as f64;
                        let vy = ((packed >> 16) & 0xFFFF) as i16 as f64;
                        app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::FlingStart, Some((vx, vy)));
                    }
                    WM_MASCOT_JUMP => app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::Jump, None),
                    WM_MASCOT_CLOSE => app.close(msg.wParam.0 as u32),
                    WM_TRAY_CALLBACK => {
                        if (msg.lParam.0 as u32) == WM_RBUTTONUP {
                            show_tray_menu(owner, &mut app);
                        }
                    }
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

unsafe fn show_tray_menu(owner: HWND, app: &mut App) {
    let menu = CreatePopupMenu().unwrap();
    let mut labels: Vec<(u16, CatalogEntry)> = Vec::new();
    for (id, label) in build_spawn_items(&app.catalog) {
        let entry = app.catalog[(id - shimeji::tray::menu::SPAWN_MENU_ID_BASE) as usize].clone();
        let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(menu, MF_STRING, id as usize, windows::core::PCWSTR(wide.as_ptr()));
        labels.push((id, entry));
    }
    let _ = AppendMenuW(menu, MF_STRING, IMPORT_MENU_ID as usize, w!("Import Mascot..."));
    let _ = AppendMenuW(menu, MF_STRING, CLOSE_ALL_MENU_ID as usize, w!("Close All"));
    let _ = AppendMenuW(menu, MF_STRING, EXIT_MENU_ID as usize, w!("Exit"));

    let mut cursor = POINT::default();
    let _ = GetCursorPos(&mut cursor);
    let _ = SetForegroundWindow(owner);
    let choice = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, cursor.x, cursor.y, Some(0), owner, None).0 as u16;

    if choice == IMPORT_MENU_ID {
        if let Some(path) = rfd::FileDialog::new().add_filter("Mascot bundle", &["zip"]).pick_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                app.import_from_bytes(&bytes);
            }
        }
    } else if choice == CLOSE_ALL_MENU_ID {
        app.close_all();
    } else if choice == EXIT_MENU_ID {
        app.close_all();
        PostQuitMessage(0);
    } else if let Some((_, entry)) = labels.into_iter().find(|(id, _)| *id == choice) {
        let _ = app.spawn(&entry.slug);
    }
}
