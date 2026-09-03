use shimeji::environment::surface::query_surface;
use shimeji::environment::win32::{Win32MonitorSource, Win32WindowSource};
use shimeji::environment::{EnvironmentTracker, Rect};
use shimeji::format::animation::{Edge, EngineEventKind};
use shimeji::format::bundle::MascotBundle;
use shimeji::format::sprites::{decode_sprite, sprite_filename};
use shimeji::importer::catalog::CatalogEntry;
use shimeji::state_machine::{StateMachine, StateMachineSnapshot};
use shimeji::tray::TrayIcon;
use shimeji::window::mascot_window::MascotWindow;
use rand::rngs::StdRng;
use rand::SeedableRng;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
use std::time::{Duration, Instant};

const IDLE_THRESHOLD_TICKS: u32 = 3600;
const SPRITE_SIZE: i32 = 512;
const TICKS_PER_SECOND: f64 = 60.0;
const FLING_GRAVITY_PER_TICK: f64 = 0.6;

pub struct MascotInstance {
    pub id: u32,
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
}

pub struct App {
    pub library_root: std::path::PathBuf,
    pub catalog: Vec<CatalogEntry>,
    pub mascots: Vec<MascotInstance>,
    pub environment: EnvironmentTracker<Win32MonitorSource, Win32WindowSource>,
    pub tray: TrayIcon,
    pub owner: HWND,
    next_instance_id: u32,
}

impl App {
    pub fn new(library_root: std::path::PathBuf, owner: HWND, tray: TrayIcon) -> Self {
        let catalog = shimeji::importer::catalog::load_catalog(&library_root).unwrap_or_default();
        let environment = EnvironmentTracker::new(
            Win32MonitorSource,
            Win32WindowSource { exclude: Vec::new() },
            Duration::from_millis(150),
        );
        App { library_root, catalog, mascots: Vec::new(), environment, tray, owner, next_instance_id: 1 }
    }

    pub fn import_from_bytes(&mut self, zip_bytes: &[u8]) {
        match shimeji::importer::import_zip(zip_bytes, &self.library_root) {
            Ok(entry) => {
                self.tray.notify("Mascot imported", &format!("\"{}\" is ready to spawn.", entry.name));
                self.catalog = shimeji::importer::catalog::load_catalog(&self.library_root).unwrap_or_default();
            }
            Err(err) => {
                self.tray.notify("Import failed", &err.to_string());
            }
        }
    }

    pub fn spawn(&mut self, slug: &str) -> windows::core::Result<()> {
        let Some(entry) = self.catalog.iter().find(|e| e.slug == slug) else { return Ok(()) };
        let bundle = match MascotBundle::load(&entry.dir) {
            Ok(b) => b,
            Err(err) => {
                self.tray.notify("Could not spawn mascot", &err.to_string());
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

        let id = self.next_instance_id;
        self.next_instance_id += 1;
        let level = bundle.manifest.levels;
        let window = MascotWindow::create(&frame, 100, 100, self.owner, id)?;

        self.mascots.push(MascotInstance {
            id,
            bundle,
            sm_state,
            window,
            x: 100,
            y: 100,
            level,
            ticks_since_interaction: 0,
            fling_velocity: None,
            rng: StdRng::seed_from_u64(rand::random()),
        });
        Ok(())
    }

    pub fn close(&mut self, instance_id: u32) {
        if let Some(pos) = self.mascots.iter().position(|m| m.id == instance_id) {
            let mascot = self.mascots.remove(pos);
            unsafe {
                let _ = DestroyWindow(mascot.window.hwnd);
            }
        }
    }

    pub fn close_all(&mut self) {
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

        let mut sm = StateMachine::from_snapshot(&mascot.bundle.animation, &mascot.sm_state);
        let (pending_dx, pending_dy) = sm.pending_movement();
        let surface = query_surface(mascot.x, mascot.y, SPRITE_SIZE, SPRITE_SIZE, pending_dx, pending_dy, &screen, &monitors, &windows);
        if sm.apply_event(event, surface, mascot.level, &mut mascot.rng) {
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
            step_one_mascot(mascot, &screen, &monitors, &windows);
        }
    }
}

fn step_one_mascot(mascot: &mut MascotInstance, screen: &Rect, monitors: &[Rect], windows: &[Rect]) {
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
        SPRITE_SIZE,
        SPRITE_SIZE,
        anim_dx + fling_dx,
        anim_dy + fling_dy,
        screen,
        monitors,
        windows,
    );
    let landed_this_tick = surface.edge_hit == Some(Edge::Bottom);
    let hit_ceiling_this_tick = surface.edge_hit == Some(Edge::Top);
    let out = sm.step(surface, mascot.level, &mut mascot.rng);

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
        mascot.y = surface.floor_y - SPRITE_SIZE;
    } else if hit_ceiling_this_tick {
        // Symmetric to the floor snap above: a mascot climbing up would otherwise stop a few
        // pixels short of surface.ceiling_y, permanently reading as still airborne and never
        // reaching the ceiling-hang animation's own on-ceiling edge detection.
        mascot.y = surface.ceiling_y;
    } else {
        mascot.y += dy;
    }
    mascot.window.move_to(mascot.x, mascot.y);

    if sm.current_key() == "fling" && surface.edge_hit.is_some() {
        sm.apply_event(EngineEventKind::FlingEnd, surface, mascot.level, &mut mascot.rng);
        mascot.fling_velocity = None;
    }

    let sprite_path = mascot
        .bundle
        .base_path
        .join(&mascot.bundle.manifest.sprites.base_path)
        .join(sprite_filename(&mascot.bundle.manifest.sprites.file_pattern, out.sprite_index));
    if let Ok(frame) = decode_sprite(&sprite_path) {
        mascot.window.update_frame(&frame);
    }

    mascot.ticks_since_interaction += 1;
    if mascot.ticks_since_interaction == IDLE_THRESHOLD_TICKS {
        sm.apply_event(EngineEventKind::Idle, surface, mascot.level, &mut mascot.rng);
    }

    mascot.sm_state = sm.snapshot();
}
