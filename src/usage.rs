use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::util::data_dir;

const MAX_DAYS: usize = 400;

/// Daily download/upload totals, persisted as `date,rx,tx` lines.
pub struct Usage {
    days: BTreeMap<String, (u64, u64)>,
    dirty: bool,
    path: PathBuf,
}

impl Usage {
    pub fn load() -> Self {
        let path = data_dir().join("usage.csv");
        let mut days = BTreeMap::new();
        if let Ok(text) = fs::read_to_string(&path) {
            for line in text.lines() {
                let mut it = line.split(',');
                if let (Some(d), Some(rx), Some(tx)) = (it.next(), it.next(), it.next()) {
                    if let (Ok(rx), Ok(tx)) = (rx.parse(), tx.parse()) {
                        days.insert(d.to_string(), (rx, tx));
                    }
                }
            }
        }
        while days.len() > MAX_DAYS {
            let first = days.keys().next().cloned();
            match first {
                Some(k) => days.remove(&k),
                None => break,
            };
        }
        Usage {
            days,
            dirty: false,
            path,
        }
    }

    pub fn add(&mut self, day: &str, rx: u64, tx: u64) {
        if rx == 0 && tx == 0 {
            return;
        }
        let e = self.days.entry(day.to_string()).or_insert((0, 0));
        e.0 += rx;
        e.1 += tx;
        self.dirty = true;
    }

    pub fn today(&self, day: &str) -> (u64, u64) {
        self.days.get(day).copied().unwrap_or((0, 0))
    }

    /// Total for the month containing `day` (`YYYY-MM-DD`).
    pub fn month(&self, day: &str) -> (u64, u64) {
        let prefix = &day[..day.len().min(7)];
        self.days
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .fold((0, 0), |a, (_, v)| (a.0 + v.0, a.1 + v.1))
    }

    pub fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        if let Some(dir) = self.path.parent() {
            if fs::create_dir_all(dir).is_err() {
                return;
            }
        }
        let mut text = String::new();
        for (d, (rx, tx)) in &self.days {
            text.push_str(&format!("{d},{rx},{tx}\n"));
        }
        let tmp = self.path.with_extension("tmp");
        if fs::write(&tmp, text).is_ok() && fs::rename(&tmp, &self.path).is_ok() {
            self.dirty = false;
        }
    }
}
