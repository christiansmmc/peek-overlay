//! CPU temperature through PawnIO: Tctl on AMD Zen (SMN register), the package sensor on Intel
//! (MSRs). Registers and formulas follow LibreHardwareMonitor and Linux k10temp.

use super::no_sensor;
use crate::model::{Avail, REASON_CPU_UNSUPPORTED, REASON_NEEDS_ADMIN};
use crate::pawnio::{self, AMD_FAMILY17_BIN, INTEL_MSR_BIN, PawnIo, PawnIoError};
use std::arch::x86_64::__cpuid;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateMutexW, MUTEX_MODIFY_STATE, OpenMutexW, ReleaseMutex, SYNCHRONIZATION_SYNCHRONIZE, WaitForSingleObject,
};
use windows::core::w;

/// SMN address of THM_TCON_CUR_TMP (current Tctl in bits 31:21).
const ZEN_THM_TCON_CUR_TMP: u64 = 0x0005_9800;
const ZEN_CUR_TEMP_RANGE_SEL: u32 = 1 << 19;
const ZEN_CUR_TEMP_TJ_SEL: u32 = 0b11 << 16;
const MSR_TEMPERATURE_TARGET: u64 = 0x1A2;
const MSR_PACKAGE_THERM_STATUS: u64 = 0x1B1;
/// How long a read waits for another tool to finish its PCI access (LibreHardwareMonitor's value).
const PCI_BUS_WAIT_MS: u32 = 10;

#[derive(Debug, PartialEq)]
pub enum CpuKind {
    AmdZen,
    Intel,
    Unsupported,
}

/// `family` is the displayed family: the base one, plus the extended one when the base is 0xF.
pub fn cpu_kind_from(vendor: &str, family: u32) -> CpuKind {
    match vendor {
        // The AMDFamily17 module loads on families 17h to 1Ah (Zen to Zen 5).
        "AuthenticAMD" if (0x17..=0x1A).contains(&family) => CpuKind::AmdZen,
        // Family 6 covers every Core and Atom; newer parts use extended families above 0xF.
        // Family 0xF itself (NetBurst) has no package thermal sensor.
        "GenuineIntel" if family == 6 || family > 0xF => CpuKind::Intel,
        _ => CpuKind::Unsupported,
    }
}

/// Vendor from CPUID leaf 0, family from leaf 1.
pub fn cpu_kind() -> CpuKind {
    let leaf0 = __cpuid(0);
    let mut vendor = [0u8; 12];
    for (chunk, reg) in vendor.chunks_exact_mut(4).zip([leaf0.ebx, leaf0.edx, leaf0.ecx]) {
        chunk.copy_from_slice(&reg.to_le_bytes());
    }
    let eax = __cpuid(1).eax;
    let base = (eax >> 8) & 0xF;
    let family = if base == 0xF { base + ((eax >> 20) & 0xFF) } else { base };
    cpu_kind_from(std::str::from_utf8(&vendor).unwrap_or_default(), family)
}

/// Tctl from THM_TCON_CUR_TMP: CUR_TEMP (bits 31:21) in 1/8 °C, reported 49 °C high when the
/// range select bit (19) is set or, on newer parts, when TJ_SEL (bits 17:16) is 0b11.
pub fn zen_tctl_c(smn_reg: u32) -> f32 {
    let t = ((smn_reg >> 21) & 0x7FF) as f32 * 0.125;
    let shifted =
        smn_reg & ZEN_CUR_TEMP_RANGE_SEL != 0 || smn_reg & ZEN_CUR_TEMP_TJ_SEL == ZEN_CUR_TEMP_TJ_SEL;
    if shifted { t - 49.0 } else { t }
}

/// Package temperature: TjMax (MSR_TEMPERATURE_TARGET bits 23:16) minus the distance to it
/// (IA32_PACKAGE_THERM_STATUS bits 22:16).
pub fn intel_pkg_c(temperature_target: u64, package_therm_status: u64) -> f32 {
    let tj_max = (temperature_target >> 16) & 0xFF;
    let readout = (package_therm_status >> 16) & 0x7F;
    tj_max as f32 - readout as f32
}

/// Rounds to whole degrees; `None` outside what a `u8` holds.
fn whole_degrees(celsius: f32) -> Option<u8> {
    let c = celsius.round();
    (0.0..=255.0).contains(&c).then_some(c as u8)
}

/// What `CpuTemp::open` reports for a PawnIO error. Without PawnIO the metric stays `Ok`: the
/// driver state is what the UI shows then.
fn availability(e: PawnIoError) -> Avail {
    match e {
        PawnIoError::AccessDenied => no_sensor(REASON_NEEDS_ADMIN),
        PawnIoError::NotInstalled => Avail::Ok,
        PawnIoError::Failed(_msg) => {
            crate::trace!("cpu temp: {_msg}");
            Avail::Ok
        }
    }
}

enum Source {
    /// The SMN read goes through the PCI config index/data pair that other tools use too.
    AmdZen { pci_bus: Option<PciBusMutex> },
    Intel,
}

pub struct CpuTemp {
    io: PawnIo,
    source: Source,
}

impl CpuTemp {
    /// Opens PawnIO with this CPU's module. The error is the availability to report.
    pub fn open() -> Result<Self, Avail> {
        let kind = cpu_kind();
        let blob = match kind {
            CpuKind::AmdZen => AMD_FAMILY17_BIN,
            CpuKind::Intel => INTEL_MSR_BIN,
            CpuKind::Unsupported => return Err(no_sensor(REASON_CPU_UNSUPPORTED)),
        };
        let lib = pawnio::find_lib().ok_or(Avail::Ok)?;
        let mut io = PawnIo::open(&lib).map_err(availability)?;
        io.load_module(blob).map_err(availability)?;
        let source =
            if kind == CpuKind::AmdZen { Source::AmdZen { pci_bus: PciBusMutex::open() } } else { Source::Intel };
        Ok(Self { io, source })
    }

    pub fn read(&self) -> Option<u8> {
        let celsius = match &self.source {
            Source::AmdZen { pci_bus } => {
                let _held = match pci_bus {
                    Some(mutex) => Some(mutex.lock()?),
                    None => None,
                };
                zen_tctl_c(self.call("ioctl_read_smn", ZEN_THM_TCON_CUR_TMP)? as u32)
            }
            Source::Intel => intel_pkg_c(
                self.call("ioctl_read_msr", MSR_TEMPERATURE_TARGET)?,
                self.call("ioctl_read_msr", MSR_PACKAGE_THERM_STATUS)?,
            ),
        };
        whole_degrees(celsius)
    }

    /// Both module functions take one input (address) and write one output (value).
    fn call(&self, func: &str, address: u64) -> Option<u64> {
        self.io.execute(func, &[address], 1).ok()?.first().copied()
    }
}

/// `Global\Access_PCI`, the mutex hardware tools hold around PCI config accesses; AMDFamily17.p
/// asks for it around `ioctl_read_smn`.
struct PciBusMutex(HANDLE);

// SAFETY: a mutex handle works from any thread, and each lock is released in the same `read`
// call (so on the same thread) that took it.
unsafe impl Send for PciBusMutex {}

impl PciBusMutex {
    /// Creates the mutex if no tool has yet, else opens it. Without it reads go unguarded, as in
    /// LibreHardwareMonitor.
    fn open() -> Option<Self> {
        let name = w!(r"Global\Access_PCI");
        unsafe { CreateMutexW(None, false, name) }
            .or_else(|_| unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE, false, name) })
            .ok()
            .map(Self)
    }

    /// `None` when another tool holds the bus longer than `PCI_BUS_WAIT_MS`.
    fn lock(&self) -> Option<PciBusGuard<'_>> {
        let r = unsafe { WaitForSingleObject(self.0, PCI_BUS_WAIT_MS) };
        // Abandoned: the previous owner exited while holding it; the mutex is ours now anyway.
        (r == WAIT_OBJECT_0 || r == WAIT_ABANDONED).then_some(PciBusGuard(self))
    }
}

impl Drop for PciBusMutex {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

struct PciBusGuard<'a>(&'a PciBusMutex);

impl Drop for PciBusGuard<'_> {
    fn drop(&mut self) {
        let _ = unsafe { ReleaseMutex(self.0.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_cpus() {
        assert_eq!(cpu_kind_from("AuthenticAMD", 0x19), CpuKind::AmdZen);
        assert_eq!(cpu_kind_from("AuthenticAMD", 0x17), CpuKind::AmdZen);
        assert_eq!(cpu_kind_from("AuthenticAMD", 0x15), CpuKind::Unsupported);
        assert_eq!(cpu_kind_from("GenuineIntel", 6), CpuKind::Intel);
        assert_eq!(cpu_kind_from("HygonGenuine", 0x18), CpuKind::Unsupported);
    }
    #[test]
    fn zen_tctl_conversion() {
        assert_eq!(zen_tctl_c(512 << 21), 64.0);
        assert_eq!(zen_tctl_c((512 << 21) | (1 << 19)), 15.0);
        assert_eq!(zen_tctl_c(0x2C0 << 21), 88.0);
    }
    #[test]
    fn intel_package_conversion() {
        assert_eq!(intel_pkg_c(100 << 16, 36 << 16), 64.0);
        assert_eq!(intel_pkg_c(0x5F << 16, 0x1E << 16), 65.0);
    }
    #[test]
    fn zen_tj_sel_also_shifts_by_49() {
        assert_eq!(zen_tctl_c((512 << 21) | (0b11 << 16)), 15.0);
        assert_eq!(zen_tctl_c((512 << 21) | (0b01 << 16)), 64.0);
    }
    #[test]
    fn rounds_to_whole_degrees() {
        assert_eq!(whole_degrees(64.625), Some(65));
        assert_eq!(whole_degrees(64.375), Some(64));
        assert_eq!(whole_degrees(-3.0), None);
    }
    #[test]
    fn pawnio_errors_map_to_availability() {
        assert_eq!(availability(PawnIoError::AccessDenied), no_sensor(REASON_NEEDS_ADMIN));
        assert_eq!(availability(PawnIoError::NotInstalled), Avail::Ok);
        assert_eq!(availability(PawnIoError::Failed("x".into())), Avail::Ok);
    }
}
