mod clipboard;
mod commands;
mod db;
mod macos;
mod paste;
mod screenshots;
mod state;
mod tray;
mod window;

use state::AppState;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64};
use std::sync::Mutex;
use tauri::Manager;
use tauri_nspanel::{
    tauri_panel, CollectionBehavior, PanelLevel, StyleMask, WebviewWindowExt as PanelWindowExt,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};

// The bar is a non-activating NSPanel: it can take keyboard input without
// activating the app, so summoning it never switches Spaces / leaves fullscreen.
tauri_panel! {
    panel!(BarPanel {
        config: {
            can_become_key_window: true,
            is_floating_panel: true
        }
    })

    panel_event!(BarPanelEventHandler {
        window_did_resign_key(notification: &NSNotification) -> ()
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::list_items,
            commands::select_item,
            commands::toggle_pin,
            commands::delete_item,
            commands::clear_all,
            commands::hide_bar,
            commands::ax_trusted,
        ])
        .setup(|app| {
            // Menu-bar-only background agent: no Dock icon.
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Storage: SQLite for metadata, files on disk for images and app icons.
            let data_dir = app.path().app_data_dir()?;
            let images_dir = data_dir.join("images");
            let icons_dir = data_dir.join("icons");
            std::fs::create_dir_all(&images_dir)?;
            std::fs::create_dir_all(&icons_dir)?;
            let conn = db::init(&data_dir.join("pastemaxx.db"))?;

            // Enable launch-at-login once on first run; the tray checkbox controls it after.
            if db::get_setting(&conn, "autostart_initialized").is_none() {
                let _ = app.autolaunch().enable();
                db::set_setting(&conn, "autostart_initialized", "1");
            }

            let capture_screenshots =
                db::get_setting(&conn, "capture_screenshots").as_deref() != Some("0");
            app.manage(AppState {
                db: Mutex::new(conn),
                images_dir,
                icons_dir,
                prev_app_pid: AtomicI32::new(0),
                self_change: AtomicI64::new(-1),
                last_change: AtomicI64::new(-1),
                last_paste: Mutex::new(None),
                hiding: AtomicBool::new(false),
                bar_origin: Mutex::new(None),
                capture_screenshots: AtomicBool::new(capture_screenshots),
            });

            // Liquid glass: native vibrancy under the transparent webview.
            let win = app
                .get_webview_window(window::BAR_LABEL)
                .expect("bar window missing");
            apply_vibrancy(
                &win,
                NSVisualEffectMaterial::Popover,
                Some(NSVisualEffectState::Active),
                Some(26.0),
            )
            .expect("failed to apply vibrancy");

            // Convert to a non-activating panel that joins the active Space,
            // including fullscreen apps.
            let panel = win.to_panel::<BarPanel>().expect("failed to convert to panel");
            panel.set_level(PanelLevel::Floating.value());
            panel.set_style_mask(StyleMask::empty().nonactivating_panel().into());
            panel.set_collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary()
                    .into(),
            );
            // Click-outside: hide when the panel loses key status.
            let handler = BarPanelEventHandler::new();
            let blur_handle = app.handle().clone();
            handler.window_did_resign_key(move |_notification| {
                window::on_blur(&blur_handle);
            });
            panel.set_event_handler(Some(handler.as_ref()));

            // Global hotkey: Cmd+Shift+V toggles the bar.
            let hotkey: Shortcut = "cmd+shift+v".parse().unwrap();
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_shortcuts([hotkey])?
                    .with_handler(move |app, shortcut, event| {
                        if shortcut == &hotkey && event.state() == ShortcutState::Pressed {
                            window::toggle_bar(app);
                        }
                    })
                    .build(),
            )?;

            tray::create(app.handle())?;
            clipboard::spawn_watcher(app.handle().clone());
            screenshots::spawn_watcher(app.handle().clone());

            // Auto-paste needs Accessibility; show the system prompt once shortly after launch.
            if !macos::ax_trusted(false) {
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(1200));
                    macos::ax_trusted(true);
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running PasteMaxx");
}
