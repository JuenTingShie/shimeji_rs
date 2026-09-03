use shimeji::format::sprites::decode_sprite;
use shimeji::window::mascot_window::MascotWindow;
use std::path::Path;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, TranslateMessage, MSG};

fn main() {
    let frame = decode_sprite(Path::new("tests/fixtures/sample_bundle/sprites/0000.webp")).unwrap();
    let _window = MascotWindow::create(&frame, 400, 300).unwrap();

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
