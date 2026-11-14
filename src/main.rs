#![windows_subsystem = "windows"]

mod autostart;
mod net;
mod overlay;
mod settings;
mod tray;
mod usage;
mod util;

use std::cell::RefCell;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use windows_sys::w;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use net::Sampler;
use overlay::Overlay;
use settings::{Color, Settings, Show, Size, Units};
use usage::Usage;
use util::{fmt_bytes, fmt_speed, today_key, wide};

const TIMER_ID: usize = 1;
const NUDGE_PX: i32 = 20;

const ID_UNITS_BYTES: u32 = 101;
const ID_UNITS_BITS: u32 = 102;
const ID_SHOW_BOTH: u32 = 111;
const ID_SHOW_DOWN: u32 = 112;
const ID_SHOW_UP: u32 = 113;
const ID_INTERVAL_1: u32 = 121;
const ID_INTERVAL_2: u32 = 122;
const ID_SIZE_SMALL: u32 = 131;
const ID_SIZE_MEDIUM: u32 = 132;
const ID_SIZE_LARGE: u32 = 133;
const ID_COLOR_AUTO: u32 = 141;
const ID_COLOR_WHITE: u32 = 142;
const ID_COLOR_BLACK: u32 = 143;
const ID_ADAPTER_ALL: u32 = 150;
const ID_MOVE_LEFT: u32 = 161;
const ID_MOVE_RIGHT: u32 = 162;
const ID_MOVE_RESET: u32 = 163;
const ID_AUTOSTART: u32 = 170;
const ID_EXIT: u32 = 199;
const ID_ADAPTER_BASE: u32 = 200;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct App {
    hwnd: HWND,
    settings: Settings,
    sampler: Sampler,
    usage: Usage,
    overlay: Overlay,
    icon: HICON,
    adapters: Vec<String>,
    down_bps: f64,
    up_bps: f64,
    ticks: u32,
    last_flush: Instant,
    last_tip: Instant,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the app state. Returns `None` if the state is already borrowed, which
/// happens when a Win32 call made from inside `f` re-enters the window procedure.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

impl App {
    fn on_tick(&mut self) {
        let s = self.sampler.tick(self.settings.adapter.as_deref());
        self.usage.add(&today_key(), s.delta_rx, s.delta_tx);
        self.down_bps = s.down_bps;
        self.up_bps = s.up_bps;
        self.adapters = s.adapters;

        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks % 10 == 0 {
            self.overlay.refresh_theme();
        }
        self.refresh_overlay();

        if self.last_flush.elapsed() >= Duration::from_secs(60) {
            self.usage.flush();
            self.last_flush = Instant::now();
        }
        if self.last_tip.elapsed() >= Duration::from_secs(5) {
            tray::set_tip(self.hwnd, &self.tooltip());
            self.last_tip = Instant::now();
        }
    }

    fn refresh_overlay(&mut self) {
        let units = self.settings.units;
        let down = format!("\u{2193} {}", fmt_speed(self.down_bps, units));
        let up = format!("\u{2191} {}", fmt_speed(self.up_bps, units));
        let lines = match self.settings.show {
            Show::Both => vec![down, up],
            Show::Down => vec![down],
            Show::Up => vec![up],
        };
        self.overlay.update(&lines, &self.settings);
    }

    fn tooltip(&self) -> String {
        let day = today_key();
        let (trx, ttx) = self.usage.today(&day);
        let (mrx, mtx) = self.usage.month(&day);
        format!(
            "Internet Speed Meter\nToday: \u{2193} {}  \u{2191} {}\nMonth: \u{2193} {}  \u{2191} {}",
            fmt_bytes(trx),
            fmt_bytes(ttx),
            fmt_bytes(mrx),
            fmt_bytes(mtx)
        )
    }

    fn apply_settings(&mut self) {
        self.settings.save();
        unsafe {
            SetTimer(
                self.hwnd,
                TIMER_ID,
                self.settings.interval_secs * 1000,
                None,
            );
        }
        self.overlay.invalidate();
        self.refresh_overlay();
    }

    fn shutdown(&mut self) {
        self.usage.flush();
        tray::remove(self.hwnd);
    }
}

fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let mutex = CreateMutexW(ptr::null(), 0, w!("Local\\InternetSpeedMeter.SingleInstance"));
        if mutex.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        let hinstance = GetModuleHandleW(ptr::null());
        let class = w!("InternetSpeedMeterOverlay");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: ptr::null_mut(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class,
            hIconSm: ptr::null_mut(),
        };
        if RegisterClassExW(&wc) == 0 {
            return;
        }
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class,
            w!("Internet Speed Meter"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            ptr::null_mut(),
            ptr::null_mut(),
            hinstance,
            ptr::null(),
        );
        if hwnd.is_null() {
            return;
        }

        let settings = Settings::load();
        let icon = tray::make_icon();
        let mut app = App {
            hwnd,
            sampler: Sampler::new(),
            usage: Usage::load(),
            overlay: Overlay::new(hwnd),
            icon,
            adapters: Vec::new(),
            down_bps: 0.0,
            up_bps: 0.0,
            ticks: 0,
            last_flush: Instant::now(),
            last_tip: Instant::now(),
            settings,
        };
        app.sampler.tick(None); // baseline so the first real tick has a delta
        tray::add(hwnd, icon, &app.tooltip());
        SetTimer(hwnd, TIMER_ID, app.settings.interval_secs * 1000, None);
        app.refresh_overlay();
        APP.with(|cell| *cell.borrow_mut() = Some(app));

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            with_app(|a| a.on_tick());
            0
        }
        tray::WM_TRAY => {
            let event = lp as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            0
        }
        WM_SETTINGCHANGE | WM_DISPLAYCHANGE | WM_DPICHANGED => {
            with_app(|a| {
                a.overlay.refresh_theme();
                a.overlay.invalidate();
            });
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_ENDSESSION => {
            if wp != 0 {
                with_app(|a| a.shutdown());
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            with_app(|a| {
                tray::add(a.hwnd, a.icon, &a.tooltip());
                a.overlay.invalidate();
            });
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn add_item(menu: HMENU, id: u32, text: &str, checked: bool) {
    let t = wide(text);
    let flags = MF_STRING | if checked { MF_CHECKED } else { 0 };
    AppendMenuW(menu, flags, id as usize, t.as_ptr());
}

unsafe fn add_info(menu: HMENU, text: &str) {
    let t = wide(text);
    AppendMenuW(menu, MF_STRING | MF_GRAYED | MF_DISABLED, 0, t.as_ptr());
}

unsafe fn add_submenu(menu: HMENU, text: &str, sub: HMENU) {
    let t = wide(text);
    AppendMenuW(menu, MF_POPUP | MF_STRING, sub as usize, t.as_ptr());
}

unsafe fn show_menu(hwnd: HWND) {
    let Some((s, adapters, today, month)) = with_app(|a| {
        let day = today_key();
        (
            a.settings.clone(),
            a.adapters.clone(),
            a.usage.today(&day),
            a.usage.month(&day),
        )
    }) else {
        return;
    };

    let menu = CreatePopupMenu();
    add_info(
        menu,
        &format!("Today: \u{2193} {}  \u{2191} {}", fmt_bytes(today.0), fmt_bytes(today.1)),
    );
    add_info(
        menu,
        &format!("This month: \u{2193} {}  \u{2191} {}", fmt_bytes(month.0), fmt_bytes(month.1)),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());

    let units = CreatePopupMenu();
    add_item(units, ID_UNITS_BYTES, "Bytes per second (MB/s)", s.units == Units::Bytes);
    add_item(units, ID_UNITS_BITS, "Bits per second (Mbps)", s.units == Units::Bits);
    add_submenu(menu, "Units", units);

    let show = CreatePopupMenu();
    add_item(show, ID_SHOW_BOTH, "Download and upload", s.show == Show::Both);
    add_item(show, ID_SHOW_DOWN, "Download only", s.show == Show::Down);
    add_item(show, ID_SHOW_UP, "Upload only", s.show == Show::Up);
    add_submenu(menu, "Show", show);

    let interval = CreatePopupMenu();
    add_item(interval, ID_INTERVAL_1, "Every second", s.interval_secs == 1);
    add_item(interval, ID_INTERVAL_2, "Every 2 seconds", s.interval_secs == 2);
    add_submenu(menu, "Update interval", interval);

    let size = CreatePopupMenu();
    add_item(size, ID_SIZE_SMALL, "Small", s.size == Size::Small);
    add_item(size, ID_SIZE_MEDIUM, "Medium", s.size == Size::Medium);
    add_item(size, ID_SIZE_LARGE, "Large", s.size == Size::Large);
    add_submenu(menu, "Text size", size);

    let color = CreatePopupMenu();
    add_item(color, ID_COLOR_AUTO, "Match Windows theme", s.color == Color::Auto);
    add_item(color, ID_COLOR_WHITE, "White", s.color == Color::White);
    add_item(color, ID_COLOR_BLACK, "Black", s.color == Color::Black);
    add_submenu(menu, "Text color", color);

    let adapter = CreatePopupMenu();
    add_item(adapter, ID_ADAPTER_ALL, "All adapters", s.adapter.is_none());
    for (i, name) in adapters.iter().enumerate() {
        add_item(
            adapter,
            ID_ADAPTER_BASE + i as u32,
            name,
            s.adapter.as_deref() == Some(name.as_str()),
        );
    }
    add_submenu(menu, "Network adapter", adapter);

    let position = CreatePopupMenu();
    add_item(position, ID_MOVE_LEFT, "Move left", false);
    add_item(position, ID_MOVE_RIGHT, "Move right", false);
    add_item(position, ID_MOVE_RESET, "Reset position", false);
    add_submenu(menu, "Position", position);

    AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
    add_item(menu, ID_AUTOSTART, "Start with Windows", autostart::is_enabled());
    AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
    add_item(menu, ID_EXIT, "Exit", false);

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    // Required so the menu closes when the user clicks elsewhere.
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        hwnd,
        ptr::null(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);

    if cmd > 0 {
        handle_command(hwnd, cmd as u32, &adapters);
    }
}

unsafe fn handle_command(hwnd: HWND, id: u32, adapters: &[String]) {
    if id == ID_EXIT {
        with_app(|a| a.shutdown());
        DestroyWindow(hwnd);
        return;
    }
    with_app(|a| {
        let s = &mut a.settings;
        match id {
            ID_UNITS_BYTES => s.units = Units::Bytes,
            ID_UNITS_BITS => s.units = Units::Bits,
            ID_SHOW_BOTH => s.show = Show::Both,
            ID_SHOW_DOWN => s.show = Show::Down,
            ID_SHOW_UP => s.show = Show::Up,
            ID_INTERVAL_1 => s.interval_secs = 1,
            ID_INTERVAL_2 => s.interval_secs = 2,
            ID_SIZE_SMALL => s.size = Size::Small,
            ID_SIZE_MEDIUM => s.size = Size::Medium,
            ID_SIZE_LARGE => s.size = Size::Large,
            ID_COLOR_AUTO => s.color = Color::Auto,
            ID_COLOR_WHITE => s.color = Color::White,
            ID_COLOR_BLACK => s.color = Color::Black,
            ID_ADAPTER_ALL => s.adapter = None,
            ID_MOVE_LEFT => s.offset_x -= NUDGE_PX,
            ID_MOVE_RIGHT => s.offset_x += NUDGE_PX,
            ID_MOVE_RESET => s.offset_x = 0,
            ID_AUTOSTART => autostart::set_enabled(!autostart::is_enabled()),
            i if i >= ID_ADAPTER_BASE => {
                if let Some(name) = adapters.get((i - ID_ADAPTER_BASE) as usize) {
                    s.adapter = Some(name.clone());
                }
            }
            _ => return,
        }
        a.apply_settings();
    });
}
