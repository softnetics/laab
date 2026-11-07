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

/// Products asked for per page of `getOwnedProductSummaries`; pages go on
/// until the list's `totalSize` (`ProductSummaryList`) is reached. The
/// app's page size is unverified.
const PAGE: usize = 100;
/// Pages at most, so a list that never says its size can't loop forever.
const MAX_PAGES: usize = 50;

/// An owned sticker package: id, latest version, name.
#[derive(Debug, Clone, PartialEq)]
struct Owned {
    id: String,
    version: String,
    name: String,
}

/// One `ProductSummaryList` page: its products and `totalSize`.
fn summaries(res: &Json) -> (Vec<Owned>, Option<usize>) {
    let products = res["productList"].as_array().or_else(|| res["1"].as_array());
    let owned = products
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let id = str_of(&p["id"]).or_else(|| str_of(&p["1"]))?;
            let version = str_of(&p["latestVersion"])
                .or_else(|| str_of(&p["21"]))
                .unwrap_or_else(|| "1".into());
            let name = str_of(&p["name"])
                .or_else(|| str_of(&p["11"]))
                .unwrap_or_else(|| id.clone());
            Some(Owned { id, version, name })
        })
        .collect();
    let total = res["totalSize"]
        .as_u64()
        .or_else(|| res["3"].as_u64())
        .map(|n| n as usize);
    (owned, total)
}

/// Whether another page may hold more after `have` products, the last
/// page having brought `got`.
fn more(have: usize, got: usize, total: Option<usize>) -> bool {
    match total {
        Some(total) => got > 0 && have < total,
        None => got >= PAGE,
    }
}

/// Every sticker package the account owns, page by page.
async fn owned(line: &crate::line::Line) -> Result<Vec<Owned>> {
    let mut all: Vec<Owned> = Vec::new();
    for _ in 0..MAX_PAGES {
        let res = line
            .client
            .base
            .shop()
            .get_owned_product_summaries(tv!({
                "shopId": "stickershop", "offset": all.len() as i64, "limit": PAGE as i64,
                "locale": {"language": "en", "country": "TH"}
            }))
            .await
            .map_err(|e| anyhow!("{e}"))?
            .to_json();
        let (page, total) = summaries(&res);
        let got = page.len();
        for p in page {
            if !all.iter().any(|o| o.id == p.id) {
                all.push(p);
            }
        }
        if !more(all.len(), got, total) {
            break;
        }
    }
    Ok(all)
}

/// A package's stickers, from its `productInfo.meta` on the sticker CDN.
pub async fn pack(id: &str, name: &str, version: &str) -> Result<StickerPack> {
    let url = format!(
        "https://stickershop.line-scdn.net/stickershop/v1/product/{id}/android/productInfo.meta"
    );
    let meta: Json = reqwest::get(&url).await?.error_for_status()?.json().await?;
    pack_from_meta(id, name, version, &meta).ok_or_else(|| anyhow!("no stickers in {id}"))
}

fn pack_from_meta(id: &str, fallback_name: &str, version: &str, meta: &Json) -> Option<StickerPack> {
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

/// The recent stickers, then every owned package without its stickers:
/// the picker loads a package's stickers with [`pack`] when it is opened.
pub async fn list(state: &Arc<AppState>) -> Result<Vec<StickerPack>> {
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    let owned = owned(&line).await?;
    let mut packs = Vec::new();
    let recent: Vec<Sticker> = recent(state)
        .into_iter()
        .filter(|s| owned.iter().any(|o| o.id == s.package_id))
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
    packs.extend(owned.into_iter().map(|o| StickerPack {
        id: o.id,
        name: o.name,
        version: Some(o.version),
        recent: false,
        stickers: Vec::new(),
    }));
    Ok(packs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_a_page_of_owned_packages() {
        let (owned, total) = summaries(&json!({
            "productList": [
                {"id": "1", "name": "Cats", "latestVersion": 7},
                {"1": "2", "11": "Dogs", "21": 3},
                {"name": "no id"},
            ],
            "offset": 0,
            "totalSize": 140,
        }));
        assert_eq!(total, Some(140));
        assert_eq!(
            owned,
            [
                Owned { id: "1".into(), version: "7".into(), name: "Cats".into() },
                Owned { id: "2".into(), version: "3".into(), name: "Dogs".into() },
            ]
        );
    }

    #[test]
    fn pages_until_the_total_is_reached() {
        assert!(more(100, 100, Some(140)));
        assert!(!more(140, 40, Some(140)));
        // A page that brings nothing ends it even below the total.
        assert!(!more(100, 0, Some(140)));
        // Without a total, a full page means there may be more.
        assert!(more(100, 100, None));
        assert!(!more(130, 30, None));
    }

    #[test]
    fn a_pack_lists_every_sticker() {
        let stickers: Vec<Json> = (0..60).map(|i| json!({"id": 1000 + i})).collect();
        let meta = json!({"title": {"en": "Cats", "ja": "猫"}, "version": 5, "stickers": stickers});
        let p = pack_from_meta("9", "fallback", "1", &meta).unwrap();
        assert_eq!(p.name, "Cats");
        assert_eq!(p.version.as_deref(), Some("5"));
        assert_eq!(p.stickers.len(), 60);
        assert_eq!(p.stickers[59].sticker_id, "1059");
        assert!(pack_from_meta("9", "x", "1", &json!({"stickers": []})).is_none());
    }
}
