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
//!   /album/<chatId>/<albumId>/<oid>
//!   /cover/<oid>
//!   /local?path=<absolute path inside the data folder>

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use tauri::http::{Request, Response, StatusCode, header};

use crate::state::AppState;

const STICKER_CDN: &str = "https://stickershop.line-scdn.net/stickershop/v1";
/// `CDN_PROFILE` (`ia0/k`): profile and group pictures.
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
            Some("svg") => "image/svg+xml",
            Some("mp4" | "mov") => "video/mp4",
            Some("m4a") => "audio/mp4",
            Some("pdf") => "application/pdf",
            _ => "application/octet-stream",
        },
    }
}

fn respond(status: StatusCode, body: Vec<u8>, mime: &str) -> Response<Vec<u8>> {
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

fn file_response(path: &Path, range: Option<&str>) -> Option<Response<Vec<u8>>> {
    let mut file = crate::vault::CacheFile::open(path)
        .inspect_err(|e| tracing::warn!("{e:#}"))
        .ok()?;
    let len = file.len();
    if len == 0 {
        return None;
    }
    let mime = sniff_mime(&file.head(16).ok()?, path);
    crate::vault::touch(path);
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
        ["album", chat, album, oid] => crate::actions::album_photo_file(&state, chat, album, oid).await,
        ["cover", oid] => crate::actions::cover_file(&state, oid).await,
        ["note", svc, id] => note_media(&state, svc, id).await,
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
    let range = req
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok());
    match result.ok().and_then(|p| file_response(&p, range)) {
        Some(r) => r,
        None => respond(StatusCode::NOT_FOUND, Vec::new(), "text/plain"),
    }
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
/// and is downloaded, as LINE caches pictures by URL (`mp8/b.k`).
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
        // A group without a picture path: OBS `r/talk/g/{gid}` (`mp8/b.o`).
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
        // CDN (`mp8/b.k`).
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

/// `X-Talk-Meta` for downloading the object of message `id` (line-rs keeps
/// its copy crate-private).
fn talk_meta(message_id: &str) -> Result<String> {
    use base64::Engine;
    use line_rs::thrift::{Protocol, TType, TValue, write_struct};
    let b64 = base64::engine::general_purpose::STANDARD;
    let s = write_struct(
        &[
            (4, TValue::Binary(message_id.as_bytes().to_vec())),
            (27, TValue::List(TType::Struct, Vec::new())),
        ],
        Protocol::Binary,
    )
    .map_err(|e| anyhow!("{e}"))?;
    let json = serde_json::json!({ "message": b64.encode(s) }).to_string();
    Ok(b64.encode(json))
}

/// The thumbnail of an E2EE photo or video. LINE stores it beside the media
/// as `<oid>__ud-preview`, encrypted with the same key; line-rs only fetches
/// the full object, which for a video is the whole file. `None` when the
/// message is not E2EE.
async fn e2ee_preview(
    line: &crate::line::Line,
    raw: &line_rs::thrift::Value,
) -> Result<Option<Vec<u8>>> {
    if raw["chunks"].as_list().is_none_or(|c| c.is_empty()) {
        return Ok(None);
    }
    let base = &line.client.base;
    let payload = base
        .decrypt_e2ee_data_message(raw)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let key = payload["keyMaterial"]
        .as_str()
        .ok_or_else(|| anyhow!("no keyMaterial"))?;
    let meta = &raw["contentMetadata"];
    let oid = meta["OID"].as_str().unwrap_or_default();
    let sid = meta["SID"].as_str().unwrap_or_default();
    let id = raw["id"].as_str().unwrap_or_default();
    let data = base
        .obs()
        .download_object_for_service(
            &format!("talk/{sid}"),
            &format!("{oid}__ud-preview"),
            vec![("X-Talk-Meta".into(), talk_meta(id)?)],
        )
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let plain = line_rs::e2ee::crypto::decrypt_by_key_material_b64(&data, key)
        .map_err(|e| anyhow!("{e}"))?;
    Ok(Some(plain))
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
    let message = line.talk_message(id);
    let e2ee = if preview {
        e2ee_preview(&line, &message.raw).await.unwrap_or_else(|e| {
            tracing::warn!("e2ee preview {id}: {e}");
            None
        })
    } else {
        None
    };
    let data = match e2ee {
        Some(d) => d,
        None => message.data(preview).await.map_err(|e| anyhow!("{e}"))?,
    };
    // A "preview" that is the whole video is kept as the full file instead.
    let full = !preview || sniff_mime(&data, Path::new("")) == "video/mp4";
    let name = if full {
        format!("{}.full", safe_name(id))
    } else {
        format!("{}.preview", safe_name(id))
    };
    let saved = save(&dir.join(name), &data)?;
    crate::cache::schedule_trim(state);
    if preview && full {
        return Err(anyhow!("no thumbnail for {id}"));
    }
    if !preview {
        let _ = state
            .store()
            .update_media(id, Some(&saved.to_string_lossy()), None);
    }
    Ok(saved)
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

/// A note photo (`h`) or comment image (`cmt`) from OBS `myhome/<svc>`.
async fn note_media(state: &Arc<AppState>, svc: &str, oid: &str) -> Result<PathBuf> {
    if !matches!(svc, "h" | "cmt") {
        return Err(anyhow!("unknown note media"));
    }
    let path = state
        .paths
        .media()
        .join(format!("note-{svc}-{}", safe_name(oid)));
    if path.exists() {
        return Ok(path);
    }
    let line = state.line().await.ok_or_else(|| anyhow!("not_logged_in"))?;
    let data = line
        .client
        .base
        .obs()
        .download_object_for_service(&format!("myhome/{svc}"), oid, vec![])
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let saved = save(&path, &data)?;
    crate::cache::schedule_trim(state);
    Ok(saved)
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
    let data = line
        .talk_message(id)
        .data(false)
        .await
        .map_err(crate::line::media_error)?;
    if data.is_empty() {
        return Err(anyhow!("download_failed: empty file"));
    }
    let name = file_name
        .filter(|n| !n.is_empty() && !n.starts_with('['))
        .unwrap_or_else(|| format!("file.{}", extension_of(&data)));
    let clean = clean_file_name(&name);
    let path = state
        .paths
        .media()
        .join(format!("{}.{clean}", safe_name(id)));
    save(&path, &data)?;
    crate::cache::schedule_trim(state);
    state
        .store()
        .remember_download(id, &path.to_string_lossy(), Some(&name))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_queries() {
        assert_eq!(decode("%2F0hAbc%3D"), "/0hAbc=");
        assert_eq!(
            url_query("p=%2Fa&x=1"),
            vec![("p".into(), "/a".into()), ("x".into(), "1".into())]
        );
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
