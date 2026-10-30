//! Types shared with the React frontend. Field names are camelCase on the wire.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    pub id: String,
    pub name: String,
    /// `dm`, `group` or `bot`.
    pub kind: String,
    pub preview: String,
    /// Milliseconds.
    pub last_activity: f64,
    pub unread: i64,
    /// Local file path of the cached avatar.
    pub avatar: Option<String>,
    pub muted: bool,
    /// LINE `picturePath` (e.g. `/0h...`), used to fetch the avatar.
    pub picture: Option<String>,
    pub member_count: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMember {
    pub mid: String,
    pub name: String,
    #[serde(default)]
    pub all: bool,
    pub picture: Option<String>,
}

/// UTF-16 offsets into the message text, as LINE's `MENTION` metadata uses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MentionSpan {
    pub start: i64,
    pub end: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mid: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reaction {
    pub user_id: String,
    /// `NICE`, `LOVE`, `FUN`, `AMAZING`, `SAD`, `OMG`, or `PAID` for paid ones.
    pub reaction: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub chat_id: String,
    pub text: String,
    pub file_name: Option<String>,
    pub file_path: Option<String>,
    pub image_path: Option<String>,
    pub content_type: String,
    /// Milliseconds.
    pub created_time: f64,
    pub mine: bool,
    pub sender_id: Option<String>,
    pub sticker_id: Option<String>,
    pub sticker_package: Option<String>,
    pub sticker_version: Option<String>,
    pub mentions: Vec<MentionSpan>,
    pub mentions_me: bool,
    pub metadata: BTreeMap<String, String>,
    pub reply_to: Option<String>,
    /// 0 = not unsent, 1 = unsent with a notice row, 2 = silently unsent.
    pub unsent: i64,
    pub edited: bool,
    pub file_size: Option<i64>,
    pub duration_ms: Option<i64>,
    pub reactions: Vec<Reaction>,
    /// The raw LINE message as JSON, for the developer inspector. Not sent
    /// with message lists.
    #[serde(skip)]
    pub raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub position: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum SearchKind {
    Chat,
    Message,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub kind: SearchKind,
    pub chat_id: String,
    pub message_id: Option<String>,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sticker {
    pub sticker_id: String,
    pub package_id: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StickerPack {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub recent: bool,
    pub stickers: Vec<Sticker>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub mid: String,
    pub display_name: String,
    pub picture: Option<String>,
    pub device: String,
}

/// Login and sync state shown in the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// `starting`, `signed_out`, `logging_in`, `signed_in`, `demo`.
    pub state: String,
    pub status: String,
    pub qr_url: Option<String>,
    pub pin: Option<String>,
    pub profile: Option<Profile>,
    pub sync_done: usize,
    pub sync_total: usize,
}
