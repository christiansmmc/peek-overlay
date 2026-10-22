param(
    [Parameter(Mandatory)][string]$Out
)

# Saves a PNG of the whole virtual screen (every monitor) in physical pixels.
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class ScreenshotNative {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
}
'@
# Before any size is read, so the virtual screen comes in physical pixels.
[void][ScreenshotNative]::SetProcessDPIAware()
Add-Type -AssemblyName System.Drawing

# SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN
$x = [ScreenshotNative]::GetSystemMetrics(76)
$y = [ScreenshotNative]::GetSystemMetrics(77)
$w = [ScreenshotNative]::GetSystemMetrics(78)
$h = [ScreenshotNative]::GetSystemMetrics(79)

$Out = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Out)
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
try {
    $g.CopyFromScreen($x, $y, 0, 0, $bmp.Size)
    $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
} finally {
    $g.Dispose()
    $bmp.Dispose()
}
Write-Output "saved $Out (${w}x${h} at $x,$y)"
