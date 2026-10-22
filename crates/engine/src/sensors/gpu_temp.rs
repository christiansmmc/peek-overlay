//! GPU temperature from the kernel graphics driver (D3DKMT), the source Task Manager uses.
//! No D3D device is created: the adapter is opened by LUID and closed right after each read.

use super::Luid;
use windows::Wdk::Graphics::Direct3D::{
    D3DKMT_ADAPTER_PERFDATA, D3DKMT_CLOSEADAPTER, D3DKMT_OPENADAPTERFROMLUID, D3DKMT_QUERYADAPTERINFO,
    D3DKMTCloseAdapter, D3DKMTOpenAdapterFromLuid, D3DKMTQueryAdapterInfo, KMTQAITYPE_ADAPTERPERFDATA,
};
use windows::Win32::Foundation::LUID;

/// Tenths of a degree to whole degrees, rounding half up; 0 means the GPU has no sensor.
pub fn deci_celsius_to_c(v: u32) -> Option<u8> {
    if v == 0 {
        return None;
    }
    u8::try_from(v.saturating_add(5) / 10).ok()
}

pub fn read_gpu_temp(luid: Luid) -> Option<u8> {
    let mut open = D3DKMT_OPENADAPTERFROMLUID {
        AdapterLuid: LUID { LowPart: luid as u32, HighPart: (luid >> 32) as i32 },
        hAdapter: 0,
    };
    if unsafe { D3DKMTOpenAdapterFromLuid(&mut open) }.is_err() {
        return None;
    }
    let mut perf = D3DKMT_ADAPTER_PERFDATA { PhysicalAdapterIndex: 0, ..Default::default() };
    let mut query = D3DKMT_QUERYADAPTERINFO {
        hAdapter: open.hAdapter,
        Type: KMTQAITYPE_ADAPTERPERFDATA,
        pPrivateDriverData: (&raw mut perf).cast(),
        PrivateDriverDataSize: size_of::<D3DKMT_ADAPTER_PERFDATA>() as u32,
    };
    let status = unsafe { D3DKMTQueryAdapterInfo(&mut query) };
    // Nothing to do if closing fails; the handle is not used again either way.
    let _ = unsafe { D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: open.hAdapter }) };
    if status.is_err() {
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
