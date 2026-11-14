use std::path::PathBuf;

use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

use crate::settings::Units;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Copies `s` into a fixed-size UTF-16 buffer, truncating and NUL-terminating.
pub fn copy_wide(dst: &mut [u16], s: &str) {
    let mut n = 0;
    for c in s.encode_utf16() {
        if n + 1 >= dst.len() {
            break;
        }
        dst[n] = c;
        n += 1;
    }
    dst[n] = 0;
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("InternetSpeedMeter")
}

pub fn today_key() -> String {
    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay)
}

pub fn fmt_speed(bytes_per_sec: f64, units: Units) -> String {
    match units {
        Units::Bytes => {
            if bytes_per_sec >= 1_048_576.0 {
                format!("{:.1} MB/s", bytes_per_sec / 1_048_576.0)
            } else if bytes_per_sec >= 1024.0 {
                format!("{:.0} KB/s", bytes_per_sec / 1024.0)
            } else {
                format!("{:.0} B/s", bytes_per_sec)
            }
        }
        Units::Bits => {
            let bits = bytes_per_sec * 8.0;
            if bits >= 1_000_000.0 {
                format!("{:.1} Mbps", bits / 1_000_000.0)
            } else if bits >= 1000.0 {
                format!("{:.0} Kbps", bits / 1000.0)
            } else {
                format!("{:.0} bps", bits)
            }
        }
    }
}

pub fn fmt_bytes(b: u64) -> String {
    let f = b as f64;
    if f >= 1_073_741_824.0 {
        format!("{:.2} GB", f / 1_073_741_824.0)
    } else if f >= 1_048_576.0 {
        format!("{:.1} MB", f / 1_048_576.0)
    } else if f >= 1024.0 {
        format!("{:.0} KB", f / 1024.0)
    } else {
        format!("{} B", b)
    }
}
