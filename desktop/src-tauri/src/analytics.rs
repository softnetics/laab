//! Anonymous usage statistics and crash reports sent to PostHog, only after
//! the user agrees.
//!
//! What leaves the machine is fixed here, in Rust, and the webview's CSP
//! blocks PostHog. Each event carries only the properties in
//! [`Event::properties`], the app version, the macOS version and the CPU
//! architecture, under a random id that has nothing to do with the LINE
//! account. The id changes on sign-out, and turning statistics off deletes
//! it. PostHog is told not to build a person profile or look up a location
//! from the IP address.
//!
//! Never add message text, names, LINE ids (mids, chat ids), URLs, file names
//! or anything else that comes from LINE or from what the user typed. The
//! one exception is [`send_report`], which sends a problem report the user
//! read, could edit, and chose to send.
//!
//! Release builds send only when built with `LAAB_POSTHOG_KEY` (and
//! optionally `LAAB_POSTHOG_HOST`). Debug builds log the event instead of
//! sending it, also in demo mode so the flow can be tried without LINE;
//! release builds in demo mode do nothing.

use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use crate::crash::Exception;
use crate::state::AppState;
use crate::usage::Action;

const KEY: Option<&str> = option_env!("LAAB_POSTHOG_KEY");
const HOST: Option<&str> = option_env!("LAAB_POSTHOG_HOST");
const DEFAULT_HOST: &str = "https://us.i.posthog.com";

/// kv: "on" or "off". Missing means the user hasn't been asked.
const CHOICE: &str = "analytics";
/// kv: the random id events are sent under, while statistics are on.
const ID: &str = "analytics-id";
/// kv: the version that ran last, to notice updates.
const LAST_VERSION: &str = "last-version";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Login {
    Qr,
    Password,
    /// The token saved from an earlier sign-in, at launch.
    Saved,
}

impl Login {
    fn name(self) -> &'static str {
        match self {
            Login::Qr => "qr",
            Login::Password => "password",
            Login::Saved => "saved",
        }
    }
}

/// An error the app handled but the user noticed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Login(Login),
    /// LINE ended the session: the token expired or another device took over.
    SessionEnded { kicked: bool },
    Sync,
    Send(Action),
    /// The keychain key didn't open the database, so it was set aside.
    DatabaseUnreadable,
}

impl Failure {
    fn name(self) -> &'static str {
        match self {
            Failure::Login(_) => "login_failed",
            Failure::SessionEnded { .. } => "session_ended",
            Failure::Sync => "sync_failed",
            Failure::Send(_) => "send_failed",
            Failure::DatabaseUnreadable => "database_unreadable",
        }
    }

    fn detail(self) -> Option<&'static str> {
        match self {
            Failure::Login(l) => Some(l.name()),
            Failure::SessionEnded { kicked } => Some(if kicked { "kicked_out" } else { "expired" }),
            Failure::Send(a) => Some(a.name()),
            Failure::Sync | Failure::DatabaseUnreadable => None,
        }
    }
}

/// What an error was about, worked out from its text on this machine. Only
/// the category is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Network,
    Auth,
    RateLimited,
    Server,
    Other,
}

impl Cause {
    pub fn of(error: &str) -> Cause {
        let e = error.to_ascii_lowercase();
        let any = |words: &[&str]| words.iter().any(|w| e.contains(w));
        if any(&["not_authorized", "logged_out", "authentication", "token", "unauthorized"]) {
            Cause::Auth
        } else if any(&["rate", "too_many", "429", "abuse"]) {
            Cause::RateLimited
        } else if any(&["timed out", "timeout", "connect", "dns", "network", "offline", "unreachable", "broken pipe"]) {
            Cause::Network
        } else if any(&["internal_error", " 500", " 502", " 503", "server"]) {
            Cause::Server
        } else {
            Cause::Other
        }
    }

    fn name(self) -> &'static str {
        match self {
            Cause::Network => "network",
            Cause::Auth => "auth",
            Cause::RateLimited => "rate_limited",
            Cause::Server => "server",
            Cause::Other => "other",
        }
    }
}

/// Everything LAAB can report. Properties are fixed values, never data.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    AppOpened,
    /// The version that ran before this launch.
    AppUpdated { from: String },
    SignedIn(Login),
    SignedOut,
    /// One day's counts. `chats` is a range, not the number.
    DailyUsage {
        date: String,
        counts: Vec<(Action, u32)>,
        chats: &'static str,
    },
    Failed(Failure, Cause),
    /// A panic, a native crash or an uncaught webview error.
    Exception(Exception),
}

impl Event {
    fn name(&self) -> &'static str {
        match self {
            Event::AppOpened => "app_opened",
            Event::AppUpdated { .. } => "app_updated",
            Event::SignedIn(_) => "signed_in",
            Event::SignedOut => "signed_out",
            Event::DailyUsage { .. } => "daily_usage",
            Event::Failed(f, _) => f.name(),
            Event::Exception(_) => "$exception",
        }
    }

    fn properties(&self) -> Value {
        match self {
            Event::AppUpdated { from } => json!({ "from_version": from }),
            Event::SignedIn(l) => json!({ "method": l.name() }),
            Event::AppOpened | Event::SignedOut => json!({}),
            Event::DailyUsage { date, counts, chats } => {
                let mut p = json!({ "date": date, "chats": chats });
                for (action, n) in counts {
                    p[format!("n_{}", action.name())] = json!(n);
                }
                p
            }
            Event::Failed(f, cause) => json!({
                "detail": f.detail(),
                "cause": cause.name(),
                "breadcrumbs": crate::usage::crumbs(),
            }),
            Event::Exception(e) => json!({
                "$exception_list": e.list(),
                "breadcrumbs": e.crumbs,
                "crashed_version": e.version,
            }),
        }
    }
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Demo mode, or a release built without a PostHog key.
    Unavailable,
    Unasked,
    On,
    Off,
}

fn available(state: &AppState) -> bool {
    cfg!(debug_assertions) || (!state.demo && KEY.is_some_and(|k| !k.is_empty()))
}

pub fn status(state: &AppState) -> Status {
    if !available(state) {
        return Status::Unavailable;
    }
    match state.store().get_kv(CHOICE).ok().flatten().as_deref() {
        Some("on") => Status::On,
        Some("off") => Status::Off,
        _ => Status::Unasked,
    }
}

pub fn set(state: &AppState, on: bool) -> anyhow::Result<()> {
    if !available(state) {
        return Ok(());
    }
    {
        let store = state.store();
        store.set_kv(CHOICE, if on { "on" } else { "off" })?;
        store.delete_kv(ID)?;
    }
    if on {
        send(state, Event::AppOpened);
    }
    Ok(())
}

/// At launch: report the launch, and an update if the version changed.
/// At launch: report the launch, an update if the version changed, and what
/// crashed since the last run; then start counting usage.
pub fn startup(state: &std::sync::Arc<AppState>) {
    if status(state) == Status::Unavailable {
        return;
    }
    let version = state.app.package_info().version.to_string();
    let last = state.store().get_kv(LAST_VERSION).ok().flatten();
    let _ = state.store().set_kv(LAST_VERSION, &version);
    if let Some(from) = last.filter(|v| *v != version) {
        send(state, Event::AppUpdated { from });
    }
    send(state, Event::AppOpened);
    crate::crash::startup(state);
    crate::usage::start(state.clone());
}

/// Sign-out starts a new id, so the next account's use isn't linked to it.
pub fn signed_out(state: &AppState) {
    send(state, Event::SignedOut);
    let _ = state.store().delete_kv(ID);
}

/// Reports a handled error, classified from its text (which isn't sent).
pub fn failed(state: &AppState, failure: Failure, error: &str) {
    send(state, Event::Failed(failure, Cause::of(error)));
}

/// Sends `event` in the background if the user turned statistics on.
pub fn send(state: &AppState, event: Event) {
    if status(state) != Status::On {
        return;
    }
    let Ok(id) = install_id(state) else { return };
    let body = payload(&event, &id, &state.app.package_info().version.to_string());
    post(body);
}

/// Sends a problem report the user has read and confirmed. It goes out even
/// with statistics off, under a one-off id in that case.
pub fn send_report(state: &AppState, report: String) -> anyhow::Result<()> {
    if status(state) == Status::Unavailable {
        anyhow::bail!("รุ่นนี้ส่งรายงานไม่ได้");
    }
    let id = if status(state) == Status::On {
        install_id(state)?
    } else {
        random_id()
    };
    let mut body = payload(&Event::AppOpened, &id, &state.app.package_info().version.to_string());
    body["event"] = json!("problem_report");
    body["properties"]["report"] = json!(report);
    post(body);
    Ok(())
}

fn post(mut body: Value) {
    if cfg!(debug_assertions) {
        tracing::info!("analytics (debug build, not sent): {body}");
        return;
    }
    let Some(key) = KEY else { return };
    body["api_key"] = json!(key);
    let url = format!("{}/i/v0/e/", HOST.unwrap_or(DEFAULT_HOST).trim_end_matches('/'));
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build();
        // Best effort: a failed send is dropped, not retried.
        if let Ok(client) = client {
            let _ = client.post(url).json(&body).send().await;
        }
    });
}

fn random_id() -> String {
    use aes_gcm::aead::OsRng;
    use aes_gcm::aead::rand_core::RngCore;

    let mut raw = [0u8; 16];
    OsRng.fill_bytes(&mut raw);
    hex::encode(raw)
}

fn install_id(state: &AppState) -> anyhow::Result<String> {
    let store = state.store();
    if let Some(id) = store.get_kv(ID)? {
        return Ok(id);
    }
    let id = random_id();
    store.set_kv(ID, &id)?;
    Ok(id)
}

/// The whole event as sent, except `api_key`.
fn payload(event: &Event, id: &str, version: &str) -> Value {
    let mut properties = event.properties();
    let p = properties.as_object_mut().expect("properties are an object");
    p.insert("app_version".into(), json!(version));
    p.insert("os_version".into(), json!(os_version()));
    p.insert("arch".into(), json!(std::env::consts::ARCH));
    p.insert("$lib".into(), json!("laab"));
    // No person profile, and no location from the IP address.
    p.insert("$process_person_profile".into(), json!(false));
    p.insert("$geoip_disable".into(), json!(true));
    json!({
        "event": event.name(),
        "distinct_id": id,
        "properties": properties,
    })
}

/// `major.minor`, e.g. `15.4`.
fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        let v = objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersion();
        format!("{}.{}", v.majorVersion, v.minorVersion)
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::consts::OS.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards against an event growing a property by accident: everything
    /// sent is listed here.
    use crate::crash::Frame;

    /// Guards against an event growing a property by accident: everything
    /// sent is listed here.
    #[test]
    fn payload_holds_only_known_fields() {
        let exception = Exception {
            kind: "panic".into(),
            value: "src/store.rs:42".into(),
            lang: "rust".into(),
            handled: false,
            frames: vec![Frame {
                function: "laab_lib::store::Store::chats".into(),
                filename: Some("src/store.rs".into()),
                lineno: Some(42),
                colno: None,
                module: None,
                in_app: true,
            }],
            crumbs: vec!["opened_chat".into()],
            version: "0.1.0".into(),
        };
        let events = [
            Event::AppOpened,
            Event::AppUpdated { from: "0.1.0".into() },
            Event::SignedIn(Login::Qr),
            Event::SignedIn(Login::Password),
            Event::SignedOut,
            Event::DailyUsage {
                date: "2026-10-30".into(),
                counts: Action::ALL.into_iter().map(|a| (a, 1)).collect(),
                chats: "0-50",
            },
            Event::Failed(Failure::Login(Login::Saved), Cause::Network),
            Event::Failed(Failure::Send(Action::SentText), Cause::Other),
            Event::Exception(exception),
        ];
        let counts: Vec<String> = Action::ALL.iter().map(|a| format!("n_{}", a.name())).collect();
        for event in events {
            let body = payload(&event, "abc", "0.2.0");
            let mut top: Vec<_> = body.as_object().unwrap().keys().cloned().collect();
            top.sort();
            assert_eq!(top, ["distinct_id", "event", "properties"]);
            for key in body["properties"].as_object().unwrap().keys() {
                assert!(
                    [
                        "app_version",
                        "os_version",
                        "arch",
                        "$lib",
                        "$process_person_profile",
                        "$geoip_disable",
                        "from_version",
                        "method",
                        "date",
                        "chats",
                        "detail",
                        "cause",
                        "breadcrumbs",
                        "$exception_list",
                        "crashed_version",
                    ]
                    .contains(&key.as_str())
                        || counts.contains(key),
                    "{} sends {key}",
                    event.name()
                );
            }
            assert_eq!(body["properties"]["$process_person_profile"], false);
            assert_eq!(body["properties"]["$geoip_disable"], true);
        }
    }

    #[test]
    fn causes() {
        assert_eq!(Cause::of("error sending request: operation timed out"), Cause::Network);
        assert_eq!(Cause::of("TalkException(NOT_AUTHORIZED_DEVICE)"), Cause::Auth);
        assert_eq!(Cause::of("ABUSE_BLOCK"), Cause::RateLimited);
        assert_eq!(Cause::of("something else"), Cause::Other);
    }
}
