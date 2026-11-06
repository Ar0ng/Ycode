//! API keys live only in Windows Credential Manager (target `Ycode/<provider>`).
//! Keys never cross back to the UI: the frontend can set, test and delete a key and
//! learn whether one exists, but cannot read it.

use crate::error::{AppError, AppResult};
use crate::model::ProviderId;

#[cfg(not(feature = "internal-preview"))]
const SERVICE: &str = "Ycode";
#[cfg(feature = "internal-preview")]
const SERVICE: &str = "YC_pre";
#[cfg(feature = "internal-preview")]
const DISABLED: &str = "__YCODE_PREVIEW_KEY_DISABLED__";

fn entry(p: ProviderId) -> AppResult<keyring::Entry> {
    entry_for(SERVICE, p)
}

fn entry_for(service: &str, p: ProviderId) -> AppResult<keyring::Entry> {
    keyring::Entry::new(service, p.as_str()).map_err(|e| AppError::Secret(e.to_string()))
}

pub fn set_key(p: ProviderId, key: &str) -> AppResult<()> {
    let key = key.trim();
    if key.is_empty() {
        return Err(AppError::Invalid("empty key".into()));
    }
    if key.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(AppError::Invalid("key contains whitespace or control characters".into()));
    }
    #[cfg(feature = "internal-preview")]
    if key == DISABLED { return Err(AppError::Invalid("reserved key value".into())); }
    entry(p)?.set_password(key).map_err(|e| AppError::Secret(e.to_string()))
}

pub fn get_key(p: ProviderId) -> AppResult<Option<String>> {
    match entry(p)?.get_password() {
        #[cfg(feature = "internal-preview")]
        Ok(k) if k == DISABLED => Ok(None),
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => {
            #[cfg(feature = "internal-preview")]
            {
                match entry_for("Ycode", p)?.get_password() {
                    Ok(k) => Ok(Some(k)),
                    Err(keyring::Error::NoEntry) => Ok(None),
                    Err(e) => Err(AppError::Secret(e.to_string())),
                }
            }
            #[cfg(not(feature = "internal-preview"))]
            Ok(None)
        },
        Err(e) => Err(AppError::Secret(e.to_string())),
    }
}

pub fn delete_key(p: ProviderId) -> AppResult<()> {
    #[cfg(feature = "internal-preview")]
    return entry(p)?.set_password(DISABLED).map_err(|e| AppError::Secret(e.to_string()));
    #[cfg(not(feature = "internal-preview"))]
    match entry(p)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Secret(e.to_string())),
    }
}

pub fn has_key(p: ProviderId) -> bool {
    matches!(get_key(p), Ok(Some(_)))
}

/// Mask for display, e.g. `sk-…9f2c`. Never logs or returns the full key.
pub fn mask(key: &str) -> String {
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    let head: String = key.chars().take(3).collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_hides_key() {
        assert_eq!(mask("sk-abcdefghijkl9f2c"), "sk-…9f2c");
    }

    /// Exercises the real Credential Manager; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn credential_manager_roundtrip() {
        let p = ProviderId::Deepseek;
        let before = get_key(p).unwrap();
        set_key(p, "test-key-123").unwrap();
        assert_eq!(get_key(p).unwrap().as_deref(), Some("test-key-123"));
        delete_key(p).unwrap();
        assert_eq!(get_key(p).unwrap(), None);
        if let Some(k) = before {
            set_key(p, &k).unwrap();
        }
    }
}
