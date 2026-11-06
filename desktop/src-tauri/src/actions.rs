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
use line_rs::client::calendar::{self as cal, EventInput, EventTarget, new_event_id};
use line_rs::link::LineLink;
use line_rs::rest::translate::TranslateText;
use line_rs::service::moa::{DownloadPhotoOptions, GetAlbumsOptions, GetPhotosOptions};
use line_rs::service::relation::AddFriendByMidOptions;
use line_rs::thrift::Value;
use line_rs::timeline::{CreatePost, ListPost, PostMedia};
use line_rs::tv;
use serde::Serialize;
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
/// edits the stored message as LINE for Android does: same chat type,
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

/// Translate `text` into `target` (e.g. `th`). Desktop REST call; unverified.
pub async fn translate(state: &Arc<AppState>, text: &str, target: &str) -> Result<String> {
    let l = line(state).await?;
    let reply = l
        .client
        .base
        .rest()
        .translate(&TranslateText::new(text, target), None)
        .await?;
    reply["translatedText"]
        .as_str()
        .or_else(|| reply["result"]["translatedText"].as_str())
        .map(str::to_string)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow!("translate_empty"))
}

/// Keep Memo is the chat with yourself; "save to Keep" sends a copy there.
pub async fn save_to_keep(state: &Arc<AppState>, chat_id: &str, message_id: &str) -> Result<()> {
    let l = line(state).await?;
    let m = state
        .store()
        .message(message_id)?
        .ok_or_else(|| anyhow!("message_not_found"))?;
    match m.content_type.as_str() {
        "IMAGE" | "VIDEO" | "FILE" => {
            let path = crate::media::download_file(state, chat_id, message_id, m.file_name.clone())
                .await?;
            let data = crate::vault::read(&path)?;
            let name = m.file_name.clone().unwrap_or_else(|| "file".into());
            let chat = l.chat(&l.me);
            let sent = match m.content_type.as_str() {
                "IMAGE" => chat.send_image(data, Some(&name)).await,
                "VIDEO" => chat.send_video(data, Some(&name), None).await,
                _ => chat.send_file(data, &name).await,
            };
            sent.map_err(|e| anyhow!("{e}"))?;
        }
        "STICKER" => bail!("ยังบันทึกสติกเกอร์ลง Keep ไม่ได้"),
        _ if !m.text.trim().is_empty() => {
            l.client
                .send_message(&l.me, line_rs::client::MessageInput::text(m.text.clone()))
                .await?;
        }
        _ => bail!("ข้อความนี้ไม่มีเนื้อหาให้บันทึก"),
    }
    Ok(())
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
    l.client
        .base
        .talk()
        .call(
            "createChatRoomAnnouncement",
            tv!({"chatRoomMid": chat_id, "type": "MESSAGE",
                "contents": {"text": text, "link": format!("line://nv/chatMsg?chatId={chat_id}&messageId={message_id}")}}),
        )
        .await?;
    Ok(())
}

// -- people -----------------------------------------------------------------

pub async fn set_blocked(state: &Arc<AppState>, mid: &str, blocked: bool) -> Result<()> {
    let l = line(state).await?;
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
}

/// Find someone by LINE ID or a line.me / line:// profile link.
pub async fn find_contact(state: &Arc<AppState>, query: &str) -> Result<FoundContact> {
    let l = line(state).await?;
    let query = query.trim();
    let raw = match LineLink::parse(query) {
        Some(link @ LineLink::Profile(_)) => l.client.base.resolve_link(&link).await?,
        Some(_) => bail!("ลิงก์นี้ไม่ใช่ลิงก์เพิ่มเพื่อน"),
        None => {
            let id = query.trim_start_matches('@');
            l.client
                .base
                .talk()
                .call("findContactByUserid", tv!({"searchId": id}))
                .await?
        }
    };
    let json = raw.to_json();
    let c = if json["contact"].is_null() {
        &json
    } else {
        &json["contact"]
    };
    let mid = str_of(&c["mid"]).ok_or_else(|| anyhow!("ไม่พบผู้ใช้"))?;
    Ok(FoundContact {
        name: str_of(&c["displayName"]).unwrap_or_else(|| mid.clone()),
        picture: str_of(&c["picturePath"]),
        mid,
    })
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

/// Someone's profile card: `getContact` for the name, status message and
/// friend/blocked state, the home profile for the cover. The cover is a
/// best effort: without it the card falls back to the profile picture.
pub async fn contact_profile(state: &Arc<AppState>, mid: &str) -> Result<ContactProfile> {
    let l = line(state).await?;
    let c = l.client.base.talk().get_contact(mid).await?.to_json();
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
    let home = line_rs::rest::home::HomeProfileQuery {
        home_id: mid.into(),
        ..Default::default()
    };
    let cover = match l.client.base.rest().home().profile(home).await {
        Ok(json) => cover_object_id(&json),
        Err(e) => {
            tracing::warn!("home profile {mid}: {e}");
            None
        }
    };
    Ok(ContactProfile {
        mid: mid.into(),
        name: str_of(&c["displayNameOverridden"])
            .or_else(|| str_of(&c["displayName"]))
            .unwrap_or_else(|| mid.into()),
        picture: str_of(&c["picturePath"]),
        status_message: str_of(&c["statusMessage"]).filter(|s| !s.trim().is_empty()),
        cover,
        friend: matches!(status, 1 | 2),
        blocked: matches!(status, 2 | 4 | 6),
    })
}

/// The cover's object id in a home profile response. The shape is not
/// documented, so look for `coverObsInfo.objectId` (or `coverObjectId`)
/// anywhere in it.
fn cover_object_id(json: &Json) -> Option<String> {
    match json {
        Json::Object(o) => {
            if let Some(id) = o.get("coverObsInfo").and_then(|c| c.get("objectId")).and_then(str_of) {
                return Some(id);
            }
            if let Some(id) = o.get("coverObjectId").and_then(str_of) {
                return Some(id);
            }
            o.values().find_map(cover_object_id)
        }
        Json::Array(a) => a.iter().find_map(cover_object_id),
        _ => None,
    }
}

/// A profile cover, downloaded into the avatar cache on first use.
pub async fn cover_file(state: &Arc<AppState>, oid: &str) -> Result<std::path::PathBuf> {
    let dest = state
        .paths
        .avatars()
        .join(format!("cover-{}.jpg", crate::media::safe_name(oid)));
    if dest.exists() {
        return Ok(dest);
    }
    let l = line(state).await?;
    let bytes = l
        .client
        .base
        .obs()
        .download_object_for_service("myhome/c", oid, vec![])
        .await
        .map_err(|e| anyhow!("{e}"))?;
    crate::vault::write(&dest, &bytes)?;
    Ok(dest)
}

pub async fn add_friend(state: &Arc<AppState>, mid: &str) -> Result<()> {
    let l = line(state).await?;
    let mut options = AddFriendByMidOptions::new(mid);
    // Found through search by ID, not a group member list.
    options.tracking_meta_type = 3;
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
    /// Image object ids in `myhome/cmt`.
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
                if let Some(id) = o.get("objectId").and_then(str_of) {
                    out.push(id);
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

pub async fn list_notes(state: &Arc<AppState>, chat_id: &str) -> Result<Vec<NotePost>> {
    let l = line(state).await?;
    let reply = l
        .client
        .base
        .timeline()
        .list_post(ListPost {
            home_id: chat_id.into(),
            ..Default::default()
        })
        .await?;
    check_timeline(reply.code, &reply.message)?;
    Ok(parse_notes(&reply.result))
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
        let (object_id, _) = timeline.upload_note_media(video, data).await?;
        post.contents.media.push(PostMedia {
            object_id,
            kind: if video { "VIDEO" } else { "PHOTO" }.into(),
        });
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
    // line-rs has no comment list; this is the endpoint LINE's note server
    // answers (checked live 2026-10-22, POST with x-lhm: GET).
    let mut headers = timeline.init().await?.clone();
    headers.insert("x-lhm", reqwest::header::HeaderValue::from_static("GET"));
    let url = url::Url::parse_with_params(
        &format!(
            "https://{}/ext/note/nt/api/v57/comment/getList.json",
            l.client.base.endpoint
        ),
        &[
            ("homeId", chat_id),
            ("contentId", post_id),
            ("actorId", chat_id),
            ("limit", "100"),
        ],
    )?;
    let body: Json = l
        .client
        .base
        .http
        .post(url)
        .headers(headers)
        .send()
        .await?
        .json()
        .await?;
    if body["code"].as_i64().unwrap_or(0) != 0 {
        tracing::warn!("note comments {post_id}: {body}");
    }
    Ok(NoteDetail {
        note,
        comments: parse_comments(&body["result"]),
    })
}

pub async fn add_comment(
    state: &Arc<AppState>,
    chat_id: &str,
    post_id: &str,
    text: &str,
) -> Result<()> {
    if text.trim().is_empty() {
        bail!("empty_comment");
    }
    let l = line(state).await?;
    let reply = l
        .client
        .base
        .timeline()
        .create_comment(post_id, text, chat_id, None, Vec::new())
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
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    /// IANA zone the event was made in; all-day dates read in it.
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

fn calendar_event(e: line_rs::client::calendar::Event, chat_id: &str) -> CalendarEvent {
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
}

impl CalendarEventInput {
    /// The event to send: the form's fields over `base`, the event as LINE
    /// last sent it (for an edit), so what the form doesn't show (repeats,
    /// reminders, invitees, colour, stamp, the message it came from) goes
    /// back as it was.
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
        .map(|e| calendar_event(e, chat_id))
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

/// Albums of one chat. LINE lists every album, so filter by chat.
pub async fn list_albums(state: &Arc<AppState>, chat_id: &str) -> Result<Vec<Album>> {
    let l = line(state).await?;
    let reply = l
        .client
        .base
        .moa()
        .get_albums(GetAlbumsOptions::default())
        .await?;
    if let Some(code) = reply.code.filter(|c| *c != 0) {
        bail!("album {code}: {}", reply.message.unwrap_or_default());
    }
    Ok(reply
        .result
        .map(|r| r.albums)
        .unwrap_or_default()
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
            let path = album_photo_path(state, &oid).to_string_lossy().into_owned();
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

fn album_photo_path(state: &AppState, oid: &str) -> std::path::PathBuf {
    state
        .paths
        .media()
        .join(format!("album-{}.jpg", crate::media::safe_name(oid)))
}

/// One album photo, downloaded into the media cache on first use.
pub async fn album_photo_file(
    state: &Arc<AppState>,
    chat_id: &str,
    album_id: &str,
    oid: &str,
) -> Result<std::path::PathBuf> {
    let dest = album_photo_path(state, oid);
    if dest.exists() {
        return Ok(dest);
    }
    let l = line(state).await?;
    let bytes = l
        .client
        .base
        .moa()
        .download_photo(DownloadPhotoOptions {
            chat_id: chat_id.into(),
            album_id: album_id.into(),
            oid: oid.into(),
            prefix: None,
        })
        .await?;
    crate::vault::write(&dest, &bytes)?;
    crate::cache::schedule_trim(state);
    Ok(dest)
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
        assert_eq!(c[0].images, vec!["img1"]);
    }

    #[test]
    fn finds_the_cover_anywhere_in_a_home_profile() {
        let nested = serde_json::json!({"result": {"homeInfo": {"coverObsInfo": {"objectId": "abc", "serviceName": "myhome"}}}});
        assert_eq!(cover_object_id(&nested).as_deref(), Some("abc"));
        let flat = serde_json::json!({"result": [{"coverObjectId": "xyz"}]});
        assert_eq!(cover_object_id(&flat).as_deref(), Some("xyz"));
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
        let e = calendar_event(wire, "c1");
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
                frequency: cal::Frequency::Weekly,
                interval: 1,
                by_day: vec!["MO".into()],
                ..Default::default()
            }),
            reminders: vec![cal::Reminder::OneDayBefore],
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
        };
        let out = input.to_line(Some(&fetched));
        assert_eq!(out.title, "new");
        assert_eq!(out.color_id, 4);
        assert_eq!(out.stamp_id, "s");
        assert_eq!(out.message_id.as_deref(), Some("m"));
        assert_eq!(out.recurrence, fetched.recurrence);
        assert_eq!(out.reminders, [cal::Reminder::OneDayBefore]);
        assert_eq!(out.invitees, ["u2"]);
        assert_eq!(out.original_invitees, ["u2"]);
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
