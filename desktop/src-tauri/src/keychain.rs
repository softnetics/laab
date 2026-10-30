//! The database key: 32 random bytes, kept hex-encoded in the login keychain
//! (service `tech.softnetics.laab`, account `database-key`). The Raycast
//! extension reads the same item.

use anyhow::{Result, bail};

const SERVICE: &str = "tech.softnetics.laab";
/// Where builds before the bundle id change kept the key.
const OLD_SERVICE: &str = "app.laab.mac";
const ACCOUNT: &str = "database-key";
/// errSecItemNotFound
const NOT_FOUND: i32 = -25300;

/// The stored key, or a new one if there is none.
#[cfg(target_os = "macos")]
pub fn database_key() -> Result<String> {
    use security_framework::passwords::{delete_generic_password, set_generic_password};
    use security_framework::random::SecRandom;

    if let Some(key) = stored_key(SERVICE)? {
        return Ok(key);
    }
    // Move the key from the old service, or the database it encrypted could
    // not be opened.
    if let Some(key) = stored_key(OLD_SERVICE)? {
        set_generic_password(SERVICE, ACCOUNT, key.as_bytes())?;
        let _ = delete_generic_password(OLD_SERVICE, ACCOUNT);
        return Ok(key);
    }
    let mut raw = [0u8; 32];
    SecRandom::default().copy_bytes(&mut raw)?;
    let key = hex::encode(raw);
    set_generic_password(SERVICE, ACCOUNT, key.as_bytes())?;
    Ok(key)
}

/// The key under `service`, or `None` if there is no such item.
#[cfg(target_os = "macos")]
fn stored_key(service: &str) -> Result<Option<String>> {
    use security_framework::passwords::get_generic_password;

    match get_generic_password(service, ACCOUNT) {
        Ok(bytes) => {
            let key = String::from_utf8(bytes)?;
            if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("the keychain item {service}/{ACCOUNT} is not a database key");
            }
            Ok(Some(key))
        }
        Err(e) if e.code() == NOT_FOUND => Ok(None),
        // Most often the user denied the keychain prompt.
        Err(e) => bail!("อ่านกุญแจฐานข้อมูลจาก Keychain ไม่ได้: {e}"),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn database_key() -> Result<String> {
    bail!("database encryption needs the macOS keychain")
}
