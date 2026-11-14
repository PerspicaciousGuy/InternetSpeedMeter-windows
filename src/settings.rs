use std::fs;

use crate::util::data_dir;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Units {
    Bytes,
    Bits,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Show {
    Both,
    Down,
    Up,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Small,
    Medium,
    Large,
}

impl Size {
    pub fn points(self) -> i32 {
        match self {
            Size::Small => 9,
            Size::Medium => 10,
            Size::Large => 12,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Auto,
    White,
    Black,
}

#[derive(Clone)]
pub struct Settings {
    pub units: Units,
    pub show: Show,
    pub interval_secs: u32,
    pub size: Size,
    pub color: Color,
    /// Alias of the single adapter to watch; `None` means all hardware adapters.
    pub adapter: Option<String>,
    /// Horizontal nudge in 96-dpi pixels (negative moves left).
    pub offset_x: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            units: Units::Bytes,
            show: Show::Both,
            interval_secs: 1,
            size: Size::Medium,
            color: Color::Auto,
            adapter: None,
            offset_x: 0,
        }
    }
}

impl Settings {
    pub fn is_first_run() -> bool {
        !data_dir().join("settings.ini").exists()
    }

    pub fn load() -> Self {
        let mut s = Settings::default();
        let Ok(text) = fs::read_to_string(data_dir().join("settings.ini")) else {
            return s;
        };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            match k.trim() {
                "units" => s.units = if v == "bits" { Units::Bits } else { Units::Bytes },
                "show" => {
                    s.show = match v {
                        "down" => Show::Down,
                        "up" => Show::Up,
                        _ => Show::Both,
                    }
                }
                "interval" => s.interval_secs = if v == "2" { 2 } else { 1 },
                "size" => {
                    s.size = match v {
                        "small" => Size::Small,
                        "large" => Size::Large,
                        _ => Size::Medium,
                    }
                }
                "color" => {
                    s.color = match v {
                        "white" => Color::White,
                        "black" => Color::Black,
                        _ => Color::Auto,
                    }
                }
                "adapter" if !v.is_empty() => s.adapter = Some(v.to_string()),
                "offset_x" => s.offset_x = v.parse().unwrap_or(0),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let dir = data_dir();
        if fs::create_dir_all(&dir).is_err() {
            return;
        }
        let text = format!(
            "units={}\nshow={}\ninterval={}\nsize={}\ncolor={}\nadapter={}\noffset_x={}\n",
            if self.units == Units::Bits { "bits" } else { "bytes" },
            match self.show {
                Show::Both => "both",
                Show::Down => "down",
                Show::Up => "up",
            },
            self.interval_secs,
            match self.size {
                Size::Small => "small",
                Size::Medium => "medium",
                Size::Large => "large",
            },
            match self.color {
                Color::Auto => "auto",
                Color::White => "white",
                Color::Black => "black",
            },
            self.adapter.as_deref().unwrap_or(""),
            self.offset_x,
        );
        let _ = fs::write(dir.join("settings.ini"), text);
    }
}
