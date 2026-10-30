//! The log, also written to `logs/laab.log` in the data folder. It never
//! leaves the machine on its own; a problem report can attach its end if the
//! user ticks it. Past 1 MB it moves to `laab.1.log`, replacing the older one.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const LIMIT: u64 = 1024 * 1024;

static FILE: OnceLock<PathBuf> = OnceLock::new();
static OPEN: Mutex<Option<File>> = Mutex::new(None);

/// Starts writing to the log file under `root`. Lines before this only go
/// to stderr.
pub fn start(root: &std::path::Path) {
    let dir = root.join("logs");
    if crate::paths::private_dir(&dir).is_ok() {
        let _ = FILE.set(dir.join("laab.log"));
    }
}

fn open(path: &PathBuf) -> io::Result<File> {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > LIMIT) {
        let _ = std::fs::rename(path, path.with_extension("1.log"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new().create(true).append(true).mode(0o600).open(path)
    }
    #[cfg(not(unix))]
    OpenOptions::new().create(true).append(true).open(path)
}

pub struct Writer;

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Some(path) = FILE.get() else { return Ok(buf.len()) };
        let mut open_file = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        if open_file.as_ref().and_then(|f| f.metadata().ok()).is_none_or(|m| m.len() > LIMIT) {
            *open_file = open(path).ok();
        }
        if let Some(f) = open_file.as_mut() {
            f.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Writer {
    type Writer = Writer;
    fn make_writer(&'a self) -> Writer {
        Writer
    }
}

/// The last `lines` lines of the log.
pub fn tail(lines: usize) -> Vec<String> {
    let Some(text) = FILE.get().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Vec::new();
    };
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].iter().map(|s| s.to_string()).collect()
}
