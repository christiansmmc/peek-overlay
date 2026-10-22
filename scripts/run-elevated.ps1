param(
    [Parameter(Mandatory)][string]$Exe,
    [string]$ArgList = '',
    [Parameter(Mandatory)][string]$Out
)

# Runs $Exe elevated (one UAC prompt), waits for it to exit, prints its stdout
# and stderr, and exits with its exit code.
$ErrorActionPreference = 'Stop'

# The elevated cmd starts in System32, so both paths must be absolute.
$Exe = (Resolve-Path -LiteralPath $Exe).Path
$Out = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Out)
Remove-Item -LiteralPath $Out -ErrorAction SilentlyContinue

# With more than two quotes, cmd /c drops the first and the last quote of the
# line; /s plus an outer pair of quotes keeps the inner quoting intact.
$p = Start-Process cmd -Verb RunAs -Wait -PassThru `
    -ArgumentList "/s /c `"`"$Exe`" $ArgList > `"$Out`" 2>&1`""
Get-Content -LiteralPath $Out
exit $p.ExitCode
