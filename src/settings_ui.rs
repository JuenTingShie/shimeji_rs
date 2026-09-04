use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use shimeji::window::mascot_window::WM_MASCOT_CLOSE;
use std::sync::mpsc::Receiver;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use winit::platform::windows::EventLoopBuilderExtWindows;

pub const WM_MASCOT_SET_SCALE: u32 = WM_APP + 16;
pub const WM_MASCOT_SET_SPEED: u32 = WM_APP + 17;
pub const WM_SETTINGS_OPENED: u32 = WM_APP + 18;

/// One row of the mascot picker: enough state for the settings window to render controls and
/// report changes back without needing to touch `App`'s own mascot list from another thread.
#[derive(Clone)]
pub struct MascotSettingsEntry {
    pub instance_id: u32,
    pub name: String,
    pub scale_pct: i32,
    pub speed_pct: i32,
}

/// Sent from `App` (main thread) to the persistent settings-window thread. Winit only allows one
/// event loop to ever be created per process (see `open_settings_window`'s doc comment) -- so
/// "opening" the settings window after the first time means showing and refocusing the one
/// window that already exists, not creating a new one.
#[derive(Clone)]
pub enum SettingsCommand {
    Open { mascots: Vec<MascotSettingsEntry>, preferred_id: u32 },
}

/// Spawns the settings window's thread exactly once for the whole app's lifetime and never lets
/// it fully close. winit enforces a hard, process-wide, one-time-only rule on creating an
/// `EventLoop` (`EventLoopBuilder::build` returns `Err(RecreationAttempt)` for every call after
/// the first, even after the previous event loop's window was closed and dropped) -- so a design
/// that spawns a fresh thread + fresh `eframe::run_native` call per "open" can only ever show the
/// window once per process. Instead, the window's OS-level close button is intercepted (see
/// `SettingsApp::ui`'s `close_requested` check) and turned into a hide, and subsequent "opens"
/// are delivered as `SettingsCommand`s over `rx` to the still-running app.
///
/// eframe::run_native() blocks its calling thread until the (never-really-closing) window exits
/// with the process, which is incompatible with sharing the main mascot loop's thread -- so this
/// runs on its own thread instead and talks back to `App` purely via `PostMessageW` to the owner
/// HWND, which is documented safe to call cross-thread and needs no shared/locked state with the
/// main thread.
pub fn open_settings_window(owner: HWND, rx: Receiver<SettingsCommand>) {
    let owner_addr = owner.0 as isize;
    std::thread::spawn(move || {
        let owner = HWND(owner_addr as *mut _);
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([280.0, 260.0])
                .with_resizable(false)
                .with_visible(true),
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
        let app = SettingsApp {
            owner,
            rx,
            mascots: Vec::new(),
            selected: 0,
            hwnd_reported: false,
            focus_pending_frames: 0,
        };
        if let Err(err) = eframe::run_native("Mascot Settings", options, Box::new(move |_cc| Ok(Box::new(app)))) {
            crate::logging::log_error("settings_window", &err.to_string());
        }
    });
}

struct SettingsApp {
    owner: HWND,
    rx: Receiver<SettingsCommand>,
    mascots: Vec<MascotSettingsEntry>,
    selected: usize,
    hwnd_reported: bool,
    focus_pending_frames: u8,
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

        // The window is never actually allowed to close (see this module's doc comment on
        // `open_settings_window`) -- the OS close button just hides it, so it can be shown again
        // later without violating winit's one-event-loop-per-process rule.
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        while let Ok(SettingsCommand::Open { mascots, preferred_id }) = self.rx.try_recv() {
            self.selected = mascots.iter().position(|m| m.instance_id == preferred_id).unwrap_or(0);
            self.mascots = mascots;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(true));
            // Focus is a no-op while the window is still invisible/minimized, so it's deferred a
            // couple frames past the Visible(true) above rather than sent in the same frame.
            self.focus_pending_frames = 2;
        }
        if self.focus_pending_frames > 0 {
            self.focus_pending_frames -= 1;
            if self.focus_pending_frames == 0 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        // Keeps this app waking up to poll `rx` and the close-request flag above even while the
        // window is hidden and receiving no OS input events.
        ui.ctx().request_repaint_after(Duration::from_millis(50));

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
