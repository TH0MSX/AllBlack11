# 3.7.0-preview.1 inventory: what exists, what does not

Branch `claude/workbench-3.7` (from `claude/ui-polish-fluent` @ `9ef0ea2`, which stays as the
recoverable checkpoint). The brief asked for 30 features, 5 wizards and 30 quality-of-life items
as a **prioritized backlog**. This pass finished a few completely and left the rest alone rather
than leave decorative stubs. Nothing here has run on Windows.

Evidence labels: **unit** = `cargo test --lib` on Linux (269 tests). **cross** = cross-compiled
for ARM64 and x64. **native CI** = built and unit-tested on real Windows runners (ARM64 and x64).
**Wine** = the real x64 exe rendered and driven under Wine (layout and flow only; Wine is not
Windows). **pending** = needs Windows hardware (see SMOKE-TEST.md, section 3.7).

Status words: **done**, **partial** (the slice that exists is complete and wired; the rest is
named), **existing** (already in the baseline; not changed or only touched), **deferred** (not
started; no UI, no stub).

## Features (F01 to F30)

| # | Item | Status | Entry point and evidence |
| --- | --- | --- | --- |
| F01 | Window Anatomy | deferred | Needs an input-transparent overlay or an owned-preview view; no safe Wine-testable path. Plan: owned preview first, observed (DWM attributes, class) vs guessed (framework) kept apart as in `explain.rs` |
| F02 | Per-window undo timeline | **partial** | **Look history**: Settings > Safety > Look history, `history.rs`, host recording in `app.rs` (`record_history`, kept trials from `TrialMachine::previous`). Bounded (20), named, coalescing, in memory. Undo parses the earlier config before writing and distinguishes itself from Reset all windows. unit (9) + trial test + cross + native CI + Wine (Keep then Undo restored the earlier `default`). **Deferred**: the per-*window* timeline; "restore an earlier look" for a single window |
| F03 | Display-aware scenes | deferred | Needs stable monitor identity and hot-plug handling |
| F04 | Living Room interface | deferred | |
| F05 | Recipe Remixer | deferred | |
| F06 | Scene Layers | deferred | |
| F07 | Contrast Lab | **done** (for the measured colors) | Settings > Appearance > Contrast check; `contrast_lab.rs`. Measures title bar text, border and all 7 classic-dialog palette pairs; words first, nearest hue-preserving fix, states it does not measure (Windows-drawn hover/focus/selection). unit (7) + Wine render. Interaction states of classic controls are **not** measured |
| F08 | Palette locking | deferred | |
| F09 | Multi-image palette board | deferred | |
| F10 | Scene variants | deferred | |
| F11 | Workspace collections | deferred | |
| F12 | Shared application profiles | deferred | |
| F13 | Rule Playground | **done** | Settings > Rules > Test without applying (typed app, or the window picked on Fix this app); `playground.rs`, `policy::exclusion_for`. Same engine as the theming pass; applies and saves nothing; never reads titles. unit (6 + 2) + Wine (protected app skipped with the reason; plain app shows each setting's source). "Selected current windows" is limited to the picked window's program and class |
| F14 | Application version guards | deferred | |
| F15 | Compatibility drift review | deferred | |
| F16 | Control-level overrides | deferred | |
| F17 | Legacy state playground | deferred | The harness exists (`harness/`); interactive states and the checklist were not added |
| F18 | Target exclusion manager | **partial** | The explanation engine exists and is tested (`Exclusion`, protected vs yours, can-be-lifted) and is shown by the Rule Playground. The existing exclusion lists stay on Rules. A single manager for global/app/window exclusions with precedence text is **deferred** |
| F19 | Screenshot comparison session | deferred | |
| F20 | Scene contact sheet | deferred | |
| F21 | Recipe lineage | deferred | |
| F22 | Portable theme bundle | deferred | |
| F23 | Styling event timeline | deferred | The log file exists (`allblack11.log`); no in-app timeline |
| F24 | Helper health dashboard | existing | Safety > x64 helper and Diagnostics show state text and stop; no error/timeout counters or retry. Re-audited by reading `helper/src/main.rs` (owner-only first-instance pipe, local clients only, parent creation time and pipe-client PID checks, bounded buffers); no new defect found; **runtime unverified** |
| F25 | Performance budget monitor | deferred | |
| F26 | Conflict troubleshooter | deferred | |
| F27 | User-verified appearance checks | existing | The compatibility notebook (Fix this app: Works / Partly / Does not work, per app and Windows build) is the user-observed record, separate from API results; the style receipt still says "not verified". Not changed |
| F28 | Local adapter test mode | deferred | |
| F29 | Scoped automation interface | deferred | |
| F30 | Compatibility pack preview | deferred | |

## Wizards (W01 to W05)

All **deferred**. Each needs capabilities that do not exist yet (W01 needs F16/F17 and state
inspection; W02 needs F08; W03 needs F03; W05 needs bundles, F22/F30). W04 (Diagnose and Recover)
could be built from existing diagnostics but was not started. First-run setup is unchanged.

## Quality of life (Q01 to Q30)

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| Q01 | Label every editable field for UI Automation | deferred | Still the main accessibility gap. Plan: set the accessible name of each native edit box with `IAccPropServices::SetHwndPropStr` (the vtable order must be checked against Wine's `oleacc.idl` and run on Windows, so it was not written blind) |
| Q02 | Off-screen / enabled / selected / expanded states | partial | Enabled and selected are exposed; off-screen is not; "expanded" has no control to apply to |
| Q03 | Deterministic focus order | done (preview.2, unchanged) | One ring: pane, page, Keep, Revert |
| Q04 | Focus after a modal | deferred / unverified | |
| Q05 | Shortcut collisions | deferred | |
| Q06 | Searchable settings with deep links | deferred | Help search exists; it does not link to controls |
| Q07 | Unsaved changes marker | n/a | Settings apply and save immediately; there is no unsaved state to mark |
| Q08 | Section reset with preview | deferred | |
| Q09 | Copy/paste color values | deferred | |
| Q10 | Recent colors | deferred | |
| Q11 | Units, ranges, defaults | deferred | |
| Q12 | Actionable errors with details | partial | New messages (playground, undo, credit link) name the cause and what was not changed; no expandable details |
| Q13 | Explain why controls are disabled | partial | The new controls say why they do nothing when pressed (nothing picked, nothing to undo) |
| Q14 | Loading, cancellation, failure states | deferred | |
| Q15 | Preserve navigation and scroll | deferred | Pages reset to the top |
| Q16 | Inspect long names and paths | deferred | Long text wraps; no inspect view |
| Q17 | Empty states with a next step | partial | Rules, picker list, Look history, playground |
| Q18 | Tutorials dismissed and replayed | existing | |
| Q19 | Searchable shortcut reference | **done** | Help > Keyboard and accessibility (rewritten to match the code); found by Help search |
| Q20 | Unify spacing constants | **done** | `ui_metrics::PAGE_MARGIN` is 20 and the page margin, card gap and padding now come from it (`settings.rs`) |
| Q21 | Layout at DPIs and text sizes | partial | Wine: 100%, 150% text size, minimum width. Windows DPI steps pending |
| Q22 | Runtime theme / contrast changes | partial | Tray icon reloads on `ImmersiveColorSet`; Settings already followed. Not run on Windows |
| Q23 | Installer in the shared visual system | partial | Header carries the mark and brand accent. Body, dark mode and text scaling unchanged |
| Q24 | Config migration with backups and recovery messages | partial | Writes are flushed and tested; the last known good copy is no longer overwritten by an unproven config; the pre-3.5 backup existed. No new migration was needed |
| Q25 | Preview diagnostic redaction | deferred | |
| Q26 | Accurate tray status text | **done** | `traytext.rs`, 6 tests: active, paused, off, high contrast, plus trial, safe mode, reduced effects, classic layer; always fits 127 characters |
| Q27 | Coalesce notifications | deferred | |
| Q28 | Quit choice | **done** | Tray Exit asks only when it would revert a trial, remove the classic layer or stop the helper; default is to keep running (`confirm_exit`, `exit_consequences`) |
| Q29 | Truthful About | **done** | About shows version, build architecture and the verification status line (`brand::VERIFICATION_STATUS`) |
| Q30 | Native x64 CI | **done** | `windows-x64` job on `windows-latest` loads the MSVC environment; the build scripts fall back to `rc` and `cvtres`. A real run built every crate and unit-tested natively (run 41267253779 jobs: windows-x64, windows-arm64, check all green) |

## Branding and the other work this pass

| Item | Status | Where |
| --- | --- | --- |
| New original app icon, SVG source, 10-size ICO, 4 mono tray ICOs, PNGs, reproducible pipeline | done; **Windows shell display pending** | `assets/brand/`, `docs/dev/BRAND.md`; check runs in CI |
| Icon integrated: exe, Settings class icons, installer, shortcut, tray (theme-aware, filled/outline), Home, nav | done; pending on Windows | `assets/allblack11.rc`, `icon.rs`, `app.rs`, `settings*.rs` |
| Brand tokens | done | `Palette::brand`, contrast-tested |
| Creator credit link on Home and About | done; browser launch and Narrator pending | `brand.rs` (4 tests), `Row::Credit` |
| Phase 0: last known good no longer marked on parse alone | **done** (a real defect) | `lkg::Promotion` (6 tests), `app.rs` |
| Phase 0: atomic writes flushed and tested | **done** (a real defect) | `atomic.rs` (5 tests) |
| Phase 0: helper re-audit | read-only audit, no change | see F24 |

## Best next batch

1. **Q01** (label the edit boxes) with a Windows run, because it is the largest remaining
   accessibility gap and everything else depends on trusting the form fields.
2. **F18 manager UI** (reuse `exclusion_for`), **F24** counters (the controller already knows
   timeouts), **F23** timeline (a bounded ring over the existing log lines).
3. **F05 Recipe Remixer** and **F08 Palette locking** (pure color math with the Contrast Lab as its
   safety check), then **W02** on top of them.
4. After the hardware pass: F01 anatomy on the owned preview.
