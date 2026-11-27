//! KS Download Manager desktop shell: the window, the commands the interface calls, and
//! the events it listens to. Downloading is done by the `engine` crate (src-tauri/engine);
//! the list and the running downloads are kept by [`manager::Manager`].

mod appupdate;
mod clipboard;
mod command;
mod component;
mod files;
mod handoff;
mod leftovers;
mod manager;
mod mark;
mod marker;
#[macro_use]
mod msg;
mod open;
mod os;
mod picker;
mod settings;
mod store;
#[cfg(test)]
mod testlinks;
mod video;

use std::sync::Arc;

use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager as _, RunEvent, State, Theme, WindowEvent};

use manager::{Active, Events, Manager, Pick, ProbeView, Tick};
use files::Category;
use settings::Settings;
use store::{Queue, Record};

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
    fn queues(&self, queues: &[Queue]) {
        let _ = self.0.emit("queues:changed", queues);
    }
}

type Downloads<'a> = State<'a, Arc<Manager>>;

#[tauri::command]
async fn list_downloads(m: Downloads<'_>) -> Result<Vec<Record>, msg::Msg> {
    m.wait_restored().await;
    Ok(m.list())
}

/// The clipboard's link for Add download, only when it holds exactly one web link and
/// the setting is on.
#[tauri::command]
async fn clipboard_link(m: Downloads<'_>) -> Result<Option<String>, msg::Msg> {
    // Off in Settings: the clipboard is not read at all.
    if !m.settings().clipboard_prefill {
        return Ok(None);
    }
    Ok(clipboard::link().await)
}

#[tauri::command]
fn default_folder(app: AppHandle) -> Result<String, msg::Msg> {
    let dir = app.path().download_dir().map_err(|e| msg!("error.noDownloadsFolder", detail = e))?;
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
async fn probe_url(m: Downloads<'_>, url: String) -> Result<ProbeView, msg::Msg> {
    m.probe(&url).await
}

#[tauri::command]
async fn add_download(m: Downloads<'_>, url: String, name: String, folder: String, start: bool) -> Result<Record, msg::Msg> {
    m.inner().add(&url, &name, &folder, start).await
}

#[tauri::command]
async fn add_playlist(m: Downloads<'_>, url: String, folder: String, picks: Vec<Pick>, number: bool, start: bool) -> Result<usize, msg::Msg> {
    m.inner().add_playlist(&url, &folder, &picks, number, start).await
}

#[tauri::command]
fn resume_playlist(m: Downloads<'_>, playlist: u64) -> Result<usize, msg::Msg> {
    m.inner().resume_playlist(playlist)
}

#[tauri::command]
fn pause_playlist(m: Downloads<'_>, playlist: u64) -> Result<usize, msg::Msg> {
    m.pause_playlist(playlist)
}

/// Raw image bytes (an `ArrayBuffer` in the page, which turns them into a data URL).
#[tauri::command]
async fn playlist_thumbnail(m: Downloads<'_>, url: String, index: u32) -> Result<tauri::ipc::Response, msg::Msg> {
    m.thumbnail(&url, index).await.map(tauri::ipc::Response::new)
}

#[tauri::command]
fn resume_download(m: Downloads<'_>, id: u64) -> Result<(), msg::Msg> {
    m.inner().resume(id)
}

#[tauri::command]
fn pause_download(m: Downloads<'_>, id: u64) -> Result<(), msg::Msg> {
    m.pause(id)
}

#[tauri::command]
async fn delete_download(m: Downloads<'_>, id: u64, delete_file: bool) -> Result<(), msg::Msg> {
    m.delete(id, delete_file).await
}

#[tauri::command]
async fn clear_all(m: Downloads<'_>) -> Result<(), msg::Msg> {
    m.clear_all().await;
    Ok(())
}

#[tauri::command]
fn open_file(m: Downloads<'_>, id: u64) -> Result<(), msg::Msg> {
    m.open_file(id)
}

#[tauri::command]
fn open_folder(m: Downloads<'_>, id: u64) -> Result<(), msg::Msg> {
    m.open_folder(id)
}

#[tauri::command]
fn active_downloads(m: Downloads<'_>) -> Active {
    m.active()
}

#[tauri::command]
fn get_settings(m: Downloads<'_>) -> Settings {
    m.settings()
}

#[tauri::command]
fn set_settings(m: Downloads<'_>, settings: Settings) -> Result<Settings, msg::Msg> {
    m.inner().set_settings(settings)
}

/// The save folder the Add dialog starts with for a category.
#[tauri::command]
fn accept_notice(m: Downloads<'_>) -> Result<(), msg::Msg> {
    m.accept_notice()
}

#[tauri::command]
fn category_folder(m: Downloads<'_>, category: Category) -> Option<String> {
    m.folder_for(category)
}

/// The system folder picker; `None` when cancelled. `start` is only where it opens.
#[tauri::command]
async fn pick_folder(prompt: String, start: Option<String>) -> Result<Option<String>, msg::Msg> {
    picker::pick_folder(&prompt, start.as_deref().map(std::path::Path::new)).await
}

#[tauri::command]
fn list_queues(m: Downloads<'_>) -> Vec<Queue> {
    m.queues()
}

#[tauri::command]
fn create_queue(m: Downloads<'_>, name: String) -> Result<Queue, msg::Msg> {
    m.create_queue(&name)
}

#[tauri::command]
fn rename_queue(m: Downloads<'_>, id: u64, name: String) -> Result<(), msg::Msg> {
    m.rename_queue(id, &name)
}

#[tauri::command]
fn delete_queue(m: Downloads<'_>, id: u64) -> Result<(), msg::Msg> {
    m.delete_queue(id)
}

#[tauri::command]
fn move_to_queue(m: Downloads<'_>, id: u64, queue: Option<u64>) -> Result<(), msg::Msg> {
    m.move_to_queue(id, queue)
}

#[tauri::command]
fn start_queue(m: Downloads<'_>, queue: Option<u64>) -> Result<(), msg::Msg> {
    m.inner().start_queue(queue)
}

/// Returns how many downloads kept running because they cannot be paused.
#[tauri::command]
fn stop_queue(m: Downloads<'_>, queue: Option<u64>) -> Result<usize, msg::Msg> {
    m.stop_queue(queue)
}

/// The last scan for leftover download files, or a new one (`rescan`, or none yet).
#[tauri::command]
async fn leftover_files(m: Downloads<'_>, rescan: bool) -> Result<Vec<leftovers::Item>, msg::Msg> {
    match m.leftovers() {
        Some(items) if !rescan => Ok(items),
        _ => Ok(m.scan_leftovers().await),
    }
}

/// Removes the leftovers of the last scan the user ticked; the interface asked first.
#[tauri::command]
async fn remove_leftover_files(m: Downloads<'_>, paths: Vec<String>) -> Result<(usize, usize), msg::Msg> {
    Ok(m.remove_leftovers(&paths).await)
}

/// "Update video component": installs the newest official yt-dlp, on request only.
#[tauri::command]
async fn update_video_component(m: Downloads<'_>) -> Result<component::Update, msg::Msg> {
    let tools = m.video_tools()?;
    component::update(&tools).await
}

type Updates<'a> = State<'a, Arc<appupdate::Updater>>;

/// The running version, as the release build stamped it ("0.0.0" when built from source).
#[tauri::command]
fn app_version(u: Updates<'_>) -> String {
    u.current().to_owned()
}

/// The update found by the last check (start-up or a click), if any.
#[tauri::command]
fn update_status(u: Updates<'_>) -> Option<appupdate::Available> {
    u.found()
}

/// "Check for updates": asks GitHub now.
#[tauri::command]
async fn check_for_update(app: AppHandle, u: Updates<'_>) -> Result<appupdate::Check, msg::Msg> {
    let r = u.check().await?;
    if let appupdate::Check::Available { update } = &r {
        let _ = app.emit("update:available", update);
    }
    Ok(r)
}

/// Downloads the installer found, checks it and opens it.
#[tauri::command]
async fn install_update(u: Updates<'_>) -> Result<appupdate::Opened, msg::Msg> {
    u.install().await
}

/// Where the bundled programs are: deno, ffmpeg and ffprobe next to the app executable
/// (Tauri `externalBin`), yt-dlp's folder in the resources.
fn video_tools(app: &tauri::App, data: &std::path::Path) -> Option<video::Tools> {
    let exe = std::env::current_exe().ok()?;
    let programs = exe.parent()?;
    let resources = app.path().resource_dir().ok()?;
    Some(video::Tools::new(programs, &resources, data))
}

/// The menu bar, with labels in the user's language (by item id; desktop/mac's
/// `MENU_ITEMS`). macOS only: Windows apps of this kind have no menu bar, so it does nothing
/// there. Called at start-up and whenever the language changes.
#[tauri::command]
fn set_app_menu(app: AppHandle, labels: std::collections::HashMap<String, String>) -> Result<(), msg::Msg> {
    #[cfg(target_os = "macos")]
    {
        let menu = os::app_menu(&app, &labels).map_err(|e| msg!("error.menu", detail = e))?;
        app.set_menu(menu).map_err(|e| msg!("error.menu", detail = e))?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, labels);
    Ok(())
}

/// A system notification (a download finished or failed), with the text in the user's
/// language from the interface. Nothing when switched off in Settings.
#[tauri::command]
fn notify(app: AppHandle, m: Downloads<'_>, title: String, body: String) -> Result<(), msg::Msg> {
    use tauri_plugin_notification::NotificationExt;
    if !m.settings().notify {
        return Ok(());
    }
    let title: String = title.chars().take(120).collect();
    let body: String = body.chars().take(300).collect();
    app.notification().builder().title(title).body(body).show().map_err(|e| msg!("error.notify", detail = e))
}

/// Help ▸ KS Download Manager Help: the README on GitHub, in the default browser. A fixed
/// address only; the interface cannot pass one.
#[tauri::command]
fn open_help() -> Result<(), msg::Msg> {
    const HELP: &str = "https://github.com/kumasumit/ks-download-manager#readme";
    open::open_url(HELP).map_err(|e| msg!("error.openFile", detail = e))
}

/// Progress of the running downloads on the Dock icon (macOS) or taskbar button (Windows),
/// and on macOS their number as the Dock badge; both cleared when none run.
fn show_progress(app: &AppHandle, running: usize, bar: Bar) {
    use tauri::window::{ProgressBarState, ProgressBarStatus};
    let Some(window) = app.get_webview_window("main") else { return };
    let (status, progress) = match bar {
        Bar::None => (ProgressBarStatus::None, None),
        Bar::Normal(p) => (ProgressBarStatus::Normal, Some(p)),
        Bar::Indeterminate => (ProgressBarStatus::Indeterminate, None),
        Bar::Paused(p) => (ProgressBarStatus::Paused, p),
        Bar::Error(p) => (ProgressBarStatus::Error, p),
    };
    let _ = window.set_progress_bar(ProgressBarState { status: Some(status), progress });
    #[cfg(target_os = "macos")]
    let _ = window.set_badge_count((running > 0).then_some(running as i64));
    // Windows: the number running as an overlay on the taskbar button.
    #[cfg(windows)]
    let _ = window.set_overlay_icon(os::count_badge(running).map(|(rgba, w, h)| tauri::image::Image::new_owned(rgba, w, h)));
}

/// What the Dock's or taskbar button's progress bar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bar {
    None,
    Normal(u64),
    /// Running, but no running download knows its size.
    Indeterminate,
    /// Every running download is retrying a server that stopped answering (yellow on Windows).
    Paused(Option<u64>),
    /// A download failed and the window has not had the focus since (red on Windows).
    Error(Option<u64>),
}

/// The bar for these totals. `unseen_failure`: a download failed since the window last had
/// the focus; the bar stays red until it is looked at.
/// `paused`: whether retrying downloads get their own state.
fn bar(totals: &manager::Totals, unseen_failure: bool, paused: bool) -> Bar {
    let percent = totals.percent();
    match totals.running {
        _ if unseen_failure => Bar::Error(percent.or(Some(100))),
        0 => Bar::None,
        n if paused && totals.stalled == n => Bar::Paused(percent),
        _ => percent.map_or(Bar::Indeterminate, Bar::Normal),
    }
}

/// Stops every download (resumable ones save their place), then exits.
#[tauri::command]
async fn quit(app: AppHandle, m: Downloads<'_>) -> Result<(), msg::Msg> {
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
                handoff::Request::Ping { .. } => {
                    let mut r = handoff::Reply::ok(None);
                    r.update = self.0.state::<Arc<appupdate::Updater>>().extension_latest();
                    r
                }
                handoff::Request::Show { .. } => {
                    if let Some(w) = self.0.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    handoff::Reply::ok(None)
                }
                handoff::Request::Add { kind, url, headers, .. } => {
                    let m = self.0.state::<Arc<Manager>>().inner().clone();
                    m.add_from_extension(kind, &url, headers).await
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
    // Windows installer: register the Chrome host for this copy, then exit (desktop/windows/installer/hooks.nsh).
    #[cfg(windows)]
    if args.get(1).map(String::as_str) == Some("--register-chrome-host") {
        let done = std::env::current_exe().and_then(|exe| handoff::install_manifest(&exe));
        std::process::exit(if done.is_ok() { 0 } else { 1 });
    }
    // The extension's socket is also the single-instance lock: a second copy shows the
    // first one's window and exits.
    let socket = handoff::data_dir_without_tauri().map(|d| handoff::socket_path(&d));
    // Windows: the pipe is registered with Tauri's own Tokio runtime, which later serves it.
    #[cfg(windows)]
    let runtime = tauri::async_runtime::handle();
    #[cfg(windows)]
    let entered = runtime.inner().enter();
    let bound = socket.as_deref().map(handoff::bind);
    #[cfg(windows)]
    drop(entered);
    let listener = match bound {
        Some(Ok(handoff::Bound::AlreadyRunning)) => {
            if let Some(s) = &socket {
                handoff::ask_to_show(s);
            }
            return;
        }
        #[cfg(any(unix, windows))]
        Some(Ok(handoff::Bound::Listener(l))) => Some(l),
        Some(Err(e)) => {
            eprintln!("the Chrome extension cannot reach the app: {e}");
            None
        }
        None => None,
    };

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let data = app.path().app_data_dir()?;
            let manager = Manager::new(store::path(&data), Arc::new(WindowEvents(app.handle().clone())));
            if let Some(tools) = video_tools(app, &data) {
                manager.set_video_tools(Arc::new(tools));
            }
            if let Ok(d) = app.path().download_dir() {
                manager.set_default_folder(d);
            }
            app.manage(manager.clone());
            let updater = Arc::new(appupdate::Updater::new(app.package_info().version.to_string(), &data));
            app.manage(updater.clone());
            // The daily update check (on unless switched off in Settings), a little after start-up
            // and then hourly while the app runs.
            tauri::async_runtime::spawn({
                let manager = manager.clone();
                let handle = app.handle().clone();
                async move {
                    // Asks again every hour while the app runs; `due` keeps it to once a day.
                    tokio::time::sleep(ks_utility::app_update::STARTUP_DELAY).await;
                    loop {
                        if manager.settings().update_check && updater.due() {
                            match updater.check().await {
                                Ok(appupdate::Check::Available { update }) => {
                                    let _ = handle.emit("update:available", update);
                                }
                                Ok(_) => {}
                                Err(e) => eprintln!("the update check failed: {e}"),
                            }
                        }
                        tokio::time::sleep(ks_utility::app_update::RECHECK_EVERY).await;
                    }
                }
            });
            // Reads the range maps of unfinished downloads after the window is up (Part 7):
            // the list is sent once this is done, and nothing starts before it.
            tauri::async_runtime::spawn({
                let manager = manager.clone();
                async move {
                    let restore = tauri::async_runtime::spawn({
                        let manager = manager.clone();
                        async move { manager.restore().await }
                    });
                    // A panic in the restore must not leave the window empty: the list is
                    // then shown as it was loaded.
                    if let Err(e) = restore.await {
                        eprintln!("checking the saved downloads failed: {e}");
                        manager.restore_done();
                    }
                }
            });
            // Leftover download files: looked for after start-up, never removed by itself.
            tauri::async_runtime::spawn({
                let manager = manager.clone();
                async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    manager.scan_leftovers().await;
                }
            });
            #[cfg(any(unix, windows))]
            if let (Some(listener), Some(path)) = (listener, socket.clone()) {
                let handler = Arc::new(ExtensionRequests(app.handle().clone()));
                tauri::async_runtime::spawn(handoff::serve(listener, path, handler));
            }
            if let Ok(exe) = std::env::current_exe()
                && let Err(e) = handoff::install_manifest(&exe)
            {
                eprintln!("could not register with Chrome: {e}");
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut ticker = tokio::time::interval(manager::TICK);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                // What the Dock or taskbar shows now; it is only updated when this changes.
                let mut shown: Option<(usize, Bar)> = None;
                // Failures the user has seen: those before the window last had the focus.
                let mut seen = 0;
                loop {
                    ticker.tick().await;
                    let totals = manager.tick();
                    // No idle sleep while downloads run, unless switched off in Settings.
                    os::set_keep_awake(totals.running > 0 && manager.settings().keep_awake);
                    if handle.get_webview_window("main").is_some_and(|w| w.is_focused().unwrap_or(false)) {
                        seen = totals.failures;
                    }
                    // The red and yellow states are the taskbar's (Windows); the Dock shows plain progress.
                    let now = (totals.running, bar(&totals, cfg!(windows) && totals.failures > seen, cfg!(windows)));
                    if shown != Some(now) {
                        show_progress(&handle, now.0, now.1);
                        shown = Some(now);
                    }
                }
            });

            // The main window starts hidden (tauri.conf.json) so it never flashes white in dark mode.
            if let Some(window) = app.get_webview_window("main") {
                let theme = window.theme().unwrap_or(Theme::Light);
                window.set_background_color(Some(background(theme)))?;
                // Windows 11: the Mica material in the title bar, as Windows' own apps have.
                // Windows 10 has no Mica and refuses it; the title bar then stays as it was.
                #[cfg(windows)]
                let _ = window.set_effects(tauri::window::EffectsBuilder::new().effect(tauri::window::Effect::Mica).build());
                window.show()?;
            }
            Ok(())
        })
        // A menu item with an id: the interface does what the toolbar or shortcut would.
        .on_menu_event(|app, event| {
            let _ = app.emit("menu", event.id().as_ref());
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
            clipboard_link,
            default_folder,
            probe_url,
            add_download,
            add_playlist,
            resume_playlist,
            pause_playlist,
            playlist_thumbnail,
            resume_download,
            pause_download,
            delete_download,
            clear_all,
            open_file,
            open_folder,
            active_downloads,
            update_video_component,
            app_version,
            update_status,
            check_for_update,
            install_update,
            set_app_menu,
            open_help,
            notify,
            get_settings,
            set_settings,
            category_folder,
            accept_notice,
            pick_folder,
            list_queues,
            create_queue,
            rename_queue,
            delete_queue,
            move_to_queue,
            start_queue,
            stop_queue,
            leftover_files,
            remove_leftover_files,
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

#[cfg(test)]
mod bar_tests {
    use super::{Bar, bar};
    use crate::manager::Totals;

    #[test]
    fn the_progress_bar_follows_the_downloads() {
        let t = |running, received, total, stalled| Totals { running, received, total, stalled, failures: 0 };
        assert_eq!(bar(&t(0, 0, 0, 0), false, true), Bar::None);
        assert_eq!(bar(&t(2, 50, 200, 0), false, true), Bar::Normal(25));
        assert_eq!(bar(&t(1, 50, 0, 0), false, true), Bar::Indeterminate, "no size known");
        // Every running download retrying: yellow; only some: still normal.
        assert_eq!(bar(&t(2, 50, 200, 2), false, true), Bar::Paused(Some(25)));
        assert_eq!(bar(&t(2, 50, 200, 1), false, true), Bar::Normal(25));
        assert_eq!(bar(&t(2, 50, 200, 2), false, false), Bar::Normal(25), "no paused state on this system");
        // A failure not yet seen: red, with the others' progress, or full when none run.
        assert_eq!(bar(&t(1, 50, 200, 0), true, true), Bar::Error(Some(25)));
        assert_eq!(bar(&t(0, 0, 0, 0), true, true), Bar::Error(Some(100)));
    }
}
