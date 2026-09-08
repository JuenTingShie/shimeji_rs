use super::{MonitorSource, Rect, WindowSource};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowTextLengthW, IsWindowVisible, IsIconic,
};

pub struct Win32MonitorSource;
pub struct Win32WindowSource {
    pub exclude: Vec<HWND>,
}

impl MonitorSource for Win32MonitorSource {
    fn monitors(&self) -> Vec<Rect> {
        let mut rects: Vec<Rect> = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(monitor_enum_proc),
                LPARAM(&mut rects as *mut Vec<Rect> as isize),
            );
        }
        rects
    }
}

unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    unsafe {
        let rects = &mut *(lparam.0 as *mut Vec<Rect>);
        // The work area (rcWork) excludes the taskbar and any other app-reserved screen space,
        // unlike the raw monitor rect -- using the raw rect here would put the floor underneath
        // an always-on-top taskbar, letting mascots walk/land where the taskbar visually covers
        // them instead of stopping on top of it. Fall back to the raw rect if the query fails.
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let r = if GetMonitorInfoW(hmonitor, &mut info).as_bool() { info.rcWork } else { *rect };
        rects.push(Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom });
    }
    BOOL(1)
}

impl WindowSource for Win32WindowSource {
    fn windows(&self) -> Vec<Rect> {
        let mut collected: Vec<(HWND, Rect)> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(window_enum_proc), LPARAM(&mut collected as *mut Vec<(HWND, Rect)> as isize));
        }
        collected
            .into_iter()
            .filter(|(hwnd, _)| !self.exclude.contains(hwnd))
            .map(|(_, rect)| rect)
            .collect()
    }
}

unsafe extern "system" fn window_enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let collected = &mut *(lparam.0 as *mut Vec<(HWND, Rect)>);

        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return BOOL(1);
        }
        if GetWindowTextLengthW(hwnd) == 0 {
            return BOOL(1); // skip windows with no title (tool/helper windows)
        }

        let mut cloaked: u32 = 0;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return BOOL(1); // skip cloaked windows (e.g. UWP apps on another virtual desktop)
        }

        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            collected.push((hwnd, Rect { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }));
        }
    }
    BOOL(1)
}
