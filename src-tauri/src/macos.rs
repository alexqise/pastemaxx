//! All raw macOS (AppKit / CoreGraphics / Accessibility) glue lives here.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::AnyThread;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSPasteboard, NSPasteboardTypeHTML,
    NSPasteboardTypePNG, NSPasteboardTypeRTF, NSPasteboardTypeString, NSPasteboardTypeTIFF,
    NSWindow, NSWorkspace,
};
use objc2_foundation::{ns_string, NSArray, NSData, NSDictionary, NSString, NSURL};

/// Content read off the pasteboard, richest format first.
pub enum Captured {
    Text {
        plain: String,
        rtf: Option<Vec<u8>>,
        html: Option<String>,
    },
    Image {
        png: Vec<u8>,
    },
    Files {
        paths: Vec<String>,
    },
}

fn pasteboard() -> Retained<NSPasteboard> {
    NSPasteboard::generalPasteboard()
}

pub fn change_count() -> i64 {
    pasteboard().changeCount() as i64
}

pub fn read_capture() -> Option<Captured> {
    unsafe {
        let pb = pasteboard();

        // 1. Finder file copies (public.file-url on each pasteboard item).
        if let Some(items) = pb.pasteboardItems() {
            let mut paths = Vec::new();
            for item in items.iter() {
                if let Some(url_str) = item.stringForType(ns_string!("public.file-url")) {
                    if let Some(url) = NSURL::URLWithString(&url_str) {
                        if let Some(path) = url.path() {
                            paths.push(path.to_string());
                        }
                    }
                }
            }
            if !paths.is_empty() {
                return Some(Captured::Files { paths });
            }
        }

        // 2. Images (PNG preferred, TIFF converted to PNG).
        if let Some(data) = pb.dataForType(NSPasteboardTypePNG) {
            return Some(Captured::Image { png: data.to_vec() });
        }
        if let Some(tiff) = pb.dataForType(NSPasteboardTypeTIFF) {
            if let Some(png) = image_data_to_png(&tiff) {
                return Some(Captured::Image { png });
            }
        }

        // 3. Text, keeping rich flavors alongside the plain string.
        if let Some(s) = pb.stringForType(NSPasteboardTypeString) {
            let plain = s.to_string();
            if plain.is_empty() {
                return None;
            }
            let rtf = pb.dataForType(NSPasteboardTypeRTF).map(|d| d.to_vec());
            let html = pb.stringForType(NSPasteboardTypeHTML).map(|h| h.to_string());
            return Some(Captured::Text { plain, rtf, html });
        }
        None
    }
}

/// Re-encode any NSImage-readable data (e.g. TIFF) as PNG.
fn image_data_to_png(data: &NSData) -> Option<Vec<u8>> {
    unsafe {
        let rep = NSBitmapImageRep::imageRepWithData(data)?;
        let png = rep.representationUsingType_properties(
            NSBitmapImageFileType::PNG,
            &NSDictionary::new(),
        )?;
        Some(png.to_vec())
    }
}

/// Write text (with optional rich flavors) back to the pasteboard. Returns the new changeCount.
pub fn write_text(plain: &str, rtf: Option<&[u8]>, html: Option<&str>) -> i64 {
    unsafe {
        let pb = pasteboard();
        pb.clearContents();
        pb.setString_forType(&NSString::from_str(plain), NSPasteboardTypeString);
        if let Some(rtf) = rtf {
            pb.setData_forType(Some(&NSData::with_bytes(rtf)), NSPasteboardTypeRTF);
        }
        if let Some(html) = html {
            pb.setString_forType(&NSString::from_str(html), NSPasteboardTypeHTML);
        }
        pb.changeCount() as i64
    }
}

/// Write a PNG image back to the pasteboard (PNG + TIFF for maximum app compatibility).
pub fn write_image_png(png: &[u8]) -> i64 {
    unsafe {
        let pb = pasteboard();
        pb.clearContents();
        let data = NSData::with_bytes(png);
        pb.setData_forType(Some(&data), NSPasteboardTypePNG);
        if let Some(img) = NSImage::initWithData(NSImage::alloc(), &data) {
            if let Some(tiff) = img.TIFFRepresentation() {
                pb.setData_forType(Some(&tiff), NSPasteboardTypeTIFF);
            }
        }
        pb.changeCount() as i64
    }
}

/// Write file URLs back to the pasteboard so Finder pastes real file references.
pub fn write_files(paths: &[String]) -> i64 {
    let pb = pasteboard();
    pb.clearContents();
    let urls: Vec<Retained<NSURL>> = paths
        .iter()
        .map(|p| NSURL::fileURLWithPath(&NSString::from_str(p)))
        .collect();
    let writers: Vec<_> = urls
        .iter()
        .map(|u| ProtocolObject::from_retained(u.clone()))
        .collect();
    let array = NSArray::from_retained_slice(&writers);
    pb.writeObjects(&array);
    pb.changeCount() as i64
}

/// (pid, localized name, bundle id) of the frontmost app.
pub fn frontmost_app() -> Option<(i32, Option<String>, Option<String>)> {
    let ws = NSWorkspace::sharedWorkspace();
    let app = ws.frontmostApplication()?;
    Some((
        app.processIdentifier(),
        app.localizedName().map(|s| s.to_string()),
        app.bundleIdentifier().map(|s| s.to_string()),
    ))
}

/// PNG bytes of the frontmost app's icon.
pub fn frontmost_app_icon_png() -> Option<Vec<u8>> {
    let ws = NSWorkspace::sharedWorkspace();
    let app = ws.frontmostApplication()?;
    let icon = app.icon()?;
    let tiff = icon.TIFFRepresentation()?;
    image_data_to_png(&tiff)
}

/// PNG bytes of the Finder icon for any file/app path.
pub fn icon_png_for_path(path: &str) -> Option<Vec<u8>> {
    let ws = NSWorkspace::sharedWorkspace();
    let icon = ws.iconForFile(&NSString::from_str(path));
    let tiff = icon.TIFFRepresentation()?;
    image_data_to_png(&tiff)
}

/// Position the bar centered at the bottom of the screen the mouse is on.
/// Returns the final frame origin (AppKit bottom-left coords) for animation.
///
/// Done entirely in AppKit coordinates (logical points, bottom-left origin) to
/// avoid the unit/origin mismatches of mixing cursor, monitor, and window APIs —
/// which silently fell back to the primary display for secondary screens.
///
/// Safety: `ptr` must be a live NSWindow pointer, called on the main thread.
pub unsafe fn position_bar_on_mouse_screen(
    ptr: *mut std::ffi::c_void,
    max_width: f64,
    height: f64,
    side_margin: f64,
    bottom_margin: f64,
) -> Option<(f64, f64)> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSScreen};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    let Some(mtm) = MainThreadMarker::new() else {
        return None;
    };
    if ptr.is_null() {
        return None;
    }
    let window: &NSWindow = &*(ptr as *const NSWindow);

    let mouse = NSEvent::mouseLocation();
    let screen = NSScreen::screens(mtm)
        .iter()
        .find(|s| {
            let f = s.frame();
            mouse.x >= f.origin.x
                && mouse.x < f.origin.x + f.size.width
                && mouse.y >= f.origin.y
                && mouse.y < f.origin.y + f.size.height
        })
        .or_else(|| NSScreen::mainScreen(mtm));
    let screen = screen?;

    // visibleFrame excludes the menu bar and Dock
    let vf = screen.visibleFrame();
    let w = (vf.size.width - side_margin * 2.0).min(max_width);
    let x = vf.origin.x + (vf.size.width - w) / 2.0;
    let y = vf.origin.y + bottom_margin;
    window.setFrame_display(
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, height)),
        true,
    );
    Some((x, y))
}

/// Set the native alpha of a Tauri window from its raw NSWindow pointer.
///
/// Safety: `ptr` must be a live NSWindow pointer, called on the main thread.
pub unsafe fn set_window_alpha(ptr: *mut std::ffi::c_void, alpha: f64) {
    if ptr.is_null() {
        return;
    }
    let window: &NSWindow = &*(ptr as *const NSWindow);
    window.setAlphaValue(alpha);
}

/// Move and fade the window in one step — used to animate the whole window
/// (glass included) sliding up from below.
///
/// Safety: `ptr` must be a live NSWindow pointer, called on the main thread.
pub unsafe fn set_window_origin_alpha(ptr: *mut std::ffi::c_void, x: f64, y: f64, alpha: f64) {
    use objc2_foundation::NSPoint;
    if ptr.is_null() {
        return;
    }
    let window: &NSWindow = &*(ptr as *const NSWindow);
    window.setFrameOrigin(NSPoint::new(x, y));
    window.setAlphaValue(alpha);
}

// ---- Accessibility (auto-paste) ----

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(
        options: core_foundation::dictionary::CFDictionaryRef,
    ) -> bool;
    static kAXTrustedCheckOptionPrompt: core_foundation::string::CFStringRef;
}

/// Check Accessibility permission; optionally shows the system prompt.
pub fn ax_trusted(prompt: bool) -> bool {
    unsafe {
        if !prompt {
            return AXIsProcessTrusted();
        }
        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let dict = CFDictionary::from_CFType_pairs(&[(
            key.as_CFType(),
            CFBoolean::true_value().as_CFType(),
        )]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as _)
    }
}

/// Simulate Cmd+V. When `pid` is known, the events are posted straight to that
/// process — the bar (a non-activating panel) can keep keyboard focus while the
/// target app receives the paste. Falls back to the system HID tap otherwise.
pub fn send_cmd_v(pid: Option<i32>) {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    const KEY_V: u16 = 9; // kVK_ANSI_V
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return;
    };
    for down in [true, false] {
        if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), KEY_V, down) {
            event.set_flags(CGEventFlags::CGEventFlagCommand);
            match pid {
                Some(pid) if pid > 0 => event.post_to_pid(pid),
                _ => event.post(CGEventTapLocation::HID),
            }
        }
    }
}
