//! Crash and error reports, in PostHog's `$exception` shape.
//!
//! Three sources, all reduced to an error type, a location and stack frames
//! (function names, source files of LAAB or its libraries, line numbers).
//! Error and panic messages are never kept: they can hold data from LINE.
//!
//! - Rust panics: the hook writes a report to `crashes/` and the next launch
//!   sends it, since a crashing app can't be trusted to finish a request.
//! - macOS crash reports (`~/Library/Logs/DiagnosticReports/<exe>-*.ips`)
//!   written since the last launch, for crashes no panic hook sees.
//! - Uncaught errors in the webview, which hands over the error name and its
//!   stack and nothing else; [`from_js`] keeps only what parses as a frame.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::analytics::{self, Event, Status};
use crate::state::{AppState, now_ms};

const MAX_FRAMES: usize = 50;
/// kv: when crash reports from macOS were last looked at (ms).
const IPS_CHECKED: &str = "ips-checked";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub function: String,
    pub filename: Option<String>,
    pub lineno: Option<u32>,
    pub colno: Option<u32>,
    /// The binary the frame is in, for native crashes.
    pub module: Option<String>,
    pub in_app: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exception {
    /// `panic`, `EXC_BAD_ACCESS`, `TypeError`…
    pub kind: String,
    /// Where it happened or what signal, never the message.
    pub value: String,
    pub lang: String,
    pub handled: bool,
    pub frames: Vec<Frame>,
    pub crumbs: Vec<String>,
    /// The version that crashed, which may not be the one sending it.
    pub version: String,
}

impl Exception {
    /// The `$exception_list` PostHog expects.
    pub fn list(&self) -> Value {
        // PostHog lists the innermost frame last.
        let frames: Vec<Value> = self
            .frames
            .iter()
            .rev()
            .map(|f| {
                json!({
                    "platform": "custom",
                    "lang": self.lang,
                    "function": f.function,
                    "filename": f.filename,
                    "lineno": f.lineno,
                    "colno": f.colno,
                    "module": f.module,
                    "in_app": f.in_app,
                    "resolved": true,
                })
            })
            .collect();
        json!([{
            "type": self.kind,
            "value": self.value,
            "mechanism": { "handled": self.handled, "synthetic": false },
            "stacktrace": { "type": "raw", "frames": frames },
        }])
    }
}

// ------------------------------------------------------------------ panics

static CRASH_DIR: OnceLock<PathBuf> = OnceLock::new();

fn crash_dir(root: &Path) -> PathBuf {
    root.join("crashes")
}

/// Records panics from here on. The default hook still prints them.
pub fn install(root: &Path) {
    let _ = CRASH_DIR.set(crash_dir(root));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        let Some(dir) = CRASH_DIR.get() else { return };
        let value = info
            .location()
            .map(|l| format!("{}:{}", source_path(l.file()), l.line()))
            .unwrap_or_else(|| "unknown".into());
        let trace = std::backtrace::Backtrace::force_capture().to_string();
        let report = Exception {
            kind: "panic".into(),
            value,
            lang: "rust".into(),
            handled: false,
            frames: rust_frames(&trace),
            crumbs: crate::usage::crumbs().into_iter().map(String::from).collect(),
            version: env!("CARGO_PKG_VERSION").into(),
        };
        if let Ok(json) = serde_json::to_vec(&report) {
            let _ = crate::paths::write_private(&dir.join(format!("panic-{}.json", now_ms() as u64)), &json);
        }
    }));
}

/// `src/line.rs`, or `line-rs/src/client.rs` for a dependency: the part of a
/// build path that names the source, without the build machine's folders.
fn source_path(file: &str) -> String {
    let file = file.replace('\\', "/");
    if file.starts_with("src/") {
        return file;
    }
    if let Some(i) = file.rfind("/src/") {
        let head = &file[..i];
        let krate = head.rsplit('/').next().unwrap_or("");
        let tail = &file[i + 1..];
        if file.starts_with('/') || head.contains("/.cargo/") || head.contains("/rustc/") {
            return format!("{krate}/{tail}");
        }
        return tail.to_string();
    }
    file.rsplit('/').next().unwrap_or(&file).to_string()
}

/// Frames from `Backtrace`'s text: `  3: laab_lib::x::y` then an optional
/// `at /path/src/x.rs:12:5`. The panic machinery's own frames are dropped.
fn rust_frames(trace: &str) -> Vec<Frame> {
    let mut frames: Vec<Frame> = Vec::new();
    for line in trace.lines() {
        let line = line.trim();
        if let Some(at) = line.strip_prefix("at ") {
            if let Some(f) = frames.last_mut() {
                let mut parts = at.rsplitn(3, ':');
                let col = parts.next().and_then(|c| c.parse().ok());
                let row = parts.next().and_then(|r| r.parse().ok());
                if let (Some(file), Some(_)) = (parts.next(), row) {
                    f.filename = Some(source_path(file));
                    f.lineno = row;
                    f.colno = col;
                }
            }
            continue;
        }
        let Some((n, function)) = line.split_once(": ") else { continue };
        if n.parse::<u32>().is_err() {
            continue;
        }
        let function = strip_hash(function);
        frames.push(Frame {
            in_app: function.starts_with("laab_lib::") || function.starts_with("laab::"),
            function,
            filename: None,
            lineno: None,
            colno: None,
            module: None,
        });
    }
    // Start at the frame that panicked, not inside std's panic handling.
    if let Some(i) = frames.iter().rposition(|f| {
        f.function.contains("core::panicking::")
            || f.function.contains("std::panicking::")
            || f.function.contains("core::option::")
            || f.function.contains("core::result::")
    }) {
        frames.drain(..=i);
    }
    frames.truncate(MAX_FRAMES);
    frames
}

/// `foo::bar::h1a2b3c4d5e6f7a8b` → `foo::bar`.
fn strip_hash(function: &str) -> String {
    match function.rsplit_once("::h") {
        Some((head, hash)) if hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()) => head.into(),
        _ => function.into(),
    }
}

/// Sends the panic reports left by earlier runs, then deletes them. While the
/// user hasn't answered they wait; if statistics are off they are deleted.
fn send_panics(state: &AppState, status: Status) {
    let dir = crash_dir(&state.paths.root);
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    files.sort();
    let keep = if status == Status::Unasked { 10 } else { 0 };
    let drop_count = files.len().saturating_sub(keep);
    for path in files.into_iter().take(drop_count) {
        if status == Status::On
            && let Some(report) = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Exception>(&b).ok())
        {
            analytics::send(state, Event::Exception(report));
        }
        let _ = std::fs::remove_file(path);
    }
}

// -------------------------------------------------------- macOS crash logs

fn diagnostic_reports() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join("Library/Logs/DiagnosticReports"))
}

/// Native crashes macOS recorded since the last check.
fn send_native_crashes(state: &AppState, status: Status) {
    let now = now_ms();
    let checked = state.store().get_kv(IPS_CHECKED).ok().flatten().and_then(|v| v.parse::<f64>().ok());
    let _ = state.store().set_kv(IPS_CHECKED, &(now as u64).to_string());
    // The first launch only sets the mark: older crashes aren't ours to send.
    let (Some(checked), Status::On) = (checked, status) else { return };
    let Some(dir) = diagnostic_reports() else { return };
    let Some(exe) = std::env::current_exe().ok().and_then(|p| p.file_name()?.to_str().map(String::from)) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(&format!("{exe}-")) || !name.ends_with(".ips") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0);
        if modified <= checked {
            continue;
        }
        if let Some(report) = std::fs::read_to_string(entry.path()).ok().and_then(|t| from_ips(&t, &exe)) {
            analytics::send(state, Event::Exception(report));
        }
    }
}

/// A macOS `.ips` crash report: a JSON header line, then a JSON body. Only
/// the exception type, the signal and the crashed thread's frames are kept;
/// the report's paths, hardware ids and other threads are not.
fn from_ips(text: &str, exe: &str) -> Option<Exception> {
    let (header, body) = text.split_once('\n')?;
    let header: Value = serde_json::from_str(header).ok()?;
    let body: Value = serde_json::from_str(body).ok()?;
    let images = body["usedImages"].as_array()?;
    let thread = body["threads"].get(body["faultingThread"].as_u64()? as usize)?;
    let frames = thread["frames"]
        .as_array()?
        .iter()
        .take(MAX_FRAMES)
        .map(|f| {
            let image = f["imageIndex"]
                .as_u64()
                .and_then(|i| images.get(i as usize))
                .and_then(|i| i["name"].as_str())
                .unwrap_or("?")
                .to_string();
            let function = f["symbol"]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| format!("{image}+0x{:x}", f["imageOffset"].as_u64().unwrap_or(0)));
            Frame {
                in_app: image == exe,
                function,
                filename: f["sourceFile"].as_str().map(source_path),
                lineno: f["sourceLine"].as_u64().map(|n| n as u32),
                colno: None,
                module: Some(image),
            }
        })
        .collect();
    let exception = &body["exception"];
    Some(Exception {
        kind: exception["type"].as_str().unwrap_or("crash").into(),
        value: exception["signal"].as_str().unwrap_or("").into(),
        lang: "native".into(),
        handled: false,
        frames,
        crumbs: Vec::new(),
        version: header["app_version"].as_str().unwrap_or("").into(),
    })
}

// ----------------------------------------------------------------- webview

/// An uncaught webview error. `name` must look like an error class and
/// frames must point into LAAB's own scripts; the rest is dropped.
pub fn from_js(name: &str, stack: &str) -> Exception {
    let kind = if !name.is_empty() && name.len() <= 40 && name.chars().all(|c| c.is_ascii_alphanumeric()) {
        name
    } else {
        "Error"
    };
    let frames = stack.lines().filter_map(js_frame).take(MAX_FRAMES).collect();
    Exception {
        kind: kind.into(),
        value: String::new(),
        lang: "javascript".into(),
        handled: false,
        frames,
        crumbs: crate::usage::crumbs().into_iter().map(String::from).collect(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

/// WebKit's `fn@url:line:col` (or `url:line:col`) → a frame, if `url` is the
/// app's own origin.
fn js_frame(line: &str) -> Option<Frame> {
    let line = line.trim();
    let (function, location) = match line.rsplit_once('@') {
        Some((f, l)) => (f, l),
        None => ("", line),
    };
    let mut parts = location.rsplitn(3, ':');
    let col = parts.next()?.parse().ok()?;
    let row = parts.next()?.parse().ok()?;
    let url = url::Url::parse(parts.next()?).ok()?;
    let own = matches!(url.host_str(), Some("tauri.localhost" | "localhost")) || url.scheme() == "tauri";
    if !own {
        return None;
    }
    let file = url.path_segments()?.next_back()?.to_string();
    let function: String = function
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_$.<>".contains(*c))
        .take(80)
        .collect();
    Some(Frame {
        function: if function.is_empty() { "<anonymous>".into() } else { function },
        filename: Some(file),
        lineno: Some(row),
        colno: Some(col),
        module: None,
        in_app: true,
    })
}

/// At launch: send what crashed since the last run.
pub fn startup(state: &AppState) {
    let status = analytics::status(state);
    send_panics(state, status);
    send_native_crashes(state, status);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_backtrace_frames() {
        let trace = "   0: std::backtrace::Backtrace::force_capture
   1: std::panicking::rust_panic_with_hook
   2: core::panicking::panic_fmt
   3: core::option::expect_failed
   4: laab_lib::store::Store::chats::h0123456789abcdef
             at /Users/runner/work/laab/desktop/src-tauri/src/store.rs:42:9
   5: line_rs::client::Client::send
             at /Users/runner/.cargo/git/checkouts/line-rs-abc/e52475b/src/client.rs:7:1
   6: tokio::runtime::task::raw::poll";
        let frames = rust_frames(trace);
        assert_eq!(frames[0].function, "laab_lib::store::Store::chats");
        assert_eq!(frames[0].filename.as_deref(), Some("src-tauri/src/store.rs"));
        assert_eq!((frames[0].lineno, frames[0].colno), (Some(42), Some(9)));
        assert!(frames[0].in_app);
        assert_eq!(frames[1].filename.as_deref(), Some("e52475b/src/client.rs"));
        assert!(!frames[1].in_app);
        assert_eq!(frames.len(), 3);
    }

    #[test]
    fn js_frames_keep_only_our_scripts() {
        let e = from_js(
            "TypeError",
            "render@http://tauri.localhost/assets/index-abc.js:12:34\n\
             evil@https://example.com/x.js?user=me:1:2\n\
             http://localhost:1420/src/components/Rail.tsx:5:6",
        );
        assert_eq!(e.kind, "TypeError");
        assert_eq!(e.value, "");
        assert_eq!(e.frames.len(), 2);
        assert_eq!(e.frames[0].function, "render");
        assert_eq!(e.frames[0].filename.as_deref(), Some("index-abc.js"));
        assert_eq!(e.frames[1].filename.as_deref(), Some("Rail.tsx"));
        assert_eq!(from_js("Error: secret text", "").kind, "Error");
    }

    #[test]
    fn ips_keeps_the_crashed_thread_only() {
        let header = r#"{"app_name":"laab","app_version":"0.3.0","bug_type":"309"}"#;
        let body = json!({
            "procPath": "/Users/someone/Applications/LAAB.app/Contents/MacOS/laab",
            "exception": {"type": "EXC_BAD_ACCESS", "signal": "SIGSEGV"},
            "faultingThread": 1,
            "usedImages": [{"name": "laab"}, {"name": "libsystem_kernel.dylib"}],
            "threads": [
                {"frames": [{"imageIndex": 1, "imageOffset": 1, "symbol": "other"}]},
                {"frames": [
                    {"imageIndex": 0, "imageOffset": 4096},
                    {"imageIndex": 1, "imageOffset": 16, "symbol": "__pthread_kill"}
                ]}
            ]
        });
        let e = from_ips(&format!("{header}\n{body}"), "laab").unwrap();
        assert_eq!((e.kind.as_str(), e.value.as_str(), e.version.as_str()), ("EXC_BAD_ACCESS", "SIGSEGV", "0.3.0"));
        assert_eq!(e.frames[0].function, "laab+0x1000");
        assert!(e.frames[0].in_app);
        assert_eq!(e.frames[1].function, "__pthread_kill");
        assert!(!serde_json::to_string(&e).unwrap().contains("someone"));
    }

    #[test]
    fn source_paths_drop_build_folders() {
        assert_eq!(source_path("src/line.rs"), "src/line.rs");
        assert_eq!(
            source_path("/Users/runner/.cargo/registry/src/index/tokio-1.0/src/rt.rs"),
            "tokio-1.0/src/rt.rs"
        );
    }
}
