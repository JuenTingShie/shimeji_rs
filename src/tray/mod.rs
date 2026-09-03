pub mod menu;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
};
use windows::Win32::UI::WindowsAndMessaging::{LoadIconW, IDI_APPLICATION, WM_APP};

pub const WM_TRAY_CALLBACK: u32 = WM_APP + 1;

pub struct TrayIcon {
    data: NOTIFYICONDATAW,
}

impl TrayIcon {
    pub fn create(owner: HWND) -> windows::core::Result<TrayIcon> {
        let mut data = NOTIFYICONDATAW::default();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = owner;
        data.uID = 1;
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        data.uCallbackMessage = WM_TRAY_CALLBACK;
        data.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION)? };
        let tip = "Shimeji\0".encode_utf16().collect::<Vec<u16>>();
        data.szTip[..tip.len()].copy_from_slice(&tip);

        unsafe {
            Shell_NotifyIconW(NIM_ADD, &data).ok()?;
        }
        Ok(TrayIcon { data })
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data);
        }
    }
}

impl TrayIcon {
    /// Shows a Windows balloon notification from the tray icon — used for both import failures
    /// ("this doesn't look like a mascot bundle: ...") and successes, per the spec's requirement
    /// that a rejected import explains why.
    pub fn notify(&self, title: &str, message: &str) {
        use windows::Win32::UI::Shell::NIF_INFO;
        let mut data = self.data;
        data.uFlags |= NIF_INFO;
        let title_u16 = title.encode_utf16().collect::<Vec<u16>>();
        let len = title_u16.len().min(data.szInfoTitle.len() - 1);
        data.szInfoTitle[..len].copy_from_slice(&title_u16[..len]);
        let msg_u16 = message.encode_utf16().collect::<Vec<u16>>();
        let len = msg_u16.len().min(data.szInfo.len() - 1);
        data.szInfo[..len].copy_from_slice(&msg_u16[..len]);
        unsafe {
            let _ = Shell_NotifyIconW(windows::Win32::UI::Shell::NIM_MODIFY, &data);
        }
    }
}
