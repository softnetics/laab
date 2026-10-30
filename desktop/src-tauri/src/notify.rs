//! macOS banners for new messages, and the Dock badge.

use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::model::{Chat, Message};
use crate::state::{AppState, Focus};

/// A banner is posted when the message is not ours, arrived after launch (30 s
/// grace), was not already shown, is not in the chat the user is looking at,
/// and the chat is not muted unless it mentions us.
pub fn should_notify(
    msg: &Message,
    chat: &Chat,
    launched_ms: f64,
    focus: &Focus,
    already: bool,
) -> bool {
    if msg.mine || already || msg.unsent > 0 {
        return false;
    }
    if msg.created_time > 0.0 && msg.created_time < launched_ms - 30_000.0 {
        return false;
    }
    if focus.active && focus.chat.as_deref() == Some(chat.id.as_str()) {
        return false;
    }
    !chat.muted || msg.mentions_me
}

pub fn body(msg: &Message) -> String {
    let base = match msg.content_type.as_str() {
        "CALL" => crate::convert::call_text(msg, "เขา"),
        "IMAGE" | "VIDEO" | "STICKER" | "AUDIO" | "FILE" | "CHATEVENT" | "FLEX" | "RICH"
        | "POSTNOTIFICATION" | "CONTACT" => crate::convert::preview_body(msg),
        _ => {
            let chars: Vec<char> = msg.text.chars().collect();
            if chars.len() > 140 {
                format!("{}…", chars[..139].iter().collect::<String>())
            } else {
                msg.text.clone()
            }
        }
    };
    if msg.mentions_me {
        format!("กล่าวถึงคุณ: {base}")
    } else {
        base
    }
}

pub fn maybe_notify(state: &Arc<AppState>, chat: &Chat, msg: &Message) {
    if state.demo {
        return;
    }
    let focus = state.focus.lock().unwrap().clone();
    let already = !state.notified.lock().unwrap().insert(msg.id.clone());
    if !should_notify(msg, chat, state.launched_ms, &focus, already) {
        return;
    }
    let title = if chat.name.is_empty() || chat.name == chat.id {
        "ข้อความใหม่".to_string()
    } else {
        chat.name.clone()
    };
    let subtitle = if chat.kind == "group" {
        msg.sender_id
            .as_ref()
            .and_then(|s| state.store().people().ok()?.get(s).map(|p| p.0.clone()))
    } else {
        None
    };
    let text = body(msg);
    let chat_id = chat.id.clone();
    let app = state.app.clone();
    post(app, title, subtitle, text, chat_id);
}

#[cfg(target_os = "macos")]
fn post(
    app: tauri::AppHandle,
    title: String,
    subtitle: Option<String>,
    text: String,
    chat_id: String,
) {
    std::thread::spawn(move || {
        let response = mac_notification_sys::Notification::new()
            .title(&title)
            .maybe_subtitle(subtitle.as_deref())
            .message(&text)
            .default_sound()
            .wait_for_click(true)
            .send();
        // Clicking the banner opens its chat.
        if let Ok(mac_notification_sys::NotificationResponse::Click) = response {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            let _ = app.emit("open-chat", serde_json::json!({ "chatId": chat_id }));
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn post(
    _app: tauri::AppHandle,
    _title: String,
    _subtitle: Option<String>,
    _text: String,
    _chat_id: String,
) {
}

pub fn setup(identifier: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = mac_notification_sys::set_application(identifier);
    }
    let _ = identifier;
}

/// The Dock badge is the sum of unread counts, "99+" above 99.
pub fn update_badge(state: &AppState) {
    if state.demo {
        return;
    }
    let total: i64 = state
        .store()
        .chats()
        .map(|c| c.iter().map(|c| c.unread.max(0)).sum())
        .unwrap_or(0);
    let label = match total {
        0 => None,
        n if n > 99 => Some("99+".to_string()),
        n => Some(n.to_string()),
    };
    if let Some(w) = state.app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        let _ = w.set_badge_label(label);
        #[cfg(not(target_os = "macos"))]
        let _ = (w, label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg() -> Message {
        Message {
            id: "1".into(),
            chat_id: "c1".into(),
            created_time: 10_000_000.0,
            text: "hi".into(),
            ..Default::default()
        }
    }

    #[test]
    fn follows_the_banner_rules() {
        let chat = Chat {
            id: "c1".into(),
            ..Default::default()
        };
        let away = Focus {
            chat: Some("c2".into()),
            active: true,
        };
        assert!(should_notify(&msg(), &chat, 10_000_000.0, &away, false));
        assert!(!should_notify(&msg(), &chat, 10_000_000.0, &away, true));
        let here = Focus {
            chat: Some("c1".into()),
            active: true,
        };
        assert!(!should_notify(&msg(), &chat, 10_000_000.0, &here, false));
        let background = Focus {
            chat: Some("c1".into()),
            active: false,
        };
        assert!(should_notify(
            &msg(),
            &chat,
            10_000_000.0,
            &background,
            false
        ));
        let muted = Chat {
            muted: true,
            ..chat.clone()
        };
        assert!(!should_notify(&msg(), &muted, 10_000_000.0, &away, false));
        assert!(should_notify(
            &Message {
                mentions_me: true,
                ..msg()
            },
            &muted,
            10_000_000.0,
            &away,
            false
        ));
        assert!(!should_notify(
            &msg(),
            &chat,
            10_000_000.0 + 31_000.0,
            &away,
            false
        ));
    }

    #[test]
    fn words_the_body() {
        let m = Message {
            content_type: "IMAGE".into(),
            mentions_me: true,
            ..msg()
        };
        assert_eq!(body(&m), "กล่าวถึงคุณ: รูปภาพ");
    }
}
