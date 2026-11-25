//! The macOS menu bar, to the platform's conventions: the app menu (About, Settings ⌘,,
//! Check for Updates, Services, Hide, Quit), File, Edit, Downloads, View, Window and Help.
//! The labels come from the interface in the user's language (`labels`, by item id); a
//! missing label falls back to its id. Items with an id send it back to the interface as a
//! `menu` event; the standard ones (Edit, Window, Hide, Quit) are handled by macOS.

use std::collections::HashMap;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Runtime};

/// The ids the interface handles, in menu order, with their shortcuts.
pub const ITEMS: &[(&str, Option<&str>)] = &[
    ("settings", Some("CmdOrCtrl+,")),
    ("checkUpdates", None),
    ("add", Some("CmdOrCtrl+N")),
    ("open", Some("CmdOrCtrl+O")),
    ("reveal", Some("CmdOrCtrl+R")),
    ("delete", Some("CmdOrCtrl+Backspace")),
    ("resume", None),
    ("pause", None),
    ("clearAll", None),
    ("queues", None),
    ("view.all", Some("CmdOrCtrl+1")),
    ("view.unfinished", Some("CmdOrCtrl+2")),
    ("view.finished", Some("CmdOrCtrl+3")),
    ("view.video", Some("CmdOrCtrl+4")),
    ("view.audio", Some("CmdOrCtrl+5")),
    ("view.images", Some("CmdOrCtrl+6")),
    ("view.other", Some("CmdOrCtrl+7")),
    ("find", Some("CmdOrCtrl+F")),
    ("help", None),
];

/// Builds the menu bar. Call it again (and set it again) when the language changes.
pub fn app_menu<R: Runtime>(app: &AppHandle<R>, labels: &HashMap<String, String>) -> tauri::Result<Menu<R>> {
    let l = |k: &str| labels.get(k).cloned().unwrap_or_else(|| k.to_owned());
    let item = |id: &str| -> tauri::Result<MenuItem<R>> {
        let accel = ITEMS.iter().find(|(i, _)| *i == id).and_then(|(_, a)| *a);
        MenuItem::with_id(app, id, l(id), true, accel)
    };
    let sep = || PredefinedMenuItem::separator(app);
    let name = app.package_info().name.clone();

    let app_menu = Submenu::with_items(
        app,
        &name,
        true,
        &[
            &PredefinedMenuItem::about(app, Some(&l("about")), None)?,
            &sep()?,
            &item("settings")?,
            &item("checkUpdates")?,
            &sep()?,
            &PredefinedMenuItem::services(app, Some(&l("services")))?,
            &sep()?,
            &PredefinedMenuItem::hide(app, Some(&l("hide")))?,
            &PredefinedMenuItem::hide_others(app, Some(&l("hideOthers")))?,
            &PredefinedMenuItem::show_all(app, Some(&l("showAll")))?,
            &sep()?,
            &PredefinedMenuItem::quit(app, Some(&l("quit")))?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        l("menu.file"),
        true,
        &[
            &item("add")?,
            &sep()?,
            &item("open")?,
            &item("reveal")?,
            &sep()?,
            &item("delete")?,
            &sep()?,
            &PredefinedMenuItem::close_window(app, Some(&l("closeWindow")))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        l("menu.edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(&l("undo")))?,
            &PredefinedMenuItem::redo(app, Some(&l("redo")))?,
            &sep()?,
            &PredefinedMenuItem::cut(app, Some(&l("cut")))?,
            &PredefinedMenuItem::copy(app, Some(&l("copy")))?,
            &PredefinedMenuItem::paste(app, Some(&l("paste")))?,
            &PredefinedMenuItem::select_all(app, Some(&l("selectAll")))?,
        ],
    )?;
    let downloads = Submenu::with_items(
        app,
        l("menu.downloads"),
        true,
        &[&item("resume")?, &item("pause")?, &sep()?, &item("queues")?, &sep()?, &item("clearAll")?],
    )?;
    let view = Submenu::with_items(
        app,
        l("menu.view"),
        true,
        &[
            &item("view.all")?,
            &item("view.unfinished")?,
            &item("view.finished")?,
            &sep()?,
            &item("view.video")?,
            &item("view.audio")?,
            &item("view.images")?,
            &item("view.other")?,
            &sep()?,
            &item("find")?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        l("menu.window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(&l("minimize")))?,
            &PredefinedMenuItem::maximize(app, Some(&l("zoom")))?,
            &sep()?,
            &PredefinedMenuItem::fullscreen(app, Some(&l("fullscreen")))?,
        ],
    )?;
    let help = Submenu::with_items(app, l("menu.help"), true, &[&item("help")?])?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &downloads, &view, &window, &help])
}
