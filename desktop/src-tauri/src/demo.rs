//! Fictional chats for `--demo`, used for screenshots and UI checks without a
//! real LINE account. Runs against a throwaway data folder.

use std::collections::BTreeMap;

use anyhow::Result;

use crate::model::{Chat, ChatMember, MentionSpan, Message, Reaction};
use crate::paths::Paths;
use crate::store::Store;

fn svg_avatar(paths: &Paths, id: &str, emoji: &str, color: &str) -> Option<String> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="160"><rect width="160" height="160" fill="{color}"/><text x="80" y="108" font-size="92" text-anchor="middle">{emoji}</text></svg>"#
    );
    let path = paths.avatars().join(format!("{id}.svg"));
    std::fs::write(&path, svg).ok()?;
    Some(path.to_string_lossy().into_owned())
}

fn whiteboard(paths: &Paths) -> Option<String> {
    let mut boxes = String::new();
    for (x, y, color, label) in [
        (40, 70, "#FFE08A", "Cart"),
        (200, 70, "#A9E5B5", "Checkout"),
        (360, 70, "#A8C8F0", "Payment"),
        (200, 200, "#F5B3C2", "Receipt"),
    ] {
        boxes.push_str(&format!(
            r##"<rect x="{x}" y="{y}" width="120" height="70" rx="10" fill="{color}"/><text x="{}" y="{}" font-family="-apple-system" font-size="20" font-weight="600" fill="#333" text-anchor="middle">{label}</text>"##,
            x + 60,
            y + 42
        ));
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="520" height="340"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#F6F1E7"/><stop offset="1" stop-color="#E4ECF5"/></linearGradient></defs><rect width="520" height="340" fill="url(#g)"/>{boxes}<path d="M160 105H200M320 105H360M260 140V200" stroke="#555" stroke-width="3"/></svg>"##
    );
    let path = paths.media().join("photo.svg");
    std::fs::write(&path, svg).ok()?;
    Some(path.to_string_lossy().into_owned())
}

pub fn extra_team_members() -> Vec<(&'static str, &'static str)> {
    vec![
        ("u_mind", "มายด์ QA"),
        ("u_ken", "Ken"),
        ("u_toey", "ใบเตย PM"),
        ("u_khim", "ขิม"),
        ("u_ice", "Ice Mobile"),
        ("u_non", "Non Backend"),
    ]
}

pub fn seed(store: &mut Store, paths: &Paths) -> Result<()> {
    let now = chrono_like_today();
    let (minute, day) = (60_000.0, 86_400_000.0);
    let today = now;
    let chat = |id: &str,
                name: &str,
                kind: &str,
                preview: &str,
                time: f64,
                unread: i64,
                avatar: Option<String>| Chat {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        preview: preview.into(),
        last_activity: time,
        unread,
        avatar,
        ..Default::default()
    };
    let chats = vec![
        chat(
            "c_harvest",
            "ทีม Harvest App",
            "group",
            "คุณ: @Arm ช่วยเช็ก endpoint ชำระเงินอีกรอบนะครับ",
            today + 13.0 * 60.0 * minute + 10.0 * minute,
            3,
            svg_avatar(paths, "c_harvest", "🌱", "#2F9E6A"),
        ),
        chat(
            "c_run",
            "ชมรมวิ่งเช้าวันอาทิตย์",
            "group",
            "เขา: อาทิตย์นี้เจอกันประตู 2 ตีห้าครึ่งนะ",
            today + 11.0 * 60.0 * minute + 42.0 * minute,
            12,
            svg_avatar(paths, "c_run", "🏃", "#F4C430"),
        ),
        chat(
            "u_lead",
            "คุณวิภา",
            "dm",
            "คุณ: ส่งไฟล์งบประมาณให้แล้วครับ",
            today + 10.0 * 60.0 * minute + 54.0 * minute,
            0,
            svg_avatar(paths, "u_lead", "👩🏻‍💼", "#6BA8E5"),
        ),
        chat(
            "u_kanya",
            "Kanya Design",
            "dm",
            "เขา: อัปหน้า checkout ใน Figma แล้วนะคะ",
            today - day + 18.0 * 60.0 * minute,
            0,
            svg_avatar(paths, "u_kanya", "🎨", "#F2A7C3"),
        ),
        chat(
            "u_arm",
            "Arm",
            "dm",
            "📞 เวลาโทร : 03:05",
            today - 2.0 * day,
            0,
            svg_avatar(paths, "u_arm", "🧑🏻‍💻", "#7FC8A9"),
        ),
        chat(
            "u_jane",
            "เจน",
            "dm",
            "เขา: เจอกันหกโมงเย็นหน้าร้านนะ",
            today - 3.0 * day,
            0,
            svg_avatar(paths, "u_jane", "🦊", "#E9D8B5"),
        ),
        chat(
            "b_parcel",
            "ส่งไว Express",
            "bot",
            "เขา: พัสดุของคุณถึงปลายทางแล้ว",
            today - 4.0 * day,
            5,
            svg_avatar(paths, "b_parcel", "📦", "#1F7A4D"),
        ),
        chat(
            "c_home",
            "ครอบครัวสุขใจ 🏡",
            "group",
            "เขา: สติกเกอร์",
            today - 19.0 * day,
            2,
            svg_avatar(paths, "c_home", "🏡", "#B9D98A"),
        ),
        chat(
            "b_news",
            "LAAB Official",
            "bot",
            "เขา: อัปเดตใหม่! รองรับโฟลเดอร์แชทแล้ว",
            today - 23.0 * day,
            0,
            svg_avatar(paths, "b_news", "🥗", "#06C755"),
        ),
    ];
    store.upsert_chats(&chats)?;
    let mut users: Vec<(String, String, Option<String>, Option<String>)> = chats
        .iter()
        .filter(|c| c.id.starts_with('u'))
        .map(|c| (c.id.clone(), c.name.clone(), None, Some("dm".into())))
        .collect();
    users.extend(
        extra_team_members()
            .into_iter()
            .map(|(m, n)| (m.to_string(), n.to_string(), None, None)),
    );
    users.push(("me".into(), "Nara".into(), None, None));
    store.upsert_users(&users)?;

    let msg = |id: &str, chat: &str, from: &str, text: &str, time: f64| Message {
        id: id.into(),
        chat_id: chat.into(),
        text: text.into(),
        content_type: "NONE".into(),
        created_time: time,
        mine: from == "me",
        sender_id: Some(from.into()),
        ..Default::default()
    };
    let meta = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>()
    };
    let t = today + 12.0 * 60.0 * minute;
    let photo = whiteboard(paths);
    let mut team = vec![
        msg(
            "m1",
            "c_harvest",
            "me",
            "สรุป scope sprint นี้ไว้ในโน้ตกลุ่มแล้วนะครับ",
            t - day + 5.0 * 60.0 * minute,
        ),
        msg(
            "m2",
            "c_harvest",
            "u_arm",
            "รับทราบครับ คืนนี้ไล่ดูให้",
            t - day + 6.0 * 60.0 * minute,
        ),
        Message {
            content_type: "CHATEVENT".into(),
            text: String::new(),
            metadata: meta(&[
                ("LOC_KEY", "C_MI"),
                ("LOC_ARGS", "me\u{1e}u_non\u{1e}u_khim"),
            ]),
            ..msg("e1", "c_harvest", "me", "", t + 30.0 * minute)
        },
        msg(
            "m3",
            "c_harvest",
            "u_kanya",
            "หน้าไหนต้องเสร็จก่อนคะ",
            t + 45.0 * minute,
        ),
        msg(
            "m4",
            "c_harvest",
            "me",
            "หน้า checkout ก่อนครับ แนบรูปจากไวท์บอร์ดเมื่อเช้าให้",
            t + 46.0 * minute,
        ),
        Message {
            content_type: "IMAGE".into(),
            image_path: photo,
            ..msg("m5", "c_harvest", "me", "", t + 46.0 * minute)
        },
        msg(
            "m6",
            "c_harvest",
            "u_arm",
            "สเปก API อยู่ที่นี่ครับ https://docs.example.com/harvest-api",
            t + 50.0 * minute,
        ),
        msg(
            "m7",
            "c_harvest",
            "u_arm",
            "ฝั่ง payment ใช้ตามสเปกเดิมได้เลยครับ",
            t + 51.0 * minute,
        ),
        Message {
            unsent: 1,
            ..msg("m7b", "c_harvest", "u_kanya", "", t + 55.0 * minute)
        },
        msg(
            "m8",
            "c_harvest",
            "u_kanya",
            "ปรับสีปุ่มให้ตรง guideline แล้ว บ่ายนี้ส่งให้รีวิวค่ะ",
            t + 61.0 * minute,
        ),
        Message {
            content_type: "CALL".into(),
            metadata: meta(&[
                ("TYPE", "G"),
                ("GC_EVT_TYPE", "S"),
                ("GC_MEDIA_TYPE", "AUDIO"),
                ("RESULT", "INFO"),
            ]),
            ..msg("k1", "c_harvest", "u_arm", "", t + 62.0 * minute)
        },
        Message {
            content_type: "CALL".into(),
            metadata: meta(&[
                ("TYPE", "G"),
                ("GC_EVT_TYPE", "E"),
                ("GC_MEDIA_TYPE", "AUDIO"),
                ("RESULT", "INFO"),
                ("DURATION", "120000"),
            ]),
            duration_ms: Some(120_000),
            ..msg("k2", "c_harvest", "u_arm", "", t + 64.0 * minute)
        },
        Message {
            reactions: vec![
                Reaction {
                    user_id: "u_arm".into(),
                    reaction: "NICE".into(),
                },
                Reaction {
                    user_id: "u_kanya".into(),
                    reaction: "LOVE".into(),
                },
                Reaction {
                    user_id: "u_lead".into(),
                    reaction: "NICE".into(),
                },
            ],
            ..msg("m9", "c_harvest", "me", "ขอบคุณครับ 🙏", t + 63.0 * minute)
        },
        msg(
            "m10",
            "c_harvest",
            "u_lead",
            "เยี่ยมเลย พรุ่งนี้บ่ายสองเดโมให้ลูกค้าดูได้ไหมคะ",
            t + 68.0 * minute,
        ),
        Message {
            mentions: vec![MentionSpan {
                start: 0,
                end: 4,
                mid: Some("u_arm".into()),
                all: false,
            }],
            ..msg(
                "m11",
                "c_harvest",
                "me",
                "@Arm ช่วยเช็ก endpoint ชำระเงินอีกรอบนะครับ",
                t + 70.0 * minute,
            )
        },
    ];
    for m in &mut team {
        m.raw = Some(
            serde_json::to_string_pretty(&serde_json::json!({"demo": true, "id": m.id})).unwrap(),
        );
    }
    store.upsert_messages(&team)?;
    for m in &team {
        if !m.reactions.is_empty() {
            store.replace_reactions(&m.id, &m.reactions)?;
        }
    }
    for (user, id) in [("u_arm", "m11"), ("u_kanya", "m11"), ("u_lead", "m5")] {
        store.set_read("c_harvest", user, id)?;
    }
    for (mid, _) in extra_team_members() {
        store.set_read("c_harvest", mid, "m11")?;
    }
    let mut members: Vec<ChatMember> = ["u_arm", "u_kanya", "u_lead"]
        .iter()
        .map(|m| ChatMember {
            mid: m.to_string(),
            name: String::new(),
            all: false,
            picture: None,
        })
        .collect();
    members.extend(extra_team_members().into_iter().map(|(m, n)| ChatMember {
        mid: m.into(),
        name: n.into(),
        all: false,
        picture: None,
    }));
    store.upsert_members("c_harvest", &members)?;

    let lead = today + 10.0 * 60.0 * minute;
    store.upsert_messages(&[
        msg(
            "b1",
            "u_lead",
            "u_lead",
            "ขอไฟล์สรุปงบประมาณไตรมาสนี้ด้วยนะคะ",
            lead + 50.0 * minute,
        ),
        msg(
            "b2",
            "u_lead",
            "me",
            "ส่งไฟล์งบประมาณให้แล้วครับ",
            lead + 54.0 * minute,
        ),
    ])?;
    store.set_read("u_lead", "u_lead", "b2")?;

    store.upsert_messages(&[
        Message {
            content_type: "CALL".into(),
            metadata: meta(&[("TYPE", "A"), ("RESULT", "NORMAL"), ("DURATION", "185000")]),
            duration_ms: Some(185_000),
            ..msg("p1", "u_arm", "u_arm", "", today - 2.0 * day)
        },
        Message {
            content_type: "CALL".into(),
            metadata: meta(&[("TYPE", "V"), ("RESULT", "NO_RESPONSE")]),
            ..msg(
                "p0",
                "u_arm",
                "u_arm",
                "",
                today - 2.0 * day - 60.0 * minute,
            )
        },
    ])?;

    let flex = serde_json::json!({
        "type": "bubble",
        "hero": {"type": "box", "layout": "vertical", "backgroundColor": "#06C755", "paddingAll": "xl",
            "contents": [{"type": "text", "text": "LAAB 0.1", "color": "#FFFFFF", "size": "xxl", "weight": "bold"}]},
        "body": {"type": "box", "layout": "vertical", "spacing": "md", "contents": [
            {"type": "text", "text": "อัปเดตใหม่! รองรับโฟลเดอร์แชทแล้ว", "weight": "bold", "size": "lg", "wrap": true},
            {"type": "text", "text": "จัดแชทงานกับแชทส่วนตัวแยกกันได้ แชทเดียวอยู่ได้หลายโฟลเดอร์", "size": "sm", "color": "#8A8C90", "wrap": true},
            {"type": "box", "layout": "horizontal", "contents": [
                {"type": "text", "text": "เวอร์ชัน", "size": "sm", "color": "#8A8C90", "flex": 0},
                {"type": "text", "text": "0.1.0", "size": "sm", "align": "end"}
            ]}
        ]},
        "footer": {"type": "box", "layout": "vertical", "spacing": "sm", "contents": [
            {"type": "button", "style": "primary", "action": {"type": "uri", "label": "ดูรายละเอียด", "uri": "https://github.com/softnetics/laab"}},
            {"type": "button", "style": "secondary", "action": {"type": "message", "label": "ขอบคุณ", "text": "ขอบคุณครับ"}}
        ]}
    });
    store.upsert_messages(&[Message {
        content_type: "FLEX".into(),
        metadata: meta(&[
            ("ALT_TEXT", "อัปเดตใหม่! รองรับโฟลเดอร์แชทแล้ว"),
            ("FLEX_JSON", &flex.to_string()),
        ]),
        ..msg("n1", "b_news", "b_news", "", today - 23.0 * day)
    }])?;

    store.set_pinned(true, "c_harvest", "main")?;
    store.set_pinned(true, "c_run", "main")?;
    let work = store.add_workspace("Work")?;
    for id in ["c_harvest", "u_lead", "u_arm", "u_kanya"] {
        store.set_pinned(true, id, &work.id)?;
    }
    Ok(())
}

/// Local midnight in milliseconds.
fn chrono_like_today() -> f64 {
    let now = crate::state::now_ms();
    let offset = local_offset_ms();
    let day = 86_400_000.0;
    ((now + offset) / day).floor() * day - offset
}

fn local_offset_ms() -> f64 {
    // The demo only needs "today" to look right; read the offset from `date`.
    std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            let s = s.trim();
            let sign = if s.starts_with('-') { -1.0 } else { 1.0 };
            let digits = s.trim_start_matches(['+', '-']);
            let h: f64 = digits.get(0..2)?.parse().ok()?;
            let m: f64 = digits.get(2..4)?.parse().ok()?;
            Some(sign * (h * 3_600_000.0 + m * 60_000.0))
        })
        .unwrap_or(0.0)
}
