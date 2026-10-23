//! GPU usage from NVIDIA's driver library (NVML): `nvmlDeviceGetUtilizationRates`, the busy
//! metric Steam, Afterburner and RTSS show. Under load it reads higher than the WDDM per-engine
//! utilization that PDH and Task Manager show, so NVIDIA adapters report this one instead.
//!
//! The real `nvml.dll` lives in the NVIDIA driver's folder of the driver store (under System32);
//! `System32\nvml.dll` is only a loader for it. That DLL commits about 20 MB of writable data,
//! and the loader never unloads it, not even after `nvmlShutdown`. So NVML is loaded straight from
//! the driver store, only once GPU usage of an NVIDIA adapter is sampled, and `release` unloads
//! it when sampling stops, which gives the memory back.

use super::Luid;
use super::adapter::{PciLoc, VENDOR_NVIDIA, adapter_driver_store, adapter_pci_loc, adapter_vendor_id};
use libloading::Library;
use libloading::os::windows::{LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32};
use std::ffi::c_void;
use std::mem::offset_of;
use std::path::Path;

const LIB_NAME: &str = "nvml.dll";

/// The NVML device (by index) of the adapter at `adapter`, among `devices` (index, location):
/// the one device at that location. An adapter whose location is unknown gets the only NVML
/// device, if the adapter is NVIDIA's.
pub fn pick_nvml_device(adapter: Option<PciLoc>, devices: &[(u32, PciLoc)], adapter_is_nvidia: bool) -> Option<u32> {
    match adapter {
        Some(loc) => {
            let mut at_loc = devices.iter().filter(|(_, l)| *l == loc).map(|(index, _)| *index);
            let index = at_loc.next()?;
            // Two devices there means two PCI domains, which the adapter's location can't tell apart.
            at_loc.next().is_none().then_some(index)
        }
        None => match devices {
            [(index, _)] if adapter_is_nvidia => Some(*index),
            _ => None,
        },
    }
}

/// GPU usage of NVIDIA adapters through NVML, loaded the first time an NVIDIA adapter's usage is
/// asked for and unloaded by `release`.
#[derive(Default)]
pub struct NvmlUsage {
    state: State,
    /// The last adapter asked for and its NVML device, if it has one. A device is only ever
    /// cached while NVML is `Ready`.
    cached: Option<(Luid, Option<Device>)>,
}

#[derive(Default)]
enum State {
    #[default]
    NotLoaded,
    /// NVML did not load or initialize; not retried until `release`.
    Failed,
    Ready(Nvml),
}

impl NvmlUsage {
    /// GPU usage of the adapter `luid` when it is an NVIDIA device NVML identifies, else `None`.
    /// The adapter's device is looked up again only when `luid` changes.
    pub fn usage(&mut self, luid: Luid) -> Option<u8> {
        let device = match self.cached {
            Some((cached_luid, device)) if cached_luid == luid => device,
            _ => {
                let device = self.device_for(luid);
                self.cached = Some((luid, device));
                device
            }
        };
        let State::Ready(nvml) = &self.state else { return None };
        nvml.gpu_utilization(device?)
    }

    /// Unloads NVML (shut down, DLL freed); the next `usage` loads it again.
    pub fn release(&mut self) {
        *self = Self::default();
    }

    /// The NVML device of the adapter `luid`, matched by PCI location (`pick_nvml_device`).
    /// Loads NVML from the adapter's driver folder if it is NVIDIA's and NVML is not loaded yet.
    fn device_for(&mut self, luid: Luid) -> Option<Device> {
        let is_nvidia = adapter_vendor_id(luid) == Some(VENDOR_NVIDIA);
        if is_nvidia && matches!(self.state, State::NotLoaded) {
            let opened = adapter_driver_store(luid)
                .ok_or_else(|| "no driver store folder".to_string())
                .and_then(|dir| Nvml::open(&dir));
            self.state = match opened {
                Ok(nvml) => State::Ready(nvml),
                Err(e) => {
                    crate::trace!("nvml: {e}");
                    State::Failed
                }
            };
        }
        let State::Ready(nvml) = &self.state else { return None };
        let index = pick_nvml_device(adapter_pci_loc(luid), &nvml.device_locations(), is_nvidia)?;
        nvml.device(index)
    }
}

// Declarations from nvml.h (NVIDIA GPU Deployment Kit). Every function returns an nvmlReturn_t.
type NvmlReturn = i32;
const NVML_SUCCESS: NvmlReturn = 0;

/// `nvmlDevice_t`: an opaque handle, valid until `nvmlShutdown`.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct Device(*mut c_void);

// SAFETY: NVML is thread-safe (nvml.h) and a device handle is not tied to the thread that got it.
unsafe impl Send for Device {}

/// `nvmlPciInfo_t`.
#[repr(C)]
#[derive(Default)]
struct PciInfo {
    _bus_id_legacy: [u8; 16],
    _domain: u32,
    bus: u32,
    device: u32,
    _pci_device_id: u32,
    _pci_sub_system_id: u32,
    _bus_id: [u8; 32],
}

const _: () = assert!(size_of::<PciInfo>() == 68 && offset_of!(PciInfo, bus) == 20 && offset_of!(PciInfo, device) == 24);

/// `nvmlUtilization_t`: percentages over the driver's last sample period (1/6 s to 1 s).
#[repr(C)]
#[derive(Default)]
struct Utilization {
    gpu: u32,
    _memory: u32,
}

type InitFn = unsafe extern "C" fn() -> NvmlReturn;
type ShutdownFn = unsafe extern "C" fn() -> NvmlReturn;
type DeviceCountFn = unsafe extern "C" fn(count: *mut u32) -> NvmlReturn;
type HandleByIndexFn = unsafe extern "C" fn(index: u32, device: *mut Device) -> NvmlReturn;
type PciInfoFn = unsafe extern "C" fn(device: Device, pci: *mut PciInfo) -> NvmlReturn;
type UtilizationFn = unsafe extern "C" fn(device: Device, utilization: *mut Utilization) -> NvmlReturn;

/// An initialized NVML, shut down on drop.
struct Nvml {
    shutdown: ShutdownFn,
    device_count: DeviceCountFn,
    handle_by_index: HandleByIndexFn,
    pci_info: PciInfoFn,
    utilization: UtilizationFn,
    // Keeps the function pointers above valid; dropped after `Drop::drop` shuts NVML down.
    _lib: Library,
}

impl Nvml {
    /// Loads `nvml.dll` from `driver_dir`, the NVIDIA driver's folder in the driver store, and
    /// initializes it.
    fn open(driver_dir: &Path) -> Result<Self, String> {
        let path = driver_dir.join(LIB_NAME);
        // SAFETY: nvml.dll is the NVIDIA driver's own library, in the driver store (writable by
        // the system only). The flags keep its imports to its folder and System32 (the app runs
        // elevated).
        let lib: Library = unsafe {
            libloading::os::windows::Library::load_with_flags(
                &path,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .map_err(|e| format!("loading {}: {e}", path.display()))?
        .into();
        let init = *symbol::<InitFn>(&lib, b"nvmlInit_v2\0")?;
        let shutdown = *symbol::<ShutdownFn>(&lib, b"nvmlShutdown\0")?;
        let device_count = *symbol::<DeviceCountFn>(&lib, b"nvmlDeviceGetCount_v2\0")?;
        let handle_by_index = *symbol::<HandleByIndexFn>(&lib, b"nvmlDeviceGetHandleByIndex_v2\0")?;
        let pci_info = *symbol::<PciInfoFn>(&lib, b"nvmlDeviceGetPciInfo_v3\0")?;
        let utilization = *symbol::<UtilizationFn>(&lib, b"nvmlDeviceGetUtilizationRates\0")?;
        let status = unsafe { init() };
        if status != NVML_SUCCESS {
            return Err(format!("nvmlInit_v2 failed: {status}"));
        }
        Ok(Self { shutdown, device_count, handle_by_index, pci_info, utilization, _lib: lib })
    }

    /// Index and PCI location of every device NVML lists and can locate.
    fn device_locations(&self) -> Vec<(u32, PciLoc)> {
        let mut count = 0;
        if unsafe { (self.device_count)(&mut count) } != NVML_SUCCESS {
            return Vec::new();
        }
        (0..count)
            .filter_map(|index| {
                let device = self.device(index)?;
                let mut pci = PciInfo::default();
                let ok = unsafe { (self.pci_info)(device, &mut pci) } == NVML_SUCCESS;
                ok.then_some((index, PciLoc { bus: pci.bus, device: pci.device }))
            })
            .collect()
    }

    fn device(&self, index: u32) -> Option<Device> {
        let mut device = Device(std::ptr::null_mut());
        let ok = unsafe { (self.handle_by_index)(index, &mut device) } == NVML_SUCCESS;
        ok.then_some(device)
    }

    fn gpu_utilization(&self, device: Device) -> Option<u8> {
        let mut utilization = Utilization::default();
        let ok = unsafe { (self.utilization)(device, &mut utilization) } == NVML_SUCCESS;
        ok.then(|| utilization.gpu.min(100) as u8)
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        // Nothing to do if it fails; NVML is not used again either way.
        let _ = unsafe { (self.shutdown)() };
    }
}

fn symbol<'lib, T>(lib: &'lib Library, name: &[u8]) -> Result<libloading::Symbol<'lib, T>, String> {
    // SAFETY: `T` is the signature nvml.h declares for `name`.
    unsafe { lib.get::<T>(name) }.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(bus: u32, device: u32) -> PciLoc {
        PciLoc { bus, device }
    }

    #[test]
    fn picks_the_device_at_the_adapter_location() {
        let devices = [(0, loc(1, 0)), (1, loc(8, 0))];
        assert_eq!(pick_nvml_device(Some(loc(8, 0)), &devices, true), Some(1));
        assert_eq!(pick_nvml_device(Some(loc(1, 0)), &devices, true), Some(0));
    }

    #[test]
    fn adapter_at_another_location_has_no_device() {
        // E.g. the game runs on the integrated GPU: its location matches no NVML device.
        assert_eq!(pick_nvml_device(Some(loc(12, 0)), &[(0, loc(8, 0))], false), None);
        assert_eq!(pick_nvml_device(Some(loc(12, 0)), &[(0, loc(8, 0))], true), None);
        assert_eq!(pick_nvml_device(Some(loc(8, 0)), &[], true), None);
    }

    #[test]
    fn unknown_location_falls_back_to_the_only_device_of_an_nvidia_adapter() {
        assert_eq!(pick_nvml_device(None, &[(0, loc(8, 0))], true), Some(0));
        assert_eq!(pick_nvml_device(None, &[(0, loc(8, 0))], false), None);
        assert_eq!(pick_nvml_device(None, &[(0, loc(1, 0)), (1, loc(8, 0))], true), None);
        assert_eq!(pick_nvml_device(None, &[], true), None);
    }

    #[test]
    fn location_shared_by_two_devices_is_ambiguous() {
        // Same bus and device in two PCI domains; D3DKMT gives no domain to tell them apart.
        let devices = [(0, loc(8, 0)), (1, loc(8, 0))];
        assert_eq!(pick_nvml_device(Some(loc(8, 0)), &devices, true), None);
    }
}
