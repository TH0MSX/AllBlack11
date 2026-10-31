# Hardware smoke test

**Nothing in 3.6.0-preview.1 has been run on Windows by the author.** It is unit-tested on Linux
(197 tests), cross-compiled for ARM64 and x64, and checked on Windows runners in CI for
compilation only. Every item below is therefore **unverified until you tick it**. Mark each line
pass or fail and attach the diagnostics report (Settings > Safety > Write report) to any failure.

## The ordered 3.6 pass (about 35 minutes, Snapdragon ARM64 laptop)

Do these in order. If step 1 fails, stop: nothing later is meaningful.

1. **Safe launch and basic navigation**
   - [ ] Run `AllBlack11.exe` from the portable zip. The tray icon appears; right-click shows a
         title, separators and rows each with their own text (the 3.0 bug), plus **Command
         palette…**, **Scenes**, **Pause all styling**.
   - [ ] Hold **Shift** while starting (or run `AllBlack11.exe --safe`): a balloon says it started
         in safe mode; Settings > Safety shows a **Safe mode** card with **Leave safe mode**.
   - [ ] Open Settings. Click each of the six tabs: Overview, Effects, Apps, Scenes, Legacy,
         Safety. Nothing is blank or overlapping.
2. **DPI, scrolling, keyboard focus, close and reopen**
   - [ ] At 100%, 150% and 200% scaling (and across two monitors if you have them): no clipped
         text; edit boxes sit inside their cards.
   - [ ] Scroll the Apps and Scenes pages with the wheel, Page Up/Down, Home/End. The scrollbar
         follows.
   - [ ] Tab moves a focus ring through every control, **including the Scenes list** (Left/Right
         moves the selection, Space selects). Ctrl+Tab changes page. **Ctrl+K** opens the command
         palette; Esc closes it.
   - [ ] Close Settings with the X and with Esc; reopen: it comes back on the Overview page.
3. **Caption and border preview, timeout revert, explicit Keep**
   - [ ] Overview > pick **Pure OLED**. A countdown appears. Do nothing for 15 s: it reverts, and
         `config.json` has **not** changed (check its modified time before and after).
   - [ ] Try it again and press **Keep**: `config.json` changes once, and the look is still there
         after closing and reopening Settings.
   - [ ] Try a look, then use the tray to apply a preset **during** the countdown: the trial ends
         immediately and the preset stays (it must not "revert" over it 15 s later).
   - [ ] Apps > **Pick a window**, drag onto Notepad. The **Fix this app** card names the exe,
         architecture, and gives a separate line for title bar, border, backdrop, content area and
         classic controls. Press **Preview here (20 s)**: the title bar and border change on that
         window only, then go back. **Copy style receipt**: no window title, no user name.
   - [ ] Pick Chrome or Edge: the card says the title bar may be drawn by the app and the content
         area is the app's own. It must **not** promise every control can be themed.
4. **Scene switching and per-app exceptions**
   - [ ] Add a rule for Notepad with its own title bar color (Apps > Save as rule, then set a color
         on Effects). Scenes > select **Graphite** > **Try (15 s)** > **Keep**: other apps change,
         **Notepad keeps its own color**.
   - [ ] Switch **OLED Black → Graphite → Midnight Accent → Focus → Living Room** from the tray
         **Scenes** submenu. Each applies at once; the active one is checked. **Undo last scene
         change** puts the previous config back.
   - [ ] **Focus** turns Focus Look on; switching to any other scene turns it off (the diff shows
         "Focus Look: on → off").
   - [ ] Save a look (type a name, **Save look**), rename it, compare it with a scene (**Compare…**
         then pick another), delete it.
   - [ ] **Before / after** opens two windows. The left is untouched, the right has the look.
   - [ ] Palette from a picture: **Choose picture…** with a photo. A "From …" scene appears with
         readable text; Try it. Choose a very large image (> 24 MP): a clear refusal, no hang.
   - [ ] Optional: turn on **Identity accents**, open two apps: different border colors; Apps >
         **Change accent** changes one. Turn on **Day / Night**, set the day start a minute or two
         ahead: the scene changes at that minute and a manual change stays until the next boundary.
5. **Legacy-control harness** (needs *Dark legacy windows* on; experimental)
   - [ ] Legacy > turn on. Open `AllBlack11-Harness.exe` and the `-Classic` build: every control
         family readable in every state; the owner-drawn button is left orange.
   - [ ] **Cycle x50** says PASS. Toggle the switch off: the harness returns to its light look.
   - [ ] Control Panel: **no change at all**. Run (Win+R): dark and readable.
6. **ARM64 plus emulated-x64 helper lifecycle** (experimental)
   - [ ] Settings > Safety shows the helper as "Off. Nothing is running." Turn on Legacy > **Cover
         emulated x64 apps**, open an x64 classic dialog (the x64 harness in `AllBlack11-x64.zip`).
         `AllBlack11Helper.x64.exe` appears in Task Manager **only now**; the dialog is dark;
         Safety shows "Running".
   - [ ] Turn the option off: the helper is gone within about two seconds.
   - [ ] End the helper in Task Manager while a dialog is open: AllBlack11 keeps running, nothing
         freezes, and Safety later shows it paused before the next try.
   - [ ] Exit AllBlack11: no helper remains. Kill `AllBlack11.exe` in Task Manager while the helper
         runs: the helper exits by itself within a few seconds.
7. **Recovery after an interrupted trial or a malformed configuration**
   - [ ] Start a trial, then kill `AllBlack11.exe` from Task Manager mid-countdown and start it
         again: the look is the **saved** one, not the trial.
   - [ ] Kill it twice in a row without exiting cleanly, then start it: it starts in **safe mode**
         with a balloon saying why. Exit cleanly and start again: normal.
   - [ ] Break `config.json` (add a typo) and choose Reload: AllBlack11 runs the last known good
         config and says so; your file is untouched. Safety > **Restore last known good** fixes it.
   - [ ] Safety > **Pause**: every window goes back; the tray tooltip says paused. **Resume**
         restores your look. Restart: it is not paused.
   - [ ] Safety > **Turn off experimental features**: legacy and helper stop; frame styling stays.
   - [ ] `AllBlack11.exe --restore` from a terminal quits the app and undoes shell tweaks.
8. **Installation, upgrade and uninstall**
   - [ ] With 3.5 or 3.0 installed, run `AllBlack11-Setup-arm64.exe`: it says "Update", your config
         is kept (`config.pre-3.5.json` exists if you came from 3.0).
   - [ ] Windows Settings > Apps shows 3.6.0-preview.1.
   - [ ] Uninstall: asks about your config; nothing is left running; the shell is back as it was.

Anything in this list that is not ticked is **hardware-unverified**. See `EVIDENCE.md`.

---

# Extended checks (from 3.5)

Run on the Snapdragon ARM64 laptop, then repeat sections 1–3 with the x64 build under emulation
and, if possible, on a native x64 PC.

## 0. Install and upgrade
1. With 3.0 Preview installed and your config in place, run `AllBlack11-Setup-arm64.exe`.
   It says "Update". Your config is kept. `config.pre-3.5.json` appears in `%APPDATA%\AllBlack11`.
2. Tray icon appears. Right-click: the **title, every separator and Presets/Shell/Maintenance
   rows each show their own text** (this was the 3.0 bug).
3. Settings > Overview: the Apps entry in Windows Settings shows version 3.6.0-preview.1.

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
3. **Copy style receipt**, paste it: no window title and no user name.
4. **Save as rule**, then **Preview here** on a window, then **Restore this window**.

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
   says so; your broken file is untouched. Safety > **Restore last known good** fixes it.
3. `AllBlack11.exe --restore` from a terminal quits the app and undoes shell tweaks.
4. Shell > **Restart Explorer…**: confirmation, taskbar returns, tray icon returns.
5. Uninstall from Windows Settings > Apps: asks about your config; nothing is left running.

## 7. Numbers
Run `scripts\measure.ps1` and keep the output. Do not quote it for other PCs.
