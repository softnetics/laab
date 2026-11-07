//! Videos that play while they download.
//!
//! The first request for a video that isn't cached yet starts one download
//! into the cache (`vault::Writer`). line-rs writes an E2EE video chunk by
//! chunk as each chunk's digest checks out (`TalkMessage::stream_to`), so
//! the part already on disk is good, and `<video>`'s range requests are
//! answered from it while the rest comes in. A request past what has
//! arrived waits for it. Once the download ends the finished file serves
//! every request, as before.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use tauri::http::{Response, StatusCode, header};
use tokio::sync::watch;

use crate::state::AppState;
use crate::vault::{CacheFile, Writer};

/// Most bytes one range answer waits for while the file is still coming:
/// enough for the player to go on, few enough to answer soon.
const STEP: u64 = 1 << 20;

/// How long a request waits for the download to move before giving up.
const IDLE: Duration = Duration::from_secs(120);

/// How far a download is.
#[derive(Debug, Clone, Default)]
struct Progress {
    /// The whole file's length, once the server said it.
    total: Option<u64>,
    /// Bytes readable from the temp file now.
    readable: u64,
    /// The finished file, or why the download failed.
    done: Option<Result<PathBuf, String>>,
}

struct Download {
    tmp: PathBuf,
    progress: watch::Receiver<Progress>,
}

/// Downloads running now, by message id.
static RUNNING: LazyLock<Mutex<HashMap<String, Arc<Download>>>> = LazyLock::new(Default::default);

/// The cache file, telling readers how much of it they can read.
struct Tracked {
    file: Writer,
    tx: Arc<watch::Sender<Progress>>,
}

impl Write for Tracked {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(data)?;
        let readable = self.file.readable();
        self.tx.send_if_modified(|p| {
            let more = p.readable != readable;
            p.readable = readable;
            more
        });
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// The download of message `id` into `target`: the running one, or a new one.
fn download(state: &Arc<AppState>, line: &crate::line::Line, id: &str, target: &Path) -> anyhow::Result<Arc<Download>> {
    let mut running = RUNNING.lock().unwrap();
    if let Some(d) = running.get(id) {
        return Ok(d.clone());
    }
    let file = Writer::create(target)?;
    let (tx, rx) = watch::channel(Progress::default());
    let d = Arc::new(Download { tmp: file.tmp_path().to_path_buf(), progress: rx });
    running.insert(id.to_string(), d.clone());
    let tx = Arc::new(tx);
    let message = line.talk_message(id);
    let (state, id) = (state.clone(), id.to_string());
    tokio::spawn(async move {
        let mut out = Tracked { file, tx: tx.clone() };
        let on_len = tx.clone();
        let res = message
            .stream_to(&mut out, &mut |n| on_len.send_modify(|p| p.total = Some(n)))
            .await;
        let done = match res {
            Ok(_) => out.file.finish().map_err(|e| format!("{e:#}")),
            Err(e) => Err(crate::line::media_error(e).to_string()),
        };
        match &done {
            Ok(path) => {
                let _ = state.store().update_media(&id, Some(&path.to_string_lossy()), None);
                crate::cache::schedule_trim(&state);
            }
            Err(e) => tracing::warn!("video download: {e}"),
        }
        RUNNING.lock().unwrap().remove(&id);
        tx.send_modify(|p| p.done = Some(done));
    });
    Ok(d)
}

/// What to answer for `range` (or the whole file) now, if anything.
enum Answer {
    Wait,
    Finished(PathBuf),
    Part { start: u64, end: u64, total: u64 },
    Fail,
}

/// Answer once `range` is there, or a first part of it of [`STEP`] bytes.
fn answer(p: &Progress, range: Option<&str>) -> Answer {
    match &p.done {
        Some(Ok(path)) => return Answer::Finished(path.clone()),
        Some(Err(_)) => return Answer::Fail,
        None => {}
    }
    let (Some(total), Some(range)) = (p.total, range) else {
        return Answer::Wait;
    };
    let Some((start, end)) = crate::media::byte_range(range, total) else {
        return Answer::Fail;
    };
    let want = end.min(start.saturating_add(STEP - 1));
    if p.readable > want {
        Answer::Part { start, end: end.min(p.readable - 1), total }
    } else {
        Answer::Wait
    }
}

fn part(tmp: &Path, readable: u64, start: u64, end: u64, total: u64) -> anyhow::Result<Response<Vec<u8>>> {
    let mut file = CacheFile::open_growing(tmp, readable)?;
    let mime = crate::media::sniff_mime(&file.head(16)?, Path::new(""));
    let data = file.read_range(start, end)?;
    let mut res = crate::media::respond(StatusCode::PARTIAL_CONTENT, data, mime);
    let h = res.headers_mut();
    h.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    h.insert(header::CONTENT_RANGE, format!("bytes {start}-{end}/{total}").parse().unwrap());
    Ok(res)
}

/// Serve video message `id` while it downloads to `target`. `None` when it
/// can't be had; the caller answers 404.
pub async fn serve(
    state: &Arc<AppState>,
    line: &crate::line::Line,
    id: &str,
    target: &Path,
    range: Option<&str>,
) -> Option<Response<Vec<u8>>> {
    let d = download(state, line, id, target).inspect_err(|e| tracing::warn!("{e:#}")).ok()?;
    let mut rx = d.progress.clone();
    loop {
        let p = rx.borrow_and_update().clone();
        match answer(&p, range) {
            Answer::Finished(path) => return crate::media::file_response(&path, range),
            Answer::Fail => return None,
            // The temp file may have just been renamed: wait for `done`.
            Answer::Part { start, end, total } => match part(&d.tmp, p.readable, start, end, total) {
                Ok(res) => return Some(res),
                Err(e) => tracing::debug!("reading a video while it downloads: {e:#}"),
            },
            Answer::Wait => {}
        }
        match tokio::time::timeout(IDLE, rx.changed()).await {
            Ok(Ok(())) => {}
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(total: Option<u64>, readable: u64) -> Progress {
        Progress { total, readable, done: None }
    }

    fn part_of(a: Answer) -> Option<(u64, u64, u64)> {
        match a {
            Answer::Part { start, end, total } => Some((start, end, total)),
            _ => None,
        }
    }

    #[test]
    fn answers_what_has_arrived() {
        let mb = STEP;
        // Nothing known yet, or no range: wait.
        assert!(matches!(answer(&progress(None, 0), Some("bytes=0-")), Answer::Wait));
        assert!(matches!(answer(&progress(Some(10 * mb), 5 * mb), None), Answer::Wait));
        // The player's first probe.
        assert_eq!(part_of(answer(&progress(Some(10 * mb), 65536), Some("bytes=0-1"))), Some((0, 1, 10 * mb)));
        // An open range gets what has arrived, once a step of it is there.
        assert!(matches!(answer(&progress(Some(10 * mb), mb / 2), Some("bytes=0-")), Answer::Wait));
        assert_eq!(part_of(answer(&progress(Some(10 * mb), 3 * mb), Some("bytes=0-"))), Some((0, 3 * mb - 1, 10 * mb)));
        // A range past what has arrived waits.
        assert!(matches!(answer(&progress(Some(10 * mb), 3 * mb), Some("bytes=8388608-")), Answer::Wait));
        // A range outside the file fails.
        assert!(matches!(answer(&progress(Some(10), 0), Some("bytes=20-")), Answer::Fail));
    }

    #[test]
    fn finished_downloads_serve_the_file() {
        let mut p = progress(Some(10), 0);
        p.done = Some(Ok(PathBuf::from("/x.full")));
        assert!(matches!(answer(&p, None), Answer::Finished(_)));
        p.done = Some(Err("file_expired".into()));
        assert!(matches!(answer(&p, Some("bytes=0-")), Answer::Fail));
    }
}
