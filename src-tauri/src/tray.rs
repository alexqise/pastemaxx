//! Menu bar (tray) icon and menu.

use crate::{db, state::AppState, window};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Show Clipboard Bar", true, Some("Cmd+Shift+V"))?;
    let clear = MenuItem::with_id(app, "clear", "Clear History", true, None::<&str>)?;
    let shots_on = app
        .state::<AppState>()
        .capture_screenshots
        .load(std::sync::atomic::Ordering::SeqCst);
    let screenshots = CheckMenuItem::with_id(
        app,
        "screenshots",
        "Capture Screenshots",
        true,
        shots_on,
        None::<&str>,
    )?;
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let login = CheckMenuItem::with_id(
        app,
        "login",
        "Launch at Login",
        true,
        autostart_on,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit PasteMaxx", true, Some("Cmd+Q"))?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &PredefinedMenuItem::separator(app)?,
            &clear,
            &PredefinedMenuItem::separator(app)?,
            &screenshots,
            &login,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main-tray")
        .icon(tray_glyph())
        .icon_as_template(true)
        .tooltip("PasteMaxx")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "toggle" => window::toggle_bar(app),
            "clear" => {
                let state = app.state::<AppState>();
                let conn = state.db.lock().unwrap();
                let _ = db::clear_all(&conn);
                drop(conn);
                let _ = app.emit("items-changed", ());
            }
            "screenshots" => {
                let state = app.state::<AppState>();
                let now_on = !state
                    .capture_screenshots
                    .fetch_xor(true, std::sync::atomic::Ordering::SeqCst);
                let conn = state.db.lock().unwrap();
                db::set_setting(&conn, "capture_screenshots", if now_on { "1" } else { "0" });
                drop(conn);
                let _ = screenshots.set_checked(now_on);
            }
            "login" => {
                let autolaunch = app.autolaunch();
                let enabled = autolaunch.is_enabled().unwrap_or(false);
                let _ = if enabled {
                    autolaunch.disable()
                } else {
                    autolaunch.enable()
                };
                let _ = login.set_checked(!enabled);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Draw a simple clipboard glyph as a template image (alpha-only, macOS tints it).
fn tray_glyph() -> tauri::image::Image<'static> {
    const S: usize = 44;
    let mut rgba = vec![0u8; S * S * 4];
    let mut fill = |x0: usize, y0: usize, x1: usize, y1: usize, corner: usize, on: bool| {
        for y in y0..y1 {
            for x in x0..x1 {
                // knock out square corners for a rounded look
                let dx = if x < x0 + corner {
                    x0 + corner - x
                } else if x >= x1 - corner {
                    x + 1 - (x1 - corner)
                } else {
                    0
                };
                let dy = if y < y0 + corner {
                    y0 + corner - y
                } else if y >= y1 - corner {
                    y + 1 - (y1 - corner)
                } else {
                    0
                };
                if dx * dx + dy * dy > corner * corner {
                    continue;
                }
                let i = (y * S + x) * 4;
                let a = if on { 255 } else { 0 };
                rgba[i] = 0;
                rgba[i + 1] = 0;
                rgba[i + 2] = 0;
                rgba[i + 3] = a;
            }
        }
    };
    // clipboard board (outline via fill + punch-out)
    fill(8, 8, 36, 40, 6, true);
    fill(11, 11, 33, 37, 4, false);
    // top tab
    fill(16, 4, 28, 13, 3, true);
    // content lines
    fill(15, 18, 29, 21, 1, true);
    fill(15, 24, 29, 27, 1, true);
    fill(15, 30, 25, 33, 1, true);
    tauri::image::Image::new_owned(rgba, S as u32, S as u32)
}
