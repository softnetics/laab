//! Turn LINE thrift values (as JSON) into the app's message model, and build
//! chat-list preview text.

use std::collections::BTreeMap;

use serde_json::Value as Json;

use crate::model::{MentionSpan, Message, Reaction};

pub fn str_of(v: &Json) -> Option<String> {
    match v {
        Json::String(s) if !s.is_empty() => Some(s.clone()),
        Json::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn num_of(v: &Json) -> Option<f64> {
    match v {
        Json::Number(n) => n.as_f64(),
        Json::String(s) => s.parse().ok(),
        _ => None,
    }
}

const CONTENT_TYPES: [&str; 23] = [
    "NONE",
    "IMAGE",
    "VIDEO",
    "AUDIO",
    "HTML",
    "PDF",
    "CALL",
    "STICKER",
    "PRESENCE",
    "GIFT",
    "GROUPBOARD",
    "APPLINK",
    "LINK",
    "CONTACT",
    "FILE",
    "LOCATION",
    "POSTNOTIFICATION",
    "RICH",
    "CHATEVENT",
    "MUSIC",
    "PAYMENT",
    "EXTIMAGE",
    "FLEX",
];

/// `contentType` as its enum name, whether LINE sent the number or the name.
pub fn content_type_name(v: &Json) -> String {
    match v {
        Json::String(s) if !s.is_empty() => s.to_ascii_uppercase(),
        Json::Number(n) => n
            .as_u64()
            .and_then(|i| CONTENT_TYPES.get(i as usize))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "NONE".into()),
        _ => "NONE".into(),
    }
}

fn metadata(raw: &Json) -> BTreeMap<String, String> {
    raw["contentMetadata"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| str_of(v).or(Some(String::new())).map(|s| (k.clone(), s)))
                .collect()
        })
        .unwrap_or_default()
}

/// `MENTION` metadata: `{"MENTIONEES":[{"S":"0","E":"5","M":"u…"} | {"S","E","A":"1"}]}`.
pub fn parse_mentions(meta: &BTreeMap<String, String>) -> Vec<MentionSpan> {
    let Some(raw) = meta.get("MENTION") else {
        return vec![];
    };
    let Ok(json) = serde_json::from_str::<Json>(raw) else {
        return vec![];
    };
    json["MENTIONEES"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|m| {
                    let start = num_of(&m["S"])? as i64;
                    let end = num_of(&m["E"])? as i64;
                    if start < 0 || end <= start {
                        return None;
                    }
                    let all = str_of(&m["A"]).as_deref() == Some("1");
                    Some(MentionSpan {
                        start,
                        end,
                        mid: if all { None } else { str_of(&m["M"]) },
                        all,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `MENTION` metadata for outgoing spans.
pub fn mention_metadata(spans: &[MentionSpan]) -> Option<String> {
    if spans.is_empty() {
        return None;
    }
    let list: Vec<Json> = spans
        .iter()
        .map(|s| {
            if s.all {
                serde_json::json!({"S": s.start.to_string(), "E": s.end.to_string(), "A": "1"})
            } else {
                serde_json::json!({"S": s.start.to_string(), "E": s.end.to_string(), "M": s.mid.clone().unwrap_or_default()})
            }
        })
        .collect();
    Some(serde_json::json!({ "MENTIONEES": list }).to_string())
}

const REACTION_NAMES: [&str; 8] = [
    "ALL", "UNDO", "NICE", "LOVE", "FUN", "AMAZING", "SAD", "OMG",
];

pub fn reaction_name(v: &Json) -> Option<String> {
    match v {
        Json::String(s) if REACTION_NAMES[2..].contains(&s.as_str()) => Some(s.clone()),
        Json::Number(n) => n
            .as_u64()
            .filter(|i| (2..8).contains(i))
            .map(|i| REACTION_NAMES[i as usize].to_string()),
        _ => None,
    }
}

/// History `reactions`: `[{fromUserMid, atMillis, reactionType:{predefinedReactionType}}]`.
/// Paid reactions carry unnamed thrift fields instead of a predefined type.
pub fn parse_reactions(raw: &Json) -> Option<Vec<Reaction>> {
    let list = raw["reactions"].as_array()?;
    Some(
        list.iter()
            .filter_map(|r| {
                let user_id = str_of(&r["fromUserMid"]).or_else(|| str_of(&r["1"]))?;
                let kind = &r["reactionType"];
                let reaction = reaction_name(&kind["predefinedReactionType"])
                    .or_else(|| reaction_name(&kind["1"]))
                    .unwrap_or_else(|| "PAID".into());
                Some(Reaction { user_id, reaction })
            })
            .collect(),
    )
}

/// Which chat a message belongs to: ours or anything sent to a group/room
/// lives under `to`; a 1:1 message from someone else under `from`.
pub fn chat_id_for(from: &str, to: &str, mine: bool) -> String {
    if mine || to.starts_with('c') || to.starts_with('r') {
        to.to_string()
    } else {
        from.to_string()
    }
}

/// Build the app message from a decrypted LINE `Message` (as JSON).
/// `decrypted` is false when E2EE decryption failed, so the text is unknown.
pub fn message_from_json(raw: &Json, me: &str, decrypted: bool) -> Option<Message> {
    let id = str_of(&raw["id"])?;
    let from = str_of(&raw["from"])
        .or_else(|| str_of(&raw["_from"]))
        .unwrap_or_default();
    let to = str_of(&raw["to"]).unwrap_or_default();
    let mine = !me.is_empty() && from == me;
    let content_type = content_type_name(&raw["contentType"]);
    let meta = metadata(raw);
    let mut text = if decrypted {
        raw["text"].as_str().unwrap_or("").to_string()
    } else {
        String::new()
    };
    let unsent = if meta.get("UNSENT").map(String::as_str) == Some("true") {
        if meta.get("SILENTLY_UNSENT").map(String::as_str) == Some("true") {
            2
        } else {
            1
        }
    } else {
        0
    };
    // Calls and group events carry protocol text; the UI words them itself.
    if matches!(
        content_type.as_str(),
        "CALL" | "CHATEVENT" | "POSTNOTIFICATION"
    ) || unsent > 0
    {
        text.clear();
    }
    let file_name = meta
        .get("FILE_NAME")
        .or_else(|| meta.get("FILENAME"))
        .cloned()
        .filter(|s| !s.is_empty());
    let mentions = parse_mentions(&meta);
    let mentions_me = mentions
        .iter()
        .any(|m| m.all || m.mid.as_deref() == Some(me));
    let reply_to = str_of(&raw["relatedMessageId"]).filter(|_| {
        let rel = &raw["messageRelationType"];
        rel.as_str() == Some("REPLY") || rel.as_i64() == Some(3)
    });
    let created = num_of(&raw["createdTime"])
        .or_else(|| num_of(&raw["deliveredTime"]))
        .unwrap_or(0.0);
    Some(Message {
        chat_id: chat_id_for(&from, &to, mine),
        id,
        text,
        file_name: if unsent > 0 { None } else { file_name },
        content_type,
        created_time: created,
        mine,
        sender_id: if from.is_empty() { None } else { Some(from) },
        sticker_id: meta.get("STKID").cloned(),
        sticker_package: meta.get("STKPKGID").cloned(),
        sticker_version: meta.get("STKVER").cloned(),
        mentions: if unsent > 0 { vec![] } else { mentions },
        mentions_me,
        file_size: meta.get("FILE_SIZE").and_then(|s| s.parse().ok()),
        duration_ms: meta.get("DURATION").and_then(|s| s.parse().ok()),
        reply_to,
        unsent,
        edited: raw["contentMetadata"].get("EDITED").is_some()
            || raw["messageRelationType"].as_str() == Some("EDIT"),
        reactions: parse_reactions(raw).unwrap_or_default(),
        raw: Some(serde_json::to_string_pretty(raw).unwrap_or_default()),
        metadata: meta,
        ..Default::default()
    })
}

fn truncate(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() > max {
        format!("{}…", chars[..max - 1].iter().collect::<String>())
    } else {
        text.to_string()
    }
}

fn duration_label(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Call wording for previews and notifications.
pub fn call_text(m: &Message, caller: &str) -> String {
    let meta = &m.metadata;
    let group =
        meta.get("TYPE").map(String::as_str) == Some("G") || meta.contains_key("GC_EVT_TYPE");
    let video = if group {
        matches!(
            meta.get("GC_MEDIA_TYPE").map(String::as_str),
            Some("VIDEO") | Some("V")
        )
    } else {
        meta.get("TYPE").map(String::as_str) == Some("V")
    };
    if group {
        return if meta.get("GC_EVT_TYPE").map(String::as_str) == Some("E") {
            "สิ้นสุดการโทรแบบกลุ่มแล้ว".into()
        } else {
            format!("📞 {caller} เริ่มการโทรแบบกลุ่มแล้ว")
        };
    }
    let icon = if video { "🎥" } else { "📞" };
    match meta.get("RESULT").map(String::as_str) {
        // LINE for Mac's list reads "📞 เวลาโทร : 01:25" for answered calls.
        Some("NORMAL") => format!(
            "{icon} เวลาโทร : {}",
            duration_label(m.duration_ms.unwrap_or(0))
        ),
        Some("NO_RESPONSE") if m.mine => format!("{icon} ไม่มีผู้รับสาย"),
        Some("REJECTED") if m.mine => format!("{icon} สายถูกปฏิเสธ"),
        Some("REJECTED") => format!("{icon} ปฏิเสธสาย"),
        Some("CANCELED") if m.mine => format!("{icon} ยกเลิก"),
        Some("NO_RESPONSE") | Some("CANCELED") => format!("{icon} สายที่ไม่ได้รับ"),
        _ => format!("{icon} การโทร"),
    }
}

/// The body of a chat-list preview or notification, without "คุณ: ".
pub fn preview_body(m: &Message) -> String {
    if m.unsent > 0 {
        return "ยกเลิกข้อความ".into();
    }
    match m.content_type.as_str() {
        "IMAGE" => "รูปภาพ".into(),
        "VIDEO" => "วิดีโอ".into(),
        "STICKER" => "สติกเกอร์".into(),
        "AUDIO" => "ข้อความเสียง".into(),
        "FILE" => m.file_name.clone().unwrap_or_else(|| "ไฟล์".into()),
        "CALL" => "การโทร".into(),
        "CHATEVENT" => "การเปลี่ยนแปลงในกลุ่ม".into(),
        "FLEX" | "RICH" => {
            let alt = m.metadata.get("ALT_TEXT").cloned().unwrap_or_default();
            if alt.is_empty() {
                "ข้อความ Flex".into()
            } else {
                truncate(&alt, 64)
            }
        }
        "POSTNOTIFICATION" => m
            .metadata
            .get("text")
            .filter(|t| !t.is_empty())
            .map(|t| truncate(t, 64))
            .unwrap_or_else(|| "โพสต์".into()),
        "LOCATION" => "ตำแหน่ง".into(),
        "CONTACT" => "รายชื่อติดต่อ".into(),
        _ => {
            let text = m.text.trim();
            if text.is_empty() {
                "ข้อความ".into()
            } else {
                truncate(text, 64)
            }
        }
    }
}

pub fn preview_line(m: &Message) -> String {
    // A new group note reads as LINE's "เขียนโน้ตใหม่", without "คุณ:".
    if m.content_type == "POSTNOTIFICATION" {
        return "เขียนโน้ตใหม่".into();
    }
    if m.content_type == "CALL" {
        return call_text(m, if m.mine { "คุณ" } else { "เขา" });
    }
    format!("{}: {}", if m.mine { "คุณ" } else { "เขา" }, preview_body(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn routes_messages_to_their_chat() {
        assert_eq!(chat_id_for("u1", "c9", false), "c9");
        assert_eq!(chat_id_for("u1", "r9", false), "r9");
        assert_eq!(chat_id_for("u1", "u2", false), "u1");
        assert_eq!(chat_id_for("u2", "u1", true), "u1");
    }

    #[test]
    fn reads_mentions_stickers_and_unsent() {
        let raw = json!({
            "id": "100", "from": "u1", "to": "c1", "contentType": 0, "text": "@Arthit hi", "createdTime": 1700000000000_i64,
            "contentMetadata": {"MENTION": "{\"MENTIONEES\":[{\"S\":\"0\",\"E\":\"7\",\"M\":\"u-me\"}]}"}
        });
        let m = message_from_json(&raw, "u-me", true).unwrap();
        assert_eq!(m.chat_id, "c1");
        assert!(m.mentions_me);
        assert_eq!(m.mentions[0].end, 7);

        let raw = json!({"id": "101", "from": "u-me", "to": "u2", "contentType": "STICKER",
            "contentMetadata": {"STKID": "52002734", "STKPKGID": "11537", "STKVER": "1"}});
        let m = message_from_json(&raw, "u-me", true).unwrap();
        assert!(m.mine);
        assert_eq!(m.sticker_package.as_deref(), Some("11537"));
        assert_eq!(preview_line(&m), "คุณ: สติกเกอร์");

        let raw = json!({"id": "103", "from": "u2", "to": "u-me", "contentType": "RICH",
            "contentMetadata": {"ALT_TEXT": "โปรโมชันใหม่"}});
        let m = message_from_json(&raw, "u-me", true).unwrap();
        assert_eq!(preview_line(&m), "เขา: โปรโมชันใหม่");

        let raw = json!({"id": "102", "from": "u2", "to": "u-me", "text": "secret", "contentType": 0,
            "contentMetadata": {"UNSENT": "true", "SILENTLY_UNSENT": "false"}});
        let m = message_from_json(&raw, "u-me", true).unwrap();
        assert_eq!((m.unsent, m.text.as_str()), (1, ""));
    }

    #[test]
    fn reads_reactions_including_paid() {
        let raw = json!({"id": "1", "reactions": [
            {"fromUserMid": "u1", "reactionType": {"predefinedReactionType": "LOVE"}},
            {"fromUserMid": "u2", "reactionType": {"predefinedReactionType": 2}},
            {"fromUserMid": "u3", "reactionType": {"2": {"1": "p"}}}
        ]});
        let r = parse_reactions(&raw).unwrap();
        assert_eq!(
            r.iter().map(|r| r.reaction.as_str()).collect::<Vec<_>>(),
            ["LOVE", "NICE", "PAID"]
        );
    }

    #[test]
    fn words_calls() {
        let mut m = Message {
            content_type: "CALL".into(),
            mine: false,
            ..Default::default()
        };
        m.metadata.insert("TYPE".into(), "A".into());
        m.metadata.insert("RESULT".into(), "NO_RESPONSE".into());
        assert_eq!(call_text(&m, "x"), "📞 สายที่ไม่ได้รับ");
        m.metadata.insert("RESULT".into(), "NORMAL".into());
        m.duration_ms = Some(185_000);
        assert_eq!(call_text(&m, "x"), "📞 เวลาโทร : 03:05");
    }

    #[test]
    fn builds_outgoing_mentions() {
        let meta = mention_metadata(&[MentionSpan {
            start: 0,
            end: 4,
            mid: None,
            all: true,
        }])
        .unwrap();
        assert!(meta.contains("\"A\":\"1\""));
    }
}
