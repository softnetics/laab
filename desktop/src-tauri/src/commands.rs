//! Tauri commands called by the React frontend.

use std::collections::HashMap;
use std::sync::Arc;

use line_rs::client::{MessageInput, ReactionType};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::convert;
use crate::line;
use crate::model::{
    ChatMember, MentionSpan, Message, SearchHit, SearchKind, Session, Sticker, StickerPack,
    Workspace,
};
use crate::state::{AppState, now_ms};
use crate::store::Draft;

type S<'a> = State<'a, Arc<AppState>>;
type R<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Reports a failed send (its kind only) and returns the error text.
fn send_failed(state: &AppState, action: crate::usage::Action, e: impl std::fmt::Display) -> String {
    let text = e.to_string();
    crate::analytics::failed(state, crate::analytics::Failure::Send(action), &text);
    text
}

#[tauri::command]
pub fn session(state: S<'_>) -> Session {
    state.session()
}

#[tauri::command]
pub async fn login_qr(state: S<'_>) -> R<()> {
    let s = state.inner().clone();
    tokio::spawn(async move {
        let _ = line::login_qr(s).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn logout(state: S<'_>) -> R<()> {
    line::logout(state.inner().clone()).await;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatList {
    chats: Vec<crate::model::Chat>,
    workspaces: Vec<Workspace>,
    pins: HashMap<String, Vec<String>>,
}

#[tauri::command]
pub fn chats(state: S<'_>) -> R<ChatList> {
    let store = state.store();
    Ok(ChatList {
        chats: store.chats().map_err(err)?,
        workspaces: store.workspaces().map_err(err)?,
        pins: store.pins().map_err(err)?,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    name: String,
    picture: Option<String>,
}

#[tauri::command]
pub fn people(state: S<'_>) -> R<HashMap<String, Person>> {
    Ok(state
        .store()
        .people()
        .map_err(err)?
        .into_iter()
        .map(|(mid, (name, picture))| (mid, Person { name, picture }))
        .collect())
}

#[tauri::command]
pub fn messages(state: S<'_>, chat_id: String, limit: Option<usize>) -> R<Vec<Message>> {
    let me = state.my_mid();
    let mut list = state
        .store()
        .messages(&chat_id, limit.unwrap_or(400))
        .map_err(err)?;
    for m in &mut list {
        m.mentions_me = m
            .mentions
            .iter()
            .any(|s| s.all || s.mid.as_deref() == Some(me.as_str()));
    }
    Ok(list)
}

#[tauri::command]
pub fn reads(state: S<'_>, chat_id: String) -> R<HashMap<String, String>> {
    state.store().reads(&chat_id).map_err(err)
}

#[tauri::command]
pub async fn members(state: S<'_>, chat_id: String, force: Option<bool>) -> R<Vec<ChatMember>> {
    let s = state.inner().clone();
    if s.demo {
        return s.store().members(&chat_id).map_err(err);
    }
    line::load_members(&s, &chat_id, force.unwrap_or(false))
        .await
        .map_err(err)
}

/// The main window tells us which chat it shows, so banners and unread
/// counts skip it.
#[tauri::command]
pub fn set_focus(state: S<'_>, chat_id: Option<String>, active: bool) {
    let mut f = state.focus.lock().unwrap();
    f.chat = chat_id;
    f.active = active;
}

/// Opening a chat: clear unread, acknowledge, then fetch fresh history,
/// read status and members in the background.
#[tauri::command]
pub async fn open_chat(state: S<'_>, chat_id: String) -> R<()> {
    crate::usage::hit(crate::usage::Action::OpenedChat);
    let s = state.inner().clone();
    let had_unread = s
        .store()
        .chat(&chat_id)
        .ok()
        .flatten()
        .is_some_and(|c| c.unread > 0);
    if had_unread {
        s.store().set_unread(&chat_id, 0).map_err(err)?;
        s.chats_changed();
    }
    if s.demo {
        return Ok(());
    }
    tokio::spawn(async move {
        let _ = line::fetch_history(&s, &chat_id).await;
        acknowledge(&s, &chat_id).await;
        let _ = line::read_status(&s, &chat_id).await;
        if chat_id.starts_with('c') || chat_id.starts_with('r') {
            let _ = line::load_members(&s, &chat_id, false).await;
        }
    });
    Ok(())
}

async fn acknowledge(state: &Arc<AppState>, chat_id: &str) {
    let Some(l) = state.line().await else { return };
    let last = state
        .store()
        .messages(chat_id, 1)
        .ok()
        .and_then(|mut m| m.pop());
    if let Some(last) = last {
        let _ = l.chat(chat_id).mark_as_read(&last.id).await;
    }
}

#[tauri::command]
pub async fn mark_read(state: S<'_>, chat_id: String) -> R<()> {
    let s = state.inner().clone();
    let _ = s.store().set_unread(&chat_id, 0);
    s.chats_changed();
    if !s.demo {
        acknowledge(&s, &chat_id).await;
        let _ = line::read_status(&s, &chat_id).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn refresh(state: S<'_>, full: Option<bool>) -> R<()> {
    crate::usage::hit(crate::usage::Action::Refreshed);
    let s = state.inner().clone();
    if s.demo {
        return Ok(());
    }
    line::refresh_chats(&s, full.unwrap_or(true))
        .await
        .map_err(|e| {
            let text = e.to_string();
            crate::analytics::failed(&s, crate::analytics::Failure::Sync, &text);
            text
        })
}

#[tauri::command]
pub async fn reload_chat(state: S<'_>, chat_id: String) -> R<()> {
    let s = state.inner().clone();
    if s.demo {
        return Ok(());
    }
    line::fetch_history(&s, &chat_id)
        .await
        .map(|_| ())
        .map_err(err)
}

fn local_message(
    state: &AppState,
    chat_id: &str,
    text: &str,
    mentions: Vec<MentionSpan>,
) -> Message {
    Message {
        id: format!("local-{}", now_ms() as u64),
        chat_id: chat_id.into(),
        text: text.into(),
        content_type: "NONE".into(),
        created_time: now_ms(),
        mine: true,
        sender_id: Some(state.my_mid()),
        mentions,
        ..Default::default()
    }
}

fn store_sent(state: &AppState, msg: &Message) {
    let _ = state.store().upsert_messages(std::slice::from_ref(msg));
    line::refresh_preview(state, &msg.chat_id);
    state.messages_changed(&msg.chat_id);
}

#[tauri::command]
pub async fn send_text(
    state: S<'_>,
    chat_id: String,
    text: String,
    mentions: Vec<MentionSpan>,
    reply_to: Option<String>,
) -> R<Message> {
    let s = state.inner().clone();
    if s.demo {
        let mut m = local_message(&s, &chat_id, &text, mentions);
        m.reply_to = reply_to;
        store_sent(&s, &m);
        return Ok(m);
    }
    let l = s.line().await.ok_or("not_logged_in")?;
    let mut input = MessageInput::text(text.clone());
    if let Some(id) = reply_to.clone() {
        input = input.reply_to(id);
    }
    if let Some(meta) = convert::mention_metadata(&mentions) {
        input = input.metadata("MENTION", meta);
    }
    let sent = l
        .client
        .send_message(&chat_id, input)
        .await
        .map_err(|e| send_failed(&s, crate::usage::Action::SentText, e))?;
    crate::usage::hit(crate::usage::Action::SentText);
    let mut m = convert::message_from_json(&sent.raw.to_json(), &l.me, true)
        .unwrap_or_else(|| local_message(&s, &chat_id, &text, mentions.clone()));
    // The send response may omit what we sent.
    m.chat_id = chat_id.clone();
    m.mine = true;
    if m.text.is_empty() {
        m.text = text;
    }
    if m.mentions.is_empty() {
        m.mentions = mentions;
    }
    if m.reply_to.is_none() {
        m.reply_to = reply_to;
    }
    if m.created_time <= 0.0 {
        m.created_time = now_ms();
    }
    store_sent(&s, &m);
    Ok(m)
}

#[tauri::command]
pub async fn send_sticker(state: S<'_>, chat_id: String, sticker: Sticker) -> R<()> {
    let s = state.inner().clone();
    if s.demo {
        return Err("ต้องเข้าสู่ระบบ LINE ก่อนส่งสติกเกอร์".into());
    }
    let l = s.line().await.ok_or("not_logged_in")?;
    let version = sticker.version.clone().unwrap_or_else(|| "1".into());
    let input =
        MessageInput::sticker(&sticker.package_id, &sticker.sticker_id, &version).e2ee(false);
    let sent = l.client.send_message(&chat_id, input).await.map_err(|e| {
        let text = send_failed(&s, crate::usage::Action::SentSticker, e);
        if text.contains("USER_NOT_STICKER_OWNER") {
            "sticker_not_owned".to_string()
        } else {
            text
        }
    })?;
    crate::usage::hit(crate::usage::Action::SentSticker);
    crate::stickers::remember(&s, &sticker);
    if let Some(mut m) = convert::message_from_json(&sent.raw.to_json(), &l.me, true) {
        m.chat_id = chat_id;
        m.mine = true;
        m.content_type = "STICKER".into();
        m.sticker_id = Some(sticker.sticker_id);
        m.sticker_package = Some(sticker.package_id);
        if m.created_time <= 0.0 {
            m.created_time = now_ms();
        }
        store_sent(&s, &m);
    }
    Ok(())
}

#[tauri::command]
pub async fn stickers(state: S<'_>) -> R<Vec<StickerPack>> {
    let s = state.inner().clone();
    if s.demo {
        return Err("ต้องเข้าสู่ระบบ LINE ก่อนใช้สติกเกอร์".into());
    }
    crate::stickers::list(&s).await.map_err(err)
}

#[tauri::command]
pub async fn send_files(state: S<'_>, chat_id: String, paths: Vec<String>) -> R<()> {
    let s = state.inner().clone();
    if s.demo {
        return Err("ต้องเข้าสู่ระบบ LINE ก่อนส่งไฟล์".into());
    }
    let l = s.line().await.ok_or("not_logged_in")?;
    let chat = l.chat(&chat_id);
    for (i, path) in paths.iter().enumerate() {
        let p = std::path::Path::new(path);
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        s.status(format!("กำลังส่ง {name} ({}/{})", i + 1, paths.len()));
        let data = tokio::fs::read(p).await.map_err(err)?;
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let result = match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" => {
                chat.send_image(data, Some(&name)).await
            }
            "mp4" | "mov" | "m4v" => chat.send_video(data, Some(&name), None).await,
            _ => chat.send_file(data, &name).await,
        };
        result.map_err(|e| {
            let text = send_failed(&s, crate::usage::Action::SentFile, e);
            format!("ส่ง {name} ไม่สำเร็จ: {text}")
        })?;
        crate::usage::hit(crate::usage::Action::SentFile);
    }
    s.status("เชื่อม LINE แล้ว");
    let _ = line::fetch_history(&s, &chat_id).await;
    Ok(())
}

#[tauri::command]
pub async fn download(
    state: S<'_>,
    chat_id: String,
    message_id: String,
    file_name: Option<String>,
) -> R<String> {
    let s = state.inner().clone();
    if let Some(Some(p)) = s
        .store()
        .message(&message_id)
        .map_err(err)?
        .map(|m| m.file_path.or(m.image_path))
        && std::path::Path::new(&p).exists()
    {
        return Ok(p);
    }
    if s.demo {
        return Err("ไม่มีไฟล์ในโหมด demo".into());
    }
    crate::media::download_file(&s, &chat_id, &message_id, file_name)
        .await
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| {
            if e.to_string().contains("file_expired") {
                "ไฟล์นี้หมดอายุแล้ว ดาวน์โหลดไม่ได้".into()
            } else {
                e.to_string()
            }
        })
}

/// Set (`Some`) or remove (`None`) our reaction.
#[tauri::command]
pub async fn react(
    state: S<'_>,
    chat_id: String,
    message_id: String,
    reaction: Option<String>,
) -> R<()> {
    crate::usage::hit(crate::usage::Action::Reacted);
    let s = state.inner().clone();
    let me = s.my_mid();
    let previous = s.store().message(&message_id).ok().flatten().and_then(|m| {
        m.reactions
            .into_iter()
            .find(|r| r.user_id == me)
            .map(|r| r.reaction)
    });
    line::apply_reaction(&s, &message_id, &me, reaction.as_deref());
    if s.demo {
        return Ok(());
    }
    let result = async {
        let l = s.line().await.ok_or("not_logged_in".to_string())?;
        if !crate::reads::is_numeric_id(&message_id) {
            return Err("bad_message_id".to_string());
        }
        let msg = l.talk_message(&message_id);
        match reaction.as_deref() {
            Some(name) => {
                let kind = ReactionType::from_name(name).ok_or("bad_reaction".to_string())?;
                msg.react(kind).await.map_err(err)
            }
            None => msg.cancel_reaction().await.map_err(err),
        }
    }
    .await;
    if let Err(e) = result {
        line::apply_reaction(&s, &message_id, &me, previous.as_deref());
        return Err(format!("ส่ง reaction ไม่สำเร็จ: {e}"));
    }
    let _ = chat_id;
    Ok(())
}

#[tauri::command]
pub async fn set_muted(state: S<'_>, chat_id: String, muted: bool) -> R<()> {
    let s = state.inner().clone();
    let before = s
        .store()
        .chat(&chat_id)
        .ok()
        .flatten()
        .map(|c| c.muted)
        .unwrap_or(false);
    s.store().set_muted(&chat_id, muted).map_err(err)?;
    s.chats_changed();
    if s.demo {
        return Ok(());
    }
    let Some(l) = s.line().await else {
        return Ok(());
    };
    let result = if chat_id.starts_with('u') {
        match l.client.get_user(&chat_id).await {
            Ok(u) => u.set_notification_disabled(muted).await.map(|_| ()),
            Err(e) => Err(e),
        }
    } else {
        l.chat(&chat_id).set_muted(muted).await.map(|_| ())
    };
    if let Err(e) = result {
        s.store().set_muted(&chat_id, before).map_err(err)?;
        s.chats_changed();
        return Err(format!("ตั้งค่าการแจ้งเตือนไม่สำเร็จ: {e}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn set_pinned(
    state: S<'_>,
    chat_id: String,
    workspace_id: String,
    pinned: bool,
) -> R<()> {
    state
        .store()
        .set_pinned(pinned, &chat_id, &workspace_id)
        .map_err(err)?;
    state.chats_changed();
    // Pins (not folders) follow to LINE's other devices. Best effort: the
    // call is unverified, and the local pin stands either way.
    if workspace_id == "main" && !state.demo {
        let s = state.inner().clone();
        tokio::spawn(async move {
            if let Err(e) = crate::actions::set_server_pin(&s, &chat_id, pinned).await {
                tracing::warn!("server pin {chat_id}: {e}");
            }
        });
    }
    Ok(())
}

#[tauri::command]
pub fn add_workspace(state: S<'_>, name: String) -> R<Workspace> {
    let w = state.store().add_workspace(name.trim()).map_err(err)?;
    state.chats_changed();
    Ok(w)
}

#[tauri::command]
pub fn create_folder(state: S<'_>, name: String, chat_ids: Vec<String>, from: Option<String>) -> R<Workspace> {
    crate::usage::hit(crate::usage::Action::CreatedFolder);
    let w = state
        .store()
        .create_folder(name.trim(), &chat_ids, from.as_deref())
        .map_err(err)?;
    state.chats_changed();
    Ok(w)
}

#[tauri::command]
pub fn move_to_folder(state: S<'_>, chat_id: String, from: Option<String>, to: Option<String>) -> R<()> {
    state
        .store()
        .move_to_folder(&chat_id, from.as_deref(), to.as_deref())
        .map_err(err)?;
    state.chats_changed();
    Ok(())
}

#[tauri::command]
pub fn rename_folder(state: S<'_>, id: String, name: String) -> R<()> {
    state.store().rename_workspace(&id, name.trim()).map_err(err)?;
    state.chats_changed();
    Ok(())
}

#[tauri::command]
pub fn delete_folder(state: S<'_>, id: String) -> R<()> {
    state.store().delete_workspace(&id).map_err(err)?;
    state.chats_changed();
    Ok(())
}

#[tauri::command]
pub fn search(state: S<'_>, query: String, kinds: Vec<SearchKind>) -> R<Vec<SearchHit>> {
    crate::usage::hit(crate::usage::Action::Searched);
    state.store().search(&query, &kinds).map_err(err)
}

#[tauri::command]
pub fn draft(state: S<'_>, chat_id: String) -> R<Option<Draft>> {
    state.store().draft(&chat_id).map_err(err)
}

#[tauri::command]
pub fn set_draft(state: S<'_>, chat_id: String, draft: Draft) -> R<()> {
    state.store().set_draft(&chat_id, &draft).map_err(err)
}

/// The raw LINE message, for the developer inspector.
#[tauri::command]
pub async fn raw_message(state: S<'_>, chat_id: String, message_id: String) -> R<String> {
    let s = state.inner().clone();
    if s.demo {
        return Err("ไม่มีข้อมูลจาก LINE ในโหมด demo".into());
    }
    line::raw_message(&s, &chat_id, &message_id)
        .await
        .map_err(|e| {
            if e.to_string() == "not_in_recent_100" {
                "ไม่พบในข้อความล่าสุด 100 ข้อความของแชทนี้".into()
            } else {
                format!("โหลดไม่สำเร็จ: {e}")
            }
        })
}

/// Flex postback. `strict` never falls back to posting the data as text.
#[tauri::command]
pub async fn send_postback(
    state: S<'_>,
    chat_id: String,
    message_id: String,
    data: String,
    display_text: Option<String>,
) -> R<()> {
    let s = state.inner().clone();
    if s.demo {
        return Err("ต้องเข้าสู่ระบบ LINE ก่อน".into());
    }
    let l = s.line().await.ok_or("not_logged_in")?;
    let url = format!("line://postback?data={}", urlencode(&data));
    let msg = l.talk_message(&message_id);
    let from = msg.raw["from"]
        .as_str()
        .or(msg.raw["_from"].as_str())
        .unwrap_or(&chat_id)
        .to_string();
    l.client
        .base
        .talk()
        .send_flex_postback(&message_id, &url, &chat_id, &from)
        .await
        .map_err(|e| format!("ส่งคำตอบให้บอทไม่สำเร็จ: {e}"))?;
    if let Some(text) = display_text.filter(|t| !t.is_empty()) {
        l.client
            .send_message(&chat_id, MessageInput::text(text))
            .await
            .map_err(err)?;
        let _ = line::fetch_history(&s, &chat_id).await;
    }
    Ok(())
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiffOpen {
    url: String,
    chat_id: Option<String>,
    consent: Option<bool>,
    web: Option<bool>,
}

/// Open a LIFF link. Errors with `liff_consent_required` until the user agrees.
#[tauri::command]
pub async fn open_liff(state: S<'_>, req: LiffOpen) -> R<()> {
    crate::usage::hit(crate::usage::Action::OpenedLiff);
    let s = state.inner().clone();
    let link = crate::liff::parse_link(&req.url).ok_or("invalid_liff_id")?;
    let view = if s.demo || req.web.unwrap_or(false) {
        crate::liff::LiffView {
            liff_id: link.id.clone(),
            url: link.web_url(),
            mode: "web".into(),
            reason: Some(if s.demo {
                "โหมด demo".into()
            } else {
                "ไม่ได้อนุญาตสิทธิ์".into()
            }),
            info: serde_json::json!({ "requestedUrl": req.url }),
        }
    } else {
        crate::liff::issue(
            &s,
            &req.url,
            req.chat_id.clone(),
            req.consent.unwrap_or(false),
        )
        .await
        .map_err(err)?
    };
    if let Some(reason) = &view.reason {
        s.status(format!("เปิดเป็นหน้าเว็บ: {reason}"));
    }
    crate::liff::open_window(&s, &view, req.chat_id).map_err(err)
}

/// Every LIFF window opened this run, with its log.
#[tauri::command]
pub fn liff_dev_sessions() -> Vec<crate::liff_devtools::Session> {
    crate::liff_devtools::sessions()
}

/// Run JavaScript in a LIFF window; the result goes to its log.
#[tauri::command]
pub fn liff_dev_eval(app: tauri::AppHandle, label: String, code: String) -> R<()> {
    crate::liff_devtools::eval(&app, &label, &code).map_err(err)
}

#[tauri::command]
pub fn open_liff_devtools(app: tauri::AppHandle, focus: Option<String>) -> R<()> {
    crate::liff_devtools::open(&app, focus.as_deref()).map_err(err)
}

/// `reload`, `inspect`, `close`, `reopen` (fresh tokens) or `web` (as a
/// plain page) for the LIFF window `label`.
#[tauri::command]
pub fn liff_dev_action(state: S<'_>, label: String, action: String) -> R<()> {
    let s = state.inner().clone();
    let session = crate::liff_devtools::session(&label).ok_or("ไม่พบหน้าต่าง LIFF นี้")?;
    let requested = session.info["requestedUrl"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let window = || {
        s.app
            .get_webview_window(&label)
            .ok_or_else(|| "หน้าต่างนี้ปิดไปแล้ว".to_string())
    };
    match action.as_str() {
        "reload" => window()?.eval("location.reload()").map_err(err),
        "close" => window()?.close().map_err(err),
        "inspect" => {
            #[cfg(debug_assertions)]
            {
                window()?.open_devtools();
                Ok(())
            }
            #[cfg(not(debug_assertions))]
            {
                window()?;
                Err("Web Inspector เปิดได้เฉพาะ dev build".into())
            }
        }
        "reopen" => {
            crate::liff::open_external_or_liff(&s, &requested, session.chat_id);
            Ok(())
        }
        "web" => {
            let link = crate::liff::parse_link(&requested).ok_or("invalid_liff_id")?;
            let view = crate::liff::LiffView {
                liff_id: link.id.clone(),
                url: link.web_url(),
                mode: "web".into(),
                reason: Some("เปิดจาก LIFF DevTools".into()),
                info: serde_json::json!({ "requestedUrl": requested }),
            };
            crate::liff::open_window(&s, &view, session.chat_id).map_err(err)
        }
        _ => Err(format!("unknown action {action}")),
    }
}

#[tauri::command]
pub async fn liff_share(
    state: S<'_>,
    window: String,
    liff_id: String,
    ott: String,
    chat_ids: Vec<String>,
    messages: serde_json::Value,
) -> R<crate::liff::ShareResult> {
    let s = state.inner().clone();
    let result = if chat_ids.is_empty() {
        crate::liff::ShareResult {
            sent: 0,
            failed: vec![],
        }
    } else {
        crate::liff::share(&s, &liff_id, &chat_ids, &messages)
            .await
            .map_err(err)?
    };
    let _ = crate::liff::picker_result(&liff_id, &ott, result.sent > 0).await;
    if let Some(w) = s.app.get_webview_window(&window) {
        let _ = w.set_focus();
    }
    Ok(result)
}

/// Open a chat in its own window.
#[tauri::command]
pub fn open_chat_window(app: tauri::AppHandle, chat_id: String, title: String) -> R<()> {
    let label = format!("chat-{}", crate::media::safe_name(&chat_id));
    if let Some(w) = app.get_webview_window(&label) {
        return w.set_focus().map_err(err);
    }
    WebviewWindowBuilder::new(
        &app,
        &label,
        WebviewUrl::App(format!("index.html#/window/{chat_id}").into()),
    )
    .title(title)
    .inner_size(460.0, 760.0)
    .min_inner_size(420.0, 420.0)
    .title_bar_style(tauri::TitleBarStyle::Overlay)
    .hidden_title(true)
    .build()
    .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub fn is_demo(state: S<'_>) -> bool {
    state.demo
}

#[tauri::command]
pub fn analytics_status(state: S<'_>) -> crate::analytics::Status {
    crate::analytics::status(&state)
}

#[tauri::command]
pub fn set_analytics(state: S<'_>, on: bool) -> R<()> {
    crate::analytics::set(&state, on).map_err(err)
}

/// The problem report as it would be sent, for the user to read and edit.
#[tauri::command]
pub fn report_preview(state: S<'_>, req: crate::report::Request) -> String {
    crate::report::preview(&state, &req)
}

#[tauri::command]
pub fn send_report(state: S<'_>, report: String) -> R<()> {
    crate::analytics::send_report(&state, report).map_err(err)
}

/// An uncaught webview error. Only `name` and the frames of `stack` that
/// point into LAAB's scripts are kept; see `crash::from_js`.
#[tauri::command]
pub fn report_js_error(state: S<'_>, name: String, stack: String) {
    crate::analytics::send(
        &state,
        crate::analytics::Event::Exception(crate::crash::from_js(&name, &stack)),
    );
}

fn inside_data(state: &AppState, path: &str) -> R<std::path::PathBuf> {
    let p = std::path::PathBuf::from(path);
    if p.starts_with(&state.paths.root) && p.exists() {
        Ok(p)
    } else {
        Err("ไม่พบไฟล์".into())
    }
}

/// Open a downloaded attachment with its default app. An encrypted cache
/// file opens as a plain temp copy, deleted when LAAB quits.
#[tauri::command]
pub fn open_path(state: S<'_>, path: String) -> R<()> {
    let p = inside_data(&state, &path)?;
    let plain = crate::cache::plain_copy(&p).map_err(err)?;
    tauri_plugin_opener::open_path(plain, None::<&str>).map_err(err)
}

/// Show an attachment in Finder. An encrypted cache file means nothing in
/// Finder, so a plain copy is saved to Downloads and shown there.
#[tauri::command]
pub fn reveal_path(state: S<'_>, path: String) -> R<()> {
    let p = inside_data(&state, &path)?;
    let shown = if crate::vault::is_encrypted(&p) {
        crate::cache::export_to_downloads(&p).map_err(err)?
    } else {
        p
    };
    tauri_plugin_opener::reveal_item_in_dir(shown).map_err(err)
}

/// Bytes used by the cache.
#[tauri::command]
pub fn cache_size(state: S<'_>) -> u64 {
    crate::cache::usage(&state)
}

#[tauri::command]
pub fn clear_cache(state: S<'_>) -> R<()> {
    crate::cache::clear(&state).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Announcement {
    seq: String,
    text: String,
    link: Option<String>,
    creator: Option<String>,
    created_time: f64,
}

/// Pinned announcements shown as a banner over the chat, newest first.
#[tauri::command]
pub async fn announcements(state: S<'_>, chat_id: String) -> R<Vec<Announcement>> {
    let s = state.inner().clone();
    if s.demo {
        return Ok(if chat_id == "c_harvest" {
            vec![Announcement {
                seq: "1".into(),
                text: "เดโมให้ลูกค้าพรุ่งนี้ 14:00 ห้องประชุมใหญ่".into(),
                link: None,
                creator: Some("me".into()),
                created_time: now_ms(),
            }]
        } else {
            vec![]
        });
    }
    if chat_id.starts_with('u') {
        return Ok(vec![]);
    }
    let Some(l) = s.line().await else {
        return Ok(vec![]);
    };
    let list = l.chat(&chat_id).announcements().await.map_err(|e| {
        tracing::warn!("announcements {chat_id}: {e:?}");
        err(e)
    })?;
    let mut out: Vec<Announcement> = list
        .iter()
        .map(|v| v.to_json())
        .filter_map(|j| {
            Some(Announcement {
                seq: convert::str_of(&j["announcementSeq"])?,
                text: j["contents"]["text"].as_str().unwrap_or("").to_string(),
                link: convert::str_of(&j["contents"]["link"]),
                creator: convert::str_of(&j["creatorMid"]),
                created_time: convert::num_of(&j["createdTime"]).unwrap_or(0.0),
            })
        })
        .collect();
    out.sort_by(|a, b| b.created_time.total_cmp(&a.created_time));
    Ok(out)
}

/// The rich menu of an official account, or `None` if it has none.
#[tauri::command]
pub async fn rich_menu(state: S<'_>, chat_id: String) -> R<Option<crate::richmenu::RichMenu>> {
    let s = state.inner().clone();
    if s.demo {
        return Ok((chat_id == "b_parcel").then(crate::richmenu::demo));
    }
    let Some(l) = s.line().await else {
        return Ok(None);
    };
    let buddy = l.client.base.buddy();
    // The menu can differ per user; the personal detail names ours.
    let detail = buddy
        .get_buddy_detail_with_personal(
            line_rs::tv!({ "buddyMid": chat_id.clone(), "attributeSet": [0] }),
        )
        .await
        .map_err(|e| {
            tracing::warn!("buddy detail {chat_id}: {e:?}");
            err(e)
        })?
        .to_json();
    let Some(menu_id) = detail["personalDetail"]["richMenuId"]
        .as_str()
        .filter(|id| !id.is_empty())
    else {
        return Ok(None);
    };
    let bar = buddy
        .call(
            "getBuddyChatBarV2",
            line_rs::tv!({ "request": { "buddyMid": chat_id.clone(), "chatBarRevision": 0, "richMenuId": menu_id } }),
        )
        .await
        .map_err(|e| {
            tracing::warn!("chat bar {chat_id}: {e:?}");
            err(e)
        })?;
    Ok(crate::richmenu::from_chat_bar(&bar.to_json()))
}

// ---------------------------------------------------------------- line-rs extras

macro_rules! live {
    ($state:expr) => {{
        let s = $state.inner().clone();
        if s.demo {
            return Err("ต้องเข้าสู่ระบบ LINE ก่อน".into());
        }
        s
    }};
}

#[tauri::command]
pub async fn login_password(state: S<'_>, identifier: String, password: String) -> R<()> {
    let s = state.inner().clone();
    tokio::spawn(async move {
        let _ = line::login_password(s, identifier, password).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn unsend(state: S<'_>, message_id: String) -> R<()> {
    let s = state.inner().clone();
    if s.demo {
        line::apply_unsend(&s, &message_id);
        return Ok(());
    }
    crate::actions::unsend(&s, &message_id)
        .await
        .map_err(|e| format!("ยกเลิกข้อความไม่สำเร็จ: {e}"))
}

#[tauri::command]
pub async fn edit_message(
    state: S<'_>,
    chat_id: String,
    message_id: String,
    text: String,
) -> R<()> {
    let s = live!(state);
    crate::actions::edit(&s, &chat_id, &message_id, &text)
        .await
        .map_err(|e| format!("แก้ไขข้อความไม่สำเร็จ: {e}"))
}

#[tauri::command]
pub async fn translate(state: S<'_>, text: String, target: String) -> R<String> {
    crate::usage::hit(crate::usage::Action::Translated);
    let s = live!(state);
    crate::actions::translate(&s, &text, &target)
        .await
        .map_err(|e| format!("แปลไม่สำเร็จ: {e}"))
}

#[tauri::command]
pub async fn save_to_keep(state: S<'_>, chat_id: String, message_id: String) -> R<()> {
    let s = live!(state);
    crate::actions::save_to_keep(&s, &chat_id, &message_id)
        .await
        .map_err(|e| format!("บันทึกลง Keep ไม่สำเร็จ: {e}"))
}

#[tauri::command]
pub async fn hide_chat(state: S<'_>, chat_id: String) -> R<()> {
    let s = state.inner().clone();
    if s.demo {
        s.store().hide_chat(&chat_id).map_err(err)?;
        s.chats_changed();
        return Ok(());
    }
    crate::actions::hide_chat(&s, &chat_id)
        .await
        .map_err(|e| format!("ซ่อนแชทไม่สำเร็จ: {e}"))
}

#[tauri::command]
pub async fn delete_history(state: S<'_>, chat_id: String) -> R<()> {
    let s = state.inner().clone();
    if s.demo {
        s.store().delete_history(&chat_id).map_err(err)?;
        s.messages_changed(&chat_id);
        s.chats_changed();
        return Ok(());
    }
    crate::actions::delete_history(&s, &chat_id)
        .await
        .map_err(|e| format!("ลบประวัติแชทไม่สำเร็จ: {e}"))
}

#[tauri::command]
pub async fn remove_announcement(state: S<'_>, chat_id: String, seq: i64) -> R<()> {
    let s = live!(state);
    crate::actions::remove_announcement(&s, &chat_id, seq)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn announce(state: S<'_>, chat_id: String, message_id: String, text: String) -> R<()> {
    let s = live!(state);
    crate::actions::announce(&s, &chat_id, &message_id, &text)
        .await
        .map_err(|e| format!("ตั้งประกาศไม่สำเร็จ: {e}"))
}

#[tauri::command]
pub async fn set_blocked(state: S<'_>, mid: String, blocked: bool) -> R<()> {
    let s = live!(state);
    crate::actions::set_blocked(&s, &mid, blocked)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn find_contact(state: S<'_>, query: String) -> R<crate::actions::FoundContact> {
    let s = live!(state);
    crate::actions::find_contact(&s, &query).await.map_err(err)
}

#[tauri::command]
pub async fn contact_profile(state: S<'_>, mid: String) -> R<crate::actions::ContactProfile> {
    let s = live!(state);
    crate::actions::contact_profile(&s, &mid).await.map_err(err)
}

#[tauri::command]
pub async fn add_friend(state: S<'_>, mid: String) -> R<()> {
    let s = live!(state);
    crate::actions::add_friend(&s, &mid)
        .await
        .map_err(|e| format!("เพิ่มเพื่อนไม่สำเร็จ: {e}"))
}

#[tauri::command]
pub async fn join_group_link(state: S<'_>, url: String) -> R<String> {
    let s = live!(state);
    crate::actions::join_group_link(&s, &url).await.map_err(err)
}

#[tauri::command]
pub async fn list_notes(state: S<'_>, chat_id: String) -> R<Vec<crate::actions::NotePost>> {
    let s = live!(state);
    crate::actions::list_notes(&s, &chat_id).await.map_err(err)
}

#[tauri::command]
pub async fn create_note(
    state: S<'_>,
    chat_id: String,
    text: String,
    photos: Option<Vec<String>>,
) -> R<()> {
    let s = live!(state);
    crate::actions::create_note(&s, &chat_id, &text, &photos.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_note(state: S<'_>, chat_id: String, post_id: String) -> R<()> {
    let s = live!(state);
    crate::actions::delete_note(&s, &chat_id, &post_id)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn list_albums(state: S<'_>, chat_id: String) -> R<Vec<crate::actions::Album>> {
    let s = live!(state);
    crate::actions::list_albums(&s, &chat_id).await.map_err(err)
}

#[tauri::command]
pub async fn album_photos(
    state: S<'_>,
    chat_id: String,
    album_id: String,
    limit: Option<u32>,
) -> R<Vec<crate::actions::AlbumPhoto>> {
    let s = live!(state);
    crate::actions::album_photos(&s, &chat_id, &album_id, limit.unwrap_or(1000))
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn create_album(state: S<'_>, chat_id: String, title: String) -> R<()> {
    let s = live!(state);
    crate::actions::create_album(&s, &chat_id, &title)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn rename_album(state: S<'_>, chat_id: String, album_id: String, title: String) -> R<()> {
    let s = live!(state);
    crate::actions::rename_album(&s, &chat_id, &album_id, &title)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_album(state: S<'_>, chat_id: String, album_id: String) -> R<()> {
    let s = live!(state);
    crate::actions::delete_album(&s, &chat_id, &album_id)
        .await
        .map_err(err)
}

/// Delete a message from this device only, as LINE's "ลบ" does.
#[tauri::command]
pub fn delete_message(state: S<'_>, message_id: String) -> R<()> {
    let chat = state.store().delete_message(&message_id).map_err(err)?;
    if let Some(chat) = chat {
        line::refresh_preview(&state, &chat);
        state.messages_changed(&chat);
    }
    Ok(())
}

#[tauri::command]
pub async fn leave_group(state: S<'_>, chat_id: String) -> R<()> {
    let s = live!(state);
    let l = s.line().await.ok_or("not_logged_in")?;
    l.chat(&chat_id)
        .leave()
        .await
        .map_err(|e| format!("ออกจากกลุ่มไม่สำเร็จ: {e}"))?;
    s.store().hide_chat(&chat_id).map_err(err)?;
    s.chats_changed();
    Ok(())
}

/// Photos & videos (`media`), files (`files`) or links (`links`) of a chat.
#[tauri::command]
pub fn chat_media(state: S<'_>, chat_id: String, kind: String) -> R<Vec<Message>> {
    state.store().chat_media(&chat_id, &kind).map_err(err)
}

/// Write text the user chose to save (บันทึกแชท) to the path they picked.
#[tauri::command]
pub fn save_text(path: String, text: String) -> R<()> {
    std::fs::write(&path, text).map_err(err)
}

/// Save an attachment: to Downloads (`dest` empty) or to the chosen path.
#[tauri::command]
pub async fn save_attachment(
    state: S<'_>,
    chat_id: String,
    message_id: String,
    dest: Option<String>,
) -> R<String> {
    let s = state.inner().clone();
    let m = s
        .store()
        .message(&message_id)
        .map_err(err)?
        .ok_or("ไม่พบข้อความ")?;
    let src = match m
        .file_path
        .clone()
        .or(m.image_path.clone())
        .filter(|p| std::path::Path::new(p).exists())
    {
        Some(p) => std::path::PathBuf::from(p),
        None if s.demo => return Err("ไม่มีไฟล์ในโหมด demo".into()),
        None => crate::media::download_file(&s, &chat_id, &message_id, m.file_name.clone())
            .await
            .map_err(err)?,
    };
    let target = match dest.filter(|d| !d.is_empty()) {
        Some(d) => std::path::PathBuf::from(d),
        None => {
            let name = m.file_name.clone().unwrap_or_else(|| {
                let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
                format!("LINE_{}.{ext}", message_id)
            });
            dirs::download_dir()
                .ok_or("ไม่พบโฟลเดอร์ดาวน์โหลด")?
                .join(name)
        }
    };
    crate::vault::export(&src, &target).map_err(err)?;
    Ok(target.to_string_lossy().into_owned())
}

/// Open a small LAAB window, like LINE's separate albums/notes and media windows.
/// The hash route of an albums/notes or media window. The tab, and the note
/// "ดูโน้ต" asked for, travel in the query; `at` is a fresh stamp, so asking
/// again for the note already in the URL still reopens it.
fn aux_route(
    kind: &str,
    chat_id: &str,
    tab: Option<String>,
    post: Option<String>,
    at: u64,
) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in [("tab", tab), ("post", post)] {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            query.append_pair(k, &v);
        }
    }
    query.append_pair("at", &at.to_string());
    let chat: String = url::form_urlencoded::byte_serialize(chat_id.as_bytes()).collect();
    format!("/{kind}/{chat}?{}", query.finish())
}

#[tauri::command]
pub fn open_aux_window(
    app: tauri::AppHandle,
    kind: String,
    chat_id: String,
    title: String,
    tab: Option<String>,
    post: Option<String>,
) -> R<()> {
    let label = format!(
        "aux-{}-{}",
        crate::media::safe_name(&kind),
        crate::media::safe_name(&chat_id)
    );
    let route = aux_route(&kind, &chat_id, tab, post, now_ms() as u64);
    if let Some(w) = app.get_webview_window(&label) {
        // Already open: move it to the asked-for tab or note.
        let hash = serde_json::to_string(&route).map_err(err)?;
        w.eval(format!("location.hash = {hash}")).map_err(err)?;
        let _ = w.unminimize();
        return w.set_focus().map_err(err);
    }
    let (w, h) = if kind == "media" {
        (420.0, 640.0)
    } else {
        (327.0, 643.0)
    };
    WebviewWindowBuilder::new(
        &app,
        &label,
        WebviewUrl::App(format!("index.html#{route}").into()),
    )
    .title(title)
    .inner_size(w, h)
    .min_inner_size(300.0, 400.0)
    .title_bar_style(tauri::TitleBarStyle::Overlay)
    .hidden_title(true)
    .build()
    .map_err(err)?;
    Ok(())
}

/// The page of history before the oldest stored message; 0 at its start.
#[tauri::command]
pub async fn older_messages(state: S<'_>, chat_id: String) -> R<usize> {
    if state.demo {
        return Ok(0);
    }
    let s = state.inner().clone();
    line::fetch_older(&s, &chat_id).await.map_err(err)
}

/// Show photos and videos in LAAB's viewer window, opening it on first use.
#[tauri::command]
pub fn open_viewer(app: tauri::AppHandle, state: S<'_>, payload: serde_json::Value) -> R<()> {
    use tauri::Emitter;
    *state.viewer.lock().unwrap_or_else(|e| e.into_inner()) = payload.clone();
    if let Some(w) = app.get_webview_window("viewer") {
        w.emit("viewer-show", payload).map_err(err)?;
        let _ = w.unminimize();
        return w.set_focus().map_err(err);
    }
    WebviewWindowBuilder::new(&app, "viewer", WebviewUrl::App("index.html#/viewer".into()))
        // The page retitles it with the chat's name, as LINE does.
        .title("รูปภาพ")
        .inner_size(900.0, 760.0)
        .min_inner_size(480.0, 360.0)
        .theme(Some(tauri::Theme::Dark))
        .build()
        .map_err(err)?;
    Ok(())
}

/// The viewer's "›" next to the date: show that message in the main window.
#[tauri::command]
pub fn show_message(app: tauri::AppHandle, chat_id: String, message_id: String) {
    crate::show_in_main(&app, &chat_id, Some(&message_id));
}

#[tauri::command]
pub fn viewer_payload(state: S<'_>) -> serde_json::Value {
    state
        .viewer
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[tauri::command]
pub fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub async fn note_detail(
    state: S<'_>,
    chat_id: String,
    post_id: String,
) -> R<crate::actions::NoteDetail> {
    let s = live!(state);
    crate::actions::note_detail(&s, &chat_id, &post_id)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn add_comment(state: S<'_>, chat_id: String, post_id: String, text: String) -> R<()> {
    let s = live!(state);
    crate::actions::add_comment(&s, &chat_id, &post_id, &text)
        .await
        .map_err(|e| format!("แสดงความคิดเห็นไม่สำเร็จ: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aux_route_carries_the_tab_and_note() {
        assert_eq!(
            aux_route(
                "albums",
                "c1",
                Some("notes".into()),
                Some("1179264411713515628".into()),
                5
            ),
            "/albums/c1?tab=notes&post=1179264411713515628&at=5"
        );
        assert_eq!(
            aux_route("media", "c1", None, Some(String::new()), 6),
            "/media/c1?at=6"
        );
    }
}
