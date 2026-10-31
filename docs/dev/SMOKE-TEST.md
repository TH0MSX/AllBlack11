# Hardware smoke test (about 20 minutes)

Run on the Snapdragon ARM64 laptop, then repeat sections 1–3 with the x64 build under emulation
and, if possible, on a native x64 PC. Mark each line pass or fail; paste failures into an issue
with the diagnostics report (Settings > Diagnostics > Write report).

## 0. Install and upgrade
1. With 3.0 Preview installed and your config in place, run `AllBlack11-Setup-arm64.exe`.
   It says "Update". Your config is kept. `config.pre-3.5.json` appears in `%APPDATA%\AllBlack11`.
2. Tray icon appears. Right-click: the **title, every separator and Presets/Shell/Maintenance
   rows each show their own text** (this was the 3.0 bug).
3. Settings > Overview: the Apps entry in Windows Settings shows version 3.5.0-preview.1.

## 1. Settings and DPI
1. Open Settings at 100%, 150% and 200% scaling (and drag it between monitors if you have two).
   No clipped text, no overlap, edit boxes aligned with their cards.
2. Tab through the page: a focus ring moves; Space toggles; Ctrl+Tab changes page.
3. Overview > pick **Pure OLED**: a countdown appears; do nothing; it reverts after 15 s.
   Pick it again and press **Keep**: it stays after closing and reopening Settings.

## 2. Window picker and "why"
1. Apps > **Pick a window**, drag onto Notepad. The report names the exe, class, process
   architecture, and says whether it is styled and why.
2. Pick Chrome or Edge: the report says it may draw its own title bar.
3. **Copy report**, paste it: no window title and no user name.
4. **Create rule**, then **Try the look** on a window, then **Restore this window**.

## 3. Effects
1. Effects > try Mica, Mica Alt, Acrylic, Tinted acrylic, None on Notepad. Note which show.
2. Turn on **Focus Look**: the active window gets an accent border, others go quiet.
3. **Open demonstration**: three panels; the transparent one shows the material, the opaque
   one does not; the text is readable.
4. Turn on battery saver (Windows Settings > Power): materials drop to plain; turn it off: back.

## 4. Legacy layer (native ARM64)
1. Legacy > turn on **Dark legacy windows**. Open `AllBlack11-Harness.exe` (and the
   `-Classic` build). Every control family is readable in every state: enabled, disabled,
   read-only, selected, checked, default button, list selection. Owner-drawn button stays orange.
2. In the harness press **Cycle x50**: the result line says PASS.
3. Toggle the legacy switch off: the harness returns to its light look. Toggle on again: dark.
4. Open Control Panel: **no change at all** (toolbar and address bar normal).
5. Open Run (Win+R): dark and readable. Open a File > Open dialog in Notepad: untouched.

## 5. Emulated x64 (experimental)
1. Legacy > **Cover emulated x64 apps**. Open an x64 app's classic dialog (the x64 build of the
   harness is ideal: `AllBlack11-x64.zip`).
2. `AllBlack11Helper.x64.exe` appears in Task Manager only now; the dialog is dark.
3. Turn the option off: the helper exits within a second.
4. Exit AllBlack11: no helper remains.

## 6. Recovery
1. Settings > Shell: turn on black shell, change your accent color in Windows Settings, then
   turn black shell off: your new accent color is **left alone** (see allblack11.log).
2. Break `config.json` (add a typo), choose Reload: AllBlack11 runs the last known good one and
   says so; your broken file is untouched. Profiles > **Restore last known good** fixes it.
3. `AllBlack11.exe --restore` from a terminal quits the app and undoes shell tweaks.
4. Shell > **Restart Explorer…**: confirmation, taskbar returns, tray icon returns.
5. Uninstall from Windows Settings > Apps: asks about your config; nothing is left running.

## 7. Numbers
Run `scripts\measure.ps1` and keep the output. Do not quote it for other PCs.
