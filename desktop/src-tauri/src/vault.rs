//! Encrypted cache files. Photos, videos, files and avatars in `cache/` are
//! AES-256-GCM in 64 KiB chunks, so the `media://` handler can decrypt only
//! the range a `<video>` asks for. The key is derived from the database key;
//! demo data has no key and stays plain. Files from before encryption are
//! read as they are until `encrypt_existing` rewrites them.
//!
//! Layout: `MAGIC` (8 bytes), a random nonce prefix (8), then chunks of
//! `CHUNK` plaintext bytes (the last one shorter) each followed by a 16-byte
//! tag. Chunk `i` uses nonce `prefix ‖ i` and AAD `i ‖ last`, so a dropped,
//! reordered or truncated chunk fails to decrypt.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"LAABENC1";
const HEADER: u64 = 16;
const CHUNK: u64 = 64 * 1024;
const TAG: u64 = 16;

static CIPHER: OnceLock<Option<Aes256Gcm>> = OnceLock::new();

/// Set the cache key once at launch; `None` keeps new files plain (demo).
pub fn init(database_key: Option<&str>) {
    let _ = CIPHER.set(database_key.map(cache_cipher));
}

fn cache_cipher(database_key: &str) -> Aes256Gcm {
    let mut h = Sha256::new();
    h.update(b"laab cache v1\0");
    h.update(database_key.as_bytes());
    Aes256Gcm::new_from_slice(&h.finalize()).expect("a SHA-256 digest is a 32-byte key")
}

fn cipher() -> Option<&'static Aes256Gcm> {
    CIPHER.get().and_then(Option::as_ref)
}

fn nonce(prefix: &[u8; 8], i: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[..8].copy_from_slice(prefix);
    n[8..].copy_from_slice(&(i as u32).to_be_bytes());
    n
}

fn aad(i: u64, last: bool) -> [u8; 5] {
    let mut a = [0u8; 5];
    a[..4].copy_from_slice(&(i as u32).to_be_bytes());
    a[4] = last as u8;
    a
}

/// Encrypt `len` bytes from `src` into `out`.
fn seal(c: &Aes256Gcm, src: &mut impl Read, len: u64, out: &mut impl Write) -> Result<()> {
    let mut prefix = [0u8; 8];
    OsRng.fill_bytes(&mut prefix);
    out.write_all(MAGIC)?;
    out.write_all(&prefix)?;
    let count = len.div_ceil(CHUNK).max(1);
    let mut buf = vec![0u8; CHUNK as usize];
    for i in 0..count {
        let n = (len - i * CHUNK).min(CHUNK) as usize;
        src.read_exact(&mut buf[..n])?;
        let sealed = c
            .encrypt(
                Nonce::from_slice(&nonce(&prefix, i)),
                Payload {
                    msg: &buf[..n],
                    aad: &aad(i, i + 1 == count),
                },
            )
            .map_err(|_| anyhow!("encrypt failed"))?;
        out.write_all(&sealed)?;
    }
    Ok(())
}

/// A cache file opened for reading, encrypted or not.
pub struct CacheFile {
    file: File,
    size: u64,
    sealed: Option<(&'static Aes256Gcm, [u8; 8])>,
}

impl CacheFile {
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let size = file.metadata()?.len();
        let mut head = [0u8; HEADER as usize];
        let sealed = if size >= HEADER + TAG
            && file.read_exact(&mut head).is_ok()
            && &head[..8] == MAGIC
        {
            let c = cipher().ok_or_else(|| anyhow!("no cache key for {}", path.display()))?;
            Some((c, head[8..].try_into().unwrap()))
        } else {
            None
        };
        Ok(Self { file, size, sealed })
    }

    pub fn is_encrypted(&self) -> bool {
        self.sealed.is_some()
    }

    fn chunks(&self) -> u64 {
        (self.size - HEADER).div_ceil(CHUNK + TAG)
    }

    /// Plaintext length.
    pub fn len(&self) -> u64 {
        if self.sealed.is_none() {
            return self.size;
        }
        let body = self.size - HEADER;
        body - self.chunks() * TAG
    }

    fn chunk(&mut self, i: u64) -> Result<Vec<u8>> {
        let (c, prefix) = self.sealed.expect("chunk of a plain file");
        let offset = HEADER + i * (CHUNK + TAG);
        let n = (self.size - offset).min(CHUNK + TAG) as usize;
        let mut buf = vec![0u8; n];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut buf)?;
        c.decrypt(
            Nonce::from_slice(&nonce(&prefix, i)),
            Payload {
                msg: &buf,
                aad: &aad(i, i + 1 == self.chunks()),
            },
        )
        .map_err(|_| anyhow!("cache file is damaged or has another key"))
    }

    /// Plaintext bytes `start..=end`.
    pub fn read_range(&mut self, start: u64, end: u64) -> Result<Vec<u8>> {
        if start > end || end >= self.len() {
            bail!("range {start}-{end} outside {}", self.len());
        }
        if self.sealed.is_none() {
            let mut data = vec![0u8; (end - start + 1) as usize];
            self.file.seek(SeekFrom::Start(start))?;
            self.file.read_exact(&mut data)?;
            return Ok(data);
        }
        let (first, last) = (start / CHUNK, end / CHUNK);
        let mut data = Vec::with_capacity(((last - first + 1) * CHUNK) as usize);
        for i in first..=last {
            data.extend(self.chunk(i)?);
        }
        let skip = (start - first * CHUNK) as usize;
        Ok(data[skip..skip + (end - start + 1) as usize].to_vec())
    }

    /// Up to `n` plaintext bytes from the start, for sniffing the type.
    pub fn head(&mut self, n: u64) -> Result<Vec<u8>> {
        match self.len() {
            0 => Ok(Vec::new()),
            len => self.read_range(0, n.min(len) - 1),
        }
    }

    pub fn read_all(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(self.len() as usize);
        self.copy_to(&mut out)?;
        Ok(out)
    }

    fn copy_to(&mut self, out: &mut impl Write) -> Result<()> {
        if self.sealed.is_none() {
            self.file.seek(SeekFrom::Start(0))?;
            std::io::copy(&mut self.file, out)?;
            return Ok(());
        }
        for i in 0..self.chunks() {
            out.write_all(&self.chunk(i)?)?;
        }
        Ok(())
    }
}

/// Write `data` to the cache, encrypted when there is a key. Atomic and 0600.
pub fn write(path: &Path, data: &[u8]) -> Result<()> {
    match cipher() {
        Some(c) => write_atomic(path, |out| seal(c, &mut &data[..], data.len() as u64, out)),
        None => write_atomic(path, |out| Ok(out.write_all(data)?)),
    }
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    CacheFile::open(path)?.read_all()
}

pub fn is_encrypted(path: &Path) -> bool {
    CacheFile::open(path).is_ok_and(|f| f.is_encrypted())
}

/// A plain copy at `dest`, for the user to keep or open in another app.
pub fn export(src: &Path, dest: &Path) -> Result<()> {
    let mut file = CacheFile::open(src)?;
    let tmp = dest.with_file_name(format!(".{}.laab-tmp", file_name(dest)));
    let result = File::create(&tmp)
        .map_err(anyhow::Error::from)
        .and_then(|mut out| {
            file.copy_to(&mut out)?;
            Ok(out.sync_all()?)
        })
        .and_then(|_| Ok(fs::rename(&tmp, dest)?));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.with_context(|| format!("export to {}", dest.display()))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Temp files end in `.tmp`; skip them when walking the cache.
pub fn is_temp(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "tmp")
}

fn write_atomic(path: &Path, fill: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // Unique per target, unlike `with_extension`, which maps `7.preview`
    // and `7.full` to the same `7.tmp`.
    let tmp = path.with_file_name(format!("{}.tmp", file_name(path)));
    let result = (|| {
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode_private()
            .open(&tmp)?;
        fill(&mut out)?;
        out.flush()?;
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

trait PrivateMode {
    fn mode_private(&mut self) -> &mut Self;
}

impl PrivateMode for fs::OpenOptions {
    #[cfg(unix)]
    fn mode_private(&mut self) -> &mut Self {
        use std::os::unix::fs::OpenOptionsExt;
        self.mode(0o600)
    }
    #[cfg(not(unix))]
    fn mode_private(&mut self) -> &mut Self {
        self
    }
}

/// Encrypt the plain files left in `dirs` by builds before encryption,
/// keeping their modification time (the cache limit evicts by it).
pub fn encrypt_existing(dirs: &[PathBuf]) -> Result<usize> {
    let Some(c) = cipher() else { return Ok(0) };
    let mut done = 0;
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else { continue };
        for path in entries.filter_map(|e| e.ok()).map(|e| e.path()) {
            if !path.is_file() || is_temp(&path) || is_encrypted(&path) {
                continue;
            }
            let meta = fs::metadata(&path)?;
            if meta.len() == 0 {
                continue;
            }
            let mut src = File::open(&path)?;
            write_atomic(&path, |out| seal(c, &mut src, meta.len(), out))
                .with_context(|| format!("encrypt {}", path.display()))?;
            if let Ok(modified) = meta.modified() {
                let _ = touch_at(&path, modified);
            }
            done += 1;
        }
    }
    Ok(done)
}

/// Mark a file as used now, for the cache limit's least-recently-used order.
/// Skipped when it was marked within the last hour.
pub fn touch(path: &Path) {
    let now = SystemTime::now();
    let recent = fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|m| now.duration_since(m).is_ok_and(|d| d.as_secs() < 3600));
    if !recent {
        let _ = touch_at(path, now);
    }
}

fn touch_at(path: &Path, time: SystemTime) -> std::io::Result<()> {
    fs::OpenOptions::new().write(true).open(path)?.set_modified(time)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn sealed(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        seal(&cache_cipher(KEY), &mut &data[..], data.len() as u64, &mut out).unwrap();
        out
    }

    fn open(dir: &Path, bytes: &[u8]) -> CacheFile {
        let path = dir.join("f");
        fs::write(&path, bytes).unwrap();
        let file = File::open(&path).unwrap();
        let mut head = [0u8; 16];
        (&file).read_exact(&mut head).unwrap();
        CacheFile {
            size: bytes.len() as u64,
            file,
            sealed: Some((Box::leak(Box::new(cache_cipher(KEY))), head[8..].try_into().unwrap())),
        }
    }

    #[test]
    fn reads_any_range_across_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0..(CHUNK * 3 + 123)).map(|i| (i % 251) as u8).collect();
        let bytes = sealed(&data);
        assert_eq!(&bytes[..8], MAGIC);
        let mut f = open(dir.path(), &bytes);
        assert_eq!(f.len(), data.len() as u64);
        assert_eq!(f.read_all().unwrap(), data);
        for (a, b) in [(0, 0), (5, CHUNK - 1), (CHUNK - 3, CHUNK + 3), (100, CHUNK * 3 + 122)] {
            assert_eq!(f.read_range(a, b).unwrap(), data[a as usize..=b as usize]);
        }
        assert!(f.read_range(0, data.len() as u64).is_err());
    }

    #[test]
    fn handles_empty_and_exact_chunk_sizes() {
        let dir = tempfile::tempdir().unwrap();
        for len in [0, 1, CHUNK, CHUNK * 2] {
            let data = vec![7u8; len as usize];
            let mut f = open(dir.path(), &sealed(&data));
            assert_eq!(f.len(), len);
            assert_eq!(f.read_all().unwrap(), data);
        }
    }

    #[test]
    fn rejects_tampering_and_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let data = vec![1u8; (CHUNK * 2 + 10) as usize];
        let mut bytes = sealed(&data);
        bytes[HEADER as usize + 5] ^= 1;
        assert!(open(dir.path(), &bytes).read_range(0, 1).is_err());
        // Cutting the last chunk leaves a full chunk that is not marked last.
        let bytes = sealed(&data);
        let cut = &bytes[..(HEADER + 2 * (CHUNK + TAG)) as usize];
        assert!(open(dir.path(), cut).read_all().is_err());
    }

    #[test]
    fn encrypts_old_files_and_exports_plain_copies() {
        init(Some(KEY));
        let dir = tempfile::tempdir().unwrap();
        let data = b"\xff\xd8\xff a private photo".repeat(5000);
        let old = dir.path().join("1.full");
        fs::write(&old, &data).unwrap();
        let then = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
        touch_at(&old, then).unwrap();

        assert_eq!(encrypt_existing(&[dir.path().to_path_buf()]).unwrap(), 1);
        assert!(is_encrypted(&old));
        assert_eq!(fs::metadata(&old).unwrap().modified().unwrap(), then);
        let raw = fs::read(&old).unwrap();
        assert!(!raw.windows(7).any(|w| w == b"private"));
        assert_eq!(read(&old).unwrap(), data);

        let out = dir.path().join("photo.jpg");
        export(&old, &out).unwrap();
        assert_eq!(fs::read(&out).unwrap(), data);

        let new = dir.path().join("2.full");
        write(&new, b"new").unwrap();
        assert!(is_encrypted(&new));
        assert_eq!(read(&new).unwrap(), b"new");
        // photo.jpg is the plain export; it sits outside the cache in real use.
        fs::remove_file(&out).unwrap();
        assert_eq!(encrypt_existing(&[dir.path().to_path_buf()]).unwrap(), 0);
    }

    #[test]
    fn plain_files_read_as_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.full");
        fs::write(&path, b"\xff\xd8\xffjpeg").unwrap();
        let mut f = CacheFile::open(&path).unwrap();
        assert!(!f.is_encrypted());
        assert_eq!(f.head(3).unwrap(), b"\xff\xd8\xff");
        assert_eq!(f.read_range(3, 6).unwrap(), b"jpeg");
    }
}
