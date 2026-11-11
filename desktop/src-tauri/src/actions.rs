//! Actions line-rs supports beyond messaging: unsend, edit, translate, chat
//! hide/history/pin, announcements, blocking, friends and groups by link,
//! Keep Memo, group notes and albums (ported from laab-gpui), and LINE
//! Calendar.
//!
//! Some come from line-rs's static reverse engineering of LINE 26.5 and are
//! not verified against the live server: edit, translate, server pin, and
//! creating, renaming or deleting albums. Nor is any of LINE Calendar, which
//! line-rs reaches over gRPC with field numbers from LINE for Android 26.11.

use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use line_rs::client::TalkMessage;
use line_rs::client::calendar::{
    self as cal, EventInput, EventTarget, Frequency, InvitationStatus, Reminder, new_event_id,
};
use line_rs::link::LineLink;
use line_rs::rest::translate::TranslateText;
use line_rs::service::moa::{DownloadPhotoOptions, GetAlbumsOptions, GetPhotosOptions};
use line_rs::service::relation::{AddFriendByMidOptions, AddFriendVia};
use line_rs::thrift::Value;
use line_rs::timeline::{CreatePost, ListPost, PostMedia};
use line_rs::tv;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::convert::{self, num_of, str_of};
use crate::line::{self, Line};
use crate::model::MentionSpan;
use crate::state::{AppState, now_ms};

async fn line(state: &Arc<AppState>) -> Result<Arc<Line>> {
    state.line().await.ok_or_else(|| anyhow!("not_logged_in"))
}

async fn talk_seq(l: &Line) -> Result<i64> {
    Ok(l.client.base.get_reqseq("talk").await?)
}

// -- messages ---------------------------------------------------------------

/// LINE's refusal of an edit or unsend, in Thai when it says why.
fn refused(e: line_rs::Error) -> anyhow::Error {
    use line_rs::ErrorCode as C;
    let text = match e.error_code() {
        Some(C::NotPremium) => "การแก้ไขข้อความใช้ได้เฉพาะสมาชิก LINE Premium",
        Some(C::MessageNotEditable) => "ข้อความนี้แก้ไขไม่ได้",
        Some(C::MessageNotEditableTimelimit) => "เลยเวลาที่ LINE ให้แก้ไขข้อความนี้แล้ว",
        Some(C::MessageNotDestructible) => "ยกเลิกข้อความนี้ไม่ได้แล้ว LINE ให้ยกเลิกได้ภายใน 24 ชั่วโมงหลังส่ง",
        _ => return e.into(),
    };
    anyhow!(text)
}

/// Unsend one of our messages for everyone.
pub async fn unsend(state: &Arc<AppState>, message_id: &str) -> Result<()> {
    let l = line(state).await?;
    let seq = talk_seq(&l).await?;
    l.client
        .base
        .talk()
        .call("unsendMessage", tv!({"seq": seq, "messageId": message_id}))
        .await
        .map_err(refused)?;
    line::apply_unsend(state, message_id);
    Ok(())
}

/// The message to edit, with `mentions` as its `MENTION` metadata: line-rs
/// keeps the metadata it finds, and the old offsets would be wrong for the
/// new text.
fn edit_target(raw: Option<Value>, me: &str, chat_id: &str, message_id: &str, mentions: &[MentionSpan]) -> Value {
    let mut raw = raw.unwrap_or_else(|| tv!({"id": message_id, "contentType": "NONE"}));
    if raw["from"].as_str().is_none_or(str::is_empty) {
        raw.insert("from", me);
    }
    if raw["to"].as_str().is_none_or(str::is_empty) {
        raw.insert("to", chat_id);
    }
    let mut meta: line_rs::thrift::Map = raw["contentMetadata"]
        .entries()
        .into_iter()
        .filter(|(k, _)| k != "MENTION")
        .map(|(k, v)| (k, v.clone()))
        .collect();
    if let Some(m) = convert::mention_metadata(mentions) {
        meta.insert("MENTION".into(), Value::String(m));
    }
    raw.insert("contentMetadata", Value::Struct(meta));
    raw
}

/// Replace the text of one of our messages (Premium; unverified). line-rs
/// edits the stored message: same chat type,
/// encrypted again if the original was or the chat now uses Letter Sealing.
pub async fn edit(
    state: &Arc<AppState>,
    chat_id: &str,
    message_id: &str,
    text: &str,
    mentions: &[MentionSpan],
) -> Result<()> {
    if text.trim().is_empty() {
        bail!("empty_message");
    }
    let l = line(state).await?;
    let raw = edit_target(line::stored_raw(state, message_id), &l.me, chat_id, message_id, mentions);
    TalkMessage::new(l.client.clone(), raw)
        .edit(text)
        .await
        .map_err(|e| {
            if e.error_code() == Some(line_rs::ErrorCode::NotPremium) {
                line::remember_premium(state, false);
            }
            refused(e)
        })?;
    let _ = line::fetch_history(state, chat_id).await;
    Ok(())
}

/// Translate `text` into `target` (LINE's code: `th`, `en`, `zh-TW`, …).
/// Desktop REST call; unverified.
pub async fn translate(state: &Arc<AppState>, text: &str, target: &str) -> Result<String> {
    let l = line(state).await?;
    let reply = translate_client(&l)
        .translate(&TranslateText::new(text, target), None)
        .await?;
    reply["translatedText"]
        .as_str()
        .or_else(|| reply["result"]["translatedText"].as_str())
        .map(str::to_string)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow!("translate_empty"))
}

/// What `copy.obs` needs to send a stored E2EE media message to `to`
/// again: its object, the key from its decrypted `payload`, and the size,
/// name, length and picture size LINE shows. `None` for anything else.
fn forward_request(to: &str, content_type: &str, raw: &Value, payload: &Json) -> Option<line_rs::obs::ForwardMedia> {
    use line_rs::obs::{ForwardMedia, MediaOptions, ObjType};
    let meta = &raw["contentMetadata"];
    let text = |key: &str| meta[key].as_str().filter(|v| !v.is_empty());
    let info: Json = text("MEDIA_CONTENT_INFO")
        .and_then(|i| serde_json::from_str(i).ok())
        .unwrap_or_default();
    let o_type = match content_type {
        "IMAGE" if info["animated"] == true || info["extension"] == "gif" => ObjType::Gif,
        "IMAGE" => ObjType::Image,
        "VIDEO" => ObjType::Video,
        "AUDIO" => ObjType::Audio,
        "FILE" => ObjType::File,
        _ => return None,
    };
    let mut opts = MediaOptions::new();
    if let Some(ms) = text("DURATION").and_then(|d| d.parse().ok()) {
        opts = opts.duration_ms(ms);
    }
    if let (Some(w), Some(h)) = (info["width"].as_u64(), info["height"].as_u64()) {
        opts = opts.size(w as u32, h as u32);
    }
    let file_name = payload["fileName"]
        .as_str()
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .or_else(|| info["extension"].as_str().map(|ext| format!("image.{ext}")));
    Some(ForwardMedia {
        to: to.to_string(),
        o_type,
        src_oid: text("OID")?.to_string(),
        src_message_id: raw["id"].as_str().filter(|v| !v.is_empty())?.to_string(),
        key_material: payload["keyMaterial"].as_str().filter(|k| !k.is_empty())?.to_string(),
        file_size: text("FILE_SIZE").and_then(|s| s.parse().ok()).unwrap_or(0),
        file_name,
        opts,
    })
}

/// Send a stored E2EE media message to `to` by having LINE copy its
/// encrypted object (`copy.obs`), so nothing is
/// downloaded or uploaded again.
async fn forward_by_copy(state: &AppState, l: &Line, message_id: &str, content_type: &str, to: &str) -> Result<()> {
    let raw = line::stored_raw(state, message_id).ok_or_else(|| anyhow!("no stored message"))?;
    if raw["chunks"].as_list().is_none_or(|c| c.is_empty()) {
        bail!("not an E2EE message");
    }
    let payload = l.client.base.decrypt_e2ee_data_message(&raw).await?;
    let request = forward_request(to, content_type, &raw, &payload)
        .ok_or_else(|| anyhow!("nothing to copy"))?;
    l.client.base.obs().forward_media(request).await?;
    Ok(())
}

/// The REST client translations go through, one per LINE session, so the
/// channel token it gets (LINE's translate channel, or the timeline one it
/// falls back to) lasts from one translation to the next.
fn translate_client(l: &Line) -> Arc<line_rs::rest::RestClient> {
    static CLIENT: std::sync::Mutex<Option<Arc<line_rs::rest::RestClient>>> = std::sync::Mutex::new(None);
    let mut slot = CLIENT.lock().unwrap();
    // The same session: the client wraps this very BaseClient.
    if let Some(c) = slot.as_ref().filter(|c| std::ptr::eq(&**c.client(), &*l.client.base)) {
        return c.clone();
    }
    let c = Arc::new(l.client.base.rest());
    *slot = Some(c.clone());
    c
}

/// Keep Memo is the chat with yourself; "save to Keep" sends a copy there.
/// Media is copied on LINE's side when it can be, else downloaded and sent
/// again.
pub async fn save_to_keep(state: &Arc<AppState>, chat_id: &str, message_id: &str) -> Result<()> {
    let l = line(state).await?;
    let me = l.me.clone();
    drop(l);
    let kind = state
        .store()
        .message(message_id)?
        .map(|m| m.content_type)
        .unwrap_or_default();
    if kind == "STICKER" {
        bail!("ยังบันทึกสติกเกอร์ลง Keep ไม่ได้");
    }
    send_copy(state, chat_id, message_id, &me).await
}

/// Forward a message to the chats `to`, as LINE's "แชร์" does. Returns
/// each chat it couldn't reach, with why.
pub async fn forward(state: &Arc<AppState>, chat_id: &str, message_id: &str, to: &[String]) -> Vec<(String, String)> {
    let mut failed = Vec::new();
    for target in to {
        match send_copy(state, chat_id, message_id, target).await {
            Ok(()) => {
                let _ = line::fetch_history(state, target).await;
            }
            Err(e) => failed.push((target.clone(), e.to_string())),
        }
    }
    failed
}

/// Send a copy of a stored message to `to`: text without its mentions
/// (they name people of another chat), a sticker, a contact card, or
/// media copied on LINE's side (`copy.obs`) and, when that fails, sent
/// again from the file.
async fn send_copy(state: &Arc<AppState>, chat_id: &str, message_id: &str, to: &str) -> Result<()> {
    use line_rs::client::MessageInput;
    let l = line(state).await?;
    let m = state
        .store()
        .message(message_id)?
        .ok_or_else(|| anyhow!("message_not_found"))?;
    let chat = l.chat(to);
    match m.content_type.as_str() {
        kind @ ("IMAGE" | "VIDEO" | "AUDIO" | "FILE") => {
            match forward_by_copy(state, &l, message_id, kind, to).await {
                Ok(()) => return Ok(()),
                Err(e) => tracing::info!("copy.obs failed, sending the file again: {e}"),
            }
            let path = crate::media::download_file(state, chat_id, message_id, m.file_name.clone())
                .await?;
            let data = crate::vault::read(&path)?;
            let name = m.file_name.clone().unwrap_or_else(|| "file".into());
            let duration = m.duration_ms.and_then(|d| u64::try_from(d).ok());
            let sent = match kind {
                "IMAGE" => chat.send_image(data, Some(&name)).await,
                "VIDEO" => chat.send_video(data, Some(&name), duration).await,
                "AUDIO" => chat.send_audio(data, Some(&name), duration).await,
                _ => chat.send_file(data, &name).await,
            };
            sent.map_err(|e| anyhow!("{e}"))?;
        }
        "STICKER" => {
            let input = sticker_copy(&m).ok_or_else(|| anyhow!("ข้อความนี้ไม่มีสติกเกอร์ให้ส่งต่อ"))?;
            l.client.send_message(to, input).await.map_err(refused_sticker)?;
        }
        "CONTACT" => {
            let mid = m.metadata.get("mid").filter(|v| !v.is_empty()).ok_or_else(|| anyhow!("ไม่มีรายชื่อให้ส่งต่อ"))?;
            let name = m.metadata.get("displayName").cloned().unwrap_or_default();
            chat.send_contact(mid, &name).await?;
        }
        _ if !m.text.trim().is_empty() => {
            l.client.send_message(to, MessageInput::text(m.text.clone())).await?;
        }
        _ => bail!("ข้อความนี้ไม่มีเนื้อหาให้ส่งต่อ"),
    }
    Ok(())
}

/// A sticker message to send again: package, sticker, version and the
/// option (animation, sound) recipients play it with.
fn sticker_copy(m: &crate::model::Message) -> Option<line_rs::client::MessageInput> {
    let id = m.sticker_id.as_deref().filter(|v| !v.is_empty())?;
    let package = m.sticker_package.as_deref().filter(|v| !v.is_empty())?;
    let version = m.sticker_version.as_deref().unwrap_or("1");
    let mut input = line_rs::client::MessageInput::sticker(package, id, version).e2ee(false);
    if let Some(opt) = m.metadata.get("STKOPT").filter(|v| !v.is_empty()) {
        input = input.sticker_options(opt.clone());
    }
    Some(input)
}

fn refused_sticker(e: line_rs::Error) -> anyhow::Error {
    if e.error_code() == Some(line_rs::ErrorCode::UserNotStickerOwner) {
        anyhow!("สติกเกอร์นี้ไม่ได้อยู่ในบัญชีของคุณ จึงส่งต่อไม่ได้")
    } else {
        anyhow!("{e}")
    }
}

/// Share someone's contact card with the chats `to`. Returns each chat it
/// couldn't reach, with why.
pub async fn share_contact(state: &Arc<AppState>, mid: &str, name: &str, to: &[String]) -> Vec<(String, String)> {
    let mut failed = Vec::new();
    for target in to {
        let sent = async {
            let l = line(state).await?;
            l.chat(target).send_contact(mid, name).await?;
            anyhow::Ok(())
        }
        .await;
        match sent {
            Ok(()) => {
                let _ = line::fetch_history(state, target).await;
            }
            Err(e) => failed.push((target.clone(), e.to_string())),
        }
    }
    failed
}

// -- chats ------------------------------------------------------------------

/// Pin or unpin on LINE's side too, so other devices follow (unverified).
pub async fn set_server_pin(state: &Arc<AppState>, chat_id: &str, pinned: bool) -> Result<()> {
    let l = line(state).await?;
    l.client
        .base
        .talk()
        .update_pin_state(chat_id, pinned)
        .await?;
    Ok(())
}

fn last_id(state: &AppState, chat_id: &str) -> String {
    state
        .store()
        .messages(chat_id, 1)
        .ok()
        .and_then(|mut m| m.pop())
        .map(|m| m.id)
        .unwrap_or_default()
}

/// Hide a chat from the list until a new message arrives.
pub async fn hide_chat(state: &Arc<AppState>, chat_id: &str) -> Result<()> {
    let l = line(state).await?;
    let last = last_id(state, chat_id).parse::<i64>().unwrap_or(0);
    l.chat(chat_id).set_hidden(true, last).await?;
    state.store().hide_chat(chat_id)?;
    state.chats_changed();
    Ok(())
}

/// Delete the chat's history on this account (`sendChatRemoved`).
pub async fn delete_history(state: &Arc<AppState>, chat_id: &str) -> Result<()> {
    let l = line(state).await?;
    let last = last_id(state, chat_id);
    l.chat(chat_id).delete_history(&last).await?;
    state.store().delete_history(chat_id)?;
    state.messages_changed(chat_id);
    state.chats_changed();
    Ok(())
}

pub async fn remove_announcement(state: &Arc<AppState>, chat_id: &str, seq: i64) -> Result<()> {
    let l = line(state).await?;
    l.chat(chat_id).remove_announcement(seq).await?;
    Ok(())
}

/// Pin a text message as the group's announcement, linking back to it.
pub async fn announce(
    state: &Arc<AppState>,
    chat_id: &str,
    message_id: &str,
    text: &str,
) -> Result<()> {
    let l = line(state).await?;
    l.chat(chat_id).announce(message_id, text).await?;
    Ok(())
}

// -- people -----------------------------------------------------------------

/// Block or unblock with RelationService (`blockRelation`,
/// `unblockRelation`); the legacy Talk calls if it fails.
pub async fn set_blocked(state: &Arc<AppState>, mid: &str, blocked: bool) -> Result<()> {
    let l = line(state).await?;
    let relation = l.client.base.relation();
    let result = if blocked {
        relation.block_relation(mid).await
    } else {
        relation.unblock_relation(mid, None).await
    };
    let Err(e) = result else { return Ok(()) };
    tracing::warn!("blockRelation/unblockRelation: {e}");
    let seq = talk_seq(&l).await?;
    let talk = l.client.base.talk();
    if blocked {
        talk.call("blockContact", tv!({"reqSeq": seq, "id": mid}))
            .await?;
    } else {
        talk.call(
            "unblockContact",
            tv!({"reqSeq": seq, "id": mid, "reference": ""}),
        )
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundContact {
    pub mid: String,
    pub name: String,
    pub picture: Option<String>,
    /// How they were found, for [`add_friend`].
    pub via: Option<FoundVia>,
}

/// Where the user found someone they may add as a friend. LINE wants to
/// know, and refuses an add without it (`MALFORMED_REQUEST`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FoundVia {
    /// Searched by LINE ID.
    SearchId { search_id: String },
    /// A profile link or QR code.
    Ticket { ticket: String },
    /// A member of the chat, or the sender of `message_id` in it.
    Chat { chat_id: String, message_id: Option<String> },
    /// Mentioned in a message.
    Mention { chat_id: String, message_id: String },
    /// A contact someone shared in a chat.
    SharedContact { chat_id: String, message_id: String, sender_id: String },
}

impl FoundVia {
    fn into_line(self) -> AddFriendVia {
        match self {
            Self::SearchId { search_id } => AddFriendVia::SearchId(search_id),
            Self::Ticket { ticket } => AddFriendVia::UserTicket(ticket),
            // In a 1:1 chat they are a stranger who wrote to the user.
            Self::Chat {
                chat_id,
                message_id: Some(message_id),
            } if chat_id.starts_with('u') => AddFriendVia::StrangerMessage {
                message_id,
                chat_mid: chat_id,
            },
            Self::Chat { chat_id, .. } => AddFriendVia::GroupMemberList { chat_mid: chat_id },
            Self::Mention {
                chat_id,
                message_id,
            } => AddFriendVia::MentionInChat {
                chat_mid: chat_id,
                message_id,
            },
            Self::SharedContact {
                chat_id,
                message_id,
                sender_id,
            } => AddFriendVia::ShareContact {
                message_id,
                chat_mid: chat_id,
                sender_mid: sender_id,
            },
        }
    }
}

/// A `GetContactV3Response`, or the legacy `Contact` (bare or under
/// `contact`) of the old Talk lookups.
fn found_contact(json: &Json) -> Option<FoundContact> {
    if let Some(mid) = str_of(&json["targetUserMid"]) {
        let detail = &json["targetProfileDetail"];
        return Some(FoundContact {
            name: str_of(&detail["profileName"]).unwrap_or_else(|| mid.clone()),
            picture: str_of(&detail["picturePath"]),
            mid,
            via: None,
        });
    }
    let c = if json["contact"].is_null() { json } else { &json["contact"] };
    let mid = str_of(&c["mid"])?;
    Some(FoundContact {
        name: str_of(&c["displayName"]).unwrap_or_else(|| mid.clone()),
        picture: str_of(&c["picturePath"]),
        mid,
        via: None,
    })
}

/// Find someone by LINE ID or a line.me / line:// profile link, with
/// `findContactBySearchIdOrTicketV3`.
/// The legacy Talk lookups stay as a fallback.
pub async fn find_contact(state: &Arc<AppState>, query: &str) -> Result<FoundContact> {
    use line_rs::link::ProfileRef;
    let l = line(state).await?;
    let query = query.trim();
    let relation = l.client.base.relation();
    let link = LineLink::parse(query);
    let via = match &link {
        Some(LineLink::Profile(ProfileRef::Ticket(t))) => Some(FoundVia::Ticket { ticket: t.clone() }),
        Some(LineLink::Profile(ProfileRef::UserId(id))) => Some(FoundVia::SearchId { search_id: id.clone() }),
        Some(LineLink::Profile(ProfileRef::OfficialAccount(id))) => Some(FoundVia::SearchId {
            search_id: format!("@{}", id.trim_start_matches('@')),
        }),
        Some(_) => None,
        None => Some(FoundVia::SearchId { search_id: query.to_string() }),
    };
    let found = |c: FoundContact| FoundContact { via: via.clone(), ..c };
    let v3 = match &link {
        Some(LineLink::Profile(ProfileRef::Ticket(t))) => relation.find_contact_by_ticket_v3(t).await,
        Some(LineLink::Profile(ProfileRef::UserId(id))) => {
            relation.find_contact_by_search_id_or_ticket_v3(id).await
        }
        // An official account's @id as a search id is unverified.
        Some(LineLink::Profile(ProfileRef::OfficialAccount(id))) => {
            relation
                .find_contact_by_search_id_or_ticket_v3(&format!("@{}", id.trim_start_matches('@')))
                .await
        }
        Some(_) => bail!("ลิงก์นี้ไม่ใช่ลิงก์เพิ่มเพื่อน"),
        None => relation.find_contact_by_search_id_or_ticket_v3(query).await,
    };
    match v3 {
        Ok(v) => {
            if let Some(c) = found_contact(&v.to_json()) {
                return Ok(found(c));
            }
        }
        Err(e) => tracing::warn!("findContactBySearchIdOrTicketV3: {e}"),
    }
    let raw = match link {
        Some(link @ LineLink::Profile(_)) => l.client.base.resolve_link(&link).await?,
        _ => {
            let id = query.trim_start_matches('@');
            l.client
                .base
                .talk()
                .call("findContactByUserid", tv!({"searchId": id}))
                .await?
        }
    };
    found_contact(&raw.to_json())
        .map(found)
        .ok_or_else(|| anyhow!("ไม่พบผู้ใช้"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactProfile {
    pub mid: String,
    pub name: String,
    pub picture: Option<String>,
    pub status_message: Option<String>,
    /// OBS object id of the profile cover (`myhome/c`), if LINE returned one.
    pub cover: Option<String>,
    pub friend: bool,
    pub blocked: bool,
}

/// Someone's profile card: `getContactsV3` for the name, status message
/// and friend/blocked state (the legacy
/// `getContact` if V3 fails), the home profile for the cover. The cover is
/// a best effort: without it the card falls back to the profile picture.
pub async fn contact_profile(state: &Arc<AppState>, mid: &str) -> Result<ContactProfile> {
    let l = line(state).await?;
    let mut card = match l
        .client
        .base
        .relation()
        .get_contacts_v3(&[mid.to_string()], None)
        .await
    {
        Ok(v) => profile_from_v3(&v.to_json(), mid),
        Err(e) => {
            tracing::warn!("getContactsV3 {mid}: {e}");
            None
        }
    };
    if card.is_none() {
        let c = l.client.base.talk().get_contact(mid).await?.to_json();
        card = Some(profile_from_contact(&c, mid));
    }
    let mut card = card.expect("set above");
    let home = line_rs::rest::home::HomeProfileQuery {
        home_id: mid.into(),
        ..Default::default()
    };
    card.cover = match l.client.base.rest().home().profile(home).await {
        Ok(json) => cover_object_id(&json),
        Err(e) => {
            tracing::warn!("home profile {mid}: {e}");
            None
        }
    };
    Ok(card)
}

/// A profile card from a `getContactsV3` reply.
fn profile_from_v3(json: &Json, mid: &str) -> Option<ContactProfile> {
    let r = json["responses"]
        .as_array()?
        .iter()
        .find(|r| r["targetUserMid"].as_str() == Some(mid))?;
    let detail = &r["targetProfileDetail"];
    let has = |d: &Json| d.get("user").is_some() || d.get("bot").is_some();
    // statusMessage is a RichString{content, meta}.
    let status = str_of(&detail["statusMessage"]["content"]).or_else(|| str_of(&detail["statusMessage"]));
    Some(ContactProfile {
        mid: mid.into(),
        name: str_of(&r["friendDetail"]["user"]["overriddenName"])
            .or_else(|| str_of(&detail["profileName"]))
            .unwrap_or_else(|| mid.into()),
        picture: str_of(&detail["picturePath"]),
        status_message: status.filter(|s| !s.trim().is_empty()),
        cover: None,
        friend: has(&r["friendDetail"]),
        blocked: has(&r["blockDetail"]),
    })
}

/// A profile card from the legacy `getContact`.
fn profile_from_contact(c: &Json, mid: &str) -> ContactProfile {
    // ContactStatus: FRIEND 1, FRIEND_BLOCKED 2, RECOMMEND_BLOCKED 4,
    // DELETED_BLOCKED 6. It arrives as a number or as the enum name.
    let status = c["status"]
        .as_i64()
        .or_else(|| {
            c["status"].as_str().map(|s| match s {
                "FRIEND" => 1,
                "FRIEND_BLOCKED" => 2,
                "RECOMMEND_BLOCKED" => 4,
                "DELETED_BLOCKED" => 6,
                _ => 0,
            })
        })
        .unwrap_or(0);
    ContactProfile {
        mid: mid.into(),
        name: str_of(&c["displayNameOverridden"])
            .or_else(|| str_of(&c["displayName"]))
            .unwrap_or_else(|| mid.into()),
        picture: str_of(&c["picturePath"]),
        status_message: str_of(&c["statusMessage"]).filter(|s| !s.trim().is_empty()),
        cover: None,
        friend: matches!(status, 1 | 2),
        blocked: matches!(status, 2 | 4 | 6),
    }
}

/// The cover's OBS path (see [`obs_path`]) in a home profile response.
/// The shape is not documented, so look for `coverObsInfo` (or
/// `coverObjectId`) anywhere in it.
fn cover_object_id(json: &Json) -> Option<String> {
    match json {
        Json::Object(o) => {
            if let Some(path) = o.get("coverObsInfo").and_then(|c| obs_path(c, "myhome/c")) {
                return Some(path);
            }
            if let Some(id) = o.get("coverObjectId").and_then(str_of) {
                return Some(format!("myhome/c/{id}"));
            }
            o.values().find_map(cover_object_id)
        }
        Json::Array(a) => a.iter().find_map(cover_object_id),
        _ => None,
    }
}

/// A profile cover, downloaded into the avatar cache on first use. `path`
/// is the OBS path from [`cover_object_id`]; a bare object id (from
/// earlier builds) lies in `myhome/c`.
pub async fn cover_file(state: &Arc<AppState>, path: &str) -> Result<std::path::PathBuf> {
    let dest = state
        .paths
        .avatars()
        .join(format!("cover-{}.jpg", crate::media::safe_name(path)));
    if dest.exists() {
        return Ok(dest);
    }
    let (base, oid) = match path.rsplit_once('/') {
        Some((base, oid)) => (base.to_string(), oid),
        None => ("myhome/c".to_string(), path),
    };
    crate::media::home_obs_file(state, &base, oid, &dest).await
}

/// Add `mid` as a friend, found as `via` says.
pub async fn add_friend(state: &Arc<AppState>, mid: &str, via: FoundVia) -> Result<()> {
    let l = line(state).await?;
    let options = AddFriendByMidOptions::new(mid, via.into_line());
    l.client.base.relation().add_friend_by_mid(options).await?;
    let s = state.clone();
    tokio::spawn(async move {
        let _ = line::refresh_chats(&s, false).await;
    });
    Ok(())
}

/// Join a group from its invitation link; returns the chat mid.
pub async fn join_group_link(state: &Arc<AppState>, url: &str) -> Result<String> {
    let l = line(state).await?;
    let link = LineLink::parse(url.trim()).ok_or_else(|| anyhow!("ไม่ใช่ลิงก์ของ LINE"))?;
    if !matches!(link, LineLink::GroupTicket(_)) {
        bail!("ลิงก์นี้ไม่ใช่ลิงก์เชิญเข้ากลุ่ม");
    }
    let chat = l.client.base.join_link(&link).await?.to_json();
    let mid = str_of(&chat["chatMid"])
        .or_else(|| str_of(&chat["chat"]["chatMid"]))
        .unwrap_or_default();
    let _ = line::refresh_chats(state, false).await;
    Ok(mid)
}

/// Accept a chat from LINE's message request box
/// (`updateChatStatusBulk` with `trusted`).
pub async fn accept_message_request(state: &Arc<AppState>, chat_id: &str) -> Result<()> {
    let l = line(state).await?;
    l.client
        .base
        .talk()
        .update_chat_status_bulk(&[chat_id.to_string()], true)
        .await?;
    state.store().set_request(chat_id, false)?;
    state.chats_changed();
    Ok(())
}

/// Delete a chat from LINE's message request box: its history goes
/// (`sendChatRemoved`) and it leaves the box until a new message arrives.
/// Unverified: whether LINE also drops it from the box server-side.
pub async fn delete_message_request(state: &Arc<AppState>, chat_id: &str) -> Result<()> {
    delete_history(state, chat_id).await?;
    state.store().hide_chat(chat_id)?;
    state.chats_changed();
    Ok(())
}

// -- groups -----------------------------------------------------------------

/// Reload a group's row and members after we changed it.
async fn group_changed(state: &Arc<AppState>, l: &Arc<Line>, chat_id: &str) {
    line::refresh_chat_row(state, l, chat_id).await;
    if let Err(e) = line::load_members(state, chat_id, true).await {
        tracing::warn!("reloading members: {e}");
    }
}

/// Join a group we were invited to.
pub async fn accept_invitation(state: &Arc<AppState>, chat_id: &str) -> Result<()> {
    let l = line(state).await?;
    l.chat(chat_id).accept_invitation().await?;
    {
        let mut store = state.store();
        if let Ok(Some(mut c)) = store.chat(chat_id) {
            c.invited = false;
            c.preview.clear();
            store.upsert_chats(&[c])?;
        }
    }
    state.chats_changed();
    group_changed(state, &l, chat_id).await;
    Ok(())
}

/// Decline an invitation: the group leaves the list.
pub async fn reject_invitation(state: &Arc<AppState>, chat_id: &str) -> Result<()> {
    let l = line(state).await?;
    l.chat(chat_id).reject_invitation().await?;
    state.store().hide_chat(chat_id)?;
    state.chats_changed();
    Ok(())
}

/// Create a group named `name` with these people invited; returns its id.
pub async fn create_group(state: &Arc<AppState>, name: &str, mids: &[String]) -> Result<String> {
    let l = line(state).await?;
    let mids: Vec<&str> = mids.iter().map(String::as_str).collect();
    let chat = l.client.create_group(name.trim(), &mids).await?;
    let id = chat.mid().to_string();
    if id.is_empty() {
        bail!("LINE ไม่ได้ส่งกลุ่มที่สร้างกลับมา");
    }
    let row = crate::model::Chat {
        id: id.clone(),
        name: if name.trim().is_empty() { id.clone() } else { name.trim().into() },
        kind: "group".into(),
        last_activity: now_ms(),
        ..Default::default()
    };
    state.store().upsert_chats(&[row])?;
    state.chats_changed();
    group_changed(state, &l, &id).await;
    Ok(id)
}

pub async fn invite_members(state: &Arc<AppState>, chat_id: &str, mids: &[String]) -> Result<()> {
    let l = line(state).await?;
    let mids: Vec<&str> = mids.iter().map(String::as_str).collect();
    l.chat(chat_id).invite(&mids).await?;
    group_changed(state, &l, chat_id).await;
    Ok(())
}

pub async fn remove_member(state: &Arc<AppState>, chat_id: &str, mid: &str) -> Result<()> {
    let l = line(state).await?;
    l.chat(chat_id).kick(mid).await?;
    group_changed(state, &l, chat_id).await;
    Ok(())
}

pub async fn cancel_invitation(state: &Arc<AppState>, chat_id: &str, mids: &[String]) -> Result<()> {
    let l = line(state).await?;
    let mids: Vec<&str> = mids.iter().map(String::as_str).collect();
    l.chat(chat_id).cancel_invitation(&mids).await?;
    group_changed(state, &l, chat_id).await;
    Ok(())
}

pub async fn rename_group(state: &Arc<AppState>, chat_id: &str, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("ชื่อกลุ่มว่าง");
    }
    let l = line(state).await?;
    l.chat(chat_id).update_name(name).await?;
    group_changed(state, &l, chat_id).await;
    Ok(())
}

// -- notes ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotePost {
    pub id: String,
    pub author_mid: String,
    pub author_name: String,
    /// Profile picture path (`/0h…`), for the avatar.
    pub author_picture: Option<String>,
    pub text: String,
    pub created_time: f64,
    pub media_count: usize,
    pub like_count: i64,
    pub comment_count: i64,
    /// Photo and video object ids, downloaded through `media://…/note/h/<id>`.
    pub media: Vec<NoteMedia>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NoteMedia {
    pub object_id: String,
    /// Its OBS path, `service/namespace/objectId` (see [`obs_path`]).
    pub obs: String,
    pub video: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NoteComment {
    pub id: String,
    pub author_mid: String,
    pub author_name: String,
    pub author_picture: Option<String>,
    pub text: String,
    pub created_time: f64,
    /// OBS paths of its images (see [`obs_path`]; `myhome/cmt/...` unless
    /// the comment names another place).
    pub images: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDetail {
    pub note: NotePost,
    pub comments: Vec<NoteComment>,
}

/// `https://profile.line-scdn.net/0h…` → `/0h…`, the form avatars use.
fn picture_path(url: &Json) -> Option<String> {
    let u = url.as_str()?;
    let path = u.strip_prefix("https://profile.line-scdn.net").unwrap_or(u);
    (!path.is_empty()).then(|| path.to_string())
}

/// The OBS path, `service/namespace/objectId`, of a media object in post
/// or profile data. LINE describes one as `{objectId, serviceName,
/// obsNamespace, ...}`; without the two names it lies under
/// `default` (`myhome/h` for note photos, `myhome/cmt` for comment images,
/// `myhome/c` for covers).
pub fn obs_path(o: &Json, default: &str) -> Option<String> {
    let oid = str_of(&o["objectId"]).or_else(|| str_of(&o["oid"]))?;
    let name = |k: &str| {
        o[k].as_str()
            .filter(|s| !s.is_empty() && crate::media::safe_name(s) == *s)
    };
    Some(match (name("serviceName"), name("obsNamespace")) {
        (Some(svc), Some(ns)) => format!("{svc}/{ns}/{oid}"),
        _ => format!("{default}/{oid}"),
    })
}

fn parse_post(post: &Json) -> Option<NotePost> {
    let info = &post["postInfo"];
    let user = &post["userInfo"];
    let media: Vec<NoteMedia> = post["contents"]["media"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(NoteMedia {
                object_id: str_of(&m["objectId"]).or_else(|| str_of(&m["oid"]))?,
                obs: obs_path(m, "myhome/h")?,
                video: m["type"]
                    .as_str()
                    .is_some_and(|t| t.eq_ignore_ascii_case("VIDEO")),
            })
        })
        .collect();
    Some(NotePost {
        id: str_of(&info["postId"])?,
        author_mid: str_of(&user["writerMid"])
            .or_else(|| str_of(&user["mid"]))
            .unwrap_or_default(),
        author_name: user["nickname"].as_str().unwrap_or_default().to_string(),
        author_picture: picture_path(&user["pictureUrl"]),
        text: post["contents"]["text"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string(),
        created_time: num_of(&info["createdTime"]).unwrap_or(0.0),
        media_count: media.len(),
        like_count: num_of(&info["likeCount"]).unwrap_or(0.0) as i64,
        comment_count: num_of(&info["commentCount"]).unwrap_or(0.0) as i64,
        media,
    })
}

/// `result.commentList` of `comment/getList.json`. LINE's comment shape is
/// not documented; read the usual field names and any image object ids.
pub fn parse_comments(result: &Json) -> Vec<NoteComment> {
    fn object_ids(v: &Json, out: &mut Vec<String>) {
        match v {
            Json::Object(o) => {
                if o.contains_key("objectId")
                    && let Some(path) = obs_path(v, "myhome/cmt")
                {
                    out.push(path);
                }
                o.values().for_each(|x| object_ids(x, out));
            }
            Json::Array(a) => a.iter().for_each(|x| object_ids(x, out)),
            _ => {}
        }
    }
    result["commentList"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let user = if c["userInfo"].is_object() {
                &c["userInfo"]
            } else {
                &c["user"]
            };
            let mut images = Vec::new();
            object_ids(&c["contentsList"], &mut images);
            Some(NoteComment {
                id: str_of(&c["commentId"]).or_else(|| str_of(&c["id"]))?,
                author_mid: str_of(&user["mid"])
                    .or_else(|| str_of(&user["writerMid"]))
                    .unwrap_or_default(),
                author_name: user["nickname"].as_str().unwrap_or_default().to_string(),
                author_picture: picture_path(&user["pictureUrl"]),
                text: str_of(&c["commentText"])
                    .or_else(|| str_of(&c["text"]))
                    .unwrap_or_default(),
                created_time: num_of(&c["createdTime"]).unwrap_or(0.0),
                images,
            })
        })
        .collect()
}

/// `result.feeds[].post` of `post/list.json`.
pub fn parse_notes(result: &Json) -> Vec<NotePost> {
    result["feeds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|feed| parse_post(&feed["post"]))
        .collect()
}

fn check_timeline(code: i64, message: &str) -> Result<()> {
    if code != 0 {
        bail!("timeline {code}: {message}");
    }
    Ok(())
}

/// Notes per `post/list.json` page.
const NOTE_PAGE: u32 = 20;

/// Every note of a group, newest first: page after page (`postLimit`, then
/// the last post's id and time).
/// Stops after 25 pages.
pub async fn list_notes(state: &Arc<AppState>, chat_id: &str) -> Result<Vec<NotePost>> {
    let l = line(state).await?;
    let timeline = l.client.base.timeline();
    let mut query = ListPost {
        home_id: chat_id.into(),
        post_limit: Some(NOTE_PAGE),
        ..Default::default()
    };
    let mut out: Vec<NotePost> = Vec::new();
    for _ in 0..25 {
        let reply = timeline.list_post(query.clone()).await?;
        check_timeline(reply.code, &reply.message)?;
        let page = parse_notes(&reply.result);
        let fresh: Vec<NotePost> = page
            .into_iter()
            .filter(|n| !out.iter().any(|o| o.id == n.id))
            .collect();
        if fresh.is_empty() {
            break;
        }
        out.extend(fresh);
        match line_rs::timeline::next_page(&reply.result, &query) {
            Some(next) => query = next,
            None => break,
        }
    }
    Ok(out)
}

/// Width and height of a PNG, GIF or JPEG, from its header.
pub fn image_size(data: &[u8]) -> Option<(u32, u32)> {
    let be16 = |i: usize| Some(u16::from_be_bytes(data.get(i..i + 2)?.try_into().ok()?) as u32);
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        let w = u32::from_be_bytes(data.get(16..20)?.try_into().ok()?);
        let h = u32::from_be_bytes(data.get(20..24)?.try_into().ok()?);
        return Some((w, h));
    }
    if data.starts_with(b"GIF8") {
        let le16 = |i: usize| Some(u16::from_le_bytes(data.get(i..i + 2)?.try_into().ok()?) as u32);
        return Some((le16(6)?, le16(8)?));
    }
    if !data.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        // Start of frame (not DHT 0xC4, JPG 0xC8 or DAC 0xCC).
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            return Some((be16(i + 7)?, be16(i + 5)?));
        }
        i += 2 + be16(i + 2)? as usize;
    }
    None
}

/// Post a note with text and photos (uploaded to `myhome/h` first).
pub async fn create_note(
    state: &Arc<AppState>,
    chat_id: &str,
    text: &str,
    photos: &[String],
) -> Result<()> {
    if text.trim().is_empty() && photos.is_empty() {
        bail!("empty_note");
    }
    let l = line(state).await?;
    let timeline = l.client.base.timeline();
    let mut post = CreatePost::new(chat_id);
    if !text.trim().is_empty() {
        post.contents.text = Some(text.to_string());
    }
    for path in photos {
        let data = tokio::fs::read(path).await?;
        let video = matches!(
            std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("mp4" | "mov" | "m4v")
        );
        // Also send where the object is and its size.
        let mut media = PostMedia::note(String::new(), video);
        media.size = Some(data.len() as u64);
        if let Some((w, h)) = image_size(&data).filter(|_| !video) {
            media.width = Some(w);
            media.height = Some(h);
        }
        let (object_id, _) = timeline.upload_note_media(video, data).await?;
        media.object_id = object_id;
        post.contents.media.push(media);
    }
    let reply = timeline.create_post(post).await?;
    check_timeline(reply.code, &reply.message)
}

/// One note with its comments.
pub async fn note_detail(
    state: &Arc<AppState>,
    chat_id: &str,
    post_id: &str,
) -> Result<NoteDetail> {
    let l = line(state).await?;
    let timeline = l.client.base.timeline();
    let reply = timeline.get_post(chat_id, post_id).await?;
    check_timeline(reply.code, &reply.message)?;
    let note = parse_post(&reply.result["feed"]["post"]).ok_or_else(|| anyhow!("ไม่พบโน้ตนี้"))?;
    // The post's author goes as actorId.
    // LAAB used to send the group, which the note server was seen to
    // answer; fall back to it if the author gets nothing.
    let mut comments = Vec::new();
    let mut actors = vec![note.author_mid.as_str(), chat_id];
    actors.dedup();
    for actor in actors.into_iter().filter(|a| !a.is_empty()) {
        match timeline.list_comments(chat_id, post_id, actor, None).await {
            Ok(r) if r.code == 0 => {
                comments = parse_comments(&r.result);
                if !comments.is_empty() || note.comment_count == 0 {
                    break;
                }
            }
            Ok(r) => tracing::warn!("note comments {post_id}: code {}", r.code),
            Err(e) => tracing::warn!("note comments {post_id}: {e}"),
        }
    }
    Ok(NoteDetail { note, comments })
}

/// Comment on a note. `actorId` is the post's author; it is looked up
/// when not given.
pub async fn add_comment(
    state: &Arc<AppState>,
    chat_id: &str,
    post_id: &str,
    text: &str,
    author_mid: Option<&str>,
) -> Result<()> {
    if text.trim().is_empty() {
        bail!("empty_comment");
    }
    let l = line(state).await?;
    let timeline = l.client.base.timeline();
    let author = match author_mid.filter(|m| !m.is_empty()) {
        Some(m) => m.to_string(),
        None => {
            let reply = timeline.get_post(chat_id, post_id).await?;
            check_timeline(reply.code, &reply.message)?;
            parse_post(&reply.result["feed"]["post"])
                .map(|p| p.author_mid)
                .filter(|m| !m.is_empty())
                .ok_or_else(|| anyhow!("ไม่พบโน้ตนี้"))?
        }
    };
    let reply = timeline
        .create_comment(post_id, &author, text, chat_id, None, Vec::new())
        .await?;
    check_timeline(reply.code, &reply.message)
}

pub async fn delete_note(state: &Arc<AppState>, chat_id: &str, post_id: &str) -> Result<()> {
    let l = line(state).await?;
    let reply = l
        .client
        .base
        .timeline()
        .delete_post(chat_id, post_id)
        .await?;
    check_timeline(reply.code, &reply.message)
}

// -- calendar ---------------------------------------------------------------

/// An event on a chat's LINE Calendar. Times are epoch milliseconds.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    /// For a recurring event, its first occurrence.
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    /// IANA zone the event was made in; all-day dates and repeats read in it.
    pub timezone: String,
    pub location: String,
    pub url: String,
    pub description: String,
    pub creator: String,
    /// LINE's `client_updated_at`; an edit or delete must send a later one.
    pub updated_at: i64,
    /// False for events LINE Calendar won't let anyone edit (made elsewhere).
    pub editable: bool,
    /// Whose calendar it is: a chat mid, or `cal:` and a calendar id for a
    /// calendar that isn't a chat's (the user's own). Edits go back there.
    pub chat_id: String,
    /// How it repeats; None for a one-off.
    pub recurrence: Option<CalendarRecurrence>,
    /// Occurrences changed or deleted on their own.
    pub overrides: Vec<CalendarOverride>,
    /// LINE's `Reminder` values (1 one hour before … 6 noon two days before).
    pub reminders: Vec<i32>,
    pub invitees: Vec<CalendarInvitee>,
    pub color_id: i32,
}

/// A recurrence rule. `until` and `except_ids` are epoch milliseconds.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarRecurrence {
    /// `daily`, `weekly`, `monthly`, `yearly`, or `other` (not shown as repeating).
    pub frequency: &'static str,
    pub interval: u32,
    pub until: Option<i64>,
    pub by_day: Vec<String>,
    pub except_ids: Vec<i64>,
}

/// One occurrence of a recurring event that was changed on its own.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarOverride {
    /// The original start of the occurrence it replaces, epoch ms.
    pub recur_id: i64,
    pub deleted: bool,
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub timezone: String,
    pub location: String,
    pub url: String,
    pub description: String,
    /// Who is invited to this occurrence and their answers to it.
    pub invitees: Vec<CalendarInvitee>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarInvitee {
    pub mid: String,
    /// `needsAction`, `accepted`, `declined` or empty.
    pub status: &'static str,
    /// The invitee is the logged-in user.
    pub me: bool,
    pub updated_at: i64,
}

/// One of the calendars in the all-calendars view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarInfo {
    /// What [`CalendarEvent::chat_id`] holds for its events.
    pub chat_id: String,
    pub name: String,
}

/// Every calendar the user can see, and all their events.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AllCalendars {
    pub calendars: Vec<CalendarInfo>,
    pub events: Vec<CalendarEvent>,
}

/// LINE Calendar sends and takes event times in epoch seconds; the app uses
/// milliseconds, as everywhere else.
const MS_PER_S: i64 = 1000;

/// Where a write for `chat_id` (see [`CalendarEvent::chat_id`]) goes.
fn calendar_target(chat_id: &str) -> EventTarget<'_> {
    match chat_id.strip_prefix("cal:") {
        Some(id) => EventTarget::Calendar(id),
        None => EventTarget::Chat(chat_id),
    }
}

fn status_name(s: InvitationStatus) -> &'static str {
    match s {
        InvitationStatus::NeedsAction => "needsAction",
        InvitationStatus::Accepted => "accepted",
        InvitationStatus::Declined => "declined",
        _ => "",
    }
}

fn invitees(list: Vec<cal::Invitee>, me: &str) -> Vec<CalendarInvitee> {
    list.into_iter()
        .map(|i| CalendarInvitee {
            me: !me.is_empty() && i.invitee_id == me,
            mid: i.invitee_id,
            status: status_name(i.status),
            updated_at: i.client_updated_at,
        })
        .collect()
}

fn calendar_event(e: cal::Event, chat_id: &str, me: &str) -> CalendarEvent {
    let recurrence = e.recurrence.map(|r| CalendarRecurrence {
        frequency: match r.frequency {
            Frequency::Daily => "daily",
            Frequency::Weekly => "weekly",
            Frequency::Monthly => "monthly",
            Frequency::Yearly => "yearly",
            _ => "other",
        },
        interval: r.interval,
        until: r.until.map(|u| u * MS_PER_S),
        by_day: r.by_day,
        except_ids: r.except_ids.iter().map(|x| x * MS_PER_S).collect(),
    });
    let overrides = e
        .recurrence_child_events
        .into_iter()
        .map(|c| CalendarOverride {
            recur_id: c.recur_id * MS_PER_S,
            deleted: c.is_deleted,
            title: c.title,
            start: c.start_time * MS_PER_S,
            end: c.end_time * MS_PER_S,
            all_day: c.is_all_day,
            timezone: c.timezone,
            location: c.location,
            url: c.url,
            description: c.description,
            invitees: invitees(c.invitees, me),
        })
        .collect();
    CalendarEvent {
        id: e.id,
        title: e.title,
        start: e.start_time * MS_PER_S,
        end: e.end_time * MS_PER_S,
        all_day: e.is_all_day,
        timezone: e.timezone,
        location: e.location,
        url: e.url,
        description: e.description,
        creator: e.creator_id,
        updated_at: e.client_updated_at,
        editable: e.is_editable_in_line_calendar,
        chat_id: chat_id.to_string(),
        recurrence,
        overrides,
        reminders: e.reminders.iter().map(|r| r.raw()).collect(),
        invitees: invitees(e.invitees, me),
        color_id: e.color_id,
    }
}

/// What the event form sends for a new or edited event.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventInput {
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub timezone: String,
    pub location: String,
    pub url: String,
    pub description: String,
    /// LINE `Reminder` values; None leaves an edited event's as they were.
    #[serde(default)]
    pub reminders: Option<Vec<i32>>,
    /// How it repeats; None leaves an edited event's rule as it was.
    #[serde(default)]
    pub repeat: Option<RepeatInput>,
}

/// The repeat rule the form sets, as LINE's calendar offers them: daily,
/// weekly on chosen days, monthly on the date or on an nth weekday
/// ("2TU", "-1FR"), yearly; every `interval`; until a day.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepeatInput {
    /// `none`, `daily`, `weekly`, `monthly` or `yearly`.
    pub frequency: String,
    #[serde(default)]
    pub interval: u32,
    /// Epoch ms; the chosen last day at 23:59:59.
    pub until: Option<i64>,
    #[serde(default)]
    pub by_day: Vec<String>,
}

impl RepeatInput {
    /// The rule to send, None for no repeat. The rule's exceptions stay
    /// as `previous` had them.
    fn to_line(&self, previous: Option<&cal::Recurrence>) -> Option<cal::Recurrence> {
        let frequency = match self.frequency.as_str() {
            "daily" => Frequency::Daily,
            "weekly" => Frequency::Weekly,
            "monthly" => Frequency::Monthly,
            "yearly" => Frequency::Yearly,
            _ => return None,
        };
        Some(cal::Recurrence {
            frequency,
            interval: self.interval.max(1),
            until: self.until.map(|u| u / MS_PER_S),
            by_day: if matches!(frequency, Frequency::Weekly | Frequency::Monthly) {
                self.by_day.clone()
            } else {
                Vec::new()
            },
            except_ids: previous.map(|p| p.except_ids.clone()).unwrap_or_default(),
        })
    }
}

impl CalendarEventInput {
    /// The event to send: the form's fields over `base`, the event as LINE
    /// last sent it (for an edit), so what the form doesn't show (repeats,
    /// invitees, colour, stamp, the message it came from) goes back as it
    /// was.
    fn to_line(&self, base: Option<&cal::Event>) -> EventInput {
        let mut out = base.map(EventInput::from_event).unwrap_or_default();
        out.title = self.title.trim().to_string();
        out.start_time = self.start / MS_PER_S;
        out.end_time = self.end / MS_PER_S;
        out.timezone = self.timezone.clone();
        out.is_all_day = self.all_day;
        out.location = self.location.trim().to_string();
        out.url = self.url.trim().to_string();
        out.description = self.description.trim_end().to_string();
        if let Some(r) = &self.reminders {
            out.reminders = r.iter().map(|v| Reminder::from_raw(*v)).collect();
        }
        if let Some(r) = &self.repeat {
            out.recurrence = r.to_line(out.recurrence.as_ref());
        }
        out
    }
}

/// The events of one chat's calendar, soonest first.
pub async fn calendar_events(state: &Arc<AppState>, chat_id: &str) -> Result<Vec<CalendarEvent>> {
    let l = line(state).await?;
    let events = l
        .client
        .base
        .list_events_stream(calendar_target(chat_id), 0)
        .await?;
    let mut out: Vec<CalendarEvent> = events
        .into_iter()
        .filter(|e| !e.is_deleted)
        .map(|e| calendar_event(e, chat_id, &l.me))
        .collect();
    out.sort_by_key(|e| (e.start, e.end));
    remember_calendar(
        state,
        chat_id,
        AllCalendars {
            calendars: Vec::new(),
            events: out.clone(),
        },
    );
    Ok(out)
}

/// The all-calendars window's key in [`AppState::calendars`]; the frontend's
/// `ALL_CALENDARS`.
pub const ALL_CALENDARS: &str = "all";

/// Set once the user has opened a calendar. LINE approves the Calendar
/// channel for the account on the first call, so nothing is loaded ahead of
/// time until the user has used it themselves.
const CALENDAR_USED: &str = "calendar_used";

/// How old a chat's cached calendar can get before opening the chat reloads it.
const CALENDAR_FRESH_MS: f64 = 120_000.0;

fn remember_calendar(state: &Arc<AppState>, key: &str, value: AllCalendars) {
    state
        .calendars
        .lock()
        .unwrap()
        .insert(key.to_string(), (now_ms(), value));
}

/// The events last loaded for a calendar window, if any.
pub fn cached_calendar(state: &Arc<AppState>, key: &str) -> Option<AllCalendars> {
    state.calendars.lock().unwrap().get(key).map(|(_, v)| v.clone())
}

pub fn mark_calendar_used(state: &Arc<AppState>) {
    let _ = state.store().set_kv(CALENDAR_USED, "1");
}

/// Loads calendars ahead of time for someone who uses them: every one when
/// `chat_id` is None (all-calendars fills each chat's cache too), else that
/// chat's when its copy is stale. Failures are left for the window to show.
pub async fn prefetch_calendars(state: &Arc<AppState>, chat_id: Option<&str>) {
    if state.demo || state.store().get_kv(CALENDAR_USED).ok().flatten().is_none() {
        return;
    }
    match chat_id {
        None => {
            let _ = all_calendar_events(state).await;
        }
        Some(id) => {
            let fresh = state
                .calendars
                .lock()
                .unwrap()
                .get(id)
                .is_some_and(|(at, _)| now_ms() - at < CALENDAR_FRESH_MS);
            if !fresh {
                let _ = calendar_events(state, id).await;
            }
        }
    }
}

/// Every calendar's events at once, as LINE's Calendar tab shows them. A
/// calendar that fails to load is left out rather than failing the rest.
pub async fn all_calendar_events(state: &Arc<AppState>) -> Result<AllCalendars> {
    let l = line(state).await?;
    let calendars: Vec<CalendarInfo> = shown_calendars(l.client.base.list_all_calendars().await?)
        .into_iter()
        .map(|c| CalendarInfo {
            chat_id: if c.chatroom_id.is_empty() {
                format!("cal:{}", c.id)
            } else {
                c.chatroom_id
            },
            name: c.name,
        })
        .collect();
    let mut tasks = tokio::task::JoinSet::new();
    for c in &calendars {
        let (state, chat_id) = (state.clone(), c.chat_id.clone());
        tasks.spawn(async move { calendar_events(&state, &chat_id).await });
    }
    let mut events = Vec::new();
    let mut failed = None;
    while let Some(done) = tasks.join_next().await {
        match done {
            Ok(Ok(list)) => events.extend(list),
            Ok(Err(e)) => failed = Some(e),
            Err(e) => failed = Some(e.into()),
        }
    }
    // Only when nothing loaded is that an error worth showing.
    if events.is_empty() {
        if let Some(e) = failed {
            return Err(e);
        }
    }
    events.sort_by_key(|e| (e.start, e.end));
    let all = AllCalendars { calendars, events };
    remember_calendar(state, ALL_CALENDARS, all.clone());
    Ok(all)
}

/// The calendars LINE's Calendar tab shows: not hidden from the list
/// (`is_shown_in_calendar_list`) and with their events turned on
/// (`is_visible`). Should LINE leave both flags out of every calendar (an
/// account that never touched them), all are shown rather than none.
fn shown_calendars(all: Vec<cal::Calendar>) -> Vec<cal::Calendar> {
    if all.iter().all(|c| !c.is_visible && !c.is_shown_in_calendar_list) {
        return all;
    }
    all.into_iter()
        .filter(|c| c.is_visible && c.is_shown_in_calendar_list)
        .collect()
}

/// Chats whose calendar is known to exist this session.
static CHAT_CALENDARS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Makes sure a chat has a calendar before the first write to it, as LINE
/// does (GetChatCalendar, then CreateChatCalendar). The user's own
/// calendars (`cal:`) always exist.
async fn ensure_calendar(l: &Line, chat_id: &str) -> Result<()> {
    if chat_id.starts_with("cal:") || CHAT_CALENDARS.lock().unwrap().iter().any(|c| c == chat_id) {
        return Ok(());
    }
    l.client.base.ensure_chat_calendar(chat_id).await?;
    CHAT_CALENDARS.lock().unwrap().push(chat_id.to_string());
    Ok(())
}

/// The event as LINE has it now, for an edit to start from.
async fn fetch_event(l: &Line, chat_id: &str, event_id: &str) -> Result<Option<cal::Event>> {
    Ok(l
        .client
        .base
        .list_events_stream(calendar_target(chat_id), 0)
        .await?
        .into_iter()
        .find(|e| e.id == event_id && !e.is_deleted))
}

/// Adds an event to a chat's calendar. `notify` posts LINE's "event
/// created" card in the chat, as the app's switch does.
pub async fn create_calendar_event(
    state: &Arc<AppState>,
    chat_id: &str,
    event: &CalendarEventInput,
    notify: bool,
) -> Result<()> {
    let l = line(state).await?;
    ensure_calendar(&l, chat_id).await?;
    let id = new_event_id();
    l.client
        .base
        .create_event(
            calendar_target(chat_id),
            &id,
            &event.to_line(None),
            Some(notify),
        )
        .await?;
    Ok(())
}

pub async fn update_calendar_event(
    state: &Arc<AppState>,
    chat_id: &str,
    event_id: &str,
    updated_at: i64,
    event: &CalendarEventInput,
    notify: bool,
) -> Result<()> {
    let l = line(state).await?;
    // LINE replaces the whole event, so start from what it has now.
    let current = fetch_event(&l, chat_id, event_id).await?;
    let previous = current.as_ref().map_or(updated_at, |e| e.client_updated_at.max(updated_at));
    l.client
        .base
        .update_event(
            calendar_target(chat_id),
            event_id,
            &event.to_line(current.as_ref()),
            previous,
            Some(notify),
        )
        .await?;
    Ok(())
}

/// Accept or decline an invitation to an event (or to one occurrence of a
/// recurring one, `recur_id` in epoch ms). Only chat calendars have
/// invitations.
pub async fn respond_calendar_invitation(
    state: &Arc<AppState>,
    chat_id: &str,
    event_id: &str,
    recur_id: Option<i64>,
    accept: bool,
    updated_at: i64,
) -> Result<()> {
    if chat_id.starts_with("cal:") {
        bail!("ปฏิทินนี้ไม่มีการเชิญ");
    }
    let l = line(state).await?;
    let status = if accept {
        InvitationStatus::Accepted
    } else {
        InvitationStatus::Declined
    };
    l.client
        .base
        .respond_to_invitation(chat_id, event_id, recur_id.map(|r| r / MS_PER_S), status, updated_at)
        .await?;
    Ok(())
}

pub async fn delete_calendar_event(
    state: &Arc<AppState>,
    chat_id: &str,
    event_id: &str,
    updated_at: i64,
    notify: bool,
) -> Result<()> {
    let l = line(state).await?;
    l.client
        .base
        .delete_event(calendar_target(chat_id), event_id, updated_at, Some(notify))
        .await?;
    Ok(())
}

// -- albums -----------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub title: String,
    pub photo_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumPhoto {
    pub id: String,
    pub oid: String,
    /// Where the photo is cached once shown (for "open with another app").
    pub path: String,
    pub video: bool,
}

/// Albums of one chat. LINE lists every album, page by page, so read
/// every page and filter by chat.
pub async fn list_albums(state: &Arc<AppState>, chat_id: &str) -> Result<Vec<Album>> {
    let l = line(state).await?;
    let albums = l
        .client
        .base
        .moa()
        .get_all_albums(GetAlbumsOptions::default())
        .await?;
    Ok(albums
        .into_iter()
        .filter(|a| a.chat_id == chat_id)
        .filter_map(|a| {
            Some(Album {
                id: str_of(&a.album_id)?,
                title: a.title.unwrap_or_default(),
                photo_count: a.photo_count.unwrap_or(0),
            })
        })
        .collect())
}

/// Up to `limit` photos of an album (4 for its cover in the list), newest
/// first. Only the list: each photo downloads when the window shows it,
/// through `media://album/...` (`album_photo_file`).
pub async fn album_photos(
    state: &Arc<AppState>,
    chat_id: &str,
    album_id: &str,
    limit: u32,
) -> Result<Vec<AlbumPhoto>> {
    let l = line(state).await?;
    let moa = l.client.base.moa();
    let mut out = Vec::new();
    let mut cursor = None;
    while (out.len() as u32) < limit {
        let reply = moa
            .get_photos(GetPhotosOptions {
                chat_id: chat_id.into(),
                album_id: album_id.into(),
                cursor: cursor.take(),
                page_size: Some((limit - out.len() as u32).min(100)),
                ..Default::default()
            })
            .await?;
        let Some(page) = reply.result else { break };
        for photo in page.photos {
            let Some(oid) = photo
                .oid
                .clone()
                .or_else(|| photo.obs_resource_id.as_ref().and_then(|r| r.oid.clone()))
            else {
                continue;
            };
            let id = photo
                .id
                .as_ref()
                .or(photo.photo_id.as_ref())
                .and_then(str_of)
                .unwrap_or_else(|| oid.clone());
            let video = photo
                .resource_type
                .as_deref()
                .is_some_and(|t| t.eq_ignore_ascii_case("VIDEO"));
            let path = album_photo_path(state, &oid, video)
                .to_string_lossy()
                .into_owned();
            out.push(AlbumPhoto {
                id,
                oid,
                path,
                video,
            });
        }
        match page.next_cursor.filter(|c| !c.is_empty()) {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    out.truncate(limit as usize);
    Ok(out)
}

/// Where an album photo, or the video itself, is cached.
fn album_photo_path(state: &AppState, oid: &str, video: bool) -> std::path::PathBuf {
    let ext = if video { "mp4" } else { "jpg" };
    state
        .paths
        .media()
        .join(format!("album-{}.{ext}", crate::media::safe_name(oid)))
}

/// One album photo, or with `video` the video itself (`album/v`; a
/// video's photo is its still), streamed into the media cache on first
/// use.
pub async fn album_photo_file(
    state: &Arc<AppState>,
    chat_id: &str,
    album_id: &str,
    oid: &str,
    video: bool,
) -> Result<std::path::PathBuf> {
    if state.demo {
        // Demo albums show the demo chat's whiteboard photo.
        return Ok(state.paths.media().join("photo.svg"));
    }
    let dest = album_photo_path(state, oid, video);
    if dest.exists() {
        return Ok(dest);
    }
    let l = line(state).await?;
    let mut file = crate::vault::Writer::create(&dest)?;
    l.client
        .base
        .moa()
        .download_photo_to(
            DownloadPhotoOptions {
                chat_id: chat_id.into(),
                album_id: album_id.into(),
                oid: oid.into(),
                prefix: Some(if video { "album/v" } else { "album/a" }.into()),
            },
            &mut file,
        )
        .await?;
    let saved = file.finish()?;
    crate::cache::schedule_trim(state);
    Ok(saved)
}

pub async fn create_album(state: &Arc<AppState>, chat_id: &str, title: &str) -> Result<()> {
    let l = line(state).await?;
    l.client
        .base
        .moa()
        .create_album(chat_id, title.trim(), Some(true))
        .await?;
    Ok(())
}

pub async fn rename_album(
    state: &Arc<AppState>,
    chat_id: &str,
    album_id: &str,
    title: &str,
) -> Result<()> {
    let l = line(state).await?;
    l.client
        .base
        .moa()
        .update_album_title(chat_id, album_id, title.trim())
        .await?;
    Ok(())
}

pub async fn delete_album(state: &Arc<AppState>, chat_id: &str, album_id: &str) -> Result<()> {
    let l = line(state).await?;
    l.client.base.moa().delete_album(chat_id, album_id).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn found_via_from_the_ui_names_where_they_were_found() {
        let via = |v: Json| serde_json::from_value::<FoundVia>(v).unwrap().into_line();
        assert_eq!(
            via(json!({"kind": "searchId", "searchId": "alice"})),
            AddFriendVia::SearchId("alice".into())
        );
        assert_eq!(via(json!({"kind": "ticket", "ticket": "t1"})), AddFriendVia::UserTicket("t1".into()));
        // A sender's picture: a group member, or a stranger in a 1:1 chat.
        assert_eq!(
            via(json!({"kind": "chat", "chatId": "c1", "messageId": "m1"})),
            AddFriendVia::GroupMemberList { chat_mid: "c1".into() }
        );
        assert_eq!(
            via(json!({"kind": "chat", "chatId": "u1", "messageId": "m1"})),
            AddFriendVia::StrangerMessage { message_id: "m1".into(), chat_mid: "u1".into() }
        );
        assert_eq!(
            via(json!({"kind": "chat", "chatId": "r1"})),
            AddFriendVia::GroupMemberList { chat_mid: "r1".into() }
        );
        assert_eq!(
            via(json!({"kind": "mention", "chatId": "c1", "messageId": "m1"})),
            AddFriendVia::MentionInChat { chat_mid: "c1".into(), message_id: "m1".into() }
        );
        assert_eq!(
            via(json!({"kind": "sharedContact", "chatId": "c1", "messageId": "m1", "senderId": "u2"})),
            AddFriendVia::ShareContact { message_id: "m1".into(), chat_mid: "c1".into(), sender_mid: "u2".into() }
        );
        // find_contact hands it to the UI in the same shape.
        assert_eq!(
            serde_json::to_value(FoundVia::SearchId { search_id: "alice".into() }).unwrap(),
            json!({"kind": "searchId", "searchId": "alice"})
        );
    }

    fn stored(meta: Value) -> Value {
        let mut raw = tv!({"id": "777", "to": "u-friend"});
        raw.insert("contentMetadata", meta);
        raw
    }

    #[test]
    fn keep_copies_a_photo_with_its_key_and_size() {
        let raw = stored(tv!({"OID": "SRC", "SID": "emi", "FILE_SIZE": "2048",
            "MEDIA_CONTENT_INFO": "{\"category\":\"original\",\"extension\":\"png\",\"width\":40,\"height\":30}"}));
        let f = forward_request("u-me", "IMAGE", &raw, &json!({"keyMaterial": "KEY"})).unwrap();
        assert_eq!(f.to, "u-me");
        assert_eq!(f.o_type, line_rs::obs::ObjType::Image);
        assert_eq!((f.src_oid.as_str(), f.src_message_id.as_str()), ("SRC", "777"));
        assert_eq!(f.key_material, "KEY");
        assert_eq!(f.file_size, 2048);
        assert_eq!(f.file_name.as_deref(), Some("image.png"));
        assert_eq!((f.opts.width, f.opts.height), (Some(40), Some(30)));
    }

    #[test]
    fn keep_copies_gifs_clips_and_files_as_what_they_are() {
        use line_rs::obs::ObjType;
        let gif = stored(tv!({"OID": "o", "MEDIA_CONTENT_INFO": "{\"extension\":\"gif\",\"animated\":true}"}));
        let key = json!({"keyMaterial": "K"});
        assert_eq!(forward_request("u", "IMAGE", &gif, &key).unwrap().o_type, ObjType::Gif);

        let voice = stored(tv!({"OID": "o", "DURATION": "3100"}));
        let f = forward_request("u", "AUDIO", &voice, &key).unwrap();
        assert_eq!((f.o_type, f.opts.duration_ms), (ObjType::Audio, Some(3100)));

        let file = stored(tv!({"OID": "o", "FILE_SIZE": "9"}));
        let f = forward_request("u", "FILE", &file, &json!({"keyMaterial": "K", "fileName": "report.pdf"})).unwrap();
        assert_eq!((f.o_type, f.file_name.as_deref()), (ObjType::File, Some("report.pdf")));
    }

    #[test]
    fn keep_sends_again_what_it_cannot_copy() {
        let key = json!({"keyMaterial": "K"});
        // No object, no key, or not media.
        assert!(forward_request("u", "IMAGE", &stored(tv!({"SID": "emi"})), &key).is_none());
        assert!(forward_request("u", "VIDEO", &stored(tv!({"OID": "o"})), &json!({})).is_none());
        assert!(forward_request("u", "STICKER", &stored(tv!({"OID": "o"})), &key).is_none());
    }

    #[test]
    fn reads_v3_and_legacy_contact_lookups() {
        let v3 = json!({"targetUserMid": "u1", "userType": "USER",
            "targetProfileDetail": {"profileName": "A", "picturePath": "/0h"}});
        let f = found_contact(&v3).unwrap();
        assert_eq!((f.mid.as_str(), f.name.as_str(), f.picture.as_deref()), ("u1", "A", Some("/0h")));
        let legacy = json!({"contact": {"mid": "u2", "displayName": "B"}});
        assert_eq!(found_contact(&legacy).unwrap().mid, "u2");
        assert!(found_contact(&json!({})).is_none());
    }

    #[test]
    fn profile_card_from_get_contacts_v3() {
        let reply = json!({"responses": [{
            "targetUserMid": "u1",
            "targetProfileDetail": {"profileName": "A", "statusMessage": {"content": " hi "}},
            "friendDetail": {"user": {}},
            "blockDetail": {"user": {}},
        }]});
        let p = profile_from_v3(&reply, "u1").unwrap();
        assert_eq!((p.name.as_str(), p.status_message.as_deref()), ("A", Some(" hi ")));
        assert!(p.friend && p.blocked);
        assert!(profile_from_v3(&reply, "u2").is_none());
        let legacy = profile_from_contact(&json!({"displayName": "B", "status": "FRIEND_BLOCKED"}), "u2");
        assert!(legacy.friend && legacy.blocked);
    }

    #[test]
    fn reads_image_sizes_from_headers() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend(640u32.to_be_bytes());
        png.extend(480u32.to_be_bytes());
        assert_eq!(image_size(&png), Some((640, 480)));
        let gif = b"GIF89a\x20\x03\x58\x02".to_vec();
        assert_eq!(image_size(&gif), Some((800, 600)));
        // SOI, an APP0 segment, then SOF0 with height 300 and width 400.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0x2C, 0x01, 0x90,
        ];
        assert_eq!(image_size(&jpeg), Some((400, 300)));
        assert_eq!(image_size(b"nope"), None);
    }

    #[test]
    fn parses_note_feed() {
        let result = json!({"feeds": [
            {"post": {"postInfo": {"postId": "1153", "createdTime": 1700000000000i64, "likeCount": 2, "commentCount": 1},
                "userInfo": {"writerMid": "u1", "nickname": "กานดา"},
                "contents": {"text": " สรุปประชุม ", "media": [{"objectId": "a"}]}}},
            {"post": {"postInfo": {}}}
        ]});
        let notes = parse_notes(&result);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].author_name, "กานดา");
        assert_eq!(notes[0].text, "สรุปประชุม");
        assert_eq!(
            (
                notes[0].media_count,
                notes[0].like_count,
                notes[0].comment_count
            ),
            (1, 2, 1)
        );
        assert_eq!(notes[0].media[0].object_id, "a");
        assert_eq!(notes[0].media[0].obs, "myhome/h/a");
    }

    #[test]
    fn media_lies_where_the_post_says() {
        let m = json!({"objectId": "o1", "serviceName": "myhome", "obsNamespace": "hm"});
        assert_eq!(obs_path(&m, "myhome/h").as_deref(), Some("myhome/hm/o1"));
        let odd = json!({"objectId": "o1", "serviceName": "../x", "obsNamespace": "h"});
        assert_eq!(obs_path(&odd, "myhome/h").as_deref(), Some("myhome/h/o1"));
        assert_eq!(obs_path(&json!({}), "myhome/h"), None);
    }

    #[test]
    fn parses_comments_with_images() {
        let result = json!({"commentList": [
            {"commentId": "9", "commentText": "สวย", "createdTime": 1700000000000i64,
             "userInfo": {"mid": "u2", "nickname": "O", "pictureUrl": "https://profile.line-scdn.net/0hAbc"},
             "contentsList": [{"categoryId": "photo", "extData": {"objectId": "img1"}}]}
        ]});
        let c = parse_comments(&result);
        assert_eq!(
            (c[0].author_name.as_str(), c[0].text.as_str()),
            ("O", "สวย")
        );
        assert_eq!(c[0].author_picture.as_deref(), Some("/0hAbc"));
        assert_eq!(c[0].images, vec!["myhome/cmt/img1"]);
    }

    #[test]
    fn finds_the_cover_anywhere_in_a_home_profile() {
        let nested = serde_json::json!({"result": {"homeInfo": {"coverObsInfo": {"objectId": "abc", "serviceName": "myhome", "obsNamespace": "c"}}}});
        assert_eq!(cover_object_id(&nested).as_deref(), Some("myhome/c/abc"));
        let flat = serde_json::json!({"result": [{"coverObjectId": "xyz"}]});
        assert_eq!(cover_object_id(&flat).as_deref(), Some("myhome/c/xyz"));
        assert_eq!(cover_object_id(&serde_json::json!({"result": {}})), None);
    }

    #[test]
    fn calendar_times_are_seconds_on_the_wire() {
        // Tue 3 Nov 2026, 18:00 to 19:00 in Bangkok, as LINE sends it.
        let wire = line_rs::client::calendar::Event {
            start_time: 1_793_703_600,
            end_time: 1_793_707_200,
            ..Default::default()
        };
        let e = calendar_event(wire, "c1", "u-me");
        assert_eq!((e.start, e.end), (1_793_703_600_000, 1_793_707_200_000));

        let input = CalendarEventInput {
            title: "x".into(),
            start: e.start,
            end: e.end,
            all_day: false,
            timezone: "Asia/Bangkok".into(),
            location: String::new(),
            url: String::new(),
            description: String::new(),
            reminders: None,
            repeat: None,
        };
        let out = input.to_line(None);
        assert_eq!((out.start_time, out.end_time), (1_793_703_600, 1_793_707_200));
    }

    #[test]
    fn an_edit_keeps_what_the_form_does_not_show() {
        let fetched = cal::Event {
            id: "a-1".into(),
            title: "old".into(),
            timezone: "Asia/Tokyo".into(),
            color_id: 4,
            stamp_id: "s".into(),
            message_id: Some("m".into()),
            recurrence: Some(cal::Recurrence {
                frequency: Frequency::Weekly,
                interval: 1,
                by_day: vec!["MO".into()],
                ..Default::default()
            }),
            reminders: vec![Reminder::OneDayBefore],
            invitees: vec![cal::Invitee {
                invitee_id: "u2".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let input = CalendarEventInput {
            title: " new ".into(),
            start: 2_000_000,
            end: 3_000_000,
            all_day: false,
            timezone: "Asia/Tokyo".into(),
            location: String::new(),
            url: String::new(),
            description: String::new(),
            reminders: None,
            repeat: None,
        };
        let out = input.to_line(Some(&fetched));
        assert_eq!(out.title, "new");
        assert_eq!(out.color_id, 4);
        assert_eq!(out.stamp_id, "s");
        assert_eq!(out.message_id.as_deref(), Some("m"));
        assert_eq!(out.recurrence, fetched.recurrence);
        assert_eq!(out.reminders, [Reminder::OneDayBefore]);
        assert_eq!(out.invitees, ["u2"]);
        assert_eq!(out.original_invitees, ["u2"]);

        let out = CalendarEventInput {
            reminders: Some(vec![1, 3]),
            ..input
        }
        .to_line(Some(&fetched));
        assert_eq!(out.reminders, [Reminder::OneHourBefore, Reminder::TwoHoursBefore]);
    }

    #[test]
    fn the_form_sets_changes_or_removes_the_repeat_rule() {
        let fetched = cal::Event {
            recurrence: Some(cal::Recurrence {
                frequency: Frequency::Weekly,
                interval: 1,
                by_day: vec!["MO".into()],
                except_ids: vec![500],
                ..Default::default()
            }),
            ..Default::default()
        };
        let input = |repeat: RepeatInput| CalendarEventInput {
            title: "x".into(),
            start: 0,
            end: 0,
            all_day: false,
            timezone: "Asia/Bangkok".into(),
            location: String::new(),
            url: String::new(),
            description: String::new(),
            reminders: None,
            repeat: Some(repeat),
        };
        let rule = |frequency: &str, interval, until, by_day: &[&str]| RepeatInput {
            frequency: frequency.into(),
            interval,
            until,
            by_day: by_day.iter().map(|d| d.to_string()).collect(),
        };
        let out = input(rule("monthly", 2, Some(1_800_000_000_999), &["-1FR"])).to_line(Some(&fetched));
        assert_eq!(
            out.recurrence,
            Some(cal::Recurrence {
                frequency: Frequency::Monthly,
                interval: 2,
                until: Some(1_800_000_000),
                by_day: vec!["-1FR".into()],
                except_ids: vec![500],
            })
        );
        let out = input(rule("daily", 0, None, &["MO"])).to_line(None);
        let r = out.recurrence.unwrap();
        assert_eq!((r.frequency, r.interval, r.by_day.len()), (Frequency::Daily, 1, 0));
        assert_eq!(input(rule("none", 1, None, &[])).to_line(Some(&fetched)).recurrence, None);
    }

    #[test]
    fn hidden_calendars_are_left_out_unless_every_one_looks_hidden() {
        let c = |id: &str, visible: bool, listed: bool| cal::Calendar {
            id: id.into(),
            is_visible: visible,
            is_shown_in_calendar_list: listed,
            ..Default::default()
        };
        let ids = |v: Vec<cal::Calendar>| v.into_iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(
            ids(shown_calendars(vec![c("a", true, true), c("b", false, true), c("c", true, false)])),
            ["a"]
        );
        assert_eq!(ids(shown_calendars(vec![c("a", false, false), c("b", false, false)])), ["a", "b"]);
    }

    #[test]
    fn calendar_events_carry_repeats_overrides_and_invitees_in_ms() {
        let wire = cal::Event {
            recurrence: Some(cal::Recurrence {
                frequency: Frequency::Monthly,
                interval: 2,
                until: Some(1_800_000_000),
                by_day: vec!["-1FR".into()],
                except_ids: vec![1_700_000_000],
            }),
            recurrence_child_events: vec![cal::ChildEvent {
                recur_id: 1_700_100_000,
                start_time: 1_700_200_000,
                is_deleted: true,
                ..Default::default()
            }],
            invitees: vec![
                cal::Invitee {
                    invitee_id: "u-me".into(),
                    status: InvitationStatus::NeedsAction,
                    client_updated_at: 7,
                },
                cal::Invitee {
                    invitee_id: "u2".into(),
                    status: InvitationStatus::Declined,
                    ..Default::default()
                },
            ],
            reminders: vec![Reminder::NoonTheDayBefore],
            ..Default::default()
        };
        let e = calendar_event(wire, "c1", "u-me");
        let r = e.recurrence.unwrap();
        assert_eq!((r.frequency, r.interval), ("monthly", 2));
        assert_eq!(r.until, Some(1_800_000_000_000));
        assert_eq!(r.except_ids, [1_700_000_000_000]);
        assert_eq!(e.overrides[0].recur_id, 1_700_100_000_000);
        assert!(e.overrides[0].deleted);
        assert_eq!((e.invitees[0].status, e.invitees[0].me), ("needsAction", true));
        assert_eq!((e.invitees[1].status, e.invitees[1].me), ("declined", false));
        assert_eq!(e.reminders, [2]);
    }

    #[test]
    fn an_edit_carries_the_new_mentions_and_keeps_the_rest() {
        let raw = tv!({"id": "9", "from": "u-me", "to": "c1", "toType": 2,
            "contentMetadata": {"MENTION": "{\"MENTIONEES\":[]}", "e2eeVersion": "2", "OBS_POP": "x"}});
        let span = MentionSpan { start: 0, end: 4, mid: Some("u2".into()), all: false };
        let edited = edit_target(Some(raw), "u-me", "c1", "9", &[span]);
        let meta = &edited["contentMetadata"];
        assert_eq!(meta["OBS_POP"].as_str(), Some("x"));
        assert!(meta["MENTION"].as_str().unwrap().contains("\"M\":\"u2\""));
        assert_eq!(edited["toType"].as_i64(), Some(2));

        let edited = edit_target(None, "u-me", "c1", "9", &[]);
        assert_eq!(edited["from"].as_str(), Some("u-me"));
        assert_eq!(edited["to"].as_str(), Some("c1"));
        assert!(edited["contentMetadata"]["MENTION"].is_null());
    }

    #[test]
    fn edit_and_unsend_refusals_read_in_thai() {
        let talk = |code: &str| line_rs::Error::Request {
            method: "editMessage".into(),
            path: "/S4".into(),
            data: tv!({"code": code, "reason": ""}),
        };
        assert!(refused(talk("NOT_PREMIUM")).to_string().contains("LINE Premium"));
        assert!(refused(talk("MESSAGE_NOT_EDITABLE_TIMELIMIT")).to_string().contains("เลยเวลา"));
        assert!(refused(talk("MESSAGE_NOT_DESTRUCTIBLE")).to_string().contains("24 ชั่วโมง"));
        assert!(refused(talk("NOT_A_MEMBER")).to_string().contains("NOT_A_MEMBER"));
    }
}
