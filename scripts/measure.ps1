<#
.SYNOPSIS
  Measures AllBlack11's own resource use. Prints numbers; invents none.

.DESCRIPTION
  Run it on the PC you care about, with AllBlack11 running. It reports, separately:
    - idle CPU (CPU time consumed over a quiet window)
    - private bytes and working set (these are different things)
    - GDI and USER handle counts
    - the same figures for the x64 helper, if it is running
    - growth in memory and handles across repeated disable/enable cycles
    - an event burst: open and close many windows, then re-measure

  Nothing here is a benchmark of "battery savings". It reports what the process used.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\scripts\measure.ps1 -IdleSeconds 60 -Cycles 20
#>
param(
    [int]$IdleSeconds = 60,
    [int]$Cycles = 20,
    [int]$BurstWindows = 30
)

$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class AB11Gui {
    [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr process, uint flags);
}
"@

function Get-Snapshot([string]$Name) {
    $p = Get-Process -Name $Name -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $p) { return $null }
    $p.Refresh()
    [pscustomobject]@{
        Name          = $Name
        Pid           = $p.Id
        CpuSeconds    = [math]::Round($p.TotalProcessorTime.TotalSeconds, 3)
        PrivateMB     = [math]::Round($p.PrivateMemorySize64 / 1MB, 2)
        WorkingSetMB  = [math]::Round($p.WorkingSet64 / 1MB, 2)
        GdiObjects    = [AB11Gui]::GetGuiResources($p.Handle, 0)
        UserObjects   = [AB11Gui]::GetGuiResources($p.Handle, 1)
        Handles       = $p.HandleCount
        Threads       = $p.Threads.Count
    }
}

$cfgPath = Join-Path $env:APPDATA "AllBlack11\config.json"
if (-not (Test-Path $cfgPath)) { throw "No config at $cfgPath. Start AllBlack11 once first." }
$main = Get-Snapshot "AllBlack11"
if (-not $main) { throw "AllBlack11 is not running." }

Write-Host "== Baseline"
$main | Format-List

Write-Host "== Idle CPU over $IdleSeconds s (leave the PC alone)"
$before = Get-Snapshot "AllBlack11"
Start-Sleep -Seconds $IdleSeconds
$after = Get-Snapshot "AllBlack11"
$cpu = $after.CpuSeconds - $before.CpuSeconds
$percent = [math]::Round(100 * $cpu / ($IdleSeconds * [Environment]::ProcessorCount), 4)
"CPU time used: $([math]::Round($cpu,3)) s over $IdleSeconds s  (= $percent % of the whole machine)"

function Set-Enabled([bool]$On) {
    $json = Get-Content -Raw $cfgPath | ConvertFrom-Json
    $json.enabled = $On
    ($json | ConvertTo-Json -Depth 20) | Set-Content -Path $cfgPath -Encoding UTF8
    # Ask the running app to re-read the file, exactly as if Settings had saved it.
    $exe = (Get-Process -Name "AllBlack11" | Select-Object -First 1).Path
    & $exe --reload
    Start-Sleep -Milliseconds 800
}
Write-Host ""
Write-Host "== $Cycles disable/enable cycles (edits config.json; restores it afterwards)"
$backup = Get-Content -Raw $cfgPath
$start = Get-Snapshot "AllBlack11"
try {
    for ($i = 0; $i -lt $Cycles; $i++) { Set-Enabled $false; Set-Enabled $true }
} finally {
    Set-Content -Path $cfgPath -Value $backup -Encoding UTF8
}
Start-Sleep -Seconds 2
$end = Get-Snapshot "AllBlack11"
"Private bytes : $($start.PrivateMB) MB -> $($end.PrivateMB) MB"
"Working set   : $($start.WorkingSetMB) MB -> $($end.WorkingSetMB) MB"
"GDI objects   : $($start.GdiObjects) -> $($end.GdiObjects)"
"USER objects  : $($start.UserObjects) -> $($end.UserObjects)"
"Handles       : $($start.Handles) -> $($end.Handles)"

Write-Host ""
Write-Host "== Event burst: $BurstWindows short-lived windows"
$burstStart = Get-Snapshot "AllBlack11"
for ($i = 0; $i -lt $BurstWindows; $i++) {
    $proc = Start-Process notepad.exe -PassThru
    Start-Sleep -Milliseconds 150
    $proc | Stop-Process -Force -ErrorAction SilentlyContinue
}
Start-Sleep -Seconds 2
$burstEnd = Get-Snapshot "AllBlack11"
"CPU time used by the burst: $([math]::Round($burstEnd.CpuSeconds - $burstStart.CpuSeconds, 3)) s"
"Private bytes: $($burstStart.PrivateMB) MB -> $($burstEnd.PrivateMB) MB   GDI: $($burstStart.GdiObjects) -> $($burstEnd.GdiObjects)"

Write-Host ""
Write-Host "== x64 helper (present only while an emulated x64 dialog is being treated)"
$helper = Get-Snapshot "AllBlack11Helper.x64"
if ($helper) { $helper | Format-List } else { "Not running, as expected when nothing needs it." }

Write-Host ""
Write-Host "Also try by hand and compare: restart Explorer from the tray (Shell > Restart Explorer) and re-run the first block."
Write-Host "Report the numbers as measured on this PC. They do not transfer to other PCs."
