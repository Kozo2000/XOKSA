//! API key secure storage via OS Keychain.
//!
//! Each xoksa API key is stored as its own Keychain entry
//! (Service="xoksa", Account="<KEY_NAME>").
//!
//! Keys are loaded on demand: requesting one key never loads unrelated keys.
//! Retrieved key material is returned to the caller as `Zeroizing<String>` and
//! is not cached by this module.
//!
//! Migration: older builds stored keys in one consolidated "_keys" entry.
//! If a per-key entry is missing, `get_key()` falls back to that legacy block,
//! migrates only the requested key, and leaves unrelated keys untouched.

use anyhow::{Context, Result};
use zeroize::Zeroizing;

const SERVICE_NAME: &str = "xoksa";

/// Legacy account name used by earlier consolidated single-entry storage.
const CONSOLIDATED_ACCOUNT: &str = "_keys";

/// Known key names managed by setup and diagnostics.
pub const ALL_KEY_NAMES: &[&str] = &[
    "OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "CLAUDE_API_KEY",
    "BRAVE_API_KEY",
    "JQUANTS_API_KEY",
];

fn init_native_keyring() {
    // Register the platform-native OS credential store as keyring-core's default
    // store. This replaces `keyring::use_native_store` — we drop the `keyring`
    // meta-crate so its turso-backed db-keystore (an embedded SQL DB) is not
    // linked. Behaviour is unchanged: keys go to the OS keychain. A missing
    // store is ignored here and surfaces later as a keyring error at get/set.
    #[cfg(target_os = "windows")]
    if let Ok(store) = windows_native_keyring_store::Store::new() {
        keyring_core::set_default_store(store);
    }
    #[cfg(target_os = "macos")]
    if let Ok(store) = apple_native_keyring_store::keychain::Store::new() {
        keyring_core::set_default_store(store);
    }
    #[cfg(target_os = "linux")]
    if let Ok(store) = linux_keyutils_keyring_store::Store::new() {
        keyring_core::set_default_store(store);
    }
}

fn entry_for(account: &str) -> Result<keyring_core::Entry> {
    init_native_keyring();
    keyring_core::Entry::new(SERVICE_NAME, account).context("❌ Failed to access OS keyring")
}

/// Find one key in a legacy "KEY=VALUE\n..." text block.
fn find_key_in_block(text: &str, key_name: &str) -> Option<Zeroizing<String>> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        let k = k.trim();
        let v = v.trim();
        if k == key_name && !v.is_empty() {
            Some(Zeroizing::new(v.to_string()))
        } else {
            None
        }
    })
}

/// Remove one key from a legacy consolidated block without touching other lines.
fn remove_key_from_block(text: &str, key_name: &str) -> (Zeroizing<String>, bool) {
    let mut changed = false;
    // `out` carries every key this block still holds, so the buffers a growing
    // String abandons are freed holding other people's secrets — `Zeroizing`
    // only wipes the last one. Sized to the input, it never reallocates.
    let mut out = String::with_capacity(text.len());

    for line in text.lines() {
        let remove = line
            .split_once('=')
            .map(|(k, _)| k.trim() == key_name)
            .unwrap_or(false);
        if remove {
            changed = true;
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }

    (Zeroizing::new(out), changed)
}

fn read_individual_key(key_name: &str) -> Result<Option<Zeroizing<String>>> {
    let entry = entry_for(key_name)?;
    match entry.get_password() {
        Ok(v) if !v.is_empty() => Ok(Some(Zeroizing::new(v))),
        Ok(_) | Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("❌ Keyring read error: {e}")),
    }
}

fn write_individual_key(key_name: &str, value: &str) -> Result<()> {
    let entry = entry_for(key_name)?;
    if value.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("❌ Keyring delete error: {e}")),
        }
    } else {
        entry.set_password(value).context("❌ Keyring write error")
    }
}

fn read_legacy_consolidated_key(key_name: &str) -> Result<Option<Zeroizing<String>>> {
    let entry = entry_for(CONSOLIDATED_ACCOUNT)?;
    match entry.get_password() {
        Ok(text) if !text.is_empty() => {
            let text = Zeroizing::new(text);
            Ok(find_key_in_block(&text, key_name))
        }
        Ok(_) | Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("❌ Keyring read error: {e}")),
    }
}

fn remove_key_from_legacy_consolidated(key_name: &str) -> Result<()> {
    let entry = entry_for(CONSOLIDATED_ACCOUNT)?;
    let text = match entry.get_password() {
        Ok(text) if !text.is_empty() => Zeroizing::new(text),
        Ok(_) | Err(keyring_core::Error::NoEntry) => return Ok(()),
        Err(e) => return Err(anyhow::anyhow!("❌ Keyring read error: {e}")),
    };

    let (updated, changed) = remove_key_from_block(&text, key_name);
    if !changed {
        return Ok(());
    }

    if updated.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("❌ Keyring delete error: {e}")),
        }
    } else {
        entry
            .set_password(updated.as_str())
            .context("❌ Keyring write error")
    }
}

/// Retrieve one key. Does not load unrelated keys.
/// Returns `Ok(Some(key))` if found, `Ok(None)` if not set.
pub fn get_key(key_name: &str) -> Result<Option<Zeroizing<String>>> {
    if let Some(v) = read_individual_key(key_name)? {
        return Ok(Some(v));
    }

    let Some(v) = read_legacy_consolidated_key(key_name)? else {
        return Ok(None);
    };

    // Best-effort one-key migration from the legacy consolidated block. A
    // migration failure should not hide a key that was successfully read.
    if write_individual_key(key_name, v.as_str()).is_ok() {
        let _ = remove_key_from_legacy_consolidated(key_name);
    }
    Ok(Some(v))
}

/// Store one key in its own Keychain entry.
pub fn set_key(key_name: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        return delete_key(key_name);
    }

    write_individual_key(key_name, value)?;
    let _ = remove_key_from_legacy_consolidated(key_name);
    Ok(())
}

/// Delete one key from its own Keychain entry and any legacy consolidated block.
pub fn delete_key(key_name: &str) -> Result<()> {
    write_individual_key(key_name, "")?;
    remove_key_from_legacy_consolidated(key_name)
}

/// Result of a key presence check for `--doctor` / `--check-keys`.
#[derive(Debug)]
pub enum KeyPresence {
    /// Key found; inner value is the source label ("xoksa.env" or "os-keyring").
    Found(&'static str),
    /// Key not present in xoksa.env or OS Keychain.
    NotFound,
    /// xoksa.env did not have the key and the OS Keychain could not be accessed.
    KeyringError,
}

impl KeyPresence {
    pub fn is_found(&self) -> bool {
        matches!(self, KeyPresence::Found(_))
    }
}

/// Check if a key is present in xoksa.env or OS Keychain.
pub fn resolve_key_presence(key_name: &str) -> KeyPresence {
    if crate::utils::key_present_in_env_file(key_name) {
        return KeyPresence::Found("xoksa.env");
    }
    match get_key(key_name) {
        Ok(Some(_)) => KeyPresence::Found("os-keyring"),
        Ok(None) => KeyPresence::NotFound,
        Err(_) => KeyPresence::KeyringError,
    }
}

/// Fixed Class A key name for the non-loopback `serve` auth token (see
/// security-design.md §4). Unlike API keys it is auto-generated by the engine and
/// may be shown to the operator on demand so a remote client can be configured.
pub const SERVE_AUTH_TOKEN: &str = "SERVE_AUTH_TOKEN";

/// A 256-bit (64 hex chars) unguessable token from the OS CSPRNG.
///
/// Both halves of the token are wiped, not just the string that leaves here.
/// `SERVE_AUTH_TOKEN` is the only thing guarding a non-loopback `serve` (§4's
/// fail-closed bind is premised on it), and this is where it comes into being:
/// the raw bytes are the token, and a per-byte `format!` would leave thirty-two
/// two-character allocations behind that reconstruct the whole of it. The buffer
/// is therefore `Zeroizing`, and the hex is written straight into the string that
/// is returned — no intermediate allocation exists to be missed.
fn random_token_256() -> Zeroizing<String> {
    use std::fmt::Write as _;
    let mut buf = Zeroizing::new([0u8; 32]);
    getrandom::getrandom(&mut *buf).expect("OS CSPRNG unavailable");
    let mut s = String::with_capacity(64);
    for b in buf.iter() {
        // Infallible: writing to a String only fails if the formatter does.
        let _ = write!(&mut s, "{b:02x}");
    }
    Zeroizing::new(s)
}

/// The serve auth token, or `None` if not set yet.
pub fn get_serve_token() -> Result<Option<Zeroizing<String>>> {
    get_key(SERVE_AUTH_TOKEN)
}

/// Return the serve auth token, generating and storing a fresh one if none
/// exists. The `bool` is `true` when a token was just generated (so the caller can
/// display it once).
pub fn ensure_serve_token() -> Result<(Zeroizing<String>, bool)> {
    if let Some(t) = get_serve_token()? {
        return Ok((t, false));
    }
    let t = random_token_256();
    set_key(SERVE_AUTH_TOKEN, t.as_str())?;
    Ok((t, true))
}

/// Regenerate and store a fresh serve auth token, returning the new value.
pub fn rotate_serve_token() -> Result<Zeroizing<String>> {
    let t = random_token_256();
    set_key(SERVE_AUTH_TOKEN, t.as_str())?;
    Ok(t)
}

/// Key name for the standalone settings app's password gate. No user ID — password
/// only. Stored in the OS keychain (never in xoksa.env); the user may change it.
pub const SETTINGS_PASSWORD: &str = "SETTINGS_PASSWORD";

/// Default settings-app password seeded on first run; the user is expected to
/// change it (change is left to the user).
pub const DEFAULT_SETTINGS_PASSWORD: &str = "XOKSA_password";

/// Constant-time byte comparison — no early return on a mismatching byte, so the
/// compare time does not leak how much of the password matched.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// The settings-app password, or `None` if never initialized.
pub fn get_settings_password() -> Result<Option<Zeroizing<String>>> {
    get_key(SETTINGS_PASSWORD)
}

/// Return the settings-app password, seeding the default (`XOKSA_password`) on first
/// use. The `bool` is `true` when the default was just written (first run).
pub fn ensure_settings_password() -> Result<(Zeroizing<String>, bool)> {
    if let Some(p) = get_settings_password()? {
        return Ok((p, false));
    }
    set_key(SETTINGS_PASSWORD, DEFAULT_SETTINGS_PASSWORD)?;
    Ok((Zeroizing::new(DEFAULT_SETTINGS_PASSWORD.to_string()), true))
}

/// Replace the settings-app password with a user-chosen value.
pub fn set_settings_password(new_password: &str) -> Result<()> {
    set_key(SETTINGS_PASSWORD, new_password)
}

/// Constant-time check of a candidate against the stored password (seeding the
/// default on first use).
pub fn verify_settings_password(candidate: &str) -> Result<bool> {
    let (stored, _) = ensure_settings_password()?;
    Ok(ct_eq(candidate.as_bytes(), stored.as_bytes()))
}

/// The desktop's connection token — the access token the desktop presents to the
/// (possibly remote) engine it connects to. A secret; stored in the OS keychain
/// (never in a plain file).
pub const DESKTOP_CONN_TOKEN: &str = "DESKTOP_CONN_TOKEN";

/// The stored desktop connection token, or `None` if unset/blank.
pub fn get_conn_token() -> Result<Option<Zeroizing<String>>> {
    Ok(get_key(DESKTOP_CONN_TOKEN)?.filter(|t| !t.is_empty()))
}

/// Store the desktop connection token (an empty value effectively clears it).
pub fn set_conn_token(token: &str) -> Result<()> {
    set_key(DESKTOP_CONN_TOKEN, token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_key_in_block_returns_only_requested_key() {
        let block = "OPENAI_API_KEY=openai\nBRAVE_API_KEY=brave\nCLAUDE_API_KEY=claude";

        assert_eq!(
            find_key_in_block(block, "BRAVE_API_KEY").map(|v| v.to_string()),
            Some("brave".to_string())
        );
        assert_eq!(find_key_in_block(block, "GEMINI_API_KEY"), None);
    }

    #[test]
    fn find_key_in_block_ignores_empty_values() {
        let block = "OPENAI_API_KEY=\nBRAVE_API_KEY=brave";

        assert_eq!(find_key_in_block(block, "OPENAI_API_KEY"), None);
        assert_eq!(
            find_key_in_block(block, "BRAVE_API_KEY").map(|v| v.to_string()),
            Some("brave".to_string())
        );
    }

    #[test]
    fn remove_key_from_block_removes_requested_key_only() {
        let block = "OPENAI_API_KEY=openai\nBRAVE_API_KEY=brave\nCLAUDE_API_KEY=claude";

        let (updated, changed) = remove_key_from_block(block, "BRAVE_API_KEY");

        assert!(changed);
        assert_eq!(
            updated.as_str(),
            "OPENAI_API_KEY=openai\nCLAUDE_API_KEY=claude"
        );
    }

    #[test]
    fn remove_key_from_block_reports_unchanged_when_absent() {
        let block = "OPENAI_API_KEY=openai\nBRAVE_API_KEY=brave";

        let (updated, changed) = remove_key_from_block(block, "GEMINI_API_KEY");

        assert!(!changed);
        assert_eq!(updated.as_str(), block);
    }
}
