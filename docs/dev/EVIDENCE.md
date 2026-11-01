# Evidence for 3.7.0-preview.1

What was actually done to each feature. Categories, strongest last:

- **Static review**: read and reasoned about, nothing executed.
- **Unit-tested**: pure logic covered by `cargo test --lib` (269 tests, run on Linux). These test
  decisions, parsers and state machines, never a window.
- **Cross-compiled**: type-checked and linked for ARM64 and x64 from Linux (`./build.sh`). This
  proves it builds. It proves nothing about behavior.
- **Native CI** (new in 3.7): `cargo test --lib` and a build of every crate on real Windows ARM64 and x64 runners. It proves the Windows-only code compiles and the pure tests pass there; it exercises no window.
- **Wine-rendered** (preview.2): the real x64 `AllBlack11.exe` was run under Wine 9.0 on Linux
  with a virtual display, and screenshots and synthetic clicks and keys were used to look at layout
  and basic interaction. **Wine is not Windows**: its font substitution, DWM, DPI handling,
  theming and UI Automation client support are all different, so this proves the layout code
  places things sensibly and the key flows do not crash, and nothing about how Windows draws or
  announces them.
- **Runtime ARM64**: run on a Snapdragon Windows 11 ARM64 laptop (the owner's).
- **Runtime x64-emulated**: an x64 build run on that laptop under emulation.
- **Runtime native x64**: run on a real x64 PC.
- **Not tested.**

**No 3.5 or 3.6 feature has been run on Windows yet.** The environment this was written in has no
Windows machine. "Runtime ARM64" below means only what the owner ran from the 3.0 Preview.
Every row that says "not tested" needs the smoke test in `SMOKE-TEST.md`. CI also builds and
unit-tests on a native `windows-11-arm` (ARM64) runner; that proves the Windows-only code
compiles and the pure tests pass there, and nothing about tray, Settings, hooks or the helper
at runtime. x64 is only cross-compiled (the `windows-latest` image has no `llvm-cvtres`, which
the build script needs).

| Area | Unit-tested | Cross-compiled | Runtime ARM64 | Runtime x64-emu | Runtime x64 |
| --- | --- | --- | --- | --- | --- |
| Legacy eligibility (only classic dialogs; Explorer/Control Panel/MMC frames refused; shell surfaces skip whole dialogs) | yes: `legacy_policy`, `inject` | yes | not tested | not tested | not tested |
| Capability table, palettes, contrast (every palette pair ≥ 7:1 body, ≥ 3:1 disabled) | yes | yes | not tested | not tested | not tested |
| Target classification, ARM64EC refusal, routing | yes: `target`, `inject` | yes | not tested | not tested | not tested |
| Hook v2 (WH_CALLWNDPROCRET, entries, restore, pin) | no (needs Windows) | yes | not tested | not tested | not tested |
| Control harness (every control family and state, self-hook, 50× leak check) | no | yes (4 builds) | not tested | not tested | not tested |
| x64 helper and pipe IPC (experimental) | pure parts via `target`/`legacy_policy` | yes | not tested | not tested | n/a |
| Rule engine: precedence, specificity, provenance, conflicts, globs | yes (10 tests) | yes | not tested | n/a | n/a |
| Config schema 2, 3.0 → 3.5 migration, unknown/future fields, corrupted file fallback | yes | yes | not tested | n/a | n/a |
| Explain engine (framework hints, reason text, support table, "accepted ≠ confirmed") | yes | yes | not tested | n/a | n/a |
| Window picker and inspector | report/redaction yes | yes | not tested | n/a | n/a |
| Sanitized report (no titles, no user names or profile paths by default) | yes | yes | not tested | n/a | n/a |
| Recipes (closed schema, validation, diff, apply, export round-trip) | yes (8 tests) | yes | not tested | n/a | n/a |
| Compatibility notebook | yes | yes | not tested | n/a | n/a |
| Focus Look and reduce-effects decisions | yes | yes | not tested | n/a | n/a |
| Trial look (15 s auto-revert), last known good | LKG picking yes; trial no | yes | not tested | n/a | n/a |
| Ownership-aware shell restore | decision logic yes | yes | not tested | n/a | n/a |
| Tray menu item identity (title/separator regression) | yes (model) | yes | **the original bug was seen and the fix not yet re-run** | n/a | n/a |
| Settings pages, DPI layout, keyboard focus, hit testing | no | yes | 3.0 layout ran; 3.5 pages not tested | n/a | n/a |
| Materials demonstration window | no | yes | not tested | n/a | n/a |
| Setup, upgrade from 3.0, uninstall, Explorer restart | no | yes | 3.0 Setup ran once; 3.5 not tested | n/a | n/a |
| `measure.ps1`, `--reload` | no | `--reload` yes | not tested | n/a | n/a |

## Added in 3.6

| Area | Unit-tested | Cross-compiled | Runtime ARM64 | Runtime x64-emu | Runtime x64 |
| --- | --- | --- | --- | --- | --- |
| Trial look: never writes before Keep, one write on Keep, timeout/revert, cancel on outside change, constrained to window looks (`trial.rs`) | yes (9 tests, with a memory/disk model) | yes | not tested | n/a | n/a |
| Trial wiring in the host (tray preset / reload / enable / shell toggles / LKG cancel a running trial; Keep writes first and reverts on a failed write) | no (needs Windows) | yes | not tested | n/a | n/a |
| Window identity: HWND reuse inside one process (fingerprint of thread, class, module, owner, style bits) (`identity.rs`) | yes (7 tests) | yes | not tested | n/a | n/a |
| Fingerprint checks before apply/restore, in snapshots (old files still load), picker entries and commands | snapshot format yes; checks no | yes | not tested | n/a | n/a |
| Helper protocol: four verbs, closed surface, bounded line buffer, backoff, "may the helper be used" gate (`helper_protocol.rs`) | yes (7 tests) | yes | not tested | not tested | n/a |
| Helper controller: bounded queue, 3 s reply wait on the worker thread, generation counter, stop-and-quit, process handle closed once | no | yes | not tested | not tested | n/a |
| Helper identity: parent creation-time check, first-instance owner-only pipe, PID checks | no | yes | not tested | not tested | n/a |
| "Fix this app": per-surface plan, helper route text, receipt (`fixapp.rs`) | yes (10 tests, incl. banned phrases) | yes | not tested | n/a | n/a |
| Safe startup decision (`startup.rs`) | yes (7 tests) | yes | not tested | n/a | n/a |
| Safe-start marker file, Shift/`--safe`, forcing experimental features off for one run | no | yes | not tested | n/a | n/a |
| Emergency pause / resume, leave safe mode | no | yes | not tested | n/a | n/a |
| Scenes and Look Book: five scenes, apply touches only defaults + Focus Look, rules survive, bounded book, hostile files refused (`scenes.rs`) | yes (11 tests) | yes | not tested | n/a | n/a |
| Scenes page, list, chips, Try/Keep, tray Scenes submenu, one-step undo | no | yes | not tested | n/a | n/a |
| Palette from a picture: contrast guarantees, bounds, determinism (`palette.rs`) | yes (11 tests, 400-case sweep) | yes | not tested | n/a | n/a |
| GDI+ sampling of a picture (`platform/image.rs`) | no | yes | not tested | n/a | n/a |
| Identity accents: stable assignment, overrides, bounds, file validation (`accents.rs`) | yes (9 tests) | yes | not tested | n/a | n/a |
| Identity accents applied to the border only where no rule sets one | no | yes | not tested | n/a | n/a |
| Day/Night: phases, wrap, clock jumps, manual choice persists to next boundary (`schedule.rs`) | yes (7 tests) | yes | not tested | n/a | n/a |
| Day/Night timer, resume and time-change handling in the host | no | yes | not tested | n/a | n/a |
| Command palette: command list, ranked search (`commands.rs`) | yes (11 tests) | yes | not tested | n/a | n/a |
| Command palette window, Ctrl+K, tray item, optional `RegisterHotKey` | no | yes | not tested | n/a | n/a |
| Before / after pair of owned windows | no | yes | not tested | n/a | n/a |
| Settings control inventory (`scripts/check-controls.py`): no control dropped by the page split | yes (static check, 94 controls) | n/a | not tested | n/a | n/a |

## Added in preview.2 (UI pass)

| Area | Unit-tested | Cross-compiled | Wine-rendered | Runtime ARM64 / x64 |
| --- | --- | --- | --- | --- |
| Palettes, contrast (text 4.5:1 and control edges 3:1 on every surface, accent adjustment, status colors never the only signal, high contrast = system colors), type ramp, 4 px grid, scale bounds (`uitheme.rs`, 11 tests) | yes | yes | n/a | not tested |
| Onboarding state machine: fresh vs upgrade, resume, skip/finish, hostile files (`onboarding.rs`, 9 tests) | yes | yes | n/a | not tested |
| Help topics, search, walkthrough content: every step names a real page and control, adapts to "picked a window / paused", asks for nothing destructive (`help.rs`, 7 tests) | yes | yes | n/a | not tested |
| Navigation pane, Home, page titles, eight pages, rows measured to their text | no | yes | yes (1100x800, 690x700 minimum, 1300x900 at 150% *Text size*) | not tested |
| Guided setup screens | state machine yes; screens no | yes | partly: wizard rows seen under Wine; every step not captured | not tested |
| Walkthrough tip and highlight ring (Pick and inspect, step 1) | content yes | yes | yes, one walkthrough | not tested |
| Keep / Revert from the keyboard; trial bar layout; edit boxes follow it | no | yes | yes (Enter focuses Keep, Tab to Revert, Enter reverts; trial started with the mouse) | not tested |
| Shared button, focus ring, hover/pressed states | no | yes | static states only; hover and pressed need a real pointer | not tested |
| UI Automation provider (hand-written COM: simple/fragment/root, Invoke, Toggle, SelectionItem, Value, focus and layout events) | no | yes | **in-process vtable self-test passed under Wine** (`ALLBLACK11_UIA_SELFTEST=1`); IIDs checked against Wine's `uiautomationcore.idl` | **no UI Automation client has ever queried it; Narrator not run** |
| Tray menu follows AllBlack11's appearance choice | no | yes | not shown (tray not drivable under Wine) | not tested |
| `--safe` and `--open-settings` arguments | no | yes | yes (`--open-settings` drives every screenshot) | not tested |
| High-contrast rendering | palette rule yes | yes | no | not tested |
| Multi-monitor DPI change | no | yes | no (Wine did not apply a DPI change) | not tested |

### Defects found and fixed during this pass

1. **Keep needed a mouse.** The trial bar was not in the Tab order, Enter and Space were refused
   during a trial, and the bar was invisible to UI Automation. (Present since 3.6.0-preview.1.)
2. **`--safe` was rejected** as an unknown argument although documented.
3. **Native edit boxes were not repositioned** when the trial bar appeared or went away (found in a
   Wine screenshot: the Save look box overlapped the text above it).
4. **Row heights were guessed** from character counts for the narrowest window, leaving large empty
   cards at normal widths and risking clipped text for long or translated strings. They are now
   measured with the painting font, width and flags.
5. **The tray menu ignored AllBlack11's own Light/Dark choice.**
6. **Comments cited Microsoft pages and numbers that the pages do not contain** (a "Layout metrics
   and spacing" page, "50 to 60 characters", "40 px for touch"). Corrected; `UI-GUIDELINES.md` lists
   what each source actually says.

### Wine observations that are *not* verified as defects

- Zeros in Wine's fallback font are drawn slashed (`#000000` looks struck through). A font
  substitution artifact, not seen as a layout problem.
- At the minimum window width one description lost its trailing period at a wrap boundary in a
  screenshot. Measuring and painting use the same function, font, flags and width, so this is
  probably Wine's `DrawText`; unconfirmed. Check on Windows.
- "AllBlack11 did not exit cleanly the last N times": the safe-start marker doing its job, because
  the test harness kills the process between screenshots.

## Added in 3.7.0-preview.1

| Area | Unit-tested | Cross-compiled | Native CI | Wine-rendered | Runtime Windows |
| --- | --- | --- | --- | --- | --- |
| Brand icon geometry, 10 sizes, tray marks, SVG; contrast of palette and mono marks | `brand_icon.py check` (also in CI) | yes | yes (check job) | PNG renders inspected at 16 to 256 px on four backgrounds | **seen on ARM64**: installer, window title bars, taskbar, tray overflow. Not tested: Start, Alt+Tab, shortcut, theme swap |
| Icon resources linked (ids 1, 10 to 13), `LoadIconWithScaleDown`, class icons | no | yes | yes (builds) | tray not drivable; nav mark and Home icon seen | not tested |
| Brand tokens in `Palette` (contrast-tested); Home header; nav mark; credit row | yes (tokens, `brand.rs` 4 tests) | yes | yes | yes: Home in Dark and Light, focus ring on the link | not tested; link launch and Narrator not tested |
| Credit link URL rules | yes | yes | yes | no | browser launch not tested |
| `lkg::Promotion` (known good only after a real run) and its wiring | yes (6); wiring no | yes | yes | not exercised (needs 60 s and a clean exit) | not tested |
| `atomic.rs` (flushed write, failed write, stale temp, failed rename) | yes (5, on Linux) | yes | yes | n/a | power-loss behavior cannot be tested here |
| Look history and undo | yes (9 + 1) | yes | yes | yes: Keep recorded, Undo restored the earlier `default` look and was not re-recorded | **recording seen on ARM64** (a kept scene was listed); Undo not shown |
| Rule playground, exclusion reasons | yes (6 + 2; includes a test that `exclusion_for` agrees with `decide`) | yes | yes | yes: protected app skipped with reason; plain app shows sources | not tested |
| Contrast Lab | yes (7) | yes | yes | yes (render only) | not tested |
| Tray tooltip text; exit consequences | yes (6) | yes | yes | no (tray) | not tested |
| Exit confirmation dialog | no | yes | yes | no | not tested |
| Native Windows x64 CI job (MSVC `rc`/`cvtres` fallback) | n/a | n/a | **yes: run 41267253779 passed on `windows-latest`** | n/a | n/a |
| Installer header | no | yes (`./build.sh`) | not built natively (needs the payload) | not run | not tested |

### Observed on real hardware (3.7.0-preview.1, ARM64)

The author ran the ARM64 installer and build on a Windows 11 ARM64 PC (Snapdragon) and sent screenshots on 2026-11-01. What those screenshots show, and nothing more:

- **Installer**: the new header (mark, name, "3.7 Preview · true black for Windows 11", blue line), the new icon in its title bar, "Native ARM64 build for this ARM64 PC", Update / Uninstall / Close.
- **Tray**: the white mark in the notification overflow; the tray menu titled "AllBlack11 3.7 Preview · ARM64" with Command palette, Presets, Scenes, Pause all styling, Shell, Maintenance, Start with Windows (checked), Exit.
- **Taskbar and desktop**: the new icon on two taskbar buttons; Control Panel, Run and File Explorer drawn with black title bars alongside Settings.
- **Safety**: Look history listing "18 min ago · Scene: OLED Black" (so a kept scene was recorded on hardware); the Pause, Reset windows, Undo shell and Turn off experimental cards. **Undo itself was not shown.**
- **Diagnostics**: "Native ARM64 build on an ARM64 PC (build ARM64, PC ARM64)", Windows build 26300, high contrast off, and the build-support list (Mica, Mica Alt, Acrylic supported; tinted acrylic experimental).
- **Settings** (icon rail, narrow window): Home with the brand header and "Made with love by TH0MSX" (TH0MSX underlined); Fix this app with a live window list; Appearance with the live preview, quick looks and toggles; Scenes with the scene list and detail for "Focus"; Rules with Test without applying; "Kept." in the status line.

**Not shown by those screenshots**, so still unverified: the x64 build on x64; Narrator and any UI Automation client; the credit link opening a browser; the tray swap between light and dark taskbars and contrast themes; Undo in the Look history, Rule playground results, Contrast check, Exit dialog and last-known-good timing; the Start menu, Alt+Tab and shortcut icons; 125% to 200% scaling; the helper.

### Defects found and fixed in 3.7

1. **A config became "last known good" the moment it parsed**, at startup and on every reload, before anything was applied. A config that parsed but misbehaved would have replaced the recovery copy.
2. **Writes were not flushed before the rename**, so a power cut could leave an empty file in place of the config on some file systems.
3. **A kept trial would not have been recorded in any history**: the host's config already equaled the file when the Keep reload arrived. Found while testing the history under Wine; fixed by recording from the trial's saved "before".
4. **Wrapped text was sliced** by Wine's `DrawText` when its break came later than the measured one (seen in 3 screenshots); wrapped text is now drawn without the rectangle clip.

### Still unproven or open

- Native edit boxes have no accessible label (Q01).
- The icon and tray behavior on Windows; the tray icon swap on theme change.
- `AppUserModelID` is not set.
- The helper was re-read, not run.

## Audit findings fixed in 3.6 (all were real in 3.5)

1. **Stale trial revert.** A tray preset, reload or enable toggle during a 15 s trial left the
   trial running; its timer then "reverted" to a copy taken *before* the change, silently
   undoing it in memory while the file kept it. Every permanent-change path now ends the trial
   first. (Wiring is static review plus cross-compile; the state machine is unit-tested.)
2. **Keep could diverge from disk.** The host was told "kept" before the file was written. Keep now
   writes first and reverts the trial if the write fails.
3. **HWND reuse in one process.** PID plus creation time cannot tell a closed dialog from a new
   one that received the same handle. Apply and restore now compare a fingerprint and treat
   doubt as "no". It cannot *prove* identity (Windows has no never-reused window id).
4. **Helper lifecycle.** Unbounded queue and an unbounded blocking read, no backoff, and a
   retiring worker could clobber its replacement's state. Fixed as described above. The helper
   also now checks the parent's *creation time*, not just its PID.
5. **Look Book validation** ran after serde had already dropped unknown recipe keys; schedule
   times accepted `+7:00`; a safe-start marker of `[]` read as a crash. All found by the new tests.

## Known gaps, stated plainly

- Screen readers: the Settings window is custom drawn. Preview.2 adds a hand-written UI Automation
  provider (names, roles, states, values, patterns, focus events) that passed an in-process
  self-test under Wine. **No screen reader or UI Automation client has been run against it.**
  Known gaps: native edit boxes have no programmatic label; rows scrolled out of view are not
  flagged off-screen; whether Narrator reads a switch as a "switch" or a "check box" is unknown.
  Keyboard access covers every control including Keep and Revert.
- DPI: layout is logical-pixel based and scales per monitor. 100–200% has not been looked at on
  Windows; 100% and 150% *Text size* were looked at under Wine.
- Whole-window opacity is not offered: changing another app's window style is invasive, and
  the tint-opacity slider is clearly labeled as material tint only.
- The x64 helper has never been started. Treat it as unproven until `SMOKE-TEST.md` step 6
  passes on ARM64 hardware.
- The helper's pipe is local-only, first-instance, owner-only, and each side checks the other's
  process. That keeps other users and accidents out. It is **not** a defense against malware
  running as you, which can already inject into your processes by other means.
- Window identity is a strong heuristic, not a proof.
- Picture reading uses GDI+ on a file you chose, bounded to 25 MB, 24 megapixels and 64 x 64
  samples. Very large images can still make Settings pause briefly while they decode.
- Not implemented in 3.6 (deferred, see `HANDOFF.md`): Window Anatomy view, per-window undo
  timeline, display-aware scenes, controller-friendly Living Room view, Recipe Remixer, saved
  thumbnails, and capturing another app's window for comparison.

## 3.8.0-preview.1 additions

Same categories as above. **Nothing below has been run on Windows.** "Wine" means the real x64
exe was run under Wine for layout and basic interaction only.

| Area | Unit-tested | Cross-compiled (x64 and ARM64) | Wine-rendered | Windows runtime |
| --- | --- | --- | --- | --- |
| Release policy and version ordering (`update.rs`: stable vs preview, flag and tag, channels, skipped, arch pick, malformed JSON, missing assets, rate limits, clock skew) | yes | yes | n/a | not tested |
| SHA-256 (NIST vectors, streaming) and bounded download (`sha256.rs`, `download.rs`: cancel mid-stream, oversize, short and long body, disk full, wrong digest, unsafe names, stale cleanup) | yes | yes | n/a | not tested |
| WinHTTP client, updater worker, Install handoff (`platform/http.rs`, `updater.rs`) | no (needs Windows) | yes | the failure path only (the sandbox network answered non-200); real GitHub not reached | not tested |
| Updates card, Home badge, balloon | layout text yes | yes | card rendered | not tested |
| Pack schema, validation, install/remove, precedence, catalog honesty (`packs.rs`) | yes (11 tests) | yes | section rendered; Install wrote a `Pack: Microsoft Store` rule | not tested |
| Package identity for `ApplicationFrameHost` frames (`procinfo.rs`) | no | yes | no | not tested |
| Whether caption/border colors apply to Store and Calculator windows | n/a | n/a | n/a | **unknown; the pack says so** |
| Look history names pack install/remove (`history.rs`) | yes | yes | n/a | not tested |
| Exclusion manager (`exclusions.rs`: layers agree with `policy::exclusion_for`, class list validation) | yes | yes | section rendered | not tested |
| Text-box names (`labels.rs` table; `accname.rs` `IAccPropServices::SetHwndPropStr`, vtable checked against the SDK `oleacc.idl`) | table only | yes | the service is absent under Wine, so 0 of 9 were named and the quiet failure path ran | **not verified with Narrator, NVDA or Accessibility Insights** |
| Recipe Remixer (`recipes::remix`: parts, contrast refusal, target mismatch, round-trip through the strict parser, every bundled pair) | yes (4 tests) | yes | section rendered; result and diff shown for Graphite + Black frames | not tested |
| Wallpaper generator (`generate.py`: deterministic, OLED rules checked) and manifest | yes (manifest, recorded stats, sizes, SHA-256 format) | n/a | contact sheet viewed | n/a |
| Gallery logic (`wallpaper.rs`: manifest validation, URL allowlist, fit/position, state, placement) | yes (8 tests) | yes | page, tile selection, full-screen preview and the "not attached yet" download message seen | not tested |
| `IDesktopWallpaper` apply, put back, display list (`gallery.rs`, vtable checked against `shobjidl_core.idl`) | no | yes | the service is absent under Wine (error shown); apply never ran | not tested |
| Per-display fit | n/a | n/a | n/a | **not possible through Windows' API (one position for all displays); documented, not implemented** |

## 3.8.0-preview.2 stabilization additions

**Nothing below has been run on Windows.**

| Area | Unit-tested | Static | Cross-compiled | Wine-rendered | Windows runtime |
| --- | --- | --- | --- | --- | --- |
| Destination registry, legacy numeric switch, tree shape (`destinations.rs`, 7 tests) | yes | n/a | yes | n/a | not tested |
| Shared index and search (`commands.rs`, 20 tests: synonyms, every destination reachable, scenes/looks/packs/wallpapers/help present, search never runs actions) | yes | 55 anchors checked against the page builders (`check-controls.py`) | yes | results page, selecting a result, scroll, focus ring and "Found" mark seen | not tested |
| Six-category shell, hubs, subpages, Back, expanders | exhaustive `match` on every destination | every control still on a page (129) | yes | Home, hubs, subpages, expanders, Back seen at 100%, 150%, 200% (Home) | not tested |
| Window bounds (`windowbounds.rs`, 9 tests: default, clamping, scaling, missing monitor, partly off-screen, damaged file) | yes | n/a | yes | n/a | **not tested** (real monitor changes) |
| Search box focus order, Tab out of boxes, Esc/Enter/Down | no (window code) | n/a | yes | partly (typing, clearing) | not tested |
| UIA: names for cards, results, sections, search; `IsOffscreen` | no | n/a | yes | our provider's self-test passed (23 of 23 elements named) | **not tested with Narrator, NVDA or Accessibility Insights** |
| Store/Calculator evidence chain (`packs::assess`, `delta`, marks; 6 tests) | yes | n/a | yes | "Target not detected" seen (no Store under Wine) | **not tested**: window enumeration, package read, report |
| Whether caption colors reach Store or Calculator frames | n/a | n/a | n/a | n/a | **unknown** (owner: no visible change) |

Corrections to earlier rows: in 3.8.0-preview.1 the two packs were described as "partly supported";
they are now "Experimental (no visible effect shown)". The UI Automation provider reported every
element as on-screen; it now reports scrolled-out content as off-screen.

## Owner screenshots of 3.8.0-preview.2 on Windows 11 ARM64 (2026-11-01)

The owner ran the ARM64 installer on a Snapdragon Windows 11 ARM64 PC and sent eleven screenshots
(the app windows from these are the images in `docs/mockup-source/shots`; the desktop shots also
showed notifications and personal data and are **not** kept or published). They are runtime
evidence for what they show, and for nothing else.

**Seen working on Windows (ARM64):** the six-category shell with the search box and Fluent icons
in the pane; the selected item marked by a filled row and an outline; the hub pages (Appearance,
Apps, Help & About) with cards and chevrons; the Back link on a subpage ("‹ Appearance"); Effects and
colors with the live preview and Quick looks; Safety with **Look history naming real changes**
("Installed the Microsoft Store pack", "Removed the Calculator pack", "Scene: OLED Black", "Changed the
default look"); Settings with the Updates card showing "AllBlack11 3.8.0-preview.2 (preview)"; the
Wallpapers page with all eight preview tiles; the Settings window itself styled black while the
Control Panel frame behind it also showed a dark title bar and the Run dialog body stayed light
(the classic-dialogs layer is off by default).

**Not shown, so still unverified:** search results and the focus/mark on a result; keyboard
navigation; Narrator or NVDA; an actual update check, download or install; a wallpaper download,
apply or put back; whether the Store or Calculator pack changed any window (the history shows the
packs were installed and removed, and the owner reported no visible change); DPI other than the
captured one; the x64 build.

