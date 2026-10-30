# Changelog

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
