//! LIFF apps: open them the way the LINE app does, with tokens in the
//! URL fragment, a LINE user agent, a minimal `window._liff` bridge and the
//! share-target picker.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use line_rs::client::liff::{self as lliff, ConsentPage, LiffDeviceSetting, LiffViewOptions, PickerResult};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri::webview::NewWindowResponse;

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
    /// For LIFF DevTools: the requested link, what LINE returned, the tokens.
    #[serde(skip)]
    pub info: Json,
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

/// The account's language as LINE has it (`preferenceLocale`), once read.
static ACCOUNT_LOCALE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn set_account_locale(locale: Option<String>) {
    *ACCOUNT_LOCALE.lock().unwrap() = locale.and_then(|l| normalize_locale(&l));
}

/// `th_TH` from `th-TH`, `en_TH@rg=thzzzz` or `th`; None for nothing usable.
fn normalize_locale(raw: &str) -> Option<String> {
    let base = raw.split(['@', '.']).next()?.trim().replace('-', "_");
    let mut parts = base.split('_').filter(|p| !p.is_empty());
    let lang = parts.next()?.to_ascii_lowercase();
    if lang.len() < 2 || !lang.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    // A script subtag (zh_Hant_TW) is skipped; the region is the last part.
    let region = parts.rfind(|p| p.len() == 2).map(str::to_ascii_uppercase);
    Some(match region {
        Some(r) => format!("{lang}_{r}"),
        None => lang,
    })
}

/// The Mac's locale (`AppleLocale`), read once.
fn mac_locale() -> Option<String> {
    static MAC: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    MAC.get_or_init(|| {
        let out = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleLocale"])
            .output()
            .ok()?;
        normalize_locale(String::from_utf8_lossy(&out.stdout).trim())
    })
    .clone()
}

/// The language LIFF apps are opened in, as LINE sends it in
/// issueLiffView: the account's, else the Mac's, else Thai.
pub fn lang() -> String {
    ACCOUNT_LOCALE
        .lock()
        .unwrap()
        .clone()
        .or_else(mac_locale)
        .unwrap_or_else(|| "th_TH".into())
}

/// LINE's consent pages fetched for the user to agree to, by LIFF id, so
/// "Allow" posts the very page they saw.
static CONSENTS: std::sync::Mutex<Vec<(String, ConsentPage)>> = std::sync::Mutex::new(Vec::new());

/// What a LIFF app asks the user to agree to, from LINE's consent page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentInfo {
    pub permissions: Vec<lliff::ConsentPermission>,
    /// LINE's consent page, to open in the browser.
    pub consent_url: String,
}

/// Fetch LINE's consent page for `url` and
/// say what it asks for. Nothing is granted here.
pub async fn consent_info(state: &Arc<AppState>, url: &str, chat: Option<String>) -> Result<ConsentInfo> {
    let link = parse_link(url).ok_or_else(|| anyhow!("invalid_liff_id"))?;
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    let liff = line.client.liff();
    let opts = view_options(url, &link, chat);
    let consent = match liff.issue_view(&opts).await {
        Ok(_) => return Ok(ConsentInfo { permissions: vec![], consent_url: String::new() }),
        Err(e) => lliff::consent_required(&e).ok_or_else(|| anyhow!("{e}"))?,
    };
    let page = liff.consent_page(&consent).await.map_err(|e| anyhow!("{e}"))?;
    let info = ConsentInfo {
        permissions: page.permissions.clone(),
        consent_url: consent.consent_url.clone(),
    };
    let mut pages = CONSENTS.lock().unwrap();
    pages.retain(|(id, _)| id != &link.id);
    pages.push((link.id, page));
    Ok(info)
}

/// The view request for opening `url`:
/// with the device's settings, and the URL's domain and the URL itself.
fn view_options(url: &str, link: &LiffLink, chat: Option<String>) -> LiffViewOptions {
    LiffViewOptions {
        chat_mid: chat,
        liff_id: Some(link.id.clone()),
        lang: Some(lang()),
        device_setting: Some(DEVICE),
        ..Default::default()
    }
    .for_url(url)
}

/// A Mac plays videos on its own and has no in-app purchases (LAAB can't
/// buy through Google Play or the App Store), nor an advertising id.
const DEVICE: LiffDeviceSetting = LiffDeviceSetting {
    video_auto_play_allowed: true,
    in_app_purchase_available: false,
};

/// Issue a LIFF view for `url`. Errors with `liff_consent_required` until the
/// user agrees on LINE's consent page; it never consents silently.
pub async fn issue(
    state: &Arc<AppState>,
    url: &str,
    chat: Option<String>,
    consent: bool,
) -> Result<LiffView> {
    let url_requested = url;
    let link = parse_link(url).ok_or_else(|| anyhow!("invalid_liff_id"))?;
    let web = |reason: &str| LiffView {
        liff_id: link.id.clone(),
        url: link.web_url(),
        mode: "web".into(),
        reason: Some(reason.into()),
        info: serde_json::json!({ "requestedUrl": url }),
    };
    let Some(line) = state.line().await else {
        return Ok(web("ยังไม่ได้เข้าสู่ระบบ"));
    };
    let opts = view_options(url, &link, chat);
    let liff = line.client.liff();
    let view = match liff.issue_view(&opts).await {
        Ok(v) => v,
        Err(e) => {
            let Some(asked) = lliff::consent_required(&e) else {
                return Err(anyhow!("{e}"));
            };
            if !consent {
                return Err(anyhow!("liff_consent_required"));
            }
            // "Allow" on the page the user was shown, or a fresh copy of it.
            let shown = {
                let mut pages = CONSENTS.lock().unwrap();
                let i = pages.iter().position(|(id, _)| id == &link.id);
                i.map(|i| pages.remove(i).1)
            };
            let page = match shown {
                Some(p) if p.consent == asked => p,
                _ => liff.consent_page(&asked).await.map_err(|e| anyhow!("{e}"))?,
            };
            liff.approve(&page).await.map_err(|e| anyhow!("{e}"))?;
            liff.issue_view(&opts).await.map_err(|e| anyhow!("{e}"))?
        }
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
    let info = serde_json::json!({
        "requestedUrl": url_requested,
        "endpoint": endpoint,
        "appName": v["view"]["appName"],
        "viewType": v["view"]["type"],
        "scopes": v["scopes"],
        "expiresIn": v["accessTokenExpiresIn"],
        "tokens": {
            "access_token": token,
            "id_token": s("idToken"),
            "context_token": s("contextToken"),
            "feature_token": s("featureToken"),
        },
    });
    Ok(LiffView {
        liff_id: link.id,
        url,
        mode: "client".into(),
        reason: None,
        info,
    })
}

/// Bridge commands LAAB answers; `liff.isApiAvailable` and `liff.init`
/// read this list (`features`).
const FEATURES: &[&str] = &["closeWindow", "openWindow", "getAdvertisingId", "scanCode"];

/// LIFF pages (Chattizen, …) put `target="_blank"` on every outbound link.
/// WKWebView's `createWebView` often gets `about:blank` for those, so an
/// `on_new_window` Deny still looks like nothing happened. Send them through
/// `laab-open://` instead, which `on_navigation` already opens.
const OPEN_BLANK_LINKS: &str = r#"(function(){
  if (window.__laabOpenBlank) return;
  window.__laabOpenBlank = true;
  function open(url) {
    if (!url) return;
    location.href = 'laab-open://open?url=' + encodeURIComponent(url);
  }
  document.addEventListener('click', function(e) {
    if (e.defaultPrevented) return;
    var a = e.target && e.target.closest && e.target.closest('a[href]');
    if (!a) return;
    var href = a.getAttribute('href') || '';
    if (!href || href.charAt(0) === '#' || href.indexOf('javascript:') === 0) return;
    var blank = String(a.target || '').toLowerCase() === '_blank' || e.metaKey || e.ctrlKey;
    if (!blank) return;
    e.preventDefault();
    e.stopPropagation();
    open(a.href || href);
  }, true);
  var orig = window.open;
  window.open = function(url) {
    if (url) { open(String(url)); return null; }
    return orig ? orig.apply(this, arguments) : null;
  };
})();"#;

/// The `window._liff` bridge, with the app's granted `scopes`. A command
/// LAAB can't do answers `CLIENT_UNSUPPORTED_OPERATION`, so the app can
/// tell "not here" from "refused".
fn bridge(scopes: &Json) -> String {
    let scopes = if scopes.is_array() { scopes.clone() } else { Json::Array(vec![]) };
    BRIDGE
        .replace("__FEATURES__", &serde_json::to_string(FEATURES).unwrap_or_default())
        .replace("__SCOPES__", &scopes.to_string())
}

const BRIDGE: &str = r#"(function(){
  if (window._liff) return;
  function reply(type, callbackId, data, error) {
    var detail = error ? {type: type, callbackId: callbackId, error: error} : {type: type, callbackId: callbackId, data: data || {}};
    window.dispatchEvent(new CustomEvent('liffEvent', {detail: detail}));
  }
  var features = __FEATURES__;
  // For answers that come back from LAAB later (scanCode).
  window.__laabLiffReply = reply;
  window._liff = {
    // LIFF SDK 2.x in a LINE app 9.5.0 or newer waits for this list, or for
    // a "ready" event, before liff.init() goes on.
    features: features,
    scopes: __SCOPES__,
    postMessage: function(type, featureToken, callbackId, params) {
      try { params = typeof params === 'string' ? JSON.parse(params) : (params || {}); } catch (e) { params = {}; }
      var known = features.indexOf(type) >= 0;
      if (window.__laabDev) {
        var shown; try { shown = JSON.stringify(params); } catch (e) { shown = '?'; }
        window.__laabDev.push('bridge', known ? 'info' : 'warn',
          type + ' ' + shown + (known ? '' : ' → CLIENT_UNSUPPORTED_OPERATION (LAAB ยังไม่รองรับ)'));
      }
      if (type === 'closeWindow') { location.href = 'liff://close'; return; }
      if (type === 'openWindow') {
        reply(type, callbackId, {});
        location.href = 'laab-open://open?url=' + encodeURIComponent(params.url || '');
        return;
      }
      // An image or a part of the screen instead of the camera.
      if (type === 'scanCode') {
        location.href = 'laab-scan://scan?cb=' + encodeURIComponent(callbackId || '');
        return;
      }
      // A Mac has no advertising id.
      if (type === 'getAdvertisingId') { reply(type, callbackId, {success: true, advertisingId: null, tracking: false}); return; }
      reply(type, callbackId, null, {code: 'CLIENT_UNSUPPORTED_OPERATION', description: 'Unsupported Operation: "' + type + '"', message: 'LAAB ยังไม่รองรับ ' + type});
    }
  };
  reply('ready');
})();"#;

/// The bridge's answer to `scanCode`: `{value}`, null when nothing was read.
fn scan_reply(callback_id: &str, value: Option<&str>) -> String {
    format!(
        "window.__laabLiffReply && window.__laabLiffReply('scanCode', {}, {})",
        Json::from(callback_id),
        serde_json::json!({ "value": value })
    )
}

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
    let (nav_app2, nav_label2) = (app.clone(), label.clone());
    let chat_for_log = chat.clone();
    let log = move |level: &str, text: String| {
        crate::liff_devtools::push(&nav_app, &nav_label, "nav", level, text)
    };
    // target="_blank" / window.open: WKWebView denies these with no handler, so
    // a LIFF page's "open another LIFF" link (Chattizen always sets target=_blank
    // on rich-text links) did nothing. Route them the same way as a click.
    let (new_state, new_chat, new_app, new_label) =
        (state.clone(), chat.clone(), app.clone(), label.clone());
    let mut builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(url))
        .title("LIFF")
        .inner_size(420.0, 760.0)
        .min_inner_size(320.0, 420.0)
        .user_agent(&user_agent(client))
        .on_new_window(move |u, _features| {
            let s = u.as_str();
            let bare = s.split('#').next().unwrap_or(s);
            // about:blank is WKWebView's placeholder for target=_blank; the
            // OPEN_BLANK_LINKS script should have rewritten the click already.
            if bare.is_empty() || bare == "about:blank" {
                return NewWindowResponse::Deny;
            }
            crate::liff_devtools::push(
                &new_app,
                &new_label,
                "nav",
                "info",
                format!("หน้าต่างใหม่ → {bare}"),
            );
            open_outbound(&new_state, s, new_chat.clone(), Some(new_label.clone()));
            NewWindowResponse::Deny
        })
        .on_navigation(move |u| {
            let s = u.as_str();
            let bare = s.split('#').next().unwrap_or(s).to_string();
            match u.scheme() {
                "laab-devtools" => {
                    let _ = crate::liff_devtools::open(&nav_app2, Some(&nav_label2));
                    false
                }
                "laab-nav" => {
                    use crate::liff_devtools::{Nav, navigate};
                    if let Some(nav) = u.host_str().and_then(Nav::from_host) {
                        let _ = navigate(&nav_app2, &nav_label2, nav);
                    }
                    false
                }
                "laab-scan" => {
                    let cb = u
                        .query_pairs()
                        .find(|(k, _)| k == "cb")
                        .map(|(_, v)| v.to_string())
                        .unwrap_or_default();
                    log("info", "liff.scanCode()".into());
                    let (app, label) = (nav_app2.clone(), nav_label2.clone());
                    tauri::async_runtime::spawn(async move {
                        let window = app.get_webview_window(&label);
                        let value = crate::scan::scan(&app, window.as_ref()).await;
                        if let Some(w) = window {
                            let _ = w.eval(scan_reply(&cb, value.as_deref()));
                        }
                    });
                    false
                }
                "liff" if u.host_str() == Some("close") => {
                    log("info", "liff.closeWindow()".into());
                    close(&nav_app2, &nav_label2);
                    false
                }
                // The SDK reports the running app this way after init; reloading
                // on it would lose the login.
                "liff" => {
                    log("debug", format!("ข้าม {bare}"));
                    false
                }
                "laab-open" => {
                    let target = u
                        .query_pairs()
                        .find(|(k, _)| k == "url")
                        .map(|(_, v)| v.to_string())
                        .unwrap_or_default();
                    log("info", format!("liff.openWindow → {target}"));
                    open_outbound(&nav_state, &target, chat.clone(), Some(nav_label2.clone()));
                    false
                }
                "line" if u.host_str() == Some("picker") => {
                    let q: std::collections::HashMap<String, String> =
                        u.query_pairs().map(|(k, v)| (k.into(), v.into())).collect();
                    log("info", "shareTargetPicker".into());
                    if client && q.get("liffId").map(String::as_str) == Some(current.as_str()) {
                        let data = q
                            .get("data")
                            .and_then(|d| serde_json::from_str(d).ok())
                            .unwrap_or(Json::Null);
                        let _ = nav_app2.emit_to(
                            "main",
                            "liff-picker",
                            PickerRequest {
                                window: nav_label2.clone(),
                                liff_id: current.clone(),
                                ott: q.get("ott").cloned().unwrap_or_default(),
                                data,
                            },
                        );
                    }
                    false
                }
                "line" => {
                    log("info", format!("เปิด {bare}"));
                    open_outbound(&nav_state, s, chat.clone(), Some(nav_label2.clone()));
                    false
                }
                "http" | "https" => {
                    // Any liff.line.me link needs a fresh issueLiffView —
                    // loading the redirector in-place drops the tokens. Same
                    // LIFF app stays in this window so back/forward work.
                    match parse_link(s) {
                        Some(_) if client => {
                            log("info", format!("เปิด LIFF → {bare}"));
                            open_outbound(
                                &nav_state,
                                s,
                                chat.clone(),
                                Some(nav_label2.clone()),
                            );
                            false
                        }
                        None if is_line_app_link(s) => {
                            log("info", format!("เปิด {bare}"));
                            open_line_link(&nav_state, s, chat.clone());
                            false
                        }
                        _ => {
                            log("info", format!("ไปที่ {bare}"));
                            true
                        }
                    }
                }
                "about" | "data" | "blob" => true,
                _ => {
                    log("info", format!("เปิดนอกแอป {bare}"));
                    let _ = tauri_plugin_opener::open_url(s, None::<&str>);
                    false
                }
            }
        });
    builder = builder
        .initialization_script(crate::liff_devtools::CAPTURE)
        .initialization_script(OPEN_BLANK_LINKS);
    if client {
        builder = builder.initialization_script(bridge(&view.info["scopes"]));
    }
    let window = builder.build()?;
    crate::liff_titlebar::add(&window);
    crate::liff_devtools::opened(&app, &label, view, chat_for_log);
    Ok(())
}

fn close(app: &tauri::AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.close();
    }
}

/// `https://line.me/R/…`, a link LINE opens itself rather than in a browser.
fn is_line_app_link(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://line.me/r/") || lower.starts_with("http://line.me/r/")
}

/// A LINE deep link from a LIFF app that isn't another LIFF app: a chat
/// message opens in the main window; anything else goes to the main
/// window's link handler, which opens what LAAB can (calendar links, …)
/// and says so when it can't.
fn open_line_link(state: &Arc<AppState>, url: &str, chat: Option<String>) {
    if let Some(line_rs::link::LineLink::ChatMessage { chat_id, message_id }) = line_rs::link::LineLink::parse(url) {
        crate::show_in_main(&state.app, &chat_id, message_id.as_deref());
        return;
    }
    let _ = state.app.emit_to("main", "liff-open-url", serde_json::json!({ "url": url, "chatId": chat }));
}

/// What a LIFF page does when it leaves for a URL: another page of the same
/// LIFF app in this window (so back/forward work), a new window for a
/// different LIFF, a LINE deep link in the main window, or the system
/// browser. Used for `liff.openWindow`, `target="_blank"` / `window.open`,
/// and `line://…`.
fn open_outbound(state: &Arc<AppState>, url: &str, chat: Option<String>, window: Option<String>) {
    if parse_link(url).is_some() {
        open_liff(state, url, chat, window);
    } else if is_line_app_link(url) || url.to_ascii_lowercase().starts_with("line:") {
        open_line_link(state, url, chat);
    } else if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.starts_with("tel:")
    {
        let _ = tauri_plugin_opener::open_url(url, None::<&str>);
    }
}

/// LIFF links open as LIFF windows; anything else goes to the browser.
pub fn open_external_or_liff(state: &Arc<AppState>, url: &str, chat: Option<String>) {
    if parse_link(url).is_some() {
        open_liff(state, url, chat, None);
    } else if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.starts_with("tel:")
    {
        let _ = tauri_plugin_opener::open_url(url, None::<&str>);
    }
}

/// Issue tokens and show the LIFF. When `window` is a live window of the
/// same LIFF id, navigate it in place so the title-bar back/forward buttons
/// can return to the previous page (LINE's stacked-page behaviour).
fn open_liff(state: &Arc<AppState>, url: &str, chat: Option<String>, window: Option<String>) {
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
        if let Some(label) = window.as_deref() {
            let same = crate::liff_devtools::session(label)
                .is_some_and(|sess| sess.liff_id == view.liff_id);
            if same {
                if let Some(w) = s.app.get_webview_window(label) {
                    match view.url.parse::<tauri::Url>() {
                        Ok(u) => {
                            crate::liff_devtools::push(
                                &s.app,
                                label,
                                "nav",
                                "info",
                                format!(
                                    "ไปที่หน้า LIFF {}",
                                    u.as_str().split('#').next().unwrap_or(u.as_str())
                                ),
                            );
                            crate::liff_devtools::updated(&s.app, label, &view);
                            if let Err(e) = w.navigate(u) {
                                s.status(format!("เปิด LIFF ไม่สำเร็จ: {e}"));
                            }
                            return;
                        }
                        Err(e) => {
                            s.status(format!("เปิด LIFF ไม่สำเร็จ: {e}"));
                            return;
                        }
                    }
                }
            }
        }
        if let Err(e) = open_window(&s, &view, chat) {
            s.status(format!("เปิด LIFF ไม่สำเร็จ: {e}"));
        }
    });
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
            lang: Some(lang()),
            ..Default::default()
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

/// How a share-target picker ended: nothing picked is a cancel.
pub fn picker_outcome(picked: usize, result: &ShareResult) -> PickerResult {
    if picked == 0 {
        PickerResult::Cancel
    } else if result.sent > 0 {
        PickerResult::Success
    } else {
        PickerResult::Failure
    }
}

/// Tell LINE how the picker ended, which the LIFF SDK waits for:
/// `storeShareTargetPickerResult`.
pub async fn picker_result(state: &Arc<AppState>, liff_id: &str, ott: &str, result: PickerResult) -> Result<()> {
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    line.client
        .liff()
        .store_share_target_picker_result(liff_id, ott, result, None)
        .await
        .map_err(|e| anyhow!("{e}"))
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
    fn maps_locales_to_lines_form() {
        assert_eq!(normalize_locale("th-TH").as_deref(), Some("th_TH"));
        assert_eq!(normalize_locale("en_TH@rg=thzzzz").as_deref(), Some("en_TH"));
        assert_eq!(normalize_locale("ja_JP.UTF-8").as_deref(), Some("ja_JP"));
        assert_eq!(normalize_locale("zh-Hant-TW").as_deref(), Some("zh_TW"));
        assert_eq!(normalize_locale("ko").as_deref(), Some("ko"));
        assert_eq!(normalize_locale(""), None);
        assert_eq!(normalize_locale("1"), None);
        set_account_locale(Some("en-US".into()));
        assert_eq!(lang(), "en_US");
        set_account_locale(None);
    }

    #[test]
    fn the_bridge_lists_what_laab_answers() {
        let js = bridge(&serde_json::json!(["profile", "chat_message.write"]));
        assert!(js.contains(r#"var features = ["closeWindow","openWindow","getAdvertisingId","scanCode"];"#));
        assert!(js.contains(r#"scopes: ["profile","chat_message.write"],"#));
        assert!(js.contains("CLIENT_UNSUPPORTED_OPERATION"));
        assert!(!js.contains("__FEATURES__") && !js.contains("__SCOPES__"));
        assert!(bridge(&Json::Null).contains("scopes: [],"));
    }

    #[test]
    fn blank_target_links_go_through_laab_open() {
        assert!(OPEN_BLANK_LINKS.contains("laab-open://open?url="));
        assert!(OPEN_BLANK_LINKS.contains("_blank"));
        assert!(OPEN_BLANK_LINKS.contains("window.open"));
    }

    #[test]
    fn scan_answers_with_the_value_or_null() {
        assert_eq!(
            scan_reply("cb'1", Some("https://example.com")),
            r#"window.__laabLiffReply && window.__laabLiffReply('scanCode', "cb'1", {"value":"https://example.com"})"#
        );
        assert!(scan_reply("2", None).ends_with(r#"{"value":null})"#));
    }

    #[test]
    fn views_carry_the_device_and_the_url() {
        let url = "https://liff.line.me/1234567890-AbCdEfGh/menu?x=1";
        let link = parse_link(url).unwrap();
        let o = view_options(url, &link, Some("c1".into()));
        assert_eq!(o.device_setting, Some(DEVICE));
        assert_eq!(o.domain.as_deref(), Some("liff.line.me"));
        assert_eq!(o.requested_url.as_deref(), Some(url));
        assert!(!o.subsequent_liff);
    }

    #[test]
    fn picker_outcomes() {
        let none = ShareResult { sent: 0, failed: vec![] };
        let one = ShareResult { sent: 1, failed: vec![] };
        assert_eq!(picker_outcome(0, &none), PickerResult::Cancel);
        assert_eq!(picker_outcome(2, &one), PickerResult::Success);
        assert_eq!(picker_outcome(1, &none), PickerResult::Failure);
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
