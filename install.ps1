# Install AllBlack11 for the current user. No administrator rights.
# From the extracted zip:
#   powershell -ExecutionPolicy Bypass -File .\install.ps1
# Skip the HKCU Run entry with -NoAutostart.
# Also retires a previous PepaTheme install (Run key and Programs folder).
# Prefer AllBlack11-Setup-<arch>.exe for a regular install with an Apps entry.

param(
    [switch]$NoAutostart
)

$ErrorActionPreference = "Stop"

function Get-PeMachine {
    param([string]$Path)
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 64 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
        throw "AllBlack11.exe is not a Windows executable."
    }
    $pe = [BitConverter]::ToInt32($bytes, 0x3C)
    if ($pe -lt 0 -or ($pe + 6) -gt $bytes.Length) {
        throw "AllBlack11.exe has an unexpected PE header."
    }
    return [BitConverter]::ToUInt16($bytes, $pe + 4)
}

# RuntimeInformation.OSArchitecture follows the PowerShell process, so x64
# PowerShell on an ARM64 PC reports X64. Ask the kernel for the native machine.
function Get-NativeOsArchitecture {
    $code = @"
using System;
using System.Runtime.InteropServices;
public static class PepaNativeArch {
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool IsWow64Process2(IntPtr process, out ushort processMachine, out ushort nativeMachine);
    [DllImport("kernel32.dll")]
    static extern IntPtr GetCurrentProcess();
    public static int NativeMachine() {
        ushort processMachine;
        ushort nativeMachine;
        if (!IsWow64Process2(GetCurrentProcess(), out processMachine, out nativeMachine)) return 0;
        return nativeMachine;
    }
}
"@
    $native = 0
    try {
        if (-not ("PepaNativeArch" -as [type])) {
            Add-Type -TypeDefinition $code -Language CSharp -ErrorAction Stop
        }
        $native = [PepaNativeArch]::NativeMachine()
    } catch {
        $native = 0
    }
    switch ($native) {
        0xAA64 { return "Arm64" }
        0x8664 { return "X64" }
        0x014c { return "X86" }
        0x01c4 { return "Arm" }
    }

    $machineEnv = [Environment]::GetEnvironmentVariable(
        "PROCESSOR_ARCHITECTURE",
        [EnvironmentVariableTarget]::Machine)
    switch ($machineEnv) {
        "ARM64" { return "Arm64" }
        "AMD64" { return "X64" }
        "x86" { return "X86" }
    }

    if ($env:PROCESSOR_ARCHITEW6432 -eq "ARM64") { return "Arm64" }
    if ($env:PROCESSOR_ARCHITEW6432 -eq "AMD64") { return "X64" }

    try {
        $cpu = Get-CimInstance -ClassName Win32_Processor | Select-Object -First 1
        switch ([int]$cpu.Architecture) {
            12 { return "Arm64" }
            9 { return "X64" }
            5 { return "Arm" }
            0 { return "X86" }
        }
    } catch {
    }

    return [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
}

function Stop-AllBlack11 {
    param([string]$Exe)
    if (Test-Path $Exe) {
        & $Exe --exit
    }
    $deadline = (Get-Date).AddSeconds(8)
    while ((Get-Process -Name "AllBlack11" -ErrorAction SilentlyContinue) -and ((Get-Date) -lt $deadline)) {
        Start-Sleep -Milliseconds 200
    }
    if (Get-Process -Name "AllBlack11" -ErrorAction SilentlyContinue) {
        Stop-Process -Name "AllBlack11" -Force
        Start-Sleep -Milliseconds 400
    }
}

function Remove-LegacyPepaTheme {
    $oldDir = Join-Path $env:LOCALAPPDATA "Programs\PepaTheme"
    $oldExe = Join-Path $oldDir "PepaTheme.exe"
    if (Test-Path $oldExe) {
        & $oldExe --exit
        $deadline = (Get-Date).AddSeconds(8)
        while ((Get-Process -Name "PepaTheme" -ErrorAction SilentlyContinue) -and ((Get-Date) -lt $deadline)) {
            Start-Sleep -Milliseconds 200
        }
    }
    Get-Process -Name "PepaTheme" -ErrorAction SilentlyContinue | Stop-Process -Force
    Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "PepaTheme" -ErrorAction SilentlyContinue
    if (Test-Path $oldDir) {
        Remove-Item -Recurse -Force $oldDir
        Write-Host "Removed the old PepaTheme install."
    }
}

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$source = Join-Path $here "AllBlack11.exe"
if (-not (Test-Path $source)) {
    Write-Error "AllBlack11.exe was not found next to install.ps1."
}

$machine = Get-PeMachine $source
$os = Get-NativeOsArchitecture
if ($machine -eq 0xAA64 -and $os -ne "Arm64") {
    Write-Error "This AllBlack11.exe is ARM64, but this PC is $os. Use the x64 zip."
}
if ($machine -eq 0x8664 -and $os -eq "Arm64") {
    Write-Warning "This is the x64 build on an ARM64 PC. It can run under emulation. Prefer the ARM64 zip."
}
if ($machine -ne 0xAA64 -and $machine -ne 0x8664) {
    Write-Error ("Unrecognized executable architecture: 0x{0:X4}" -f $machine)
}

$hookSource = Join-Path $here "AllBlack11Hook.dll"
if (Test-Path $hookSource) {
    $hookMachine = Get-PeMachine $hookSource
    if ($hookMachine -ne $machine) {
        Write-Error ("AllBlack11Hook.dll is 0x{0:X4} but AllBlack11.exe is 0x{1:X4}. The zip is mixed up." -f $hookMachine, $machine)
    }
}

Remove-LegacyPepaTheme

$dest = Join-Path $env:LOCALAPPDATA "Programs\AllBlack11"
$destExe = Join-Path $dest "AllBlack11.exe"
if (Test-Path $destExe) {
    Stop-AllBlack11 $destExe
}

New-Item -ItemType Directory -Force -Path $dest | Out-Null
Copy-Item $source $destExe -Force
$hook = Join-Path $here "AllBlack11Hook.dll"
if (Test-Path $hook) {
    Copy-Item $hook (Join-Path $dest "AllBlack11Hook.dll") -Force
}
foreach ($name in @("README.md", "uninstall.ps1", "config.default.json", "install.ps1")) {
    $from = Join-Path $here $name
    if (Test-Path $from) {
        Copy-Item $from (Join-Path $dest $name) -Force
    }
}

$cfgDir = Join-Path $env:APPDATA "AllBlack11"
New-Item -ItemType Directory -Force -Path $cfgDir | Out-Null
$cfg = Join-Path $cfgDir "config.json"
$defaultCfg = Join-Path $here "config.default.json"
$legacyDir = Join-Path $env:APPDATA "PepaTheme"
$legacyCfg = Join-Path $legacyDir "config.json"
if (-not (Test-Path $cfg) -and (Test-Path $legacyCfg)) {
    Copy-Item $legacyCfg $cfg
    $legacySnapshots = Join-Path $legacyDir "snapshots.json"
    $newSnapshots = Join-Path $cfgDir "snapshots.json"
    if ((Test-Path $legacySnapshots) -and -not (Test-Path $newSnapshots)) {
        Copy-Item $legacySnapshots $newSnapshots
    }
    Write-Host "Copied config and snapshots from PepaTheme."
} elseif (-not (Test-Path $cfg)) {
    if (-not (Test-Path $defaultCfg)) {
        Write-Error "config.default.json was not found next to install.ps1."
    }
    Copy-Item $defaultCfg $cfg
    Write-Host "Created $cfg"
} else {
    Write-Host "Kept existing $cfg"
}

$run = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
if ($NoAutostart) {
    Remove-ItemProperty -Path $run -Name "AllBlack11" -ErrorAction SilentlyContinue
    Write-Host "Autostart not enabled."
} else {
    Set-ItemProperty -Path $run -Name "AllBlack11" -Value "`"$destExe`""
    Write-Host "Autostart enabled (HKCU Run\AllBlack11)."
}

Start-Process -FilePath $destExe
Write-Host "AllBlack11 is running. Right-click its tray icon for the menu."
