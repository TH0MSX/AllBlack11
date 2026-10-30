# Remove AllBlack11 for the current user. No administrator rights.
#   powershell -ExecutionPolicy Bypass -File .\uninstall.ps1
# Restores themed windows, then puts back any shell registry values
# snapshotted in %APPDATA%\AllBlack11\shell-backup.json.

$ErrorActionPreference = "Continue"

function Restore-ShellBackup {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        return
    }
    $backup = Get-Content -Raw -Path $Path | ConvertFrom-Json
    $advanced = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced"
    $accent = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Accent"
    $personalize = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
    $dwm = "HKCU:\Software\Microsoft\Windows\DWM"
    if (Test-AlreadyApplied $backup.oledTaskbar) {
        Remove-ItemProperty -Path $advanced -Name "UseOLEDTaskbarTransparency" -ErrorAction SilentlyContinue
    } else {
        Restore-Dword $advanced "UseOLEDTaskbarTransparency" $backup.oledTaskbar
    }
    if (Test-AlreadyApplied $backup.accentPalette) {
        Remove-ItemProperty -Path $accent -Name "AccentPalette" -ErrorAction SilentlyContinue
        if (Test-Path $personalize) {
            New-ItemProperty -Path $personalize -Name "ColorPrevalence" -PropertyType DWord -Value 0 -Force | Out-Null
        }
    } else {
        Restore-Binary $accent "AccentPalette" $backup.accentPalette
        Restore-Dword $personalize "ColorPrevalence" $backup.colorPrevalence
        Restore-Dword $personalize "EnableTransparency" $backup.enableTransparency
        Restore-Dword $personalize "SystemUsesLightTheme" $backup.systemUsesLightTheme
        Restore-Dword $dwm "ColorPrevalence" $backup.dwmColorPrevalence
    }
    Broadcast-ImmersiveColor
    Write-Host "Restored shell settings from shell-backup.json."
}

function Test-AlreadyApplied {
    param($Slot)
    return ($null -ne $Slot -and $Slot.alreadyApplied -eq $true)
}

function Restore-Dword {
    param($Key, $Name, $Slot)
    if ($null -eq $Slot) { return }
    if (-not (Test-Path $Key)) { return }
    if (Test-AlreadyApplied $Slot) { return }
    if ($Slot.present) {
        New-ItemProperty -Path $Key -Name $Name -PropertyType DWord -Value ([int]$Slot.value) -Force | Out-Null
    } else {
        Remove-ItemProperty -Path $Key -Name $Name -ErrorAction SilentlyContinue
    }
}

function Restore-Binary {
    param($Key, $Name, $Slot)
    if ($null -eq $Slot) { return }
    if (-not (Test-Path $Key)) { return }
    if ($Slot.present) {
        $bytes = New-Object byte[] ($Slot.bytes.Count)
        for ($i = 0; $i -lt $Slot.bytes.Count; $i++) { $bytes[$i] = [byte]$Slot.bytes[$i] }
        New-ItemProperty -Path $Key -Name $Name -PropertyType Binary -Value $bytes -Force | Out-Null
    } else {
        Remove-ItemProperty -Path $Key -Name $Name -ErrorAction SilentlyContinue
    }
}

function Restore-CaptionHeight {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        return
    }
    $backup = Get-Content -Raw -Path $Path | ConvertFrom-Json
    $key = "HKCU:\Control Panel\Desktop\WindowMetrics"
    if (-not (Test-Path $key)) {
        return
    }
    if ($backup.present) {
        Set-ItemProperty -Path $key -Name "CaptionHeight" -Value ([string]$backup.captionHeight)
        Write-Host "Restored CaptionHeight. Sign out for the title bar height to return."
    } else {
        Remove-ItemProperty -Path $key -Name "CaptionHeight" -ErrorAction SilentlyContinue
        Write-Host "Removed CaptionHeight. Sign out for the title bar height to return."
    }
}

function Broadcast-ImmersiveColor {
    if (-not ("AllBlack11Broadcast" -as [type])) {
        Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class AllBlack11Broadcast {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);
    public static void Immersive() {
        UIntPtr result;
        SendMessageTimeout((IntPtr)0xffff, 0x001A, UIntPtr.Zero, "ImmersiveColorSet", 2, 250, out result);
    }
}
"@
    }
    try { [AllBlack11Broadcast]::Immersive() } catch { }
}

$dest = Join-Path $env:LOCALAPPDATA "Programs\AllBlack11"
$destExe = Join-Path $dest "AllBlack11.exe"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$localExe = Join-Path $scriptDir "AllBlack11.exe"

$exe = $null
if (Test-Path $destExe) {
    $exe = $destExe
} elseif (Test-Path $localExe) {
    $exe = $localExe
}

if ($exe) {
    & $exe --exit
}

$deadline = (Get-Date).AddSeconds(8)
while ((Get-Process -Name "AllBlack11" -ErrorAction SilentlyContinue) -and ((Get-Date) -lt $deadline)) {
    Start-Sleep -Milliseconds 200
}
Get-Process -Name "AllBlack11" -ErrorAction SilentlyContinue | Stop-Process -Force

$cfg = Join-Path $env:APPDATA "AllBlack11"
Restore-ShellBackup (Join-Path $cfg "shell-backup.json")
Restore-CaptionHeight (Join-Path $cfg "metrics-backup.json")

Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "AllBlack11" -ErrorAction SilentlyContinue

if (Test-Path $dest) {
    Remove-Item -Recurse -Force $dest
}
if (Test-Path $cfg) {
    Remove-Item -Recurse -Force $cfg
}

Write-Host "AllBlack11 removed. Frames are restored when the app exits cleanly."
Write-Host "Shell colors were restored from shell-backup.json when that file existed."
Write-Host "If a window was still themed because the process was force-killed, restart that app."
