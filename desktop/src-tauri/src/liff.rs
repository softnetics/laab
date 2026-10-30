//! LIFF apps: open them the way the LINE app does, with tokens in the
//! URL fragment, a LINE user agent, a minimal `window._liff` bridge and the
//! share-target picker.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use line_rs::client::liff::LiffViewOptions;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::state::AppState;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiffLink {
    pub id: String,
    pub path: String,
    pub query: String,
}

fn valid_id(id: &str) -> bool {
    let Some((a, b)) = id.split_once('-') else {
        return false;
    };
    a.len() >= 6
        && a.bytes().all(|c| c.is_ascii_digit())
        && b.len() >= 6
        && b.bytes().all(|c| c.is_ascii_alphanumeric())
}

/// `https://liff.line.me/{id}/path?q`, `https://miniapp.line.me/{id}`,
/// `https://line.me/R/app/{id}` and `line://app/{id}`.
pub fn parse_link(url: &str) -> Option<LiffLink> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let (rest, _fragment) = rest.split_once('#').unwrap_or((rest, ""));
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let (id, extra) = match (scheme.as_str(), host.as_str()) {
        ("https" | "http", "liff.line.me" | "miniapp.line.me") => {
            (segments.first()?, &segments[1.min(segments.len())..])
        }
        ("https" | "http", "line.me")
            if segments.len() >= 3 && segments[0] == "R" && segments[1] == "app" =>
        {
            (&segments[2], &segments[3..])
        }
        ("line", "app") => (segments.first()?, &segments[1.min(segments.len())..]),
        _ => return None,
    };
    valid_id(id).then(|| LiffLink {
        id: id.to_string(),
        path: extra.join("/"),
        query: query.to_string(),
    })
}

impl LiffLink {
    pub fn web_url(&self) -> String {
        let mut url = format!("https://liff.line.me/{}", self.id);
        if !self.path.is_empty() {
            url.push('/');
            url.push_str(&self.path);
        }
        if !self.query.is_empty() {
            url.push('?');
            url.push_str(&self.query);
        }
        url
    }
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'&' | b'=' | b'+' | b'#' | b'%' | b' ' => format!("%{b:02X}"),
            _ => (b as char).to_string(),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiffView {
    pub liff_id: String,
    pub url: String,
    /// `client` (tokens issued) or `web` (plain page).
    pub mode: String,
    pub reason: Option<String>,
}

pub fn client_url(endpoint: &str, link: &LiffLink, tokens: &[(&str, String)]) -> Result<String> {
    if !endpoint.starts_with("https://") {
        return Err(anyhow!("LIFF endpoint is not https"));
    }
    let endpoint = endpoint.split('#').next().unwrap_or(endpoint);
    // The extra path goes before the endpoint's own query
    // (`https://app/liff?tenant=x` + `pages/a` is `https://app/liff/pages/a?tenant=x`).
    let (mut url, endpoint_query) = match endpoint.split_once('?') {
        Some((base, query)) => (base.to_string(), query),
        None => (endpoint.to_string(), ""),
    };
    if !link.path.is_empty() {
        if !url.ends_with('/') {
            url.push('/');
        }
        url.push_str(&link.path);
    }
    // Pairs the link repeats from the endpoint are kept once.
    let mut pairs: Vec<&str> = Vec::new();
    for pair in endpoint_query.split('&').chain(link.query.split('&')) {
        if !pair.is_empty() && !pairs.contains(&pair) {
            pairs.push(pair);
        }
    }
    if !pairs.is_empty() {
        url.push('?');
        url.push_str(&pairs.join("&"));
    }
    let fragment: Vec<String> = tokens
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}={}", encode(v)))
        .collect();
    Ok(format!("{url}#{}", fragment.join("&")))
}

fn consent_required(e: &line_rs::Error) -> bool {
    if let line_rs::Error::Request { data, .. } = e {
        let json = data.to_json();
        let exc = if json.get("liffException").is_some() {
            &json["liffException"]
        } else {
            &json
        };
        return exc["code"].as_str() == Some("CONSENT_REQUIRED") || exc["code"].as_i64() == Some(3);
    }
    format!("{e:?}").contains("CONSENT_REQUIRED")
}

/// Issue a LIFF view for `url`. Errors with `liff_consent_required` until the
/// user agrees; it never consents silently.
pub async fn issue(
    state: &Arc<AppState>,
    url: &str,
    chat: Option<String>,
    consent: bool,
) -> Result<LiffView> {
    let link = parse_link(url).ok_or_else(|| anyhow!("invalid_liff_id"))?;
    let web = |reason: &str| LiffView {
        liff_id: link.id.clone(),
        url: link.web_url(),
        mode: "web".into(),
        reason: Some(reason.into()),
    };
    let Some(line) = state.line().await else {
        return Ok(web("ยังไม่ได้เข้าสู่ระบบ"));
    };
    let opts = LiffViewOptions {
        chat_mid: chat,
        liff_id: Some(link.id.clone()),
        lang: Some("th_TH".into()),
    };
    let liff = line.client.liff();
    let view = match liff.issue_view(&opts).await {
        Ok(v) => v,
        Err(e) if consent_required(&e) => {
            if !consent {
                return Err(anyhow!("liff_consent_required"));
            }
            liff.get_token(&opts).await.map_err(|e| anyhow!("{e}"))?;
            liff.issue_view(&opts).await.map_err(|e| anyhow!("{e}"))?
        }
        Err(e) => return Err(anyhow!("{e}")),
    };
    let v = view.to_json();
    let endpoint = v["view"]["url"].as_str().unwrap_or("");
    let token = v["accessToken"].as_str().unwrap_or("");
    if endpoint.is_empty() || token.is_empty() {
        return Ok(web("LINE ไม่ส่ง endpoint ของ LIFF กลับมา"));
    }
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    let channel = link.id.split('-').next().unwrap_or("").to_string();
    let url = client_url(
        endpoint,
        &link,
        &[
            ("context_token", s("contextToken")),
            ("feature_token", s("featureToken")),
            ("access_token", token.to_string()),
            ("id_token", s("idToken")),
            ("client_id", channel),
        ],
    )?;
    Ok(LiffView {
        liff_id: link.id,
        url,
        mode: "client".into(),
        reason: None,
    })
}

const BRIDGE: &str = r#"(function(){
  if (window._liff) return;
  function reply(type, callbackId, data, error) {
    var detail = error ? {type: type, callbackId: callbackId, error: error} : {type: type, callbackId: callbackId, data: data || {}};
    window.dispatchEvent(new CustomEvent('liffEvent', {detail: detail}));
  }
  window._liff = {
    // LIFF SDK 2.x in a LINE app 9.5.0 or newer waits for this list, or for
    // a "ready" event, before liff.init() goes on.
    features: ['closeWindow', 'openWindow'],
    postMessage: function(type, featureToken, callbackId, params) {
      try { params = typeof params === 'string' ? JSON.parse(params) : (params || {}); } catch (e) { params = {}; }
      if (type === 'closeWindow') { location.href = 'liff://close'; return; }
      if (type === 'openWindow') {
        reply(type, callbackId, {});
        location.href = 'laab-open://open?url=' + encodeURIComponent(params.url || '');
        return;
      }
      reply(type, callbackId, null, {code: 'FORBIDDEN', message: 'LAAB ยังไม่รองรับ ' + type});
    }
  };
  reply('ready');
})();"#;

pub fn user_agent(client: bool) -> String {
    if client {
        "Mozilla/5.0 (Linux; Android 14) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile Safari/605.1.15 Line/26.6.2 LIFF".into()
    } else {
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15"
            .into()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PickerRequest {
    window: String,
    liff_id: String,
    ott: String,
    data: Json,
}

/// Open a LIFF view in its own window.
pub fn open_window(state: &Arc<AppState>, view: &LiffView, chat: Option<String>) -> Result<()> {
    let app = state.app.clone();
    let label = format!("liff-{}", crate::state::now_ms() as u64);
    let url: tauri::Url = view.url.parse()?;
    let client = view.mode == "client";
    let current = view.liff_id.clone();
    let (nav_app, nav_label, nav_state) = (app.clone(), label.clone(), state.clone());
    let mut builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(url))
        .title("LIFF")
        .inner_size(420.0, 760.0)
        .min_inner_size(320.0, 420.0)
        .user_agent(&user_agent(client))
        .on_navigation(move |u| {
            let s = u.as_str();
            match u.scheme() {
                "liff" if u.host_str() == Some("close") => {
                    close(&nav_app, &nav_label);
                    false
                }
                // The SDK reports the running app this way after init; reloading
                // on it would lose the login.
                "liff" => false,
                "laab-open" => {
                    let target = u
                        .query_pairs()
                        .find(|(k, _)| k == "url")
                        .map(|(_, v)| v.to_string())
                        .unwrap_or_default();
                    open_external_or_liff(&nav_state, &target, chat.clone());
                    false
                }
                "line" if u.host_str() == Some("picker") => {
                    let q: std::collections::HashMap<String, String> =
                        u.query_pairs().map(|(k, v)| (k.into(), v.into())).collect();
                    if client && q.get("liffId").map(String::as_str) == Some(current.as_str()) {
                        let data = q
                            .get("data")
                            .and_then(|d| serde_json::from_str(d).ok())
                            .unwrap_or(Json::Null);
                        let _ = nav_app.emit_to(
                            "main",
                            "liff-picker",
                            PickerRequest {
                                window: nav_label.clone(),
                                liff_id: current.clone(),
                                ott: q.get("ott").cloned().unwrap_or_default(),
                                data,
                            },
                        );
                    }
                    false
                }
                "line" => {
                    if parse_link(s).is_some() {
                        open_external_or_liff(&nav_state, s, chat.clone());
                    }
                    false
                }
                "http" | "https" => {
                    // A different LIFF app gets fresh tokens.
                    match parse_link(s) {
                        Some(link) if link.id != current && client => {
                            open_external_or_liff(&nav_state, s, chat.clone());
                            false
                        }
                        _ => true,
                    }
                }
                "about" | "data" | "blob" => true,
                _ => {
                    let _ = tauri_plugin_opener::open_url(s, None::<&str>);
                    false
                }
            }
        });
    if client {
        builder = builder.initialization_script(BRIDGE);
    }
    builder.build()?;
    Ok(())
}

fn close(app: &tauri::AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.close();
    }
}

/// LIFF links open as LIFF windows; anything else goes to the browser.
pub fn open_external_or_liff(state: &Arc<AppState>, url: &str, chat: Option<String>) {
    if parse_link(url).is_some() {
        let s = state.clone();
        let url = url.to_string();
        tauri::async_runtime::spawn(async move {
            let view = match issue(&s, &url, chat.clone(), false).await {
                Ok(v) => v,
                Err(e) => {
                    let _ = s.app.emit_to(
                        "main",
                        "liff-consent",
                        serde_json::json!({"url": url, "chatId": chat, "error": e.to_string()}),
                    );
                    return;
                }
            };
            if let Err(e) = open_window(&s, &view, chat) {
                s.status(format!("เปิด LIFF ไม่สำเร็จ: {e}"));
            }
        });
    } else if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.starts_with("tel:")
    {
        let _ = tauri_plugin_opener::open_url(url, None::<&str>);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareResult {
    pub sent: usize,
    pub failed: Vec<String>,
}

/// shareTargetPicker: post the messages into each chosen chat.
pub async fn share(
    state: &Arc<AppState>,
    liff_id: &str,
    chats: &[String],
    messages: &Json,
) -> Result<ShareResult> {
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    let http = reqwest::Client::new();
    let mut result = ShareResult {
        sent: 0,
        failed: vec![],
    };
    for chat in chats {
        let opts = LiffViewOptions {
            chat_mid: Some(chat.clone()),
            liff_id: Some(liff_id.into()),
            lang: Some("th_TH".into()),
        };
        let ok = async {
            let v = line
                .client
                .liff()
                .issue_view(&opts)
                .await
                .map_err(|e| anyhow!("{e}"))?
                .to_json();
            let token = v["accessToken"]
                .as_str()
                .ok_or_else(|| anyhow!("no token"))?
                .to_string();
            let res = http
                .post("https://api.line.me/message/v3/share")
                .bearer_auth(token)
                .json(&serde_json::json!({ "messages": messages }))
                .send()
                .await?;
            if res.status().is_success() {
                Ok(())
            } else {
                Err(anyhow!("{}", res.status()))
            }
        }
        .await;
        match ok {
            Ok(()) => result.sent += 1,
            Err(e) => result.failed.push(format!("{chat}: {e}")),
        }
    }
    Ok(result)
}

/// Tell the LIFF SDK how the picker ended. Inferred from the SDK source; not
/// checked against a real account.
pub async fn picker_result(liff_id: &str, ott: &str, success: bool) -> Result<()> {
    let url = format!(
        "https://access.line.me/oauth2/v2.1/liff/shareTargetPicker/result?liffId={}&ott={}",
        encode(liff_id),
        encode(ott)
    );
    reqwest::Client::new()
        .post(url)
        .form(&[("result", if success { "SUCCESS" } else { "CANCEL" })])
        .send()
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_liff_links() {
        let l = parse_link("https://liff.line.me/1234567-AbCdEf12/shop/item?x=1").unwrap();
        assert_eq!(
            (l.id.as_str(), l.path.as_str(), l.query.as_str()),
            ("1234567-AbCdEf12", "shop/item", "x=1")
        );
        assert_eq!(
            l.web_url(),
            "https://liff.line.me/1234567-AbCdEf12/shop/item?x=1"
        );
        assert!(parse_link("line://app/1234567-AbCdEf12").is_some());
        assert!(parse_link("https://line.me/R/app/1234567-AbCdEf12").is_some());
        assert!(parse_link("https://liff.line.me/nope").is_none());
        assert!(parse_link("https://example.com/1234567-AbCdEf12").is_none());
    }

    #[test]
    fn builds_the_client_url() {
        let l = parse_link("https://liff.line.me/1234567-AbCdEf12/a?b=1").unwrap();
        let u = client_url(
            "https://app.example/liff",
            &l,
            &[("access_token", "x=y".into()), ("id_token", String::new())],
        )
        .unwrap();
        assert_eq!(u, "https://app.example/liff/a?b=1#access_token=x%3Dy");
    }

    #[test]
    fn puts_the_path_before_the_endpoint_query() {
        let l =
            parse_link("https://liff.line.me/1234567-AbCdEf12/pages/p1?tenant=demo&x=1").unwrap();
        let u = client_url(
            "https://app.example/liff?tenant=demo",
            &l,
            &[("access_token", "t".into())],
        )
        .unwrap();
        assert_eq!(
            u,
            "https://app.example/liff/pages/p1?tenant=demo&x=1#access_token=t"
        );
    }
}
