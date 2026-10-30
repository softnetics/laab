//! LINE for Mac packs the window buttons closer than macOS does, so all three
//! fit in its 62pt left bar: 6pt from the edge, origins 18pt apart (macOS 26+
//! uses 23pt).
//!
//! AppKit lays the buttons out again on every resize step, after any window
//! event has been handled, so moving them from the resize event let the
//! default spacing flash on screen. A frame-change observer on each button
//! caught most steps, but AppKit can defer that notification until after the
//! frame is drawn. So placement also runs at the end of `NSTitlebarView`'s own
//! `layout`, inside the same pass, before anything is drawn.

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
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSViewFrameDidChangeNotification, NSWindow, NSWindowButton};
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint};

    // SAFETY: the pointer is the live NSWindow behind a Tauri window, and
    // this runs on the main thread.
    let ns_window = unsafe { &*(addr as *const NSWindow) };
    let first = PINNED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(std::collections::HashSet::new)
        .insert(addr);
    hook_titlebar_layout();
    let kinds = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ];
    pin_now(ns_window);
    if !first {
        return;
    }
    for (i, kind) in kinds.into_iter().enumerate() {
        let Some(button) = ns_window.standardWindowButton(kind) else {
            continue;
        };
        let x = button_x(i);
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

/// Windows whose buttons LAAB places, by NSWindow address.
#[cfg(target_os = "macos")]
static PINNED: std::sync::Mutex<Option<std::collections::HashSet<usize>>> =
    std::sync::Mutex::new(None);

/// AppKit's own `-[NSTitlebarView layout]`, called before ours.
#[cfg(target_os = "macos")]
static ORIGINAL_LAYOUT: std::sync::OnceLock<objc2::runtime::Imp> = std::sync::OnceLock::new();

/// Replace `-[NSTitlebarView layout]` with one that calls AppKit's and then
/// places the buttons. If the class only inherits `layout`, a method is added
/// to it alone (calling NSView's), so other views are untouched. A missing
/// class (a future macOS) leaves the observers to do the work.
#[cfg(target_os = "macos")]
fn hook_titlebar_layout() {
    use std::sync::Once;

    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::sel;

    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let Some(class) = AnyClass::get(c"NSTitlebarView") else {
            return;
        };
        let layout = sel!(layout);
        // SAFETY: same signature as -layout, which takes no arguments.
        let ours: Imp = unsafe {
            std::mem::transmute::<unsafe extern "C-unwind" fn(&AnyObject, Sel), Imp>(
                titlebar_layout,
            )
        };
        let own = class
            .instance_methods()
            .iter()
            .copied()
            .find(|m| m.name() == layout);
        if let Some(method) = own {
            let _ = ORIGINAL_LAYOUT.set(method.implementation());
            // SAFETY: main thread, matching signature.
            unsafe { method.set_implementation(ours) };
        } else if let Some(inherited) = class.superclass().and_then(|c| c.instance_method(layout)) {
            let _ = ORIGINAL_LAYOUT.set(inherited.implementation());
            // SAFETY: adds -layout (v@:) to NSTitlebarView only.
            unsafe {
                objc2::ffi::class_addMethod(
                    class as *const AnyClass as *mut AnyClass,
                    layout,
                    ours,
                    c"v@:".as_ptr(),
                )
            };
        }
    });
}

#[cfg(target_os = "macos")]
unsafe extern "C-unwind" fn titlebar_layout(
    this: &objc2::runtime::AnyObject,
    cmd: objc2::runtime::Sel,
) {
    use objc2::runtime::{AnyObject, Sel};
    use objc2_app_kit::NSView;

    if let Some(original) = ORIGINAL_LAYOUT.get() {
        // SAFETY: the IMP of -layout, which takes no arguments.
        let original = unsafe {
            std::mem::transmute::<objc2::runtime::Imp, unsafe extern "C-unwind" fn(&AnyObject, Sel)>(
                *original,
            )
        };
        unsafe { original(this, cmd) };
    }
    // SAFETY: only installed on NSTitlebarView, an NSView subclass.
    let view = unsafe { &*(this as *const AnyObject as *const NSView) };
    let Some(window) = view.window() else { return };
    let addr = &*window as *const objc2_app_kit::NSWindow as usize;
    let ours = PINNED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|pinned| pinned.contains(&addr));
    if ours {
        pin_now(&window);
    }
}

/// Where the `i`th button's left edge goes.
#[cfg(target_os = "macos")]
fn button_x(i: usize) -> f64 {
    6.0 + 18.0 * i as f64
}

/// Put the buttons where LAAB wants them, without registering observers.
/// Main thread only.
#[cfg(target_os = "macos")]
pub fn pin_now(ns_window: &objc2_app_kit::NSWindow) {
    use objc2_app_kit::NSWindowButton;
    use objc2_foundation::NSPoint;

    let kinds = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ];
    for (i, kind) in kinds.into_iter().enumerate() {
        let Some(button) = ns_window.standardWindowButton(kind) else {
            continue;
        };
        let x = button_x(i);
        if (button.frame().origin.x - x).abs() > 0.5 {
            button.setFrameOrigin(NSPoint::new(x, button.frame().origin.y));
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn place<R: Runtime>(_window: &Window<R>) {}
