use super::alpha::premultiply;
use crate::window::input::{classify_release, PointerSample, ReleaseKind};
use image::RgbaImage;
use std::time::Instant;
use windows::core::{w, Result};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, BLENDFUNCTION, AC_SRC_ALPHA, AC_SRC_OVER,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, GetCursorPos,
    GetWindowLongPtrW, PostMessageW, RegisterClassW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, TrackPopupMenu, UpdateLayeredWindow,
    GWLP_USERDATA, HTCLIENT, HTTRANSPARENT, MF_STRING, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, ULW_ALPHA,
    WM_APP, WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCHITTEST, WM_NULL,
    WM_RBUTTONUP, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP, WS_VISIBLE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;

pub const WM_MASCOT_TAP: u32 = WM_APP + 10;
pub const WM_MASCOT_FLING: u32 = WM_APP + 11;
pub const WM_MASCOT_JUMP: u32 = WM_APP + 12;
pub const WM_MASCOT_CLOSE: u32 = WM_APP + 13;
pub const WM_MASCOT_DRAG_START: u32 = WM_APP + 14;
pub const WM_MASCOT_OPEN_SETTINGS: u32 = WM_APP + 15;

struct WindowState {
    frame: RgbaImage,
    mouse_down: Option<PointerSample>,
    instance_id: u32,
    owner: HWND,
}

unsafe extern "system" fn mascot_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *ptr;

    match msg {
        WM_NCHITTEST => {
            // lparam carries screen coordinates for WM_NCHITTEST specifically.
            let mut pt = POINT {
                x: (lparam.0 & 0xFFFF) as i16 as i32,
                y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
            };
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);

            if crate::window::hit_test::is_opaque_at(&state.frame, pt.x, pt.y) {
                LRESULT(HTCLIENT as isize)
            } else {
                LRESULT(HTTRANSPARENT as isize)
            }
        }
        WM_LBUTTONDOWN => {
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            state.mouse_down = Some(PointerSample { time: Instant::now(), x: cursor.x, y: cursor.y });
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
            let _ = PostMessageW(Some(state.owner), WM_MASCOT_DRAG_START, WPARAM(state.instance_id as usize), LPARAM(0));
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if state.mouse_down.is_some() {
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                let half = (state.frame.width() / 2) as i32;
                let _ = SetWindowPos(hwnd, None, cursor.x - half, cursor.y - half, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            if let Some(down) = state.mouse_down.take() {
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                let up = PointerSample { time: Instant::now(), x: cursor.x, y: cursor.y };
                match classify_release(down, up) {
                    ReleaseKind::Tap => {
                        let _ = PostMessageW(Some(state.owner), WM_MASCOT_TAP, WPARAM(state.instance_id as usize), LPARAM(0));
                    }
                    ReleaseKind::Fling { vx, vy } => {
                        let packed = ((vy.clamp(-32768.0, 32767.0) as i16 as u32) << 16)
                            | (vx.clamp(-32768.0, 32767.0) as i16 as u16 as u32);
                        let _ = PostMessageW(Some(state.owner), WM_MASCOT_FLING, WPARAM(state.instance_id as usize), LPARAM(packed as isize));
                    }
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let menu = CreatePopupMenu().unwrap();
            let _ = AppendMenuW(menu, MF_STRING, 1, w!("Jump"));
            let _ = AppendMenuW(menu, MF_STRING, 3, w!("Settings..."));
            let _ = AppendMenuW(menu, MF_STRING, 2, w!("Close"));
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let _ = SetForegroundWindow(hwnd);
            let choice = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, cursor.x, cursor.y, Some(0), hwnd, None);
            // TrackPopupMenu doesn't free the HMENU (leaking one per right-click eventually
            // exhausts the process's USER handle quota and makes CreatePopupMenu itself start
            // failing), and the trailing WM_NULL is required for the owner to keep accepting
            // clicks on a later menu.
            let _ = DestroyMenu(menu);
            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            if choice.0 == 1 {
                let _ = PostMessageW(Some(state.owner), WM_MASCOT_JUMP, WPARAM(state.instance_id as usize), LPARAM(0));
            } else if choice.0 == 2 {
                let _ = PostMessageW(Some(state.owner), WM_MASCOT_CLOSE, WPARAM(state.instance_id as usize), LPARAM(0));
            } else if choice.0 == 3 {
                let _ = PostMessageW(Some(state.owner), WM_MASCOT_OPEN_SETTINGS, WPARAM(state.instance_id as usize), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            drop(Box::from_raw(ptr));
            let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub struct MascotWindow {
    pub hwnd: HWND,
}

impl MascotWindow {
    pub fn create(initial_frame: &RgbaImage, x: i32, y: i32, owner: HWND, instance_id: u32) -> Result<MascotWindow> {
        unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let class_name = w!("ShimejiMascotWindow");

            let wc = WNDCLASSW {
                lpfnWndProc: Some(mascot_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            // RegisterClassW fails harmlessly if already registered by an earlier instance; ignore that case.
            let _ = RegisterClassW(&wc);

            let width = initial_frame.width() as i32;
            let height = initial_frame.height() as i32;

            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                class_name,
                w!("Shimeji"),
                WS_POPUP | WS_VISIBLE,
                x,
                y,
                width,
                height,
                Some(owner),
                None,
                Some(hinstance.into()),
                None,
            )?;

            let state = Box::new(WindowState {
                frame: initial_frame.clone(),
                mouse_down: None,
                instance_id,
                owner,
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

            let window = MascotWindow { hwnd };
            window.update_frame(initial_frame);
            Ok(window)
        }
    }

    pub fn update_frame(&self, frame: &RgbaImage) {
        unsafe {
            let width = frame.width() as i32;
            let height = frame.height() as i32;
            let screen_dc = windows::Win32::Graphics::Gdi::GetDC(None);
            let mem_dc = CreateCompatibleDC(Some(screen_dc));

            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // negative = top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };

            let mut bits_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
            let dib = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0)
                .expect("CreateDIBSection failed");
            let old_bitmap = SelectObject(mem_dc, dib.into());

            let bgra = premultiply(frame);
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits_ptr as *mut u8, bgra.len());

            let size = SIZE { cx: width, cy: height };
            let src_pos = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };

            let _ = UpdateLayeredWindow(
                self.hwnd,
                Some(screen_dc),
                None,
                Some(&size),
                Some(mem_dc),
                Some(&src_pos),
                windows::Win32::Foundation::COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );

            SelectObject(mem_dc, old_bitmap);
            let _ = DeleteObject(dib.into());
            let _ = DeleteDC(mem_dc);
            windows::Win32::Graphics::Gdi::ReleaseDC(None, screen_dc);

            let ptr = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                (&mut *ptr).frame = frame.clone();
            }
        }
    }

    pub fn move_to(&self, x: i32, y: i32) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
}
