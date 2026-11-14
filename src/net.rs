use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::time::Instant;

use windows_sys::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfTable2, MIB_IF_TABLE2,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;

use crate::util::wide_to_string;

const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
const IF_TYPE_TUNNEL: u32 = 131;

struct Adapter {
    luid: u64,
    alias: String,
    rx: u64,
    tx: u64,
}

/// One sampling result, covering the time since the previous `tick`.
pub struct Sample {
    pub down_bps: f64,
    pub up_bps: f64,
    pub delta_rx: u64,
    pub delta_tx: u64,
    /// Aliases of the adapters that are currently up, for the adapter menu.
    pub adapters: Vec<String>,
}

pub struct Sampler {
    prev: HashMap<u64, (u64, u64)>,
    last: Instant,
}

impl Sampler {
    pub fn new() -> Self {
        Sampler {
            prev: HashMap::new(),
            last: Instant::now(),
        }
    }

    /// Reads the interface counters and returns the traffic since the last call.
    /// `selected` limits the totals to one adapter alias; `None` sums all of them.
    pub fn tick(&mut self, selected: Option<&str>) -> Sample {
        let now = Instant::now();
        let secs = now.duration_since(self.last).as_secs_f64().max(0.001);
        self.last = now;

        let mut next = HashMap::new();
        let (mut d_rx, mut d_tx) = (0u64, 0u64);
        let mut names = Vec::new();

        for a in snapshot() {
            if let Some(&(prx, ptx)) = self.prev.get(&a.luid) {
                // A counter lower than before means the adapter was reset.
                let drx = if a.rx >= prx { a.rx - prx } else { a.rx };
                let dtx = if a.tx >= ptx { a.tx - ptx } else { a.tx };
                if selected.map_or(true, |s| s == a.alias) {
                    d_rx += drx;
                    d_tx += dtx;
                }
            }
            next.insert(a.luid, (a.rx, a.tx));
            if !names.contains(&a.alias) {
                names.push(a.alias);
            }
        }
        self.prev = next;

        Sample {
            down_bps: d_rx as f64 / secs,
            up_bps: d_tx as f64 / secs,
            delta_rx: d_rx,
            delta_tx: d_tx,
            adapters: names,
        }
    }
}

/// Connected physical adapters. Virtual ones (VPNs, Hyper-V switches, tunnels) are
/// skipped because their traffic is already counted on the hardware adapter below.
fn snapshot() -> Vec<Adapter> {
    let mut out = Vec::new();
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = ptr::null_mut();
        if GetIfTable2(&mut table) != 0 || table.is_null() {
            return out;
        }
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        for r in rows {
            let hardware = r.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
            if r.OperStatus != IfOperStatusUp
                || !hardware
                || r.Type == IF_TYPE_SOFTWARE_LOOPBACK
                || r.Type == IF_TYPE_TUNNEL
            {
                continue;
            }
            out.push(Adapter {
                luid: r.InterfaceLuid.Value,
                alias: wide_to_string(&r.Alias),
                rx: r.InOctets,
                tx: r.OutOctets,
            });
        }
        FreeMibTable(table as *const c_void);
    }
    out
}
