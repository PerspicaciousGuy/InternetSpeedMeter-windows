use std::ptr;

use windows_sys::w;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};

use crate::util::wide;

pub fn is_enabled() -> bool {
    unsafe {
        let mut key: HKEY = ptr::null_mut();
        let sub = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        if RegOpenKeyExW(HKEY_CURRENT_USER, sub, 0, KEY_READ, &mut key) != 0 {
            return false;
        }
        let found = RegQueryValueExW(
            key,
            w!("InternetSpeedMeter"),
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        ) == 0;
        RegCloseKey(key);
        found
    }
}

pub fn set_enabled(on: bool) {
    unsafe {
        let mut key: HKEY = ptr::null_mut();
        let sub = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            sub,
            0,
            ptr::null(),
            0,
            KEY_SET_VALUE,
            ptr::null(),
            &mut key,
            ptr::null_mut(),
        ) != 0
        {
            return;
        }
        if on {
            if let Ok(exe) = std::env::current_exe() {
                let value = wide(&format!("\"{}\"", exe.display()));
                RegSetValueExW(
                    key,
                    w!("InternetSpeedMeter"),
                    0,
                    REG_SZ,
                    value.as_ptr() as *const u8,
                    (value.len() * 2) as u32,
                );
            }
        } else {
            RegDeleteValueW(key, w!("InternetSpeedMeter"));
        }
        RegCloseKey(key);
    }
}
