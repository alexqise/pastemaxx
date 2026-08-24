//! Screenshot watcher: screenshots saved to disk (⌘⇧3/4/5) never touch the
//! clipboard, so this polls the screenshot folder and ingests new ones as
//! image history items.

use crate::clipboard;
use crate::db;
use crate::macos;
use crate::state::AppState;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

const POLL: Duration = Duration::from_secs(2);
/// Polls to keep checking Spotlight metadata on an unrecognized new PNG
/// before giving up on it (Spotlight can lag a few seconds).
const MAX_PENDING_POLLS: u32 = 15;
/// Refresh the (user-configurable) screenshot folder about once a minute.
const DIR_REFRESH_POLLS: u32 = 30;

pub fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let mut dir = screenshot_dir();
        let mut seen: HashSet<PathBuf> = list_pngs(&dir).into_iter().collect();
        let mut pending: HashMap<PathBuf, u32> = HashMap::new();
        let mut polls = 0u32;
        loop {
            std::thread::sleep(POLL);
            polls += 1;
            if polls % DIR_REFRESH_POLLS == 0 {
                let new_dir = screenshot_dir();
                if new_dir != dir {
                    dir = new_dir;
                    seen = list_pngs(&dir).into_iter().collect();
                    pending.clear();
                }
            }
            if !app
                .state::<AppState>()
                .capture_screenshots
                .load(Ordering::SeqCst)
            {
                // keep in sync while off so re-enabling doesn't ingest a backlog
                seen = list_pngs(&dir).into_iter().collect();
                pending.clear();
                continue;
            }
            for path in list_pngs(&dir) {
                if seen.contains(&path) {
                    continue;
                }
                let attempts = pending.entry(path.clone()).or_insert(0);
                *attempts += 1;
                if looks_like_screenshot(&path) || mdls_is_screencapture(&path) {
                    if ingest(&app, &path) {
                        seen.insert(path.clone());
                        pending.remove(&path);
                    }
                    // ingest false = file still settling; retry next poll
                } else if *attempts > MAX_PENDING_POLLS {
                    // some other PNG landed in the folder — not a screenshot
                    seen.insert(path.clone());
                    pending.remove(&path);
                }
            }
            pending.retain(|p, _| p.exists());
        }
    });
}

fn ingest(app: &AppHandle, path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    // let the write settle before reading
    if let Ok(modified) = meta.modified() {
        if SystemTime::now()
            .duration_since(modified)
            .map(|d| d < Duration::from_millis(600))
            .unwrap_or(false)
        {
            return false;
        }
    }
    let Ok(png) = std::fs::read(path) else {
        return false;
    };
    if png.len() < 8 || png[..8] != *b"\x89PNG\r\n\x1a\n" {
        return false;
    }
    let state = app.state::<AppState>();
    let item = objc2::rc::autoreleasepool(|_| {
        clipboard::image_item_from_bytes(
            &state,
            png,
            Some("Screenshot".to_string()),
            None,
            screenshot_icon(&state),
        )
    });
    let Some(item) = item else { return false };
    let ok = {
        let conn = state.db.lock().unwrap();
        db::insert_or_bump(&conn, item).is_ok()
    };
    if ok {
        let _ = app.emit("items-changed", ());
    }
    true
}

/// Cached icon of the system Screenshot app, saved once into the icons dir.
fn screenshot_icon(state: &AppState) -> Option<String> {
    static ICON: OnceLock<Option<String>> = OnceLock::new();
    ICON.get_or_init(|| {
        let path = state.icons_dir.join("screenshot.png");
        if !path.exists() {
            let png = macos::icon_png_for_path("/System/Applications/Utilities/Screenshot.app")?;
            std::fs::write(&path, png).ok()?;
        }
        Some(path.to_string_lossy().to_string())
    })
    .clone()
}

/// The user's screenshot save location (`defaults read com.apple.screencapture
/// location`), falling back to ~/Desktop.
fn screenshot_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    if let Ok(out) = Command::new("/usr/bin/defaults")
        .args(["read", "com.apple.screencapture", "location"])
        .output()
    {
        if out.status.success() {
            let loc = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let expanded = match loc.strip_prefix('~') {
                Some(rest) => format!("{home}{rest}"),
                None => loc,
            };
            if !expanded.is_empty() {
                let p = PathBuf::from(expanded);
                if p.is_dir() {
                    return p;
                }
            }
        }
    }
    PathBuf::from(home).join("Desktop")
}

fn list_pngs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let hidden = p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'));
            let png = p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("png"));
            !hidden && png
        })
        .collect()
}

/// English screenshot filenames; other locales are caught by Spotlight metadata.
fn looks_like_screenshot(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("Screenshot ") || n.starts_with("Screen Shot "))
}

/// Spotlight marks real screenshots with kMDItemIsScreenCapture, regardless of
/// filename localization.
fn mdls_is_screencapture(path: &Path) -> bool {
    Command::new("/usr/bin/mdls")
        .args(["-raw", "-name", "kMDItemIsScreenCapture"])
        .arg(path)
        .output()
        .ok()
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
}
