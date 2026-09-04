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
        // Guarantees WM_SETTINGS_CLOSED reaches the owner no matter how this thread's scope
        // ends -- a normal run_native return, an Err return, or a panic unwinding through here
        // (e.g. glow/GL context creation failing on a machine with no usable GPU driver). A
        // plain post placed after run_native's call only covers the first two: a panic unwinds
        // straight past it, which would permanently wedge App::open_settings_instances for this
        // instance_id since nothing would ever release the duplicate-open guard.
        let _guard = PostClosedOnDrop { owner, instance_id };
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([260.0, 190.0])
                .with_resizable(false),
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        };
        let app = SettingsApp { owner, instance_id, scale_pct, speed_pct, hwnd_reported: false };
        let _ = eframe::run_native("Mascot Settings", options, Box::new(move |_cc| Ok(Box::new(app))));
    });
}

struct PostClosedOnDrop {
    owner: HWND,
    instance_id: u32,
}

impl Drop for PostClosedOnDrop {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(self.owner), WM_SETTINGS_CLOSED, WPARAM(self.instance_id as usize), LPARAM(0));
        }
    }
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
