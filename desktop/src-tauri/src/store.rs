//! Local SQLite store (`laab.sqlite`), encrypted with SQLCipher. The key
//! comes from the keychain (`keychain.rs`); demo data stays unencrypted.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::model::{
    Chat, ChatMember, MentionSpan, Message, Reaction, SearchHit, SearchKind, Workspace,
};

pub struct Store {
    db: Connection,
}

const PLACEHOLDER_FILE_NAMES: [&str; 3] = ["[File]", "[Image]", "[Video]"];

fn is_placeholder_file_name(name: Option<&str>) -> bool {
    match name {
        None => true,
        Some(n) => n.is_empty() || PLACEHOLDER_FILE_NAMES.contains(&n),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Draft {
    pub text: String,
    #[serde(default)]
    pub mentions: Vec<DraftMention>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DraftMention {
    pub mid: String,
    #[serde(default)]
    pub all: bool,
    pub label: String,
}

impl Store {
    /// `key` is 64 hex characters (a raw SQLCipher key). A database written
    /// before encryption is encrypted in place first.
    pub fn open(path: &Path, key: Option<&str>) -> Result<Self> {
        if let Some(dir) = path.parent() {
            crate::paths::private_dir(dir)?;
        }
        if let Some(key) = key
            && is_plaintext(path)
        {
            encrypt_in_place(path, key)?;
        }
        let db = Connection::open(path)?;
        if let Some(key) = key {
            db.execute_batch(&format!("PRAGMA key = \"x'{key}'\";"))?;
        }
        // Fails with SQLITE_NOTADB if the key does not match.
        db.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))?;
        let store = Self { db };
        store.migrate()?;
        Ok(store)
    }

    #[cfg(test)]
    pub fn memory() -> Result<Self> {
        let store = Self {
            db: Connection::open_in_memory()?,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let db = &self.db;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS chats (id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, preview TEXT NOT NULL, activity REAL NOT NULL, unread INTEGER NOT NULL, avatar TEXT, muted INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS messages (id TEXT PRIMARY KEY, chat_id TEXT NOT NULL, body TEXT NOT NULL, file_name TEXT, file_path TEXT, image_path TEXT, content_type TEXT NOT NULL, created REAL NOT NULL, mine INTEGER NOT NULL, sender_id TEXT, sticker_id TEXT, mentions TEXT);",
        )?;
        for (table, column, definition) in [
            ("chats", "picture", "TEXT"),
            ("chats", "member_count", "INTEGER"),
            ("messages", "sticker_package", "TEXT"),
            ("messages", "sticker_version", "TEXT"),
            ("messages", "metadata", "TEXT"),
            ("messages", "raw", "TEXT"),
            ("messages", "reply_to", "TEXT"),
            ("messages", "unsent", "INTEGER NOT NULL DEFAULT 0"),
            ("messages", "edited", "INTEGER NOT NULL DEFAULT 0"),
            ("messages", "file_size", "INTEGER"),
            ("messages", "duration_ms", "INTEGER"),
            // The chat's last activity at its last successful history sync.
            ("chats", "synced", "REAL"),
            // A hidden chat stays out of the list until it gets a newer message.
            ("chats", "hidden_at", "REAL"),
        ] {
            self.add_column_if_missing(table, column, definition)?;
        }
        db.execute_batch(
            "CREATE INDEX IF NOT EXISTS message_chat_time ON messages(chat_id, created);
             CREATE VIRTUAL TABLE IF NOT EXISTS message_search USING fts5(id UNINDEXED, body, file_name, tokenize='trigram');
             CREATE TABLE IF NOT EXISTS reads (chat_id TEXT NOT NULL, user_id TEXT NOT NULL, message_id TEXT NOT NULL, PRIMARY KEY(chat_id, user_id));
             CREATE TABLE IF NOT EXISTS workspaces (id TEXT PRIMARY KEY, name TEXT NOT NULL, position INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS pins (workspace_id TEXT NOT NULL, chat_id TEXT NOT NULL, position INTEGER NOT NULL, PRIMARY KEY(workspace_id, chat_id));
             CREATE TABLE IF NOT EXISTS members (chat_id TEXT NOT NULL, mid TEXT NOT NULL, name TEXT NOT NULL, picture TEXT, PRIMARY KEY(chat_id, mid));
             CREATE TABLE IF NOT EXISTS users (mid TEXT PRIMARY KEY, name TEXT NOT NULL, picture TEXT, kind TEXT);
             CREATE TABLE IF NOT EXISTS reactions (message_id TEXT NOT NULL, user_mid TEXT NOT NULL, type TEXT NOT NULL, mine INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(message_id, user_mid));
             CREATE TABLE IF NOT EXISTS drafts (chat_id TEXT PRIMARY KEY, body TEXT NOT NULL, mentions TEXT);
             CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             -- line-rs storage: E2EE keys, refresh token, counters (JSON values).
             CREATE TABLE IF NOT EXISTS session (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             -- The Raycast extension reads sender names from `people`.
             CREATE VIEW IF NOT EXISTS people AS SELECT mid AS id, name FROM users;",
        )?;
        let count: i64 = db.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0))?;
        if count == 0 {
            db.execute("INSERT INTO workspaces (id,name,position) VALUES ('main', 'หลัก', 0)", [])?;
        }
        Ok(())
    }

    fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self.db.prepare(&format!("PRAGMA table_info({table})"))?;
        let found = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|name| name == column);
        Ok(found)
    }

    fn add_column_if_missing(&self, table: &str, column: &str, definition: &str) -> Result<()> {
        if !self.has_column(table, column)? {
            self.db.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- kv

    pub fn get_kv(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .query_row("SELECT value FROM kv WHERE key=?", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_kv(&self, key: &str, value: &str) -> Result<()> {
        self.db.execute(
            "INSERT INTO kv(key,value) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn delete_kv(&self, key: &str) -> Result<()> {
        self.db.execute("DELETE FROM kv WHERE key=?", [key])?;
        Ok(())
    }

    /// Drop the database's references to a cache file that was deleted.
    pub fn forget_file(&self, path: &str) -> Result<()> {
        self.db.execute("UPDATE messages SET image_path=NULL WHERE image_path=?1", [path])?;
        self.db.execute("UPDATE messages SET file_path=NULL WHERE file_path=?1", [path])?;
        self.db.execute("UPDATE chats SET avatar=NULL WHERE avatar=?1", [path])?;
        Ok(())
    }

    /// The whole cache was cleared.
    pub fn forget_all_files(&self) -> Result<()> {
        self.db.execute_batch(
            "UPDATE messages SET image_path=NULL, file_path=NULL WHERE image_path IS NOT NULL OR file_path IS NOT NULL;
             UPDATE chats SET avatar=NULL WHERE avatar IS NOT NULL;",
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- session

    pub fn session_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .query_row("SELECT value FROM session WHERE key=?", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn session_set(&self, key: &str, value: &str) -> Result<()> {
        self.db.execute(
            "INSERT INTO session(key,value) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn session_delete(&self, key: &str) -> Result<()> {
        self.db.execute("DELETE FROM session WHERE key=?", [key])?;
        Ok(())
    }

    pub fn session_clear(&self) -> Result<()> {
        self.db.execute("DELETE FROM session", [])?;
        Ok(())
    }

    pub fn session_all(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.db.prepare("SELECT key, value FROM session")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    // ---------------------------------------------------------------- chats

    pub fn upsert_chats(&mut self, chats: &[Chat]) -> Result<()> {
        let tx = self.db.transaction()?;
        for c in chats {
            tx.execute(
                "INSERT INTO chats (id,name,kind,preview,activity,unread,avatar,muted,picture,member_count) VALUES (?,?,?,?,?,?,?,?,?,?)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name, kind=excluded.kind, preview=excluded.preview, activity=excluded.activity,
                 unread=excluded.unread, avatar=COALESCE(excluded.avatar, chats.avatar), muted=excluded.muted,
                 picture=COALESCE(excluded.picture, chats.picture), member_count=COALESCE(excluded.member_count, chats.member_count)",
                params![c.id, c.name, c.kind, c.preview, c.last_activity, c.unread, c.avatar, c.muted as i64, c.picture, c.member_count],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The chat list, without hidden chats.
    pub fn chats(&self) -> Result<Vec<Chat>> {
        self.chats_where("WHERE hidden_at IS NULL OR activity > hidden_at", [])
    }

    /// One chat, hidden or not.
    pub fn chat(&self, id: &str) -> Result<Option<Chat>> {
        Ok(self.chats_where("WHERE id=?", [id])?.pop())
    }

    fn chats_where<P: rusqlite::Params>(&self, filter: &str, params: P) -> Result<Vec<Chat>> {
        let mut stmt = self.db.prepare(&format!(
            "SELECT id,name,kind,preview,activity,unread,avatar,muted,picture,member_count FROM chats {filter} ORDER BY activity DESC"
        ))?;
        let rows = stmt.query_map(params, |r| {
            Ok(Chat {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                preview: r.get(3)?,
                last_activity: r.get(4)?,
                unread: r.get(5)?,
                avatar: r.get(6)?,
                muted: r.get::<_, i64>(7)? != 0,
                picture: r.get(8)?,
                member_count: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn hide_chat(&self, chat_id: &str) -> Result<()> {
        self.db
            .execute("UPDATE chats SET hidden_at=activity WHERE id=?", [chat_id])?;
        Ok(())
    }

    /// Remove one message from this device only (LINE's "ลบ").
    pub fn delete_message(&self, message_id: &str) -> Result<Option<String>> {
        let chat: Option<String> = self
            .db
            .query_row(
                "SELECT chat_id FROM messages WHERE id=?",
                [message_id],
                |r| r.get(0),
            )
            .optional()?;
        self.db
            .execute("DELETE FROM message_search WHERE id=?", [message_id])?;
        self.db
            .execute("DELETE FROM reactions WHERE message_id=?", [message_id])?;
        self.db
            .execute("DELETE FROM messages WHERE id=?", [message_id])?;
        Ok(chat)
    }

    /// Photos and videos, files, or messages with links, newest first.
    pub fn chat_media(&self, chat_id: &str, kind: &str) -> Result<Vec<Message>> {
        let filter = match kind {
            "media" => "content_type IN ('IMAGE','VIDEO')",
            "files" => "content_type='FILE'",
            _ => {
                "content_type='NONE' AND (body LIKE '%http://%' OR body LIKE '%https://%' OR body LIKE '%www.%')"
            }
        };
        let sql = format!(
            "SELECT {} FROM messages WHERE chat_id=? AND unsent=0 AND {filter} ORDER BY created DESC LIMIT 500",
            Self::MESSAGE_COLUMNS
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map([chat_id], Self::message_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn delete_history(&self, chat_id: &str) -> Result<()> {
        self.db.execute(
            "DELETE FROM message_search WHERE id IN (SELECT id FROM messages WHERE chat_id=?)",
            [chat_id],
        )?;
        self.db.execute(
            "DELETE FROM reactions WHERE message_id IN (SELECT id FROM messages WHERE chat_id=?)",
            [chat_id],
        )?;
        self.db
            .execute("DELETE FROM messages WHERE chat_id=?", [chat_id])?;
        self.db
            .execute("UPDATE chats SET preview='' WHERE id=?", [chat_id])?;
        Ok(())
    }

    pub fn set_unread(&self, chat_id: &str, unread: i64) -> Result<()> {
        self.db.execute(
            "UPDATE chats SET unread=? WHERE id=?",
            params![unread, chat_id],
        )?;
        Ok(())
    }

    pub fn set_muted(&self, chat_id: &str, muted: bool) -> Result<()> {
        self.db.execute(
            "UPDATE chats SET muted=? WHERE id=?",
            params![muted as i64, chat_id],
        )?;
        Ok(())
    }

    pub fn set_avatar(&self, chat_id: &str, path: &str) -> Result<()> {
        self.db.execute(
            "UPDATE chats SET avatar=? WHERE id=?",
            params![path, chat_id],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- people

    pub fn upsert_members(&mut self, chat_id: &str, members: &[ChatMember]) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM members WHERE chat_id=?", [chat_id])?;
        for m in members.iter().filter(|m| !m.all) {
            tx.execute(
                "INSERT OR REPLACE INTO members(chat_id,mid,name,picture) VALUES (?,?,?,?)",
                params![chat_id, m.mid, m.name, m.picture],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn members(&self, chat_id: &str) -> Result<Vec<ChatMember>> {
        let mut stmt = self.db.prepare(
            "SELECT m.mid, COALESCE(NULLIF(m.name,''), u.name, m.mid), COALESCE(m.picture, u.picture)
             FROM members m LEFT JOIN users u ON u.mid=m.mid WHERE m.chat_id=?",
        )?;
        let rows = stmt.query_map([chat_id], |r| {
            Ok(ChatMember {
                mid: r.get(0)?,
                name: r.get(1)?,
                picture: r.get(2)?,
                all: false,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn upsert_users(
        &mut self,
        users: &[(String, String, Option<String>, Option<String>)],
    ) -> Result<()> {
        let tx = self.db.transaction()?;
        for (mid, name, picture, kind) in users {
            tx.execute(
                "INSERT INTO users(mid,name,picture,kind) VALUES (?,?,?,?) ON CONFLICT(mid) DO UPDATE SET
                 name=CASE WHEN excluded.name='' THEN users.name ELSE excluded.name END,
                 picture=COALESCE(excluded.picture, users.picture), kind=COALESCE(excluded.kind, users.kind)",
                params![mid, name, picture, kind],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// mid → (display name, avatar path) for everyone we know.
    pub fn people(&self) -> Result<HashMap<String, (String, Option<String>)>> {
        let mut out = HashMap::new();
        let mut stmt = self.db.prepare("SELECT mid,name,picture FROM users")?;
        for row in stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?))
        })? {
            let (mid, name, pic) = row?;
            out.insert(mid, (name, pic));
        }
        // 1:1 chat rows hold the friend's name and avatar too.
        let mut stmt = self
            .db
            .prepare("SELECT id,name,avatar FROM chats WHERE id LIKE 'u%'")?;
        for row in stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?))
        })? {
            let (mid, name, pic) = row?;
            out.entry(mid.clone()).or_insert((name, pic));
        }
        Ok(out)
    }

    // ---------------------------------------------------------------- messages

    fn preferred_file_name(
        &self,
        incoming: Option<&str>,
        message_id: &str,
    ) -> Result<Option<String>> {
        if !is_placeholder_file_name(incoming) {
            return Ok(incoming.map(str::to_string));
        }
        let stored: Option<String> = self
            .db
            .query_row(
                "SELECT file_name FROM messages WHERE id=?",
                [message_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(if is_placeholder_file_name(stored.as_deref()) {
            incoming.map(str::to_string)
        } else {
            stored
        })
    }

    pub fn upsert_messages(&mut self, messages: &[Message]) -> Result<()> {
        let names: Vec<Option<String>> = messages
            .iter()
            .map(|m| self.preferred_file_name(m.file_name.as_deref(), &m.id))
            .collect::<Result<_>>()?;
        let tx = self.db.transaction()?;
        for (m, file_name) in messages.iter().zip(names) {
            let mentions = if m.mentions.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&m.mentions)?)
            };
            let metadata = if m.metadata.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&m.metadata)?)
            };
            tx.execute(
                "INSERT INTO messages(id,chat_id,body,file_name,file_path,image_path,content_type,created,mine,sender_id,sticker_id,mentions,
                    sticker_package,sticker_version,metadata,raw,reply_to,unsent,edited,file_size,duration_ms)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                 ON CONFLICT(id) DO UPDATE SET chat_id=excluded.chat_id, body=excluded.body,
                    file_name=COALESCE(excluded.file_name,messages.file_name), file_path=COALESCE(excluded.file_path,messages.file_path),
                    image_path=COALESCE(excluded.image_path,messages.image_path), content_type=excluded.content_type, created=excluded.created,
                    mine=excluded.mine, sender_id=COALESCE(excluded.sender_id,messages.sender_id), sticker_id=COALESCE(excluded.sticker_id,messages.sticker_id),
                    mentions=COALESCE(excluded.mentions,messages.mentions), sticker_package=COALESCE(excluded.sticker_package,messages.sticker_package),
                    sticker_version=COALESCE(excluded.sticker_version,messages.sticker_version), metadata=COALESCE(excluded.metadata,messages.metadata),
                    raw=COALESCE(excluded.raw,messages.raw), reply_to=COALESCE(excluded.reply_to,messages.reply_to),
                    unsent=MAX(excluded.unsent,messages.unsent), edited=MAX(excluded.edited,messages.edited),
                    file_size=COALESCE(excluded.file_size,messages.file_size), duration_ms=COALESCE(excluded.duration_ms,messages.duration_ms)",
                params![
                    m.id, m.chat_id, m.text, file_name, m.file_path, m.image_path, m.content_type, m.created_time, m.mine as i64,
                    m.sender_id, m.sticker_id, mentions, m.sticker_package, m.sticker_version, metadata, m.raw, m.reply_to,
                    m.unsent, m.edited as i64, m.file_size, m.duration_ms
                ],
            )?;
            // Once unsent, a stale history copy must never bring the text back.
            tx.execute(
                "UPDATE messages SET body='', file_name=NULL, file_path=NULL, image_path=NULL, mentions=NULL WHERE id=? AND unsent>0",
                [&m.id],
            )?;
            tx.execute("DELETE FROM message_search WHERE id=?", [&m.id])?;
            if m.unsent == 0 {
                tx.execute(
                    "INSERT INTO message_search(id, body, file_name) VALUES (?,?,?)",
                    params![m.id, m.text, file_name.clone().unwrap_or_default()],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn contains_message(&self, id: &str) -> Result<bool> {
        Ok(self
            .db
            .query_row(
                "SELECT 1 FROM messages WHERE id=? LIMIT 1",
                [id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    const MESSAGE_COLUMNS: &'static str = "id,chat_id,body,file_name,file_path,image_path,content_type,created,mine,sender_id,sticker_id,mentions,
        sticker_package,sticker_version,metadata,reply_to,unsent,edited,file_size,duration_ms";

    fn message_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
        let mentions: Option<String> = r.get(11)?;
        let metadata: Option<String> = r.get(14)?;
        Ok(Message {
            id: r.get(0)?,
            chat_id: r.get(1)?,
            text: r.get(2)?,
            file_name: r.get(3)?,
            file_path: r.get(4)?,
            image_path: r.get(5)?,
            content_type: r.get(6)?,
            created_time: r.get(7)?,
            mine: r.get::<_, i64>(8)? != 0,
            sender_id: r.get(9)?,
            sticker_id: r.get(10)?,
            mentions: mentions
                .and_then(|s| serde_json::from_str::<Vec<MentionSpan>>(&s).ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|m| m.start >= 0 && m.end > m.start)
                .collect(),
            sticker_package: r.get(12)?,
            sticker_version: r.get(13)?,
            metadata: metadata
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            reply_to: r.get(15)?,
            unsent: r.get(16)?,
            edited: r.get::<_, i64>(17)? != 0,
            file_size: r.get(18)?,
            duration_ms: r.get(19)?,
            ..Default::default()
        })
    }

    /// The `(deliveredTime, messageId)` of a chat's oldest stored LINE message:
    /// where the next page of older history starts. Local ids are skipped.
    pub fn oldest_cursor(&self, chat_id: &str) -> Result<Option<(i64, i64)>> {
        let mut stmt = self.db.prepare(
            "SELECT id, created, raw FROM messages WHERE chat_id=? ORDER BY created LIMIT 20",
        )?;
        let rows = stmt.query_map([chat_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in rows {
            let (id, created, raw) = row?;
            let Ok(id) = id.parse::<i64>() else { continue };
            let delivered = raw
                .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
                .and_then(|v| {
                    let d = &v["deliveredTime"];
                    d.as_i64()
                        .or_else(|| d.as_str().and_then(|s| s.parse().ok()))
                })
                .unwrap_or(created as i64);
            return Ok(Some((delivered, id)));
        }
        Ok(None)
    }

    /// The latest available message to show when reopening an unread chat.
    pub fn unread_open_target(&self, chat_id: &str) -> Result<Option<String>> {
        Ok(self.db.query_row(
            "SELECT id FROM messages WHERE chat_id=? AND unsent=0 ORDER BY created DESC LIMIT 1",
            [chat_id],
            |r| r.get(0),
        ).optional()?)
    }

    /// The newest `limit` messages of a chat, oldest first.
    pub fn messages(&self, chat_id: &str, limit: usize) -> Result<Vec<Message>> {
        let sql = format!(
            "SELECT * FROM (SELECT {} FROM messages WHERE chat_id=? ORDER BY created DESC LIMIT ?) ORDER BY created",
            Self::MESSAGE_COLUMNS
        );
        let mut stmt = self.db.prepare(&sql)?;
        let mut list: Vec<Message> = stmt
            .query_map(params![chat_id, limit as i64], Self::message_row)?
            .collect::<rusqlite::Result<_>>()?;
        self.attach_reactions(&mut list)?;
        Ok(list)
    }

    pub fn message(&self, id: &str) -> Result<Option<Message>> {
        let sql = format!("SELECT {} FROM messages WHERE id=?", Self::MESSAGE_COLUMNS);
        let mut list: Vec<Message> = self
            .db
            .prepare(&sql)?
            .query_map([id], Self::message_row)?
            .collect::<rusqlite::Result<_>>()?;
        self.attach_reactions(&mut list)?;
        Ok(list.pop())
    }

    pub fn raw_message(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .query_row("SELECT raw FROM messages WHERE id=?", [id], |r| r.get(0))
            .optional()?
            .flatten())
    }

    fn attach_reactions(&self, list: &mut [Message]) -> Result<()> {
        if list.is_empty() {
            return Ok(());
        }
        let mut stmt = self
            .db
            .prepare("SELECT user_mid, type FROM reactions WHERE message_id=? ORDER BY user_mid")?;
        for m in list.iter_mut() {
            m.reactions = stmt
                .query_map([&m.id], |r| {
                    Ok(Reaction {
                        user_id: r.get(0)?,
                        reaction: r.get(1)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
        }
        Ok(())
    }

    /// Returns the chat id and any cached media paths, or `None` for an
    /// unknown message.
    pub fn mark_unsent(&self, message_id: &str) -> Result<Option<(String, Vec<String>)>> {
        let row: Option<(String, Option<String>, Option<String>)> = self
            .db
            .query_row(
                "SELECT chat_id, file_path, image_path FROM messages WHERE id=?",
                [message_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((chat, file, image)) = row else {
            return Ok(None);
        };
        self.db.execute(
            "UPDATE messages SET unsent=MAX(unsent,1), body='', file_name=NULL, file_path=NULL, image_path=NULL, mentions=NULL WHERE id=?",
            [message_id],
        )?;
        self.db
            .execute("DELETE FROM message_search WHERE id=?", [message_id])?;
        self.db
            .execute("DELETE FROM reactions WHERE message_id=?", [message_id])?;
        Ok(Some((chat, [file, image].into_iter().flatten().collect())))
    }

    pub fn synced(&self) -> Result<HashMap<String, f64>> {
        let mut stmt = self
            .db
            .prepare("SELECT id, synced FROM chats WHERE synced IS NOT NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn mark_synced(&self, chat_id: &str, activity: f64) -> Result<()> {
        self.db.execute(
            "UPDATE chats SET synced=? WHERE id=?",
            params![activity, chat_id],
        )?;
        Ok(())
    }

    pub fn set_reaction(
        &self,
        message_id: &str,
        user_id: &str,
        reaction: Option<&str>,
    ) -> Result<Option<String>> {
        match reaction {
            Some(r) => self.db.execute(
                "INSERT INTO reactions(message_id,user_mid,type) VALUES (?,?,?) ON CONFLICT(message_id,user_mid) DO UPDATE SET type=excluded.type",
                params![message_id, user_id, r],
            )?,
            None => self.db.execute("DELETE FROM reactions WHERE message_id=? AND user_mid=?", params![message_id, user_id])?,
        };
        Ok(self
            .db
            .query_row(
                "SELECT chat_id FROM messages WHERE id=?",
                [message_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn replace_reactions(&mut self, message_id: &str, reactions: &[Reaction]) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM reactions WHERE message_id=?", [message_id])?;
        for r in reactions {
            tx.execute(
                "INSERT OR REPLACE INTO reactions(message_id,user_mid,type) VALUES (?,?,?)",
                params![message_id, r.user_id, r.reaction],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Files stored without a name, with their raw message: E2EE files
    /// synced before LAAB read names from the encrypted payload.
    pub fn unnamed_files(&self) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self.db.prepare(
            "SELECT id, chat_id, raw FROM messages WHERE content_type='FILE' AND file_name IS NULL AND unsent=0 AND raw IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn set_file_name(&self, message_id: &str, name: &str) -> Result<()> {
        self.db.execute(
            "UPDATE messages SET file_name=? WHERE id=? AND file_name IS NULL",
            params![name, message_id],
        )?;
        Ok(())
    }

    pub fn update_media(
        &self,
        message_id: &str,
        image_path: Option<&str>,
        file_path: Option<&str>,
    ) -> Result<()> {
        self.db.execute(
            "UPDATE messages SET image_path=COALESCE(?,image_path), file_path=COALESCE(?,file_path) WHERE id=?",
            params![image_path, file_path, message_id],
        )?;
        Ok(())
    }

    /// Records a completed file download. Placeholder names such as "[File]"
    /// do not replace a real filename.
    pub fn remember_download(
        &self,
        message_id: &str,
        path: &str,
        file_name: Option<&str>,
    ) -> Result<()> {
        let name = self.preferred_file_name(file_name, message_id)?;
        self.db.execute(
            "UPDATE messages SET file_path=?, file_name=COALESCE(?, file_name) WHERE id=?",
            params![path, name, message_id],
        )?;
        let body: String = self
            .db
            .query_row("SELECT body FROM messages WHERE id=?", [message_id], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or_default();
        self.db
            .execute("DELETE FROM message_search WHERE id=?", [message_id])?;
        self.db.execute(
            "INSERT INTO message_search(id, body, file_name) VALUES (?,?,?)",
            params![message_id, body, name.unwrap_or_default()],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- reads

    pub fn reads(&self, chat_id: &str) -> Result<HashMap<String, String>> {
        let mut stmt = self
            .db
            .prepare("SELECT user_id, message_id FROM reads WHERE chat_id=?")?;
        let rows = stmt.query_map([chat_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_read(&self, chat_id: &str, user_id: &str, message_id: &str) -> Result<()> {
        self.db.execute(
            "INSERT INTO reads (chat_id, user_id, message_id) VALUES (?,?,?) ON CONFLICT(chat_id, user_id) DO UPDATE SET message_id=excluded.message_id",
            [chat_id, user_id, message_id],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- drafts

    pub fn draft(&self, chat_id: &str) -> Result<Option<Draft>> {
        let row: Option<(String, Option<String>)> = self
            .db
            .query_row(
                "SELECT body, mentions FROM drafts WHERE chat_id=?",
                [chat_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row.map(|(text, mentions)| Draft {
            text,
            mentions: mentions
                .and_then(|m| serde_json::from_str(&m).ok())
                .unwrap_or_default(),
        }))
    }

    pub fn set_draft(&self, chat_id: &str, draft: &Draft) -> Result<()> {
        if draft.text.is_empty() {
            self.db
                .execute("DELETE FROM drafts WHERE chat_id=?", [chat_id])?;
        } else {
            self.db.execute(
                "INSERT INTO drafts(chat_id,body,mentions) VALUES (?,?,?) ON CONFLICT(chat_id) DO UPDATE SET body=excluded.body, mentions=excluded.mentions",
                params![chat_id, draft.text, serde_json::to_string(&draft.mentions)?],
            )?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- folders

    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        let mut stmt = self
            .db
            .prepare("SELECT id,name,position FROM workspaces ORDER BY position")?;
        let rows = stmt.query_map([], |r| {
            Ok(Workspace {
                id: r.get(0)?,
                name: r.get(1)?,
                position: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn add_workspace(&self, name: &str) -> Result<Workspace> {
        let position: i64 = self
            .db
            .query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0))?;
        let id = format!(
            "ws-{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        self.db.execute(
            "INSERT INTO workspaces (id,name,position) VALUES (?,?,?)",
            params![id, name, position],
        )?;
        Ok(Workspace {
            id,
            name: name.to_string(),
            position,
        })
    }

    pub fn pins(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut out: HashMap<String, Vec<String>> = HashMap::new();
        let mut stmt = self
            .db
            .prepare("SELECT workspace_id, chat_id FROM pins ORDER BY position")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (ws, chat) = row?;
            out.entry(ws).or_default().push(chat);
        }
        Ok(out)
    }

    pub fn set_pinned(&self, pinned: bool, chat_id: &str, workspace_id: &str) -> Result<()> {
        if pinned {
            self.db.execute(
                "INSERT OR IGNORE INTO pins VALUES (?,?,(SELECT COALESCE(MAX(position)+1,0) FROM pins WHERE workspace_id=?))",
                params![workspace_id, chat_id, workspace_id],
            )?;
        } else {
            self.db.execute(
                "DELETE FROM pins WHERE workspace_id=? AND chat_id=?",
                params![workspace_id, chat_id],
            )?;
        }
        Ok(())
    }

    /// A folder holding `chats`, the first of which leaves `from` (a drag
    /// out of another folder). Named later; the list shows the chats' names
    /// until then.
    pub fn create_folder(&mut self, name: &str, chats: &[String], from: Option<&str>) -> Result<Workspace> {
        let w = self.add_workspace(name)?;
        if let (Some(from), Some(first)) = (from, chats.first()) {
            self.move_to_folder(first, Some(from), None)?;
        }
        for chat in chats {
            self.set_pinned(true, chat, &w.id)?;
        }
        Ok(w)
    }

    /// Move a chat between folders; `None` is the plain list. A folder the
    /// move empties goes away, as in Discord.
    pub fn move_to_folder(&mut self, chat_id: &str, from: Option<&str>, to: Option<&str>) -> Result<()> {
        let tx = self.db.transaction()?;
        if let Some(from) = from.filter(|f| *f != "main") {
            tx.execute("DELETE FROM pins WHERE workspace_id=? AND chat_id=?", params![from, chat_id])?;
            let left: i64 = tx.query_row("SELECT COUNT(*) FROM pins WHERE workspace_id=?", [from], |r| r.get(0))?;
            if left == 0 && to != Some(from) {
                tx.execute("DELETE FROM workspaces WHERE id=?", [from])?;
            }
        }
        if let Some(to) = to {
            tx.execute(
                "INSERT OR IGNORE INTO pins VALUES (?,?,(SELECT COALESCE(MAX(position)+1,0) FROM pins WHERE workspace_id=?))",
                params![to, chat_id, to],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn rename_workspace(&self, id: &str, name: &str) -> Result<()> {
        self.db.execute("UPDATE workspaces SET name=? WHERE id=?", params![name, id])?;
        Ok(())
    }

    /// Delete a folder; its chats go back to the plain list. Pins stay.
    pub fn delete_workspace(&mut self, id: &str) -> Result<()> {
        if id == "main" {
            anyhow::bail!("pins are not a folder");
        }
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM pins WHERE workspace_id=?", [id])?;
        tx.execute("DELETE FROM workspaces WHERE id=?", [id])?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- search

    pub fn search(&self, query: &str, kinds: &[SearchKind]) -> Result<Vec<SearchHit>> {
        let term = query.trim();
        let has = |k: SearchKind| kinds.contains(&k);
        let mut results = Vec::new();
        if term.is_empty() {
            // ⌘K with an empty field lists recent chats so a conversation can
            // be opened without typing.
            if has(SearchKind::Chat) {
                let mut stmt = self
                    .db
                    .prepare("SELECT id,name,preview FROM chats ORDER BY activity DESC LIMIT 30")?;
                for row in stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))? {
                    let (chat_id, title, detail) = row?;
                    results.push(SearchHit {
                        kind: SearchKind::Chat,
                        chat_id,
                        message_id: None,
                        title,
                        detail,
                    });
                }
            }
            return Ok(results);
        }
        let like = format!(
            "%{}%",
            term.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        if has(SearchKind::Chat) {
            let mut stmt = self.db.prepare(
                "SELECT id,name,preview FROM chats WHERE name LIKE ? ESCAPE '\\' ORDER BY activity DESC LIMIT 30",
            )?;
            for row in stmt.query_map([&like], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))? {
                let (chat_id, title, detail) = row?;
                results.push(SearchHit {
                    kind: SearchKind::Chat,
                    chat_id,
                    message_id: None,
                    title,
                    detail,
                });
            }
        }
        if has(SearchKind::Message) || has(SearchKind::File) {
            // The trigram tokenizer needs at least three characters.
            let use_fts = term.chars().count() >= 3;
            let base = "SELECT m.id,m.chat_id,c.name,m.body,m.file_name,m.content_type";
            let (sql, values): (String, Vec<String>) = if use_fts {
                (
                    format!(
                        "{base} FROM message_search s JOIN messages m ON m.id=s.id LEFT JOIN chats c ON c.id=m.chat_id WHERE message_search MATCH ? AND m.unsent=0 ORDER BY m.created DESC LIMIT 80"
                    ),
                    vec![format!("\"{}\"", term.replace('"', "\"\""))],
                )
            } else {
                (
                    format!(
                        "{base} FROM messages m LEFT JOIN chats c ON c.id=m.chat_id WHERE (m.body LIKE ?1 ESCAPE '\\' OR m.file_name LIKE ?1 ESCAPE '\\') AND m.unsent=0 ORDER BY m.created DESC LIMIT 80"
                    ),
                    vec![like.clone()],
                )
            };
            let mut stmt = self.db.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })?;
            for row in rows {
                let (id, chat_id, name, body, file, ct) = row?;
                let kind = if ct == "FILE" {
                    SearchKind::File
                } else {
                    SearchKind::Message
                };
                if !has(kind) {
                    continue;
                }
                let name = name.unwrap_or_else(|| "แชท".into());
                let (title, detail) = match file {
                    Some(f) if kind == SearchKind::File => (f, name),
                    _ => (name, body),
                };
                results.push(SearchHit {
                    kind,
                    chat_id,
                    message_id: Some(id),
                    title,
                    detail,
                });
            }
        }
        Ok(results)
    }
}

/// A database file SQLite can read without a key.
fn is_plaintext(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && &head == b"SQLite format 3\0"
}

/// Rewrite an unencrypted database with `key`, then delete the plain copy.
fn encrypt_in_place(path: &Path, key: &str) -> Result<()> {
    let tmp = path.with_extension("sqlite.encrypting");
    let _ = std::fs::remove_file(&tmp);
    {
        let db = Connection::open(path)?;
        db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        db.execute(
            &format!("ATTACH DATABASE ?1 AS encrypted KEY \"x'{key}'\""),
            [tmp.to_string_lossy()],
        )?;
        db.query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))?;
        db.execute("DETACH DATABASE encrypted", [])?;
    }
    std::fs::rename(&tmp, path)?;
    for suffix in ["-wal", "-shm"] {
        let mut side = path.as_os_str().to_owned();
        side.push(suffix);
        let _ = std::fs::remove_file(side);
    }
    tracing::info!("encrypted {}", path.display());
    Ok(())
}

/// The database exists but the key does not open it.
pub fn is_wrong_key(e: &anyhow::Error) -> bool {
    e.downcast_ref::<rusqlite::Error>()
        .and_then(|e| e.sqlite_error_code())
        == Some(rusqlite::ErrorCode::NotADatabase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, chat: &str, text: &str, created: f64) -> Message {
        Message {
            id: id.into(),
            chat_id: chat.into(),
            text: text.into(),
            content_type: "NONE".into(),
            created_time: created,
            ..Default::default()
        }
    }

    fn chat(id: &str, name: &str) -> Chat {
        Chat {
            id: id.into(),
            name: name.into(),
            kind: "dm".into(),
            ..Default::default()
        }
    }

    #[test]
    fn finds_thai_substrings_with_the_trigram_index() {
        let mut s = Store::memory().unwrap();
        s.upsert_chats(&[chat("u1", "กานดา")]).unwrap();
        s.upsert_messages(&[
            msg("1", "u1", "ส่งไฟล์สรุปประชุมให้แล้ว", 1.0),
            msg("2", "u1", "ok", 2.0),
        ])
        .unwrap();
        let hits = s.search("สรุปประชุม", &[SearchKind::Message]).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "กานดา");
        // Two characters fall back to LIKE.
        assert_eq!(s.search("ok", &[SearchKind::Message]).unwrap().len(), 1);
        // An empty query lists recent chats.
        assert_eq!(s.search("  ", &[SearchKind::Chat]).unwrap().len(), 1);
    }

    #[test]
    fn older_history_starts_at_the_oldest_line_message() {
        let mut s = Store::memory().unwrap();
        assert_eq!(s.oldest_cursor("u1").unwrap(), None);
        let mut old = msg("9007199254740993", "u1", "first", 5.0);
        old.raw = Some(r#"{"deliveredTime": 7}"#.into());
        s.upsert_messages(&[
            msg("local-1", "u1", "pending", 1.0),
            old,
            msg("200", "u1", "later", 9.0),
        ])
        .unwrap();
        // The local id is skipped; the 64-bit id stays exact.
        assert_eq!(
            s.oldest_cursor("u1").unwrap(),
            Some((7, 9_007_199_254_740_993))
        );
    }

    #[test]
    fn keeps_real_file_names_over_placeholders() {
        let mut s = Store::memory().unwrap();
        let mut m = msg("f", "u1", "", 1.0);
        m.content_type = "FILE".into();
        m.file_name = Some("report.pdf".into());
        s.upsert_messages(&[m.clone()]).unwrap();
        m.file_name = Some("[File]".into());
        s.upsert_messages(&[m]).unwrap();
        assert_eq!(
            s.message("f").unwrap().unwrap().file_name.as_deref(),
            Some("report.pdf")
        );
        assert_eq!(
            s.search("report", &[SearchKind::File]).unwrap()[0].title,
            "report.pdf"
        );
    }

    #[test]
    fn unread_open_skips_all_trailing_unsends() {
        let mut s = Store::memory().unwrap();
        s.upsert_messages(&[
            msg("1", "u1", "available", 1.0),
            msg("2", "u1", "unsent", 2.0),
            msg("3", "u1", "silent", 3.0),
            msg("4", "other", "another chat", 4.0),
        ]).unwrap();
        s.mark_unsent("2").unwrap();
        s.db.execute("UPDATE messages SET unsent=2 WHERE id='3'", []).unwrap();
        assert_eq!(s.unread_open_target("u1").unwrap().as_deref(), Some("1"));
        s.mark_unsent("1").unwrap();
        assert_eq!(s.unread_open_target("u1").unwrap(), None);
        assert_eq!(s.unread_open_target("empty").unwrap(), None);
        let mut image = msg("5", "u1", "", 5.0);
        image.content_type = "IMAGE".into();
        s.upsert_messages(&[image]).unwrap();
        assert_eq!(s.unread_open_target("u1").unwrap().as_deref(), Some("5"));
    }

    #[test]
    fn unsent_messages_never_come_back() {
        let mut s = Store::memory().unwrap();
        s.upsert_messages(&[msg("1", "u1", "secret text", 1.0)])
            .unwrap();
        s.mark_unsent("1").unwrap();
        s.upsert_messages(&[msg("1", "u1", "secret text", 1.0)])
            .unwrap();
        let m = s.message("1").unwrap().unwrap();
        assert_eq!((m.unsent, m.text.as_str()), (1, ""));
        assert!(
            s.search("secret", &[SearchKind::Message])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn pins_folders_reads_and_reactions() {
        let mut s = Store::memory().unwrap();
        s.set_pinned(true, "c1", "main").unwrap();
        s.set_pinned(true, "c2", "main").unwrap();
        let w = s.add_workspace("Work").unwrap();
        s.set_pinned(true, "c2", &w.id).unwrap();
        let pins = s.pins().unwrap();
        assert_eq!(pins["main"], vec!["c1", "c2"]);
        assert_eq!(pins[&w.id], vec!["c2"]);
        s.set_read("c1", "u1", "5").unwrap();
        s.set_read("c1", "u1", "7").unwrap();
        assert_eq!(s.reads("c1").unwrap()["u1"], "7");
        s.upsert_messages(&[msg("9", "c1", "hi", 1.0)]).unwrap();
        assert_eq!(
            s.set_reaction("9", "u1", Some("LOVE")).unwrap().as_deref(),
            Some("c1")
        );
        s.set_reaction("9", "u2", Some("NICE")).unwrap();
        s.set_reaction("9", "u1", None).unwrap();
        assert_eq!(s.message("9").unwrap().unwrap().reactions.len(), 1);
    }

    #[test]
    fn folders_made_by_dragging() {
        let mut s = Store::memory().unwrap();
        let a = s.create_folder("", &["c1".into(), "c2".into()], None).unwrap();
        assert_eq!(s.pins().unwrap()[&a.id], vec!["c1", "c2"]);
        // c2 dragged out of `a` onto c3 makes a second folder.
        let b = s.create_folder("", &["c2".into(), "c3".into()], Some(&a.id)).unwrap();
        assert_eq!(s.pins().unwrap()[&a.id], vec!["c1"]);
        assert_eq!(s.pins().unwrap()[&b.id], vec!["c2", "c3"]);
        // Dragging the last chat out removes the folder.
        s.move_to_folder("c1", Some(&a.id), Some(&b.id)).unwrap();
        assert!(s.workspaces().unwrap().iter().all(|w| w.id != a.id));
        assert_eq!(s.pins().unwrap()[&b.id], vec!["c2", "c3", "c1"]);
        s.rename_workspace(&b.id, "งาน").unwrap();
        assert!(s.workspaces().unwrap().iter().any(|w| w.name == "งาน"));
        s.set_pinned(true, "c1", "main").unwrap();
        s.delete_workspace(&b.id).unwrap();
        let pins = s.pins().unwrap();
        assert!(!pins.contains_key(&b.id));
        assert_eq!(pins["main"], vec!["c1"]);
        assert!(s.delete_workspace("main").is_err());
    }

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    #[test]
    fn encrypts_a_plain_database_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("laab.sqlite");
        {
            let mut s = Store::open(&path, None).unwrap();
            s.upsert_chats(&[chat("u1", "Arthit")]).unwrap();
            s.upsert_messages(&[msg("1", "u1", "สวัสดีครับ", 1.0)]).unwrap();
        }
        assert!(is_plaintext(&path));
        let s = Store::open(&path, Some(KEY)).unwrap();
        assert!(!is_plaintext(&path));
        assert_eq!(s.chats().unwrap()[0].name, "Arthit");
        let hits = s.search("สวัสดี", &[SearchKind::Message]).unwrap();
        assert_eq!(hits.len(), 1);
        drop(s);
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.windows(6).any(|w| w == b"Arthit"));
    }

    #[test]
    fn a_wrong_key_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("laab.sqlite");
        Store::open(&path, Some(KEY)).unwrap();
        let other = KEY.replace('0', "f");
        let e = Store::open(&path, Some(&other)).err().unwrap();
        assert!(is_wrong_key(&e));
        assert!(is_wrong_key(&Store::open(&path, None).err().unwrap()));
    }

    #[test]
    fn session_round_trip() {
        let s = Store::memory().unwrap();
        s.session_set("a", "\"x\"").unwrap();
        s.session_set("b", "2").unwrap();
        assert_eq!(s.session_get("a").unwrap().as_deref(), Some("\"x\""));
        s.session_delete("a").unwrap();
        assert!(s.session_get("a").unwrap().is_none());
        assert_eq!(s.session_all().unwrap().len(), 1);
        s.session_clear().unwrap();
        assert!(s.session_all().unwrap().is_empty());
    }

    #[test]
    fn hidden_chats_return_with_new_messages() {
        let mut s = Store::memory().unwrap();
        let mut c = chat("u1", "Arthit");
        c.last_activity = 10.0;
        s.upsert_chats(&[c.clone()]).unwrap();
        s.hide_chat("u1").unwrap();
        assert!(s.chats().unwrap().is_empty());
        c.last_activity = 11.0;
        s.upsert_chats(&[c]).unwrap();
        assert_eq!(s.chats().unwrap().len(), 1);
    }

    #[test]
    fn drafts_round_trip() {
        let s = Store::memory().unwrap();
        let d = Draft {
            text: "@All hi".into(),
            mentions: vec![DraftMention {
                mid: "all".into(),
                all: true,
                label: "@All".into(),
            }],
        };
        s.set_draft("c1", &d).unwrap();
        assert_eq!(s.draft("c1").unwrap(), Some(d));
        s.set_draft("c1", &Draft::default()).unwrap();
        assert_eq!(s.draft("c1").unwrap(), None);
    }
}
