//! Reading the Windows registry.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{RegGetValueW, HKEY, RRF_RT_REG_SZ};

pub use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

/// A text value, with environment variables expanded. `None` for the key's
/// default value. Empty values count as missing.
pub fn read_string(root: HKEY, key: &str, value: Option<&str>) -> Option<String> {
    let key = HSTRING::from(key);
    let value = value.map(HSTRING::from);
    let value = value
        .as_ref()
        .map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr()));
    unsafe {
        let mut size = 0u32;
        let found = RegGetValueW(
            root,
            &key,
            value,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        );
        if found != ERROR_SUCCESS {
            return None;
        }
        let mut buffer = vec![0u16; size as usize / 2 + 1];
        let mut size = (buffer.len() * 2) as u32;
        let read = RegGetValueW(
            root,
            &key,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        );
        if read != ERROR_SUCCESS {
            return None;
        }
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let text = String::from_utf16_lossy(&buffer[..len]);
        (!text.trim().is_empty()).then_some(text)
    }
}
