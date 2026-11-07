//! `liff.scanCode()` without a camera: the user picks an image or drags
//! over part of the screen, and macOS Vision reads the code in it.
//!
//! The answer is `{value}` with what was read, or `{value: null}` when the
//! user backs out or the picture has no code.

use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};

const PICK: &str = "เลือกรูป";
const SCREEN: &str = "จับภาพหน้าจอ";

/// Ask where the code is, read it, and give back its text, or None.
pub async fn scan(app: &AppHandle, parent: Option<&tauri::WebviewWindow>) -> Option<String> {
    let mut dialog = app
        .dialog()
        .message("เลือกรูปที่มี QR code หรือบาร์โค้ด หรือลากคลุมโค้ดบนหน้าจอ")
        .title("สแกนโค้ด")
        .buttons(MessageDialogButtons::YesNoCancelCustom(
            PICK.into(),
            SCREEN.into(),
            "ยกเลิก".into(),
        ));
    if let Some(w) = parent {
        dialog = dialog.parent(w);
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    dialog.show_with_result(move |r| {
        let _ = tx.send(r);
    });
    let image = match rx.await.ok()? {
        MessageDialogResult::Custom(label) if label == PICK => pick_image(app).await?,
        MessageDialogResult::Custom(label) if label == SCREEN => capture_region().await?,
        _ => return None,
    };
    let screenshot = image.starts_with(std::env::temp_dir());
    let read = tokio::task::spawn_blocking({
        let image = image.clone();
        move || read_code(&image)
    })
    .await
    .ok()
    .flatten();
    if screenshot {
        let _ = std::fs::remove_file(&image);
    }
    read
}

async fn pick_image(app: &AppHandle) -> Option<PathBuf> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("เลือกรูปที่มีโค้ด")
        .add_filter(
            "รูปภาพ",
            &["png", "jpg", "jpeg", "heic", "gif", "tiff", "bmp", "webp"],
        )
        .pick_file(move |f| {
            let _ = tx.send(f);
        });
    rx.await.ok()??.into_path().ok()
}

/// macOS's own region screenshot (⌘⇧4): the user drags over the code, or
/// presses Escape and nothing is saved.
async fn capture_region() -> Option<PathBuf> {
    let path =
        std::env::temp_dir().join(format!("laab-scan-{}.png", crate::state::now_ms() as u64));
    let status = tokio::process::Command::new("/usr/sbin/screencapture")
        .args(["-i", "-x", "-t", "png"])
        .arg(&path)
        .status()
        .await
        .ok()?;
    (status.success() && path.exists()).then_some(path)
}

/// The text of the first barcode or QR code macOS Vision finds in the
/// image at `path` (`VNDetectBarcodesRequest`).
#[cfg(target_os = "macos")]
pub fn read_code(path: &Path) -> Option<String> {
    use objc2::msg_send;
    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSArray, NSDictionary, NSString, NSURL};

    #[link(name = "Vision", kind = "framework")]
    unsafe extern "C" {}

    let path = path.to_str()?;
    autoreleasepool(|_| unsafe {
        let request_class = AnyClass::get(c"VNDetectBarcodesRequest")?;
        let handler_class = AnyClass::get(c"VNImageRequestHandler")?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let options = NSDictionary::<AnyObject, AnyObject>::new();
        let request: Retained<AnyObject> = msg_send![request_class, new];
        let handler: Retained<AnyObject> = msg_send![
            msg_send![handler_class, alloc],
            initWithURL: &*url,
            options: &*options
        ];
        let requests = NSArray::from_retained_slice(std::slice::from_ref(&request));
        let ok: bool = msg_send![
            &*handler,
            performRequests: &*requests,
            error: std::ptr::null_mut::<*mut AnyObject>()
        ];
        if !ok {
            return None;
        }
        let results: Option<Retained<NSArray<AnyObject>>> = msg_send![&*request, results];
        results?.iter().find_map(|r| {
            let text: Option<Retained<NSString>> = msg_send![&*r, payloadStringValue];
            text.map(|t| t.to_string()).filter(|t| !t.is_empty())
        })
    })
}

#[cfg(not(target_os = "macos"))]
pub fn read_code(_path: &Path) -> Option<String> {
    None
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn reads_a_qr_code_from_an_image() {
        let qr = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/qr.png");
        assert_eq!(
            read_code(&qr).as_deref(),
            Some("https://example.com/laab-scan-test")
        );
    }

    #[test]
    fn nothing_to_read_is_none() {
        let icon = Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/32x32.png");
        assert_eq!(read_code(&icon), None);
        assert_eq!(read_code(Path::new("/nonexistent/laab.png")), None);
    }
}
