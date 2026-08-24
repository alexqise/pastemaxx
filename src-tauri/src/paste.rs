//! Write an item back to the pasteboard and auto-paste it into the previous app.

use crate::db;
use crate::macos;
use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Brief settle time between the pasteboard write and the synthesized Cmd+V.
const PASTE_DELAY: Duration = Duration::from_millis(60);

pub fn select_item(app: &AppHandle, id: i64) -> Result<(), String> {
    let state = app.state::<AppState>();

    let item = {
        let conn = state.db.lock().unwrap();
        db::get_full(&conn, id).map_err(|e| e.to_string())?
    }
    .ok_or("item not found")?;

    // Mark the paste in flight so the blur handler keeps the bar open.
    *state.last_paste.lock().unwrap() = Some(Instant::now());

    let new_count = match item.kind.as_str() {
        "image" => {
            let path = item.image_path.ok_or("image file missing")?;
            let png = std::fs::read(&path).map_err(|e| e.to_string())?;
            macos::write_image_png(&png)
        }
        "files" => {
            let paths = item.file_paths.ok_or("file paths missing")?;
            macos::write_files(&paths)
        }
        _ => {
            let plain = item.plain_text.unwrap_or_default();
            macos::write_text(&plain, item.rtf.as_deref(), item.html.as_deref())
        }
    };
    state.self_change.store(new_count, Ordering::SeqCst);

    // The item is now the live clipboard content — bump it to the top.
    {
        let conn = state.db.lock().unwrap();
        let _ = db::bump(&conn, id);
    }
    let _ = app.emit("items-changed", ());

    // The bar never activates our app, so the target is still frontmost —
    // paste straight into it by pid. The bar keeps keyboard focus for the next pick.
    let pid = state.prev_app_pid.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        std::thread::sleep(PASTE_DELAY);
        if macos::ax_trusted(false) {
            macos::send_cmd_v((pid > 0).then_some(pid));
        } else {
            // No Accessibility permission: content is on the clipboard, ask for it for next time.
            macos::ax_trusted(true);
        }
    });
    Ok(())
}
