//! The LINE session (line-rs storage and the auth token) lives in the
//! encrypted database, not in files next to it.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use line_rs::storage::{Storage, StorageValue};

use crate::paths::Paths;
use crate::state::AppState;
use crate::store::Store;

const AUTH_TOKEN: &str = "auth_token";
const AUTH_DEVICE: &str = "auth_device";

pub struct SessionStorage(pub Arc<AppState>);

fn storage_error(e: anyhow::Error) -> line_rs::Error {
    line_rs::Error::Internal {
        kind: "StorageError".into(),
        message: e.to_string(),
    }
}

#[async_trait]
impl Storage for SessionStorage {
    async fn set(&self, key: &str, value: StorageValue) -> line_rs::Result<()> {
        self.0
            .store()
            .session_set(key, &value.to_string())
            .map_err(storage_error)
    }
    async fn get(&self, key: &str) -> line_rs::Result<Option<StorageValue>> {
        let value = self.0.store().session_get(key).map_err(storage_error)?;
        Ok(value.map(|v| serde_json::from_str(&v)).transpose()?)
    }
    async fn delete(&self, key: &str) -> line_rs::Result<()> {
        self.0.store().session_delete(key).map_err(storage_error)
    }
    async fn clear(&self) -> line_rs::Result<()> {
        self.0.store().session_clear().map_err(storage_error)
    }
    async fn get_all(&self) -> line_rs::Result<serde_json::Map<String, StorageValue>> {
        let rows = self.0.store().session_all().map_err(storage_error)?;
        rows.into_iter()
            .map(|(k, v)| Ok((k, serde_json::from_str(&v)?)))
            .collect()
    }
}

/// The saved auth token and the device it belongs to.
pub fn auth(store: &Store) -> Result<Option<(String, Option<String>)>> {
    let Some(token) = store.get_kv(AUTH_TOKEN)?.filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    Ok(Some((token, store.get_kv(AUTH_DEVICE)?)))
}

pub fn save_auth(store: &Store, token: &str, device: &str) -> Result<()> {
    store.set_kv(AUTH_TOKEN, token)?;
    store.set_kv(AUTH_DEVICE, device)
}

pub fn clear_auth(store: &Store) -> Result<()> {
    store.delete_kv(AUTH_TOKEN)?;
    store.delete_kv(AUTH_DEVICE)
}

/// Builds before encryption kept the session in plain files. Move it into
/// the database and delete the files.
pub fn import_files(store: &Store, paths: &Paths) -> Result<()> {
    let storage = paths.line_storage();
    if let Ok(text) = std::fs::read_to_string(&storage) {
        if !text.trim().is_empty() {
            let map: serde_json::Map<String, StorageValue> = serde_json::from_str(&text)?;
            for (k, v) in map {
                store.session_set(&k, &v.to_string())?;
            }
        }
        std::fs::remove_file(&storage)?;
    }
    let _ = std::fs::remove_file(storage.with_extension("tmp"));
    for (path, key) in [(paths.auth_token(), AUTH_TOKEN), (paths.auth_device(), AUTH_DEVICE)] {
        if let Ok(text) = std::fs::read_to_string(&path) {
            store.set_kv(key, text.trim())?;
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_and_deletes_the_plain_session_files() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            root: dir.path().to_path_buf(),
        };
        std::fs::write(paths.line_storage(), r#"{"refreshToken":"r1","reqseq":{"a":3}}"#).unwrap();
        std::fs::write(paths.auth_token(), "tok\n").unwrap();
        std::fs::write(paths.auth_device(), "DESKTOPMAC").unwrap();
        let store = Store::memory().unwrap();
        import_files(&store, &paths).unwrap();
        assert_eq!(
            store.session_get("refreshToken").unwrap().as_deref(),
            Some("\"r1\"")
        );
        assert_eq!(store.session_get("reqseq").unwrap().as_deref(), Some(r#"{"a":3}"#));
        assert_eq!(
            auth(&store).unwrap(),
            Some(("tok".into(), Some("DESKTOPMAC".into())))
        );
        assert!(!paths.line_storage().exists());
        assert!(!paths.auth_token().exists());
        assert!(!paths.auth_device().exists());
    }
}
