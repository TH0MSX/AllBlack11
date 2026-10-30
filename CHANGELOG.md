# Changelog

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
