//! At-rest encryption for stored secrets (API keys) using Windows DPAPI, scoped to the
//! current user account: the ciphertext can't be read by another user or on another
//! machine. Stored as `dpapi:` + base64(ciphertext).
//!
//! Decryption is deliberately tolerant: a value without the `dpapi:` prefix (legacy
//! plaintext) is returned unchanged, so existing data keeps working and is re-encrypted
//! on its next write. If encryption is unavailable, values are stored as-is - no worse
//! than the previous plaintext behavior.

const PREFIX: &str = "dpapi:";

#[cfg(windows)]
pub fn protect(plain: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    if plain.is_empty() {
        return String::new();
    }
    let mut input = plain.as_bytes().to_vec();
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
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize);
            let encoded = STANDARD.encode(bytes);
            let _ = LocalFree(Some(HLOCAL(out.pbData as *mut core::ffi::c_void)));
            format!("{PREFIX}{encoded}")
        } else {
            plain.to_string()
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

#[cfg(not(windows))]
pub fn protect(plain: &str) -> String {
    plain.to_string()
}

#[cfg(not(windows))]
pub fn unprotect(stored: &str) -> String {
    stored.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unprotect_passes_through_plaintext() {
        // Legacy plaintext (no prefix) and empty values are returned unchanged, so
        // pre-encryption data keeps working.
        assert_eq!(
            unprotect("sk-ant-legacy-plaintext"),
            "sk-ant-legacy-plaintext"
        );
        assert_eq!(unprotect(""), "");
        assert_eq!(unprotect("{\"api_key\":\"abc\"}"), "{\"api_key\":\"abc\"}");
    }

    #[cfg(windows)]
    #[test]
    fn protect_then_unprotect_roundtrips() {
        let secret = "sk-ant-12345-SECRET-key";
        let enc = protect(secret);
        assert!(
            enc.starts_with("dpapi:"),
            "ciphertext should be tagged: {enc}"
        );
        assert_ne!(enc, secret, "stored value must not be the plaintext");
        assert_eq!(unprotect(&enc), secret, "round-trips back to the secret");
    }
}
