# Roadmap

What's next for Peek after 0.1.0. Roughly in order of how much a player would notice.

## Next

### FPS for Vulkan and OpenGL games
Today FPS and the frametime graph come from the DXGI and D3D9 ETW providers only, so Vulkan and OpenGL games show "—".
- Vulkan/OpenGL presents still reach the kernel graphics stack: track them through `Microsoft-Windows-DxgKrnl` present events (Present / Blit / Flip / PresentHistory), the way PresentMon does in `PresentData/PresentMonTraceConsumer.cpp`.
- Keep it in `crates/engine/src/frames/etw.rs`; `FrameShared::on_present` stays the single entry point, so detection and the graph don't change.
- Test with a Vulkan title (e.g. a vkcube sample or a game with a Vulkan renderer) against `PresentMon` output.

### Code-sign the installers
The `.exe` and `.msi` aren't signed, so SmartScreen warns on first run and the elevated binary carries no publisher.
- Get a code-signing certificate (or use Azure Trusted Signing) and configure `bundle.windows.certificateThumbprint` / `signCommand` in `src-tauri/tauri.conf.json`.
- Sign `Peek.exe`, the NSIS installer/uninstaller and the MSI; add the signing step to the release checklist.

### Process-wide DLL search hardening
Peek runs elevated. `PawnIOLib.dll` and NVIDIA's `nvml.dll` are already loaded by full path with `LOAD_LIBRARY_SEARCH_*` flags, but DLLs they load at runtime still follow the default search order (which includes the current directory and `PATH`).
- Call `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS)` at the very start of `src-tauri/src/main.rs` (before Tauri or any sensor loads a DLL).
- Check nothing breaks (WebView2 loader, NVML, PawnIO) on a clean machine.

## Later / known small issues

- **Game detected in a window never upgrades to the fullscreen path.** If a game is first confirmed while windowed (overlay shown by hotkey) and then goes fullscreen, auto-show won't apply to it until it's restarted. See `crates/engine/src/game.rs`.
- **First frame after showing the overlay** shows "—" for GPU/CPU usage for ~0.5 s (PDH needs two collects).
- **Optimus laptops:** polling NVML while the settings window is open may keep a sleeping dGPU awake. Untested — no laptop available.
- **Overlay hide latency while ETW stops:** when `sync()` runs on the native thread, `FrameCapture::stop()` can delay the queued hide by a few ms. Measure on a slower machine before changing the ordering.
- **Hotkey capture edge cases** in `src/components/ShortcutField.tsx`: ignore keydowns after the capture ends (`live.current` guard), refresh the startup conflict message after a cancel, and give the driver sheet Esc/focus handling.
- **Settings migration:** settings written by pre-release builds under `%APPDATA%\perf-overlay` aren't moved to `%APPDATA%\Peek`.
- **Uninstall cancelled at "app is running":** the NSIS pre-uninstall hook removes the logon task before that prompt; it comes back the next time Peek starts.
