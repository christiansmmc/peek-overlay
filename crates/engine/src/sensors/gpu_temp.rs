//! GPU temperature from the kernel graphics driver (D3DKMT), the source Task Manager uses.
//! No D3D device is created: the adapter is opened by LUID and closed right after each read.

use super::Luid;
use super::adapter::query_adapter_info;
use windows::Wdk::Graphics::Direct3D::{D3DKMT_ADAPTER_PERFDATA, KMTQAITYPE_ADAPTERPERFDATA};

/// Tenths of a degree to whole degrees, rounding half up; 0 means the GPU has no sensor.
pub fn deci_celsius_to_c(v: u32) -> Option<u8> {
    if v == 0 {
        return None;
    }
    u8::try_from(v.saturating_add(5) / 10).ok()
}

pub fn read_gpu_temp(luid: Luid) -> Option<u8> {
    let mut perf = D3DKMT_ADAPTER_PERFDATA { PhysicalAdapterIndex: 0, ..Default::default() };
    // SAFETY: KMTQAITYPE_ADAPTERPERFDATA fills a D3DKMT_ADAPTER_PERFDATA.
    if !unsafe { query_adapter_info(luid, KMTQAITYPE_ADAPTERPERFDATA, &mut perf) } {
        return None;
    }
    deci_celsius_to_c(perf.Temperature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deci_celsius_conversion() {
        assert_eq!(deci_celsius_to_c(712), Some(71));
        assert_eq!(deci_celsius_to_c(715), Some(72));
        assert_eq!(deci_celsius_to_c(0), None);
    }
}
