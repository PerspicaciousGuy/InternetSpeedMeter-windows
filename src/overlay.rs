use std::ffi::c_void;
use std::ptr;

use windows_sys::w;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GdiFlush,
    GetDC, GetTextExtentPoint32W, ReleaseDC, SelectObject, SetBkMode, SetTextColor, BITMAPINFO,
    BITMAPINFOHEADER, BLENDFUNCTION, DIB_RGB_COLORS, DT_LEFT, DT_NOCLIP, DT_NOPREFIX, DT_RIGHT,
    DT_SINGLELINE, HFONT,
};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetWindowRect, IsWindowVisible, SetWindowPos, ShowWindow,
    UpdateLayeredWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE,
    SW_SHOWNOACTIVATE, ULW_ALPHA,
};

use crate::settings::{Color, Settings, Units};
use crate::util::wide;

type Rgb = (u8, u8, u8);

const PAD: i32 = 2;
const WHITE: Rgb = (255, 255, 255);
const DARK: Rgb = (24, 24, 24);

/// One row of the meter: a direction arrow and its formatted speed.
#[derive(Clone, PartialEq)]
pub struct Line {
    pub down: bool,
    pub value: String,
}

#[derive(Clone, Copy, PartialEq)]
struct Palette {
    text: Rgb,
    down: Rgb,
    up: Rgb,
}

impl Palette {
    fn new(color: Color, light_theme: bool) -> Palette {
        let mono = |c: Rgb| Palette { text: c, down: c, up: c };
        match color {
            Color::White => mono(WHITE),
            Color::Black => mono(DARK),
            // Colored arrows, in darker shades on a light taskbar for contrast.
            Color::Auto if light_theme => Palette {
                text: DARK,
                down: (16, 124, 84),
                up: (190, 96, 0),
            },
            Color::Auto => Palette {
                text: WHITE,
                down: (72, 200, 140),
                up: (255, 176, 80),
            },
        }
    }
}

/// Font and measured text sizes for the current size/DPI/unit combination.
struct Layout {
    key: (i32, u32, bool),
    font: HFONT,
    arrow_w: i32,
    gap: i32,
    width: i32,
    line_h: i32,
}

impl Layout {
    fn new(points: i32, dpi: u32, bits: bool) -> Layout {
        unsafe {
            let font = CreateFontW(
                -(points * dpi as i32 / 72),
                0,
                0,
                0,
                600, // semibold reads better on a busy taskbar
                0,
                0,
                0,
                1, // DEFAULT_CHARSET
                0,
                0,
                4, // ANTIALIASED_QUALITY: grayscale coverage that we turn into alpha
                0,
                w!("Segoe UI"),
            );
            let dc = CreateCompatibleDC(ptr::null_mut());
            let old = SelectObject(dc, font);
            let measure = |s: &str| {
                let t = wide(s);
                let mut size = SIZE { cx: 0, cy: 0 };
                GetTextExtentPoint32W(dc, t.as_ptr(), (t.len() - 1) as i32, &mut size);
                size
            };
            let arrow = measure("\u{2193}");
            let value = measure(if bits { "999.9 Mbps" } else { "999.9 MB/s" });
            SelectObject(dc, old);
            DeleteDC(dc);

            let gap = (dpi as i32 * 5 / 96).max(3);
            Layout {
                key: (points, dpi, bits),
                font,
                arrow_w: arrow.cx,
                gap,
                width: PAD * 2 + arrow.cx + gap + value.cx,
                line_h: arrow.cy.max(value.cy),
            }
        }
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.font) };
    }
}

/// The click-through, always-on-top layered window parked on top of the taskbar.
pub struct Overlay {
    hwnd: HWND,
    layout: Option<Layout>,
    visible: bool,
    last_lines: Vec<Line>,
    last_pos: (i32, i32),
    last_palette: Option<Palette>,
    light_theme: bool,
}

impl Overlay {
    pub fn new(hwnd: HWND) -> Overlay {
        let mut o = Overlay {
            hwnd,
            layout: None,
            visible: false,
            last_lines: Vec::new(),
            last_pos: (0, 0),
            last_palette: None,
            light_theme: false,
        };
        o.refresh_theme();
        o
    }

    /// Forces a full redraw on the next `update`.
    pub fn invalidate(&mut self) {
        self.last_lines.clear();
    }

    /// Top-left of the meter while it is on screen, used to anchor the menu.
    pub fn anchor(&self) -> Option<(i32, i32)> {
        self.visible.then_some(self.last_pos)
    }

    pub fn refresh_theme(&mut self) {
        let light = taskbar_is_light();
        if light != self.light_theme {
            self.light_theme = light;
            self.invalidate();
        }
    }

    fn hide(&mut self) {
        if self.visible {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            self.visible = false;
            self.invalidate();
        }
    }

    pub fn update(&mut self, lines: &[Line], settings: &Settings) {
        let taskbar = if lines.is_empty() || fullscreen_app_active() {
            None
        } else {
            find_taskbar()
        };
        let Some(tb) = taskbar else {
            self.hide();
            return;
        };

        let dpi = match unsafe { GetDpiForWindow(self.hwnd) } {
            0 => 96,
            d => d,
        };
        let key = (settings.size.points(), dpi, settings.units == Units::Bits);
        if self.layout.as_ref().map(|l| l.key) != Some(key) {
            self.layout = Some(Layout::new(key.0, key.1, key.2));
            self.invalidate();
        }
        let (width, line_h) = match &self.layout {
            Some(l) => (l.width, l.line_h),
            None => return,
        };

        let scale = |v: i32| v * dpi as i32 / 96;
        let height = lines.len() as i32 * line_h;
        let right_edge = tb.tray_left.unwrap_or(tb.rect.right - scale(320));
        let x = (right_edge - width - scale(8) + scale(settings.offset_x)).max(tb.rect.left);
        let y = tb.rect.top + ((tb.rect.bottom - tb.rect.top) - height) / 2;
        let palette = Palette::new(settings.color, self.light_theme);

        let changed = self.last_lines != lines
            || self.last_pos != (x, y)
            || self.last_palette != Some(palette);
        if changed {
            if let Some(layout) = &self.layout {
                draw(self.hwnd, layout, lines, palette, x, y);
            }
            self.last_lines = lines.to_vec();
            self.last_pos = (x, y);
            self.last_palette = Some(palette);
        }
        if !self.visible {
            unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
            self.visible = true;
        }
        // The taskbar jumps above us whenever it is clicked, so claim the top again.
        unsafe {
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

/// Renders the lines as white-on-black with grayscale antialiasing, then converts the
/// coverage into per-pixel alpha so the text blends onto whatever the taskbar looks like.
fn draw(hwnd: HWND, layout: &Layout, lines: &[Line], palette: Palette, x: i32, y: i32) {
    let (w, h) = (layout.width, layout.line_h * lines.len() as i32);
    if w <= 0 || h <= 0 {
        return;
    }
    unsafe {
        let screen = GetDC(ptr::null_mut());
        let mem = CreateCompatibleDC(screen);

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        let mut bits: *mut c_void = ptr::null_mut();
        let bmp = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);

        if !bmp.is_null() && !bits.is_null() {
            let old_bmp = SelectObject(mem, bmp);
            let old_font = SelectObject(mem, layout.font);
            SetBkMode(mem, 1); // TRANSPARENT
            SetTextColor(mem, 0x00FF_FFFF);
            let flags = DT_NOPREFIX | DT_SINGLELINE | DT_NOCLIP;
            for (i, line) in lines.iter().enumerate() {
                let top = i as i32 * layout.line_h;
                let bottom = top + layout.line_h;

                let arrow = wide(if line.down { "\u{2193}" } else { "\u{2191}" });
                let mut r = RECT { left: PAD, top, right: PAD + layout.arrow_w, bottom };
                DrawTextW(mem, arrow.as_ptr(), -1, &mut r, DT_LEFT | flags);

                // Right-aligned so the units line up between the two rows.
                let value = wide(&line.value);
                let mut r = RECT {
                    left: PAD + layout.arrow_w + layout.gap,
                    top,
                    right: w - PAD,
                    bottom,
                };
                DrawTextW(mem, value.as_ptr(), -1, &mut r, DT_RIGHT | flags);
            }
            GdiFlush();

            let arrow_end = PAD + layout.arrow_w + layout.gap / 2;
            let px = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
            for (i, p) in px.iter_mut().enumerate() {
                let a = *p & 0xFF;
                if a == 0 {
                    *p = 0;
                    continue;
                }
                let (col, row) = (i as i32 % w, (i as i32 / w / layout.line_h) as usize);
                let (r, g, b) = if col < arrow_end {
                    if lines.get(row).map_or(true, |l| l.down) {
                        palette.down
                    } else {
                        palette.up
                    }
                } else {
                    palette.text
                };
                let (r, g, b) = (r as u32, g as u32, b as u32);
                *p = (a << 24) | ((r * a / 255) << 16) | ((g * a / 255) << 8) | (b * a / 255);
            }

            let dst = POINT { x, y };
            let src = POINT { x: 0, y: 0 };
            let size = SIZE { cx: w, cy: h };
            let blend = BLENDFUNCTION {
                BlendOp: 0, // AC_SRC_OVER
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: 1, // AC_SRC_ALPHA
            };
            UpdateLayeredWindow(hwnd, screen, &dst, &size, mem, &src, 0, &blend, ULW_ALPHA);

            SelectObject(mem, old_font);
            SelectObject(mem, old_bmp);
        }
        if !bmp.is_null() {
            DeleteObject(bmp);
        }
        DeleteDC(mem);
        ReleaseDC(ptr::null_mut(), screen);
    }
}

struct Taskbar {
    rect: RECT,
    /// Left edge of the notification area, when Windows reports a usable one.
    tray_left: Option<i32>,
}

fn find_taskbar() -> Option<Taskbar> {
    unsafe {
        let tb = FindWindowW(w!("Shell_TrayWnd"), ptr::null());
        if tb.is_null() || IsWindowVisible(tb) == 0 {
            return None;
        }
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(tb, &mut rect) == 0 {
            return None;
        }
        let tray = FindWindowExW(tb, ptr::null_mut(), w!("TrayNotifyWnd"), ptr::null());
        let mut tray_left = None;
        if !tray.is_null() {
            let mut r: RECT = std::mem::zeroed();
            if GetWindowRect(tray, &mut r) != 0
                && r.right > r.left
                && r.left > rect.left
                && r.left < rect.right
            {
                tray_left = Some(r.left);
            }
        }
        Some(Taskbar { rect, tray_left })
    }
}

fn fullscreen_app_active() -> bool {
    let mut state = 0;
    let hr = unsafe { SHQueryUserNotificationState(&mut state) };
    hr == 0
        && (state == QUNS_BUSY
            || state == QUNS_RUNNING_D3D_FULL_SCREEN
            || state == QUNS_PRESENTATION_MODE)
}

fn taskbar_is_light() -> bool {
    let mut value: u32 = 0;
    let mut size: u32 = 4;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            &mut value as *mut u32 as *mut c_void,
            &mut size,
        )
    };
    rc == 0 && value == 1
}
