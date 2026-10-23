//! Identifies GPUs: the dedicated one and an adapter's vendor through a DXGI factory; an adapter's
//! PCI location and driver folder through the kernel graphics driver (D3DKMT). No D3D device is
//! created.

use super::{Luid, luid_from_parts};
use std::ffi::OsString;
use std::mem::offset_of;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use windows::Wdk::Graphics::Direct3D::{
    D3DDDI_QUERYREGISTRY_DRIVERSTOREPATH, D3DDDI_QUERYREGISTRY_INFO, D3DDDI_QUERYREGISTRY_STATUS_SUCCESS,
    D3DKMT_ADAPTERADDRESS, D3DKMT_CLOSEADAPTER, D3DKMT_OPENADAPTERFROMLUID, D3DKMT_QUERYADAPTERINFO,
    D3DKMTCloseAdapter, D3DKMTOpenAdapterFromLuid, D3DKMTQueryAdapterInfo, KMTQAITYPE_ADAPTERADDRESS,
    KMTQAITYPE_QUERYREGISTRY, KMTQUERYADAPTERINFOTYPE,
};
use windows::Win32::Foundation::{LUID, MAX_PATH};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
};
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;

/// PCI vendor ID of NVIDIA, as DXGI reports it.
pub const VENDOR_NVIDIA: u32 = 0x10DE;

/// Where an adapter sits on the PCI bus. D3DKMT reports no PCI domain, so there is none here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciLoc {
    pub bus: u32,
    pub device: u32,
}

/// LUID of the hardware adapter with the most dedicated video memory.
pub fn default_dedicated_luid() -> Option<Luid> {
    let mut best: Option<(usize, Luid)> = None;
    for desc in adapter_descs()? {
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        if best.is_none_or(|(memory, _)| desc.DedicatedVideoMemory > memory) {
            best = Some((desc.DedicatedVideoMemory, desc_luid(&desc)));
        }
    }
    best.map(|(_, luid)| luid)
}

/// PCI vendor ID of the adapter with this LUID (e.g. `VENDOR_NVIDIA`).
pub fn adapter_vendor_id(luid: Luid) -> Option<u32> {
    adapter_descs()?.into_iter().find(|desc| desc_luid(desc) == luid).map(|desc| desc.VendorId)
}

/// PCI bus and device of the adapter with this LUID.
pub fn adapter_pci_loc(luid: Luid) -> Option<PciLoc> {
    let mut address = D3DKMT_ADAPTERADDRESS::default();
    // SAFETY: KMTQAITYPE_ADAPTERADDRESS fills a D3DKMT_ADAPTERADDRESS.
    let ok = unsafe { query_adapter_info(luid, KMTQAITYPE_ADAPTERADDRESS, &mut address) };
    ok.then_some(PciLoc { bus: address.BusNumber, device: address.DeviceNumber })
}

/// Folder of the adapter's driver in the driver store (e.g. NVIDIA's, which holds the real
/// `nvml.dll`), from the kernel graphics driver.
pub fn adapter_driver_store(luid: Luid) -> Option<PathBuf> {
    let mut query = RegistryQuery {
        info: D3DDDI_QUERYREGISTRY_INFO { QueryType: D3DDDI_QUERYREGISTRY_DRIVERSTOREPATH, ..Default::default() },
        _room: [0; MAX_PATH as usize],
    };
    // SAFETY: KMTQAITYPE_QUERYREGISTRY fills a D3DDDI_QUERYREGISTRY_INFO whose output string runs
    // past its end, into `_room`; the size passed covers both.
    let ok = unsafe { query_adapter_info(luid, KMTQAITYPE_QUERYREGISTRY, &mut query) };
    if !ok || query.info.Status != D3DDDI_QUERYREGISTRY_STATUS_SUCCESS {
        return None;
    }
    let start = offset_of!(RegistryQuery, info) + offset_of!(D3DDDI_QUERYREGISTRY_INFO, Anonymous);
    let len = (query.info.OutputValueSize as usize / 2).min((size_of::<RegistryQuery>() - start) / 2);
    // SAFETY: the `len` UTF-16 units from `start` lie inside `query`, at an even offset.
    let wide = unsafe { std::slice::from_raw_parts((&raw const query).cast::<u8>().add(start).cast::<u16>(), len) };
    let raw = String::from_utf16(wide).ok()?;
    driver_store_dir(raw.trim_end_matches('\0'), &system_windows_dir()?)
}

/// `D3DDDI_QUERYREGISTRY_INFO` with room after it for the string it returns.
#[repr(C)]
struct RegistryQuery {
    info: D3DDDI_QUERYREGISTRY_INFO,
    _room: [u16; MAX_PATH as usize],
}

/// The Windows folder the kernel's `\SystemRoot` stands for.
fn system_windows_dir() -> Option<PathBuf> {
    let mut buf = [0u16; MAX_PATH as usize];
    let len = unsafe { GetSystemWindowsDirectoryW(Some(&mut buf)) } as usize;
    (len > 0 && len < buf.len()).then(|| PathBuf::from(OsString::from_wide(&buf[..len])))
}

/// The adapter driver's folder in the driver store, from the kernel's `\SystemRoot\...` form
/// (`raw`) and the Windows folder. Only plain folders inside `System32\DriverStore` are accepted.
pub fn driver_store_dir(raw: &str, windows_dir: &Path) -> Option<PathBuf> {
    let below_root = strip_prefix_ignore_case(raw, r"\SystemRoot\")?;
    let below_store = strip_prefix_ignore_case(below_root, r"System32\DriverStore\")?;
    // No `..`, drive letters, other separators or empty parts that could lead elsewhere.
    let plain = below_store
        .split('\\')
        .all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains([':', '/']));
    plain.then(|| windows_dir.join(below_root))
}

fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &s[prefix.len()..])
}

/// Descriptions of every adapter DXGI lists, software ones included.
fn adapter_descs() -> Option<Vec<DXGI_ADAPTER_DESC1>> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    // EnumAdapters1 fails with DXGI_ERROR_NOT_FOUND past the last adapter.
    let adapters = (0..).map_while(|i| unsafe { factory.EnumAdapters1(i) }.ok());
    Some(adapters.filter_map(|adapter| unsafe { adapter.GetDesc1() }.ok()).collect())
}

fn desc_luid(desc: &DXGI_ADAPTER_DESC1) -> Luid {
    luid_from_parts(desc.AdapterLuid.HighPart, desc.AdapterLuid.LowPart)
}

/// One `D3DKMTQueryAdapterInfo` of `kind` into `data`, on the adapter opened by LUID and closed
/// right after. `true` when the query succeeded.
///
/// # Safety
/// `T` must be the plain-data structure the driver fills for `kind`.
pub(crate) unsafe fn query_adapter_info<T>(luid: Luid, kind: KMTQUERYADAPTERINFOTYPE, data: &mut T) -> bool {
    let mut open = D3DKMT_OPENADAPTERFROMLUID {
        AdapterLuid: LUID { LowPart: luid as u32, HighPart: (luid >> 32) as i32 },
        hAdapter: 0,
    };
    if unsafe { D3DKMTOpenAdapterFromLuid(&mut open) }.is_err() {
        return false;
    }
    let mut query = D3DKMT_QUERYADAPTERINFO {
        hAdapter: open.hAdapter,
        Type: kind,
        pPrivateDriverData: (data as *mut T).cast(),
        PrivateDriverDataSize: size_of::<T>() as u32,
    };
    let status = unsafe { D3DKMTQueryAdapterInfo(&mut query) };
    // Nothing to do if closing fails; the handle is not used again either way.
    let _ = unsafe { D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: open.hAdapter }) };
    status.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: &str = r"C:\WINDOWS";

    #[test]
    fn driver_store_path_from_the_kernel_becomes_a_win32_path() {
        let raw = r"\SystemRoot\System32\DriverStore\FileRepository\nv_dispi.inf_amd64_feedb8c0271ca811";
        assert_eq!(
            driver_store_dir(raw, Path::new(WINDOWS)),
            Some(PathBuf::from(r"C:\WINDOWS\System32\DriverStore\FileRepository\nv_dispi.inf_amd64_feedb8c0271ca811"))
        );
        let raw = r"\systemroot\system32\driverstore\FileRepository\x";
        assert_eq!(
            driver_store_dir(raw, Path::new(WINDOWS)),
            Some(PathBuf::from(r"C:\WINDOWS\system32\driverstore\FileRepository\x"))
        );
    }

    #[test]
    fn driver_store_path_outside_the_driver_store_is_rejected() {
        for raw in [
            r"\SystemRoot\Temp\x",
            r"C:\Users\me\x",
            r"\SystemRoot\System32\DriverStore\..\..\Temp",
            r"\SystemRoot\System32\DriverStore\FileRepository\x/../../../Temp",
            r"\SystemRoot\System32\DriverStore\FileRepository\C:\x",
            r"\SystemRoot\System32\DriverStore\",
            "",
        ] {
            assert_eq!(driver_store_dir(raw, Path::new(WINDOWS)), None, "{raw}");
        }
    }
}
