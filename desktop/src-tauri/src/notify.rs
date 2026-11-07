//! macOS banners for new messages, and the Dock badge.

use std::sync::{Arc, Mutex};

use line_rs::client::settings::NotificationSettings;
use tauri::{Emitter, Manager};

use crate::model::{Chat, Message};
use crate::state::{AppState, Focus};

/// The account's notification settings as LINE last sent them (LINE's
/// ตั้งค่า > การแจ้งเตือน, shared by every device). Empty until read: every
/// switch then counts as on.
static SETTINGS: Mutex<Option<NotificationSettings>> = Mutex::new(None);

pub fn set_settings(settings: Option<NotificationSettings>) {
    *SETTINGS.lock().unwrap() = settings;
}

fn settings() -> NotificationSettings {
    SETTINGS.lock().unwrap().clone().unwrap_or_default()
}

/// The account lets banners through: notifications on, not paused.
fn account_allows(s: &NotificationSettings, now_ms: f64) -> bool {
    NotificationSettings::on(s.enabled) && !s.paused_at(now_ms as i64)
}

fn flag(msg: &Message, key: &str) -> bool {
    msg.metadata.get(key).map(String::as_str) == Some("true")
}

/// A banner is posted when the message is not ours, arrived after launch (30 s
/// grace), was not already shown, is not in the chat the user is looking at,
/// is not a group event (someone joined or left) or one sent silently, the
/// account has notifications on and not paused, and the chat is not muted
/// unless it mentions us and mentions are on.
pub fn should_notify(
    msg: &Message,
    chat: &Chat,
    launched_ms: f64,
    focus: &Focus,
    already: bool,
    settings: &NotificationSettings,
    now_ms: f64,
) -> bool {
    if msg.mine || already || msg.unsent > 0 {
        return false;
    }
    // No banner for these. PREMIUM_MUTE_MESSAGE is LINE Premium's "send
    // without a sound" (NOTIFICATION_DISABLED comes with it).
    if msg.content_type == "CHATEVENT"
        || flag(msg, "NOTIFICATION_DISABLED")
        || flag(msg, "PREMIUM_MUTE_MESSAGE")
    {
        return false;
    }
    if msg.created_time > 0.0 && msg.created_time < launched_ms - 30_000.0 {
        return false;
    }
    if focus.active && focus.chat.as_deref() == Some(chat.id.as_str()) {
        return false;
    }
    if !account_allows(settings, now_ms) || !NotificationSettings::on(settings.new_message) {
        return false;
    }
    !chat.muted || (msg.mentions_me && NotificationSettings::on(settings.mention))
}

/// What a banner says when it can't or mustn't show the message.
const NEW_MESSAGE: &str = "ส่งข้อความถึงคุณ";

/// The banner's text. `show_message` off (LINE's "แสดงข้อความ") hides what
/// the message says, as LINE does; a message LAAB couldn't decrypt says
/// there is one rather than nothing.
pub fn body(msg: &Message, show_message: bool) -> String {
    if !show_message {
        return if msg.mentions_me {
            "กล่าวถึงคุณ".into()
        } else {
            NEW_MESSAGE.into()
        };
    }
    let base = match msg.content_type.as_str() {
        "CALL" => crate::convert::call_text(msg, "เขา"),
        "IMAGE" | "VIDEO" | "STICKER" | "AUDIO" | "FILE" | "CHATEVENT" | "FLEX" | "RICH"
        | "POSTNOTIFICATION" | "CONTACT" => crate::convert::preview_body(msg),
        _ => {
            let chars: Vec<char> = msg.text.chars().collect();
            if chars.is_empty() {
                NEW_MESSAGE.into()
            } else if chars.len() > 140 {
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
    let settings = settings();
    let now = crate::state::now_ms();
    if !should_notify(msg, chat, state.launched_ms, &focus, already, &settings, now) {
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
    let text = body(msg, NotificationSettings::on(settings.show_message));
    let chat_id = chat.id.clone();
    let app = state.app.clone();
    post(app, title, subtitle, text, chat_id);
}

/// A banner for someone's reaction to one of our messages, when the
/// account has reaction notifications on (LINE's `notificationReaction`).
/// Not in a muted chat or the chat being looked at, and not in the first
/// 30 s after launch, when the events missed while LAAB was closed arrive.
#[allow(clippy::too_many_arguments)] // one flag per rule keeps the decision table readable
pub fn should_notify_reaction(
    chat: &Chat,
    message_mine: bool,
    by_me: bool,
    focus: &Focus,
    already: bool,
    settings: &NotificationSettings,
    launched_ms: f64,
    now_ms: f64,
) -> bool {
    message_mine
        && !by_me
        && !already
        && !chat.muted
        && now_ms - launched_ms > 30_000.0
        && (!focus.active || focus.chat.as_deref() != Some(chat.id.as_str()))
        && account_allows(settings, now_ms)
        && NotificationSettings::on(settings.reaction)
}

pub fn maybe_notify_reaction(state: &Arc<AppState>, message_id: &str, reactor: &str, me: &str) {
    if state.demo {
        return;
    }
    let (chat, message, name) = {
        let store = state.store();
        let Some(message) = store.message(message_id).ok().flatten() else {
            return;
        };
        let Some(chat) = store.chat(&message.chat_id).ok().flatten() else {
            return;
        };
        let name = store.people().ok().and_then(|p| p.get(reactor).map(|p| p.0.clone()));
        (chat, message, name)
    };
    let focus = state.focus.lock().unwrap().clone();
    let already = !state
        .notified
        .lock()
        .unwrap()
        .insert(format!("reaction:{message_id}:{reactor}"));
    let settings = settings();
    let now = crate::state::now_ms();
    if !should_notify_reaction(
        &chat,
        message.mine,
        reactor == me,
        &focus,
        already,
        &settings,
        state.launched_ms,
        now,
    ) {
        return;
    }
    let title = if chat.name.is_empty() || chat.name == chat.id {
        "รีแอคชันใหม่".to_string()
    } else {
        chat.name.clone()
    };
    let who = name.unwrap_or_else(|| "มีคน".into());
    post(
        state.app.clone(),
        title,
        None,
        format!("{who} รีแอคข้อความของคุณ"),
        chat.id,
    );
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
    fn group_events_and_silent_messages_stay_quiet() {
        let chat = Chat {
            id: "c1".into(),
            ..Default::default()
        };
        let away = Focus {
            chat: Some("c2".into()),
            active: true,
        };
        let left = Message {
            content_type: "CHATEVENT".into(),
            ..msg()
        };
        let on = NotificationSettings::default();
        assert!(!should_notify(&left, &chat, 10_000_000.0, &away, false, &on, 0.0));
        let mut silent = msg();
        silent.metadata.insert("NOTIFICATION_DISABLED".into(), "true".into());
        assert!(!should_notify(&silent, &chat, 10_000_000.0, &away, false, &on, 0.0));
        let mut premium = msg();
        premium.metadata.insert("PREMIUM_MUTE_MESSAGE".into(), "true".into());
        assert!(!should_notify(&premium, &chat, 10_000_000.0, &away, false, &on, 0.0));
    }

    /// The account's settings, over a message that would otherwise notify.
    #[test]
    fn follows_the_account_settings() {
        let chat = Chat {
            id: "c1".into(),
            ..Default::default()
        };
        let muted = Chat {
            muted: true,
            ..chat.clone()
        };
        let away = Focus {
            chat: Some("c2".into()),
            active: true,
        };
        let now = 1_800_000_000_000.0;
        let mention = Message {
            mentions_me: true,
            ..msg()
        };
        let s = |f: fn(&mut NotificationSettings)| {
            let mut s = NotificationSettings::default();
            f(&mut s);
            s
        };
        let ok = |m: &Message, c: &Chat, s: &NotificationSettings| {
            should_notify(m, c, 10_000_000.0, &away, false, s, now)
        };
        // (setting, plain message, mention in a muted chat)
        let table: [(NotificationSettings, bool, bool); 7] = [
            (s(|_| {}), true, true),
            (s(|s| s.enabled = Some(true)), true, true),
            (s(|s| s.enabled = Some(false)), false, false),
            (s(|s| s.mute_expiration = 1_800_000_060_000), false, false),
            (s(|s| s.mute_expiration = 1_700_000_000_000), true, true),
            (s(|s| s.new_message = Some(false)), false, false),
            (s(|s| s.mention = Some(false)), true, false),
        ];
        for (i, (settings, plain, muted_mention)) in table.iter().enumerate() {
            assert_eq!(ok(&msg(), &chat, settings), *plain, "row {i}, plain");
            assert_eq!(ok(&mention, &muted, settings), *muted_mention, "row {i}, mention");
        }
        assert!(!ok(&msg(), &muted, &NotificationSettings::default()));
    }

    #[test]
    fn reactions_notify_only_when_asked() {
        let chat = Chat {
            id: "c1".into(),
            ..Default::default()
        };
        let away = Focus {
            chat: Some("c2".into()),
            active: true,
        };
        let on = NotificationSettings::default();
        let off = NotificationSettings {
            reaction: Some(false),
            ..Default::default()
        };
        let (launched, now) = (1_000_000.0, 1_100_000.0);
        let r = |c: &Chat, mine, by_me, s: &NotificationSettings, now| {
            should_notify_reaction(c, mine, by_me, &away, false, s, launched, now)
        };
        assert!(r(&chat, true, false, &on, now));
        assert!(!r(&chat, true, false, &off, now));
        assert!(!r(&chat, false, false, &on, now));
        assert!(!r(&chat, true, true, &on, now));
        assert!(!r(&chat, true, false, &on, launched + 5_000.0));
        let muted = Chat {
            muted: true,
            ..chat.clone()
        };
        assert!(!r(&muted, true, false, &on, now));
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
        let on = NotificationSettings::default();
        assert!(should_notify(&msg(), &chat, 10_000_000.0, &away, false, &on, 0.0));
        assert!(!should_notify(&msg(), &chat, 10_000_000.0, &away, true, &on, 0.0));
        let here = Focus {
            chat: Some("c1".into()),
            active: true,
        };
        assert!(!should_notify(&msg(), &chat, 10_000_000.0, &here, false, &on, 0.0));
        let background = Focus {
            chat: Some("c1".into()),
            active: false,
        };
        assert!(should_notify(
            &msg(),
            &chat,
            10_000_000.0,
            &background,
            false,
            &on,
            0.0
        ));
        let muted = Chat {
            muted: true,
            ..chat.clone()
        };
        assert!(!should_notify(&msg(), &muted, 10_000_000.0, &away, false, &on, 0.0));
        assert!(should_notify(
            &Message {
                mentions_me: true,
                ..msg()
            },
            &muted,
            10_000_000.0,
            &away,
            false,
            &on,
            0.0
        ));
        assert!(!should_notify(
            &msg(),
            &chat,
            10_000_000.0 + 31_000.0,
            &away,
            false,
            &on,
            0.0
        ));
    }

    #[test]
    fn words_the_body() {
        let m = Message {
            content_type: "IMAGE".into(),
            mentions_me: true,
            ..msg()
        };
        assert_eq!(body(&m, true), "กล่าวถึงคุณ: รูปภาพ");
        assert_eq!(body(&m, false), "กล่าวถึงคุณ");
        assert_eq!(body(&msg(), false), "ส่งข้อความถึงคุณ");
        // A Letter Sealing message LAAB couldn't decrypt.
        let sealed = Message {
            text: String::new(),
            ..msg()
        };
        assert_eq!(body(&sealed, true), "ส่งข้อความถึงคุณ");
    }
}
