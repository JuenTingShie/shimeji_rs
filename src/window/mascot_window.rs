use super::alpha::premultiply;
use image::RgbaImage;
use windows::core::{w, Result};
use windows::Win32::Foundation::{HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, BLENDFUNCTION, AC_SRC_ALPHA, AC_SRC_OVER,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, SetWindowPos, UpdateLayeredWindow,
    HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, ULW_ALPHA,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    WS_VISIBLE,
};

pub struct MascotWindow {
    pub hwnd: HWND,
}

unsafe extern "system" fn default_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

impl MascotWindow {
    pub fn create(initial_frame: &RgbaImage, x: i32, y: i32) -> Result<MascotWindow> {
        unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let class_name = w!("ShimejiMascotWindow");

            let wc = WNDCLASSW {
                lpfnWndProc: Some(default_window_proc),
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
                None,
                None,
                Some(hinstance.into()),
                None,
            )?;

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
        }
    }

    pub fn move_to(&self, x: i32, y: i32) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
}
