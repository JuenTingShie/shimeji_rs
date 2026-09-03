use shimeji::environment::win32::{Win32MonitorSource, Win32WindowSource};
use shimeji::environment::{MonitorSource, WindowSource};

fn main() {
    let monitors = Win32MonitorSource.monitors();
    println!("Monitors ({}):", monitors.len());
    for r in &monitors {
        println!("  {:?}", r);
    }

    let windows = Win32WindowSource { exclude: vec![] }.windows();
    println!("Visible top-level windows ({}):", windows.len());
    for r in &windows {
        println!("  {:?}", r);
    }
}
