//! Hardware sensors: CPU and GPU usage via PDH and GPU temperature via D3DKMT, all without
//! elevation; CPU temperature through the PawnIO driver, which needs elevation.
//!
//! PDH rate counters need two collects, so each counter's first sample after `new()`, or after a
//! pause longer than `MAX_SAMPLE_GAP` (the controller only samples while the overlay or the
//! settings window is visible), is `None` instead of an average over the whole pause.

pub mod adapter;
pub mod cpu_temp;
pub mod cpu_usage;
pub mod gpu_temp;
pub mod gpu_usage;

pub use adapter::default_dedicated_luid;
pub use cpu_temp::{CpuKind, CpuTemp, cpu_kind, cpu_kind_from, intel_pkg_c, zen_tctl_c};
pub use cpu_usage::CpuUsage;
pub use gpu_temp::{deci_celsius_to_c, read_gpu_temp};
pub use gpu_usage::{EngineInstance, GpuUsage, parse_engine_instance, pick_adapter_usage};

use crate::model::{Avail, REASON_NO_CPU_COUNTERS, REASON_NO_GPU_COUNTERS, REASON_NO_GPU_TEMP};
use crate::settings::Metrics;
use std::time::{Duration, Instant};
use windows::Win32::System::Performance::{
    PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhOpenQueryW,
};
use windows::core::{HSTRING, PCWSTR};

/// Adapter LUID packed as `HighPart << 32 | LowPart`.
pub type Luid = u64;

pub fn luid_from_parts(high: i32, low: u32) -> Luid {
    (high as u32 as u64) << 32 | low as u64
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SensorSample {
    pub cpu_use: Option<u8>,
    pub gpu_use: Option<u8>,
    pub gpu_temp: Option<u8>,
    pub cpu_temp: Option<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SensorAvailability {
    pub cpu_use: Avail,
    pub gpu_use: Avail,
    pub gpu_temp: Avail,
    pub cpu_temp: Avail,
}

pub struct Sensors {
    cpu: Option<CpuUsage>,
    gpu: Option<GpuUsage>,
    cpu_temp: Option<CpuTemp>,
    default_luid: Option<Luid>,
    availability: SensorAvailability,
}

// The controller keeps `Sensors` behind a mutex shared by the native and Tauri threads.
const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<Sensors>();
};

fn no_sensor(reason: &str) -> Avail {
    Avail::NoSensor { reason: reason.into() }
}

impl Default for Sensors {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensors {
    /// Opens every sensor it can; the ones that fail are reported by `availability()`.
    pub fn new() -> Sensors {
        let cpu = CpuUsage::new().inspect_err(|e| crate::trace!("cpu usage: {e}")).ok();
        let gpu = GpuUsage::new().inspect_err(|e| crate::trace!("gpu usage: {e}")).ok();
        let default_luid = default_dedicated_luid();
        let gpu_temp_ok = default_luid.and_then(read_gpu_temp).is_some();
        let (cpu_temp, cpu_temp_avail) = open_cpu_temp();
        let availability = SensorAvailability {
            cpu_use: if cpu.is_some() { Avail::Ok } else { no_sensor(REASON_NO_CPU_COUNTERS) },
            gpu_use: if gpu.is_some() { Avail::Ok } else { no_sensor(REASON_NO_GPU_COUNTERS) },
            gpu_temp: if gpu_temp_ok { Avail::Ok } else { no_sensor(REASON_NO_GPU_TEMP) },
            cpu_temp: cpu_temp_avail,
        };
        Sensors { cpu, gpu, cpu_temp, default_luid, availability }
    }

    /// Reopens the CPU temperature sensor, e.g. after PawnIO was installed.
    pub fn reload_cpu_temp(&mut self) {
        // Close the old PawnIO handle before opening a new one.
        self.cpu_temp = None;
        let (cpu_temp, avail) = open_cpu_temp();
        self.cpu_temp = cpu_temp;
        self.availability.cpu_temp = avail;
    }

    pub fn availability(&self) -> SensorAvailability {
        self.availability.clone()
    }

    /// Reads only the metrics `wanted` asks for. The temperature comes from the adapter the usage
    /// sample picked (the game's); without a usage sample it falls back to the dedicated adapter.
    pub fn sample(&mut self, wanted: &Metrics, game_pid: Option<u32>) -> SensorSample {
        let cpu_use = if wanted.cpu_use { self.cpu.as_mut().and_then(CpuUsage::sample) } else { None };
        let usage = if wanted.gpu_use {
            self.gpu.as_mut().and_then(|gpu| gpu.sample(game_pid, self.default_luid))
        } else {
            None
        };
        let gpu_temp = if wanted.gpu_temp {
            usage.map(|(luid, _)| luid).or(self.default_luid).and_then(read_gpu_temp)
        } else {
            None
        };
        let cpu_temp = if wanted.cpu_temp { self.cpu_temp.as_ref().and_then(CpuTemp::read) } else { None };
        SensorSample { cpu_use, gpu_use: usage.map(|(_, pct)| pct), gpu_temp, cpu_temp }
    }
}

fn open_cpu_temp() -> (Option<CpuTemp>, Avail) {
    match CpuTemp::open() {
        Ok(sensor) => (Some(sensor), Avail::Ok),
        Err(avail) => (None, avail),
    }
}

/// A rate counter whose previous collect is older than this yields `None` (see module docs).
const MAX_SAMPLE_GAP: Duration = Duration::from_secs(2);

/// One PDH query holding one counter added by its English path (counter names are localized).
pub(crate) struct PdhQuery {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    last_collect: Option<Instant>,
}

// SAFETY: PDH query and counter handles are not tied to the thread that opened them, and every
// use goes through `&mut self` or `&self` of the owner, so moving the owner across threads is fine.
unsafe impl Send for PdhQuery {}

impl PdhQuery {
    pub(crate) fn open(path: &str) -> Result<Self, String> {
        let mut query = PDH_HQUERY::default();
        let status = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) };
        if status != 0 {
            return Err(format!("PdhOpenQueryW failed: {status:#010X}"));
        }
        let mut counter = PDH_HCOUNTER::default();
        let status = unsafe { PdhAddEnglishCounterW(query, &HSTRING::from(path), 0, &mut counter) };
        if status != 0 {
            unsafe { PdhCloseQuery(query) };
            return Err(format!("PdhAddEnglishCounterW({path}) failed: {status:#010X}"));
        }
        Ok(Self { query, counter, last_collect: None })
    }

    /// Collects a new raw value; `true` when the counter now holds a rate over a recent interval.
    pub(crate) fn collect(&mut self) -> bool {
        let now = Instant::now();
        let recent = self.last_collect.is_some_and(|t| now.duration_since(t) <= MAX_SAMPLE_GAP);
        let ok = unsafe { PdhCollectQueryData(self.query) } == 0;
        self.last_collect = ok.then_some(now);
        ok && recent
    }

    pub(crate) fn counter(&self) -> PDH_HCOUNTER {
        self.counter
    }
}

impl Drop for PdhQuery {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.query) };
    }
}

/// `true` for the per-value statuses PDH uses for a usable formatted value.
pub(crate) fn pdh_value_ok(cstatus: u32) -> bool {
    cstatus == PDH_CSTATUS_VALID_DATA || cstatus == PDH_CSTATUS_NEW_DATA
}

/// Rounds a percentage to an integer in `0..=100`.
pub(crate) fn percent(value: f64) -> u8 {
    value.round().clamp(0.0, 100.0) as u8
}
