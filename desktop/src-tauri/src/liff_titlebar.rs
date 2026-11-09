//! Back and forward buttons on the leading edge of a LIFF window's title bar,
//! pin (always on top) and reload on the trailing edge. Drawn inside the page,
//! buttons would sit over the LIFF app's own UI.

use tauri::WebviewWindow;

#[cfg(target_os = "macos")]
pub fn add(window: &WebviewWindow) {
    use crate::liff_devtools::Nav;
    use tauri::Manager;

    let Ok(ptr) = window.ns_window() else { return };
    let addr = ptr as usize;
    let (app, label) = (window.app_handle().clone(), window.label().to_string());
    let nav_label = label.clone();
    let pin = {
        let (app, label) = (app.clone(), label.clone());
        Box::new(move |sender: &objc2::runtime::AnyObject| {
            let Some(w) = app.get_webview_window(&label) else { return };
            let on = !w.is_always_on_top().unwrap_or(false);
            if w.set_always_on_top(on).is_err() {
                return;
            }
            set_pin_appearance(sender, on);
        }) as Box<dyn Fn(&objc2::runtime::AnyObject) + Send>
    };
    let action = move |nav: Nav| -> Box<dyn Fn(&objc2::runtime::AnyObject) + Send> {
        let (app, label) = (app.clone(), label.clone());
        Box::new(move |_| {
            let _ = crate::liff_devtools::navigate(&app, &label, nav);
        })
    };
    let leading = vec![
        Button {
            symbol: "chevron.backward",
            name: "ย้อนกลับ",
            tip: "ย้อนกลับ (⌘[)",
            action: action(Nav::Back),
        },
        Button {
            symbol: "chevron.forward",
            name: "ไปข้างหน้า",
            tip: "ไปข้างหน้า (⌘])",
            action: action(Nav::Forward),
        },
    ];
    // Trailing: pin (window state, nearer the title) then reload (page action
    // at the outer edge). Same WIDTH/GAP rhythm as the leading pair.
    let trailing = vec![
        Button {
            symbol: "pin",
            name: "ปักหมุด",
            tip: "ปักหมุดให้อยู่บนสุด",
            action: pin,
        },
        Button {
            symbol: "arrow.clockwise",
            name: "รีโหลด",
            tip: "รีโหลด (⌘R)",
            action: action(Nav::Reload),
        },
    ];
    let _ = window.run_on_main_thread(move || {
        use objc2_app_kit::NSLayoutAttribute;
        // A new window has nowhere to go back or forward to yet.
        if let [back, forward] = install(addr, leading, NSLayoutAttribute::Left).as_slice() {
            set_enabled(back, false);
            set_enabled(forward, false);
            NAV_BUTTONS.with(|all| {
                all.borrow_mut().insert(nav_label, (back.clone(), forward.clone()));
            });
        }
        install(addr, trailing, NSLayoutAttribute::Right);
    });
}

#[cfg(not(target_os = "macos"))]
pub fn add(_window: &WebviewWindow) {}

#[cfg(target_os = "macos")]
thread_local! {
    /// Each LIFF window's back and forward buttons, by window label. Main
    /// thread only, like the buttons.
    static NAV_BUTTONS: std::cell::RefCell<
        std::collections::HashMap<
            String,
            (objc2::rc::Retained<objc2_app_kit::NSButton>, objc2::rc::Retained<objc2_app_kit::NSButton>),
        >,
    > = Default::default();
}

/// Enable back and forward only when the page's history has somewhere to
/// go, as the web view itself reports it.
#[cfg(target_os = "macos")]
pub fn refresh(window: &WebviewWindow) {
    let label = window.label().to_string();
    // `with_webview` runs on the main thread.
    let _ = window.with_webview(move |webview| {
        let view = webview.inner() as *const objc2::runtime::AnyObject;
        if view.is_null() {
            return;
        }
        // SAFETY: on macOS the platform webview is a live WKWebView, and
        // canGoBack / canGoForward take no arguments and return BOOL.
        let (back, forward): (bool, bool) = unsafe {
            let view = &*view;
            (objc2::msg_send![view, canGoBack], objc2::msg_send![view, canGoForward])
        };
        NAV_BUTTONS.with(|all| {
            if let Some((b, f)) = all.borrow().get(&label) {
                set_enabled(b, back);
                set_enabled(f, forward);
            }
        });
    });
}

/// A disabled button is drawn fainter: its tint is set by hand, so AppKit
/// would not dim it.
#[cfg(target_os = "macos")]
fn set_enabled(button: &objc2_app_kit::NSButton, on: bool) {
    use objc2_app_kit::NSColor;
    if button.isEnabled() == on {
        return;
    }
    button.setEnabled(on);
    let tint = if on { NSColor::secondaryLabelColor() } else { NSColor::tertiaryLabelColor() };
    button.setContentTintColor(Some(&tint));
}

#[cfg(not(target_os = "macos"))]
pub fn refresh(_window: &WebviewWindow) {}

/// Drop a closed window's buttons.
#[cfg(target_os = "macos")]
pub fn forget(app: &tauri::AppHandle, label: &str) {
    let label = label.to_string();
    let _ = app.run_on_main_thread(move || {
        NAV_BUTTONS.with(|all| {
            all.borrow_mut().remove(&label);
        });
    });
}

#[cfg(not(target_os = "macos"))]
pub fn forget(_app: &tauri::AppHandle, _label: &str) {}

#[cfg(target_os = "macos")]
struct Button {
    /// An SF Symbol.
    symbol: &'static str,
    /// What VoiceOver reads.
    name: &'static str,
    tip: &'static str,
    action: Box<dyn Fn(&objc2::runtime::AnyObject) + Send>,
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSResponder, NSTitlebarAccessoryViewController, NSViewController};
    use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol};

    pub struct Ivars {
        /// Indexed by the sending button's tag.
        pub actions: Vec<Box<dyn Fn(&AnyObject)>>,
    }

    define_class!(
        // SAFETY: NSTitlebarAccessoryViewController has no subclassing
        // requirements, and this class doesn't implement Drop.
        //
        // The controller is its buttons' target: buttons don't retain their
        // target, but the window retains its accessory controllers.
        #[unsafe(super(NSTitlebarAccessoryViewController, NSViewController, NSResponder, NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "LAABLiffTitlebarAccessory"]
        #[ivars = Ivars]
        pub struct Accessory;

        impl Accessory {
            #[unsafe(method(act:))]
            fn act(&self, sender: Option<&AnyObject>) {
                let Some(sender) = sender else { return };
                // SAFETY: only the accessory's NSButtons send `act:`.
                let tag: isize = unsafe { msg_send![sender, tag] };
                if let Some(action) = usize::try_from(tag).ok().and_then(|i| self.ivars().actions.get(i)) {
                    action(sender);
                }
            }
        }

        unsafe impl NSObjectProtocol for Accessory {}
    );

    impl Accessory {
        pub fn new(mtm: MainThreadMarker, actions: Vec<Box<dyn Fn(&AnyObject)>>) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(Ivars { actions });
            // SAFETY: NSViewController's designated initializer with no nib.
            unsafe {
                msg_send![super(this), initWithNibName: None::<&AnyObject>, bundle: None::<&AnyObject>]
            }
        }
    }
}

/// Update the pin button's symbol, VoiceOver name, tip and tint for `on`.
#[cfg(target_os = "macos")]
fn set_pin_appearance(sender: &objc2::runtime::AnyObject, on: bool) {
    use objc2_app_kit::{NSButton, NSColor, NSImage};
    use objc2_foundation::{MainThreadMarker, NSString};

    let Some(_mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: `act:` only forwards the accessory's own NSButtons.
    let button = unsafe { &*(sender as *const _ as *const NSButton) };
    let (symbol, name, tip) = if on {
        ("pin.fill", "เลิกปักหมุด", "เลิกปักหมุดให้อยู่บนสุด")
    } else {
        ("pin", "ปักหมุด", "ปักหมุดให้อยู่บนสุด")
    };
    let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(symbol),
        Some(&NSString::from_str(name)),
    ) else {
        return;
    };
    button.setImage(Some(&image));
    button.setToolTip(Some(&NSString::from_str(tip)));
    let tint = if on {
        NSColor::controlAccentColor()
    } else {
        NSColor::secondaryLabelColor()
    };
    button.setContentTintColor(Some(&tint));
}

/// Main thread only. Returns the buttons, in order.
#[cfg(target_os = "macos")]
fn install(
    addr: usize,
    buttons: Vec<Button>,
    side: objc2_app_kit::NSLayoutAttribute,
) -> Vec<objc2::rc::Retained<objc2_app_kit::NSButton>> {
    use objc2::{MainThreadOnly, sel};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSBezelStyle, NSButton, NSColor, NSImage, NSView, NSWindow,
    };
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

    const WIDTH: f64 = 28.0;
    const GAP: f64 = 2.0;
    const INSET: f64 = 4.0;

    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    // SAFETY: the pointer is the live NSWindow behind a Tauri window, and
    // this runs on the main thread.
    let window = unsafe { &*(addr as *const NSWindow) };
    let mut images = Vec::with_capacity(buttons.len());
    for b in &buttons {
        let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(b.symbol),
            Some(&NSString::from_str(b.name)),
        ) else {
            return Vec::new();
        };
        images.push(image);
    }

    let n = buttons.len() as f64;
    let bar = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(INSET * 2.0 + WIDTH * n + GAP * (n - 1.0), 28.0),
        ),
    );
    let tips: Vec<_> = buttons.iter().map(|b| b.tip).collect();
    let accessory = mac::Accessory::new(
        mtm,
        buttons
            .into_iter()
            .map(|b| b.action as Box<dyn Fn(&objc2::runtime::AnyObject)>)
            .collect(),
    );
    let mut made = Vec::with_capacity(images.len());
    for (i, (image, tip)) in images.iter().zip(tips).enumerate() {
        // SAFETY: the target implements `act:`, and outlives the button.
        let button = unsafe {
            NSButton::buttonWithImage_target_action(image, Some(&accessory), Some(sel!(act:)), mtm)
        };
        button.setTag(i as isize);
        button.setBezelStyle(NSBezelStyle::Toolbar);
        button.setShowsBorderOnlyWhileMouseInside(true);
        button.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
        button.setToolTip(Some(&NSString::from_str(tip)));
        button.setFrame(NSRect::new(
            NSPoint::new(INSET + (WIDTH + GAP) * i as f64, 2.0),
            NSSize::new(WIDTH, 24.0),
        ));
        button.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinYMargin | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );
        bar.addSubview(&button);
        made.push(button);
    }
    accessory.setView(&bar);
    accessory.setLayoutAttribute(side);
    window.addTitlebarAccessoryViewController(&accessory);
    made
}
