//! The `media://` protocol: avatars, message images, stickers and pack icons,
//! served from the disk cache and downloaded on first use. Avatars and
//! message media are encrypted on disk (`vault.rs`); stickers are public and
//! stay plain.
//!
//! URLs (`media://localhost/...` on macOS):
//!   /avatar/<mid>?p=<picturePath>
//!   /message/<chatId>/<messageId>?preview=1
//!   /sticker/<stickerId>?anim=1
//!   /pack/<packageId>
//!   /sticon/<productId>/<sticonId>?v=<version>&anim=1
//!   /album/<chatId>/<albumId>/<oid>?video=1
//!   /cover/<service/namespace/oid, as one segment>
//!   /note/<service>/<namespace>/<oid>
//!   /local?path=<absolute path inside the data folder>

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use tauri::http::{Request, Response, StatusCode, header};

use crate::state::AppState;

const STICKER_CDN: &str = "https://stickershop.line-scdn.net/stickershop/v1";
/// Profile and group pictures.
const PROFILE_CDN: &str = "https://profile.line-scdn.net";

fn query(req: &Request<Vec<u8>>, key: &str) -> Option<String> {
    let q = req.uri().query()?;
    url_query(q)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

pub fn url_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(k), decode(v))
        })
        .collect()
}

pub fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                {
                    out.push(b);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn sniff_mime(data: &[u8], path: &Path) -> &'static str {
    match data {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [b'G', b'I', b'F', ..] => "image/gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "image/webp",
        [_, _, _, _, b'f', b't', b'y', b'p', b'h', b'e', b'i', b'c', ..]
        | [_, _, _, _, b'f', b't', b'y', b'p', b'h', b'e', b'i', b'x', ..]
        | [_, _, _, _, b'f', b't', b'y', b'p', b'm', b'i', b'f', b'1', ..] => "image/heic",
        // Every other ISO media brand (isom, mp42, M4V, 3gp4, qt, dash…).
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => "video/mp4",
        _ => match path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("heic" | "heif") => "image/heic",
            Some("svg") => "image/svg+xml",
            Some("mp4" | "mov") => "video/mp4",
            Some("m4a") => "audio/mp4",
            Some("pdf") => "application/pdf",
            _ => "application/octet-stream",
        },
    }
}

/// WebKit on some macOS versions cannot display HEIC from a custom protocol,
/// so HEIC photos are served as a JPEG made once with the system decoder and
/// kept, encrypted like the original, beside it in the cache.
fn heic_as_jpeg(path: &Path, data: &[u8]) -> Option<Vec<u8>> {
    let mut name = path.file_name()?.to_os_string();
    name.push(".jpg");
    let cached = path.with_file_name(name);
    if let Ok(jpeg) = crate::vault::read(&cached) {
        crate::vault::touch(&cached);
        return Some(jpeg);
    }
    let jpeg = convert_heic(data)?;
    if let Err(e) = crate::vault::write(&cached, &jpeg) {
        tracing::warn!("heic cache: {e:#}");
    }
    Some(jpeg)
}

/// `sips` only works on files: the plain HEIC and JPEG live for the length of
/// the call in a fresh private (0700) folder, removed even on an early return.
#[cfg(target_os = "macos")]
fn convert_heic(data: &[u8]) -> Option<Vec<u8>> {
    let dir = tempfile::Builder::new().prefix("laab-heic-").tempdir().ok()?;
    let input = dir.path().join("in.heic");
    let output = dir.path().join("out.jpg");
    std::fs::write(&input, data).ok()?;
    std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "jpeg"])
        .arg(&input)
        .arg("--out")
        .arg(&output)
        .output()
        .ok()
        .filter(|r| r.status.success())
        .and_then(|_| std::fs::read(&output).ok())
}

#[cfg(not(target_os = "macos"))]
fn convert_heic(_: &[u8]) -> Option<Vec<u8>> {
    None
}

pub(crate) fn respond(status: StatusCode, body: Vec<u8>, mime: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "max-age=86400")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(body)
        .unwrap()
}

/// `bytes=a-b`, `bytes=a-` or `bytes=-n` against a file of `len` bytes, as an
/// inclusive range. WebKit plays `<video>` only through range requests.
pub fn byte_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header
        .trim()
        .strip_prefix("bytes=")?
        .split(',')
        .next()?
        .trim();
    let (a, b) = spec.split_once('-')?;
    let (start, end) = match (a.trim(), b.trim()) {
        ("", n) => (len.checked_sub(n.parse().ok()?)?, len.checked_sub(1)?),
        (a, "") => (a.parse().ok()?, len.checked_sub(1)?),
        (a, b) => (
            a.parse().ok()?,
            b.parse::<u64>().ok()?.min(len.checked_sub(1)?),
        ),
    };
    (start <= end && end < len).then_some((start, end))
}

/// The picture of a file dragged onto a chat, for the tray's thumbnail. Only
/// paths `dropped_files` was given, only images, read as they are: the
/// user's file is not touched and nothing is written beside it.
async fn dropped(state: &Arc<AppState>, path: String) -> Response<Vec<u8>> {
    let path = PathBuf::from(path);
    let allowed = state
        .dropped
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&path);
    let missing = || respond(StatusCode::NOT_FOUND, Vec::new(), "text/plain");
    if !allowed {
        return missing();
    }
    tokio::task::spawn_blocking(move || {
        let meta = std::fs::metadata(&path).ok()?;
        if meta.len() > 50 * 1024 * 1024 {
            return None;
        }
        let data = std::fs::read(&path).ok()?;
        let mime = sniff_mime(&data[..data.len().min(16)], &path);
        mime.starts_with("image/").then(|| {
            // Only LAAB's own pages may read the user's files, and not from cache.
            let mut res = respond(StatusCode::OK, data, mime);
            res.headers_mut().remove(header::ACCESS_CONTROL_ALLOW_ORIGIN);
            res.headers_mut().insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            res
        })
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_else(missing)
}

pub(crate) fn file_response(path: &Path, range: Option<&str>) -> Option<Response<Vec<u8>>> {
    let mut file = crate::vault::CacheFile::open(path)
        .inspect_err(|e| tracing::warn!("{e:#}"))
        .ok()?;
    let len = file.len();
    if len == 0 {
        return None;
    }
    let mime = sniff_mime(&file.head(16).ok()?, path);
    crate::vault::touch(path);
    if mime == "image/heic" && range.is_none() {
        let data = file.read_all().ok()?;
        if let Some(jpeg) = heic_as_jpeg(path, &data) {
            return Some(respond(StatusCode::OK, jpeg, "image/jpeg"));
        }
    }
    let Some((start, end)) = range.and_then(|r| byte_range(r, len)) else {
        let data = file.read_all().ok()?;
        let mut res = respond(StatusCode::OK, data, mime);
        res.headers_mut()
            .insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
        return Some(res);
    };
    let data = file.read_range(start, end).ok()?;
    let mut res = respond(StatusCode::PARTIAL_CONTENT, data, mime);
    let h = res.headers_mut();
    h.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    h.insert(
        header::CONTENT_RANGE,
        format!("bytes {start}-{end}/{len}").parse().unwrap(),
    );
    Some(res)
}

pub async fn handle(state: Arc<AppState>, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = req.uri().path().to_string();
    let parts: Vec<String> = path
        .trim_start_matches('/')
        .split('/')
        .map(decode)
        .collect();
    let range = req
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok());
    if let [kind, chat, id] = parts.as_slice()
        && kind == "message"
        && query(&req, "preview").is_none()
        && let Some(res) = stream_video(&state, chat, id, range).await
    {
        return res;
    }
    if let [kind] = parts.as_slice()
        && kind == "dropped"
    {
        return dropped(&state, query(&req, "path").unwrap_or_default()).await;
    }
    let result = match parts
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["avatar", mid] => avatar(&state, mid, query(&req, "p")).await,
        ["message", chat, id] => {
            message_media(&state, chat, id, query(&req, "preview").is_some()).await
        }
        ["sticker", id] => sticker(&state, id, query(&req, "anim").is_some()).await,
        ["pack", id] => pack_icon(&state, id).await,
        ["sticon", product, id] => {
            let version = query(&req, "v").and_then(|v| v.parse().ok()).unwrap_or(1);
            sticon(&state, product, id, version, query(&req, "anim").is_some()).await
        }
        ["album", chat, album, oid] => {
            let video = query(&req, "video").is_some();
            crate::actions::album_photo_file(&state, chat, album, oid, video).await
        }
        ["cover", oid] => crate::actions::cover_file(&state, oid).await,
        ["note", svc @ ("h" | "cmt"), id] => note_media(&state, &format!("myhome/{svc}"), id).await,
        ["note", service, namespace, id] => {
            note_media(&state, &format!("{service}/{namespace}"), id).await
        }
        ["local"] => {
            let p = PathBuf::from(query(&req, "path").unwrap_or_default());
            if p.starts_with(&state.paths.root) {
                Ok(p)
            } else {
                Err(anyhow!("outside data folder"))
            }
        }
        _ => Err(anyhow!("unknown media path")),
    };
    // Decrypting, and converting HEIC, is blocking work: keep it off the
    // runtime so one large file does not hold up every other request.
    let range = range.map(str::to_owned);
    let response = match result {
        Ok(p) => tokio::task::spawn_blocking(move || file_response(&p, range.as_deref()))
            .await
            .ok()
            .flatten(),
        Err(_) => None,
    };
    response.unwrap_or_else(|| respond(StatusCode::NOT_FOUND, Vec::new(), "text/plain"))
}

async fn fetch(url: &str) -> Result<Vec<u8>> {
    let res = reqwest::get(url).await?;
    if !res.status().is_success() {
        return Err(anyhow!("{} {url}", res.status()));
    }
    let data = res.bytes().await?.to_vec();
    if data.is_empty() {
        return Err(anyhow!("empty body"));
    }
    Ok(data)
}

/// Save to the cache, encrypted.
fn save(path: &Path, data: &[u8]) -> Result<PathBuf> {
    crate::vault::write(path, data)?;
    Ok(path.to_path_buf())
}

/// Stickers and pack icons are public; they stay plain.
fn save_plain(path: &Path, data: &[u8]) -> Result<PathBuf> {
    crate::paths::write_private(path, data)?;
    Ok(path.to_path_buf())
}

/// The extension for a file that starts with `head`.
pub fn extension_of(head: &[u8]) -> &'static str {
    // An ISO media file names what it is in its `ftyp` brand.
    if let [_, _, _, _, b'f', b't', b'y', b'p', brand @ ..] = head
        && brand.len() >= 4
    {
        match &brand[..4] {
            b"heic" | b"heix" | b"mif1" | b"msf1" => return "heic",
            b"qt  " => return "mov",
            b"M4A " => return "m4a",
            _ => {}
        }
    }
    match sniff_mime(head, Path::new("")) {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        _ if head.starts_with(b"%PDF") => "pdf",
        _ if head.starts_with(b"PK") => "zip",
        _ => "bin",
    }
}

/// A file name as the user sees it, safe to save under: only `/` and `:`
/// (the path separators of macOS) become `_`, so the extension stays.
pub fn clean_file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if matches!(c, '/' | ':' | '\0') { '_' } else { c })
        .collect();
    match clean.trim() {
        "" | "." | ".." => "file".into(),
        _ => clean,
    }
}

/// The name to save a message's attachment under: its own file name, or
/// `LINE_<id>.<ext>` with the extension of the cached file's content
/// (never the cache's own `.full`).
pub fn save_name(file_name: Option<&str>, message_id: &str, cached: &Path) -> String {
    match file_name.filter(|n| !n.trim().is_empty() && !n.starts_with('[')) {
        Some(n) => clean_file_name(n),
        None => {
            let head = crate::vault::CacheFile::open(cached)
                .and_then(|mut f| f.head(16))
                .unwrap_or_default();
            format!("LINE_{}.{}", safe_name(message_id), extension_of(&head))
        }
    }
}

pub fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The cache file of `mid`'s picture at `picture` (its `picturePath`).
/// The name carries a hash of the path, so a new picture gets a new file
/// and is downloaded.
fn avatar_file(dir: &Path, mid: &str, picture: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let hash = hex::encode(&Sha256::digest(picture.as_bytes())[..6]);
    dir.join(format!("{}-{hash}.jpg", safe_name(mid)))
}

/// `mid`'s other cached pictures: older pictures and the unhashed
/// `<mid>.jpg` of earlier builds.
fn stale_avatars(dir: &Path, mid: &str, keep: &Path) -> Vec<PathBuf> {
    let id = safe_name(mid);
    let (hashed, legacy) = (format!("{id}-"), format!("{id}.jpg"));
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p != keep && !crate::vault::is_temp(p))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name == legacy
                || name
                    .strip_prefix(&hashed)
                    .is_some_and(|rest| rest.len() == 16 && rest.ends_with(".jpg"))
        })
        .collect()
}

async fn avatar(state: &AppState, mid: &str, picture: Option<String>) -> Result<PathBuf> {
    let dir = state.paths.avatars();
    // Demo pictures.
    for ext in ["png", "svg"] {
        let other = dir.join(format!("{}.{ext}", safe_name(mid)));
        if other.exists() {
            return Ok(other);
        }
    }
    let picture = picture.unwrap_or_default();
    let path = avatar_file(&dir, mid, &picture);
    if path.exists() {
        return Ok(path);
    }
    if state.demo {
        return Err(anyhow!("demo"));
    }
    let data = if !picture.is_empty() {
        let p = if picture.starts_with('/') {
            picture
        } else {
            format!("/{picture}")
        };
        fetch_preview(&format!("{PROFILE_CDN}{p}")).await?
    } else if mid.starts_with('c') {
        // A group without a picture path: OBS `r/talk/g/{gid}`.
        let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
        let obs = line.client.base.obs();
        match obs
            .download_object_for_service("talk/g", &format!("{mid}/preview"), vec![])
            .await
        {
            Ok(d) if !d.is_empty() => d.to_vec(),
            _ => obs
                .download_object_for_service("talk/g", mid, vec![])
                .await
                .map_err(|e| anyhow!("{e}"))?
                .to_vec(),
        }
    } else {
        // A user without a picture path: `r/talk/p/{mid}` on the profile
        // CDN.
        fetch_preview(&format!("{PROFILE_CDN}/r/talk/p/{}", safe_name(mid))).await?
    };
    let saved = save(&path, &data)?;
    for old in stale_avatars(&dir, mid, &saved) {
        let _ = std::fs::remove_file(old);
    }
    let _ = state.store().set_avatar(mid, &saved.to_string_lossy());
    Ok(saved)
}

/// `{base}/preview`, else `base` itself.
async fn fetch_preview(base: &str) -> Result<Vec<u8>> {
    match fetch(&format!("{base}/preview")).await {
        Ok(d) => Ok(d),
        Err(_) => fetch(base).await,
    }
}

/// Whether a cached file is a video. A preview must be a picture.
fn is_video(path: &Path) -> bool {
    crate::vault::CacheFile::open(path)
        .and_then(|mut f| f.head(12))
        .is_ok_and(|head| head.len() == 12 && sniff_mime(&head, Path::new("")) == "video/mp4")
}

/// A message's cached thumbnail (`preview`) or full file. Earlier builds
/// saved a whole E2EE video as its preview; that file becomes the full one.
fn cached_media(dir: &Path, id: &str, preview: bool) -> Option<PathBuf> {
    let id = safe_name(id);
    let thumb_jpg = dir.join(format!("{id}.thumb.jpg"));
    let thumb = dir.join(format!("{id}.preview"));
    let full = dir.join(format!("{id}.full"));
    if thumb.exists() && is_video(&thumb) {
        let _ = if full.exists() {
            std::fs::remove_file(&thumb)
        } else {
            std::fs::rename(&thumb, &full)
        };
    }
    let full = full.exists().then_some(full);
    if !preview {
        return full;
    }
    [thumb_jpg, thumb]
        .into_iter()
        .find(|p| p.exists())
        .or_else(|| full.filter(|p| !is_video(p)))
}

async fn message_media(
    state: &Arc<AppState>,
    chat: &str,
    id: &str,
    preview: bool,
) -> Result<PathBuf> {
    let dir = state.paths.media();
    if let Some(p) = cached_media(&dir, id, preview) {
        return Ok(p);
    }
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    crate::line::ensure_media_message(state, &line, chat, id).await?;
    let ext = if preview { "preview" } else { "full" };
    let target = dir.join(format!("{}.{ext}", safe_name(id)));
    // Streamed to disk: a video never sits in memory whole. An E2EE
    // thumbnail is the `ud-preview` the sender uploaded; line-rs never
    // downloads a whole video for it.
    let mut file = crate::vault::Writer::create(&target)?;
    line.talk_message(id)
        .download_to(preview, &mut file)
        .await
        .map_err(crate::line::media_error)?;
    let video = sniff_mime(file.head(), Path::new("")) == "video/mp4";
    if preview && video {
        // Not a thumbnail: keep it as the full file.
        let full = dir.join(format!("{}.full", safe_name(id)));
        file.finish_as(&full)?;
        crate::cache::schedule_trim(state);
        return Err(anyhow!("no thumbnail for {id}"));
    }
    let saved = file.finish()?;
    crate::cache::schedule_trim(state);
    if !preview {
        let _ = state
            .store()
            .update_media(id, Some(&saved.to_string_lossy()), None);
    }
    Ok(saved)
}

/// A video message that isn't cached yet plays while it downloads (see
/// `stream.rs`). `None` for anything else.
async fn stream_video(
    state: &Arc<AppState>,
    chat: &str,
    id: &str,
    range: Option<&str>,
) -> Option<Response<Vec<u8>>> {
    if state.demo || cached_media(&state.paths.media(), id, false).is_some() {
        return None;
    }
    let message = state.store().message(id).ok().flatten()?;
    if message.content_type != "VIDEO" {
        return None;
    }
    let line = state.line().await?;
    let not_found = || respond(StatusCode::NOT_FOUND, Vec::new(), "text/plain");
    if let Err(e) = crate::line::ensure_media_message(state, &line, chat, id).await {
        tracing::debug!("video {id}: {e:#}");
        return Some(not_found());
    }
    let target = state.paths.media().join(format!("{}.full", safe_name(id)));
    Some(
        crate::stream::serve(state, &line, id, &target, range)
            .await
            .unwrap_or_else(not_found),
    )
}

async fn sticker(state: &AppState, id: &str, anim: bool) -> Result<PathBuf> {
    let dir = state.paths.stickers();
    let id = safe_name(id);
    let still = dir.join(format!("{id}.png"));
    if anim {
        let path = dir.join(format!("{id}.anim.png"));
        let miss = dir.join(format!("{id}.anim.miss"));
        if path.exists() {
            return Ok(path);
        }
        if !miss.exists() && !state.demo {
            match fetch(&format!(
                "{STICKER_CDN}/sticker/{id}/android/sticker_animation.png"
            ))
            .await
            {
                Ok(data) => return save_plain(&path, &data),
                Err(_) => {
                    let _ = std::fs::write(&miss, b"");
                }
            }
        }
    }
    if still.exists() {
        return Ok(still);
    }
    if state.demo {
        return Err(anyhow!("demo"));
    }
    for url in [
        format!("{STICKER_CDN}/sticker/{id}/android/sticker.png"),
        format!("{STICKER_CDN}/sticker/{id}/iPhone/sticker@2x.png"),
        format!("{STICKER_CDN}/sticker/{id}/ANDROID/sticker.png"),
    ] {
        if let Ok(data) = fetch(&url).await {
            return save_plain(&still, &data);
        }
    }
    Err(anyhow!("sticker unavailable"))
}

/// Where a LINE emoji's picture is on the sticker CDN: LINE for Android's
/// `sticonshop/v1/<product>/sticon/android/<id>[_animation].png?v=<version>`
/// (`ev5/m.java`), then the order line-rs and older clients use (which of
/// the two the CDN serves is unverified).
fn sticon_urls(product: &str, id: &str, version: u64, anim: bool) -> [String; 2] {
    let file = if anim { format!("{id}_animation.png") } else { format!("{id}.png") };
    [
        format!("https://stickershop.line-scdn.net/sticonshop/v1/{product}/sticon/android/{file}?v={version}"),
        format!("https://stickershop.line-scdn.net/sticonshop/v1/sticon/{product}/android/{file}"),
    ]
}

/// A LINE emoji's picture; public like stickers, so it stays plain.
async fn sticon(state: &AppState, product: &str, id: &str, version: u64, anim: bool) -> Result<PathBuf> {
    let (product, id) = (safe_name(product), safe_name(id));
    let suffix = if anim { ".anim" } else { "" };
    let path = state
        .paths
        .stickers()
        .join(format!("sticon-{product}-{id}-{version}{suffix}.png"));
    if path.exists() {
        return Ok(path);
    }
    if state.demo {
        return Err(anyhow!("demo"));
    }
    for url in sticon_urls(&product, &id, version, anim) {
        if let Ok(data) = fetch(&url).await {
            return save_plain(&path, &data);
        }
    }
    Err(anyhow!("sticon unavailable"))
}

async fn pack_icon(state: &AppState, id: &str) -> Result<PathBuf> {
    let path = state
        .paths
        .stickers()
        .join(format!("pack-{}.png", safe_name(id)));
    if path.exists() {
        return Ok(path);
    }
    let data = fetch(&format!(
        "{STICKER_CDN}/product/{}/android/main.png",
        safe_name(id)
    ))
    .await?;
    save_plain(&path, &data)
}

/// A note photo or comment image: OBS `base/oid`, where `base` is the
/// `service/namespace` the post names (`myhome/h` and `myhome/cmt` by
/// default).
async fn note_media(state: &Arc<AppState>, base: &str, oid: &str) -> Result<PathBuf> {
    let path = state
        .paths
        .media()
        .join(format!("note-{}-{}", safe_name(base), safe_name(oid)));
    if path.exists() {
        return Ok(path);
    }
    let saved = home_obs_file(state, base, oid, &path).await?;
    crate::cache::schedule_trim(state);
    Ok(saved)
}

/// The MyHome channel token, issued once per session.
static HOME_TOKEN: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);

async fn home_channel_token(line: &crate::line::Line) -> Option<String> {
    let session = line.client.base.auth_token()?;
    if let Some((s, token)) = HOME_TOKEN.lock().unwrap().as_ref()
        && *s == session
    {
        return Some(token.clone());
    }
    match line
        .client
        .base
        .rest()
        .channel_token(line_rs::rest::channels::MYHOME)
        .await
    {
        Ok(token) => {
            *HOME_TOKEN.lock().unwrap() = Some((session, token.clone()));
            Some(token)
        }
        Err(e) => {
            tracing::warn!("home channel token: {e}");
            None
        }
    }
}

/// Stream note, comment or cover media from OBS `base/oid` (`base` is
/// `service/namespace`) into `dest`, with `X-Line-ChannelToken`, which
/// LINE asks for with home images.
pub async fn home_obs_file(
    state: &Arc<AppState>,
    base: &str,
    oid: &str,
    dest: &Path,
) -> Result<PathBuf> {
    let parts: Vec<&str> = base.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| p.is_empty() || safe_name(p) != *p)
        || oid.is_empty()
    {
        return Err(anyhow!("unknown note media"));
    }
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    let headers = match home_channel_token(&line).await {
        Some(token) => vec![("X-Line-ChannelToken".to_string(), token)],
        None => Vec::new(),
    };
    let mut file = crate::vault::Writer::create(dest)?;
    line.client
        .base
        .obs()
        .download_object_to(base, oid, headers, &mut file)
        .await
        .map_err(crate::line::media_error)?;
    file.finish()
}

/// Download an attachment to `cache/media/<id>.<filename>` and record it.
pub async fn download_file(
    state: &Arc<AppState>,
    chat: &str,
    id: &str,
    file_name: Option<String>,
) -> Result<PathBuf> {
    if let Some(Some(p)) = state.store().message(id)?.map(|m| m.file_path)
        && Path::new(&p).exists()
    {
        return Ok(PathBuf::from(p));
    }
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    crate::line::ensure_media_message(state, &line, chat, id).await?;
    let dir = state.paths.media();
    let mut file = crate::vault::Writer::create(&dir.join(format!("{}.download", safe_name(id))))?;
    line.talk_message(id)
        .download_to(false, &mut file)
        .await
        .map_err(crate::line::media_error)?;
    if file.len() == 0 {
        return Err(anyhow!("download_failed: empty file"));
    }
    let name = file_name
        .filter(|n| !n.is_empty() && !n.starts_with('['))
        .unwrap_or_else(|| format!("file.{}", extension_of(file.head())));
    let clean = clean_file_name(&name);
    let path = file.finish_as(&dir.join(format!("{}.{clean}", safe_name(id))))?;
    crate::cache::schedule_trim(state);
    state
        .store()
        .remember_download(id, &path.to_string_lossy(), Some(&name))?;
    // So file bubbles flip from "คลิกเพื่อดาวน์โหลด" to "คลิกเพื่อเปิด" / Finder.
    state.messages_changed(chat);
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_heic_apart_from_other_iso_media() {
        let head = |brand: &[u8; 4]| [&[0, 0, 0, 0x18][..], b"ftyp", brand].concat();
        let none = Path::new("");
        assert_eq!(sniff_mime(&head(b"heic"), none), "image/heic");
        assert_eq!(sniff_mime(&head(b"mif1"), none), "image/heic");
        for brand in [b"isom", b"M4V ", b"iso5", b"3gp4", b"dash"] {
            assert_eq!(sniff_mime(&head(brand), none), "video/mp4");
        }
    }

    #[test]
    fn decodes_queries() {
        assert_eq!(decode("%2F0hAbc%3D"), "/0hAbc=");
        assert_eq!(
            url_query("p=%2Fa&x=1"),
            vec![("p".into(), "/a".into()), ("x".into(), "1".into())]
        );
    }

    #[test]
    fn sticon_pictures_follow_the_apps_path_first() {
        let [app, old] = sticon_urls("5ac1", "001", 3, true);
        assert_eq!(
            app,
            "https://stickershop.line-scdn.net/sticonshop/v1/5ac1/sticon/android/001_animation.png?v=3"
        );
        assert_eq!(old, "https://stickershop.line-scdn.net/sticonshop/v1/sticon/5ac1/android/001_animation.png");
        assert!(sticon_urls("5ac1", "001", 1, false)[0].ends_with("/001.png?v=1"));
    }

    #[test]
    fn saved_names_keep_their_extension() {
        assert_eq!(clean_file_name("report v2.pdf"), "report v2.pdf");
        assert_eq!(clean_file_name("a/b:c.tar.gz"), "a_b_c.tar.gz");
        assert_eq!(clean_file_name(".."), "file");
        let dir = tempfile::tempdir().unwrap();
        let full = dir.path().join("7.full");
        std::fs::write(&full, b"\x89PNG\r\n\x1a\nrest").unwrap();
        assert_eq!(save_name(None, "7", &full), "LINE_7.png");
        assert_eq!(save_name(Some("[Photo]"), "7", &full), "LINE_7.png");
        assert_eq!(save_name(Some("notes.txt"), "7", &full), "notes.txt");
        std::fs::write(&full, b"\0\0\0\x18ftypmp42").unwrap();
        assert_eq!(save_name(None, "7", &full), "LINE_7.mp4");
        for (head, ext) in [
            (&b"GIF89a\x01\0"[..], "gif"),
            (b"\xff\xd8\xff\xe0", "jpg"),
            (b"RIFF\0\0\0\0WEBPVP8 ", "webp"),
            (b"\0\0\0\x18ftypheic\0\0", "heic"),
            (b"\0\0\0\x14ftypqt  \0\0", "mov"),
            (b"\0\0\0\x20ftypM4A \0\0", "m4a"),
        ] {
            std::fs::write(&full, head).unwrap();
            assert_eq!(save_name(None, "7", &full), format!("LINE_7.{ext}"));
        }
    }

    #[test]
    fn a_new_picture_gets_a_new_avatar_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = avatar_file(dir.path(), "u1", "/0hAAA");
        let b = avatar_file(dir.path(), "u1", "/0hBBB");
        assert_ne!(a, b);
        assert_eq!(a, avatar_file(dir.path(), "u1", "/0hAAA"));
        assert_ne!(avatar_file(dir.path(), "u1", ""), a);
        for p in [&a, &b] {
            std::fs::write(p, b"x").unwrap();
        }
        for name in ["u1.jpg", "u10-0123456789ab.jpg", "u1.png", "cover-u1.jpg"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let mut stale = stale_avatars(dir.path(), "u1", &b);
        stale.sort();
        let mut want = vec![a, dir.path().join("u1.jpg")];
        want.sort();
        assert_eq!(stale, want);
    }

    #[test]
    fn a_video_is_never_a_preview() {
        let dir = tempfile::tempdir().unwrap();
        let mp4 = b"\0\0\0\x18ftypmp42rest";
        std::fs::write(dir.path().join("7.preview"), mp4).unwrap();
        // The old whole-video "preview" moves to the full file.
        assert_eq!(cached_media(dir.path(), "7", true), None);
        assert_eq!(
            cached_media(dir.path(), "7", false),
            Some(dir.path().join("7.full"))
        );
        // A photo's thumbnail is not its full file.
        std::fs::write(dir.path().join("8.preview"), b"\xff\xd8\xffjpeg").unwrap();
        assert_eq!(
            cached_media(dir.path(), "8", true),
            Some(dir.path().join("8.preview"))
        );
        assert_eq!(cached_media(dir.path(), "8", false), None);
    }

    #[test]
    fn parses_byte_ranges() {
        assert_eq!(byte_range("bytes=0-1", 10), Some((0, 1)));
        assert_eq!(byte_range("bytes=4-", 10), Some((4, 9)));
        assert_eq!(byte_range("bytes=-3", 10), Some((7, 9)));
        assert_eq!(byte_range("bytes=0-99", 10), Some((0, 9)));
        assert_eq!(byte_range("bytes=10-", 10), None);
        assert_eq!(byte_range("items=0-1", 10), None);
    }
}
