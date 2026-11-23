//! At-rest encryption for stored secrets (API keys) using Windows DPAPI, scoped to the
//! current user account: the ciphertext can't be read by another user or on another
//! machine. Stored as `dpapi:` + base64(ciphertext).
//!
//! Reading is deliberately tolerant: a value without the `dpapi:` prefix (legacy
//! plaintext) is returned unchanged, so existing data keeps working and is re-encrypted
//! on its next write. Writing fails closed: if encryption is unavailable (DPAPI returns
//! an error, or the build is not Windows) `protect` returns an error and the caller
//! stores nothing, so a key is never written to disk in plaintext.

const PREFIX: &str = "dpapi:";

const UNAVAILABLE: &str =
    "Could not encrypt the key with Windows, so it was not saved. Nothing was stored.";

/// Tag and encode the ciphertext produced by `encrypt`. If `encrypt` returns `None` the
/// result is an error, never the plaintext.
#[cfg_attr(not(windows), allow(dead_code))]
fn seal(plain: &str, encrypt: impl FnOnce(&[u8]) -> Option<Vec<u8>>) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    if plain.is_empty() {
        return Ok(String::new());
    }
    match encrypt(plain.as_bytes()) {
        Some(bytes) if !bytes.is_empty() => Ok(format!("{PREFIX}{}", STANDARD.encode(bytes))),
        _ => Err(UNAVAILABLE.to_string()),
    }
}

/// Encrypt `plain` for storage. Errors rather than falling back to plaintext.
#[cfg(windows)]
pub fn protect(plain: &str) -> Result<String, String> {
    seal(plain, dpapi_encrypt)
}

#[cfg(windows)]
fn dpapi_encrypt(plain: &[u8]) -> Option<Vec<u8>> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    let mut input = plain.to_vec();
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_mut_ptr(),
    };
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: in_blob points at `input` (alive for the call); `out` is filled by DPAPI
    // with a LocalAlloc'd buffer we free with LocalFree. 0x1 = CRYPTPROTECT_UI_FORBIDDEN
    // (never show UI - this is a background store).
    unsafe {
        if CryptProtectData(&in_blob, PCWSTR::null(), None, None, None, 0x1, &mut out).is_ok()
            && !out.pbData.is_null()
        {
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            let _ = LocalFree(Some(HLOCAL(out.pbData as *mut core::ffi::c_void)));
            Some(bytes)
        } else {
            None
        }
    }
}

#[cfg(windows)]
pub fn unprotect(stored: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let Some(b64) = stored.strip_prefix(PREFIX) else {
        return stored.to_string(); // legacy plaintext (or not ours)
    };
    let Ok(mut bytes) = STANDARD.decode(b64) else {
        return stored.to_string();
    };
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_mut_ptr(),
    };
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: as in `protect`; on failure we return the stored value untouched.
    unsafe {
        if CryptUnprotectData(&in_blob, None, None, None, None, 0x1, &mut out).is_ok()
            && !out.pbData.is_null()
        {
            let plain = std::slice::from_raw_parts(out.pbData, out.cbData as usize);
            let s = String::from_utf8_lossy(plain).into_owned();
            let _ = LocalFree(Some(HLOCAL(out.pbData as *mut core::ffi::c_void)));
            s
        } else {
            stored.to_string()
        }
    }
}

/// The app is Windows only; there is no encryption backend elsewhere, so refuse to
/// store a key rather than keep it in plaintext.
#[cfg(not(windows))]
pub fn protect(_plain: &str) -> Result<String, String> {
    Err("Key storage is only available on Windows, so the key was not saved.".to_string())
}

#[cfg(not(windows))]
pub fn unprotect(stored: &str) -> String {
    stored.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_KEY: &str = "test-key-0000-not-a-real-secret";

    #[test]
    fn unprotect_passes_through_plaintext() {
        // Legacy plaintext (no prefix) and empty values are returned unchanged, so
        // pre-encryption data keeps working.
        assert_eq!(
            unprotect("legacy-plaintext-test-key"),
            "legacy-plaintext-test-key"
        );
        assert_eq!(unprotect(""), "");
        assert_eq!(unprotect("{\"api_key\":\"abc\"}"), "{\"api_key\":\"abc\"}");
    }

    #[test]
    fn seal_fails_closed_when_encryption_fails() {
        let out = seal(FAKE_KEY, |_| None);
        let err = out.expect_err("a failed encryption must be an error");
        assert!(!err.contains(FAKE_KEY), "the error must not echo the key");
        // An empty ciphertext is also a failure, never a silent plaintext store.
        assert!(seal(FAKE_KEY, |_| Some(Vec::new())).is_err());
    }

    #[test]
    fn seal_tags_ciphertext_and_never_contains_the_plaintext() {
        let out = seal(FAKE_KEY, |p| Some(p.iter().rev().copied().collect())).unwrap();
        assert!(out.starts_with("dpapi:"));
        assert!(!out.contains(FAKE_KEY));
    }

    #[cfg(not(windows))]
    #[test]
    fn protect_refuses_to_store_off_windows() {
        assert!(protect(FAKE_KEY).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn protect_then_unprotect_roundtrips() {
        let enc = protect(FAKE_KEY).expect("DPAPI should be available");
        assert!(
            enc.starts_with("dpapi:"),
            "ciphertext should be tagged: {enc}"
        );
        assert_ne!(enc, FAKE_KEY, "stored value must not be the plaintext");
        assert_eq!(unprotect(&enc), FAKE_KEY, "round-trips back to the secret");
    }
}
