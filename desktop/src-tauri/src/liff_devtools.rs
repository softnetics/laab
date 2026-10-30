//! LIFF DevTools: what each LIFF window was opened with, and a log of its
//! console, network requests, `window._liff` bridge calls and navigations.
//!
//! LIFF pages are remote and get no IPC. `CAPTURE` keeps their log in a
//! buffer inside the page; a poller drains it with `eval_with_callback`.

use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value as Json;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::liff::LiffView;
use crate::state::now_ms;

pub const WINDOW: &str = "aux-liff-devtools";
const MAX_SESSIONS: usize = 30;
const MAX_EVENTS: usize = 2000;
const POLL: Duration = Duration::from_millis(700);

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    /// Milliseconds.
    pub t: f64,
    /// `console`, `network`, `bridge`, `nav`, `error` or `page`.
    pub kind: String,
    /// `debug`, `log`, `info`, `warn` or `error`.
    pub level: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub label: String,
    pub liff_id: String,
    pub chat_id: Option<String>,
    /// `client` (tokens issued) or `web`.
    pub mode: String,
    pub reason: Option<String>,
    /// The page URL with the tokens in its fragment left out.
    pub url: String,
    /// What LINE returned: requested link, endpoint, app name, scopes and
    /// the tokens.
    pub info: Json,
    pub opened_at: f64,
    pub closed: bool,
    pub events: Vec<Event>,
}

static SESSIONS: LazyLock<Mutex<Vec<Session>>> = LazyLock::new(|| Mutex::new(Vec::new()));

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Update<'a> {
    label: &'a str,
    session: Option<&'a Session>,
    events: &'a [Event],
}

fn emit(app: &AppHandle, update: Update<'_>) {
    if app.get_webview_window(WINDOW).is_some() {
        let _ = app.emit_to(WINDOW, "liff-dev", update);
    }
}

fn masked(url: &str) -> String {
    match url.split_once('#') {
        Some((base, fragment)) if fragment.contains("_token=") => format!("{base}#…tokens"),
        _ => url.to_string(),
    }
}

/// A LIFF window opened: remember it and start draining its page log.
pub fn opened(app: &AppHandle, label: &str, view: &LiffView, chat: Option<String>) {
    let session = Session {
        label: label.to_string(),
        liff_id: view.liff_id.clone(),
        chat_id: chat,
        mode: view.mode.clone(),
        reason: view.reason.clone(),
        url: masked(&view.url),
        info: view.info.clone(),
        opened_at: now_ms(),
        closed: false,
        events: vec![],
    };
    {
        let mut all = SESSIONS.lock().unwrap();
        all.push(session.clone());
        let extra = all.len().saturating_sub(MAX_SESSIONS);
        all.drain(..extra);
    }
    emit(
        app,
        Update {
            label,
            session: Some(&session),
            events: &[],
        },
    );
    poll(app.clone(), label.to_string());
}

pub fn push(app: &AppHandle, label: &str, kind: &str, level: &str, text: impl Into<String>) {
    add(
        app,
        label,
        vec![Event {
            t: now_ms(),
            kind: kind.into(),
            level: level.into(),
            text: text.into(),
        }],
    );
}

fn add(app: &AppHandle, label: &str, events: Vec<Event>) {
    if events.is_empty() {
        return;
    }
    // Dev builds mirror the log to the terminal.
    #[cfg(debug_assertions)]
    for e in &events {
        tracing::info!("liff {label} {} {} {}", e.kind, e.level, e.text);
    }
    {
        let mut all = SESSIONS.lock().unwrap();
        let Some(s) = all.iter_mut().find(|s| s.label == label) else {
            return;
        };
        s.events.extend(events.iter().cloned());
        let extra = s.events.len().saturating_sub(MAX_EVENTS);
        s.events.drain(..extra);
    }
    emit(
        app,
        Update {
            label,
            session: None,
            events: &events,
        },
    );
}

fn set_closed(app: &AppHandle, label: &str) {
    let session = {
        let mut all = SESSIONS.lock().unwrap();
        let Some(s) = all.iter_mut().find(|s| s.label == label) else {
            return;
        };
        s.closed = true;
        s.clone()
    };
    emit(
        app,
        Update {
            label,
            session: Some(&session),
            events: &[],
        },
    );
}

/// The page's buffered events, as `eval_with_callback` hands them back: the
/// JSON of the string `drain()` returned.
fn parse_drained(raw: &str) -> Vec<Event> {
    let value: Json = serde_json::from_str(raw).unwrap_or(Json::Null);
    let value = match value {
        Json::String(inner) => serde_json::from_str(&inner).unwrap_or(Json::Null),
        v => v,
    };
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            Some(Event {
                t: e["t"].as_f64()?,
                kind: e["kind"].as_str()?.to_string(),
                level: e["level"].as_str().unwrap_or("log").to_string(),
                text: e["text"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

fn poll(app: AppHandle, label: String) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL).await;
            let Some(w) = app.get_webview_window(&label) else {
                set_closed(&app, &label);
                return;
            };
            let (app2, label2) = (app.clone(), label.clone());
            let _ = w.eval_with_callback(
                "window.__laabDev ? window.__laabDev.drain() : '[]'",
                move |raw| add(&app2, &label2, parse_drained(&raw)),
            );
        }
    });
}

/// Run `code` in the LIFF page and log it and its result, like a console.
pub fn eval(app: &AppHandle, label: &str, code: &str) -> tauri::Result<()> {
    let Some(w) = app.get_webview_window(label) else {
        return Err(tauri::Error::WebviewNotFound);
    };
    push(app, label, "console", "info", format!("> {code}"));
    let quoted = serde_json::to_string(code).unwrap_or_default();
    // An exception would be lost, so the page reports it as the result.
    let js = format!(
        "(function(){{try{{var r=(0,eval)({quoted});return typeof r==='string'?r:JSON.stringify(r,null,2)??String(r)}}catch(e){{return '⚠ '+(e&&e.stack||e)}}}})()"
    );
    let (app, label) = (app.clone(), label.to_string());
    w.eval_with_callback(js, move |raw| {
        let value: Json = serde_json::from_str(&raw).unwrap_or(Json::String(raw.clone()));
        let text = match value {
            Json::String(s) => s,
            Json::Null => "undefined".into(),
            v => v.to_string(),
        };
        let level = if text.starts_with('⚠') {
            "error"
        } else {
            "log"
        };
        push(&app, &label, "console", level, format!("← {text}"));
    })
}

pub fn sessions() -> Vec<Session> {
    SESSIONS.lock().unwrap().clone()
}

pub fn session(label: &str) -> Option<Session> {
    SESSIONS
        .lock()
        .unwrap()
        .iter()
        .find(|s| s.label == label)
        .cloned()
}

/// Open the DevTools window, or bring it forward; `focus` selects a LIFF
/// window in it.
pub fn open(app: &AppHandle, focus: Option<&str>) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window(WINDOW) {
        let _ = w.unminimize();
        w.set_focus()?;
    } else {
        WebviewWindowBuilder::new(
            app,
            WINDOW,
            WebviewUrl::App("index.html#/liff-devtools".into()),
        )
        .title("LIFF DevTools")
        .inner_size(960.0, 620.0)
        .min_inner_size(640.0, 360.0)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .build()?;
    }
    if let Some(label) = focus {
        let _ = app.emit_to(WINDOW, "liff-dev-focus", label);
    }
    Ok(())
}

/// Records page events: console output, uncaught errors, failed resources,
/// fetch and XHR results, and page loads. Tokens in the URL fragment are
/// left out. ⌥⌘L opens LIFF DevTools on this window.
pub const CAPTURE: &str = r#"(function(){
  if (window.__laabDev) return;
  var buf = [];
  function text(a) {
    if (typeof a === 'string') return a;
    if (a instanceof Error) return a.stack || (a.name + ': ' + a.message);
    try { return JSON.stringify(a); } catch (e) { return String(a); }
  }
  function push(kind, level, t) {
    if (buf.length >= 500) buf.shift();
    buf.push({t: Date.now(), kind: kind, level: level, text: String(t).slice(0, 8000)});
  }
  // No fragment, and no token values in the query.
  function bare(u) {
    return String(u).split('#')[0].replace(/([?&](?:access_token|id_token|context_token|feature_token|code)=)[^&]*/g, '$1…');
  }
  Object.defineProperty(window, '__laabDev', {value: {
    push: push,
    drain: function() { return JSON.stringify(buf.splice(0)); }
  }});
  ['debug', 'log', 'info', 'warn', 'error'].forEach(function(level) {
    var original = console[level];
    console[level] = function() {
      push('console', level, Array.prototype.map.call(arguments, text).join(' '));
      return original.apply(this, arguments);
    };
  });
  addEventListener('error', function(e) {
    var el = e.target;
    if (el && el !== window && (el.src || el.href)) {
      push('network', 'error', 'โหลดไม่ได้ ' + (el.tagName || '').toLowerCase() + ' ' + bare(el.src || el.href));
    } else {
      push('error', 'error', (e.error ? text(e.error) : e.message) + (e.filename ? '\n  at ' + e.filename + ':' + e.lineno : ''));
    }
  }, true);
  addEventListener('unhandledrejection', function(e) { push('error', 'error', 'Unhandled rejection: ' + text(e.reason)); });
  var f = window.fetch;
  if (f) window.fetch = function(input, init) {
    var method = ((init && init.method) || (input && input.method) || 'GET').toUpperCase();
    var url = bare(typeof input === 'string' ? input : (input && input.url) || input);
    var start = performance.now();
    var ms = function() { return Math.round(performance.now() - start) + 'ms'; };
    push('network', 'debug', method + ' → ' + url);
    return f.apply(this, arguments).then(function(r) {
      push('network', r.ok ? 'info' : 'warn', method + ' ' + r.status + ' ' + url + ' · ' + ms());
      return r;
    }, function(err) {
      push('network', 'error', method + ' ล้มเหลว ' + url + ' · ' + ms() + ' · ' + text(err));
      throw err;
    });
  };
  var open = XMLHttpRequest.prototype.open, send = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function(method, url) {
    this.__laab = {method: String(method).toUpperCase(), url: bare(url)};
    return open.apply(this, arguments);
  };
  XMLHttpRequest.prototype.send = function() {
    var x = this, info = x.__laab || {method: '?', url: '?'}, start = performance.now();
    push('network', 'debug', info.method + ' → ' + info.url + ' (XHR)');
    x.addEventListener('loadend', function() {
      var ok = x.status >= 200 && x.status < 400;
      push('network', x.status === 0 ? 'error' : ok ? 'info' : 'warn',
        info.method + ' ' + (x.status || 'ล้มเหลว') + ' ' + info.url + ' · ' + Math.round(performance.now() - start) + 'ms (XHR)');
    });
    return send.apply(this, arguments);
  };
  push('page', 'info', 'เริ่มโหลด ' + bare(location.href));
  // Which LIFF parameters arrived in the fragment, without their values.
  var hashKeys = location.hash.replace(/^#/, '').split('&').map(function(p) { return p.split('=')[0]; }).filter(Boolean);
  push('page', hashKeys.length ? 'info' : 'warn', 'fragment: ' + (hashKeys.join(', ') || '(ว่าง)'));
  ['pushState', 'replaceState'].forEach(function(name) {
    var original = history[name];
    history[name] = function(state, title, url) {
      if (url != null) push('nav', 'debug', 'history.' + name + ' ' + bare(new URL(url, location.href).href) + (String(url).indexOf('#') >= 0 ? ' (มี #)' : ''));
      return original.apply(this, arguments);
    };
  });
  setTimeout(function() {
    try {
      var keys = Object.keys(sessionStorage).concat(Object.keys(localStorage)).filter(function(k) { return k.indexOf('LIFF_STORE') === 0; });
      push('page', 'debug', 'LIFF_STORE หลัง 5 วินาที: ' + (keys.map(function(k) { return k.split(':').pop(); }).join(', ') || '(ไม่มี)'));
    } catch (e) {}
  }, 5000);
  addEventListener('message', function(e) {
    var d = e.data, keys;
    try { keys = d && typeof d === 'object' ? Object.keys(d).join(',') : String(d).slice(0, 120); } catch (err) { keys = '?'; }
    push('page', 'debug', 'message จาก ' + (e.origin || 'null') + ' · ' + keys);
  }, true);
  new MutationObserver(function(list) {
    list.forEach(function(m) {
      m.addedNodes.forEach(function(n) {
        if (n.tagName === 'IFRAME' || n.tagName === 'SCRIPT') {
          push('page', 'debug', 'เพิ่ม ' + n.tagName.toLowerCase() + ' ' + bare(n.src || '(inline)'));
          if (n.tagName === 'SCRIPT' && n.src) {
            n.addEventListener('load', function() { push('network', 'debug', 'โหลด script ' + bare(n.src)); });
          }
        }
      });
    });
  }).observe(document, {childList: true, subtree: true});
  addEventListener('DOMContentLoaded', function() { push('page', 'info', 'DOMContentLoaded'); });
  addEventListener('load', function() { push('page', 'info', 'load · ' + document.title); });
  addEventListener('keydown', function(e) {
    if (e.altKey && e.metaKey && (e.key === 'l' || e.key === 'L' || e.code === 'KeyL')) {
      e.preventDefault();
      location.href = 'laab-devtools://open';
    }
  }, true);
})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_drained_buffer() {
        let inner = r#"[{"t":1,"kind":"console","level":"warn","text":"hi"},{"kind":"x"}]"#;
        let raw = serde_json::to_string(inner).unwrap();
        let events = parse_drained(&raw);
        assert_eq!(events.len(), 1);
        assert_eq!(
            (
                events[0].kind.as_str(),
                events[0].level.as_str(),
                events[0].text.as_str()
            ),
            ("console", "warn", "hi")
        );
        assert!(parse_drained("null").is_empty());
    }

    #[test]
    fn leaves_tokens_out_of_the_url() {
        assert_eq!(
            masked("https://a.example/p?x=1#access_token=t&id_token=i"),
            "https://a.example/p?x=1#…tokens"
        );
        assert_eq!(
            masked("https://a.example/p#section"),
            "https://a.example/p#section"
        );
    }
}
