//! Folding the conversation pane slides the main window between the full
//! layout and the chat list, the way LINE for Mac does.
//!
//! `setSize` jumps in one step. AppKit's animator is smooth but scales a
//! snapshot of the window, which squeezed the web view and let the window
//! buttons fall back to macOS's spacing mid-slide (`traffic_lights.rs` only
//! hears real frame changes). So the slide is a series of real frame changes
//! instead, each one laid out and drawn by AppKit.
//!
//! The web view keeps one width for the whole slide (the wider of the two)
//! and the window clips it. Resizing it every step made WebKit stretch its
//! last frame to the new size until it drew the next one.

use std::time::{Duration, Instant};

use tauri::{Runtime, WebviewWindow};

const DURATION: Duration = Duration::from_millis(220);
/// One step per frame on a 120 Hz display.
const STEP: Duration = Duration::from_millis(8);
/// Before growing: time for WebKit to draw the newly widened web view, so
/// the strip the window uncovers already has the conversation in it.
const PAINT: Duration = Duration::from_millis(34);

/// Slide the window to `width` (keeping its top-left corner and height),
/// with `min_width` as the new minimum. Resolves when the slide ends.
///
/// With `release` false the web view stays at its wide size afterwards, for
/// the page to drop the conversation pane first; `release_web_view` then fits
/// it to the window. A web view narrower than the page laid out in it drew
/// the page shifted sideways until the layout caught up.
///
/// `background` is the page's background colour. While the window grows,
/// the web view paints the uncovered strip a frame late; filling the window
/// with the same colour keeps that strip from flashing dark.
#[tauri::command]
pub async fn resize_window<R: Runtime>(
    window: WebviewWindow<R>,
    width: f64,
    min_width: f64,
    background: [u8; 3],
    release: bool,
) -> Result<(), String> {
    let [r, g, b] = background;
    window
        .set_background_color(Some(tauri::window::Color(r, g, b, 255)))
        .map_err(|e| e.to_string())?;
    let from = start(&window, width, min_width).await?;
    if width > from {
        tokio::time::sleep(PAINT).await;
    }
    let began = Instant::now();
    loop {
        let t = (began.elapsed().as_secs_f64() / DURATION.as_secs_f64()).min(1.0);
        // Ease out: quick at first, settling into place.
        let eased = 1.0 - (1.0 - t).powi(3);
        set_width(&window, (from + (width - from) * eased).round())?;
        if t >= 1.0 {
            return if release { finish(&window) } else { Ok(()) };
        }
        tokio::time::sleep(STEP).await;
    }
}

/// Fit the web view to the window after a slide that kept it wide.
#[tauri::command]
pub async fn release_web_view<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    finish(&window)
}

/// Apply the new minimum, give the web view its width for the whole slide,
/// and return the window's current width.
#[cfg(target_os = "macos")]
async fn start<R: Runtime>(
    window: &WebviewWindow<R>,
    to: f64,
    min_width: f64,
) -> Result<f64, String> {
    use objc2_app_kit::NSAutoresizingMaskOptions;

    let addr = window.ns_window().map_err(|e| e.to_string())? as usize;
    let (send, width) = tokio::sync::oneshot::channel();
    window
        .run_on_main_thread(move || {
            let ns_window = ns_window(addr);
            let min = ns_window.contentMinSize();
            ns_window.setContentMinSize(objc2_foundation::NSSize::new(min_width, min.height));
            let from = ns_window.frame().size.width;
            if let Some(view) = web_view(ns_window) {
                view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable);
                let mut frame = view.frame();
                frame.size.width = from.max(to);
                view.setFrame(frame);
            }
            let _ = send.send(from);
        })
        .map_err(|e| e.to_string())?;
    width.await.map_err(|_| "the window went away".to_string())
}

/// Let the web view follow the window again, at its final width.
#[cfg(target_os = "macos")]
fn finish<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    use objc2_app_kit::NSAutoresizingMaskOptions;

    let addr = window.ns_window().map_err(|e| e.to_string())? as usize;
    window
        .run_on_main_thread(move || {
            let ns_window = ns_window(addr);
            let (Some(view), Some(content)) = (web_view(ns_window), ns_window.contentView()) else {
                return;
            };
            view.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            view.setFrame(content.bounds());
        })
        .map_err(|e| e.to_string())
}

/// The WKWebView, which wry puts inside the window's content view.
#[cfg(target_os = "macos")]
fn web_view(
    ns_window: &objc2_app_kit::NSWindow,
) -> Option<objc2::rc::Retained<objc2_app_kit::NSView>> {
    let content = ns_window.contentView()?;
    content
        .subviews()
        .iter()
        .find(|view| view.class().name().to_string_lossy().contains("WebView"))
}

#[cfg(target_os = "macos")]
fn set_width<R: Runtime>(window: &WebviewWindow<R>, width: f64) -> Result<(), String> {
    let addr = window.ns_window().map_err(|e| e.to_string())? as usize;
    window
        .run_on_main_thread(move || {
            let ns_window = ns_window(addr);
            let mut frame = ns_window.frame();
            // AppKit's origin is the bottom-left corner; with the height
            // unchanged, keeping it keeps the top-left corner in place too.
            frame.size.width = width;
            // Draw only after the buttons are back in place: AppKit lays the
            // title bar out again inside setFrame.
            ns_window.setFrame_display(frame, false);
            crate::traffic_lights::pin_now(ns_window);
            ns_window.displayIfNeeded();
        })
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn ns_window<'a>(addr: usize) -> &'a objc2_app_kit::NSWindow {
    // SAFETY: the address is the live NSWindow behind a Tauri window, and
    // callers run on the main thread.
    unsafe { &*(addr as *const objc2_app_kit::NSWindow) }
}

#[cfg(not(target_os = "macos"))]
fn finish<R: Runtime>(_window: &WebviewWindow<R>) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
async fn start<R: Runtime>(
    window: &WebviewWindow<R>,
    _to: f64,
    min_width: f64,
) -> Result<f64, String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let size = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    window
        .set_min_size(Some(tauri::LogicalSize::new(min_width, 480.0)))
        .map_err(|e| e.to_string())?;
    Ok(size.width)
}

#[cfg(not(target_os = "macos"))]
fn set_width<R: Runtime>(window: &WebviewWindow<R>, width: f64) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let size = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    window
        .set_size(tauri::LogicalSize::new(width, size.height))
        .map_err(|e| e.to_string())
}
