# Hardware smoke test

**The ARM64 build of 3.7.0-preview.1 has been started and looked at on a Windows 11 ARM64 PC by the author (installer, tray, Settings pages; see EVIDENCE.md). None of the numbered steps below has been ticked, and x64 has not been run.** It is unit-tested on Linux
(269 tests), cross-compiled for ARM64 and x64, rendered under Wine for layout (Wine is not
Windows), and checked on a Windows ARM64 runner in CI for compilation only. Every item below is
therefore **unverified until you tick it**, whatever the screenshots showed. Mark each line pass or fail and attach the diagnostics
report (Settings > Diagnostics > Write report) to any failure.

Do **section 3.8 first** (about 40 minutes: updater, packs, exclusion manager, text-box names, wallpapers; none of it has run on Windows), then **section 3.7** (about 30 minutes: icon, credit, recovery, new pages), then **section U** (about 25 minutes): it covers the interface redesign and is where a
screen, a keyboard or a screen reader will disagree with what was rendered under Wine. Then the
3.6 pass below. In preview.2 the six old tabs became eight pages: Overview is **Home**; Effects is
**Appearance**; Apps is **Fix this app** (pick and inspect) and **Rules** (rules, exclusions,
notebook, classic dialogs); Scenes is **Scenes**; Legacy moved into **Rules**; Safety is
**Safety**; the report, support table and tools are on **Diagnostics**; About and the
command palette are on **Settings & help**. Steps below that say "tab" mean "page".

## 3.8. Updater, packs, exclusions, labels, wallpapers (3.8.0-preview.1)

Nothing in this section has been run on Windows. Expected outcomes are in italics. Mark each line
pass or fail and note exactly what you see or hear. Do the steps in order: the updater steps need
a published release newer than the running build (publish 3.8.0-preview.1 first, then test with
the 3.7.0-preview.1 build installed, or the reverse for "up to date").

**A. Updater (Settings & help > Updates)**
1. [ ] Open the page with no network (airplane mode), press **Check for updates**. *A plain "could not reach GitHub" line within about 15 seconds; no dialog; Check again works; styling is unaffected.*
2. [ ] With network, on 3.7.0-preview.1, channel **Automatic**, press Check. *"A newer version is available: 3.8.0-preview.1 (preview)", notes excerpt, release page link. The Home page shows a badge row. A tray balloon appears once.*
3. [ ] Set the channel to **Stable only** and check again. *"You have the latest stable version" (a preview is not offered).* Back to Automatic.
4. [ ] **Skip this version**. *The badge goes; the Updates card says it is skipped; a later check does not re-announce it.* **Show it again** restores it.
5. [ ] Press **Download**. *A progress line with bytes and percent; the page stays responsive; Cancel works and leaves nothing in `%LOCALAPPDATA%\AllBlack11\updates`.* Start again and let it finish. *"Verified against the published checksum" and the SHA-256 is shown. The file name matches this PC's CPU (arm64 on the Snapdragon laptop, x64 on an x64 PC).*
6. [ ] Close and reopen AllBlack11. *The downloaded, verified installer is still offered; Discard deletes it.*
7. [ ] Corrupt the downloaded file (append a byte) and press **Install**. *It refuses: "does not match", file deleted; AllBlack11 keeps running.*
8. [ ] Download again and press **Install**. *Setup opens, AllBlack11 closes, the upgrade completes, settings are kept, the tray icon returns, About says 3.8.0-preview.1.* Record how long the swap took.
9. [ ] Roll back: install the 3.7.0-preview.1 Setup over 3.8. *Settings are kept; the optional `updates` and `ui` sections are ignored by 3.7 without errors (check `config.json` still loads).*
10. [ ] Turn on the daily check, restart. *Nothing is contacted until about a day passes (or press Check); with it off nothing runs.* Confirm in a firewall log or Resource Monitor that only github.com is contacted.
11. [ ] Narrator on the Updates card. *Each button and the channel choice are announced with their names.*

**B. Compatibility packs (Rules > Compatibility packs)**
12. [ ] Pick **Microsoft Store**, read the details. *Verdict "Partly supported", the limit "Client content of the Store app is rendered by the app itself and cannot be recolored externally.", "Not yet tested on Windows hardware".*
13. [ ] Open the Store, then **Install pack**. *Report line "Added rule Pack: Microsoft Store ... priority -10". Look history shows "Installed the Microsoft Store pack".* Look at the window. *Record exactly what changed: title bar color, border, text color. If nothing changed, note the build and open Fix this app on the Store window: the package should read Microsoft.WindowsStore_..., not ApplicationFrameHost.* This decides whether the pack must become "Not supported".
14. [ ] **Remove pack**. *The rule disappears; the window returns to how Windows draws it after a refresh; Undo (Safety) brings the pack back.*
15. [ ] Repeat 12 to 14 for **Calculator**. *Same limits; its keys and display stay as the app draws them.*
16. [ ] Add your own rule for the same package with a different title bar color, then install the pack. *The report says "Your rule N also matches ... and wins"; the window uses your color.*
17. [ ] **Import pack...** with a valid file (copy a built-in from the docs), then with: an unknown field, an oversized file (over 24 KB), a `look` key outside the allowed set, an unsupported pack with a look. *Each bad file is refused with a plain reason; nothing is saved.*

**C. Exclusion manager and text-box names**
18. [ ] Rules > Exclusion manager. *Counts of protected classes and programs, your lists, rules that switch styling off, and the five numbered layers.*
19. [ ] Type `class=Notepad` in the "Never theme" box and press Add. *"Windows of class Notepad are now left alone"; Notepad (if styled) returns to normal after a refresh; the class shows in the list; Remove class takes it away.* Try `class=Shell_TrayWnd` (*already protected*) and `class=a*b` (*refused*).
20. [ ] Narrator, Tab to each text box on Fix this app, Appearance, Rules, Scenes and Help. *Each says its name ("Filter windows", "Title bar color", "New rule or test", "App to leave alone", "Look name", "Search help") and role. Record any box that is announced as just "edit".* Check with NVDA and Accessibility Insights if available. If names are missing, the property service did not apply: report it, do not assume it works.

**C2. Recipe Remixer (Scenes > Recipes)**
- [ ] Pick Graphite, then under **Remix two recipes** set Take from = Black frames, Take = Frame colors. *"Result: Graphite + Black frames (frame colors)" and a list of what would change.* Press **Try remix (15 s)**. *Windows change for 15 seconds, then Keep or Revert; look history shows it only after Keep.* Press **Save remix**. *A new recipe appears in the list and survives a restart.*
- [ ] Try a mix that makes the title text hard to read (take a light title bar into a recipe with light text, if you have one imported). *A plain refusal naming the contrast ratio; nothing is applied.*

**D. Wallpapers**
21. [ ] Open **Wallpapers**. *Eight tiles with names and "Download N KB/MB"; arrow keys and Tab move between tiles; Narrator reads "Eclipse, Download 633 KB".*
22. [ ] Select one, press **Preview full screen**. *A black window covers this display and shows the picture; the hint says "Preview from the small picture" until downloaded. Esc closes; clicking closes; switching to another window closes it.*
23. [ ] Press **Download image** with the 3.8.0-preview.1 release published and the PNGs attached. *Progress, then "checked against the published checksum"; the file is in `%LOCALAPPDATA%\AllBlack11\wallpapers`.* Before the files are attached: *a clear "not attached to release ... yet" message.*
24. [ ] Open the preview again (now full resolution), change **Fit** between Fill, Fit and Center. *Fill covers (crops on an ultrawide), Fit shows everything with black bars, Center shows actual size.* Press Enter in the preview. *The wallpaper is applied.*
25. [ ] **Apply to** a single display on a multi-monitor PC. *Only that display changes; the other keeps its picture.* Then All displays.
26. [ ] **Put back previous**. *The earlier picture returns on each display it was recorded for.* If the earlier background was a solid color or a slideshow, *the button is absent or says it cannot be put back.*
27. [ ] Check black bars: with Fit on a 16:10 or ultrawide display, *the bars are black, not the old desktop color.*
28. [ ] **Open file location**. *Explorer opens with the file selected, or the folder if not downloaded.*
29. [ ] Look at an OLED panel (or a very dark room): *true black stays off; strokes are thin; no large bright area.* Note any banding in the soft glows (Moonrise, Eclipse, Horizon).

**E. Both CPUs**
30. [ ] Run 1 to 29 on the ARM64 laptop, and on an x64 PC if you have one. *Same behavior; the updater offers the installer for that CPU.*

## 3.7. The brand, recovery and new pages (3.7.0-preview.1)

Expected outcomes are in italics. Mark each line pass or fail; note exactly what you see or hear.

1. **Icon everywhere**
   - [ ] Install with the new Setup (or run the portable exe). *Setup's header shows the moon-and-window mark and a blue line; its title-bar and taskbar icon are the same mark.*
   - [ ] Start menu shortcut, Apps and features entry, `AllBlack11.exe` in Explorer at small, medium and large icon views, Alt+Tab, the taskbar button of Settings. *The silver moon with a window-shaped bite and a blue frame, crisp at 16, 24 and 32 px; not the old four-pane tile. Note any size that looks blurry or wrong (the sizes are tuned separately).*
   - [ ] On a light desktop wallpaper and a dark one, and at 100%, 150%, 200% display scaling. *Recognizable on both; the dark plate edge is faint on a dark desktop (known, 1.6:1).*
2. **Tray icon**
   - [ ] Windows set to dark taskbar: *a white mark; a filled moon when styling is on.* Pause all styling: *the same mark as an outline.* Resume: *filled again.*
   - [ ] Switch Windows to a light taskbar while AllBlack11 runs (Settings > Personalization > Colors > Choose your mode, "Windows mode"). *The tray mark changes to near-black without restarting; no blank icon.* Switch back.
   - [ ] A contrast theme (Aquatic, then Night sky). *The mark stays visible against the taskbar in both.*
   - [ ] Hover the icon. *The tooltip reads "AllBlack11: N windows", plus "(trying a look, ...)" during a trial, "safe mode" after a Shift start, "classic dialogs on" when that layer is on.*
3. **Home and the credit**
   - [ ] Open Settings. *Home shows the colored icon beside "AllBlack11" and one plain line; the navigation pane has the small mark; at the foot of Home: "Made with love by TH0MSX" with TH0MSX underlined in blue.*
   - [ ] Tab to the credit. *A visible focus ring around TH0MSX only.* Press Enter, then try Space and a mouse click. *Each opens `https://github.com/TH0MSX` in the default browser, nothing else, no extra parameters; the status line says it opened.* Nothing may open when you merely hover or arrive at the page.
   - [ ] Settings & help > About. *The same credit, and a line saying the build is a pre-release not yet verified on hardware.* Edit nothing; just read it.
   - [ ] Narrator on, Tab to the credit. *It announces "TH0MSX - creator's GitHub profile", link.* Note the exact words.
   - [ ] Narrow the window to its minimum. *The link moves to its own line; nothing overlaps; start a trial and confirm the trial bar and countdown are not covered.*
   - [ ] Set AllBlack11 to Light (Settings & help > AllBlack11 itself), then a contrast theme. *Light: dark mark, dark-blue link. Contrast theme: no icon on Home or in the pane; the link uses the system link color.*
4. **Last known good needs a real run**
   - [ ] Note the time of `config.lkg.json` in the data folder. Change a setting and keep it. *The file does not change right away.* Wait 70 seconds (styling on, not paused, not in safe mode). *Now it equals `config.json`.*
   - [ ] Change a setting, then quit from the tray within 5 seconds. *`config.lkg.json` is unchanged (too short a run).* Change another, wait 15 seconds, quit from the tray. *It now matches (clean exit after 10 s).*
   - [ ] Start with Shift held (safe mode), change a setting, wait 70 seconds. *`config.lkg.json` is not updated.*
5. **Look history**
   - [ ] Appearance > Quick looks > Accent glass, then Keep. Safety > Look history. *One line, "Changed the default look" (or "Scene: ..." for a scene), a few seconds old.*
   - [ ] Press **Undo last change**. *The previous look comes back, Settings follows, the list is empty again, and the tray tooltip and windows update.* Repeat with two different changes: *Undo goes back one at a time, newest first.*
   - [ ] Press Undo with nothing listed. *"There is nothing to undo yet..." and no change.*
   - [ ] Confirm Reset all windows is unchanged and still means "as Windows draws them".
6. **Rule playground**
   - [ ] Rules > type `notepad.exe` in the rule box > **Test typed app**. *"Testing notepad.exe. Nothing is applied." and where each setting comes from; no rule is added; `config.json` is unchanged.*
   - [ ] Test `dwm.exe` and an app in your never-theme list. *"Skipped." with the reason; protected ones say no list changes it.*
   - [ ] Add two overlapping rules and test an app both match. *The winner, the loser and "priority N beats priority M" or the specificity sentence.*
   - [ ] Pick a window on Fix this app, then **Test picked window**. *The answer matches the Fix this app report's "which rule won".*
7. **Contrast check**
   - [ ] Appearance > bottom. *"All 8 measured pairs pass" for the shipped look.* Set the title bar text to a dim gray on a dark title bar. *A "Fails: ..." line with a suggested hex that passes; apply it and re-check.*
8. **Exit choice**
   - [ ] Tray > Exit with nothing special running. *It just exits.* Start a trial, then Exit from the tray. *A question listing that the trial will be reverted; the default button keeps AllBlack11 running.* Turn on the dark classic dialogs layer, Exit. *It mentions removing that layer.*
9. **Native x64 and ARM64**
   - [ ] Run the x64 exe on an x64 PC and the ARM64 exe on the ARM64 laptop. *Same behavior; About names the architecture.*

## U. The UI pass (preview.2)

1. **Home and the vertical slice**
   - [ ] Delete `%APPDATA%\AllBlack11\onboarding.json` and the config (or use a fresh account) and
         start AllBlack11: Settings opens on the guided setup, not Home. An *upgrade* (existing
         config, no onboarding file) must **not** show setup.
   - [ ] Setup, seven steps, Back / Next / Skip each work; close Settings on step 3 and reopen: it
         resumes on step 3. Nothing is enabled that you did not choose (startup stays off).
   - [ ] On "Pick an app", drag the crosshair onto Notepad; on "Keep or revert" press **Try**; a bar
         with a countdown, **Keep** and **Revert** appears above the page; the page and the edit
         boxes shift down together (no box overlapping text).
   - [ ] **With the keyboard only**: Tab reaches Keep, then Revert; Enter on Keep saves; Esc
         reverts. Starting a trial with Enter on **Try** puts focus on **Keep**.
   - [ ] Home afterwards says customization is on, names the look, and lists any experimental
         feature that is on.
2. **Navigation and appearance**
   - [ ] All eight pages open and none is blank or overlapping. Narrow the window to its minimum:
         the pane becomes an icon rail with tooltips, nothing is clipped, no horizontal scrolling.
   - [ ] Settings & help > AllBlack11 itself: System, Light and Dark each recolor Settings and the
         tray menu; none of them changes how Notepad looks.
   - [ ] Turn Windows between light and dark while Settings is open (Choice = System): it follows.
   - [ ] The title bar of Settings follows the choice.
3. **DPI and text size**
   - [ ] Display scaling 100%, 125%, 150%, 200%: every card is as tall as its text, no clipped
         text, no overlapping controls, edit boxes inside their cards, focus ring never cut off.
   - [ ] Settings > Accessibility > Text size at 150% and 225%: same.
   - [ ] Drag Settings between two monitors of different scaling: the layout re-flows once, text is
         sharp, hit regions match what is drawn (click the right edge of a button).
   - [ ] A long name: pick a window with a long title or path; text wraps or ends in an ellipsis,
         never over a neighboring control.
4. **High contrast and reduced effects**
   - [ ] Windows contrast theme "Night sky" and "Aquatic": every page readable, the selected page
         and focus ring distinct, nothing relies on a color you cannot see; switch back and
         Settings repaints without restarting.
   - [ ] Transparency effects off, animation effects off: nothing changes (nothing in the UI uses
         either).
5. **Keyboard (no mouse)**
   - [ ] Tab / Shift+Tab visits the pane once, every control of the page in reading order, and Keep
         and Revert during a trial; focus never disappears; Shift+Tab reverses.
   - [ ] F6 moves between pane and page; Up/Down/Home/End choose pages in the pane; Ctrl+Tab and
         Ctrl+Shift+Tab change page; Ctrl+K opens the palette.
   - [ ] Enter and Space operate every button and switch; Left/Right moves a slider and the
         selection in a row of buttons or in the Scenes list.
   - [ ] After a dialog closes (Import recipe, Choose picture), focus returns to a sensible
         control on the same page.
6. **Narrator and UI Automation** (the part most likely to need fixing)
   - [ ] Turn Narrator on (Win+Ctrl+Enter). Tab through Home: each stop is announced with a name
         and a role (button, switch with On/Off, slider with a value, navigation item as selected).
   - [ ] Toggle a switch with Space: the new state is announced. Move a slider: the value.
   - [ ] Start a trial: Narrator announces Keep; it can press it.
   - [ ] **Edit boxes**: focus the color box on Appearance. Is a label announced? (Known gap: it may
         say "edit" only.) Note exactly what you hear.
   - [ ] Scroll a long page with Narrator on: does it announce rows that are off screen? (Known
         gap: rows scrolled out of view are not marked off-screen.)
   - [ ] Run Accessibility Insights for Windows (FastPass) on Settings; save the result. Expect
         findings on the edit boxes; anything else is new information.
   - [ ] If Settings misbehaves with a screen reader running: start AllBlack11 with environment
         variable `ALLBLACK11_NO_UIA=1` and note whether the problem disappears.
7. **Walkthroughs and Help**
   - [ ] Settings & help > each of the six walkthroughs: the tip appears on the right page, the
         outlined control is the one described, Next tip and Close tip work from the keyboard, and
         the outlined control still works. Run **Pick and inspect** with no window picked: the
         first step asks you to pick one.
   - [ ] Help search: type "revert", "privacy", "why": each finds a topic. Clear the box: all topics.
8. **Tray menu**
   - [ ] Right-click the tray icon with AllBlack11 set to Light, then Dark, while Windows is the
         opposite: the menu follows the AllBlack11 choice. Every row has its own text.
9. **Setup program** (unchanged, but look at it)
   - [ ] Run Setup: it still works and is legible at 150% scaling and in a contrast theme. It does
         not follow the new design system (documented gap).

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
