use shimeji::environment::surface::query_surface;
use shimeji::environment::win32::{Win32MonitorSource, Win32WindowSource};
use shimeji::environment::{EnvironmentTracker, Rect};
use shimeji::format::animation::{Edge, EngineEventKind};
use shimeji::format::bundle::MascotBundle;
use shimeji::format::sprites::{decode_sprite, oriented_sprite, sprite_filename};
use shimeji::importer::catalog::CatalogEntry;
use shimeji::state_machine::{StateMachine, StateMachineSnapshot};
use shimeji::tray::TrayIcon;
use shimeji::window::mascot_window::MascotWindow;
use image::imageops::{resize, FilterType};
use image::RgbaImage;
use rand::rngs::StdRng;
use rand::SeedableRng;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, GetWindowRect};
use std::time::{Duration, Instant};

const IDLE_THRESHOLD_TICKS: u32 = 3600;
const SPRITE_SIZE: i32 = 512;
const TICKS_PER_SECOND: f64 = 60.0;
const FLING_GRAVITY_PER_TICK: f64 = 0.6;

/// The physical sprite/collision size for this mascot at its current scale -- SPRITE_SIZE scaled
/// by `mascot.scale`, used everywhere query_surface or a floor/ceiling snap needs the mascot's
/// actual on-screen footprint instead of the bundle's native size.
fn sprite_size(mascot: &MascotInstance) -> i32 {
    (SPRITE_SIZE as f64 * mascot.scale).round() as i32
}

/// Resizes a decoded sprite frame to the mascot's current scale before it's handed to
/// `MascotWindow` -- `UpdateLayeredWindow` resizes the actual OS window to match whatever frame
/// it's given, so this is the only place scale needs to be applied for rendering.
fn scaled_frame(frame: RgbaImage, scale: f64) -> RgbaImage {
    if (scale - 1.0).abs() < f64::EPSILON {
        return frame;
    }
    let size = ((SPRITE_SIZE as f64 * scale).round() as u32).max(1);
    resize(&frame, size, size, FilterType::Lanczos3)
}

pub struct MascotInstance {
    pub id: u32,
    pub slug: String,
    pub bundle: MascotBundle,
    // Holds the state machine's mid-animation progress (frame position, tick counts, resolved
    // timers) between ticks. A `StateMachine<'a>` can't be stored here directly — it borrows
    // `bundle.animation`, and Rust can't express a struct holding both a value and a borrow of
    // that same value's sibling field — so each tick rebuilds one via `StateMachine::from_snapshot`
    // and saves the result back here via `.snapshot()`. See `StateMachineSnapshot`'s doc comment.
    pub sm_state: StateMachineSnapshot,
    pub window: MascotWindow,
    pub x: i32,
    pub y: i32,
    pub level: u8,
    pub ticks_since_interaction: u32,
    pub fling_velocity: Option<(f64, f64)>,
    pub rng: StdRng,
    // While the user is physically holding the mascot (WM_MOUSEMOVE repositions the OS window
    // directly, bypassing x/y below), tick() must not also step physics/animation for it — doing
    // both at once raced the cursor-driven position against a stale computed one every frame. See
    // handle_engine_event's DragStart/Tap/FlingStart handling.
    pub dragging: bool,
    pub scale: f64,
    pub speed: f64,
    // Fractional ticks carried between calls to `tick()` so `speed` can run the state machine
    // faster (multiple `step_one_mascot` calls in one real tick) or slower (skip some real ticks
    // entirely) than the real 60Hz timer, without either behavior needing its own separate path.
    pub tick_accumulator: f64,
}

pub struct App {
    pub library_root: std::path::PathBuf,
    pub catalog: Vec<CatalogEntry>,
    pub mascots: Vec<MascotInstance>,
    pub environment: EnvironmentTracker<Win32MonitorSource, Win32WindowSource>,
    pub tray: TrayIcon,
    pub owner: HWND,
    session_path: std::path::PathBuf,
    next_instance_id: u32,
    settings_tx: Option<std::sync::mpsc::Sender<crate::settings_ui::SettingsCommand>>,
}

impl App {
    pub fn new(library_root: std::path::PathBuf, session_path: std::path::PathBuf, owner: HWND, tray: TrayIcon) -> Self {
        let catalog = shimeji::importer::catalog::load_catalog(&library_root).unwrap_or_default();
        let environment = EnvironmentTracker::new(
            Win32MonitorSource,
            Win32WindowSource { exclude: Vec::new() },
            Duration::from_millis(150),
        );
        tray.set_menu(shimeji::tray::build_menu(&catalog, &[]));
        let mut app = App {
            library_root,
            catalog,
            mascots: Vec::new(),
            environment,
            tray,
            owner,
            session_path,
            next_instance_id: 1,
            settings_tx: None,
        };
        app.restore_session();
        app
    }

    pub fn import_from_bytes(&mut self, zip_bytes: &[u8]) {
        match shimeji::importer::import_zip(zip_bytes, &self.library_root) {
            Ok(_entry) => {
                self.catalog = shimeji::importer::catalog::load_catalog(&self.library_root).unwrap_or_default();
                self.refresh_tray_menu();
            }
            Err(err) => crate::logging::log_error("import", &err.to_string()),
        }
    }

    pub fn spawn(&mut self, slug: &str) -> windows::core::Result<()> {
        let Some(entry) = self.catalog.iter().find(|e| e.slug == slug) else { return Ok(()) };
        let bundle = match MascotBundle::load(&entry.dir) {
            Ok(b) => b,
            Err(err) => {
                crate::logging::log_error("spawn", &err.to_string());
                return Ok(());
            }
        };

        let sm_state = StateMachine::initial_snapshot(&bundle.animation);
        let default_anim = bundle
            .animation
            .animations
            .iter()
            .find(|a| a.key == sm_state.current_key)
            .unwrap();
        let sprite_path = bundle
            .base_path
            .join(&bundle.manifest.sprites.base_path)
            .join(sprite_filename(&bundle.manifest.sprites.file_pattern, default_anim.frames[0].sprite));
        let frame = decode_sprite(&sprite_path).map_err(|_| windows::core::Error::empty())?;
        let frame = oriented_sprite(frame, sm_state.facing);

        let id = self.next_instance_id;
        self.next_instance_id += 1;
        let level = bundle.manifest.levels;
        let window = MascotWindow::create(&scaled_frame(frame, 1.0), 100, 100, self.owner, id)?;
        // Each mascot's own window is itself visible and titled ("Shimeji"), so without this it
        // shows up in the environment's own "other windows on the desktop" list -- meaning with a
        // second mascot spawned, either could be treated as real floor/ceiling geometry for the
        // other, or a mascot could even land on its own window rect.
        self.environment.window_source.exclude.push(window.hwnd);

        self.mascots.push(MascotInstance {
            id,
            slug: slug.to_string(),
            bundle,
            sm_state,
            window,
            x: 100,
            y: 100,
            level,
            ticks_since_interaction: 0,
            fling_velocity: None,
            rng: StdRng::seed_from_u64(rand::random()),
            dragging: false,
            scale: 1.0,
            speed: 1.0,
            tick_accumulator: 0.0,
        });
        self.refresh_tray_menu();
        self.save_session();
        Ok(())
    }

    /// Opens the single app-wide settings window, preselecting `preferred_id` if it's currently
    /// live. Winit only allows one event loop to ever be created per process, so the window's
    /// thread is spawned at most once for the whole app's lifetime; every open after the first is
    /// delivered as a `SettingsCommand` to the still-running window, which shows and refocuses
    /// itself (see `settings_ui::open_settings_window`'s doc comment for why). If the still-running
    /// window's thread has actually died (e.g. it failed to create a GL context on first launch),
    /// the send fails and a fresh thread is spawned to retry.
    pub fn open_settings(&mut self, preferred_id: u32) {
        let mascots: Vec<crate::settings_ui::MascotSettingsEntry> = self
            .mascots
            .iter()
            .map(|m| crate::settings_ui::MascotSettingsEntry {
                instance_id: m.id,
                name: m.bundle.manifest.name.clone(),
                scale_pct: (m.scale * 100.0).round() as i32,
                speed_pct: (m.speed * 100.0).round() as i32,
            })
            .collect();
        if mascots.is_empty() {
            return;
        }
        let cmd = crate::settings_ui::SettingsCommand::Open { mascots, preferred_id };
        let needs_spawn = match &self.settings_tx {
            Some(tx) => tx.send(cmd.clone()).is_err(),
            None => true,
        };
        if needs_spawn {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = tx.send(cmd);
            crate::settings_ui::open_settings_window(self.owner, rx);
            self.settings_tx = Some(tx);
        }
    }

    fn refresh_tray_menu(&mut self) {
        let live: Vec<(u32, String)> = self.mascots.iter().map(|m| (m.id, m.bundle.manifest.name.clone())).collect();
        self.tray.set_menu(shimeji::tray::build_menu(&self.catalog, &live));
    }

    fn save_session(&self) {
        let entries: Vec<shimeji::session::SessionEntry> = self
            .mascots
            .iter()
            .map(|m| shimeji::session::SessionEntry { slug: m.slug.clone(), scale: m.scale, speed: m.speed })
            .collect();
        if let Err(err) = shimeji::session::save_session(&self.session_path, &entries) {
            crate::logging::log_error("session", &err.to_string());
        }
    }

    /// Restores last session's live mascots (by catalog slug) and their scale/speed, run once
    /// from `App::new`. Position is intentionally not restored -- every restored mascot spawns at
    /// the same default point a fresh manual spawn would. An entry whose slug no longer resolves
    /// to a loadable bundle (removed/renamed/corrupt since last run) is silently dropped; the
    /// re-save after the loop prunes it from session.json instead of retrying it forever.
    fn restore_session(&mut self) {
        let entries = match shimeji::session::load_session(&self.session_path) {
            Ok(entries) => entries,
            Err(err) => {
                crate::logging::log_error("session", &err.to_string());
                return;
            }
        };
        for entry in &entries {
            let before = self.mascots.len();
            let _ = self.spawn(&entry.slug);
            if self.mascots.len() > before {
                if let Some(mascot) = self.mascots.last_mut() {
                    mascot.scale = entry.scale;
                    mascot.speed = entry.speed;
                }
            }
        }
        self.save_session();
    }

    pub fn settings_window_opened(&mut self, hwnd_raw: isize) {
        let hwnd = HWND(hwnd_raw as *mut _);
        // The settings window is itself a real, visible, titled top-level window -- excluded from
        // the desktop window list for the same reason mascot windows are (see spawn's comment).
        // Reported exactly once: the window is created at most once per process and only ever
        // hidden, never destroyed (see settings_ui::open_settings_window).
        self.environment.window_source.exclude.push(hwnd);
    }

    pub fn set_scale(&mut self, instance_id: u32, pct: i32) {
        if let Some(mascot) = self.mascots.iter_mut().find(|m| m.id == instance_id) {
            mascot.scale = pct as f64 / 100.0;
        }
        self.save_session();
    }

    pub fn set_speed(&mut self, instance_id: u32, pct: i32) {
        if let Some(mascot) = self.mascots.iter_mut().find(|m| m.id == instance_id) {
            mascot.speed = pct as f64 / 100.0;
        }
        self.save_session();
    }

    pub fn close(&mut self, instance_id: u32) {
        if let Some(pos) = self.mascots.iter().position(|m| m.id == instance_id) {
            let mascot = self.mascots.remove(pos);
            self.environment.window_source.exclude.retain(|h| *h != mascot.window.hwnd);
            unsafe {
                let _ = DestroyWindow(mascot.window.hwnd);
            }
            self.refresh_tray_menu();
            self.save_session();
        }
    }

    pub fn close_all(&mut self) {
        self.environment.window_source.exclude.clear();
        for mascot in self.mascots.drain(..) {
            unsafe {
                let _ = DestroyWindow(mascot.window.hwnd);
            }
        }
        self.refresh_tray_menu();
        self.save_session();
    }

    /// Destroys every live mascot window without touching session.json -- used only when the
    /// whole app is shutting down (see main.rs's "exit" handler). Unlike `close_all`, which is a
    /// real user action ("Close All" while the app keeps running) and correctly persists "nothing
    /// is running" as the saved state, quitting should leave the current session on disk so it
    /// can be restored on the next launch.
    pub fn destroy_all_windows(&mut self) {
        for mascot in self.mascots.drain(..) {
            unsafe {
                let _ = DestroyWindow(mascot.window.hwnd);
            }
        }
    }

    pub fn handle_engine_event(&mut self, instance_id: u32, event: EngineEventKind, fling_velocity: Option<(f64, f64)>) {
        let (screen, monitors, windows) = self.environment.poll(Instant::now());
        let screen = *screen;
        let monitors = monitors.to_vec();
        let windows = windows.to_vec();
        let Some(mascot) = self.mascots.iter_mut().find(|m| m.id == instance_id) else { return };

        if event == EngineEventKind::DragStart {
            mascot.dragging = true;
        } else if mascot.dragging && matches!(event, EngineEventKind::Tap | EngineEventKind::FlingStart) {
            // The mouse was just released (WM_LBUTTONUP always classifies into one of these two
            // events). x/y haven't tracked the window's real position since DragStart -- pull it
            // from the actual OS window before physics resumes, or the mascot would snap back to
            // wherever it was picked up from on the very next tick.
            mascot.dragging = false;
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(mascot.window.hwnd, &mut rect) }.is_ok() {
                mascot.x = rect.left;
                mascot.y = rect.top;
            }
        }

        let size = sprite_size(mascot);
        let mut sm = StateMachine::from_snapshot(&mascot.bundle.animation, &mascot.sm_state);
        let (pending_dx, pending_dy) = sm.pending_movement();
        let mut surface = query_surface(mascot.x, mascot.y, size, size, pending_dx, pending_dy, &screen, &monitors, &windows);
        if event == EngineEventKind::Jump {
            // JUMP's schema rules key `when` off which surface the mascot is currently resting
            // on (ground/ceiling/wall), not an edge crossed this exact tick like every other
            // event genuinely wants from the live geometry query above -- see
            // StateMachine::resting_edge's doc comment.
            surface.edge_hit = sm.resting_edge();
        }
        if sm.apply_event(event, surface, mascot.level, &mut mascot.rng) {
            crate::logging::log_animation(mascot.id, &format!("{event:?}"), sm.current_key());
            mascot.sm_state = sm.snapshot();
            mascot.ticks_since_interaction = 0;
            if event == EngineEventKind::FlingStart {
                if let Some((vx, vy)) = fling_velocity {
                    mascot.fling_velocity = Some((vx / TICKS_PER_SECOND, vy / TICKS_PER_SECOND));
                }
            }
        }
    }

    pub fn tick(&mut self, now: Instant) {
        let (screen, monitors, windows) = self.environment.poll(now);
        let screen = *screen;
        let monitors = monitors.to_vec();
        let windows = windows.to_vec();

        for mascot in &mut self.mascots {
            if mascot.dragging {
                continue;
            }
            // `speed` lets a mascot run faster (multiple steps this real tick) or slower (skip
            // some real ticks) than the 60Hz timer -- see tick_accumulator's doc comment.
            mascot.tick_accumulator += mascot.speed;
            while mascot.tick_accumulator >= 1.0 {
                mascot.tick_accumulator -= 1.0;
                step_one_mascot(mascot, &screen, &monitors, &windows);
            }
        }
    }
}

fn step_one_mascot(mascot: &mut MascotInstance, screen: &Rect, monitors: &[Rect], windows: &[Rect]) {
    let size = sprite_size(mascot);
    let mut sm = StateMachine::from_snapshot(&mascot.bundle.animation, &mascot.sm_state);

    // The surface query needs this tick's *total* proposed movement to predict edge crossings
    // (e.g. a falling mascot about to reach the floor) — that's the current animation frame's own
    // dx/dy plus any active fling velocity (the "fling" animation's own frames carry no movement;
    // the thrown velocity is engine state layered on top, per the design).
    let (anim_dx, anim_dy) = sm.pending_movement();
    let (fling_dx, fling_dy) = mascot
        .fling_velocity
        .map(|(vx, vy)| (vx.round() as i32, vy.round() as i32))
        .unwrap_or((0, 0));
    let surface = query_surface(
        mascot.x,
        mascot.y,
        size,
        size,
        anim_dx + fling_dx,
        anim_dy + fling_dy,
        screen,
        monitors,
        windows,
    );
    let landed_this_tick = surface.edge_hit == Some(Edge::Bottom);
    let hit_ceiling_this_tick = surface.edge_hit == Some(Edge::Top);
    let out = sm.step(surface, mascot.level, &mut mascot.rng);
    if out.changed_animation {
        crate::logging::log_animation(mascot.id, "auto", sm.current_key());
    }

    let mut dx = out.dx;
    let mut dy = out.dy;
    if let Some((vx, vy)) = mascot.fling_velocity {
        dx += vx.round() as i32;
        dy += vy.round() as i32;
        mascot.fling_velocity = Some((vx, vy + FLING_GRAVITY_PER_TICK));
    }

    mascot.x += dx;
    if landed_this_tick {
        // The fall animation's own dy is a fixed per-tick step that essentially never divides
        // evenly into "distance to the floor", so the tick that reports the BOTTOM edge would
        // otherwise land a few pixels short of surface.floor_y instead of exactly on it —
        // permanently, since nothing else corrects it afterward. That gap can exceed
        // query_surface's ground tolerance, which misclassifies an already-landed mascot as
        // still airborne on every later tick; since ground edge detection only runs while
        // grounded, that silently disables it and lets the mascot walk straight off the real
        // screen edge undetected. Snap to the exact floor instead of trusting the animation's
        // own dy (now discarded in favor of this) to land there.
        mascot.y = surface.floor_y - size;
    } else if hit_ceiling_this_tick {
        // Symmetric to the floor snap above: a mascot climbing up would otherwise stop a few
        // pixels short of surface.ceiling_y, permanently reading as still airborne and never
        // reaching the ceiling-hang animation's own on-ceiling edge detection.
        mascot.y = surface.ceiling_y;
    } else {
        mascot.y += dy;
    }
    mascot.window.move_to(mascot.x, mascot.y);

    if mascot.fling_velocity.is_some() && surface.edge_hit.is_some() {
        // Key off fling_velocity rather than the current animation being named "fling": a
        // bundle's own on_finish/on_timer/border-transition rules can move the state machine off
        // that key before the mascot actually lands, and checking the key name would then never
        // fire again, leaving fling_velocity accumulating gravity forever. Skip the wildcard
        // FLING_END rule if a border transition already redirected the animation this same tick
        // (e.g. a landing animation with its own Bottom/Left/Right transitions) so it doesn't get
        // immediately overridden.
        if !out.changed_animation {
            if sm.apply_event(EngineEventKind::FlingEnd, surface, mascot.level, &mut mascot.rng) {
                crate::logging::log_animation(mascot.id, "FlingEnd", sm.current_key());
            }
        }
        mascot.fling_velocity = None;
    }

    let sprite_path = mascot
        .bundle
        .base_path
        .join(&mascot.bundle.manifest.sprites.base_path)
        .join(sprite_filename(&mascot.bundle.manifest.sprites.file_pattern, out.sprite_index));
    if let Ok(frame) = decode_sprite(&sprite_path) {
        let frame = oriented_sprite(frame, sm.facing());
        mascot.window.update_frame(&scaled_frame(frame, mascot.scale));
    }

    mascot.ticks_since_interaction += 1;
    if mascot.ticks_since_interaction == IDLE_THRESHOLD_TICKS {
        if sm.apply_event(EngineEventKind::Idle, surface, mascot.level, &mut mascot.rng) {
            crate::logging::log_animation(mascot.id, "Idle", sm.current_key());
        }
    }

    mascot.sm_state = sm.snapshot();
}
