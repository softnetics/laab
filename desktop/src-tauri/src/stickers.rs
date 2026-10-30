//! Owned sticker packs and the recent-sticker list.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use line_rs::tv;
use serde_json::Value as Json;

use crate::convert::str_of;
use crate::model::{Sticker, StickerPack};
use crate::state::AppState;

const RECENT_KEY: &str = "recent_stickers";
const RECENT_LIMIT: usize = 24;

pub fn recent(state: &AppState) -> Vec<Sticker> {
    state
        .store()
        .get_kv(RECENT_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn remember(state: &AppState, sticker: &Sticker) {
    let mut list = recent(state);
    list.retain(|s| s.sticker_id != sticker.sticker_id);
    list.insert(0, sticker.clone());
    list.truncate(RECENT_LIMIT);
    if let Ok(json) = serde_json::to_string(&list) {
        let _ = state.store().set_kv(RECENT_KEY, &json);
    }
}

async fn pack_meta(id: &str, fallback_name: &str, version: &str) -> Option<StickerPack> {
    let url = format!(
        "https://stickershop.line-scdn.net/stickershop/v1/product/{id}/android/productInfo.meta"
    );
    let meta: Json = reqwest::get(&url).await.ok()?.json().await.ok()?;
    let name = match &meta["title"] {
        Json::String(s) if !s.trim().is_empty() => s.trim().to_string(),
        Json::Object(t) => ["th", "en", "ja"]
            .iter()
            .find_map(|k| t.get(*k).and_then(Json::as_str).filter(|s| !s.is_empty()))
            .unwrap_or(fallback_name)
            .to_string(),
        _ => fallback_name.to_string(),
    };
    let version = str_of(&meta["version"]).unwrap_or_else(|| version.to_string());
    let stickers: Vec<Sticker> = meta["stickers"]
        .as_array()
        .into_iter()
        .flatten()
        .take(48)
        .filter_map(|s| {
            Some(Sticker {
                sticker_id: str_of(&s["id"])?,
                package_id: id.to_string(),
                version: Some(version.clone()),
            })
        })
        .collect();
    if stickers.is_empty() {
        return None;
    }
    Some(StickerPack {
        id: id.to_string(),
        name,
        version: Some(version),
        recent: false,
        stickers,
    })
}

pub async fn list(state: &Arc<AppState>) -> Result<Vec<StickerPack>> {
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    let res = line
        .client
        .base
        .shop()
        .get_owned_product_summaries(tv!({
            "shopId": "stickershop", "offset": 0, "limit": 80, "locale": {"language": "en", "country": "TH"}
        }))
        .await
        .map_err(|e| anyhow!("{e}"))?
        .to_json();
    let products = res["productList"]
        .as_array()
        .or_else(|| res["1"].as_array())
        .cloned()
        .unwrap_or_default();
    let owned: Vec<(String, String, String)> = products
        .iter()
        .filter_map(|p| {
            let id = str_of(&p["id"]).or_else(|| str_of(&p["1"]))?;
            let version = str_of(&p["latestVersion"])
                .or_else(|| str_of(&p["21"]))
                .unwrap_or_else(|| "1".into());
            let name = str_of(&p["name"])
                .or_else(|| str_of(&p["11"]))
                .unwrap_or_else(|| id.clone());
            Some((id, version, name))
        })
        .collect();
    let mut packs = Vec::new();
    let recent: Vec<Sticker> = recent(state)
        .into_iter()
        .filter(|s| owned.iter().any(|(id, ..)| *id == s.package_id))
        .collect();
    if !recent.is_empty() {
        packs.push(StickerPack {
            id: "__recent__".into(),
            name: "ล่าสุด".into(),
            version: None,
            recent: true,
            stickers: recent,
        });
    }
    for (id, version, name) in owned.into_iter().take(12) {
        if let Some(p) = pack_meta(&id, &name, &version).await {
            packs.push(p);
        }
    }
    Ok(packs)
}
