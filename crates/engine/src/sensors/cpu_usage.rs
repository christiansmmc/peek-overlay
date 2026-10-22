//! Total CPU usage as Task Manager shows it: `% Processor Utility` of all processors.

use super::{PdhQuery, pdh_value_ok, percent};
use windows::Win32::System::Performance::{PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PdhGetFormattedCounterValue};

const COUNTER: &str = r"\Processor Information(_Total)\% Processor Utility";

pub struct CpuUsage {
    query: PdhQuery,
}

impl CpuUsage {
    pub fn new() -> Result<Self, String> {
        Ok(Self { query: PdhQuery::open(COUNTER)? })
    }

    /// Usage since the previous call, limited to 100 (utility exceeds 100 under boost).
    pub fn sample(&mut self) -> Option<u8> {
        if !self.query.collect() {
            return None;
        }
        let (counter, mut value) = (self.query.counter(), PDH_FMT_COUNTERVALUE::default());
        let status = unsafe { PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, None, &mut value) };
        if status != 0 || !pdh_value_ok(value.CStatus) {
            return None;
        }
        Some(percent(unsafe { value.Anonymous.doubleValue }))
    }
}
