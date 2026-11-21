//! KS Download Manager desktop shell: the window, the commands the interface calls, and
//! the events it listens to. Downloading is done by the `engine` crate (src-tauri/engine);
//! the list and the running downloads are kept by [`manager::Manager`].

mod files;
mod manager;
mod open;
mod store;

use std::sync::Arc;

use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager as _, RunEvent, State, Theme, WindowEvent};

use manager::{Active, Events, Manager, ProbeView, Tick};
use store::Record;

/// Window background before the page paints, matching `--background` in src/index.css.
fn background(theme: Theme) -> Color {
    match theme {
        Theme::Dark => Color(0x0e, 0x10, 0x12, 0xff),
        _ => Color(0xff, 0xff, 0xff, 0xff),
    }
}

/// Sends manager updates to the window.
struct WindowEvents(AppHandle);

impl Events for WindowEvents {
    fn changed(&self, record: &Record) {
        let _ = self.0.emit("downloads:changed", record);
    }
    fn removed(&self, id: u64) {
        let _ = self.0.emit("downloads:removed", id);
    }
    fn progress(&self, ticks: &[Tick]) {
        let _ = self.0.emit("downloads:progress", ticks);
    }
}

type Downloads<'a> = State<'a, Arc<Manager>>;

#[tauri::command]
fn list_downloads(m: Downloads<'_>) -> Vec<Record> {
    m.list()
}

#[tauri::command]
fn default_folder(app: AppHandle) -> Result<String, String> {
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
async fn probe_url(m: Downloads<'_>, url: String) -> Result<ProbeView, String> {
    m.probe(&url).await
}

#[tauri::command]
async fn add_download(m: Downloads<'_>, url: String, name: String, folder: String, start: bool) -> Result<Record, String> {
    m.inner().add(&url, &name, &folder, start).await
}

#[tauri::command]
fn resume_download(m: Downloads<'_>, id: u64) -> Result<(), String> {
    m.inner().resume(id)
}

#[tauri::command]
fn pause_download(m: Downloads<'_>, id: u64) -> Result<(), String> {
    m.pause(id)
}

#[tauri::command]
async fn delete_download(m: Downloads<'_>, id: u64, delete_file: bool) -> Result<(), String> {
    m.delete(id, delete_file).await
}

#[tauri::command]
async fn clear_all(m: Downloads<'_>) -> Result<(), String> {
    m.clear_all().await;
    Ok(())
}

#[tauri::command]
fn open_file(m: Downloads<'_>, id: u64) -> Result<(), String> {
    m.open_file(id)
}

#[tauri::command]
fn open_folder(m: Downloads<'_>, id: u64) -> Result<(), String> {
    m.open_folder(id)
}

#[tauri::command]
fn active_downloads(m: Downloads<'_>) -> Active {
    m.active()
}

/// Stops every download (resumable ones save their place), then exits.
#[tauri::command]
async fn quit(app: AppHandle, m: Downloads<'_>) -> Result<(), String> {
    m.quit().await;
    app.exit(0);
    Ok(())
}

/// Asks the window to confirm quitting while downloads run. True when it was asked.
fn confirm_quit(app: &AppHandle) -> bool {
    let active = app.state::<Arc<Manager>>().active();
    if active.running == 0 {
        return false;
    }
    let _ = app.emit("app:quit-requested", active);
    true
}

pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            let manager = Manager::new(store::path(&data), Arc::new(WindowEvents(app.handle().clone())));
            // Reads only the range maps of unfinished downloads; nothing starts by itself.
            tauri::async_runtime::block_on(manager.restore());
            app.manage(manager.clone());
            tauri::async_runtime::spawn(async move {
                let mut ticker = tokio::time::interval(manager::TICK);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    ticker.tick().await;
                    manager.tick();
                }
            });

            // The main window starts hidden (tauri.conf.json) so it never flashes white in dark mode.
            if let Some(window) = app.get_webview_window("main") {
                let theme = window.theme().unwrap_or(Theme::Light);
                window.set_background_color(Some(background(theme)))?;
                window.show()?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && confirm_quit(window.app_handle())
            {
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_downloads,
            default_folder,
            probe_url,
            add_download,
            resume_download,
            pause_download,
            delete_download,
            clear_all,
            open_file,
            open_folder,
            active_downloads,
            quit,
        ])
        .build(tauri::generate_context!())
        .expect("error while building KS Download Manager");

    app.run(|app, event| {
        // Cmd+Q / the app menu: same confirmation as closing the window. `code` is set
        // when we exit on purpose (the `quit` command), which must not be stopped.
        if let RunEvent::ExitRequested { code: None, api, .. } = event
            && confirm_quit(app)
        {
            api.prevent_exit();
        }
    });
}
