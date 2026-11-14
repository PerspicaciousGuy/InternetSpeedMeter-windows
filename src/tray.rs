use std::ptr;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO, WM_APP};

use crate::util::copy_wide;

pub const WM_TRAY: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ICON_ID;
    nid
}

pub fn add(hwnd: HWND, icon: HICON, tip: &str) {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    copy_wide(&mut nid.szTip, tip);
    unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
}

pub fn set_tip(hwnd: HWND, tip: &str) {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_TIP;
    copy_wide(&mut nid.szTip, tip);
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
}

pub fn remove(hwnd: HWND) {
    let nid = data(hwnd);
    unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

/// Draws a small blue badge with a down and an up arrow; no icon resource needed.
pub fn make_icon() -> HICON {
    const N: i32 = 32;
    let mut pixels = vec![0u32; (N * N) as usize];
    for y in 0..N {
        for x in 0..N {
            let (r, cx, cy) = (7, x.clamp(7, N - 1 - 7), y.clamp(7, N - 1 - 7));
            if (x - cx) * (x - cx) + (y - cy) * (y - cy) > r * r {
                continue;
            }
            let down = (x - 10).abs() <= 1 && (8..=16).contains(&y)
                || (17..=23).contains(&y) && (x - 10).abs() <= 23 - y;
            let up = (x - 22).abs() <= 1 && (15..=23).contains(&y)
                || (8..=14).contains(&y) && (x - 22).abs() <= y - 8;
            pixels[(y * N + x) as usize] = if down || up { 0xFFFF_FFFF } else { 0xFF1F6FEB };
        }
    }

    unsafe {
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = N;
        bmi.bmiHeader.biHeight = -N;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;

        let screen = GetDC(ptr::null_mut());
        let mut bits = ptr::null_mut();
        let color = CreateDIBSection(screen, &bmi, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
        ReleaseDC(ptr::null_mut(), screen);
        if color.is_null() || bits.is_null() {
            return ptr::null_mut();
        }
        ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u32, pixels.len());

        let mask_bits = [0u8; (N as usize / 8) * N as usize];
        let mask = CreateBitmap(N, N, 1, 1, mask_bits.as_ptr() as *const _);

        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}
