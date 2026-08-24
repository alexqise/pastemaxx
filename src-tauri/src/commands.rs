use crate::db::{self, ItemDto};
use crate::state::AppState;
use crate::{macos, paste, window};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn list_items(state: State<AppState>, query: Option<String>) -> Result<Vec<ItemDto>, String> {
    let conn = state.db.lock().unwrap();
    db::list(&conn, query.as_deref().unwrap_or("").trim()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn select_item(app: AppHandle, id: i64) -> Result<(), String> {
    paste::select_item(&app, id)
}

#[tauri::command]
pub fn toggle_pin(app: AppHandle, state: State<AppState>, id: i64) -> Result<bool, String> {
    let pinned = {
        let conn = state.db.lock().unwrap();
        db::toggle_pin(&conn, id).map_err(|e| e.to_string())?
    };
    let _ = app.emit("items-changed", ());
    Ok(pinned)
}

#[tauri::command]
pub fn delete_item(app: AppHandle, state: State<AppState>, id: i64) -> Result<(), String> {
    {
        let conn = state.db.lock().unwrap();
        db::delete(&conn, id).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("items-changed", ());
    Ok(())
}

#[tauri::command]
pub fn clear_all(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    {
        let conn = state.db.lock().unwrap();
        db::clear_all(&conn).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("items-changed", ());
    Ok(())
}

#[tauri::command]
pub fn hide_bar(app: AppHandle) {
    window::hide_bar(&app);
}

#[tauri::command]
pub fn ax_trusted() -> bool {
    macos::ax_trusted(false)
}
