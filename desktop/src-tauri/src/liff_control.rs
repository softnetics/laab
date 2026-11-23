//! Driving a LIFF window for MCP: read the page as text with numbered
//! elements, click and type into them, run scripts, and take screenshots.
//! LIFF pages are remote and get no IPC, so everything goes through
//! `eval_with_callback`; a script's result (even a promise's) is parked in
//! the page and polled for.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::sync::oneshot;

type R<T> = Result<T, String>;

static NEXT: AtomicU64 = AtomicU64::new(1);

/// How long one `eval_with_callback` may take to answer: none comes while
/// the page is navigating.
const ANSWER: Duration = Duration::from_secs(2);
/// How long a script (or its promise) may run.
pub const SCRIPT: Duration = Duration::from_secs(30);
/// How long a page may take to load after opening or a click.
const LOAD: Duration = Duration::from_secs(15);

/// The window of a LIFF session MCP may drive.
pub fn window(app: &AppHandle, label: &str) -> R<WebviewWindow> {
    if !label.starts_with("liff-") {
        return Err(format!("not a LIFF window: {label}"));
    }
    app.get_webview_window(label)
        .ok_or_else(|| format!("LIFF window {label} is closed; see list_liff_windows"))
}

/// Run `js` (an expression) once and return what it evaluates to, as the
/// JSON WebKit hands back. None if the page didn't answer in time.
async fn eval_once(w: &WebviewWindow, js: String) -> R<Option<Json>> {
    let (tx, rx) = oneshot::channel();
    let tx = Mutex::new(Some(tx));
    w.eval_with_callback(js, move |raw| {
        if let Some(tx) = tx.lock().unwrap().take() {
            let _ = tx.send(raw);
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(match tokio::time::timeout(ANSWER, rx).await {
        Ok(Ok(raw)) => Some(serde_json::from_str(&raw).unwrap_or(Json::String(raw))),
        _ => None,
    })
}

/// Call `function_js` (the source of a JS function, sync or async) in the
/// page and return its result.
pub async fn call(w: &WebviewWindow, function_js: &str, limit: Duration) -> R<Json> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let start = format!(
        "(function(){{var b=window.__laabMcp=window.__laabMcp||{{}};b[{id}]={{done:false}};\
         Promise.resolve().then(function(){{return ({function_js})();}}).then(\
         function(v){{b[{id}]={{done:true,value:v===undefined?null:v}};}},\
         function(e){{b[{id}]={{done:true,error:String(e&&e.stack||e)}};}});return 'started';}})()"
    );
    if eval_once(w, start).await?.is_none() {
        return Err("the page didn't answer; it may still be loading".into());
    }
    let poll = format!(
        "(function(){{var b=window.__laabMcp||{{}},e=b[{id}];if(!e)return 'gone';if(!e.done)return 'wait';\
         delete b[{id}];try{{return JSON.stringify(e)}}catch(x){{return JSON.stringify({{done:true,value:String(e.value)}})}}}})()"
    );
    let until = Instant::now() + limit;
    let mut delay = Duration::from_millis(20);
    loop {
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_millis(250));
        match eval_once(w, poll.clone()).await? {
            Some(Json::String(s)) if s == "wait" => {}
            Some(Json::String(s)) if s == "gone" => {
                return Ok(json!({ "navigated": true, "note": "the page navigated away before the script finished" }));
            }
            Some(Json::String(s)) => {
                let e: Json = serde_json::from_str(&s).map_err(|e| e.to_string())?;
                if let Some(err) = e.get("error") {
                    return Err(format!("the script threw: {}", err.as_str().unwrap_or_default()));
                }
                return Ok(e["value"].clone());
            }
            // Navigating: the new page won't have it, which the next poll says.
            None | Some(_) => {}
        }
        if Instant::now() > until {
            return Err(format!("the script didn't finish in {}s", limit.as_secs()));
        }
    }
}

/// Wait until the page has loaded, after opening or something that may
/// navigate.
pub async fn settle(w: &WebviewWindow) {
    tokio::time::sleep(Duration::from_millis(600)).await;
    let until = Instant::now() + LOAD;
    while Instant::now() < until {
        if let Ok(Some(Json::String(s))) = eval_once(w, "document.readyState".into()).await
            && s == "complete"
        {
            // Pages often render after load (SPAs fetching their data).
            tokio::time::sleep(Duration::from_millis(400)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// The page as text: visible text, and each element that can be clicked
/// or typed into as `[n] kind "label"`, numbered for `target`.
pub async fn outline(w: &WebviewWindow) -> R<Json> {
    call(w, OUTLINE, SCRIPT).await
}

/// Which element an action is for: its number from the outline, a CSS
/// selector, or its text.
pub fn target(args: &Json) -> R<Json> {
    let t = json!({
        "ref": args.get("ref").and_then(Json::as_i64),
        "selector": args.get("selector").and_then(Json::as_str),
        "text": args.get("text").and_then(Json::as_str),
    });
    if t.as_object().unwrap().values().all(Json::is_null) {
        return Err("give ref (from read_liff), selector or text".into());
    }
    Ok(t)
}

pub async fn click(w: &WebviewWindow, target: &Json) -> R<Json> {
    call(w, &format!("function(){{var T={target};{FIND}{CLICK}}}"), SCRIPT).await
}

pub async fn type_text(w: &WebviewWindow, target: &Json, value: &str, append: bool, submit: bool) -> R<Json> {
    let value = serde_json::to_string(value).unwrap();
    call(
        w,
        &format!("function(){{var T={target},V={value},APPEND={append},SUBMIT={submit};{FIND}{TYPE}}}"),
        SCRIPT,
    )
    .await
}

/// Run the user's script as an expression (or statements; the last value
/// counts), awaiting it if it is a promise.
pub async fn run_script(w: &WebviewWindow, script: &str) -> R<Json> {
    let quoted = serde_json::to_string(script).unwrap();
    call(w, &format!("function(){{return (0,eval)({quoted});}}"), SCRIPT).await
}

/// A PNG of what the window shows.
pub async fn screenshot(w: &WebviewWindow) -> R<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        use block2::RcBlock;
        use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
        use objc2_foundation::{NSDictionary, NSError};
        use objc2_web_kit::WKWebView;

        let (tx, rx) = oneshot::channel::<Option<Vec<u8>>>();
        let tx = Arc::new(Mutex::new(Some(tx)));
        w.with_webview(move |pv| {
            let tx = tx.clone();
            let done = RcBlock::new(move |image: *mut NSImage, _error: *mut NSError| {
                // SAFETY: WebKit hands a valid NSImage or null.
                let png = unsafe { image.as_ref() }.and_then(|image| {
                    let tiff = image.TIFFRepresentation()?;
                    let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
                    // SAFETY: an empty properties dictionary is allowed.
                    let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
                    Some(data.to_vec())
                });
                if let Some(tx) = tx.lock().unwrap().take() {
                    let _ = tx.send(png);
                }
            });
            // SAFETY: on macOS the platform webview is a WKWebView, and this
            // runs on the main thread.
            unsafe {
                let web = &*(pv.inner() as *const WKWebView);
                web.takeSnapshotWithConfiguration_completionHandler(None, &done);
            }
        })
        .map_err(|e| e.to_string())?;
        match tokio::time::timeout(Duration::from_secs(10), rx).await {
            Ok(Ok(Some(png))) => Ok(png),
            _ => Err("couldn't take a screenshot of the LIFF window".into()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = w;
        Err("screenshots need macOS".into())
    }
}

// ------------------------------------------------------------ page scripts

const OUTLINE: &str = r#"function(){
  var refs = 0, out = [], max = 24000, len = 0;
  document.querySelectorAll('[data-laab-ref]').forEach(function(e){ e.removeAttribute('data-laab-ref'); });
  function shown(el){
    var s = getComputedStyle(el);
    if (s.display === 'none' || s.visibility === 'hidden' || +s.opacity === 0) return false;
    if (s.display === 'contents') return true;
    var r = el.getBoundingClientRect();
    return r.width >= 1 && r.height >= 1;
  }
  function clickable(el){
    var t = el.tagName;
    if (/^(BUTTON|INPUT|SELECT|TEXTAREA|SUMMARY)$/.test(t)) return true;
    if (t === 'A') return el.hasAttribute('href') || !!el.onclick;
    var role = el.getAttribute('role');
    if (role && /^(button|link|checkbox|radio|tab|menuitem|option|switch|combobox|textbox|slider)$/.test(role)) return true;
    if (el.isContentEditable && !(el.parentElement && el.parentElement.isContentEditable)) return true;
    if (el.onclick || el.hasAttribute('onclick')) return true;
    var p = el.parentElement;
    return getComputedStyle(el).cursor === 'pointer' && !(p && getComputedStyle(p).cursor === 'pointer');
  }
  function clean(s, n){ return String(s || '').replace(/\s+/g, ' ').trim().slice(0, n); }
  function label(el){
    var t = el.getAttribute('aria-label') || (el.tagName === 'INPUT' ? '' : el.innerText) || el.title || el.alt || '';
    if (!clean(t, 1)) { var img = el.querySelector && el.querySelector('img[alt]'); if (img) t = img.alt; }
    if (!clean(t, 1) && el.id) { var l = document.querySelector('label[for="' + CSS.escape(el.id) + '"]'); if (l) t = l.innerText; }
    return clean(t, 120);
  }
  function line(depth, s){ if (len > max) return; s = '  '.repeat(Math.min(depth, 12)) + s; out.push(s); len += s.length + 1; }
  function walk(node, depth){
    for (var c = node.firstChild; c; c = c.nextSibling) {
      if (c.nodeType === 3) {
        var t = clean(c.textContent, 400);
        if (t) line(depth, t);
        continue;
      }
      if (c.nodeType !== 1) continue;
      var el = c, tag = el.tagName.toUpperCase();
      if (/^(SCRIPT|STYLE|NOSCRIPT|TEMPLATE|SVG|HEAD|META|LINK)$/.test(tag)) continue;
      if (!shown(el)) continue;
      if (tag === 'IFRAME') { line(depth, '[iframe ' + clean(el.src, 200) + ']'); continue; }
      if (clickable(el)) {
        var n = ++refs;
        el.setAttribute('data-laab-ref', n);
        var kind = tag.toLowerCase();
        if (tag === 'INPUT') kind = 'input[' + (el.type || 'text') + ']';
        var role = el.getAttribute('role');
        if (role) kind += ' role=' + role;
        if (el.isContentEditable) kind += ' editable';
        var extra = '';
        if (tag === 'INPUT' || tag === 'TEXTAREA') {
          if (el.type === 'checkbox' || el.type === 'radio') extra = el.checked ? ' (checked)' : ' (unchecked)';
          else if (el.type === 'password') extra = el.value ? ' (filled)' : '';
          else extra = ' value="' + clean(el.value, 200) + '"' + (el.placeholder ? ' placeholder="' + clean(el.placeholder, 80) + '"' : '');
        }
        if (tag === 'SELECT') {
          var o = el.selectedOptions[0];
          extra = ' value="' + clean(o ? o.text : '', 80) + '" options=' + JSON.stringify([].map.call(el.options, function(o){ return clean(o.text, 60); }).slice(0, 60));
        }
        if (tag === 'A' && el.href) extra += ' href=' + el.href.split('#')[0].slice(0, 200);
        if (el.disabled || el.getAttribute('aria-disabled') === 'true') extra += ' (disabled)';
        line(depth, '[' + n + '] ' + kind + ' "' + label(el) + '"' + extra);
        if (tag !== 'SELECT' && el.querySelector('a,button,input,select,textarea,[role],[onclick],[contenteditable]')) walk(el, depth + 1);
        continue;
      }
      if (tag === 'IMG') { if (clean(el.alt, 1)) line(depth, '[img "' + clean(el.alt, 80) + '"]'); continue; }
      var block = /^(UL|OL|SECTION|FORM|NAV|HEADER|FOOTER|MAIN|DIALOG|TABLE|ASIDE|ARTICLE)$/.test(tag) || el.getAttribute('role') === 'dialog';
      walk(el, depth + (block ? 1 : 0));
    }
  }
  if (document.body) walk(document.body, 0);
  return {
    title: document.title,
    url: location.href.split('#')[0],
    readyState: document.readyState,
    scroll: { y: Math.round(scrollY), height: document.documentElement.scrollHeight, viewport: innerHeight },
    truncated: len > max,
    content: out.join('\n')
  };
}"#;

/// Finds `T` ({ref, selector, text}) as `el`, or throws.
const FIND: &str = r#"
  var el = null;
  if (T.ref != null) {
    el = document.querySelector('[data-laab-ref="' + T.ref + '"]');
    if (!el) throw new Error('no element [' + T.ref + ']; the page changed, call read_liff again');
  } else if (T.selector) {
    el = document.querySelector(T.selector);
    if (!el) throw new Error('nothing matches ' + T.selector);
  } else {
    var want = T.text.replace(/\s+/g, ' ').trim().toLowerCase();
    var best = null, bestScore = 1e9;
    document.querySelectorAll('body *').forEach(function(e){
      if (/^(SCRIPT|STYLE|HTML|BODY)$/.test(e.tagName)) return;
      var r = e.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) return;
      var t = (e.getAttribute('aria-label') || e.innerText || e.value || e.placeholder || '').replace(/\s+/g, ' ').trim().toLowerCase();
      if (!t || t.indexOf(want) < 0) return;
      var score = (t === want ? 0 : 1000) + t.length - (/^(A|BUTTON|INPUT|SELECT|TEXTAREA)$/.test(e.tagName) || e.getAttribute('role') ? 500 : 0);
      if (score < bestScore) { best = e; bestScore = score; }
    });
    el = best;
    if (!el) throw new Error('no element with the text ' + JSON.stringify(T.text));
  }
  function describe(e){ return e.tagName.toLowerCase() + ' "' + String(e.getAttribute('aria-label') || e.innerText || e.value || '').replace(/\s+/g, ' ').trim().slice(0, 80) + '"'; }
"#;

const CLICK: &str = r#"
  el.scrollIntoView({ block: 'center', inline: 'center' });
  var r = el.getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2;
  var hit = document.elementFromPoint(x, y);
  var to = hit && (el.contains(hit) || hit.contains(el)) ? hit : el;
  var o = { bubbles: true, cancelable: true, composed: true, clientX: x, clientY: y, button: 0, view: window };
  to.dispatchEvent(new PointerEvent('pointerdown', Object.assign({ pointerType: 'mouse', isPrimary: true }, o)));
  to.dispatchEvent(new MouseEvent('mousedown', o));
  if (to.focus) to.focus();
  to.dispatchEvent(new PointerEvent('pointerup', Object.assign({ pointerType: 'mouse', isPrimary: true }, o)));
  to.dispatchEvent(new MouseEvent('mouseup', o));
  // The point may land on an <svg> or <path> inside a button; SVG elements
  // have no click(), so click the nearest HTML element around it.
  var clickTo = to;
  while (clickTo && !(clickTo instanceof HTMLElement)) clickTo = clickTo.parentElement;
  if (clickTo) clickTo.click(); else to.dispatchEvent(new MouseEvent('click', o));
  var covered = hit && !(el.contains(hit) || hit.contains(el)) ? ' (covered by ' + describe(hit) + ')' : '';
  return 'clicked ' + describe(el) + covered;
"#;

const TYPE: &str = r#"
  el.scrollIntoView({ block: 'center' });
  if (el.focus) el.focus();
  var tag = el.tagName;
  if (tag === 'SELECT') {
    var want = V.trim().toLowerCase();
    var i = [].findIndex.call(el.options, function(o){ return o.text.trim().toLowerCase() === want || o.value.toLowerCase() === want; });
    if (i < 0) throw new Error('no option ' + JSON.stringify(V) + ' in ' + describe(el));
    el.selectedIndex = i;
  } else if (tag === 'INPUT' || tag === 'TEXTAREA') {
    if (el.type === 'checkbox' || el.type === 'radio') throw new Error('use liff_click for checkboxes and radio buttons');
    var proto = tag === 'INPUT' ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
    var set = Object.getOwnPropertyDescriptor(proto, 'value').set;
    set.call(el, APPEND ? el.value + V : V);
  } else if (el.isContentEditable) {
    if (!APPEND) document.execCommand('selectAll', false);
    document.execCommand('insertText', false, V);
  } else {
    throw new Error(describe(el) + ' is not something to type into');
  }
  if (!el.isContentEditable) el.dispatchEvent(new InputEvent('input', { bubbles: true, data: V, inputType: 'insertText' }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  if (SUBMIT) {
    var k = { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true };
    var go = el.dispatchEvent(new KeyboardEvent('keydown', k));
    el.dispatchEvent(new KeyboardEvent('keypress', k));
    el.dispatchEvent(new KeyboardEvent('keyup', k));
    if (go && el.form) { if (el.form.requestSubmit) el.form.requestSubmit(); else el.form.submit(); }
  }
  return 'typed into ' + describe(el) + (SUBMIT ? ' and pressed Enter' : '');
"#;
