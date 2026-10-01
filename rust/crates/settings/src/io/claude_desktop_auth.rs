//! Read-only macOS Chromium cookies. Secret buffers never cross the wire or logs.
#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::Result;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
#[error("Allow Mando Keychain access to refresh this plan")]
pub(crate) struct KeychainAccessRequired;

#[cfg(target_os = "macos")]
static KEYCHAIN_LOCK: std::sync::Mutex<Option<zeroize::Zeroizing<[u8; 16]>>> =
    std::sync::Mutex::new(None);

/// Only the explicit permission button calls this function. The OS owns its
/// prompt and authorization; Mando never receives a user's login password.
#[cfg(target_os = "macos")]
pub(crate) async fn authorize_keychain() -> Result<()> {
    tokio::task::spawn_blocking(|| keychain_key(true).map(|_key| ()))
        .await
        .context("Claude Desktop Keychain authorization task failed")?
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn authorize_keychain() -> Result<()> {
    anyhow::bail!("Claude Desktop Keychain authorization requires macOS")
}

#[cfg(target_os = "macos")]
fn keychain_key(allow_prompt: bool) -> Result<zeroize::Zeroizing<[u8; 16]>> {
    use core_foundation::{
        base::{CFType, TCFType},
        boolean::CFBoolean,
        data::CFData,
        dictionary::CFDictionary,
        string::CFString,
    };
    use security_framework_sys::{
        item::{
            kSecAttrAccount, kSecAttrService, kSecClass, kSecClassGenericPassword, kSecMatchLimit,
            kSecReturnData, kSecUseAuthenticationUI,
        },
        keychain::{SecKeychainGetUserInteractionAllowed, SecKeychainSetUserInteractionAllowed},
        keychain_item::SecItemCopyMatching,
    };
    // security-framework-sys does not yet expose the Fail/Allow constants.
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        static kSecUseAuthenticationUIFail: core_foundation::string::CFStringRef;
        static kSecUseAuthenticationUIAllow: core_foundation::string::CFStringRef;
        static kSecMatchLimitOne: core_foundation::string::CFStringRef;
    }
    let mut cached = KEYCHAIN_LOCK
        .lock()
        .map_err(|error| anyhow::anyhow!("Claude Keychain access lock failed: {error}"))?;
    if let Some(key) = cached.as_ref() {
        return Ok(zeroize::Zeroizing::new(**key));
    }
    // Legacy login Keychain ACLs can ignore per-query UI flags, so also forbid
    // process interaction for an automatic lookup. Serialize and restore it.
    let mut original = 0_u8;
    // SAFETY: the Boolean out pointer is live and Security owns these APIs.
    let status = unsafe { SecKeychainGetUserInteractionAllowed(&mut original) };
    anyhow::ensure!(
        status == 0,
        "Could not inspect macOS Keychain interaction policy ({status})"
    );
    // SAFETY: changes only this process's interaction flag.
    let status = unsafe { SecKeychainSetUserInteractionAllowed(u8::from(allow_prompt)) };
    anyhow::ensure!(
        status == 0,
        "Could not set macOS Keychain interaction policy ({status})"
    );
    // SAFETY: Security constants are immortal CFString references. Query keys
    // and values are retained by CFDictionary until the synchronous call ends.
    let result = unsafe {
        let string = |reference| CFString::wrap_under_get_rule(reference).as_CFType();
        let query = CFDictionary::from_CFType_pairs(&[
            (string(kSecClass), string(kSecClassGenericPassword)),
            (
                string(kSecAttrService),
                CFString::new("Claude Safe Storage").as_CFType(),
            ),
            (
                string(kSecAttrAccount),
                CFString::new("Claude Key").as_CFType(),
            ),
            (string(kSecReturnData), CFBoolean::true_value().as_CFType()),
            (string(kSecMatchLimit), string(kSecMatchLimitOne)),
            (
                string(kSecUseAuthenticationUI),
                string(if allow_prompt {
                    kSecUseAuthenticationUIAllow
                } else {
                    kSecUseAuthenticationUIFail
                }),
            ),
        ]);
        let mut found = std::ptr::null();
        let status = SecItemCopyMatching(query.as_concrete_TypeRef(), &mut found);
        if status == -25308 || status == -25293 || status == -128 {
            Err(anyhow::Error::new(KeychainAccessRequired))
        } else if status == -25300 {
            Err(anyhow::anyhow!(
                "Claude Desktop Safe Storage key is missing; sign in to Claude Desktop first"
            ))
        } else if status != 0 {
            Err(anyhow::anyhow!(
                "Claude Desktop Keychain lookup failed ({status})"
            ))
        } else if found.is_null() {
            Err(anyhow::anyhow!("Claude Desktop Keychain returned no key"))
        } else {
            let object = CFType::wrap_under_create_rule(found);
            if object.type_of() != CFData::type_id() {
                Err(anyhow::anyhow!(
                    "Claude Desktop Keychain returned an invalid key type"
                ))
            } else {
                let data = CFData::wrap_under_get_rule(found.cast());
                let secret = zeroize::Zeroizing::new(data.bytes().to_vec());
                if secret.is_empty() {
                    Err(anyhow::anyhow!(
                        "Claude Desktop Keychain returned an empty key"
                    ))
                } else {
                    let mut key = zeroize::Zeroizing::new([0_u8; 16]);
                    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(&secret, b"saltysalt", 1003, key.as_mut());
                    Ok(key)
                }
            }
        }
    };
    // SAFETY: restore the original process flag before returning any error.
    let restore = unsafe { SecKeychainSetUserInteractionAllowed(original) };
    anyhow::ensure!(
        restore == 0,
        "Could not restore macOS Keychain interaction policy ({restore})"
    );
    let key = result?;
    *cached = Some(zeroize::Zeroizing::new(*key));
    Ok(key)
}

#[cfg(target_os = "macos")]
pub(crate) async fn cookie_header(profile: &Path) -> Result<String> {
    use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    use sha2::{Digest, Sha256};
    use sqlx::{sqlite::SqliteConnectOptions, Connection, Row, SqliteConnection};
    use std::time::Duration;

    let key = tokio::task::spawn_blocking(|| keychain_key(false))
        .await
        .context("Claude Desktop Keychain lookup task failed")??;
    let options = SqliteConnectOptions::new()
        .filename(profile.join("Cookies"))
        .read_only(true)
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(3));
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .context("Could not read Claude Desktop Cookies database")?;
    let version: String = sqlx::query_scalar("SELECT value FROM meta WHERE key = 'version'")
        .fetch_one(&mut connection)
        .await
        .context("Claude Desktop Cookies database has no schema version")?;
    let version: u32 = version
        .parse()
        .context("Invalid Claude Desktop cookie schema version")?;
    let rows = sqlx::query("SELECT host_key, name, path, value, encrypted_value, expires_utc FROM cookies WHERE host_key IN ('claude.ai', '.claude.ai') ORDER BY length(path) DESC, last_access_utc DESC LIMIT 257")
        .fetch_all(&mut connection)
        .await
        .context("Could not read Claude Desktop session cookies")?;
    anyhow::ensure!(
        rows.len() <= 256,
        "Claude Desktop has too many session cookies"
    );
    let now =
        time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1000 + 11_644_473_600_000_000;
    let mut parts = Vec::new();
    let mut has_session = false;
    for row in rows {
        let expiry: i64 = row.try_get("expires_utc")?;
        let path: String = row.try_get("path")?;
        if expiry != 0 && i128::from(expiry) <= now
            || !(path == "/" || path == "/api" || path == "/api/")
        {
            continue;
        }
        let host: String = row.try_get("host_key")?;
        let name: String = row.try_get("name")?;
        let encrypted: Vec<u8> = row.try_get("encrypted_value")?;
        let value: String = if encrypted.is_empty() {
            row.try_get("value")?
        } else {
            anyhow::ensure!(
                encrypted.starts_with(b"v10") || encrypted.starts_with(b"v11"),
                "Claude Desktop cookie encryption format is unsupported"
            );
            let mut buffer = encrypted[3..].to_vec();
            let clear = cbc::Decryptor::<aes::Aes128>::new(&(*key).into(), &[b' '; 16].into())
                .decrypt_padded_mut::<Pkcs7>(&mut buffer)
                .map_err(|error| {
                    anyhow::anyhow!("Claude Desktop cookie decryption failed: {error}")
                })?;
            let clear = if version >= 24 {
                let digest = Sha256::digest(host.as_bytes());
                anyhow::ensure!(
                    clear.starts_with(&digest),
                    "Claude Desktop cookie host validation failed"
                );
                &clear[32..]
            } else {
                clear
            };
            std::str::from_utf8(clear)
                .context("Claude Desktop cookie is not valid UTF-8")?
                .to_owned()
        };
        anyhow::ensure!(
            name.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                && !value.bytes().any(|b| b.is_ascii_control() || b == b';'),
            "Claude Desktop cookie contains invalid header characters"
        );
        if name == "sessionKey" && !value.is_empty() {
            has_session = true;
        }
        parts.push(format!("{name}={value}"));
    }
    connection
        .close()
        .await
        .context("Could not close Claude Desktop cookie reader")?;
    anyhow::ensure!(
        has_session,
        "Claude Desktop login has expired or is missing; reopen this profile and sign in"
    );
    let header = parts.join("; ");
    anyhow::ensure!(
        header.len() <= 64 * 1024,
        "Claude Desktop session cookie header is too large"
    );
    Ok(header)
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn cookie_header(_profile: &Path) -> Result<String> {
    anyhow::bail!("Claude Desktop subscription refresh requires macOS")
}
