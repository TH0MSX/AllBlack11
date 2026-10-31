# Changelog

## 3.0.0-preview.1

A preview. It is built on 1.2.0 and rebuilds the Settings window, the tray menu, and the installer. It is marked "pre-release" on GitHub, so the "latest" release link still points at the stable build.

**New**

- **Setup.exe.** `AllBlack11-Setup-arm64.exe` and `AllBlack11-Setup-x64.exe` are native installers: no PowerShell, no administrator rights. They install to `%LOCALAPPDATA%\Programs\AllBlack11`, add a Start menu shortcut and an entry in Settings > Apps, offer to start at sign-in, and update in place. Uninstall puts the Windows settings back first and asks whether to keep your config. `/S` runs them silently. The zips and `install.ps1` are still published.
- **Restart Explorer** from the tray (Shell > Restart Explorer…) and from Settings, with a confirmation that says File Explorer windows will close. It ends the shell process and lets Windows bring it back, then starts `explorer.exe` itself if that has not happened after a few seconds. `AllBlack11.exe --restart-explorer` does the same from a terminal.
- **Presets**: Pure OLED, Accent glass, Classic Mica, and Factory defaults, from the tray (Presets) and the top of Settings. A preset rewrites the default look (and the shell toggles, for Pure OLED and Factory defaults). Rules and exclusions are never touched.
- **Settings redesign**, in the style of PowerToys but black: one card per option with a plain-language explanation on the left and the control on the right, an On/Off label on every switch, section headings, a resizable window, and a live preview pinned at the top. The preview redraws a mock window, Start menu and taskbar from your config, so you see what a backdrop, corner style, border, tint, or shell toggle does before you look at a real window.
- **Tray menu**: items carry a one-line explanation, and the less common commands moved into Presets, Shell, and Maintenance submenus. The title shows the CPU type (ARM64, x64, or x64 emulated).
- **ARM64 status**: Settings > About and `--check` report the build, the PC, and whether you are running the x64 build under emulation, with a pointer to the ARM64 download.
- **Recovery**: `AllBlack11.exe --restore` quits the app, undoes the shell tweaks and CaptionHeight from their backups, and turns the shell flags off in `config.json`, without needing the tray. `--quiet` suppresses message boxes for scripts.
- **Data folder and diagnostics** buttons in Settings and Maintenance. The diagnostics report is `check.txt`.

**Fixed**

- Settings mixed scaled and unscaled pixels. On a 150% or 200% display (most ARM laptops) the painted rows, the edit boxes, and the slider hit areas no longer lined up. Every coordinate is now logical and scaled once, and the window handles per-monitor DPI changes.
- Shell toggles changed in Settings were saved and applied, but Windows was never told (no `ImmersiveColorSet` broadcast), so the taskbar and Start often looked unchanged until Explorer restarted. Settings and presets now broadcast like the tray does.
- The exclusion list in Settings only showed, and could only select, the first four entries. The default config has 23. Lists now show every entry.
- Color fields only applied after you clicked away. They now apply as soon as the value is valid (and still complain on blur if it is not).
- Settings painted straight to the window, which can flicker while scrolling. It now paints through an off-screen buffer.
- `config.json`, `snapshots.json`, `shell-backup.json` and `metrics-backup.json` were written in place, so a crash or full disk could truncate them. They are now written to a temporary file and renamed.
- The tray menu took its DPI from the foreground window, not the monitor the menu opened on, so it could be the wrong size on a mixed-DPI setup.
- `build.sh` did not build the hook DLL, so zips from it had no hook. It now builds the app, hook, and Setup for each CPU type and checks that every binary's PE machine type matches.
- `install.ps1` refuses a zip whose hook DLL and exe are for different CPUs.
- The embedded manifest still said 1.1.1.
- Found on real hardware before release: descriptions in Settings were cut off, Setup's status line overlapped its explanation text, color fields drew their text at the top of the box, and the version showed as "3 Preview". All fixed.

**Unchanged and worth knowing on ARM64**

- The experimental dark legacy hook only enters processes of its own CPU type. On ARM64 it covers native ARM64 apps; apps running under x64 emulation are skipped. Settings now says so next to the switch.
- The binaries are unsigned. SmartScreen asks for More info, then Run anyway.
- The GUI has been run on a Snapdragon ZenBook (Setup and Settings), but not every path: Restart Explorer, uninstall, and the x64 builds have not been exercised on hardware. Please report anything that looks off.

## 1.2.0

- The tray menu follows the Windows app theme, light or dark, with taller rows, horizontal padding, and rounded corners. It uses `SetPreferredAppMode(AllowDark)`, `FlushMenuThemes`, and `AllowDarkModeForWindow` on the tray window. High contrast keeps the system menu.
- Settings is a native Win32 window (no WebView). Open it from **Settings…** or by double-clicking the tray icon. It edits backdrop, colors, acrylic opacity, corners, legacy caption height, shell toggles, per-app rules, exclusions, startup, and the experimental switch. Changes write `config.json` and apply as you go. Sliders apply when you release the thumb.
- `captionHeight` writes HKCU `CaptionHeight` (negative twips). Windows applies it at the next sign-in. The previous value is stored in `metrics-backup.json` and restored when the slider is set back to System, or by `uninstall.ps1`. AllBlack11 does not call `SetSysColors`.
- Classic `#32770` dialogs, including Run, now get a dark caption from the existing DWM path. AllBlack11 also calls `SetWindowTheme` (`DarkMode_CFD` / `DarkMode_Explorer`) on the dialog and its children. That call is cross-process and often does not recolor the client area.
- **Experimental: dark legacy windows** is off by default. When on, `AllBlack11Hook.dll` is loaded only into same-architecture processes that are not elevated, protected, or on the exclude list. The DLL subclasses `#32770` dialogs, paints `WM_CTLCOLOR*` with a dark background and light text (links and large static fonts included), and asks list views and Explorer-hosted children to use `DarkMode_Explorer`. Control Panel is included for `mmc.exe` and for Explorer windows titled "Control Panel". Turning the switch off, or exiting, removes the hooks. Games, launchers, and elevated processes are never entered. Antivirus and anti-cheat products may flag a hook DLL. DirectUI, XAML, and HTML pages can stay their own color. An ARM64 build does not hook x64-emulated processes, and an x64 build does not hook ARM64 processes.

## 1.1.1

- The Shell menu shows "Black Start, taskbar & notification center". The ampersand is no longer swallowed as a menu shortcut.
- If OLED taskbar or the black shell surfaces are already in the target state when you turn a toggle on, `shell-backup.json` records `alreadyApplied` instead of treating those values as the original. Uncheck then deletes `AccentPalette` (Windows rebuilds it), sets Personalize `ColorPrevalence` back to 0, and removes `UseOLEDTaskbarTransparency`. A backup file that already has real originals is not overwritten.

## 1.1.0

- Renamed the project to AllBlack11. The first launch copies `%APPDATA%\PepaTheme\config.json` and `snapshots.json` when the new config does not exist yet. `install.ps1` stops the old PepaTheme process, removes its Run key, and deletes `%LOCALAPPDATA%\Programs\PepaTheme`.
- Application and tray icon from `allblack11.ico`.
- Opt-in `tintedAcrylic` backdrop: `SetWindowCompositionAttribute` accent policy with a configurable tint and opacity. Falls back to DWM acrylic if that call fails. Drag and resize can temporarily use blur-behind.
- Shell menu: OLED taskbar, and black Start / taskbar / notification center. Original HKCU values are saved in `shell-backup.json` and restored on uncheck or uninstall.
- GitHub Actions release workflow builds the ARM64 and x64 zips when a `v*` tag is pushed.

## 1.0.0

- First release, then named PepaTheme: dark captions, accent border, Mica, per-process rules, and out-of-context window events. No injection and no administrator rights.
