//! Link previews, as LINE's app shows them under a message with a URL.
//!
//! LINE's own preview service (`/PAGEPOKER`, through line-rs's
//! `Home::page_preview`) fetches the page; the site never sees this Mac.
//! Answers are kept in the database per URL, as the app keeps them in its
//! `url_preview.db` (`vs6/o`): a preview for a day, a refusal for a day,
//! a server error for 10 minutes (`ws6/d.java`: 500 → 600000 ms). The
//! app's TTL comes from server config; a day is unverified.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::state::{AppState, now_ms};

const DAY_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;
/// How long a server error is remembered (`ws6/d.java`: 500).
const SERVER_ERROR_MS: f64 = 10.0 * 60.0 * 1000.0;
/// How long a timeout (408) is remembered (`ws6/d.java`: 10 s).
const TIMEOUT_MS: f64 = 10.0 * 1000.0;

/// What the message shows under its text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPreview {
    pub url: String,
    pub title: String,
    pub summary: String,
    pub domain: String,
    /// LINE's copy of the page's picture; never the site's own.
    pub image: Option<String>,
    /// LINE refused the page: `phishing` (452) or `blocked` (451).
    pub warning: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Cached {
    until: f64,
    preview: Option<LinkPreview>,
}

fn key(url: &str) -> String {
    format!("link_preview:{url}")
}

/// Whether `url` is on LINE's CDN, so showing it contacts only LINE.
fn line_hosted(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some_and(|h| {
                ["line-scdn.net", "line-apps.com", "line.me"]
                    .iter()
                    .any(|d| h == *d || h.ends_with(&format!(".{d}")))
            })
    })
}

/// The card for a preview LINE gave, or `None` when there is nothing to show.
fn card(url: &str, p: line_rs::rest::home::PagePreview) -> Option<LinkPreview> {
    if p.title.is_empty() && p.summary.is_empty() {
        return None;
    }
    Some(LinkPreview {
        url: if p.url.is_empty() { url.to_string() } else { p.url },
        title: p.title,
        summary: p.summary,
        domain: p.domain,
        image: p.image.filter(|i| line_hosted(i)),
        warning: None,
    })
}

/// What a refusal shows and how long it is kept, by LINE's code.
fn refused(url: &str, code: Option<u16>) -> (Option<LinkPreview>, f64) {
    let warn = |w: &str| LinkPreview {
        url: url.to_string(),
        warning: Some(w.into()),
        ..Default::default()
    };
    match code {
        Some(452) => (Some(warn("phishing")), DAY_MS),
        Some(451) => (Some(warn("blocked")), DAY_MS),
        Some(408) => (None, TIMEOUT_MS),
        Some(500..=599) | None => (None, SERVER_ERROR_MS),
        Some(_) => (None, DAY_MS),
    }
}

fn is_refusal(e: &line_rs::Error) -> bool {
    matches!(e, line_rs::Error::Internal { kind, .. } if kind == "PagePokerError")
}

/// The preview of `url`, from the database or LINE's preview service.
pub async fn get(state: &Arc<AppState>, url: &str) -> Option<LinkPreview> {
    if state.demo || !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    let now = now_ms();
    if let Some(c) = state
        .store()
        .get_kv(&key(url))
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Cached>(&s).ok())
        && c.until > now
    {
        return c.preview;
    }
    let line = state.line().await?;
    let query = line_rs::rest::home::PageInfoQuery {
        url: url.to_string(),
        lang: Some(crate::liff::lang()),
        phishing_check: Some(true),
        ..Default::default()
    };
    let (preview, keep) = match line.client.base.rest().home().page_preview(query).await {
        Ok(p) => (card(url, p), DAY_MS),
        Err(e @ line_rs::Error::Internal { .. }) if is_refusal(&e) => refused(url, e.http_status()),
        // A network failure: try again next time.
        Err(e) => {
            tracing::debug!("link preview: {e}");
            return None;
        }
    };
    let cached = Cached { until: now + keep, preview: preview.clone() };
    if let Ok(json) = serde_json::to_string(&cached) {
        let _ = state.store().set_kv(&key(url), &json);
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;
    use line_rs::rest::home::PagePreview;

    #[test]
    fn shows_only_pictures_line_keeps() {
        assert!(line_hosted("https://obs.line-scdn.net/0hABC"));
        assert!(line_hosted("https://scdn.line-apps.com/x.png"));
        assert!(!line_hosted("https://example.com/og.png"));
        assert!(!line_hosted("https://evil-line-scdn.net/x"));
        assert!(!line_hosted("http://obs.line-scdn.net/x"));
        let p = card(
            "https://example.com",
            PagePreview {
                title: "T".into(),
                image: Some("https://example.com/og.png".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.image, None);
        assert_eq!(p.url, "https://example.com");
        assert!(card("https://example.com", PagePreview::default()).is_none());
    }

    #[test]
    fn refusals_warn_or_wait() {
        let (p, keep) = refused("u", Some(452));
        assert_eq!(p.unwrap().warning.as_deref(), Some("phishing"));
        assert_eq!(keep, DAY_MS);
        assert_eq!(refused("u", Some(451)).0.unwrap().warning.as_deref(), Some("blocked"));
        assert_eq!(refused("u", Some(500)), (None, SERVER_ERROR_MS));
        assert_eq!(refused("u", Some(404)), (None, DAY_MS));
    }
}
