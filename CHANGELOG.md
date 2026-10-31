# Changelog

## 3.5.0-preview.1

A **pre-release**. 3.0 Preview stays the latest release until the smoke test in `docs/dev/SMOKE-TEST.md` has been run on real ARM64 and x64 hardware. **Nothing new in this version has been run on Windows by the author yet**: it is built and unit-tested (110 tests) and cross-compiled for ARM64 and x64, and `docs/dev/EVIDENCE.md` lists, feature by feature, what that does and does not prove.

Layers, so claims stay honest:
- **A. Frame styling** (DWM attributes, no code in other processes): colors, corners, Mica, Mica Alt, Acrylic, tinted Acrylic (undocumented, experimental).
- **B. Legacy layer** (opt-in, per window): recolors *classic dialogs* from inside the process. Experimental.
- **C. App integrations**: none ship in this version. Recipes can describe a per-app look, but nothing edits another app's settings.

**Legacy dark controls (layer B)**
- One shared policy (`legacy_policy`) now decides eligibility in the exe, the hook and the helper. Only classic dialogs are ever candidates. Explorer, Control Panel (never identified by its title), MMC main frames and any dialog that embeds an Explorer or web view are skipped whole rather than half-darkened.
- Control families get a technique each: static text, edit boxes, combo and list boxes, push buttons, check boxes, radio buttons and group boxes, list views (with header), tree views. Tab strips, rich edit, date pickers, owner-drawn controls and tooltips are left as they are and reported as partial or unsupported. Settings > Legacy shows the table.
- Disabled and read-only states stay readable; both palettes (graphite, true black) are tested for contrast.
- Hook v2: observes after a control has handled its own creation (the old hook could poke a list view before it existed), records original state per window, restores only on the owning thread, never unlinks from under another subclass, and pins itself so turning styling off can never unload code a window procedure still points at. "Off" means styling stopped, not module unloaded.
- `AllBlack11-Harness.exe` (and a `-Classic` build with no visual-style manifest) puts every control family and state in one dialog, with a self-hook mode and a 50× enable/disable leak check on GDI and USER objects.
- ARM64EC processes are detected from the executable's PE header and refused: a plain ARM64 DLL cannot load into them.

**Emulated x64 on ARM64 (experimental, off by default)**
- An x64 helper (`AllBlack11Helper.x64.exe`, with `AllBlack11Hook.x64.dll`) ships inside the ARM64 package and starts only when an eligible emulated x64 dialog appears. It talks over a first-instance, owner-only, local-only pipe, accepts four commands, re-validates every target itself, and exits when idle, when the option is turned off, or when AllBlack11 exits. Binaries are packaged, never downloaded.
- Never run on hardware yet. See the smoke test.

**Effects Studio, picker and "why didn't this work?"**
- Settings is now six pages: Overview, Effects, Apps, Legacy, Profiles, Diagnostics.
- Window picker (crosshair drag or a searchable list) and a report that names the executable, class, process architecture, which rule won and why, where every setting came from, the exact HRESULT of each DWM call, and what is *not* known. "Accepted" is never reported as "confirmed".
- Frame-framework hints (Chromium, WinUI, UWP, WPF, Qt, Java, Office, Terminal, classic dialog) and a note when an app likely draws its own title bar or an opaque client that hides a backdrop.
- **Reduce effects** drops backdrop materials on battery saver, in a remote session, or when Windows transparency is off, each switchable, with the reason shown.
- **Focus Look**: accent border on the active window, quieter inactive windows, only on focus changes. A color a rule sets itself always wins, and dimmed text is refused if it would be hard to read.
- **Prefer app defaults** per target. Material tint opacity is labeled as that, not as window opacity (AllBlack11 does not change whole-window opacity).
- A **materials demonstration** window applies your look to itself with transparent, opaque and half-transparent panels, labeled as a demonstration.

**Rules**
- Rules can match by executable, window class, path (glob), package family (glob) and, optionally, window title. Titles are optional and only read when a rule asks, because they change with the document and the display language.
- Deterministic precedence (priority, then specificity, then file order), conflict detection, and a stated reason for every outcome.

**Safety and recovery**
- Presets and recipes run as a **15-second trial** with Keep and Revert; nothing is saved until Keep, and a crash cannot persist a trial.
- **Last known good**: `config.lkg.json` is kept; a broken `config.json` falls back to it and is left untouched. One-time `config.pre-3.5.json` backup, because a 3.0 build cannot read what 3.5 saves.
- **Compare-before-restore**: unchecking a shell option restores the saved original only if the registry still holds the value AllBlack11 wrote; anything you or Windows changed since is left alone and logged.
- **Recipes**: small local JSON files with a closed schema (no field can run anything, change the shell, or reach outside the look), strict validation, a diff before use, export and import. Four are built in: Black frames, Graphite, Teal glass, Low distraction.
- **Compatibility notebook**: your own notes per app, Windows build and process architecture. Local only; nothing is sent anywhere.
- Sanitized reports leave out window titles, your user name and your profile path unless you opt in.

**Fixed**

- The tray menu item identity bug (title and separators) now has a regression test on the item model.
- `build.sh` verifies the machine type of every binary, including that the x64 helper in the ARM64 package is x64 on purpose.

**Added for measuring**
- `scripts/measure.ps1` (idle CPU, private bytes vs working set, GDI/USER handles, enable/disable growth, event burst, helper) and `AllBlack11.exe --reload`. It prints numbers and invents none.

**Known limits**
- Custom-drawn Settings controls are not exposed to screen readers yet. Keyboard access works.
- Windows Terminal, Chromium/Electron and WinUI apps keep their own opaque client areas; backdrops will not show through them and some ignore caption colors. The inspector says so.

## 3.0.0-preview.2

The current release. It is built on 1.2.0 and rebuilds the Settings window, the tray menu, and the installer. "Preview" describes the feature set, which is still being refined; it is the version to use today and is published as the latest release.

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

- Control Panel (Explorer) showed black blocks where the address bar and navigation buttons should be, and unreadable toolbar glyphs. AllBlack11 was forcing a dark theme onto every child control of the window from outside the process, even with the experimental switch off. That cross-process theming now runs only when **Dark legacy windows** is on, and only for classic dialogs. Explorer-hosted Control Panel is no longer touched by the experimental feature at all, because its toolbar and address band cannot be darkened without breaking over a light page.

- Tray menu: separators and submenu rows (Presets, Shell, Maintenance) drew the title text instead of their own. The owner-drawn item data was patched in by command id, and the title and every separator share id 0. It is now passed when each item is created.

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
