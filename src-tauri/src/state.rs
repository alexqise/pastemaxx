use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64};
use std::sync::Mutex;
use std::time::Instant;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub images_dir: PathBuf,
    pub icons_dir: PathBuf,
    /// pid of the app that was frontmost before the bar was shown (paste target).
    pub prev_app_pid: AtomicI32,
    /// changeCount produced by our own pasteboard writes, so the watcher skips them.
    pub self_change: AtomicI64,
    /// last changeCount the watcher has seen.
    pub last_change: AtomicI64,
    /// set while a paste is in flight so the blur handler doesn't hide the bar.
    pub last_paste: Mutex<Option<Instant>>,
    /// true while the hide animation is running.
    pub hiding: AtomicBool,
    /// resting frame origin of the bar (AppKit coords), set when shown.
    pub bar_origin: Mutex<Option<(f64, f64)>>,
}
