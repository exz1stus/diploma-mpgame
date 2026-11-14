<#
.SYNOPSIS
    Windows counterpart of scripts/dev.sh: builds server and client, opens the
    server in its own console window and runs N clients in this one with
    prefixed output. Closing the server (or Ctrl+C here) stops everything.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\dev.ps1 -n 2 -s circle
#>
[CmdletBinding()]
param(
    [Alias('n')][ValidateRange(0, [int]::MaxValue)][int]$ClientCount = 1,
    [Alias('s')][ValidateSet('empty', 'circle')][string]$Scene = 'empty'
)

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

# Keep Windows artifacts apart from WSL/Linux builds that share this checkout.
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = 'target\windows' }

cargo build -p server -p client
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$binaryDir = Join-Path (Resolve-Path $env:CARGO_TARGET_DIR) 'debug'
$serverExe = Join-Path $binaryDir 'server.exe'
$clientExe = Join-Path $binaryDir 'client.exe'

function Stop-Tree([System.Diagnostics.Process]$Process) {
    if ($Process -and -not $Process.HasExited) {
        & taskkill.exe /T /F /PID $Process.Id 2>&1 | Out-Null
    }
}

# Server in a separate console; pause on unexpected exit so the error stays visible.
# 0xC000013A (-1073741510) is STATUS_CONTROL_C_EXIT.
$serverCommand = @"
`$host.UI.RawUI.WindowTitle = 'server ($Scene)'
`$env:TEST_SCENE = '$Scene'
& '$($serverExe -replace "'", "''")'
`$status = `$LASTEXITCODE
if (`$status -ne 0 -and `$status -ne -1073741510) {
    Write-Host "``nServer exited with status `$status. Press any key to close this window..."
    `$null = `$host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown')
}
exit `$status
"@
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($serverCommand))
$shell = (Get-Process -Id $PID).Path
$server = Start-Process -FilePath $shell -ArgumentList '-NoProfile', '-EncodedCommand', $encoded -PassThru

$clients = @()
$streams = @()
try {
    for ($i = 1; $i -le $ClientCount; $i++) {
        $info = New-Object System.Diagnostics.ProcessStartInfo $clientExe
        $info.UseShellExecute = $false
        $info.RedirectStandardOutput = $true
        $info.RedirectStandardError = $true
        $info.WorkingDirectory = (Get-Location).Path
        $process = [System.Diagnostics.Process]::Start($info)
        $clients += $process
        foreach ($reader in $process.StandardOutput, $process.StandardError) {
            $streams += [pscustomobject]@{
                Label  = "client $i"
                Reader = $reader
                Task   = $reader.ReadLineAsync()
            }
        }
    }

    # Pump client output until the server window goes away.
    while (-not $server.HasExited) {
        $idle = $true
        foreach ($stream in $streams) {
            while ($stream.Task -and $stream.Task.IsCompleted) {
                $line = $stream.Task.Result
                if ($null -eq $line) { $stream.Task = $null; break }
                Write-Host "[$($stream.Label)] $line"
                $stream.Task = $stream.Reader.ReadLineAsync()
                $idle = $false
            }
        }
        if ($idle) { Start-Sleep -Milliseconds 20 }
    }
    exit $server.ExitCode
}
finally {
    foreach ($process in $clients) { Stop-Tree $process }
    Stop-Tree $server
}
