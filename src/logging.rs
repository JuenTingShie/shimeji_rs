use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Must be called once at startup before any `log_animation` call is expected to do anything —
/// calls before `init` (or if it's never called) are silently dropped rather than panicking, so
/// logging can never be the reason the app fails to start.
pub fn init(path: PathBuf) {
    let _ = LOG_PATH.set(path);
}

/// Appends one line recording an animation actually taking effect — either a named engine event
/// (`Tap`, `Jump`, `FlingStart`, ...) forcing a transition, or `"auto"` for the state machine's
/// own progression (onFinish/onTimer/border transitions during normal ticking). Exists so a
/// bundle's animation behavior can be verified against `animation.json` after the fact, without
/// needing to watch the mascot live.
pub fn log_animation(instance_id: u32, trigger: &str, key: &str) {
    let Some(path) = LOG_PATH.get() else { return };
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{secs:.3}] mascot#{instance_id} {trigger} -> {key}");
    }
}

/// Records an operation failure (import/spawn/bundle-load) that has no other visible surface --
/// this is a `windows_subsystem = "windows"` GUI binary, so `eprintln!` has no console to reach,
/// and the tray-icon migration dropped balloon notifications entirely. Without this, a corrupt
/// zip or a bad bundle would fail with zero record anywhere.
pub fn log_error(context: &str, message: &str) {
    let Some(path) = LOG_PATH.get() else { return };
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{secs:.3}] ERROR {context}: {message}");
    }
}
