//! KS Download Manager desktop shell: the window, the commands the interface calls, and
//! the events it listens to. Downloading is done by the `engine` crate (src-tauri/engine);
//! the list and the running downloads are kept by [`manager::Manager`].

mod component;
mod files;
mod handoff;
mod manager;
mod open;
mod store;
mod video;

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

/// "Update video component": installs the newest official yt-dlp, on request only.
#[tauri::command]
async fn update_video_component(m: Downloads<'_>) -> Result<component::Update, String> {
    let tools = m.video_tools()?;
    component::update(&tools).await
}

/// Where the bundled programs are: deno, ffmpeg and ffprobe next to the app executable
/// (Tauri `externalBin`), yt-dlp's folder in the resources.
fn video_tools(app: &tauri::App, data: &std::path::Path) -> Option<video::Tools> {
    let exe = std::env::current_exe().ok()?;
    let programs = exe.parent()?;
    let resources = app.path().resource_dir().ok()?;
    Some(video::Tools::new(programs, &resources, data))
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

/// Answers requests from the Chrome extension's native host (src/handoff.rs).
struct ExtensionRequests(AppHandle);

impl handoff::Handler for ExtensionRequests {
    fn handle(&self, req: handoff::Request) -> std::pin::Pin<Box<dyn std::future::Future<Output = handoff::Reply> + Send + '_>> {
        Box::pin(async move {
            match req {
                handoff::Request::Ping { .. } => handoff::Reply::ok(None),
                handoff::Request::Show { .. } => {
                    if let Some(w) = self.0.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    handoff::Reply::ok(None)
                }
                handoff::Request::Add { kind, url, .. } => {
                    let folder = match self.0.path().download_dir() {
                        Ok(d) => d.to_string_lossy().into_owned(),
                        Err(_) => return handoff::Reply::err("add_failed", "No downloads folder."),
                    };
                    let m = self.0.state::<Arc<Manager>>().inner().clone();
                    m.add_from_extension(kind, &url, &folder).await
                }
            }
        })
    }
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    // Started by Chrome as the extension's native messaging host: no window, just relay.
    if handoff::is_host_invocation(&args) {
        let socket = handoff::data_dir_without_tauri().map(|d| handoff::socket_path(&d));
        std::process::exit(socket.map_or(1, |s| handoff::run_host(&args, &s)));
    }
    // The extension's socket is also the single-instance lock: a second copy shows the
    // first one's window and exits.
    let socket = handoff::data_dir_without_tauri().map(|d| handoff::socket_path(&d));
    let listener = match socket.as_deref().map(handoff::bind) {
        Some(Ok(handoff::Bound::AlreadyRunning)) => {
            if let Some(s) = &socket {
                handoff::ask_to_show(s);
            }
            return;
        }
        #[cfg(unix)]
        Some(Ok(handoff::Bound::Listener(l))) => Some(l),
        Some(Err(e)) => {
            eprintln!("the Chrome extension cannot reach the app: {e}");
            None
        }
        None => None,
    };

    let app = tauri::Builder::default()
        .setup(move |app| {
            let data = app.path().app_data_dir()?;
            let manager = Manager::new(store::path(&data), Arc::new(WindowEvents(app.handle().clone())));
            if let Some(tools) = video_tools(app, &data) {
                manager.set_video_tools(Arc::new(tools));
            }
            // Reads only the range maps of unfinished downloads; nothing starts by itself.
            tauri::async_runtime::block_on(manager.restore());
            app.manage(manager.clone());
            #[cfg(unix)]
            if let (Some(listener), Some(path)) = (listener, socket.clone()) {
                let handler = Arc::new(ExtensionRequests(app.handle().clone()));
                tauri::async_runtime::spawn(handoff::serve(listener, path, handler));
            }
            if let Ok(exe) = std::env::current_exe()
                && let Err(e) = handoff::install_manifest(&exe)
            {
                eprintln!("could not register with Chrome: {e}");
            }
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
            update_video_component,
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
