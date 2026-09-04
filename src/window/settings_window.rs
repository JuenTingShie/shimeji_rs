use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{HBRUSH, COLOR_BTNFACE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::SetScrollInfo;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetScrollInfo, GetWindowLongPtrW, HMENU,
    PostMessageW, RegisterClassW, SetWindowLongPtrW, SetWindowTextW, ShowWindow, BN_CLICKED,
    BS_PUSHBUTTON, GWLP_USERDATA, SB_CTL, SB_ENDSCROLL, SB_LINELEFT, SB_LINERIGHT, SB_PAGELEFT,
    SB_PAGERIGHT, SB_THUMBPOSITION, SB_THUMBTRACK, SCROLLBAR_COMMAND, SCROLLINFO, SIF_ALL, SIF_POS,
    SW_SHOW, WINDOW_STYLE, WM_APP, WM_COMMAND, WM_DESTROY, WM_HSCROLL, WNDCLASSW, WS_CAPTION,
    WS_CHILD, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::window::mascot_window::WM_MASCOT_CLOSE;

pub const WM_MASCOT_SET_SCALE: u32 = WM_APP + 16;
pub const WM_MASCOT_SET_SPEED: u32 = WM_APP + 17;
pub const WM_SETTINGS_CLOSED: u32 = WM_APP + 18;

const SCALE_MIN: i32 = 50;
const SCALE_MAX: i32 = 200;
const SPEED_MIN: i32 = 25;
const SPEED_MAX: i32 = 300;
const REMOVE_BUTTON_ID: i32 = 1;

#[derive(Clone, Copy)]
enum Slider {
    Scale,
    Speed,
}

struct WindowState {
    owner: HWND,
    instance_id: u32,
    scale_scroll: HWND,
    speed_scroll: HWND,
    scale_label: HWND,
    speed_label: HWND,
}

fn scroll_pos(hwnd: HWND) -> i32 {
    let mut si = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_ALL, ..Default::default() };
    unsafe {
        let _ = GetScrollInfo(hwnd, SB_CTL, &mut si as *mut SCROLLINFO);
    }
    si.nPos
}

fn set_scroll_pos(hwnd: HWND, pos: i32) {
    let si = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_POS, nPos: pos, ..Default::default() };
    unsafe {
        let _ = SetScrollInfo(hwnd, SB_CTL, &si, true);
    }
}

fn set_text(hwnd: HWND, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
    }
}

unsafe extern "system" fn settings_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *ptr;

    match msg {
        WM_HSCROLL => {
            let control = HWND(lparam.0 as *mut _);
            let Some(which) = (if control == state.scale_scroll {
                Some(Slider::Scale)
            } else if control == state.speed_scroll {
                Some(Slider::Speed)
            } else {
                None
            }) else {
                return LRESULT(0);
            };
            let (min, max) = match which {
                Slider::Scale => (SCALE_MIN, SCALE_MAX),
                Slider::Speed => (SPEED_MIN, SPEED_MAX),
            };
            let request = SCROLLBAR_COMMAND((wparam.0 & 0xFFFF) as i32);
            let current = scroll_pos(control);
            let new_pos = match request {
                SB_LINELEFT => current - 1,
                SB_LINERIGHT => current + 1,
                SB_PAGELEFT => current - 10,
                SB_PAGERIGHT => current + 10,
                SB_THUMBTRACK | SB_THUMBPOSITION => ((wparam.0 >> 16) & 0xFFFF) as i32,
                SB_ENDSCROLL => return LRESULT(0),
                _ => current,
            }
            .clamp(min, max);
            set_scroll_pos(control, new_pos);

            match which {
                Slider::Scale => {
                    set_text(state.scale_label, &format!("Scale: {new_pos}%"));
                    let _ = PostMessageW(Some(state.owner), WM_MASCOT_SET_SCALE, WPARAM(state.instance_id as usize), LPARAM(new_pos as isize));
                }
                Slider::Speed => {
                    set_text(state.speed_label, &format!("Speed: {new_pos}%"));
                    let _ = PostMessageW(Some(state.owner), WM_MASCOT_SET_SPEED, WPARAM(state.instance_id as usize), LPARAM(new_pos as isize));
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let control_id = (wparam.0 & 0xFFFF) as i32;
            let notification = ((wparam.0 >> 16) & 0xFFFF) as u32;
            if control_id == REMOVE_BUTTON_ID && notification == BN_CLICKED {
                let _ = PostMessageW(Some(state.owner), WM_MASCOT_CLOSE, WPARAM(state.instance_id as usize), LPARAM(0));
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = PostMessageW(Some(state.owner), WM_SETTINGS_CLOSED, WPARAM(hwnd.0 as usize), LPARAM(0));
            drop(Box::from_raw(ptr));
            let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub struct SettingsWindow {
    pub hwnd: HWND,
}

impl SettingsWindow {
    pub fn create(owner: HWND, instance_id: u32, scale_pct: i32, speed_pct: i32) -> Result<SettingsWindow> {
        unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let class_name = w!("ShimejiSettingsWindow");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(settings_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                hbrBackground: HBRUSH((COLOR_BTNFACE.0 as usize + 1) as *mut _),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                Default::default(),
                class_name,
                w!("Mascot Settings"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
                200,
                200,
                260,
                190,
                None,
                None,
                Some(hinstance.into()),
                None,
            )?;

            let scale_label_text = to_wide(&format!("Scale: {scale_pct}%"));
            let scale_label = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                PCWSTR(scale_label_text.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                20,
                15,
                200,
                20,
                Some(hwnd),
                None,
                Some(hinstance.into()),
                None,
            )?;
            let scale_scroll = CreateWindowExW(
                Default::default(),
                w!("SCROLLBAR"),
                w!(""),
                WS_CHILD | WS_VISIBLE,
                20,
                40,
                200,
                20,
                Some(hwnd),
                None,
                Some(hinstance.into()),
                None,
            )?;
            init_scroll(scale_scroll, SCALE_MIN, SCALE_MAX, scale_pct);

            let speed_label_text = to_wide(&format!("Speed: {speed_pct}%"));
            let speed_label = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                PCWSTR(speed_label_text.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                20,
                75,
                200,
                20,
                Some(hwnd),
                None,
                Some(hinstance.into()),
                None,
            )?;
            let speed_scroll = CreateWindowExW(
                Default::default(),
                w!("SCROLLBAR"),
                w!(""),
                WS_CHILD | WS_VISIBLE,
                20,
                100,
                200,
                20,
                Some(hwnd),
                None,
                Some(hinstance.into()),
                None,
            )?;
            init_scroll(speed_scroll, SPEED_MIN, SPEED_MAX, speed_pct);

            CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Remove Mascot"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
                20,
                135,
                200,
                30,
                Some(hwnd),
                Some(HMENU(REMOVE_BUTTON_ID as *mut _)),
                Some(hinstance.into()),
                None,
            )?;

            let state = Box::new(WindowState { owner, instance_id, scale_scroll, speed_scroll, scale_label, speed_label });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

            let _ = ShowWindow(hwnd, SW_SHOW);
            Ok(SettingsWindow { hwnd })
        }
    }
}

fn init_scroll(hwnd: HWND, min: i32, max: i32, pos: i32) {
    let si = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_ALL,
        nMin: min,
        nMax: max,
        nPage: 1,
        nPos: pos,
        ..Default::default()
    };
    unsafe {
        let _ = SetScrollInfo(hwnd, SB_CTL, &si, true);
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
