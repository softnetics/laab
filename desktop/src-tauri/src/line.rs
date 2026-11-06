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
use line_rs::service::talk::{GetChatsOptions, MessageBoxQuery};
use line_rs::thrift::Value;
use line_rs::{Device, tv};
use serde_json::Value as Json;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::AbortHandle;

use crate::analytics;
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
    /// A catch-up after missed events is waiting to run (see [`resync`]).
    resync_pending: AtomicBool,
    /// A pass over undecrypted messages is running (see [`retry_undecrypted`]).
    retrying: AtomicBool,
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

/// Whether `e` means the session is over and the user has to sign in
/// again. line-rs's `is_auth_dead`, plus two cases LAAB also counts as dead:
/// a request that still asks for a token refresh after line-rs tried one
/// (`MUST_REFRESH_V3_TOKEN`, which line-rs leaves to the app), and a refresh
/// LINE refused for any reason other than "try again" (line-rs counts only
/// `INVALID_REQUEST`).
fn auth_error(e: &line_rs::Error) -> bool {
    e.is_auth_dead()
        || e.error_code() == Some(line_rs::ErrorCode::MustRefreshV3Token)
        || matches!(
            e,
            line_rs::Error::Request { path, data, .. }
                if path == line_rs::request::TOKEN_REFRESH_PATH
                    && data["errorCode"].as_str() != Some("RETRY_REQUIRED")
        )
}

/// [`auth_error`] for the text of a `RefreshError` log event, which line-rs
/// sends as text: the `refresh` call was refused (other than "try again"),
/// or a request still asks for a refresh.
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

const PREMIUM_KEY: &str = "premium";

/// Record whether the account has LINE Premium, for the UI and next launch.
pub fn remember_premium(state: &AppState, premium: bool) {
    logged("saving premium", state.store().set_kv(PREMIUM_KEY, if premium { "1" } else { "0" }));
    state.update_session(|x| x.premium = Some(premium));
}

/// Logs an error the app carries on after, so the log shows why something
/// went missing.
fn logged<T, E: std::fmt::Display>(what: &str, result: std::result::Result<T, E>) {
    if let Err(e) = result {
        tracing::warn!("{what}: {e:#}");
    }
}

/// Ask the server whether the session still works. A dead token ends it
/// here; a logout reason makes line-rs emit `KickedOut`, which ends it
/// instead; a network error changes nothing.
async fn check_session(state: Arc<AppState>, line: Arc<Line>) {
    if let Err(e) = line.client.base.talk().get_server_time().await
        && auth_error(&e)
        && !kicked_out(&e)
    {
        tracing::warn!("session expired: {}", e.full_message());
        analytics::failed(&state, analytics::Failure::SessionEnded { kicked: None }, "");
        session_ended(state, line, EXPIRED).await;
    }
}

/// Whether line-rs ends the session itself for `e` and emits `KickedOut`:
/// a logout reason (LAAB signs in as a secondary device), or a request on a
/// client already kicked out or closed.
fn kicked_out(e: &line_rs::Error) -> bool {
    line_rs::base::LogoutReason::from_error_for(e, true).is_some()
        || matches!(e, line_rs::Error::Internal { kind, .. } if kind == "LoggedOut" || kind == "ClientClosed")
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
            tracing::warn!("token login failed: {}", e.full_message());
            analytics::failed_with(&state, analytics::Failure::Login(analytics::Login::Saved), &e);
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
            connected(state.clone(), client, DEFAULT_DEVICE).await;
            analytics::send(&state, analytics::Event::SignedIn(analytics::Login::Qr));
            Ok(())
        }
        Err(e) => {
            analytics::failed_with(&state, analytics::Failure::Login(analytics::Login::Qr), &e);
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
            connected(state.clone(), client, Device::DesktopWin).await;
            analytics::send(&state, analytics::Event::SignedIn(analytics::Login::Password));
            Ok(())
        }
        Err(e) => {
            analytics::failed_with(&state, analytics::Failure::Login(analytics::Login::Password), &e);
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
        // Stopped first, so a refresh or kick-out during logout isn't
        // handled as a session of its own.
        line.stop();
        // logoutV2, as LINE's apps do; line-rs refreshes the token if it
        // has to and falls back to logoutZ. Signed out here either way.
        if let Err(e) = line.client.base.auth().logout().await {
            tracing::warn!("logging out of LINE: {}", e.full_message());
        }
    }
    clear_auth(&state);
    let _ = state.store().session_clear();
    crate::analytics::signed_out(&state);
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
        resync_pending: AtomicBool::new(false),
        retrying: AtomicBool::new(false),
    });
    if let Some(old) = state.line.write().await.replace(line.clone()) {
        old.stop();
    }
    let display_name = profile["displayName"].as_str().unwrap_or("").to_string();
    let picture = str_of(&profile["picturePath"]);
    {
        let mut store = state.store();
        logged("saving my profile", store.upsert_users(&[(me.clone(), display_name.clone(), picture.clone(), None)]));
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
    tokio::spawn(load_notification_settings(client.clone()));
    {
        let (s, c) = (state.clone(), client.clone());
        tokio::spawn(async move {
            match c.premium_status().await {
                Ok(p) => remember_premium(&s, p.active),
                Err(e) => {
                    // Not every region answers the status call; fall back to
                    // what an earlier edit taught us.
                    tracing::warn!("premium status: {}", e.full_message());
                    let known = s.store().get_kv(PREMIUM_KEY).ok().flatten().map(|v| v == "1");
                    s.update_session(|x| x.premium = known);
                }
            }
        });
    }
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
            logged("loading history", fetch_history(&s, &chat).await);
        }
        if let Err(e) = refresh_chats(&s, false).await {
            s.status(format!("โหลดรายชื่อแชทไม่สำเร็จ: {e}"));
        }
        name_stored_files(&s).await;
        // The login's key chain may open messages that failed before.
        if let Some(line) = s.line().await {
            retry_undecrypted(&s, &line, None).await;
        }
        crate::actions::prefetch_calendars(&s, None).await;
    });
}

/// Read the account's settings (LINE's, shared by every device): the
/// notification ones for the banner rules, and the account's language for
/// LIFF apps. On failure the banner rules treat every switch as on, as
/// before.
async fn load_notification_settings(client: Client) {
    match client.base.talk().get_settings(line_rs::tv!({})).await {
        Ok(v) => {
            use line_rs::client::settings::NotificationSettings;
            crate::notify::set_settings(Some(NotificationSettings::from_settings(&v)));
            crate::liff::set_account_locale(v["preferenceLocale"].as_str().map(str::to_string));
        }
        Err(e) => tracing::warn!("account settings: {}", e.full_message()),
    }
}

/// How many undecrypted messages one pass tries, newest first.
const RETRY_LIMIT: usize = 300;

/// Try again to decrypt stored E2EE messages that failed (in `chat_id`, or
/// everywhere), after keys may have changed: at login, when LINE says
/// keys changed, and when the chat is opened. A message that opens gets
/// its text and leaves the queue; one that still fails stays as it is.
/// line-rs remembers keys LINE could not resolve, so a pass costs no
/// requests for those. The Android app keeps such messages as
/// E2EE_UNDECRYPTED; whether it re-tries them is unverified.
pub async fn retry_undecrypted(state: &Arc<AppState>, line: &Arc<Line>, chat_id: Option<&str>) {
    if line.retrying.swap(true, Ordering::SeqCst) {
        return;
    }
    let pending = state
        .store()
        .undecrypted_messages(chat_id, RETRY_LIMIT)
        .unwrap_or_default();
    let mut opened: Vec<Message> = Vec::new();
    for (_, _, raw) in pending {
        let Some(raw) = raw_from_store(&raw) else { continue };
        let (raw, ok) = decrypt(line, raw).await;
        if !ok || line_rs::e2ee::is_undecrypted(&raw) {
            continue;
        }
        let ct = TalkMessage::new(line.client.clone(), raw.clone()).content_type();
        if let Some(mut m) = convert::message_from_json(&message_json(&raw), &line.me, true) {
            m.content_type = ct;
            opened.push(m);
        }
    }
    line.retrying.store(false, Ordering::SeqCst);
    if opened.is_empty() {
        return;
    }
    tracing::info!("decrypted {} messages on a second try", opened.len());
    logged("saving decrypted messages", state.store().upsert_messages(&opened));
    let chats: HashSet<&str> = opened.iter().map(|m| m.chat_id.as_str()).collect();
    for chat in chats {
        refresh_preview(state, chat);
        state.messages_changed(chat);
    }
    state.chats_changed();
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

/// The raw (decrypted) message stored for `message_id`, as line-rs takes it.
pub fn stored_raw(state: &AppState, message_id: &str) -> Option<Value> {
    raw_from_store(&state.store().raw_message(message_id).ok()??)
}

/// A raw message as stored back as the thrift value line-rs decrypts.
/// Chunks are stored in base64 ([`message_json`]); rows stored before that
/// hold a chunk that was valid UTF-8 (a key id like `00 00 00 05`) as the
/// text itself, so a chunk that is not valid base64 is taken as its bytes.
fn raw_from_store(json: &str) -> Option<Value> {
    use base64::Engine;
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut raw = Value::from(v.clone());
    if let Some(chunks) = v.get("chunks").and_then(|c| c.as_array()) {
        let b64 = base64::engine::general_purpose::STANDARD;
        let bytes: Vec<Value> = chunks
            .iter()
            .map(|c| {
                let text = c.as_str().unwrap_or_default();
                Value::Binary(b64.decode(text).unwrap_or_else(|_| text.as_bytes().to_vec()))
            })
            .collect();
        raw.insert("chunks", Value::List(bytes));
    }
    Some(raw)
}

/// A LINE message as JSON for storing, with every E2EE chunk in base64.
/// `Value::to_json` writes binary as base64 but a chunk that decoded as
/// UTF-8 text as the text, so the stored kinds could not be told apart: a
/// nonce read back as its base64 text is 16 bytes, not 12, which line-rs
/// refused as "malformed nonce or ciphertext".
pub fn message_json(raw: &Value) -> Json {
    use base64::Engine;
    let mut json = raw.to_json();
    if let Some(chunks) = raw["chunks"].as_list() {
        let b64 = base64::engine::general_purpose::STANDARD;
        json["chunks"] = chunks
            .iter()
            .map(|c| Json::String(b64.encode(c.as_bytes().unwrap_or_default())))
            .collect();
    }
    json
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
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("dropped {n} LINE events");
                    let Some(line) = l.upgrade() else { break };
                    resync(&s, line);
                }
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
                    analytics::failed(
                        &s,
                        analytics::Failure::SessionEnded { kicked: Some(analytics::Kick::of(&reason)) },
                        "",
                    );
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
                        analytics::failed(&s, analytics::Failure::SessionEnded { kicked: None }, "");
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
                // LINE won't replay what changed since the saved revision.
                Ok(line_rs::ClientEvent::FullSyncRequired { categories, .. }) => {
                    tracing::info!("LINE asked for a full sync: {categories:?}");
                    let Some(line) = l.upgrade() else { break };
                    resync(&s, line);
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("dropped {n} LINE session events");
                    let Some(line) = l.upgrade() else { break };
                    resync(&s, line);
                }
                Ok(_) => {}
                Err(RecvError::Closed) => break,
            }
        }
    });
    line.tasks
        .lock()
        .unwrap()
        .extend([talk.abort_handle(), base.abort_handle()]);
}

/// How long [`resync`] waits, so a burst of requests runs it once.
const RESYNC_DELAY: Duration = Duration::from_secs(3);

/// Reload the chat list and catch up on the history of chats that changed,
/// after events were missed: LINE asked for a full sync, or LAAB fell behind
/// on line-rs's events. Waits [`RESYNC_DELAY`] first, and requests that
/// come in meanwhile share the run.
fn resync(state: &Arc<AppState>, line: Arc<Line>) {
    if line.resync_pending.swap(true, Ordering::SeqCst) {
        return;
    }
    let s = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(RESYNC_DELAY).await;
        line.resync_pending.store(false, Ordering::SeqCst);
        if !s.line().await.is_some_and(|c| Arc::ptr_eq(&c, &line)) {
            return;
        }
        drop(line);
        logged("catching up after missed events", refresh_chats(&s, false).await);
    });
}

// ---------------------------------------------------------------------------
// Live events
// ---------------------------------------------------------------------------

async fn handle_event(state: &Arc<AppState>, line: &Arc<Line>, ev: LineEvent) {
    match ev {
        LineEvent::Message(m) | LineEvent::MessageEdit(m) => {
            let ct = m.content_type();
            line.remember_media(&m.raw, &ct);
            // line-rs decrypts text but not an E2EE file's payload, which
            // holds its name; without this a file that arrives live, or that
            // we just sent, has no name until LAAB restarts.
            let raw = with_e2ee_file_name(line, m.raw.clone()).await;
            // line-rs hands over a message it could not decrypt as it came.
            let decrypted = !line_rs::e2ee::is_undecrypted(&raw);
            let json = message_json(&raw);
            let Some(mut msg) = convert::message_from_json(&json, &line.me, decrypted) else {
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
            TalkEvent::ChatChecked {
                chat_mid,
                message_id,
            } => apply_checked(state, &chat_mid, &message_id),
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
                if !removed {
                    crate::notify::maybe_notify_reaction(state, &r.message_id, &user, &line.me);
                }
            }
            // Settings changed, maybe on the phone: read the notification
            // ones again when they are among them.
            TalkEvent::SettingsUpdated { attributes } => {
                use line_rs::client::settings::NOTIFICATION_ATTRIBUTES;
                if attributes.is_empty() || attributes.iter().any(|a| NOTIFICATION_ATTRIBUTES.contains(a)) {
                    tokio::spawn(load_notification_settings(line.client.clone()));
                }
            }
            TalkEvent::Unsend { message_id, .. } => apply_unsend(state, &message_id),
            // Invited to a group: it shows up in the list to accept or not.
            TalkEvent::Invite { invitee_mids, .. } if invitee_mids.contains(&line.me) => {
                let s = state.clone();
                tokio::spawn(async move {
                    logged("adding a group invitation", refresh_chats(&s, false).await);
                });
            }
            // Removed from a group: it leaves the list, as when we leave.
            TalkEvent::Kick { chat_mid, kicked_mids, .. } if kicked_mids.contains(&line.me) => {
                state.members_loaded.lock().unwrap().remove(&chat_mid);
                logged("hiding a chat we were removed from", state.store().hide_chat(&chat_mid));
                state.chats_changed();
            }
            TalkEvent::Invite { chat_mid, .. }
            | TalkEvent::Join { chat_mid, .. }
            | TalkEvent::Leave { chat_mid, .. }
            | TalkEvent::Kick { chat_mid, .. } => {
                state.members_loaded.lock().unwrap().remove(&chat_mid);
                state.members_changed(&chat_mid);
            }
            // We left a group, here or on another device: it leaves the
            // list, as in LINE, until a new message brings it back.
            TalkEvent::LeftChat { chat_mid } => {
                state.members_loaded.lock().unwrap().remove(&chat_mid);
                logged("hiding a chat we left", state.store().hide_chat(&chat_mid));
                state.chats_changed();
            }
            TalkEvent::ChatHidden { chat_mid, hidden, .. } => {
                let store = state.store();
                logged(
                    "hiding a chat",
                    if hidden { store.hide_chat(&chat_mid) } else { store.unhide_chat(&chat_mid) },
                );
                drop(store);
                state.chats_changed();
            }
            // The chat was deleted from the list (`sendChatRemoved`), here or
            // on another device.
            TalkEvent::ChatRemoved { chat_mid, last_message_id } => {
                apply_chat_removed(state, &chat_mid, &last_message_id)
            }
            // Every chat's messages were deleted on another device.
            TalkEvent::AllMessagesRemoved => {
                let ids = state.store().all_chat_ids().unwrap_or_default();
                for id in ids {
                    clear_history(state, &id);
                }
            }
            // A chat moved into or out of the message request box (accepted
            // or deleted on another device, or a new request).
            TalkEvent::MessageRequestBox { chat_mid, in_box, .. } => {
                logged("marking a message request", state.store().set_request(&chat_mid, in_box));
                state.chats_changed();
            }
            // Keys changed: messages that could not be decrypted may open now.
            TalkEvent::E2eeKeysChanged { .. } => {
                let (s, l) = (state.clone(), line.clone());
                tokio::spawn(async move { retry_undecrypted(&s, &l, None).await });
            }
            TalkEvent::ChatUpdated { chat_mid, .. } => refresh_chat_row(state, line, &chat_mid).await,
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
        logged("saving read state", store.set_read(chat, reader, &next));
        drop(store);
        state.reads_changed(chat);
    }
}

/// We read a chat, maybe on the phone: lower its unread count to what came
/// after the message we read through.
pub fn apply_checked(state: &AppState, chat: &str, message_id: &str) {
    let store = state.store();
    let Ok(Some(row)) = store.chat(chat) else {
        return;
    };
    if row.unread <= 0 {
        return;
    }
    let recent = store.messages(chat, 500).unwrap_or_default();
    let others = recent.iter().filter(|m| !m.mine).map(|m| m.id.as_str());
    let unread = reads::unread_after(row.unread, message_id, others);
    if unread != row.unread {
        logged("saving unread count", store.set_unread(chat, unread));
        drop(store);
        state.chats_changed();
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
    logged("saving chat", store.upsert_chats(&[chat]));
    drop(store);
    state.chats_changed();
}

async fn ingest_live(state: &Arc<AppState>, line: &Arc<Line>, msg: Message) {
    let chat_id = msg.chat_id.clone();
    let known = state.store().contains_message(&msg.id).unwrap_or(false);
    logged("saving message", state.store().upsert_messages(std::slice::from_ref(&msg)));
    if let Some(list) = convert::parse_reactions(
        &serde_json::from_str(msg.raw.as_deref().unwrap_or("{}")).unwrap_or_default(),
    ) && !list.is_empty()
    {
        logged("saving reactions", state.store().replace_reactions(&msg.id, &list));
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
    logged("saving chat", state.store().upsert_chats(std::slice::from_ref(&chat)));
    state.messages_changed(&chat_id);
    state.chats_changed();
    if known {
        return;
    }
    if focused && active && !msg.mine {
        logged("marking read", line.chat(&chat_id).mark_as_read(&msg.id).await);
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
            let people = state.store().people().unwrap_or_default();
            apply_group(&mut chat, &c.raw.to_json(), &line.me, &people);
        }
    } else if let Some((name, pic)) = state
        .store()
        .people()
        .ok()
        .and_then(|p| p.get(chat_id).cloned())
    {
        chat.name = name;
        chat.picture = pic;
    } else if let Some(c) = fetch_contacts(line, &[chat_id.to_string()])
        .await
        .into_iter()
        .next()
    {
        chat.name = c.name;
        chat.picture = c.picture;
        chat.kind = if c.bot { "bot".into() } else { "dm".into() };
    }
    chat
}

/// Delete a chat's history on this Mac, after it was deleted on LINE.
/// `SEND_CHAT_REMOVED` as LINE for Android applies it
/// (`SEND_CHAT_REMOVED.kt`): a room keeps its row and loses its messages;
/// a 1:1 chat or group also leaves the list until a new message arrives.
/// Messages after `last_message_id` (newer than the removal) stay.
pub fn apply_chat_removed(state: &AppState, chat_id: &str, last_message_id: &str) {
    match last_message_id.parse::<i64>() {
        Ok(id) if id > 0 => {
            logged(
                "clearing a chat's history",
                state.store().remove_history_through(chat_id, id).map(|_| ()),
            );
            refresh_preview(state, chat_id);
            state.messages_changed(chat_id);
        }
        _ => clear_history(state, chat_id),
    }
    if !chat_id.starts_with('r') {
        logged("hiding a removed chat", state.store().hide_chat(chat_id));
    }
    state.chats_changed();
}

fn clear_history(state: &AppState, chat_id: &str) {
    logged("clearing a chat's history", state.store().delete_history(chat_id));
    state.messages_changed(chat_id);
    state.chats_changed();
}

/// Fetch a chat's name, picture and mute again after LINE says it changed.
pub async fn refresh_chat_row(state: &Arc<AppState>, line: &Arc<Line>, chat_id: &str) {
    let Some(mut chat) = state.store().chat(chat_id).ok().flatten() else {
        return;
    };
    if chat_id.starts_with('c') || chat_id.starts_with('r') {
        match line.client.get_chat(chat_id).await {
            Ok(c) => {
                let json = c.raw.to_json();
                let people = state.store().people().unwrap_or_default();
                apply_group(&mut chat, &json, &line.me, &people);
                if let Some(m) = json["notificationDisabled"].as_bool() {
                    chat.muted = m;
                }
            }
            Err(e) => {
                tracing::warn!("reloading an updated chat: {}", e.full_message());
                return;
            }
        }
    } else {
        let Some(c) = fetch_contacts(line, &[chat_id.to_string()])
            .await
            .into_iter()
            .next()
        else {
            return;
        };
        chat.name = c.name;
        chat.picture = c.picture.or(chat.picture);
        if let Some(m) = c.muted {
            chat.muted = m;
        }
    }
    logged("saving an updated chat", state.store().upsert_chats(&[chat]));
    state.chats_changed();
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

fn apply_group(
    chat: &mut Chat,
    raw: &Json,
    me: &str,
    people: &HashMap<String, (String, Option<String>)>,
) {
    if let Some(name) = group_name(raw, me, people) {
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

#[derive(Debug, Clone, Default)]
pub struct ContactInfo {
    pub mid: String,
    pub name: String,
    pub picture: Option<String>,
    pub bot: bool,
    pub muted: Option<bool>,
    /// Whether they are a friend, when the response says (V3 only).
    pub friend: Option<bool>,
    /// Whether the account blocked them, when the response says (V3 only).
    pub blocked: Option<bool>,
}

/// The `GetContactV3Response`s of a `getContactsV3` reply, as LINE for
/// Android reads them: the name the user gave a friend, else their profile
/// name.
pub fn contacts_from_v3(json: &Json) -> Vec<ContactInfo> {
    let has_detail = |d: &Json| d.get("user").is_some() || d.get("bot").is_some();
    json["responses"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let mid = str_of(&r["targetUserMid"])?;
            let detail = &r["targetProfileDetail"];
            let ty = &r["userType"];
            Some(ContactInfo {
                name: str_of(&r["friendDetail"]["user"]["overriddenName"])
                    .or_else(|| str_of(&detail["profileName"]))
                    .unwrap_or_default(),
                picture: str_of(&detail["picturePath"]),
                bot: ty.as_str() == Some("BOT") || ty.as_i64() == Some(2),
                muted: r["notificationSettingEntry"]["notificationSetting"]["mute"].as_bool(),
                friend: r.get("friendDetail").map(has_detail),
                blocked: r.get("blockDetail").map(has_detail),
                mid,
            })
        })
        .collect()
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
                        ..Default::default()
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

/// Contacts by mid: `getContactsV3` on RelationService, as LINE for Android
/// reads them, then the legacy `getContacts` / `getContactsV2` for a chunk
/// V3 refuses.
pub async fn fetch_contacts(line: &Line, mids: &[String]) -> Vec<ContactInfo> {
    let mut out = Vec::new();
    for chunk in mids.chunks(50) {
        match line.client.base.relation().get_contacts_v3(chunk, None).await {
            Ok(v) => {
                let found = contacts_from_v3(&v.to_json());
                if !found.is_empty() {
                    out.extend(found);
                    continue;
                }
            }
            Err(e) => tracing::warn!("getContactsV3: {e}"),
        }
        let talk = line.client.base.talk();
        match talk.get_contacts(chunk).await {
            Ok(v) => out.extend(contacts_from(&v.to_json())),
            Err(e) => {
                tracing::warn!("getContacts: {e}");
                match talk.get_contacts_v2(chunk).await {
                    Ok(v) => out.extend(contacts_from(&v.to_json())),
                    Err(e) => tracing::warn!("getContactsV2: {e}"),
                }
            }
        }
    }
    out
}

/// Every friend's mid, users and official accounts, from RelationService
/// as LINE for Android reads it; the legacy `getAllContactIds` if that
/// fails. `None` when neither answers.
async fn friend_mids(line: &Line) -> Option<Vec<String>> {
    let relation = line.client.base.relation();
    match relation.all_user_friend_ids().await {
        Ok(mut mids) => {
            match relation.all_bot_friend_ids().await {
                Ok(bots) => mids.extend(bots),
                Err(e) => tracing::warn!("getBotFriendIds: {e}"),
            }
            return Some(mids);
        }
        Err(e) => tracing::warn!("getUserFriendIds: {e}"),
    }
    match line
        .client
        .base
        .talk()
        .get_all_contact_ids(tv!({"syncReason": "INTERNAL"}))
        .await
    {
        Ok(v) => Some(
            v.as_list()
                .unwrap_or_default()
                .iter()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect(),
        ),
        Err(e) => {
            tracing::warn!("getAllContactIds: {e}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Chat list
// ---------------------------------------------------------------------------

/// Every message box, paged by `minChatId` as LINE for Android pages them
/// (active boxes only, 100 a page), with unread counts and the last
/// message for the preview. The flag says whether the list is complete.
async fn message_boxes(line: &Line) -> Result<(Vec<Value>, bool)> {
    let mut all = Vec::new();
    let mut seen = HashSet::new();
    let mut query = MessageBoxQuery {
        last_messages: 1,
        ..Default::default()
    };
    for _ in 0..50 {
        let res = line.client.base.talk().message_box_page(&query).await?;
        let page: Vec<Value> = res["messageBoxes"]
            .as_list()
            .map(|l| l.to_vec())
            .unwrap_or_default();
        let full_page = page.len() >= query.limit as usize;
        let fresh: Vec<Value> = page
            .into_iter()
            .filter(|b| {
                b["id"]
                    .as_str()
                    .is_some_and(|id| seen.insert(id.to_string()))
            })
            .collect();
        if fresh.is_empty() {
            return Ok((all, true));
        }
        query.min_chat_id = fresh
            .last()
            .and_then(|b| b["id"].as_str().map(str::to_string));
        let more = res["hasNext"].as_bool().unwrap_or(full_page);
        all.extend(fresh);
        if !more {
            return Ok((all, true));
        }
    }
    Ok((all, false))
}

/// What a message box says other devices did to its chat.
#[derive(Debug, Default, PartialEq)]
pub struct BoxState {
    /// The newest message, by id.
    pub last_id: i64,
    /// Hidden at a message no older than the newest: the chat stays out of
    /// the list until a new message arrives.
    pub hidden: bool,
    /// History deleted up to this message id.
    pub removed_through: Option<i64>,
}

pub fn box_state(b: &Value) -> BoxState {
    let last_id = b["lastDeliveredMessageId"]["messageId"].as_i64().unwrap_or(0);
    let hidden_at = b["hiddenAtMessageId"].as_i64().unwrap_or(0);
    let removed = b["lastRemovedMessageId"].as_i64().unwrap_or(0);
    BoxState {
        last_id,
        hidden: hidden_at > 0 && hidden_at >= last_id,
        removed_through: (removed > 0).then_some(removed),
    }
}

/// A group's name: its own, or for a group LINE names itself (`autoName`,
/// or no name), the names of its members other than us, oldest member
/// first, as LINE shows such groups. The exact format LINE uses is
/// unverified. `None` when neither is known.
pub fn group_name(
    raw: &Json,
    me: &str,
    people: &HashMap<String, (String, Option<String>)>,
) -> Option<String> {
    let own = str_of(&raw["chatName"]);
    let auto = raw["extra"]["groupExtra"]["autoName"].as_bool() == Some(true);
    if own.is_some() && !auto {
        return own;
    }
    let mut members: Vec<(&String, i64)> = raw["extra"]["groupExtra"]["memberMids"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter(|(mid, _)| mid.as_str() != me)
                .map(|(mid, t)| (mid, t.as_i64().unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default();
    members.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(b.0)));
    let names: Vec<&str> = members
        .iter()
        .filter_map(|(mid, _)| people.get(*mid).map(|(n, _)| n.as_str()))
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        return own;
    }
    Some(names.join(", "))
}

async fn decrypt(line: &Line, raw: Value) -> (Value, bool) {
    if raw["contentMetadata"].get("e2eeVersion").is_none() {
        return (raw, true);
    }
    match line.client.base.decrypt_e2ee_message(raw.clone()).await {
        Ok(v) => (with_e2ee_file_name(line, v).await, true),
        Err(e) => {
            // Local log only: ids and lengths, never content.
            tracing::warn!(
                "decrypt failed: {e}; {}",
                line_rs::e2ee::describe_e2ee_message(&raw)
            );
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
            tracing::warn!("file name decrypt failed: {e}");
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
    let mut m = convert::message_from_json(&message_json(&raw), &line.me, ok)?;
    m.content_type = ct;
    Some(m)
}

/// Rebuild the chat list from LINE: message boxes (unread, last activity),
/// friends, and groups. Then sync history for chats that changed.
pub async fn refresh_chats(state: &Arc<AppState>, full: bool) -> Result<()> {
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    state.status("กำลังโหลดรายชื่อแชท…");
    let (boxes, boxes_complete) = message_boxes(&line).await?;
    let mut rows: HashMap<String, Chat> = HashMap::new();
    let mut server_mute: HashSet<String> = HashSet::new();
    let mut hidden: Vec<String> = Vec::new();
    let mut removed: Vec<(String, i64)> = Vec::new();
    for b in &boxes {
        let Some(mid) = b["id"].as_str() else {
            continue;
        };
        let json = b.to_json();
        let state = box_state(b);
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
            && !state
                .removed_through
                .is_some_and(|r| m.id.parse::<i64>().is_ok_and(|id| id <= r))
        {
            chat.preview = convert::preview_line(&m);
            chat.last_activity = chat.last_activity.max(m.created_time);
        }
        if state.hidden {
            hidden.push(mid.into());
        }
        if let Some(r) = state.removed_through {
            removed.push((mid.into(), r));
        }
        rows.insert(mid.into(), chat);
    }
    *line.boxes.lock().unwrap() = boxes
        .iter()
        .filter_map(|b| Some((b["id"].as_str()?.to_string(), b.clone())))
        .collect();

    // Friends and 1:1 partners.
    let friends = friend_mids(&line).await;
    let mut mids: Vec<String> = friends.clone().unwrap_or_default();
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
        logged("saving contacts", store.upsert_users(&users));
        if let Some(friends) = &friends {
            let listed: HashSet<&str> = friends.iter().map(String::as_str).collect();
            let current: Vec<String> = contacts
                .iter()
                .filter(|c| c.friend.unwrap_or(listed.contains(c.mid.as_str())) && c.blocked != Some(true))
                .map(|c| c.mid.clone())
                .collect();
            logged("saving friends", store.set_friends(&current));
        }
    }
    // Friends are listed on the friends page; a 1:1 chat is a row only when
    // it has a message box, as in LINE.
    for c in contacts {
        let Some(row) = rows.get_mut(&c.mid) else {
            continue;
        };
        row.name = c.name;
        row.picture = c.picture;
        row.kind = kind_for(&c.mid, c.bot);
        if let Some(m) = c.muted {
            row.muted = m;
            server_mute.insert(c.mid.clone());
        }
    }

    // Groups we are in, in chunks: one big getChats fails with
    // INVALID_LENGTH. An unnamed group is named after its members.
    let people = state.store().people().unwrap_or_default();
    let joined = line
        .client
        .base
        .talk()
        .get_all_chat_mids(
            tv!({"request": {"withMemberChats": true, "withInvitedChats": true}, "syncReason": "INTERNAL"}),
        )
        .await;
    let groups_ok = joined.is_ok();
    if let Ok(joined) = joined {
        let mids_of = |key: &str| -> Vec<String> {
            joined[key]
                .as_list()
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        };
        let invited: HashSet<String> = mids_of("invitedChatMids").into_iter().collect();
        let mut group_mids = mids_of("memberChatMids");
        group_mids.extend(invited.iter().cloned());
        for chunk in group_mids.chunks(50) {
            let res = line
                .client
                .base
                .talk()
                .get_chats(GetChatsOptions {
                    chat_mids: chunk.to_vec(),
                    with_invitees: Some(chunk.iter().any(|m| invited.contains(m))),
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
                apply_group(row, &json, &line.me, &people);
                row.invited = invited.contains(&mid);
                if row.invited {
                    // Shown when the invitation came, until answered.
                    if let Some(at) = num_of(&json["extra"]["groupExtra"]["inviteeMids"][line.me.as_str()]) {
                        row.last_activity = row.last_activity.max(at);
                    }
                    row.preview = "คุณได้รับคำเชิญเข้ากลุ่ม".into();
                }
                if let Some(m) = json["notificationDisabled"].as_bool() {
                    row.muted = m;
                    server_mute.insert(mid.clone());
                }
            }
        }
    }
    let listed: HashSet<String> = rows.keys().cloned().collect();

    // Chats from people who are not friends wait in LINE's message request
    // box until accepted. LINE leaves them out of its chat list; LAAB lists
    // them marked as requests. `None` keeps what we knew.
    let requests: Option<HashSet<String>> = match line.client.message_request_chats().await {
        Ok(list) => Some(list.into_iter().map(|(mid, _)| mid).collect()),
        Err(e) => {
            tracing::warn!("getMessageRequestBoxList: {e}");
            None
        }
    };
    if let Some(requests) = &requests {
        for (id, row) in rows.iter_mut() {
            row.request = requests.contains(id);
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
                    if requests.is_none() {
                        c.request = o.request;
                    }
                }
                c
            })
            .collect()
    };
    state.store().upsert_chats(&merged)?;
    apply_box_states(state, &hidden, &removed);
    // Only a complete list can say a chat is gone (a group we left, a
    // deleted chat or friend): leave those out until a new message.
    if boxes_complete && groups_ok {
        drop_unlisted(state, &line, &listed);
    }
    state.chats_changed();
    state.status("เชื่อม LINE แล้ว");

    let s = state.clone();
    tokio::spawn(async move { sync_history(&s, full).await });
    Ok(())
}

/// Hide what other devices hid, and delete history they deleted.
fn apply_box_states(state: &AppState, hidden: &[String], removed: &[(String, i64)]) {
    let focused = state.focus.lock().unwrap().chat.clone();
    let store = state.store();
    for id in hidden.iter().filter(|id| Some(*id) != focused.as_ref()) {
        logged("hiding a chat hidden elsewhere", store.hide_chat(id));
    }
    let mut changed = Vec::new();
    for (id, through) in removed {
        match store.remove_history_through(id, *through) {
            Ok(true) => changed.push(id.clone()),
            Ok(false) => {}
            Err(e) => tracing::warn!("deleting history deleted elsewhere: {e}"),
        }
    }
    drop(store);
    for id in changed {
        if state.store().messages(&id, 1).map(|m| m.is_empty()).unwrap_or(false) {
            logged("clearing a preview", state.store().clear_preview(&id));
        } else {
            refresh_preview(state, &id);
        }
        state.messages_changed(&id);
    }
}

/// Leave out of the list every chat LINE no longer lists, except Keep Memo
/// and the chat on screen. A new message brings one back.
fn drop_unlisted(state: &AppState, line: &Line, listed: &HashSet<String>) {
    let focused = state.focus.lock().unwrap().chat.clone();
    let store = state.store();
    let Ok(visible) = store.chats() else { return };
    for c in visible {
        if listed.contains(&c.id) || c.id == line.me || Some(&c.id) == focused.as_ref() {
            continue;
        }
        logged("leaving out a chat LINE no longer lists", store.hide_chat(&c.id));
    }
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
        match fetch_history(state, &chat).await {
            Ok(_) => logged("saving sync state", state.store().mark_synced(&chat, activity)),
            Err(e) => tracing::warn!("syncing a chat: {e:#}"),
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
            logged("loading members", load_members(&s, &c, true).await);
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
    let others = |key: &str| -> Vec<String> {
        raw["extra"]["groupExtra"][key]
            .as_object()
            .map(|m| m.keys().filter(|k| **k != line.me).cloned().collect())
            .unwrap_or_default()
    };
    let mids = others("memberMids");
    let invitees = others("inviteeMids");
    let people = state.store().people()?;
    let missing: Vec<String> = mids
        .iter()
        .chain(&invitees)
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
    let member = |mid: &String, invited: bool| {
        let (name, picture) = people.get(mid).cloned().unwrap_or((mid.clone(), None));
        ChatMember {
            mid: mid.clone(),
            name,
            picture,
            all: false,
            invited,
        }
    };
    let members: Vec<ChatMember> = mids
        .iter()
        .map(|m| member(m, false))
        .chain(invitees.iter().map(|m| member(m, true)))
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
            apply_group(&mut c, &raw, &line.me, &people);
            logged("saving chat", store.upsert_chats(&[c]));
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

/// Make sure the raw media message is known: from memory, then from the
/// message stored in the database, then from the latest 100 in the chat.
/// `message_not_found` when none has it. Whether the file itself is gone
/// is only known from the download (`file_expired`).
pub async fn ensure_media_message(
    state: &Arc<AppState>,
    line: &Line,
    chat_id: &str,
    message_id: &str,
) -> Result<()> {
    if line.has_media(message_id) {
        return Ok(());
    }
    if let Some(raw) = stored_raw(state, message_id) {
        let ct = TalkMessage::new(line.client.clone(), raw.clone()).content_type();
        line.remember_media(&raw, &ct);
        if line.has_media(message_id) {
            return Ok(());
        }
    }
    for raw in recent_raw(line, chat_id, 100).await? {
        if raw["id"].as_str() == Some(message_id) {
            let (raw, _) = decrypt(line, raw).await;
            let ct = TalkMessage::new(line.client.clone(), raw.clone()).content_type();
            line.remember_media(&raw, &ct);
            return Ok(());
        }
    }
    Err(anyhow!("message_not_found"))
}

/// The error to show for a failed media download: `file_expired` only when
/// LINE's storage says the file is gone (HTTP 403, 404 or 410), as LINE
/// shows "deleted or expired" only then.
pub fn media_error(e: line_rs::Error) -> anyhow::Error {
    if e.is_media_expired() {
        anyhow!("file_expired")
    } else {
        anyhow!("download_failed: {}", e.full_message())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_gone_file_is_expired() {
        let obs = |status: u16| {
            line_rs::Error::internal("ObsError", format!("Object download failed: HTTP {status}"))
        };
        for status in [403, 404, 410] {
            assert_eq!(media_error(obs(status)).to_string(), "file_expired");
        }
        let failed = |e| media_error(e).to_string().starts_with("download_failed");
        for status in [401, 500, 503] {
            assert!(failed(obs(status)));
        }
        assert!(failed(line_rs::Error::internal("TimeoutError", "stalled")));
    }

    #[test]
    fn message_boxes_say_what_other_devices_did() {
        let b = tv!({"id": "u1", "lastDeliveredMessageId": {"messageId": 9007199254740993_i64}, "hiddenAtMessageId": 9007199254740993_i64, "lastRemovedMessageId": 50});
        assert_eq!(
            box_state(&b),
            BoxState { last_id: 9007199254740993, hidden: true, removed_through: Some(50) }
        );
        // A message after the hide brings the chat back.
        let b = tv!({"id": "u1", "lastDeliveredMessageId": {"messageId": 60}, "hiddenAtMessageId": 59});
        assert_eq!(box_state(&b), BoxState { last_id: 60, hidden: false, removed_through: None });
    }

    #[test]
    fn unnamed_groups_take_their_members_names() {
        let people: HashMap<String, (String, Option<String>)> = [
            ("u1".to_string(), ("Ann".to_string(), None)),
            ("u2".to_string(), ("Bo".to_string(), None)),
            ("me".to_string(), ("Me".to_string(), None)),
        ]
        .into();
        let auto = serde_json::json!({"chatName": "", "extra": {"groupExtra": {"autoName": true,
            "memberMids": {"u2": 5, "me": 1, "u1": 3, "u9": 4}}}});
        assert_eq!(group_name(&auto, "me", &people).as_deref(), Some("Ann, Bo"));
        let named = serde_json::json!({"chatName": "Team", "extra": {"groupExtra": {"memberMids": {"u1": 1}}}});
        assert_eq!(group_name(&named, "me", &people).as_deref(), Some("Team"));
        let unknown = serde_json::json!({"extra": {"groupExtra": {"memberMids": {"u9": 1}}}});
        assert_eq!(group_name(&unknown, "me", &people), None);
    }

    #[test]
    fn reads_get_contacts_v3_like_the_app() {
        let reply = serde_json::json!({"responses": [
            {
                "targetUserMid": "u1",
                "userType": "USER",
                "targetProfileDetail": {"profileName": "Profile", "picturePath": "/0h1"},
                "friendDetail": {"user": {"overriddenName": "Nick"}},
                "blockDetail": {"notBlocked": {}},
                "notificationSettingEntry": {"notificationSetting": {"mute": true}},
            },
            {
                "targetUserMid": "u2",
                "userType": "BOT",
                "targetProfileDetail": {"profileName": "Shop"},
                "friendDetail": {"notFriend": {}},
                "blockDetail": {"bot": {}},
            },
            {"userType": "USER"},
        ]});
        let c = contacts_from_v3(&reply);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].name.as_str(), c[0].picture.as_deref()), ("Nick", Some("/0h1")));
        assert_eq!((c[0].friend, c[0].blocked, c[0].muted, c[0].bot), (Some(true), Some(false), Some(true), false));
        assert_eq!((c[1].name.as_str(), c[1].bot), ("Shop", true));
        assert_eq!((c[1].friend, c[1].blocked), (Some(false), Some(true)));
    }

    /// A sealed message as thrift decodes it: a chunk that is valid UTF-8
    /// (the key ids here) arrives as a string, the others as binary.
    fn sealed_message() -> Value {
        let mut m = tv!({"id": "1", "to": "u-me", "contentType": "NONE"});
        m.insert(
            "chunks",
            Value::List(vec![
                Value::Binary(vec![0x80; 16]),
                Value::Binary(vec![0x81; 40]),
                Value::Binary(vec![0xff, 0xfe, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
                Value::String("\0\0\0\x05".into()),
                Value::String("\0\0\0\x07".into()),
            ]),
        );
        m
    }

    fn chunk_bytes(v: &Value) -> Vec<Vec<u8>> {
        v["chunks"]
            .as_list()
            .unwrap()
            .iter()
            .map(|c| c.as_bytes().unwrap().to_vec())
            .collect()
    }

    #[test]
    fn stored_chunks_keep_their_bytes() {
        let m = sealed_message();
        // What was stored before: binary as base64, UTF-8 chunks as text.
        // Read back as plain values, the 12-byte nonce became its 16-byte
        // base64 text, the "malformed nonce or ciphertext" line-rs refused.
        let naive = Value::from(m.to_json());
        assert_eq!(chunk_bytes(&naive)[2].len(), 16);
        // Such rows (key ids as text) now read back byte for byte.
        let legacy = raw_from_store(&m.to_json().to_string()).unwrap();
        assert_eq!(chunk_bytes(&legacy), chunk_bytes(&m));
        // New rows store every chunk as base64.
        let stored = message_json(&m);
        assert!(stored["chunks"].as_array().unwrap().iter().all(|c| c.is_string()));
        let back = raw_from_store(&stored.to_string()).unwrap();
        assert_eq!(chunk_bytes(&back), chunk_bytes(&m));
        assert_eq!(chunk_bytes(&back)[2].len(), 12);
    }

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
        let at = |path: &str, data: Value| line_rs::Error::Request {
            method: "m".into(),
            path: path.into(),
            data,
        };
        let refresh = line_rs::request::TOKEN_REFRESH_PATH;
        assert!(auth_error(&at(refresh, tv!({"errorCode": "INVALID_REQUEST"}))));
        assert!(auth_error(&at(refresh, tv!({"errorCode": "SOMETHING_ELSE"}))));
        assert!(!auth_error(&at(refresh, tv!({"errorCode": "RETRY_REQUIRED"}))));
        assert!(auth_error(&at("/S4", tv!({"code": "MUST_REFRESH_V3_TOKEN"}))));
        assert!(auth_error(&at("/S4", tv!({"code": "NOT_AUTHORIZED_DEVICE"}))));
        assert!(!auth_error(&at("/S4", tv!({"code": "INVALID_REQUEST"}))));
        assert!(!auth_error(&line_rs::Error::internal("TimeoutError", "timed out")));

        let failed = |method: &str, data: Value| {
            line_rs::Error::Request { method: method.into(), path: "/p".into(), data }.to_string()
        };
        assert!(token_dead(&failed("refresh", tv!({"errorCode": "INVALID_REQUEST"}))));
        assert!(!token_dead(&failed("refresh", tv!({"errorCode": "RETRY_REQUIRED"}))));
        assert!(token_dead(&failed("sync", tv!({"code": "MUST_REFRESH_V3_TOKEN"}))));
        assert!(!token_dead(&failed("sendMessage", tv!({"code": "INVALID_REQUEST"}))));
        assert!(!token_dead("http: connection reset"));
    }

    #[test]
    fn line_rs_ends_a_kicked_session_itself() {
        let talk = |code: &str| line_rs::Error::Request {
            method: "getServerTime".into(),
            path: "/S4".into(),
            data: tv!({"code": code, "reason": ""}),
        };
        assert!(kicked_out(&talk("AUTHENTICATION_FAILED")));
        assert!(kicked_out(&talk("NOT_AUTHORIZED_DEVICE")));
        assert!(!kicked_out(&talk("MUST_REFRESH_V3_TOKEN")));
    }
}
