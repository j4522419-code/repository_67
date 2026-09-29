//! Reading the Windows registry.

use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, KEY_READ, RRF_RT_REG_DWORD,
    RRF_RT_REG_SZ,
};

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

pub fn read_dword(root: HKEY, key: &str, value: &str) -> Option<u32> {
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    let found = unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(key),
            &HSTRING::from(value),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut data).cast()),
            Some(&mut size),
        )
    };
    (found == ERROR_SUCCESS).then_some(data)
}

/// The names of a key's subkeys; empty if the key doesn't exist.
pub fn subkeys(root: HKEY, key: &str) -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        let mut handle = HKEY::default();
        if RegOpenKeyExW(root, &HSTRING::from(key), None, KEY_READ, &mut handle) != ERROR_SUCCESS {
            return names;
        }
        for index in 0.. {
            // Key names are at most 255 characters.
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            let result = RegEnumKeyExW(
                handle,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            );
            if result != ERROR_SUCCESS {
                break;
            }
            names.push(String::from_utf16_lossy(&name[..len as usize]));
        }
        let _ = RegCloseKey(handle);
    }
    names
}
