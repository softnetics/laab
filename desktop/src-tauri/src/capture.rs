//! Pictures of messages as LAAB draws them. The main window renders the
//! rows off screen and hands back a PNG (`src/lib/capture.ts`); this side
//! asks for one and waits for it.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::json;
use tauri::Emitter;
use tokio::sync::oneshot;

use crate::model::Message;

type Reply = Result<Vec<u8>, String>;

static NEXT: AtomicU64 = AtomicU64::new(1);
static PENDING: Mutex<Option<HashMap<u64, oneshot::Sender<Reply>>>> = Mutex::new(None);

/// Longer than loading a few photos from LINE takes.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Render `messages` of `chat`, in this order, to a PNG.
pub async fn messages(app: &tauri::AppHandle, chat: &str, messages: &[Message]) -> Reply {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    PENDING.lock().unwrap().get_or_insert_with(HashMap::new).insert(id, tx);
    let asked = app.emit_to("main", "capture-request", json!({ "id": id, "chatId": chat, "messages": messages }));
    let result = match asked {
        Err(e) => Err(format!("หน้าต่าง LAAB ไม่ตอบ: {e}")),
        Ok(()) => match tokio::time::timeout(TIMEOUT, rx).await {
            Ok(Ok(reply)) => reply,
            Ok(Err(_)) => Err("หน้าต่าง LAAB ปิดไปก่อนวาดรูปเสร็จ".into()),
            Err(_) => Err("วาดรูปข้อความไม่ทันเวลา (LAAB เปิดหน้าต่างหลักอยู่หรือไม่)".into()),
        },
    };
    take(id);
    result
}

fn take(id: u64) -> Option<oneshot::Sender<Reply>> {
    PENDING.lock().unwrap().as_mut().and_then(|p| p.remove(&id))
}

/// The PNG for a `capture-request`, as raw IPC bytes with its id in the
/// `capture-id` header.
#[tauri::command]
pub fn capture_done(request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let id = request
        .headers()
        .get("capture-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or("missing capture id")?;
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else {
        return Err("no image".into());
    };
    if let Some(tx) = take(id) {
        let _ = tx.send(Ok(data.clone()));
    }
    Ok(())
}

/// A `capture-request` the window couldn't draw.
#[tauri::command]
pub fn capture_failed(id: u64, error: String) {
    if let Some(tx) = take(id) {
        let _ = tx.send(Err(error));
    }
}

/// Pictures bigger than this aren't drawn into a capture.
const MAX_REMOTE: usize = 10 * 1024 * 1024;

/// An https image a flex card shows, for the capture to draw: the window
/// can't read it itself, since LINE's servers don't allow other origins.
#[tauri::command]
pub async fn capture_image(url: String) -> Result<tauri::ipc::Response, String> {
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "https" {
        return Err("only https images".into());
    }
    // The URL comes from a message, so its sender picks it: follow only
    // https redirects, never to http (a local or private service).
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|a| {
            if a.url().scheme() != "https" {
                a.error("redirect away from https")
            } else if a.previous().len() >= 5 {
                a.stop()
            } else {
                a.follow()
            }
        }))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .get(parsed)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let image = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.starts_with("image/"));
    if !image {
        return Err("not an image".into());
    }
    if res.content_length().is_some_and(|n| n as usize > MAX_REMOTE) {
        return Err("image too large".into());
    }
    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_REMOTE {
        return Err("image too large".into());
    }
    Ok(tauri::ipc::Response::new(bytes.to_vec()))
}
