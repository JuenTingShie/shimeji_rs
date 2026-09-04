use shimeji::format::sprites::decode_sprite;
use shimeji::window::mascot_window::MascotWindow;
use std::path::Path;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, MSG, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::w;

// DefWindowProcW is a plain `unsafe fn` in this crate version, not `extern "system"`, so it
// can't be used directly as a WNDCLASSW.lpfnWndProc value — same issue MascotWindow::create
// had before it grew its own real WndProc. This throwaway owner window just needs any valid
// passthrough handler.
unsafe extern "system" fn owner_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn main() -> windows::core::Result<()> {
    let frame = decode_sprite(Path::new("tests/fixtures/fixture_bundle/sprites/0000.webp")).unwrap();

    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("ExampleOwnerWindow");
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
            w!("Owner"),
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

        let _window = MascotWindow::create(&frame, 400, 300, owner, 1)?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
