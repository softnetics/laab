//! The tools LAAB's MCP server offers. Each one checks [`mcp::authorize`]
//! for the chats it touches before doing anything, and lands in the
//! activity list LAAB shows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use serde_json::{Map, Value, json};
use tauri::Manager;

use crate::commands;
use crate::mcp::{Access, Ctx, Group, Scope, Settings, authorize, clip};
use crate::model::{Chat, MentionSpan, Message, SearchKind, Sticker};
use crate::state::AppState;

type R<T> = Result<T, String>;

/// Photos larger than this aren't returned inline.
const MAX_IMAGE: usize = 5 * 1024 * 1024;

// ---------------------------------------------------------------- definitions

struct Def {
    name: &'static str,
    group: Group,
    title: &'static str,
    description: &'static str,
    /// Properties of the input schema, and which are required.
    props: Value,
    required: &'static [&'static str],
    read_only: bool,
    destructive: bool,
}

fn s(desc: &str) -> Value {
    json!({ "type": "string", "description": desc })
}
fn n(desc: &str) -> Value {
    json!({ "type": "integer", "description": desc })
}
fn b(desc: &str) -> Value {
    json!({ "type": "boolean", "description": desc })
}
fn strings(desc: &str) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": desc })
}

const CHAT: &str = "Chat id (`u…`, `c…` or `r…`) from list_chats or search.";
const MSG: &str = "Message id from read_messages or search.";

fn defs() -> Vec<Def> {
    use Group::*;
    let d = |name, group, title, description, props: Value, required, read_only, destructive| Def {
        name,
        group,
        title,
        description,
        props,
        required,
        read_only,
        destructive,
    };
    vec![
        // ------------------------------------------------------------ read
        d("get_status", Read, "LAAB status", "Whether LAAB is signed in to LINE, who the user is, sync progress, and what the user allows by default.", json!({}), &[], true, false),
        d("list_chats", Read, "List chats", "Chats, most recent first, with unread counts and the last message. Filter by name, kind, folder or unread.", json!({
            "query": s("Only chats whose name contains this (case-insensitive)."),
            "kind": { "type": "string", "enum": ["dm", "group", "bot"], "description": "Only this kind of chat." },
            "folder_id": s("Only chats in this folder (from list_folders)."),
            "unread_only": b("Only chats with unread messages."),
            "pinned_only": b("Only pinned chats."),
            "limit": n("At most this many (default 50, max 500)."),
            "offset": n("Skip this many first, for paging."),
        }), &[], true, false),
        d("list_folders", Read, "List folders", "LAAB's chat folders and pinned chats.", json!({}), &[], true, false),
        d("read_messages", Read, "Read messages", "Messages of a chat, oldest first. Pass before_message_id (the first id of the previous page) to page back; older history is fetched from LINE as needed. Doesn't mark the chat as read.", json!({
            "chat_id": s(CHAT),
            "limit": n("How many (default 30, max 200)."),
            "before_message_id": s("Only messages before this one."),
            "sync": b("Fetch the latest messages from LINE first (default false; LAAB keeps chats in sync while it runs)."),
        }), &["chat_id"], true, false),
        d("get_message", Read, "Get a message", "One message with the message it replies to. raw adds LINE's raw data.", json!({
            "message_id": s(MSG),
            "raw": b("Include LINE's raw message (only for the last 100 messages of a chat)."),
        }), &["message_id"], true, false),
        d("search", Read, "Search", "Search chat names, message text and file names stored in LAAB.", json!({
            "query": s("What to look for."),
            "kinds": { "type": "array", "items": { "type": "string", "enum": ["chat", "message", "file"] }, "description": "What to search (default all)." },
            "limit": n("At most this many (default 50)."),
        }), &["query"], true, false),
        d("list_members", Read, "List members", "Members of a group or room, with their mids.", json!({
            "chat_id": s(CHAT),
            "refresh": b("Reload from LINE."),
        }), &["chat_id"], true, false),
        d("get_read_status", Read, "Read status", "Who has read up to which message in a chat.", json!({ "chat_id": s(CHAT) }), &["chat_id"], true, false),
        d("list_contacts", Read, "List people", "People LAAB knows (friends and people in shared groups), with mids.", json!({
            "query": s("Only names containing this."),
            "limit": n("At most this many (default 100)."),
        }), &[], true, false),
        d("get_profile", Read, "Get a profile", "Someone's LINE profile: name, status message, whether they are a friend or blocked.", json!({ "mid": s("The person's mid (`u…`).") }), &["mid"], true, false),
        d("find_contact", Read, "Find by LINE ID", "Look someone up by LINE ID or a line.me profile link. Doesn't add them.", json!({ "query": s("LINE ID or link.") }), &["query"], true, false),
        d("list_announcements", Read, "List announcements", "Announcements pinned in a group.", json!({ "chat_id": s(CHAT) }), &["chat_id"], true, false),
        d("list_notes", Read, "List notes", "Posts on a group's notes (โน้ต).", json!({ "chat_id": s(CHAT) }), &["chat_id"], true, false),
        d("get_note", Read, "Get a note", "A note post with its comments.", json!({ "chat_id": s(CHAT), "post_id": s("Post id from list_notes.") }), &["chat_id", "post_id"], true, false),
        d("list_albums", Read, "List albums", "Photo albums of a chat.", json!({ "chat_id": s(CHAT) }), &["chat_id"], true, false),
        d("list_album_photos", Read, "List album photos", "Photos in an album. View one with view_album_photo.", json!({
            "chat_id": s(CHAT), "album_id": s("Album id from list_albums."), "limit": n("At most this many (default 100)."),
        }), &["chat_id", "album_id"], true, false),
        d("view_album_photo", Read, "View an album photo", "Returns the photo as an image.", json!({
            "chat_id": s(CHAT), "album_id": s("Album id."), "oid": s("The photo's oid from list_album_photos."),
        }), &["chat_id", "album_id", "oid"], true, false),
        d("list_shared_media", Read, "List shared media", "Photos and videos, files, or links sent in a chat, newest first.", json!({
            "chat_id": s(CHAT),
            "kind": { "type": "string", "enum": ["media", "files", "links"] },
            "limit": n("At most this many (default 50)."),
        }), &["chat_id", "kind"], true, false),
        d("view_image", Read, "View a photo", "Returns a photo message (or a video's preview) as an image you can look at.", json!({ "message_id": s(MSG) }), &["message_id"], true, false),
        d("save_attachment", Read, "Save an attachment", "Save a message's photo, video, audio or file to Downloads (or a folder you name) and return its path, e.g. to read a PDF.", json!({
            "message_id": s(MSG),
            "directory": s("Folder to save into (default Downloads). It must exist."),
        }), &["message_id"], false, false),
        d("list_stickers", Read, "List stickers", "Sticker packs the user owns, for send_sticker.", json!({ "limit": n("At most this many packs (default 20).") }), &[], true, false),
        d("translate", Read, "Translate", "Translate text with LINE's translator.", json!({
            "text": s("Text to translate."), "target": s("Language code, e.g. `en`, `th`, `ja`."),
        }), &["text", "target"], true, false),
        d("get_rich_menu", Read, "Get a bot's menu", "The rich menu of an official account chat, with each area's action.", json!({ "chat_id": s(CHAT) }), &["chat_id"], true, false),
        d("write_draft", Read, "Write a draft", "Put text in a chat's message box in LAAB for the user to check and send themselves. Nothing is sent.", json!({
            "chat_id": s(CHAT),
            "text": s("The draft. Write @Name for each mention."),
            "mention_ids": strings("Mids (or `all`) mentioned in the text as @Name."),
            "open": b("Also show the chat in LAAB (default true)."),
        }), &["chat_id", "text"], false, false),
        d("open_in_laab", Read, "Show in LAAB", "Bring LAAB forward on a chat, scrolled to a message if given.", json!({
            "chat_id": s(CHAT), "message_id": s("Message to scroll to."),
        }), &["chat_id"], false, false),
        d("refresh_chats", Read, "Sync with LINE", "Fetch chats that changed from LINE. full also rechecks every chat (slow).", json!({ "full": b("Check every chat.") }), &[], false, false),
        // ------------------------------------------------------------ send
        d("send_message", Send, "Send a message", "Send a text message as the user. To mention someone, write @Name in the text and pass their mid in mention_ids.", json!({
            "chat_id": s(CHAT),
            "text": s("The message."),
            "reply_to_message_id": s("Reply to this message."),
            "mention_ids": strings("Mids (or `all`) mentioned in the text as @Name (their name as list_members shows it)."),
        }), &["chat_id", "text"], false, false),
        d("send_sticker", Send, "Send a sticker", "Send a sticker the user owns (from list_stickers).", json!({
            "chat_id": s(CHAT), "package_id": s("Package id."), "sticker_id": s("Sticker id."), "version": s("Package version, if listed."),
        }), &["chat_id", "package_id", "sticker_id"], false, false),
        d("send_files", Send, "Send files", "Send files from this Mac. Photos and videos are sent as such, anything else as a file.", json!({
            "chat_id": s(CHAT), "paths": strings("Absolute paths of the files."),
        }), &["chat_id", "paths"], false, false),
        d("react", Send, "React", "Set or remove the user's reaction on a message.", json!({
            "message_id": s(MSG),
            "reaction": { "type": ["string", "null"], "enum": ["NICE", "LOVE", "FUN", "AMAZING", "SAD", "OMG", null], "description": "The reaction, or null to remove it." },
        }), &["message_id", "reaction"], false, false),
        d("mark_as_read", Send, "Mark as read", "Mark a chat as read; the others see it as read.", json!({ "chat_id": s(CHAT) }), &["chat_id"], false, false),
        d("tap_postback", Send, "Tap a bot button", "Answer a bot's flex or rich menu postback button.", json!({
            "chat_id": s(CHAT), "message_id": s("The message holding the button."), "data": s("The button's postback data."), "display_text": s("Text the button posts in the chat, if any."),
        }), &["chat_id", "message_id", "data"], false, false),
        d("create_note", Send, "Post a note", "Post to a group's notes, with photos from this Mac if given.", json!({
            "chat_id": s(CHAT), "text": s("The post."), "photo_paths": strings("Absolute paths of photos."),
        }), &["chat_id", "text"], false, false),
        d("comment_on_note", Send, "Comment on a note", "Add a comment to a note post.", json!({
            "chat_id": s(CHAT), "post_id": s("Post id."), "text": s("The comment."),
        }), &["chat_id", "post_id", "text"], false, false),
        // ------------------------------------------------------------ manage
        d("edit_message", Manage, "Edit a message", "Change the text of a message the user sent.", json!({ "message_id": s(MSG), "text": s("The new text.") }), &["message_id", "text"], false, false),
        d("unsend_message", Manage, "Unsend a message", "Unsend a message the user sent, for everyone.", json!({ "message_id": s(MSG) }), &["message_id"], false, true),
        d("delete_message_locally", Manage, "Delete on this Mac", "Delete a message from LAAB only; others still see it.", json!({ "message_id": s(MSG) }), &["message_id"], false, true),
        d("set_muted", Manage, "Mute a chat", "Turn a chat's notifications off or on.", json!({ "chat_id": s(CHAT), "muted": b("true to mute.") }), &["chat_id", "muted"], false, false),
        d("set_pinned", Manage, "Pin a chat", "Pin or unpin a chat at the top of the list.", json!({ "chat_id": s(CHAT), "pinned": b("true to pin.") }), &["chat_id", "pinned"], false, false),
        d("create_folder", Manage, "Create a folder", "Make a LAAB folder holding these chats.", json!({ "name": s("Folder name."), "chat_ids": strings("Chats to put in it.") }), &["name", "chat_ids"], false, false),
        d("move_to_folder", Manage, "Move to a folder", "Move a chat into a folder, out of one, or between two.", json!({
            "chat_id": s(CHAT), "folder_id": s("Folder to move into; omit to take it out."), "from_folder_id": s("Folder it leaves."),
        }), &["chat_id"], false, false),
        d("rename_folder", Manage, "Rename a folder", "Rename a LAAB folder.", json!({ "folder_id": s("Folder id."), "name": s("New name.") }), &["folder_id", "name"], false, false),
        d("delete_folder", Manage, "Delete a folder", "Delete a LAAB folder; its chats go back to the list.", json!({ "folder_id": s("Folder id.") }), &["folder_id"], false, true),
        d("hide_chat", Manage, "Hide a chat", "Hide a chat from the list until a new message arrives.", json!({ "chat_id": s(CHAT) }), &["chat_id"], false, false),
        d("announce", Manage, "Announce a message", "Pin a message as an announcement in a group.", json!({
            "chat_id": s(CHAT), "message_id": s(MSG), "text": s("Announcement text (default: the message's text)."),
        }), &["chat_id", "message_id"], false, false),
        d("remove_announcement", Manage, "Remove an announcement", "Remove a group announcement.", json!({ "chat_id": s(CHAT), "seq": n("Its seq from list_announcements.") }), &["chat_id", "seq"], false, true),
        d("delete_note", Manage, "Delete a note", "Delete a note post.", json!({ "chat_id": s(CHAT), "post_id": s("Post id.") }), &["chat_id", "post_id"], false, true),
        d("create_album", Manage, "Create an album", "Create an empty album in a chat.", json!({ "chat_id": s(CHAT), "title": s("Album name.") }), &["chat_id", "title"], false, false),
        d("rename_album", Manage, "Rename an album", "Rename an album.", json!({ "chat_id": s(CHAT), "album_id": s("Album id."), "title": s("New name.") }), &["chat_id", "album_id", "title"], false, false),
        d("delete_album", Manage, "Delete an album", "Delete an album and its photos.", json!({ "chat_id": s(CHAT), "album_id": s("Album id.") }), &["chat_id", "album_id"], false, true),
        d("save_to_keep", Manage, "Save to Keep", "Save a message to the user's Keep Memo.", json!({ "chat_id": s(CHAT), "message_id": s(MSG) }), &["chat_id", "message_id"], false, false),
        // ------------------------------------------------------------ account
        d("delete_chat_history", Account, "Delete chat history", "Delete a chat's history from the user's account. It can't be brought back.", json!({ "chat_id": s(CHAT) }), &["chat_id"], false, true),
        d("leave_group", Account, "Leave a group", "Leave a group or room.", json!({ "chat_id": s(CHAT) }), &["chat_id"], false, true),
        d("set_blocked", Account, "Block someone", "Block or unblock a person.", json!({ "mid": s("Their mid (`u…`)."), "blocked": b("true to block.") }), &["mid", "blocked"], false, true),
        d("add_friend", Account, "Add a friend", "Add someone as a friend (find them with find_contact).", json!({ "mid": s("Their mid.") }), &["mid"], false, false),
        d("join_group_by_link", Account, "Join a group", "Join a group by its invite link.", json!({ "url": s("The invite link.") }), &["url"], false, false),
    ]
}

/// Tools of every group the user allows somewhere.
pub fn list(settings: &Settings) -> Vec<Value> {
    defs()
        .into_iter()
        .filter(|d| settings.most(d.group) != Access::Off)
        .map(|d| {
            let ask = settings.most(d.group) == Access::Ask && settings.defaults.get(d.group) != Access::On;
            let description = if ask {
                format!("{} The user is asked to allow each call.", d.description)
            } else {
                d.description.to_string()
            };
            json!({
                "name": d.name,
                "title": d.title,
                "description": description,
                "inputSchema": { "type": "object", "properties": d.props, "required": d.required },
                "annotations": {
                    "title": d.title,
                    "readOnlyHint": d.read_only,
                    "destructiveHint": d.destructive,
                    "openWorldHint": true,
                },
            })
        })
        .collect()
}

// ---------------------------------------------------------------- helpers

fn arg_str(args: &Value, key: &str) -> R<String> {
    opt_str(args, key).ok_or_else(|| format!("missing `{key}`"))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    match &args[key] {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args[key].as_bool()
}

fn arg_bool(args: &Value, key: &str) -> R<bool> {
    opt_bool(args, key).ok_or_else(|| format!("missing `{key}` (true or false)"))
}

fn opt_num(args: &Value, key: &str) -> Option<i64> {
    args[key]
        .as_i64()
        .or_else(|| args[key].as_str().and_then(|s| s.parse().ok()))
}

fn limit(args: &Value, default: usize, max: usize) -> usize {
    opt_num(args, "limit")
        .map(|n| n.clamp(1, max as i64) as usize)
        .unwrap_or(default)
}

fn str_list(args: &Value, key: &str) -> Vec<String> {
    args[key]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn text(v: Value) -> Vec<Value> {
    let body = serde_json::to_string_pretty(&v).unwrap_or_default();
    vec![json!({ "type": "text", "text": body })]
}

fn done(what: &str) -> R<Vec<Value>> {
    Ok(text(json!({ "ok": true, "done": what })))
}

pub fn time(ms: f64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M:%S %:z").to_string())
        .unwrap_or_default()
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Chats, folders and names, read once per call.
struct Directory {
    chats: HashMap<String, Chat>,
    folders_of: HashMap<String, Vec<String>>,
    folder_names: HashMap<String, String>,
    pinned: Vec<String>,
    names: HashMap<String, String>,
}

impl Directory {
    fn load(state: &AppState) -> R<Self> {
        let store = state.store();
        let chats: HashMap<String, Chat> = store
            .chats()
            .map_err(err)?
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect();
        let pins = store.pins().map_err(err)?;
        let folder_names: HashMap<String, String> = store
            .workspaces()
            .map_err(err)?
            .into_iter()
            .map(|w| (w.id, w.name))
            .collect();
        let mut folders_of: HashMap<String, Vec<String>> = HashMap::new();
        for (ws, ids) in &pins {
            if ws == "main" {
                continue;
            }
            for id in ids {
                folders_of.entry(id.clone()).or_default().push(ws.clone());
            }
        }
        let mut names: HashMap<String, String> = store
            .people()
            .map_err(err)?
            .into_iter()
            .map(|(mid, (name, _))| (mid, name))
            .collect();
        for c in chats.values() {
            names.entry(c.id.clone()).or_insert_with(|| c.name.clone());
        }
        Ok(Self {
            chats,
            folders_of,
            folder_names,
            pinned: pins.get("main").cloned().unwrap_or_default(),
            names,
        })
    }

    fn kind(&self, id: &str) -> String {
        match self.chats.get(id) {
            Some(c) => c.kind.clone(),
            None if id.starts_with('u') => "dm".into(),
            None => "group".into(),
        }
    }

    fn scope(&self, id: &str) -> Scope {
        Scope::Chat {
            id: id.into(),
            kind: self.kind(id),
            folders: self.folders_of.get(id).cloned().unwrap_or_default(),
        }
    }

    fn name(&self, id: &str) -> String {
        self.chats
            .get(id)
            .map(|c| c.name.clone())
            .or_else(|| self.names.get(id).cloned())
            .unwrap_or_else(|| id.to_string())
    }

    /// “Name”, for dialogs and the activity list.
    fn quoted(&self, id: &str) -> String {
        format!("“{}”", self.name(id))
    }
}

fn kind_of(content_type: &str) -> String {
    match content_type {
        "NONE" | "" => "text".into(),
        t => t.to_ascii_lowercase(),
    }
}

/// Every `text` in a flex message, in order, for cards that come without alt text.
fn flex_text(json: &str) -> String {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(o) => {
                if o.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(t) = o.get("text").and_then(Value::as_str) {
                        let t = t.trim();
                        if !t.is_empty() && out.last().map(String::as_str) != Some(t) {
                            out.push(t.to_string());
                        }
                    }
                }
                o.values().for_each(|v| walk(v, out));
            }
            Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = vec![];
    if let Ok(v) = serde_json::from_str::<Value>(json) {
        walk(&v, &mut out);
    }
    out.join("\n")
}

/// A message for a client. Cards (flex, rich) get their alt text, or the
/// text in the card, as `text`; their layout JSON and download URLs only
/// come with `full`, since they are long and rarely useful.
fn message_json(m: &Message, dir: &Directory, me: &str, full: bool) -> Value {
    let mut o = Map::new();
    o.insert("id".into(), json!(m.id));
    o.insert("time".into(), json!(time(m.created_time)));
    let sender = m.sender_id.clone().unwrap_or_default();
    let from = if m.mine {
        "me".to_string()
    } else {
        dir.names.get(&sender).cloned().unwrap_or_else(|| sender.clone())
    };
    o.insert("from".into(), json!(from));
    if !m.mine && !sender.is_empty() {
        o.insert("fromId".into(), json!(sender));
    }
    o.insert("type".into(), json!(kind_of(&m.content_type)));
    if m.unsent > 0 {
        o.insert("unsent".into(), json!(true));
        return Value::Object(o);
    }
    let meta = |k: &str| m.metadata.get(k).filter(|v| !v.trim().is_empty());
    if !m.text.is_empty() {
        o.insert("text".into(), json!(m.text));
    } else if let Some(alt) = meta("ALT_TEXT") {
        o.insert("text".into(), json!(clip(alt, 2000)));
    } else if let Some(flex) = meta("FLEX_JSON") {
        let t = flex_text(flex);
        if !t.is_empty() {
            o.insert("text".into(), json!(clip(&t, 2000)));
        }
    }
    if let Some(f) = &m.file_name {
        o.insert("fileName".into(), json!(f));
    }
    if let Some(size) = m.file_size {
        o.insert("fileSize".into(), json!(size));
    }
    if let Some(d) = m.duration_ms {
        o.insert("durationSeconds".into(), json!(d / 1000));
    }
    if let (Some(p), Some(s)) = (&m.sticker_package, &m.sticker_id) {
        o.insert("sticker".into(), json!({ "packageId": p, "stickerId": s }));
    }
    if let Some(r) = &m.reply_to {
        o.insert("replyTo".into(), json!(r));
    }
    if m.edited {
        o.insert("edited".into(), json!(true));
    }
    let mentions_me = m.mentions.iter().any(|s| s.all || s.mid.as_deref() == Some(me));
    if mentions_me {
        o.insert("mentionsMe".into(), json!(true));
    }
    if !m.reactions.is_empty() {
        let list: Vec<Value> = m
            .reactions
            .iter()
            .map(|r| {
                let by = if r.user_id == me {
                    "me".to_string()
                } else {
                    dir.names.get(&r.user_id).cloned().unwrap_or_else(|| r.user_id.clone())
                };
                json!({ "by": by, "reaction": r.reaction })
            })
            .collect();
        o.insert("reactions".into(), Value::Array(list));
    }
    if let Some(l) = meta("LOCATION") {
        o.insert("location".into(), json!(clip(l, 1000)));
    }
    if full {
        for k in ["ALT_TEXT", "FLEX_JSON", "DOWNLOAD_URL"] {
            if let Some(v) = meta(k) {
                o.insert(format!("meta_{k}"), json!(clip(v, 4000)));
            }
        }
    }
    Value::Object(o)
}

/// The message and the scope of its chat.
fn message(state: &AppState, dir: &Directory, id: &str) -> R<(Message, Scope)> {
    let m = state
        .store()
        .message(id)
        .map_err(err)?
        .ok_or_else(|| format!("no message {id} in LAAB"))?;
    let scope = dir.scope(&m.chat_id);
    Ok((m, scope))
}

/// UTF-16 spans of `@Name` for each mentioned mid, with the `@Name`.
fn mention_spans(text: &str, ids: &[String], names: &HashMap<String, String>) -> R<Vec<(MentionSpan, String)>> {
    let mut spans = vec![];
    for id in ids {
        let all = id.eq_ignore_ascii_case("all");
        let label = if all {
            "@All".to_string()
        } else {
            format!(
                "@{}",
                names
                    .get(id)
                    .ok_or_else(|| format!("unknown mid {id}; see list_members"))?
                    .replace('\n', " ")
            )
        };
        let at = text
            .find(&label)
            .ok_or_else(|| format!("write {label} in the text to mention {id}"))?;
        let start = text[..at].encode_utf16().count() as i64;
        let end = start + label.encode_utf16().count() as i64;
        let span = MentionSpan {
            start,
            end,
            mid: (!all).then(|| id.clone()),
            all,
        };
        spans.push((span, label));
    }
    Ok(spans)
}

/// Names of everyone in a chat, plus everyone LAAB knows.
async fn names_with_members(state: &Arc<AppState>, dir: &Directory, chat: &str) -> HashMap<String, String> {
    let mut names = dir.names.clone();
    if !chat.starts_with('u') {
        let members = if state.demo {
            state.store().members(chat).unwrap_or_default()
        } else {
            crate::line::load_members(state, chat, false).await.unwrap_or_default()
        };
        for m in members {
            names.insert(m.mid, m.name);
        }
    }
    names
}

fn image(data: &[u8], path: &Path, caption: String) -> R<Vec<Value>> {
    if data.len() > MAX_IMAGE {
        return Err("the photo is too large to return; use save_attachment instead".into());
    }
    let mime = crate::media::sniff_mime(data, path);
    if !mime.starts_with("image/") {
        return Err(format!("not a photo ({mime}); use save_attachment instead"));
    }
    Ok(vec![
        json!({ "type": "image", "data": base64::engine::general_purpose::STANDARD.encode(data), "mimeType": mime }),
        json!({ "type": "text", "text": caption }),
    ])
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = p.extension().and_then(|s| s.to_str());
    (2..)
        .map(|i| match ext {
            Some(e) => dir.join(format!("{stem} ({i}).{e}")),
            None => dir.join(format!("{stem} ({i})")),
        })
        .find(|p| !p.exists())
        .unwrap_or(first)
}

fn check_files(paths: &[String]) -> R<()> {
    if paths.is_empty() {
        return Err("no paths".into());
    }
    for p in paths {
        if !Path::new(p).is_file() {
            return Err(format!("not a file: {p}"));
        }
    }
    Ok(())
}

fn file_names(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| {
            Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.clone())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------- dispatch

/// What the call did, for the activity list.
struct Trace {
    what: String,
    denied: bool,
}

pub async fn call(ctx: &Ctx, name: &str, args: &Value) -> R<Vec<Value>> {
    let Some(def) = defs().into_iter().find(|d| d.name == name) else {
        return Err(format!("unknown tool {name}"));
    };
    let mut trace = Trace {
        what: def.title.to_string(),
        denied: false,
    };
    let result = if crate::mcp::settings().most(def.group) == Access::Off {
        trace.denied = true;
        Err("ผู้ใช้ปิดเครื่องมือนี้ไว้ใน LAAB".to_string())
    } else {
        run(ctx, name, args, &mut trace).await
    };
    let outcome = match (&result, trace.denied) {
        (Ok(_), _) => "ok",
        (Err(_), true) => "denied",
        (Err(_), false) => "error",
    };
    let summary = match &result {
        Err(e) if !trace.denied => format!("{} — {}", trace.what, clip(e, 200)),
        _ => trace.what.clone(),
    };
    crate::mcp::record(&ctx.state, &ctx.client, name, &clip(&summary, 300), outcome);
    result
}

/// Ask for `group` over `scopes`, noting what is being done.
async fn gate(ctx: &Ctx, trace: &mut Trace, group: Group, scopes: &[Scope], what: String) -> R<()> {
    trace.what = what;
    authorize(ctx, group, scopes, &trace.what).await.inspect_err(|_| trace.denied = true)
}

/// For tools that don't touch one chat: allowed when anything is readable,
/// and never a dialog.
fn any_read(settings: &Settings, trace: &mut Trace, what: &str) -> R<()> {
    trace.what = what.into();
    if settings.most(Group::Read) == Access::Off {
        trace.denied = true;
        return Err("ผู้ใช้ไม่อนุญาตให้ MCP อ่านแชท (ตั้งได้ที่ LAAB เมนู ⋯ > MCP)".into());
    }
    Ok(())
}

/// Whether the client may see a chat at all.
fn readable(settings: &Settings, scope: &Scope) -> Access {
    settings.access(Group::Read, scope)
}

async fn run(ctx: &Ctx, name: &str, args: &Value, trace: &mut Trace) -> R<Vec<Value>> {
    let state = &ctx.state;
    let st = || state.app.state::<Arc<AppState>>();
    let settings = crate::mcp::settings();
    let dir = Directory::load(state)?;
    let me = state.my_mid();
    use Group::*;

    match name {
        // ------------------------------------------------------------ read
        "get_status" => {
            any_read(&settings, trace, "ดูสถานะ LAAB")?;
            let s = state.session();
            Ok(text(json!({
                "state": s.state,
                "status": s.status,
                "me": s.profile.map(|p| json!({ "mid": p.mid, "name": p.display_name })),
                "demo": state.demo,
                "sync": { "done": s.sync_done, "total": s.sync_total },
                "now": time(crate::state::now_ms()),
                "defaults": settings.defaults,
                "hasPerChatRules": !settings.rules.is_empty(),
            })))
        }
        "list_chats" => {
            any_read(&settings, trace, "ดูรายชื่อแชท")?;
            let query = opt_str(args, "query").map(|q| q.to_lowercase());
            let kind = opt_str(args, "kind");
            let folder = opt_str(args, "folder_id");
            let unread_only = opt_bool(args, "unread_only").unwrap_or(false);
            let pinned_only = opt_bool(args, "pinned_only").unwrap_or(false);
            let offset = opt_num(args, "offset").unwrap_or(0).max(0) as usize;
            let max = limit(args, 50, 500);
            let mut chats: Vec<&Chat> = dir.chats.values().collect();
            chats.sort_by(|a, b| b.last_activity.total_cmp(&a.last_activity));
            let mut out = vec![];
            let mut matched = 0;
            for c in chats {
                let access = readable(&settings, &dir.scope(&c.id));
                if access == Access::Off
                    || query.as_ref().is_some_and(|q| !c.name.to_lowercase().contains(q))
                    || kind.as_ref().is_some_and(|k| *k != c.kind)
                    || folder.as_ref().is_some_and(|f| !dir.folders_of.get(&c.id).is_some_and(|fs| fs.contains(f)))
                    || (unread_only && c.unread <= 0)
                    || (pinned_only && !dir.pinned.contains(&c.id))
                {
                    continue;
                }
                matched += 1;
                if matched <= offset || out.len() >= max {
                    continue;
                }
                let mut o = Map::new();
                o.insert("id".into(), json!(c.id));
                o.insert("name".into(), json!(c.name));
                o.insert("kind".into(), json!(c.kind));
                o.insert("lastActivity".into(), json!(time(c.last_activity)));
                if c.unread > 0 {
                    o.insert("unread".into(), json!(c.unread));
                }
                if c.muted {
                    o.insert("muted".into(), json!(true));
                }
                if let Some(n) = c.member_count {
                    o.insert("members".into(), json!(n));
                }
                if dir.pinned.contains(&c.id) {
                    o.insert("pinned".into(), json!(true));
                }
                if let Some(fs) = dir.folders_of.get(&c.id) {
                    let names: Vec<_> = fs.iter().filter_map(|f| dir.folder_names.get(f)).collect();
                    o.insert("folders".into(), json!(names));
                }
                if access == Access::On {
                    o.insert("lastMessage".into(), json!(clip(&c.preview, 200)));
                } else {
                    o.insert("needsApproval".into(), json!(true));
                }
                out.push(Value::Object(o));
            }
            Ok(text(json!({ "total": matched, "chats": out })))
        }
        "list_folders" => {
            any_read(&settings, trace, "ดูโฟลเดอร์")?;
            let visible = |id: &String| readable(&settings, &dir.scope(id)) != Access::Off;
            let pinned: Vec<Value> = dir
                .pinned
                .iter()
                .filter(|id| visible(id))
                .map(|id| json!({ "id": id, "name": dir.name(id) }))
                .collect();
            let mut folders: Vec<Value> = dir
                .folder_names
                .iter()
                .map(|(id, name)| {
                    let chats: Vec<Value> = dir
                        .folders_of
                        .iter()
                        .filter(|(chat, fs)| fs.contains(id) && visible(chat))
                        .map(|(chat, _)| json!({ "id": chat, "name": dir.name(chat) }))
                        .collect();
                    json!({ "id": id, "name": name, "chats": chats })
                })
                .collect();
            folders.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            Ok(text(json!({ "pinned": pinned, "folders": folders })))
        }
        "read_messages" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("อ่านข้อความใน {}", dir.quoted(&chat))).await?;
            let max = limit(args, 30, 200);
            let live = !state.demo && state.line().await.is_some();
            let before = match opt_str(args, "before_message_id") {
                Some(id) => Some(message(state, &dir, &id)?.0.created_time),
                None => None,
            };
            let load = || match before {
                Some(t) => state.store().messages_before(&chat, t, max),
                None => state.store().messages(&chat, max),
            };
            let mut sync_error = None;
            if live && (opt_bool(args, "sync").unwrap_or(false) || state.store().messages(&chat, 1).map(|m| m.is_empty()).unwrap_or(true)) {
                if let Err(e) = crate::line::fetch_history(state, &chat).await {
                    sync_error = Some(e.to_string());
                }
            }
            let mut list = load().map_err(err)?;
            let mut more = true;
            for _ in 0..4 {
                if list.len() >= max || !live {
                    break;
                }
                match crate::line::fetch_older(state, &chat).await {
                    Ok(0) | Err(_) => {
                        more = false;
                        break;
                    }
                    Ok(_) => list = load().map_err(err)?,
                }
            }
            let names = names_with_members(state, &dir, &chat).await;
            let dir = Directory { names, ..dir };
            let messages: Vec<Value> = list.iter().map(|m| message_json(m, &dir, &me, false)).collect();
            let mut out = json!({
                "chat": { "id": chat, "name": dir.name(&chat), "kind": dir.kind(&chat) },
                "messages": messages,
                "olderBefore": (more && list.len() >= max).then(|| list.first().map(|m| m.id.clone())).flatten(),
            });
            if let Some(e) = sync_error {
                out["syncError"] = json!(clip(&e, 300));
            }
            if list.is_empty() {
                out["note"] = json!("LAAB has no messages for this chat and LINE sent none to LAAB, even if the chat shows unread ones. Tell the user to check this chat in the LINE app.");
            }
            Ok(text(out))
        }
        "get_message" => {
            let id = arg_str(args, "message_id")?;
            let (m, scope) = message(state, &dir, &id)?;
            gate(ctx, trace, Read, &[scope], format!("อ่านข้อความใน {}", dir.quoted(&m.chat_id))).await?;
            let mut v = message_json(&m, &dir, &me, true);
            v["chatId"] = json!(m.chat_id);
            if let Some(r) = m.reply_to.as_ref().and_then(|r| state.store().message(r).ok().flatten()) {
                v["replyToMessage"] = message_json(&r, &dir, &me, false);
            }
            if opt_bool(args, "raw").unwrap_or(false) && !state.demo {
                let raw = crate::line::raw_message(state, &m.chat_id, &m.id).await.map_err(err)?;
                v["raw"] = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
            }
            Ok(text(v))
        }
        "search" => {
            let query = arg_str(args, "query")?;
            any_read(&settings, trace, &format!("ค้นหา “{}”", clip(&query, 60)))?;
            let mut kinds: Vec<SearchKind> = str_list(args, "kinds")
                .iter()
                .filter_map(|k| serde_json::from_value(json!(k)).ok())
                .collect();
            if kinds.is_empty() {
                kinds = vec![SearchKind::Chat, SearchKind::Message, SearchKind::File];
            }
            let max = limit(args, 50, 500);
            let hits = state.store().search(&query, &kinds).map_err(err)?;
            let out: Vec<Value> = hits
                .into_iter()
                .filter(|h| match readable(&settings, &dir.scope(&h.chat_id)) {
                    Access::On => true,
                    Access::Ask => h.kind == SearchKind::Chat,
                    Access::Off => false,
                })
                .take(max)
                .map(|h| {
                    let mut v = json!({ "kind": h.kind, "chatId": h.chat_id, "chatName": dir.name(&h.chat_id), "title": h.title, "detail": clip(&h.detail, 300) });
                    if let Some(id) = &h.message_id {
                        v["messageId"] = json!(id);
                        if let Ok(Some(m)) = state.store().message(id) {
                            v["time"] = json!(time(m.created_time));
                        }
                    }
                    v
                })
                .collect();
            Ok(text(json!({ "results": out })))
        }
        "list_members" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("ดูสมาชิกของ {}", dir.quoted(&chat))).await?;
            let members = commands::members(st(), chat, opt_bool(args, "refresh")).await?;
            let out: Vec<Value> = members
                .into_iter()
                .filter(|m| !m.all)
                .map(|m| json!({ "mid": m.mid, "name": m.name, "isMe": m.mid == me }))
                .collect();
            Ok(text(json!({ "members": out })))
        }
        "get_read_status" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("ดูว่าใครอ่านแล้วใน {}", dir.quoted(&chat))).await?;
            let names = names_with_members(state, &dir, &chat).await;
            let reads = state.store().reads(&chat).map_err(err)?;
            let out: Vec<Value> = reads
                .into_iter()
                .filter(|(user, _)| *user != me)
                .map(|(user, msg)| {
                    let at = state.store().message(&msg).ok().flatten().map(|m| time(m.created_time));
                    json!({ "mid": user, "name": names.get(&user).cloned().unwrap_or_else(|| user.clone()), "readUpTo": msg, "readUpToTime": at })
                })
                .collect();
            Ok(text(json!({ "readers": out })))
        }
        "list_contacts" => {
            any_read(&settings, trace, "ดูรายชื่อคน")?;
            let query = opt_str(args, "query").map(|q| q.to_lowercase());
            let max = limit(args, 100, 1000);
            let mut people: Vec<(&String, &String)> = dir
                .names
                .iter()
                .filter(|(mid, name)| {
                    mid.starts_with('u')
                        && **mid != me
                        && readable(&settings, &dir.scope(mid)) != Access::Off
                        && query.as_ref().is_none_or(|q| name.to_lowercase().contains(q))
                })
                .collect();
            people.sort_by(|a, b| a.1.cmp(b.1));
            let out: Vec<Value> = people
                .into_iter()
                .take(max)
                .map(|(mid, name)| json!({ "mid": mid, "name": name, "hasChat": dir.chats.contains_key(mid) }))
                .collect();
            Ok(text(json!({ "people": out })))
        }
        "get_profile" => {
            let mid = arg_str(args, "mid")?;
            gate(ctx, trace, Read, &[dir.scope(&mid)], format!("ดูโปรไฟล์ของ {}", dir.quoted(&mid))).await?;
            if state.demo {
                return Ok(text(json!({ "mid": mid, "name": dir.name(&mid) })));
            }
            let p = crate::actions::contact_profile(state, &mid).await.map_err(err)?;
            Ok(text(json!({ "mid": p.mid, "name": p.name, "statusMessage": p.status_message, "friend": p.friend, "blocked": p.blocked })))
        }
        "find_contact" => {
            let query = arg_str(args, "query")?;
            any_read(&settings, trace, &format!("ค้นหา LINE ID “{}”", clip(&query, 60)))?;
            Ok(text(json!(commands::find_contact(st(), query).await?)))
        }
        "list_announcements" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("ดูประกาศใน {}", dir.quoted(&chat))).await?;
            let list = commands::announcements(st(), chat).await?;
            let mut v = serde_json::to_value(list).map_err(err)?;
            for a in v.as_array_mut().into_iter().flatten() {
                if let Some(t) = a["createdTime"].as_f64() {
                    a["createdTime"] = json!(time(t));
                }
            }
            Ok(text(json!({ "announcements": v })))
        }
        "list_notes" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("ดูโน้ตใน {}", dir.quoted(&chat))).await?;
            let notes = commands::list_notes(st(), chat).await?;
            let out: Vec<Value> = notes
                .into_iter()
                .map(|p| json!({ "id": p.id, "author": p.author_name, "authorMid": p.author_mid, "time": time(p.created_time), "text": p.text, "photos": p.media_count, "likes": p.like_count, "comments": p.comment_count }))
                .collect();
            Ok(text(json!({ "notes": out })))
        }
        "get_note" => {
            let chat = arg_str(args, "chat_id")?;
            let post = arg_str(args, "post_id")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("อ่านโน้ตใน {}", dir.quoted(&chat))).await?;
            let d = commands::note_detail(st(), chat, post).await?;
            let comments: Vec<Value> = d
                .comments
                .into_iter()
                .map(|c| json!({ "id": c.id, "author": c.author_name, "authorMid": c.author_mid, "time": time(c.created_time), "text": c.text }))
                .collect();
            let p = d.note;
            Ok(text(json!({ "id": p.id, "author": p.author_name, "authorMid": p.author_mid, "time": time(p.created_time), "text": p.text, "photos": p.media_count, "likes": p.like_count, "comments": comments })))
        }
        "list_albums" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("ดูอัลบั้มใน {}", dir.quoted(&chat))).await?;
            Ok(text(json!({ "albums": commands::list_albums(st(), chat).await? })))
        }
        "list_album_photos" => {
            let chat = arg_str(args, "chat_id")?;
            let album = arg_str(args, "album_id")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("ดูรูปในอัลบั้มของ {}", dir.quoted(&chat))).await?;
            let photos = commands::album_photos(st(), chat, album, Some(limit(args, 100, 1000) as u32)).await?;
            let out: Vec<Value> = photos.into_iter().map(|p| json!({ "id": p.id, "oid": p.oid, "video": p.video })).collect();
            Ok(text(json!({ "photos": out })))
        }
        "view_album_photo" => {
            let chat = arg_str(args, "chat_id")?;
            let album = arg_str(args, "album_id")?;
            let oid = arg_str(args, "oid")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("ดูรูปในอัลบั้มของ {}", dir.quoted(&chat))).await?;
            if state.demo {
                return Err("ไม่มีรูปในโหมด demo".into());
            }
            let path = crate::actions::album_photo_file(state, &chat, &album, &oid).await.map_err(err)?;
            let data = crate::vault::read(&path).map_err(err)?;
            image(&data, &path, format!("Album photo {oid} from {}", dir.name(&chat)))
        }
        "list_shared_media" => {
            let chat = arg_str(args, "chat_id")?;
            let kind = arg_str(args, "kind")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("ดูรูป ไฟล์ และลิงก์ใน {}", dir.quoted(&chat))).await?;
            let list = state.store().chat_media(&chat, &kind).map_err(err)?;
            let out: Vec<Value> = list.iter().take(limit(args, 50, 500)).map(|m| message_json(m, &dir, &me, false)).collect();
            Ok(text(json!({ "items": out })))
        }
        "view_image" => {
            let id = arg_str(args, "message_id")?;
            let (m, scope) = message(state, &dir, &id)?;
            gate(ctx, trace, Read, &[scope], format!("ดูรูปใน {}", dir.quoted(&m.chat_id))).await?;
            let cached = [&m.image_path, &m.file_path]
                .into_iter()
                .flatten()
                .find(|p| Path::new(p).exists())
                .cloned();
            let path = match cached {
                Some(p) => PathBuf::from(p),
                None => PathBuf::from(commands::download(st(), m.chat_id.clone(), m.id.clone(), m.file_name.clone()).await?),
            };
            let data = crate::vault::read(&path).map_err(err)?;
            let from = if m.mine { "me".to_string() } else { m.sender_id.as_ref().and_then(|s| dir.names.get(s).cloned()).unwrap_or_default() };
            image(&data, &path, format!("{} from {from} in {}, {}", kind_of(&m.content_type), dir.name(&m.chat_id), time(m.created_time)))
        }
        "save_attachment" => {
            let id = arg_str(args, "message_id")?;
            let (m, scope) = message(state, &dir, &id)?;
            gate(ctx, trace, Read, &[scope], format!("บันทึกไฟล์จาก {}", dir.quoted(&m.chat_id))).await?;
            let dest = match opt_str(args, "directory") {
                Some(d) => {
                    let d = PathBuf::from(d);
                    if !d.is_dir() {
                        return Err(format!("not a folder: {}", d.display()));
                    }
                    Some(d)
                }
                None => dirs::download_dir(),
            }
            .ok_or("ไม่พบโฟลเดอร์ดาวน์โหลด")?;
            let ext = match m.content_type.as_str() {
                "VIDEO" => "mp4",
                "AUDIO" => "m4a",
                _ => "jpg",
            };
            let name = m.file_name.clone().unwrap_or_else(|| format!("LINE_{}.{ext}", m.id));
            let target = unique_path(&dest, &crate::media::safe_name(&name));
            let saved = commands::save_attachment(st(), m.chat_id.clone(), m.id.clone(), Some(target.to_string_lossy().into_owned())).await?;
            Ok(text(json!({ "path": saved })))
        }
        "list_stickers" => {
            any_read(&settings, trace, "ดูสติกเกอร์")?;
            let packs = commands::stickers(st()).await?;
            let out: Vec<Value> = packs
                .into_iter()
                .take(limit(args, 20, 200))
                .map(|p| {
                    let ids: Vec<&String> = p.stickers.iter().map(|s| &s.sticker_id).collect();
                    json!({ "packageId": p.id, "name": p.name, "version": p.version, "recent": p.recent, "stickerIds": ids })
                })
                .collect();
            Ok(text(json!({ "packs": out })))
        }
        "translate" => {
            let t = arg_str(args, "text")?;
            let target = arg_str(args, "target")?;
            any_read(&settings, trace, "แปลข้อความ")?;
            Ok(text(json!({ "translation": commands::translate(st(), t, target).await? })))
        }
        "get_rich_menu" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("ดูเมนูของ {}", dir.quoted(&chat))).await?;
            Ok(text(json!({ "menu": commands::rich_menu(st(), chat).await? })))
        }
        "write_draft" => {
            let chat = arg_str(args, "chat_id")?;
            let t = args["text"].as_str().unwrap_or_default().to_string();
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("เขียนร่างข้อความใน {}:\n\n{}", dir.quoted(&chat), clip(&t, 400))).await?;
            let ids = str_list(args, "mention_ids");
            let names = names_with_members(state, &dir, &chat).await;
            let spans = mention_spans(&t, &ids, &names)?;
            let mentions = spans
                .into_iter()
                .map(|(s, label)| crate::store::DraftMention {
                    mid: s.mid.unwrap_or_else(|| "all".into()),
                    all: s.all,
                    label,
                })
                .collect();
            state
                .store()
                .set_draft(&chat, &crate::store::Draft { text: t, mentions })
                .map_err(err)?;
            use tauri::Emitter;
            let _ = state.app.emit("draft-changed", json!({ "chatId": chat }));
            if opt_bool(args, "open").unwrap_or(true) {
                crate::show_in_main(&state.app, &chat, None);
            }
            done("the draft is in the chat's message box; the user sends it")
        }
        "open_in_laab" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Read, &[dir.scope(&chat)], format!("เปิด {} ใน LAAB", dir.quoted(&chat))).await?;
            crate::show_in_main(&state.app, &chat, opt_str(args, "message_id").as_deref());
            done("shown")
        }
        "refresh_chats" => {
            any_read(&settings, trace, "ซิงก์แชทกับ LINE")?;
            commands::refresh(st(), Some(opt_bool(args, "full").unwrap_or(false))).await?;
            done("synced")
        }
        // ------------------------------------------------------------ send
        "send_message" => {
            let chat = arg_str(args, "chat_id")?;
            let t = arg_str(args, "text")?;
            let reply = opt_str(args, "reply_to_message_id");
            let ids = str_list(args, "mention_ids");
            let names = if ids.is_empty() { dir.names.clone() } else { names_with_members(state, &dir, &chat).await };
            let mentions = mention_spans(&t, &ids, &names)?.into_iter().map(|(s, _)| s).collect();
            gate(ctx, trace, Send, &[dir.scope(&chat)], format!("ส่งข้อความถึง {}:\n\n{}", dir.quoted(&chat), t)).await?;
            trace.what = format!("ส่งข้อความถึง {}: {}", dir.quoted(&chat), clip(&t, 80));
            let m = commands::send_text(st(), chat, t, mentions, reply, None).await?;
            Ok(text(json!({ "ok": true, "messageId": m.id, "time": time(m.created_time) })))
        }
        "send_sticker" => {
            let chat = arg_str(args, "chat_id")?;
            let sticker = Sticker {
                package_id: arg_str(args, "package_id")?,
                sticker_id: arg_str(args, "sticker_id")?,
                version: opt_str(args, "version"),
            };
            gate(ctx, trace, Send, &[dir.scope(&chat)], format!("ส่งสติกเกอร์ถึง {}", dir.quoted(&chat))).await?;
            commands::send_sticker(st(), chat, sticker).await?;
            done("sticker sent")
        }
        "send_files" => {
            let chat = arg_str(args, "chat_id")?;
            let paths = str_list(args, "paths");
            check_files(&paths)?;
            gate(ctx, trace, Send, &[dir.scope(&chat)], format!("ส่งไฟล์ถึง {}:\n\n{}", dir.quoted(&chat), paths.join("\n"))).await?;
            trace.what = format!("ส่ง {} ถึง {}", file_names(&paths), dir.quoted(&chat));
            let failed = commands::send_files(st(), chat, paths.clone()).await?;
            if !failed.is_empty() {
                let errors: Vec<String> = failed.into_iter().map(|f| f.error).collect();
                return Err(format!(
                    "sent {} of {} files; failed:\n{}",
                    paths.len() - errors.len(),
                    paths.len(),
                    errors.join("\n")
                ));
            }
            done("files sent")
        }
        "react" => {
            let id = arg_str(args, "message_id")?;
            let (m, scope) = message(state, &dir, &id)?;
            let reaction = args["reaction"].as_str().filter(|r| !r.is_empty() && !r.eq_ignore_ascii_case("none")).map(str::to_uppercase);
            let what = match &reaction {
                Some(r) => format!("กด {r} ให้ข้อความใน {}: “{}”", dir.quoted(&m.chat_id), clip(&m.text, 120)),
                None => format!("เอา reaction ออกจากข้อความใน {}", dir.quoted(&m.chat_id)),
            };
            gate(ctx, trace, Send, &[scope], what).await?;
            commands::react(st(), m.chat_id, id, reaction).await?;
            done("reaction set")
        }
        "mark_as_read" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Send, &[dir.scope(&chat)], format!("ทำเครื่องหมายว่าอ่านแล้วใน {}", dir.quoted(&chat))).await?;
            commands::mark_read(st(), chat).await?;
            done("marked as read")
        }
        "tap_postback" => {
            let chat = arg_str(args, "chat_id")?;
            let id = arg_str(args, "message_id")?;
            let data = arg_str(args, "data")?;
            let display = opt_str(args, "display_text");
            gate(ctx, trace, Send, &[dir.scope(&chat)], format!("กดปุ่มของบอท {}{}", dir.quoted(&chat), display.as_ref().map(|d| format!(": “{d}”")).unwrap_or_default())).await?;
            commands::send_postback(st(), chat, id, data, display).await?;
            done("postback sent")
        }
        "create_note" => {
            let chat = arg_str(args, "chat_id")?;
            let t = args["text"].as_str().unwrap_or_default().to_string();
            let photos = str_list(args, "photo_paths");
            if !photos.is_empty() {
                check_files(&photos)?;
            }
            gate(ctx, trace, Send, &[dir.scope(&chat)], format!("โพสต์โน้ตใน {}:\n\n{}", dir.quoted(&chat), t)).await?;
            commands::create_note(st(), chat, t, Some(photos)).await?;
            done("note posted")
        }
        "comment_on_note" => {
            let chat = arg_str(args, "chat_id")?;
            let post = arg_str(args, "post_id")?;
            let t = arg_str(args, "text")?;
            gate(ctx, trace, Send, &[dir.scope(&chat)], format!("แสดงความคิดเห็นในโน้ตของ {}:\n\n{}", dir.quoted(&chat), t)).await?;
            commands::add_comment(st(), chat, post, t).await?;
            done("comment posted")
        }
        // ------------------------------------------------------------ manage
        "edit_message" => {
            let id = arg_str(args, "message_id")?;
            let t = arg_str(args, "text")?;
            let (m, scope) = message(state, &dir, &id)?;
            if !m.mine {
                return Err("only the user's own messages can be edited".into());
            }
            gate(ctx, trace, Manage, &[scope], format!("แก้ข้อความใน {}\n\nจาก: {}\nเป็น: {}", dir.quoted(&m.chat_id), clip(&m.text, 300), t)).await?;
            commands::edit_message(st(), m.chat_id, id, t, None).await?;
            done("edited")
        }
        "unsend_message" => {
            let id = arg_str(args, "message_id")?;
            let (m, scope) = message(state, &dir, &id)?;
            if !m.mine {
                return Err("only the user's own messages can be unsent".into());
            }
            gate(ctx, trace, Manage, &[scope], format!("ยกเลิกข้อความใน {}: “{}”", dir.quoted(&m.chat_id), clip(&m.text, 200))).await?;
            commands::unsend(st(), id).await?;
            done("unsent")
        }
        "delete_message_locally" => {
            let id = arg_str(args, "message_id")?;
            let (m, scope) = message(state, &dir, &id)?;
            gate(ctx, trace, Manage, &[scope], format!("ลบข้อความใน {} ออกจากเครื่องนี้: “{}”", dir.quoted(&m.chat_id), clip(&m.text, 200))).await?;
            commands::delete_message(st(), id)?;
            done("deleted on this Mac")
        }
        "set_muted" => {
            let chat = arg_str(args, "chat_id")?;
            let muted = arg_bool(args, "muted")?;
            gate(ctx, trace, Manage, &[dir.scope(&chat)], format!("{}การแจ้งเตือนของ {}", if muted { "ปิด" } else { "เปิด" }, dir.quoted(&chat))).await?;
            commands::set_muted(st(), chat, muted).await?;
            done(if muted { "muted" } else { "unmuted" })
        }
        "set_pinned" => {
            let chat = arg_str(args, "chat_id")?;
            let pinned = arg_bool(args, "pinned")?;
            gate(ctx, trace, Manage, &[dir.scope(&chat)], format!("{} {}", if pinned { "ปักหมุด" } else { "เลิกปักหมุด" }, dir.quoted(&chat))).await?;
            commands::set_pinned(st(), chat, "main".into(), pinned).await?;
            done(if pinned { "pinned" } else { "unpinned" })
        }
        "create_folder" => {
            let name = arg_str(args, "name")?;
            let chats = str_list(args, "chat_ids");
            if chats.is_empty() {
                return Err("no chat_ids".into());
            }
            let scopes: Vec<Scope> = chats.iter().map(|c| dir.scope(c)).collect();
            let list = chats.iter().map(|c| dir.quoted(c)).collect::<Vec<_>>().join(", ");
            gate(ctx, trace, Manage, &scopes, format!("สร้างโฟลเดอร์ “{name}” มี {list}")).await?;
            let w = commands::create_folder(st(), name, chats, None)?;
            Ok(text(json!({ "ok": true, "folderId": w.id })))
        }
        "move_to_folder" => {
            let chat = arg_str(args, "chat_id")?;
            let to = opt_str(args, "folder_id");
            let from = opt_str(args, "from_folder_id");
            let mut scopes = vec![dir.scope(&chat)];
            scopes.extend(to.iter().chain(from.iter()).map(|f| Scope::Folder(f.clone())));
            let target = to.as_ref().and_then(|f| dir.folder_names.get(f)).map(|n| format!("เข้าโฟลเดอร์ “{n}”")).unwrap_or_else(|| "ออกจากโฟลเดอร์".into());
            gate(ctx, trace, Manage, &scopes, format!("ย้าย {} {target}", dir.quoted(&chat))).await?;
            commands::move_to_folder(st(), chat, from, to)?;
            done("moved")
        }
        "rename_folder" => {
            let id = arg_str(args, "folder_id")?;
            let name = arg_str(args, "name")?;
            let old = dir.folder_names.get(&id).cloned().ok_or("no such folder")?;
            gate(ctx, trace, Manage, &[Scope::Folder(id.clone())], format!("เปลี่ยนชื่อโฟลเดอร์ “{old}” เป็น “{name}”")).await?;
            commands::rename_folder(st(), id, name)?;
            done("renamed")
        }
        "delete_folder" => {
            let id = arg_str(args, "folder_id")?;
            let old = dir.folder_names.get(&id).cloned().ok_or("no such folder")?;
            gate(ctx, trace, Manage, &[Scope::Folder(id.clone())], format!("ลบโฟลเดอร์ “{old}” (แชทในนั้นกลับไปที่รายการ)")).await?;
            commands::delete_folder(st(), id)?;
            done("folder deleted")
        }
        "hide_chat" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Manage, &[dir.scope(&chat)], format!("ซ่อน {}", dir.quoted(&chat))).await?;
            commands::hide_chat(st(), chat).await?;
            done("hidden")
        }
        "announce" => {
            let chat = arg_str(args, "chat_id")?;
            let id = arg_str(args, "message_id")?;
            let (m, _) = message(state, &dir, &id)?;
            let t = opt_str(args, "text").unwrap_or(m.text);
            gate(ctx, trace, Manage, &[dir.scope(&chat)], format!("ตั้งประกาศใน {}: “{}”", dir.quoted(&chat), clip(&t, 300))).await?;
            commands::announce(st(), chat, id, t).await?;
            done("announced")
        }
        "remove_announcement" => {
            let chat = arg_str(args, "chat_id")?;
            let seq = opt_num(args, "seq").ok_or("missing `seq`")?;
            gate(ctx, trace, Manage, &[dir.scope(&chat)], format!("เอาประกาศออกจาก {}", dir.quoted(&chat))).await?;
            commands::remove_announcement(st(), chat, seq).await?;
            done("announcement removed")
        }
        "delete_note" => {
            let chat = arg_str(args, "chat_id")?;
            let post = arg_str(args, "post_id")?;
            gate(ctx, trace, Manage, &[dir.scope(&chat)], format!("ลบโน้ตใน {}", dir.quoted(&chat))).await?;
            commands::delete_note(st(), chat, post).await?;
            done("note deleted")
        }
        "create_album" => {
            let chat = arg_str(args, "chat_id")?;
            let title = arg_str(args, "title")?;
            gate(ctx, trace, Manage, &[dir.scope(&chat)], format!("สร้างอัลบั้ม “{title}” ใน {}", dir.quoted(&chat))).await?;
            commands::create_album(st(), chat, title).await?;
            done("album created")
        }
        "rename_album" => {
            let chat = arg_str(args, "chat_id")?;
            let album = arg_str(args, "album_id")?;
            let title = arg_str(args, "title")?;
            gate(ctx, trace, Manage, &[dir.scope(&chat)], format!("เปลี่ยนชื่ออัลบั้มใน {} เป็น “{title}”", dir.quoted(&chat))).await?;
            commands::rename_album(st(), chat, album, title).await?;
            done("album renamed")
        }
        "delete_album" => {
            let chat = arg_str(args, "chat_id")?;
            let album = arg_str(args, "album_id")?;
            gate(ctx, trace, Manage, &[dir.scope(&chat)], format!("ลบอัลบั้มใน {}", dir.quoted(&chat))).await?;
            commands::delete_album(st(), chat, album).await?;
            done("album deleted")
        }
        "save_to_keep" => {
            let chat = arg_str(args, "chat_id")?;
            let id = arg_str(args, "message_id")?;
            gate(ctx, trace, Manage, &[dir.scope(&chat)], format!("บันทึกข้อความจาก {} ลง Keep", dir.quoted(&chat))).await?;
            commands::save_to_keep(st(), chat, id).await?;
            done("saved to Keep")
        }
        // ------------------------------------------------------------ account
        "delete_chat_history" => {
            let chat = arg_str(args, "chat_id")?;
            gate(ctx, trace, Account, &[dir.scope(&chat)], format!("ลบประวัติแชททั้งหมดของ {} (กู้คืนไม่ได้)", dir.quoted(&chat))).await?;
            commands::delete_history(st(), chat).await?;
            done("history deleted")
        }
        "leave_group" => {
            let chat = arg_str(args, "chat_id")?;
            if chat.starts_with('u') {
                return Err("not a group".into());
            }
            gate(ctx, trace, Account, &[dir.scope(&chat)], format!("ออกจากกลุ่ม {}", dir.quoted(&chat))).await?;
            commands::leave_group(st(), chat).await?;
            done("left the group")
        }
        "set_blocked" => {
            let mid = arg_str(args, "mid")?;
            let blocked = arg_bool(args, "blocked")?;
            gate(ctx, trace, Account, &[dir.scope(&mid)], format!("{} {}", if blocked { "บล็อก" } else { "เลิกบล็อก" }, dir.quoted(&mid))).await?;
            commands::set_blocked(st(), mid, blocked).await?;
            done(if blocked { "blocked" } else { "unblocked" })
        }
        "add_friend" => {
            let mid = arg_str(args, "mid")?;
            gate(ctx, trace, Account, &[dir.scope(&mid)], format!("เพิ่ม {} เป็นเพื่อน", dir.quoted(&mid))).await?;
            commands::add_friend(st(), mid).await?;
            done("friend added")
        }
        "join_group_by_link" => {
            let url = arg_str(args, "url")?;
            gate(ctx, trace, Account, &[Scope::Global], format!("เข้ากลุ่มจากลิงก์ {url}")).await?;
            let chat = commands::join_group_link(st(), url).await?;
            Ok(text(json!({ "ok": true, "chatId": chat })))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn flex_text_reads_cards() {
        let flex = r#"{"type":"bubble","body":{"type":"box","contents":[{"type":"text","text":"Withdraw"},{"type":"box","contents":[{"type":"text","text":"฿180.00"},{"type":"image","url":"x"}]}]}}"#;
        assert_eq!(flex_text(flex), "Withdraw\n฿180.00");
        assert_eq!(flex_text("not json"), "");
    }

    use super::*;

    #[test]
    fn every_tool_is_dispatched_and_named_once() {
        let names: Vec<&str> = defs().iter().map(|d| d.name).collect();
        let unique: std::collections::HashSet<&&str> = names.iter().collect();
        assert_eq!(unique.len(), names.len());
        let src = include_str!("mcp_tools.rs");
        for n in names {
            assert!(src.contains(&format!("\"{n}\" =>")), "{n} has no match arm");
        }
    }

    #[test]
    fn tools_follow_the_settings() {
        let mut s = Settings::default();
        assert!(list(&s).is_empty());
        s.enabled = true;
        let names: Vec<String> = list(&s).iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"read_messages".to_string()));
        assert!(names.contains(&"send_message".to_string()));
        assert!(!names.contains(&"leave_group".to_string()));
        let send = list(&s).into_iter().find(|t| t["name"] == "send_message").unwrap();
        assert!(send["description"].as_str().unwrap().contains("asked to allow"));
        // A chat rule can open a group the defaults keep closed.
        s.rules.push(crate::mcp::Rule {
            kind: crate::mcp::RuleKind::Chat,
            id: "c1".into(),
            levels: crate::mcp::Overrides { account: Some(Access::Ask), ..Default::default() },
        });
        assert!(list(&s).iter().any(|t| t["name"] == "leave_group"));
    }

    #[test]
    fn mentions_are_utf16_spans_of_at_names() {
        let names = HashMap::from([("u1".to_string(), "นารา".to_string())]);
        let spans = mention_spans("😀 @นารา hi @All", &["u1".into(), "all".into()], &names).unwrap();
        assert_eq!(spans[0].0.start, 3);
        assert_eq!(spans[0].0.end, 8);
        assert_eq!(spans[0].0.mid.as_deref(), Some("u1"));
        assert_eq!(spans[0].1, "@นารา");
        assert!(spans[1].0.all);
        assert!(mention_spans("hi", &["u1".into()], &names).is_err());
        assert!(mention_spans("hi @x", &["u2".into()], &names).is_err());
    }

    #[test]
    fn unique_path_counts_up() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), b"").unwrap();
        assert_eq!(unique_path(dir.path(), "a.pdf"), dir.path().join("a (2).pdf"));
        assert_eq!(unique_path(dir.path(), "b.pdf"), dir.path().join("b.pdf"));
    }
}
