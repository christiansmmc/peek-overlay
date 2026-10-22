//! Picks the dedicated GPU through a DXGI factory only; no D3D device is created.

use super::{Luid, luid_from_parts};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1};

/// LUID of the hardware adapter with the most dedicated video memory.
pub fn default_dedicated_luid() -> Option<Luid> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    let mut best: Option<(usize, Luid)> = None;
    // EnumAdapters1 fails with DXGI_ERROR_NOT_FOUND past the last adapter.
    for adapter in (0..).map_while(|i| unsafe { factory.EnumAdapters1(i) }.ok()) {
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else { continue };
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        if best.is_none_or(|(memory, _)| desc.DedicatedVideoMemory > memory) {
            let luid = luid_from_parts(desc.AdapterLuid.HighPart, desc.AdapterLuid.LowPart);
            best = Some((desc.DedicatedVideoMemory, luid));
        }
    }
    best.map(|(_, luid)| luid)
}
