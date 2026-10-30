//! Shared app state and the events pushed to the frontend.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::line::Line;
use crate::model::Session;
use crate::paths::Paths;
use crate::store::Store;

pub struct AppState {
    pub app: AppHandle,
    pub paths: Paths,
    pub demo: bool,
    pub launched_ms: f64,
    store: Mutex<Store>,
    session: Mutex<Session>,
    pub line: tokio::sync::RwLock<Option<Arc<Line>>>,
    /// The chat the main window shows, and whether the app is in front.
    pub focus: Mutex<Focus>,
    pub notified: Mutex<HashSet<String>>,
    /// Members are loaded at most once per chat per run unless forced.
    pub members_loaded: Mutex<HashMap<String, f64>>,
    /// What the photo/video viewer window shows: `{ items, index }`.
    pub viewer: Mutex<serde_json::Value>,
}

#[derive(Debug, Default, Clone)]
pub struct Focus {
    pub chat: Option<String>,
    pub active: bool,
}

pub fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatEvent<'a> {
    chat_id: &'a str,
}

impl AppState {
    pub fn new(app: AppHandle, paths: Paths, demo: bool, store: Store) -> Arc<Self> {
        Arc::new(Self {
            app,
            paths,
            demo,
            launched_ms: now_ms(),
            store: Mutex::new(store),
            session: Mutex::new(Session {
                state: "starting".into(),
                ..Default::default()
            }),
            line: tokio::sync::RwLock::new(None),
            focus: Mutex::new(Focus {
                chat: None,
                active: true,
            }),
            notified: Mutex::new(HashSet::new()),
            members_loaded: Mutex::new(HashMap::new()),
            viewer: Mutex::new(serde_json::Value::Null),
        })
    }

    pub fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn session(&self) -> Session {
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn update_session(&self, f: impl FnOnce(&mut Session)) {
        let snapshot = {
            let mut s = self.session.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut s);
            s.clone()
        };
        let _ = self.app.emit("session", snapshot);
    }

    pub fn status(&self, text: impl Into<String>) {
        let text = text.into();
        self.update_session(|s| s.status = text);
    }

    pub async fn line(&self) -> Option<Arc<Line>> {
        self.line.read().await.clone()
    }

    pub fn my_mid(&self) -> String {
        self.session().profile.map(|p| p.mid).unwrap_or_default()
    }

    pub fn chats_changed(&self) {
        let _ = self.app.emit("chats-changed", ());
        crate::notify::update_badge(self);
    }

    pub fn messages_changed(&self, chat_id: &str) {
        let _ = self.app.emit("messages-changed", ChatEvent { chat_id });
    }

    pub fn reads_changed(&self, chat_id: &str) {
        let _ = self.app.emit("reads-changed", ChatEvent { chat_id });
    }

    pub fn members_changed(&self, chat_id: &str) {
        let _ = self.app.emit("members-changed", ChatEvent { chat_id });
    }
}
