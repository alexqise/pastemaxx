//! Clipboard watcher: polls NSPasteboard.changeCount and captures new content.

use crate::db::{self, NewItem};
use crate::macos::{self, Captured};
use crate::state::AppState;
use sha2::{Digest, Sha256};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const POLL_INTERVAL: Duration = Duration::from_millis(200);
const THUMB_MAX_DIM: u32 = 480;

pub fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        // Ignore whatever is already on the clipboard at launch.
        {
            let state = app.state::<AppState>();
            state.last_change.store(macos::change_count(), Ordering::SeqCst);
        }
        loop {
            std::thread::sleep(POLL_INTERVAL);
            objc2::rc::autoreleasepool(|_| {
                poll_once(&app);
            });
        }
    });
}

fn poll_once(app: &AppHandle) {
    let state = app.state::<AppState>();
    let count = macos::change_count();
    if count == state.last_change.load(Ordering::SeqCst) {
        return;
    }
    if count == state.self_change.load(Ordering::SeqCst) {
        state.last_change.store(count, Ordering::SeqCst);
        return; // our own paste-back write
    }

    let Some(captured) = macos::read_capture() else {
        // A copier may have cleared the pasteboard but not written content yet
        // (changeCount bumps on clearContents) — leave last_change alone and retry.
        return;
    };
    state.last_change.store(count, Ordering::SeqCst);

    let (source_app, source_bundle_id, source_icon_path) = capture_source(&state);

    let item = match build_item(&state, captured, source_app, source_bundle_id, source_icon_path) {
        Some(item) => item,
        None => return,
    };

    let ok = {
        let conn = state.db.lock().unwrap();
        db::insert_or_bump(&conn, item).is_ok()
    };
    if ok {
        let _ = app.emit("items-changed", ());
    }
}

fn capture_source(state: &AppState) -> (Option<String>, Option<String>, Option<String>) {
    let Some((_, name, bundle_id)) = macos::frontmost_app() else {
        return (None, None, None);
    };
    let icon_path = bundle_id.as_ref().and_then(|bid| {
        let path = state.icons_dir.join(format!("{bid}.png"));
        if !path.exists() {
            let png = macos::frontmost_app_icon_png()?;
            let small = downscale_png(&png, 64).unwrap_or(png);
            std::fs::write(&path, small).ok()?;
        }
        Some(path.to_string_lossy().to_string())
    });
    (name, bundle_id, icon_path)
}

fn build_item(
    state: &AppState,
    captured: Captured,
    source_app: Option<String>,
    source_bundle_id: Option<String>,
    source_icon_path: Option<String>,
) -> Option<NewItem<'static>> {
    match captured {
        Captured::Text { plain, rtf, html } => {
            if plain.trim().is_empty() {
                return None;
            }
            let mut hasher = Sha256::new();
            hasher.update(plain.as_bytes());
            let hash = hex(&hasher.finalize());
            let byte_size = (plain.len()
                + rtf.as_ref().map(|r| r.len()).unwrap_or(0)
                + html.as_ref().map(|h| h.len()).unwrap_or(0)) as i64;
            Some(NewItem {
                kind: "text",
                hash,
                plain_text: Some(plain),
                rtf,
                html,
                image_path: None,
                thumb_path: None,
                file_paths: None,
                byte_size,
                source_app,
                source_bundle_id,
                source_icon_path,
            })
        }
        Captured::Image { png } => {
            image_item_from_bytes(state, png, source_app, source_bundle_id, source_icon_path)
        }
        Captured::Files { paths } => {
            let joined = paths.join("\n");
            let mut hasher = Sha256::new();
            hasher.update(joined.as_bytes());
            let hash = hex(&hasher.finalize());
            let byte_size = joined.len() as i64;
            Some(NewItem {
                kind: "files",
                hash,
                // file names go into plain_text so type-to-filter finds them
                plain_text: Some(joined),
                rtf: None,
                html: None,
                image_path: None,
                thumb_path: None,
                file_paths: Some(paths),
                byte_size,
                source_app,
                source_bundle_id,
                source_icon_path,
            })
        }
    }
}

/// Build an image history item from raw PNG bytes (used by the clipboard
/// watcher and the screenshot watcher).
pub fn image_item_from_bytes(
    state: &AppState,
    png: Vec<u8>,
    source_app: Option<String>,
    source_bundle_id: Option<String>,
    source_icon_path: Option<String>,
) -> Option<NewItem<'static>> {
    let mut hasher = Sha256::new();
    hasher.update(&png);
    let hash = hex(&hasher.finalize());
    let image_path = state.images_dir.join(format!("{hash}.png"));
    let thumb_path = state.images_dir.join(format!("{hash}.thumb.png"));
    let mut byte_size = png.len() as i64;
    if !image_path.exists() {
        std::fs::write(&image_path, &png).ok()?;
        if let Some(thumb) = downscale_png(&png, THUMB_MAX_DIM) {
            byte_size += thumb.len() as i64;
            let _ = std::fs::write(&thumb_path, thumb);
        }
    }
    Some(NewItem {
        kind: "image",
        hash,
        plain_text: None,
        rtf: None,
        html: None,
        image_path: Some(image_path.to_string_lossy().to_string()),
        thumb_path: thumb_path
            .exists()
            .then(|| thumb_path.to_string_lossy().to_string()),
        file_paths: None,
        byte_size,
        source_app,
        source_bundle_id,
        source_icon_path,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn downscale_png(png: &[u8], max_dim: u32) -> Option<Vec<u8>> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?;
    if img.width() <= max_dim && img.height() <= max_dim {
        return None;
    }
    let thumb = img.thumbnail(max_dim, max_dim);
    let mut out = std::io::Cursor::new(Vec::new());
    thumb.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}
