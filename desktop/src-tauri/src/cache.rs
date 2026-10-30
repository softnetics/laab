//! Cache housekeeping: the size limit on `cache/media`, clearing the cache,
//! and plain copies of encrypted files for other apps.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

use anyhow::{Result, anyhow};

use crate::state::{AppState, now_ms};
use crate::vault;

/// `cache/media` (photos, videos, files) is trimmed back to `TARGET` when it
/// grows past this. Thumbnails go last.
pub const LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const TARGET: u64 = LIMIT / 10 * 8;

static TRIMMING: AtomicBool = AtomicBool::new(false);
static LAST_TRIM_MS: AtomicU64 = AtomicU64::new(0);

fn files(dir: &Path) -> Vec<(PathBuf, std::fs::Metadata)> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| Some((e.path(), e.metadata().ok()?)))
        .filter(|(p, m)| m.is_file() && !vault::is_temp(p))
        .collect()
}

fn is_thumbnail(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".preview") || name.ends_with(".thumb.jpg")
}

/// Bytes used by `cache/` (media, avatars and stickers).
pub fn usage(state: &AppState) -> u64 {
    [state.paths.media(), state.paths.avatars(), state.paths.stickers()]
        .iter()
        .flat_map(|d| files(d))
        .map(|(_, m)| m.len())
        .sum()
}

/// Delete the least recently used media until it fits under `TARGET`.
/// Messages that pointed at a deleted file download it again when opened.
pub fn trim(state: &AppState) -> Result<usize> {
    let doomed = evictions(files(&state.paths.media()), LIMIT, TARGET);
    let mut removed = 0;
    for path in &doomed {
        if std::fs::remove_file(path).is_ok() {
            removed += 1;
            state.store().forget_file(&path.to_string_lossy())?;
        }
    }
    if removed > 0 {
        tracing::info!("cache trimmed: {removed} files");
    }
    Ok(removed)
}

/// Which files to delete to get from over `limit` down to `target`: full
/// files before thumbnails, least recently used first.
fn evictions(mut list: Vec<(PathBuf, std::fs::Metadata)>, limit: u64, target: u64) -> Vec<PathBuf> {
    let mut total: u64 = list.iter().map(|(_, m)| m.len()).sum();
    if total <= limit {
        return Vec::new();
    }
    let modified = |m: &std::fs::Metadata| m.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    list.sort_by(|(a, am), (b, bm)| {
        is_thumbnail(a)
            .cmp(&is_thumbnail(b))
            .then(modified(am).cmp(&modified(bm)))
    });
    let mut doomed = Vec::new();
    for (path, meta) in list {
        if total <= target {
            break;
        }
        total -= meta.len();
        doomed.push(path);
    }
    doomed
}

/// Trim in the background after new media, at most once a minute.
pub fn schedule_trim(state: &Arc<AppState>) {
    let now = now_ms() as u64;
    if now.saturating_sub(LAST_TRIM_MS.load(Ordering::Relaxed)) < 60_000
        || TRIMMING.swap(true, Ordering::Acquire)
    {
        return;
    }
    LAST_TRIM_MS.store(now, Ordering::Relaxed);
    let state = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = trim(&state) {
            tracing::warn!("cache trim failed: {e:?}");
        }
        TRIMMING.store(false, Ordering::Release);
    });
}

/// Delete every cached file and the paths the database kept to them.
pub fn clear(state: &AppState) -> Result<()> {
    for dir in [state.paths.media(), state.paths.avatars(), state.paths.stickers()] {
        for (path, _) in files(&dir) {
            let _ = std::fs::remove_file(path);
        }
    }
    state.store().forget_all_files()?;
    clear_open_copies();
    state.chats_changed();
    Ok(())
}

/// Encrypt files from before encryption, then apply the limit. Runs once in
/// the background at launch.
pub fn startup(state: Arc<AppState>) {
    clear_open_copies();
    tauri::async_runtime::spawn_blocking(move || {
        match vault::encrypt_existing(&[state.paths.media(), state.paths.avatars()]) {
            Ok(0) => {}
            Ok(n) => tracing::info!("encrypted {n} cached files"),
            Err(e) => tracing::warn!("encrypting the cache stopped: {e:?}"),
        }
        if let Err(e) = trim(&state) {
            tracing::warn!("cache trim failed: {e:?}");
        }
    });
}

fn open_dir() -> PathBuf {
    std::env::temp_dir().join("laab-open")
}

/// Plain copies made for other apps live until LAAB quits or starts again.
pub fn clear_open_copies() {
    let _ = std::fs::remove_dir_all(open_dir());
}

/// A file another app can read: the cache file itself if it is plain, else a
/// decrypted copy in a private temp folder, named like the original.
pub fn plain_copy(path: &Path) -> Result<PathBuf> {
    if !vault::is_encrypted(path) {
        return Ok(path.to_path_buf());
    }
    let dir = open_dir().join(format!("{}", now_ms() as u64));
    crate::paths::private_dir(&dir)?;
    let dest = dir.join(display_name(path)?);
    vault::export(path, &dest)?;
    Ok(dest)
}

/// A plain copy in Downloads (made unique), for "show in Finder".
pub fn export_to_downloads(path: &Path) -> Result<PathBuf> {
    let dir = dirs::download_dir().ok_or_else(|| anyhow!("ไม่พบโฟลเดอร์ดาวน์โหลด"))?;
    let dest = unique(&dir.join(display_name(path)?));
    vault::export(path, &dest)?;
    Ok(dest)
}

fn unique(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|e| e.to_str());
    (2..)
        .map(|n| match ext {
            Some(e) => path.with_file_name(format!("{stem} ({n}).{e}")),
            None => path.with_file_name(format!("{stem} ({n})")),
        })
        .find(|p| !p.exists())
        .expect("an unused name")
}

/// The name a user should see for a cache file: `<id>.<name>` downloads keep
/// their name; `<id>.full` and thumbnails become `LINE_<id>.<ext>`; other
/// files keep theirs, with an extension added from their content.
fn display_name(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("bad cache path"))?;
    let head = vault::CacheFile::open(path)?.head(16)?;
    let ext = extension(crate::media::sniff_mime(&head, path));
    let (stem, rest) = name.split_once('.').unwrap_or((name, ""));
    let id = !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit());
    Ok(match rest {
        "" | "full" | "preview" | "thumb.jpg" if id => format!("LINE_{stem}.{ext}"),
        rest if id => rest.to_string(),
        "" => format!("{name}.{ext}"),
        _ => name.to_string(),
    })
}

fn extension(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "video/mp4" => "mp4",
        "audio/mp4" => "m4a",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_cache_files_for_people() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = b"\xff\xd8\xffjpeg-data-here";
        for (file, want) in [
            ("123.full", "LINE_123.jpg"),
            ("123.preview", "LINE_123.jpg"),
            ("123.report v2.pdf", "report v2.pdf"),
            ("album-abc.jpg", "album-abc.jpg"),
            ("note-h-xyz", "note-h-xyz.jpg"),
        ] {
            let path = dir.path().join(file);
            std::fs::write(&path, jpeg).unwrap();
            assert_eq!(display_name(&path).unwrap(), want, "{file}");
        }
    }

    #[test]
    fn evicts_old_full_files_before_thumbnails() {
        let dir = tempfile::tempdir().unwrap();
        let old = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
        let new = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000);
        let newest = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(3_000);
        for (name, size, time) in [
            ("1.preview", 10, old),
            ("1.full", 40, new),
            ("2.full", 40, old),
            ("3.full", 10, newest),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, vec![0u8; size]).unwrap();
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(time).unwrap();
        }
        let names = |limit, target| {
            evictions(files(dir.path()), limit, target)
                .iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert!(names(100, 50).is_empty());
        assert_eq!(names(90, 60), ["2.full"]);
        assert_eq!(names(90, 10), ["2.full", "1.full", "3.full"]);
        assert_eq!(names(90, 0), ["2.full", "1.full", "3.full", "1.preview"]);
    }

    #[test]
    fn unique_names_do_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.jpg");
        assert_eq!(unique(&a), a);
        std::fs::write(&a, b"x").unwrap();
        assert_eq!(unique(&a), dir.path().join("a (2).jpg"));
    }
}
