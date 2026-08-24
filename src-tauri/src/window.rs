//! Bar window management: positioning on the active display, native slide+fade.

use crate::macos;
use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_nspanel::ManagerExt as PanelManagerExt;

pub const BAR_LABEL: &str = "bar";
const BAR_HEIGHT: f64 = 210.0;
const BAR_MAX_WIDTH: f64 = 1440.0;
const SIDE_MARGIN: f64 = 24.0;
const BOTTOM_MARGIN: f64 = 16.0;
/// How far below its resting spot the bar starts/ends its slide.
const SLIDE_DISTANCE: f64 = 26.0;
/// Blur events within this window of a paste don't hide the bar (spec: stays open).
const PASTE_BLUR_GRACE: Duration = Duration::from_millis(1500);

fn bar_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(BAR_LABEL)
}

pub fn toggle_bar(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.hiding.load(Ordering::SeqCst) {
        return; // mid hide-animation; ignore
    }
    let Some(win) = bar_window(app) else { return };
    if win.is_visible().unwrap_or(false) {
        hide_bar(app);
    } else {
        show_bar(app);
    }
}

pub fn show_bar(app: &AppHandle) {
    let Some(win) = bar_window(app) else { return };
    let state = app.state::<AppState>();

    // Remember the paste target (still frontmost — the panel never activates us).
    if let Some((pid, _, _)) = macos::frontmost_app() {
        state.prev_app_pid.store(pid, Ordering::SeqCst);
    }

    // Position, start below-and-invisible, and order front — one main-thread
    // block so the window can't flash at its resting spot first.
    let app2 = app.clone();
    let w = win.clone();
    let _ = win.run_on_main_thread(move || {
        let Ok(ptr) = w.ns_window() else { return };
        let origin = unsafe {
            macos::position_bar_on_mouse_screen(
                ptr,
                BAR_MAX_WIDTH,
                BAR_HEIGHT,
                SIDE_MARGIN,
                BOTTOM_MARGIN,
            )
        };
        *app2.state::<AppState>().bar_origin.lock().unwrap() = origin;
        unsafe {
            match origin {
                Some((x, y)) => macos::set_window_origin_alpha(ptr, x, y - SLIDE_DISTANCE, 0.0),
                None => macos::set_window_alpha(ptr, 0.0),
            }
        }
        if let Ok(panel) = app2.get_webview_panel(BAR_LABEL) {
            panel.show_and_make_key();
        }
    });
    let _ = app.emit("bar-shown", ());

    // Slide the whole window (native glass included) up while fading in.
    let origin = *app.state::<AppState>().bar_origin.lock().unwrap();
    match origin {
        Some((x, y)) => animate(win, Some((x, y - SLIDE_DISTANCE, y)), 0.0, 1.0, 200, |_| {}),
        None => animate(win, None, 0.0, 1.0, 160, |_| {}),
    }
}

pub fn hide_bar(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.hiding.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(win) = bar_window(app) else {
        state.hiding.store(false, Ordering::SeqCst);
        return;
    };
    let _ = app.emit("bar-hiding", ());

    let origin = *state.bar_origin.lock().unwrap();
    let app2 = app.clone();
    let done = move |w: &WebviewWindow| {
        let app3 = app2.clone();
        let _ = w.run_on_main_thread(move || {
            if let Ok(panel) = app3.get_webview_panel(BAR_LABEL) {
                panel.hide();
            }
            app3.state::<AppState>().hiding.store(false, Ordering::SeqCst);
        });
    };
    match origin {
        Some((x, y)) => animate(win, Some((x, y, y - SLIDE_DISTANCE)), 1.0, 0.0, 160, done),
        None => animate(win, None, 1.0, 0.0, 140, done),
    }
}

/// Key-lost handler: click-outside hides the bar, but a paste-triggered blur doesn't.
pub fn on_blur(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.hiding.load(Ordering::SeqCst) {
        return;
    }
    if let Some(t) = *state.last_paste.lock().unwrap() {
        if t.elapsed() < PASTE_BLUR_GRACE {
            return;
        }
    }
    let Some(win) = bar_window(app) else { return };
    if win.is_visible().unwrap_or(false) {
        hide_bar(app);
    }
}

/// Ease-out animation of window origin + alpha, then `done` on a background thread.
/// `slide` is (x, y_from, y_to); None fades in place.
fn animate(
    win: WebviewWindow,
    slide: Option<(f64, f64, f64)>,
    alpha_from: f64,
    alpha_to: f64,
    duration_ms: u64,
    done: impl FnOnce(&WebviewWindow) + Send + 'static,
) {
    std::thread::spawn(move || {
        const STEPS: u32 = 14;
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let eased = 1.0 - (1.0 - t).powi(3);
            let alpha = alpha_from + (alpha_to - alpha_from) * eased;
            let w = win.clone();
            let _ = win.run_on_main_thread(move || {
                let Ok(ptr) = w.ns_window() else { return };
                unsafe {
                    match slide {
                        Some((x, y_from, y_to)) => {
                            let y = y_from + (y_to - y_from) * eased;
                            macos::set_window_origin_alpha(ptr, x, y, alpha);
                        }
                        None => macos::set_window_alpha(ptr, alpha),
                    }
                }
            });
            std::thread::sleep(Duration::from_millis(duration_ms / STEPS as u64));
        }
        done(&win);
    });
}
