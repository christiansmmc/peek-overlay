param(
    [Parameter(Mandatory)][string]$Name,
    [int]$Seconds = 10
)

# Samples a process once per second: max private working set, average CPU, and
# the number of msedgewebview2.exe children. WMI property names are English
# even on localized Windows.
$cores = [Environment]::ProcessorCount
$maxWs = 0
$cpuSum = 0.0
$samples = 0
$procId = $null

for ($i = 0; $i -lt $Seconds; $i++) {
    $p = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "Name='$Name'" |
        Select-Object -First 1
    if ($p) {
        $procId = $p.IDProcess
        if ($p.WorkingSetPrivate -gt $maxWs) { $maxWs = $p.WorkingSetPrivate }
        $cpuSum += $p.PercentProcessorTime / $cores
        $samples++
    }
    Start-Sleep -Seconds 1
}

if (-not $samples) {
    Write-Error "process '$Name' not found"
    exit 1
}

$children = 0
if ($procId) {
    $children = @(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe' AND ParentProcessId=$procId").Count
}

$ws = [math]::Round($maxWs / 1MB, 1)
$cpu = [math]::Round($cpuSum / $samples, 2)
Write-Output "private_ws_mb=$ws cpu_pct_avg=$cpu webview_children=$children"
