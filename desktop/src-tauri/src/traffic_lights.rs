//! LINE for Mac packs the window buttons closer than macOS does, so all three
//! fit in its 62pt left bar: 6pt from the edge, origins 18pt apart (macOS 26+
//! uses 23pt).
//!
//! AppKit lays the buttons out again on every live-resize step, after any
//! window event has been handled, so moving them from the resize event let
//! the default spacing flash on screen. Instead each button gets a
//! synchronous frame-change observer that puts it back before AppKit draws.

use tauri::{Runtime, Window};

#[cfg(target_os = "macos")]
pub fn place<R: Runtime>(window: &Window<R>) {
    use objc2_foundation::MainThreadMarker;

    let Ok(ptr) = window.ns_window() else { return };
    let addr = ptr as usize;
    if MainThreadMarker::new().is_some() {
        pin_buttons(addr);
    } else {
        let _ = window.run_on_main_thread(move || pin_buttons(addr));
    }
}

/// Move the buttons now and, once per window, keep them there.
#[cfg(target_os = "macos")]
fn pin_buttons(addr: usize) {
    use std::collections::HashSet;
    use std::ptr::NonNull;
    use std::sync::Mutex;

    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSViewFrameDidChangeNotification, NSWindow, NSWindowButton};
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint};

    static PINNED: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

    // SAFETY: the pointer is the live NSWindow behind a Tauri window, and
    // this runs on the main thread.
    let ns_window = unsafe { &*(addr as *const NSWindow) };
    let first = PINNED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashSet::new)
        .insert(addr);
    let kinds = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ];
    for (i, kind) in kinds.into_iter().enumerate() {
        let Some(button) = ns_window.standardWindowButton(kind) else { continue };
        let x = 6.0 + 18.0 * i as f64;
        let y = button.frame().origin.y;
        button.setFrameOrigin(NSPoint::new(x, y));
        if !first {
            continue;
        }
        button.setPostsFrameChangedNotifications(true);
        let pinned = button.clone();
        let block = RcBlock::new(move |_: NonNull<NSNotification>| {
            let f = pinned.frame();
            // Our own move posts this again; the check ends it there.
            if (f.origin.x - x).abs() > 0.5 {
                pinned.setFrameOrigin(NSPoint::new(x, f.origin.y));
            }
        });
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: no queue, so the block runs synchronously on the posting
        // (main) thread, where AppKit lays out the title bar.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSViewFrameDidChangeNotification),
                Some(&*button as &AnyObject),
                None,
                &block,
            )
        };
        // The window lives as long as the app; so does the observer.
        std::mem::forget(token);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn place<R: Runtime>(_window: &Window<R>) {}
