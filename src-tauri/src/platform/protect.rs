//! Encrypting data for this Windows account only (DPAPI), so clipboard
//! history on disk is unreadable to other accounts or on another PC.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

use super::message;

pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    let input = blob(data);
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(message)?;
        Ok(take(output))
    }
}

pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    let input = blob(data);
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(message)?;
        Ok(take(output))
    }
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        // Only read from; the API just isn't declared that way.
        pbData: data.as_ptr().cast_mut(),
    }
}

/// Copies out data Windows allocated for us, then frees it.
unsafe fn take(blob: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let bytes = std::slice::from_raw_parts(blob.pbData, blob.cbData as usize).to_vec();
    LocalFree(Some(HLOCAL(blob.pbData.cast())));
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_hides_the_data() {
        let secret = b"copied password? never on disk in the clear";
        let sealed = protect(secret).unwrap();
        assert!(!sealed.windows(secret.len()).any(|w| w == secret));
        assert_eq!(unprotect(&sealed).unwrap(), secret);
        assert!(unprotect(b"not sealed").is_err());
    }
}
