//! KS Download Manager desktop shell. The download engine is the `engine` crate (src-tauri/engine);
//! the shell does not call it yet (Stage 4 wires it to the interface).

use tauri::window::Color;
use tauri::{Manager, Theme};

/// Window background before the page paints, matching `--background` in src/index.css.
fn background(theme: Theme) -> Color {
    match theme {
        Theme::Dark => Color(0x0e, 0x10, 0x12, 0xff),
        _ => Color(0xff, 0xff, 0xff, 0xff),
    }
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // The main window starts hidden (tauri.conf.json) so it never flashes white in dark mode.
            if let Some(window) = app.get_webview_window("main") {
                let theme = window.theme().unwrap_or(Theme::Light);
                window.set_background_color(Some(background(theme)))?;
                window.show()?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running KS Download Manager");
}
