# Evidence for 3.6.0-preview.2

What was actually done to each feature. Categories, strongest last:

- **Static review**: read and reasoned about, nothing executed.
- **Unit-tested**: pure logic covered by `cargo test --lib` (225 tests, run on Linux). These test
  decisions, parsers and state machines, never a window.
- **Cross-compiled**: type-checked and linked for ARM64 and x64 from Linux (`./build.sh`). This
  proves it builds. It proves nothing about behavior.
- **Wine-rendered** (new in preview.2): the real x64 `AllBlack11.exe` was run under Wine 9.0 on Linux
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
