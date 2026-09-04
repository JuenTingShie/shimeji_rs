use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use shimeji::window::mascot_window::WM_MASCOT_CLOSE;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use winit::platform::windows::EventLoopBuilderExtWindows;

pub const WM_MASCOT_SET_SCALE: u32 = WM_APP + 16;
pub const WM_MASCOT_SET_SPEED: u32 = WM_APP + 17;
pub const WM_SETTINGS_OPENED: u32 = WM_APP + 18;
pub const WM_SETTINGS_CLOSED: u32 = WM_APP + 19;

/// One row of the mascot picker: enough state for the settings window to render controls and
/// report changes back without needing to touch `App`'s own mascot list from another thread.
pub struct MascotSettingsEntry {
    pub instance_id: u32,
    pub name: String,
    pub scale_pct: i32,
    pub speed_pct: i32,
}

/// Spawns the single settings window for the app's whole lifetime (see `App::open_settings`'s
/// singleton guard), covering every live mascot at once via a picker rather than one window per
/// mascot. eframe::run_native() blocks its calling thread until the window closes, which is
/// incompatible with sharing the main mascot loop's thread -- so this runs on its own thread
/// instead and talks back to `App` purely via `PostMessageW` to the owner HWND, which is
/// documented safe to call cross-thread and needs no shared/locked state with the main thread.
pub fn open_settings_window(owner: HWND, mascots: Vec<MascotSettingsEntry>, preferred_id: u32) {
    let owner_addr = owner.0 as isize;
    std::thread::spawn(move || {
        let owner = HWND(owner_addr as *mut _);
        // Guarantees WM_SETTINGS_CLOSED reaches the owner no matter how this thread's scope
        // ends -- a normal run_native return, an Err return, or a panic unwinding through here
        // (e.g. glow/GL context creation failing on a machine with no usable GPU driver). A
        // plain post placed after run_native's call only covers the first two: a panic unwinds
        // straight past it, which would permanently wedge App's singleton guard.
        let _guard = PostClosedOnDrop { owner };
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([280.0, 260.0])
                .with_resizable(false),
            renderer: eframe::Renderer::Glow,
            // winit refuses to create an event loop off the main thread by default (a
            // cross-platform footgun on most platforms, but this app's main thread is
            // permanently occupied by its own Win32 message loop) -- without this, run_native
            // below panics immediately and no window ever appears.
            event_loop_builder: Some(Box::new(|builder| {
                builder.with_any_thread(true);
            })),
            ..Default::default()
        };
        let selected = mascots.iter().position(|m| m.instance_id == preferred_id).unwrap_or(0);
        let app = SettingsApp { owner, mascots, selected, hwnd_reported: false };
        if let Err(err) = eframe::run_native("Mascot Settings", options, Box::new(move |_cc| Ok(Box::new(app)))) {
            crate::logging::log_error("settings_window", &err.to_string());
        }
    });
}

struct PostClosedOnDrop {
    owner: HWND,
}

impl Drop for PostClosedOnDrop {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(self.owner), WM_SETTINGS_CLOSED, WPARAM(0), LPARAM(0));
        }
    }
}

struct SettingsApp {
    owner: HWND,
    mascots: Vec<MascotSettingsEntry>,
    selected: usize,
    hwnd_reported: bool,
}

impl SettingsApp {
    fn post(&self, msg: u32, instance_id: u32, value: i32) {
        unsafe {
            let _ = PostMessageW(Some(self.owner), msg, WPARAM(instance_id as usize), LPARAM(value as isize));
        }
    }

    fn post_raw(&self, msg: u32, value: isize) {
        unsafe {
            let _ = PostMessageW(Some(self.owner), msg, WPARAM(0), LPARAM(value));
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

        if self.mascots.is_empty() {
            ui.label("No mascots are currently open.");
            return;
        }
        if self.selected >= self.mascots.len() {
            self.selected = self.mascots.len() - 1;
        }

        ui.heading("Mascot Settings");
        egui::ComboBox::from_label("Mascot")
            .selected_text(format!("{} #{}", self.mascots[self.selected].name, self.mascots[self.selected].instance_id))
            .show_ui(ui, |ui| {
                for (i, m) in self.mascots.iter().enumerate() {
                    ui.selectable_value(&mut self.selected, i, format!("{} #{}", m.name, m.instance_id));
                }
            });
        ui.separator();

        let instance_id = self.mascots[self.selected].instance_id;
        if ui.add(egui::Slider::new(&mut self.mascots[self.selected].scale_pct, 50..=200).text("Scale %")).changed() {
            self.post(WM_MASCOT_SET_SCALE, instance_id, self.mascots[self.selected].scale_pct);
        }
        if ui.add(egui::Slider::new(&mut self.mascots[self.selected].speed_pct, 25..=300).text("Speed %")).changed() {
            self.post(WM_MASCOT_SET_SPEED, instance_id, self.mascots[self.selected].speed_pct);
        }
        ui.separator();
        if ui.button("Remove Mascot").clicked() {
            self.post(WM_MASCOT_CLOSE, instance_id, 0);
            self.mascots.remove(self.selected);
            if self.selected >= self.mascots.len() && self.selected > 0 {
                self.selected -= 1;
            }
        }
    }
}
