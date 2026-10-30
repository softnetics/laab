//! The LINE session: login, the live event loop, chat list and history sync.
//! Built on line-rs.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow};
use line_rs::client::{
    Chat as LineChat, Client, ClientEvent as LineEvent, InitOptions, ListenHandle, ListenOptions,
    TalkEvent, TalkMessage, login_with_auth_token, login_with_qr,
};
use line_rs::service::talk::GetChatsOptions;
use line_rs::thrift::Value;
use line_rs::{Device, tv};
use serde_json::Value as Json;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::AbortHandle;

use crate::convert::{self, num_of, str_of};
use crate::model::{Chat, ChatMember, Message, Profile};
use crate::reads;
use crate::session::{self, SessionStorage};
use crate::state::{AppState, now_ms};

/// The device the Deno sidecar used. It does not log out LINE for Mac.
pub const DEFAULT_DEVICE: Device = Device::AndroidSecondary;
const HISTORY_LIMIT: i32 = 100;

pub struct Line {
    pub client: Client,
    pub me: String,
    listen: Mutex<Option<ListenHandle>>,
    tasks: Mutex<Vec<AbortHandle>>,
    /// Message boxes by chat mid, from the last chat-list refresh. They hold
    /// the history cursor.
    boxes: Mutex<HashMap<String, Value>>,
    /// Raw (decrypted) media messages, kept so attachments can be downloaded.
    media: Mutex<HashMap<String, Value>>,
    syncing: AtomicBool,
}

impl Drop for Line {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Line {
    fn stop(&self) {
        if let Some(h) = self.listen.lock().unwrap().take() {
            h.abort();
        }
        for t in self.tasks.lock().unwrap().drain(..) {
            t.abort();
        }
    }

    pub fn chat(&self, mid: &str) -> LineChat {
        LineChat::new(self.client.clone(), tv!({ "chatMid": mid }))
    }

    pub fn talk_message(&self, id: &str) -> TalkMessage {
        let raw = self
            .media
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .unwrap_or_else(|| tv!({ "id": id }));
        TalkMessage::new(self.client.clone(), raw)
    }

    fn remember_media(&self, raw: &Value, content_type: &str) {
        if matches!(content_type, "IMAGE" | "VIDEO" | "AUDIO" | "FILE")
            && let Some(id) = raw["id"].as_str()
        {
            self.media
                .lock()
                .unwrap()
                .insert(id.to_string(), raw.clone());
        }
    }

    pub fn has_media(&self, id: &str) -> bool {
        self.media.lock().unwrap().contains_key(id)
    }
}

// ---------------------------------------------------------------------------
// Session start and login
// ---------------------------------------------------------------------------

fn open_storage(state: &Arc<AppState>) -> SessionStorage {
    SessionStorage(state.clone())
}

fn auth_error(e: &line_rs::Error) -> bool {
    let text = format!("{e:?} {e}");
    [
        "NOT_AUTHORIZED",
        "LOGGED_OUT",
        "AUTHENTICATION",
        "V3_TOKEN_CLIENT_LOGGED_OUT",
        "NOT_AUTHORIZED_DEVICE",
    ]
    .iter()
    .any(|k| text.contains(k))
        || token_dead(&text)
}

/// Whether an error says the access token expired and LINE will not renew
/// it: the `refresh` call itself was refused (other than "try again"), or a
/// request still asks for a refresh after line-rs tried one. line-rs ends the
/// session itself only for the logout reasons, not for these.
fn token_dead(text: &str) -> bool {
    (text.contains("request refresh(") && !text.contains("RETRY_REQUIRED"))
        || text.contains("MUST_REFRESH_V3_TOKEN")
}

const EXPIRED: &str = "session หมดอายุ เข้าสู่ระบบใหม่";

/// The session `line` belongs to is over: stop it, forget the token and
/// show the login screen. The cache and the E2EE keys stay for the next
/// login. Does nothing if another login has replaced `line` already.
async fn session_ended(state: Arc<AppState>, line: Arc<Line>, status: &str) {
    {
        let mut current = state.line.write().await;
        if !current.as_ref().is_some_and(|c| Arc::ptr_eq(c, &line)) {
            return;
        }
        current.take();
    }
    line.stop();
    clear_auth(&state);
    state.update_session(|s| {
        *s = crate::model::Session {
            state: "signed_out".into(),
            status: status.into(),
            ..Default::default()
        };
    });
}

/// Ask the server whether the session still works. A dead token ends it
/// here; a logout reason makes line-rs emit `KickedOut`; a network error
/// changes nothing.
async fn check_session(state: Arc<AppState>, line: Arc<Line>) {
    if let Err(e) = line.client.base.talk().get_server_time().await
        && token_dead(&e.to_string())
    {
        tracing::warn!("session expired: {e}");
        session_ended(state, line, EXPIRED).await;
    }
}

/// Resume the saved session, or show the signed-out screen.
pub async fn start(state: Arc<AppState>) {
    let saved = session::auth(&state.store()).unwrap_or_else(|e| {
        tracing::warn!("reading the saved session failed: {e:?}");
        None
    });
    let Some((token, device)) = saved else {
        state.update_session(|s| {
            s.state = "signed_out".into();
            s.status = "ยังไม่ได้เข้าสู่ระบบ".into();
        });
        return;
    };
    let device: Device = device
        .and_then(|d| d.parse().ok())
        .unwrap_or(DEFAULT_DEVICE);
    state.update_session(|s| {
        s.state = "logging_in".into();
        s.status = "กำลังเชื่อม LINE…".into();
    });
    let storage = open_storage(&state);
    match login_with_auth_token(&token, InitOptions::new(device).storage(storage)).await {
        Ok(client) => connected(state, client, device).await,
        Err(e) => {
            if auth_error(&e) {
                clear_auth(&state);
            }
            tracing::warn!("token login failed: {e:?}");
            state.update_session(|s| {
                s.state = "signed_out".into();
                s.status = if auth_error(&e) {
                    EXPIRED.into()
                } else {
                    format!("เชื่อม LINE ไม่สำเร็จ: {e}")
                };
            });
        }
    }
}

fn clear_auth(state: &AppState) {
    if let Err(e) = session::clear_auth(&state.store()) {
        tracing::warn!("clearing the auth token failed: {e:?}");
    }
}

pub async fn login_qr(state: Arc<AppState>) -> Result<()> {
    let storage = open_storage(&state);
    state.update_session(|s| {
        s.state = "logging_in".into();
        s.status = "กำลังขอ QR Code…".into();
        s.qr_url = None;
        s.pin = None;
    });
    let (a, b) = (state.clone(), state.clone());
    let result = login_with_qr(
        InitOptions::new(DEFAULT_DEVICE).storage(storage),
        move |url| {
            a.update_session(|s| {
                s.qr_url = Some(url);
                s.pin = None;
                s.status = "เปิด LINE บนโทรศัพท์ แล้วสแกน QR นี้".into();
            })
        },
        move |pin| {
            b.update_session(|s| {
                s.pin = Some(pin);
                s.status = "ใส่รหัสนี้ใน LINE บนโทรศัพท์".into();
            })
        },
    )
    .await;
    match result {
        Ok(client) => {
            connected(state, client, DEFAULT_DEVICE).await;
            Ok(())
        }
        Err(e) => {
            state.update_session(|s| {
                s.state = "signed_out".into();
                s.qr_url = None;
                s.pin = None;
                s.status = format!("เข้าสู่ระบบไม่สำเร็จ: {e}");
            });
            Err(anyhow!("{e}"))
        }
    }
}

/// Email or phone number (e.g. `+66812345678`) and password. LINE asks the
/// phone to confirm with a PIN, shown like the QR flow's. Password login
/// uses the Windows desktop device, as laab-gpui does; it signs out LINE for
/// Windows, not LINE for Mac.
pub async fn login_password(
    state: Arc<AppState>,
    identifier: String,
    password: String,
) -> Result<()> {
    let storage = open_storage(&state);
    state.update_session(|s| {
        s.state = "logging_in".into();
        s.status = "กำลังเข้าสู่ระบบ…".into();
        s.qr_url = None;
        s.pin = None;
    });
    let pin = state.clone();
    let result = line_rs::client::login_with_password(
        InitOptions::new(Device::DesktopWin).storage(storage),
        identifier.trim(),
        &password,
        None,
        move |code| {
            pin.update_session(|s| {
                s.pin = Some(code);
                s.status = "ใส่รหัสนี้ใน LINE บนโทรศัพท์".into();
            })
        },
    )
    .await;
    match result {
        Ok(client) => {
            connected(state, client, Device::DesktopWin).await;
            Ok(())
        }
        Err(e) => {
            state.update_session(|s| {
                s.state = "signed_out".into();
                s.pin = None;
                s.status = format!("เข้าสู่ระบบไม่สำเร็จ: {e}");
            });
            Err(anyhow!("{e}"))
        }
    }
}

pub async fn logout(state: Arc<AppState>) {
    if let Some(line) = state.line.write().await.take() {
        let _ = line.client.base.auth().logout_z().await;
        line.stop();
    }
    clear_auth(&state);
    let _ = state.store().session_clear();
    if let Err(e) = crate::cache::clear(&state) {
        tracing::warn!("clearing the cache on logout failed: {e:?}");
    }
    state.update_session(|s| {
        *s = crate::model::Session {
            state: "signed_out".into(),
            status: "ออกจากระบบแล้ว".into(),
            ..Default::default()
        };
    });
}

fn save_token(state: &AppState, token: &str, device: Device) {
    if let Err(e) = session::save_auth(&state.store(), token, device.as_str()) {
        tracing::warn!("saving the auth token failed: {e:?}");
    }
}

async fn connected(state: Arc<AppState>, client: Client, device: Device) {
    if let Some(t) = client.auth_token() {
        save_token(&state, &t, device);
    }
    let profile = client
        .base
        .profile()
        .map(|p| p.to_json())
        .unwrap_or(Json::Null);
    let me = client.mid().unwrap_or_default();
    let line = Arc::new(Line {
        client: client.clone(),
        me: me.clone(),
        listen: Mutex::new(None),
        tasks: Mutex::new(Vec::new()),
        boxes: Mutex::new(HashMap::new()),
        media: Mutex::new(HashMap::new()),
        syncing: AtomicBool::new(false),
    });
    if let Some(old) = state.line.write().await.replace(line.clone()) {
        old.stop();
    }
    let display_name = profile["displayName"].as_str().unwrap_or("").to_string();
    let picture = str_of(&profile["picturePath"]);
    {
        let mut store = state.store();
        let _ = store.upsert_users(&[(me.clone(), display_name.clone(), picture.clone(), None)]);
    }
    state.update_session(|s| {
        s.state = "signed_in".into();
        s.status = "เชื่อม LINE แล้ว".into();
        s.qr_url = None;
        s.pin = None;
        s.profile = Some(Profile {
            mid: me.clone(),
            display_name,
            picture,
            device: device.as_str().into(),
        });
    });

    spawn_forwarders(&state, &line);
    *line.listen.lock().unwrap() = Some(client.listen(ListenOptions {
        talk: true,
        square: false,
        polling: false,
    }));

    let s = state.clone();
    tokio::spawn(async move {
        // The open chat first, then the list and the background history sync.
        let open = s.focus.lock().unwrap().chat.clone();
        if let Some(chat) = open {
            let _ = fetch_history(&s, &chat).await;
        }
        if let Err(e) = refresh_chats(&s, false).await {
            s.status(format!("โหลดรายชื่อแชทไม่สำเร็จ: {e}"));
        }
        name_stored_files(&s).await;
    });
}

/// Give names to E2EE files synced before LAAB read them from the payload,
/// by decrypting the stored raw message again.
async fn name_stored_files(state: &Arc<AppState>) {
    let Some(line) = state.line().await else {
        return;
    };
    let Ok(files) = state.store().unnamed_files() else {
        return;
    };
    let mut chats = std::collections::HashSet::new();
    for (id, chat, raw) in files {
        let Some(raw) = raw_from_store(&raw) else {
            continue;
        };
        let named = with_e2ee_file_name(&line, raw).await;
        if let Some(name) = named["contentMetadata"]["FILE_NAME"].as_str()
            && state.store().set_file_name(&id, name).is_ok()
        {
            chats.insert(chat);
        }
    }
    for chat in chats {
        state.messages_changed(&chat);
    }
}

/// A raw message as stored (JSON, binary chunks in base64) back as the
/// thrift value line-rs decrypts.
fn raw_from_store(json: &str) -> Option<Value> {
    use base64::Engine;
    let mut v: serde_json::Value = serde_json::from_str(json).ok()?;
    if let Some(chunks) = v.get_mut("chunks").and_then(|c| c.as_array_mut()) {
        let b64 = base64::engine::general_purpose::STANDARD;
        let bytes: Option<Vec<Vec<u8>>> = chunks
            .iter()
            .map(|c| b64.decode(c.as_str()?).ok())
            .collect();
        let mut raw = Value::from(v.clone());
        raw.insert(
            "chunks",
            Value::List(bytes?.into_iter().map(Value::Binary).collect()),
        );
        return Some(raw);
    }
    Some(Value::from(v))
}

fn spawn_forwarders(state: &Arc<AppState>, line: &Arc<Line>) {
    let mut rx = line.client.subscribe();
    let (s, l) = (state.clone(), Arc::downgrade(line));
    let talk = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let Some(line) = l.upgrade() else { break };
                    handle_event(&s, &line, ev).await;
                }
                Err(RecvError::Lagged(n)) => tracing::warn!("dropped {n} LINE events"),
                Err(RecvError::Closed) => break,
            }
        }
    });

    let mut rx = line.client.base.subscribe();
    let (s, l) = (state.clone(), Arc::downgrade(line));
    let base = tokio::spawn(async move {
        let mut probed: Option<std::time::Instant> = None;
        loop {
            match rx.recv().await {
                Ok(line_rs::ClientEvent::UpdateAuthToken(t)) => {
                    if let Some(line) = l.upgrade() {
                        let device = s
                            .session()
                            .profile
                            .and_then(|p| p.device.parse().ok())
                            .unwrap_or(DEFAULT_DEVICE);
                        save_token(&s, &t, device);
                        drop(line);
                    }
                }
                // `session_ended` stops this task, so it runs in its own.
                Ok(line_rs::ClientEvent::KickedOut(reason)) => {
                    tracing::warn!("kicked out: {reason:?}");
                    if let Some(line) = l.upgrade() {
                        tokio::spawn(session_ended(s.clone(), line, "ถูกออกจากระบบ เข้าสู่ระบบใหม่"));
                    }
                    break;
                }
                Ok(line_rs::ClientEvent::Log { kind, data }) => {
                    let Some(line) = l.upgrade() else { break };
                    let error = data["error"].as_str().unwrap_or("");
                    if kind == "RefreshError" && token_dead(error) {
                        tracing::warn!("session expired: {error}");
                        tokio::spawn(session_ended(s.clone(), line, EXPIRED));
                        break;
                    }
                    // The push connection drops when the token dies, but
                    // also on network trouble or sleep. Ask the server which
                    // it is, at most once a minute.
                    if kind.starts_with("LegyPusherError")
                        && probed.is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
                    {
                        probed = Some(std::time::Instant::now());
                        tokio::spawn(check_session(s.clone(), line));
                    }
                }
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    });
    line.tasks
        .lock()
        .unwrap()
        .extend([talk.abort_handle(), base.abort_handle()]);
}

// ---------------------------------------------------------------------------
// Live events
// ---------------------------------------------------------------------------

async fn handle_event(state: &Arc<AppState>, line: &Arc<Line>, ev: LineEvent) {
    match ev {
        LineEvent::Message(m) | LineEvent::MessageEdit(m) => {
            let ct = m.content_type();
            line.remember_media(&m.raw, &ct);
            let json = m.raw.to_json();
            let Some(mut msg) = convert::message_from_json(&json, &line.me, true) else {
                return;
            };
            msg.content_type = ct;
            ingest_live(state, line, msg).await;
        }
        LineEvent::Talk(t) => match t {
            TalkEvent::Read {
                chat_mid,
                reader_mid,
                message_id,
            } => {
                if reader_mid == line.me {
                    return;
                }
                apply_read(state, &chat_mid, &reader_mid, &message_id);
            }
            TalkEvent::Reaction(r) => {
                let user = if r.reactor_mid.is_empty() {
                    line.me.clone()
                } else {
                    r.reactor_mid.clone()
                };
                let removed = r.detail["curr"].is_null();
                let name = if removed {
                    None
                } else {
                    Some(
                        r.reaction
                            .map(|t| t.as_str().to_string())
                            .unwrap_or("PAID".into()),
                    )
                };
                apply_reaction(state, &r.message_id, &user, name.as_deref());
            }
            TalkEvent::Unsend { message_id, .. } => apply_unsend(state, &message_id),
            TalkEvent::Invite { chat_mid, .. }
            | TalkEvent::Join { chat_mid, .. }
            | TalkEvent::Leave { chat_mid, .. }
            | TalkEvent::Kick { chat_mid, .. } => {
                state.members_loaded.lock().unwrap().remove(&chat_mid);
            }
            _ => {}
        },
        LineEvent::Event(op) => {
            let kind = op["type"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| op["type"].to_json().to_string());
            let p = |n: u8| {
                op[format!("param{n}").as_str()]
                    .as_str()
                    .unwrap_or("")
                    .to_string()
            };
            match kind.as_str() {
                // Our own unsend, from any device.
                "DESTROY_MESSAGE" => apply_unsend(state, &p(2)),
                // Our own reaction from another device (no param3).
                "SEND_REACTION" => {
                    let detail: Json = serde_json::from_str(&p(2)).unwrap_or_default();
                    let curr = &detail["curr"];
                    let name = if curr.is_null() {
                        None
                    } else {
                        Some(
                            convert::reaction_name(&curr["predefinedReactionType"])
                                .unwrap_or("PAID".into()),
                        )
                    };
                    apply_reaction(state, &p(1), &line.me, name.as_deref());
                }
                _ => {}
            }
        }
        _ => {}
    }
}

pub fn apply_read(state: &AppState, chat: &str, reader: &str, message_id: &str) {
    let fallback = {
        let store = state.store();
        store
            .messages(chat, 1)
            .ok()
            .and_then(|m| m.last().map(|m| m.id.clone()))
    };
    let Some(cursor) = reads::cursor(Some(message_id), fallback.as_deref()) else {
        return;
    };
    let store = state.store();
    let current = store.reads(chat).ok().and_then(|r| r.get(reader).cloned());
    let next = reads::later(current.as_deref(), &cursor);
    if current.as_deref() != Some(next.as_str()) {
        let _ = store.set_read(chat, reader, &next);
        drop(store);
        state.reads_changed(chat);
    }
}

pub fn apply_reaction(state: &AppState, message_id: &str, user: &str, reaction: Option<&str>) {
    let chat = state
        .store()
        .set_reaction(message_id, user, reaction)
        .ok()
        .flatten();
    if let Some(chat) = chat {
        state.messages_changed(&chat);
    }
}

pub fn apply_unsend(state: &AppState, message_id: &str) {
    let result = state.store().mark_unsent(message_id).ok().flatten();
    let Some((chat, files)) = result else { return };
    // Cached copies go too, but only files inside our own folder.
    for f in files {
        let p = std::path::Path::new(&f);
        if p.starts_with(&state.paths.root) {
            let _ = std::fs::remove_file(p);
        }
    }
    refresh_preview(state, &chat);
    state.messages_changed(&chat);
}

/// Recompute a chat's preview from its newest stored message.
pub fn refresh_preview(state: &AppState, chat_id: &str) {
    let mut store = state.store();
    let Ok(Some(mut chat)) = store.chat(chat_id) else {
        return;
    };
    let Some(last) = store.messages(chat_id, 1).ok().and_then(|mut m| m.pop()) else {
        return;
    };
    chat.preview = convert::preview_line(&last);
    chat.last_activity = chat.last_activity.max(last.created_time);
    let _ = store.upsert_chats(&[chat]);
    drop(store);
    state.chats_changed();
}

async fn ingest_live(state: &Arc<AppState>, line: &Arc<Line>, msg: Message) {
    let chat_id = msg.chat_id.clone();
    let known = state.store().contains_message(&msg.id).unwrap_or(false);
    let _ = state.store().upsert_messages(std::slice::from_ref(&msg));
    if let Some(list) = convert::parse_reactions(
        &serde_json::from_str(msg.raw.as_deref().unwrap_or("{}")).unwrap_or_default(),
    ) && !list.is_empty()
    {
        let _ = state.store().replace_reactions(&msg.id, &list);
    }
    let (focused, active) = {
        let f = state.focus.lock().unwrap();
        (f.chat.as_deref() == Some(chat_id.as_str()), f.active)
    };
    // Update or create the chat row.
    let existing = state.store().chat(&chat_id).ok().flatten();
    let mut chat = match existing {
        Some(c) => c,
        None => new_chat_row(state, line, &chat_id).await,
    };
    if !known {
        chat.preview = convert::preview_line(&msg);
        chat.last_activity = chat.last_activity.max(msg.created_time);
        if !msg.mine && !(focused && active) {
            chat.unread += 1;
        }
    }
    let _ = state.store().upsert_chats(std::slice::from_ref(&chat));
    state.messages_changed(&chat_id);
    state.chats_changed();
    if known {
        return;
    }
    if focused && active && !msg.mine {
        let _ = line.chat(&chat_id).mark_as_read(&msg.id).await;
    }
    crate::notify::maybe_notify(state, &chat, &msg);
}

async fn new_chat_row(state: &Arc<AppState>, line: &Arc<Line>, chat_id: &str) -> Chat {
    let mut chat = Chat {
        id: chat_id.into(),
        name: chat_id.into(),
        kind: kind_for(chat_id, false),
        ..Default::default()
    };
    if chat_id.starts_with('c') || chat_id.starts_with('r') {
        if let Ok(c) = line.client.get_chat(chat_id).await {
            apply_group(&mut chat, &c.raw.to_json());
        }
    } else if let Some((name, pic)) = state
        .store()
        .people()
        .ok()
        .and_then(|p| p.get(chat_id).cloned())
    {
        chat.name = name;
        chat.picture = pic;
    } else if let Ok(contacts) = line
        .client
        .base
        .talk()
        .get_contacts(&[chat_id.to_string()])
        .await
        && let Some(c) = contacts_from(&contacts.to_json()).into_iter().next()
    {
        chat.name = c.name;
        chat.picture = c.picture;
        chat.kind = if c.bot { "bot".into() } else { "dm".into() };
    }
    chat
}

fn kind_for(mid: &str, bot: bool) -> String {
    if mid.starts_with('c') || mid.starts_with('r') {
        "group".into()
    } else if bot {
        "bot".into()
    } else {
        "dm".into()
    }
}

fn apply_group(chat: &mut Chat, raw: &Json) {
    if let Some(name) = str_of(&raw["chatName"]) {
        chat.name = name;
    }
    chat.picture = str_of(&raw["picturePath"]).or(chat.picture.take());
    chat.kind = "group".into();
    if let Some(members) = raw["extra"]["groupExtra"]["memberMids"].as_object() {
        chat.member_count = Some(members.len() as i64);
    }
}

// ---------------------------------------------------------------------------
// Contacts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ContactInfo {
    pub mid: String,
    pub name: String,
    pub picture: Option<String>,
    pub bot: bool,
    pub muted: Option<bool>,
}

/// Find every `Contact`-shaped object in a response. `getContacts`,
/// `getContactsV2` and `getContactsV3` nest them differently.
pub fn contacts_from(json: &Json) -> Vec<ContactInfo> {
    fn walk(v: &Json, out: &mut Vec<ContactInfo>, depth: usize) {
        if depth > 8 {
            return;
        }
        match v {
            Json::Array(a) => a.iter().for_each(|x| walk(x, out, depth + 1)),
            Json::Object(o) => {
                let mid = o.get("mid").and_then(str_of);
                let name = o
                    .get("displayNameOverridden")
                    .and_then(str_of)
                    .or_else(|| o.get("displayName").and_then(str_of))
                    .or_else(|| o.get("profileName").and_then(str_of));
                if let (Some(mid), Some(name)) = (mid, name) {
                    let ty = o.get("type").map(|t| t.to_string()).unwrap_or_default();
                    let settings = o.get("settings").and_then(Json::as_i64);
                    out.push(ContactInfo {
                        mid,
                        name,
                        picture: o.get("picturePath").and_then(str_of),
                        bot: ty.contains("BOT")
                            || o.get("capableBuddy").and_then(Json::as_bool) == Some(true),
                        // CONTACT_SETTING_NOTIFICATION_DISABLE is bit 1.
                        muted: settings.map(|s| s & 1 == 1),
                    });
                    return;
                }
                o.values().for_each(|x| walk(x, out, depth + 1));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(json, &mut out, 0);
    out
}

pub async fn fetch_contacts(line: &Line, mids: &[String]) -> Vec<ContactInfo> {
    let mut out = Vec::new();
    for chunk in mids.chunks(40) {
        let talk = line.client.base.talk();
        match talk.get_contacts(chunk).await {
            Ok(v) => out.extend(contacts_from(&v.to_json())),
            Err(_) => {
                if let Ok(v) = talk.get_contacts_v2(chunk).await {
                    out.extend(contacts_from(&v.to_json()));
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Chat list
// ---------------------------------------------------------------------------

async fn message_boxes(line: &Line) -> Result<Vec<Value>> {
    let mut all = Vec::new();
    let mut seen = HashSet::new();
    let mut min_chat: Option<String> = None;
    for _ in 0..20 {
        let mut req = tv!({"withUnreadCount": true, "lastMessagesPerMessageBoxCount": 1, "messageBoxCountLimit": 200});
        if let Some(m) = &min_chat {
            req.insert("minChatId", Value::from(m.clone()));
        }
        let mut args = Value::object();
        args.insert("messageBoxListRequest", req);
        let res = line.client.base.talk().get_message_boxes(args).await?;
        let page: Vec<Value> = res["messageBoxes"]
            .as_list()
            .map(|l| l.to_vec())
            .unwrap_or_default();
        let fresh: Vec<Value> = page
            .into_iter()
            .filter(|b| {
                b["id"]
                    .as_str()
                    .is_some_and(|id| seen.insert(id.to_string()))
            })
            .collect();
        if fresh.is_empty() {
            break;
        }
        min_chat = fresh
            .last()
            .and_then(|b| b["id"].as_str().map(str::to_string));
        let more = res["hasNext"].as_bool().unwrap_or(fresh.len() >= 200);
        all.extend(fresh);
        if !more {
            break;
        }
    }
    Ok(all)
}

async fn decrypt(line: &Line, raw: Value) -> (Value, bool) {
    if raw["contentMetadata"].get("e2eeVersion").is_none() {
        return (raw, true);
    }
    match line.client.base.decrypt_e2ee_message(raw.clone()).await {
        Ok(v) => (with_e2ee_file_name(line, v).await, true),
        Err(e) => {
            tracing::debug!("decrypt failed: {e}");
            (raw, false)
        }
    }
}

/// An E2EE file keeps its name in the encrypted payload, which line-rs does
/// not open for files; copy it into `FILE_NAME` as plain files carry it.
async fn with_e2ee_file_name(line: &Line, mut raw: Value) -> Value {
    let ct = &raw["contentType"];
    let file = ct.as_i64() == Some(14) || ct.as_str() == Some("FILE");
    if !file || raw["contentMetadata"].get("FILE_NAME").is_some() {
        return raw;
    }
    let name = match line.client.base.decrypt_e2ee_data_message(&raw).await {
        Ok(p) => p["fileName"]
            .as_str()
            .filter(|n| !n.is_empty())
            .map(str::to_string),
        Err(e) => {
            tracing::debug!("file name decrypt failed: {e}");
            None
        }
    };
    if let Some(name) = name {
        let mut entries: Vec<(Value, Value)> = raw["contentMetadata"]
            .entries()
            .into_iter()
            .map(|(k, v)| (Value::String(k), v.clone()))
            .collect();
        entries.push((Value::String("FILE_NAME".into()), Value::String(name)));
        raw.insert("contentMetadata", Value::Map(entries));
    }
    raw
}

async fn to_message(line: &Line, raw: Value) -> Option<Message> {
    let (raw, ok) = decrypt(line, raw).await;
    let ct = TalkMessage::new(line.client.clone(), raw.clone()).content_type();
    line.remember_media(&raw, &ct);
    let mut m = convert::message_from_json(&raw.to_json(), &line.me, ok)?;
    m.content_type = ct;
    Some(m)
}

/// Rebuild the chat list from LINE: message boxes (unread, last activity),
/// friends, and groups. Then sync history for chats that changed.
pub async fn refresh_chats(state: &Arc<AppState>, full: bool) -> Result<()> {
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    state.status("กำลังโหลดรายชื่อแชท…");
    let boxes = message_boxes(&line).await?;
    let mut rows: HashMap<String, Chat> = HashMap::new();
    let mut server_mute: HashSet<String> = HashSet::new();
    for b in &boxes {
        let Some(mid) = b["id"].as_str() else {
            continue;
        };
        let json = b.to_json();
        let mut chat = Chat {
            id: mid.into(),
            name: mid.into(),
            kind: kind_for(mid, false),
            unread: json["unreadCount"].as_i64().unwrap_or(0),
            last_activity: num_of(&json["lastDeliveredMessageId"]["deliveredTime"]).unwrap_or(0.0),
            ..Default::default()
        };
        if let Some(last) = b["lastMessages"].as_list().and_then(|l| l.first()).cloned()
            && let Some(m) = to_message(&line, last).await
        {
            chat.preview = convert::preview_line(&m);
            chat.last_activity = chat.last_activity.max(m.created_time);
        }
        rows.insert(mid.into(), chat);
    }
    *line.boxes.lock().unwrap() = boxes
        .iter()
        .filter_map(|b| Some((b["id"].as_str()?.to_string(), b.clone())))
        .collect();

    // Friends and 1:1 partners.
    let mut mids: Vec<String> = match line
        .client
        .base
        .talk()
        .get_all_contact_ids(tv!({"syncReason": "INTERNAL"}))
        .await
    {
        Ok(v) => v
            .as_list()
            .unwrap_or_default()
            .iter()
            .filter_map(|m| m.as_str().map(str::to_string))
            .collect(),
        Err(e) => {
            tracing::warn!("getAllContactIds: {e}");
            vec![]
        }
    };
    for mid in rows.keys().filter(|m| m.starts_with('u')) {
        if !mids.contains(mid) {
            mids.push(mid.clone());
        }
    }
    let contacts = fetch_contacts(&line, &mids).await;
    {
        let mut store = state.store();
        let users: Vec<_> = contacts
            .iter()
            .map(|c| {
                (
                    c.mid.clone(),
                    c.name.clone(),
                    c.picture.clone(),
                    Some(kind_for(&c.mid, c.bot)),
                )
            })
            .collect();
        let _ = store.upsert_users(&users);
    }
    for c in contacts {
        let row = rows.entry(c.mid.clone()).or_insert_with(|| Chat {
            id: c.mid.clone(),
            name: c.mid.clone(),
            kind: "dm".into(),
            ..Default::default()
        });
        row.name = c.name;
        row.picture = c.picture;
        row.kind = kind_for(&c.mid, c.bot);
        if let Some(m) = c.muted {
            row.muted = m;
            server_mute.insert(c.mid.clone());
        }
    }

    // Groups, in chunks: one big getChats fails with INVALID_LENGTH.
    let joined = line
        .client
        .base
        .talk()
        .get_all_chat_mids(tv!({"request": {"withMemberChats": true}, "syncReason": "INTERNAL"}))
        .await;
    if let Ok(joined) = joined {
        let group_mids: Vec<String> = joined["memberChatMids"]
            .as_list()
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        for chunk in group_mids.chunks(50) {
            let res = line
                .client
                .base
                .talk()
                .get_chats(GetChatsOptions {
                    chat_mids: chunk.to_vec(),
                    with_invitees: Some(false),
                    with_members: Some(true),
                })
                .await;
            let Ok(res) = res else { continue };
            for raw in res["chats"].as_list().unwrap_or_default() {
                let json = raw.to_json();
                let Some(mid) = str_of(&json["chatMid"]) else {
                    continue;
                };
                let row = rows.entry(mid.clone()).or_insert_with(|| Chat {
                    id: mid.clone(),
                    name: mid.clone(),
                    kind: "group".into(),
                    ..Default::default()
                });
                apply_group(row, &json);
                if let Some(m) = json["notificationDisabled"].as_bool() {
                    row.muted = m;
                    server_mute.insert(mid.clone());
                }
            }
        }
    }

    // Merge with what we have: a placeholder name never replaces a real one,
    // and mute is only trusted from the server for 1:1 chats.
    let merged: Vec<Chat> = {
        let store = state.store();
        let old: HashMap<String, Chat> = store
            .chats()?
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect();
        rows.into_values()
            .map(|mut c| {
                if let Some(o) = old.get(&c.id) {
                    if c.name == c.id && o.name != o.id {
                        c.name = o.name.clone();
                    }
                    if c.preview.is_empty() {
                        c.preview = o.preview.clone();
                    }
                    c.avatar = c.avatar.or(o.avatar.clone());
                    c.picture = c.picture.or(o.picture.clone());
                    if !server_mute.contains(&c.id) {
                        c.muted = o.muted;
                    }
                    c.last_activity = c.last_activity.max(o.last_activity);
                }
                c
            })
            .collect()
    };
    state.store().upsert_chats(&merged)?;
    state.chats_changed();
    state.status("เชื่อม LINE แล้ว");

    let s = state.clone();
    tokio::spawn(async move { sync_history(&s, full).await });
    Ok(())
}

/// Whether a chat's history changed since its last successful sync.
pub fn needs_history_sync(activity: f64, synced: Option<f64>) -> bool {
    if activity <= 0.0 {
        return false;
    }
    match synced {
        None => true,
        Some(s) => activity > s,
    }
}

async fn sync_history(state: &Arc<AppState>, full: bool) {
    let Some(line) = state.line().await else {
        return;
    };
    if line.syncing.swap(true, Ordering::SeqCst) {
        return;
    }
    let queue: Vec<(String, f64)> = {
        let store = state.store();
        let synced = store.synced().unwrap_or_default();
        store
            .chats()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| full || needs_history_sync(c.last_activity, synced.get(&c.id).copied()))
            .filter(|c| c.last_activity > 0.0)
            .map(|c| (c.id, c.last_activity))
            .collect()
    };
    let total = queue.len();
    for (i, (chat, activity)) in queue.into_iter().enumerate() {
        if state.line().await.is_none() {
            break;
        }
        state.update_session(|s| {
            s.sync_done = i;
            s.sync_total = total;
            s.status = format!("กำลัง sync {}/{} แชท", i + 1, total);
        });
        if fetch_history(state, &chat).await.is_ok() {
            let _ = state.store().mark_synced(&chat, activity);
        }
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
    state.update_session(|s| {
        s.sync_done = total;
        s.sync_total = 0;
        s.status = "เชื่อม LINE แล้ว".into();
    });
    line.syncing.store(false, Ordering::SeqCst);
}

/// The latest 100 messages of a chat, decrypted and stored.
pub async fn fetch_history(state: &Arc<AppState>, chat_id: &str) -> Result<usize> {
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    let raws = recent_raw(&line, chat_id, HISTORY_LIMIT).await?;
    Ok(store_raws(state, &line, chat_id, raws).await?.len())
}

/// The 100 messages before the oldest one stored, for scrolling up. Returns
/// how many were new; 0 means the start of the chat's history.
pub async fn fetch_older(state: &Arc<AppState>, chat_id: &str) -> Result<usize> {
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    let Some((delivered, id)) = state.store().oldest_cursor(chat_id)? else {
        return fetch_history(state, chat_id).await;
    };
    let mut request = tv!({"messageBoxId": chat_id, "messagesCount": HISTORY_LIMIT});
    request.insert(
        "endMessageId",
        tv!({"deliveredTime": delivered, "messageId": id}),
    );
    let mut args = Value::object();
    args.insert("request", request);
    let res = line
        .client
        .base
        .talk()
        .get_previous_messages_v2_with_request(args)
        .await?;
    let raws = res.as_list().map(|l| l.to_vec()).unwrap_or_default();
    let known: Vec<bool> = {
        let store = state.store();
        raws.iter()
            .map(|r| {
                let id = r["id"]
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| r["id"].as_i64().map(|i| i.to_string()));
                id.is_some_and(|id| store.contains_message(&id).unwrap_or(false))
            })
            .collect()
    };
    let fresh = known.iter().filter(|k| !**k).count();
    store_raws(state, &line, chat_id, raws).await?;
    Ok(fresh)
}

/// Decrypt, convert and store raw history messages.
async fn store_raws(
    state: &Arc<AppState>,
    line: &Arc<Line>,
    chat_id: &str,
    raws: Vec<Value>,
) -> Result<Vec<Message>> {
    let mut messages = Vec::with_capacity(raws.len());
    let mut reactions = Vec::new();
    for raw in raws {
        let has_reactions = raw.get("reactions").is_some();
        if let Some(m) = to_message(line, raw).await {
            if has_reactions {
                reactions.push((m.id.clone(), m.reactions.clone()));
            }
            messages.push(m);
        }
    }
    let n = messages.len();
    {
        let mut store = state.store();
        store.upsert_messages(&messages)?;
        // History replaces a message's reactions only when it carries them.
        for (id, list) in reactions {
            store.replace_reactions(&id, &list)?;
        }
    }
    if n > 0 {
        refresh_preview(state, chat_id);
    }
    state.messages_changed(chat_id);
    if (chat_id.starts_with('c') || chat_id.starts_with('r'))
        && unnamed_senders(state, chat_id, &messages)
    {
        let s = state.clone();
        let c = chat_id.to_string();
        tokio::spawn(async move {
            let _ = load_members(&s, &c, true).await;
        });
    }
    Ok(messages)
}

fn unnamed_senders(state: &AppState, chat_id: &str, messages: &[Message]) -> bool {
    let people = state.store().people().unwrap_or_default();
    let loaded = state.members_loaded.lock().unwrap().contains_key(chat_id);
    !loaded
        && messages.iter().any(|m| {
            !m.mine
                && m.sender_id
                    .as_ref()
                    .is_some_and(|s| !people.contains_key(s))
        })
}

/// Raw recent messages (newest first) from the chat's message box cursor.
pub async fn recent_raw(line: &Line, chat_id: &str, limit: i32) -> Result<Vec<Value>> {
    let cursor = line
        .boxes
        .lock()
        .unwrap()
        .get(chat_id)
        .map(|b| b["lastDeliveredMessageId"].clone());
    let list = match cursor {
        Some(cursor) if !cursor.is_null() => {
            let mut request = tv!({"messageBoxId": chat_id, "messagesCount": limit});
            request.insert("endMessageId", cursor);
            let mut args = Value::object();
            args.insert("request", request);
            line.client
                .base
                .talk()
                .get_previous_messages_v2_with_request(args)
                .await?
        }
        _ => {
            line.client
                .base
                .talk()
                .get_recent_messages_v2(chat_id, limit)
                .await?
        }
    };
    Ok(list.as_list().map(|l| l.to_vec()).unwrap_or_default())
}

/// Group members with names. Unknown names are fetched from LINE.
pub async fn load_members(
    state: &Arc<AppState>,
    chat_id: &str,
    force: bool,
) -> Result<Vec<ChatMember>> {
    let cached = state.store().members(chat_id)?;
    let fresh = state
        .members_loaded
        .lock()
        .unwrap()
        .get(chat_id)
        .is_some_and(|t| now_ms() - t < 30.0 * 60_000.0);
    if !cached.is_empty() && fresh && !force {
        return Ok(cached);
    }
    let Some(line) = state.line().await else {
        return Ok(cached);
    };
    if chat_id.starts_with('u') {
        return Ok(cached);
    }
    let chat = line
        .client
        .get_chat(chat_id)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let raw = chat.raw.to_json();
    let mids: Vec<String> = raw["extra"]["groupExtra"]["memberMids"]
        .as_object()
        .map(|m| m.keys().filter(|k| **k != line.me).cloned().collect())
        .unwrap_or_default();
    let people = state.store().people()?;
    let missing: Vec<String> = mids
        .iter()
        .filter(|m| !people.contains_key(*m))
        .cloned()
        .collect();
    let found = fetch_contacts(&line, &missing).await;
    {
        let mut store = state.store();
        let users: Vec<_> = found
            .iter()
            .map(|c| (c.mid.clone(), c.name.clone(), c.picture.clone(), None))
            .collect();
        store.upsert_users(&users)?;
    }
    let people = state.store().people()?;
    let members: Vec<ChatMember> = mids
        .iter()
        .map(|mid| {
            let (name, picture) = people.get(mid).cloned().unwrap_or((mid.clone(), None));
            ChatMember {
                mid: mid.clone(),
                name,
                picture,
                all: false,
            }
        })
        .collect();
    state.store().upsert_members(chat_id, &members)?;
    state
        .members_loaded
        .lock()
        .unwrap()
        .insert(chat_id.to_string(), now_ms());
    {
        // Keep the group's name and picture fresh too.
        let mut store = state.store();
        if let Ok(Some(mut c)) = store.chat(chat_id) {
            apply_group(&mut c, &raw);
            let _ = store.upsert_chats(&[c]);
        }
    }
    state.members_changed(chat_id);
    state.messages_changed(chat_id);
    state.store().members(chat_id)
}

/// `getMessageReadRange` → reader mid → furthest message id read.
pub async fn read_status(state: &Arc<AppState>, chat_id: &str) -> Result<()> {
    let Some(line) = state.line().await else {
        return Ok(());
    };
    let Some(range) = line
        .chat(chat_id)
        .read_range()
        .await
        .map_err(|e| anyhow!("{e}"))?
    else {
        return Ok(());
    };
    let json = range.to_json();
    let Some(ranges) = json["ranges"].as_object() else {
        return Ok(());
    };
    for (mid, list) in ranges {
        if *mid == line.me {
            continue;
        }
        let mut best: Option<String> = None;
        for entry in list.as_array().into_iter().flatten() {
            // line-rs leaves these fields unnamed, so the end id may sit under "2".
            let end = str_of(&entry["endMessageId"]).or_else(|| str_of(&entry["2"]));
            if let Some(end) = end {
                best = Some(reads::later(best.as_deref(), &end));
            }
        }
        if let Some(best) = best {
            apply_read(state, chat_id, mid, &best);
        }
    }
    Ok(())
}

/// The raw message as LINE sent it, for the inspector. Looks through the
/// latest 100 messages when it is not stored.
pub async fn raw_message(state: &Arc<AppState>, chat_id: &str, message_id: &str) -> Result<String> {
    if let Some(raw) = state.store().raw_message(message_id)? {
        return Ok(raw);
    }
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    for raw in recent_raw(&line, chat_id, 100).await? {
        if raw["id"].as_str() == Some(message_id) {
            let (raw, _) = decrypt(&line, raw).await;
            return Ok(serde_json::to_string_pretty(&raw.to_json())?);
        }
    }
    Err(anyhow!("not_in_recent_100"))
}

/// Make sure the raw media message is known, refetching recent history.
pub async fn ensure_media_message(
    state: &Arc<AppState>,
    line: &Line,
    chat_id: &str,
    message_id: &str,
) -> Result<()> {
    if line.has_media(message_id) {
        return Ok(());
    }
    for raw in recent_raw(line, chat_id, 100).await? {
        if raw["id"].as_str() == Some(message_id) {
            let (raw, _) = decrypt(line, raw).await;
            let ct = TalkMessage::new(line.client.clone(), raw.clone()).content_type();
            line.remember_media(&raw, &ct);
            return Ok(());
        }
    }
    let _ = state;
    Err(anyhow!("file_expired"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_chunks_come_back_as_bytes() {
        let raw =
            raw_from_store(r#"{"id": "1", "chunks": ["AAEC", "/w=="], "contentType": "FILE"}"#)
                .unwrap();
        let chunks = raw["chunks"].as_list().unwrap();
        assert_eq!(chunks[0], Value::Binary(vec![0, 1, 2]));
        assert_eq!(chunks[1], Value::Binary(vec![255]));
        assert_eq!(raw["contentType"].as_str(), Some("FILE"));
    }

    #[test]
    fn a_refused_refresh_ends_the_session() {
        let failed = |method: &str, data: Value| {
            line_rs::Error::Request { method: method.into(), path: "/p".into(), data }.to_string()
        };
        assert!(token_dead(&failed("refresh", tv!({"errorCode": "INVALID_REQUEST"}))));
        assert!(!token_dead(&failed("refresh", tv!({"errorCode": "RETRY_REQUIRED"}))));
        assert!(token_dead(&failed("sync", tv!({"code": "MUST_REFRESH_V3_TOKEN"}))));
        assert!(!token_dead(&failed("sendMessage", tv!({"code": "INVALID_REQUEST"}))));
        assert!(!token_dead("http: connection reset"));
    }
}
