//! Data directory layout. Everything lives in `~/Library/Application Support/Laab/`
//! unless `LAAB_DATA_DIR` points elsewhere.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn resolve(demo: bool) -> Result<Self> {
        let root = if demo {
            std::env::temp_dir().join("laab-demo")
        } else if let Some(dir) = std::env::var_os("LAAB_DATA_DIR") {
            PathBuf::from(dir)
        } else {
            dirs::data_dir()
                .context("no Application Support folder")?
                .join("Laab")
        };
        if demo {
            let _ = fs::remove_dir_all(&root);
        }
        private_dir(&root)?;
        let paths = Self { root };
        for dir in [paths.avatars(), paths.media(), paths.stickers()] {
            private_dir(&dir)?;
        }
        Ok(paths)
    }

    pub fn database(&self) -> PathBuf {
        self.root.join("laab.sqlite")
    }

    /// Where builds before encryption kept the LINE session. `session.rs`
    /// imports these files into the database and deletes them.
    pub fn line_storage(&self) -> PathBuf {
        self.root.join("linejs-rs-storage.json")
    }

    pub fn auth_token(&self) -> PathBuf {
        self.root.join("auth-token.txt")
    }

    pub fn auth_device(&self) -> PathBuf {
        self.root.join("auth-device.txt")
    }

    pub fn avatars(&self) -> PathBuf {
        self.root.join("cache/avatars")
    }

    pub fn media(&self) -> PathBuf {
        self.root.join("cache/media")
    }

    pub fn stickers(&self) -> PathBuf {
        self.root.join("cache/stickers")
    }
}

pub fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Write a file through a temp file and rename, mode 0600.
pub fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
