# AllBlack11 development handoff (3.8 preview)

Keep this file current. It exists so the work survives a context reset.

## State (3.8)

- **Branch `claude/next-3.8`**, from the released `v3.7.0-preview.1` (merged to main as PR #6 plus the release commits). Version **3.8.0-preview.1**. Not merged, not tagged, not published; no pull request.
- What is in it: updater (A), compatibility packs (B), Recipe Remixer, exclusion manager and edit-box labels (C3, C4, C6), wallpaper gallery (D). CI (check, windows-arm64, windows-x64) was green at `a555e02`. Status per item: `docs/dev/INVENTORY-3.8.md`. Windows checklist: `SMOKE-TEST.md` section 3.8. Evidence: `EVIDENCE.md` "3.8.0-preview.1 additions".
- Release steps for the owner (the tag push and workflow dispatch are blocked from the assistant's sandbox): see `RELEASE-POLICY.md`. Publish as a **pre-release**; attach the eight wallpaper PNGs from `python3 assets/wallpapers/generate.py` (`assets/wallpapers/README.md`) to the same release; also tick "Set as a pre-release" on `v3.7.0-preview.1`, which was published without the flag.
- README still points at 3.7 (the recommended download) on purpose; change it when 3.8 is published.
- Wine harness: `scripts/ui-drive.sh PAGE ...` where PAGE is the navigation index (0 Home, 1 Fix, 2 Appearance, 3 Scenes, 4 Rules, 5 Safety, 6 Diagnostics, 7 Wallpapers, 8 Help); `rootshot PATH` captures the whole screen (for the wallpaper preview window).

## State (3.7, kept for history)

- Baseline: `7b65598` (3.0.0-preview.2 plus two docs/banner merges). Tag `v3.0.0-preview.2`.
- Crates: root exe, `hook/` (DLL, built for both CPUs), `setup/` (installer), `harness/` (test dialog, two manifests), `helper/` (x64-only helper shipped in the ARM64 package).
- Branches: `claude/peaceful-archimedes-ppfhwb` is the **3.5.0-preview.1 checkpoint** (head `7372140`, untouched). `claude/next-preview-3.6` continues from it: **3.6.0-preview.1** (head `9911cd3`, the recoverable 3.6 checkpoint; CI green; prerelease only). **`claude/ui-polish-fluent`** branches from that and is **3.6.0-preview.2** (head `9ef0ea2`, the UI-pass checkpoint, CI green). **`claude/workbench-3.7`** branches from it and is **3.7.0-preview.1**: brand, recovery fixes and the first workbench features. Nothing is merged, tagged or published; no pull request exists.
- Toolchain: Rust stable, `cargo-xwin` for ARM64/x64 Windows cross-builds from Linux. `./build.sh [arm64|x64|all]`.
- Nothing can be *run* in this environment. Every Windows behavior is "cross-compiled" at best.
  See the evidence table in `docs/dev/EVIDENCE.md`.

## Architecture decisions

1. **One policy, many binaries.** `src/legacy_policy.rs` is pure and is compiled into the exe,
   the hook DLL (`#[path]`), the x64 helper and the harness. Eligibility, the control capability
   table, palettes and the shared-memory layout live there only.
2. **Only classic dialogs are legacy candidates**, in every host. Explorer, Control Panel,
   MMC main frames and any dialog containing a shell/foreign-engine surface are skipped.
   Titles are never consulted. (Regression history: Control Panel toolbar/address band.)
3. **Hook v2** observes with `WH_CALLWNDPROCRET`, keeps per-window entries (owner thread,
   original proc/colors), restores only on the owning thread, falls back to pass-through when it
   cannot unlink, and pins itself in the target. "Styling off" != "module unloaded".
4. **Architecture routing** is `target::classify_target` + `target::route`. ARM64EC is detected
   through the PE machine type and refused. Emulated x64 goes to a helper (see M3).
5. **Rules** (`src/rules.rs`): priority, then specificity, then file order. `resolve()` returns
   provenance for every field. Titles only match when the caller supplied one.
6. **Config schema 2**: `schemaVersion`, rule matchers, `focus`, `reduceEffects`, `legacy`.
   Older files load unchanged; the new version is written on next save.

## Added in 3.6 (read `EVIDENCE.md` first)

7. **Trial = a pure state machine** (`src/trial.rs`). The host owns one `TrialMachine`; every
   path that changes the permanent config calls `end_trial_for_external_change()` first. Keep is
   written by Settings *before* the host is told.
8. **Window identity = fingerprint** (`src/identity.rs`, `inspect::fingerprint_of`). Compared
   before apply/restore; snapshots carry it (optional field, 3.5 files still load).
9. **Helper protocol and backoff are shared** (`src/helper_protocol.rs`, compiled into the helper
   too). The controller (`platform/helper.rs`) has a bounded queue, 3 s reply wait on its worker
   thread, a generation counter, and `set_enabled(false)` => `stop_and_quit()`.
10. **Safe start** (`src/startup.rs`, `session.json`): two unclean ends, Shift at launch or
    `--safe` => experimental features and automatic extras are off for that run, in memory only.
11. **Scenes / Look Book** (`src/scenes.rs`, `lookbook.json`): a scene is a bundled recipe plus an
    optional Focus Look. Applying touches only the default look and Focus Look.
12. **Features** (`config.features`, all off by default, each independent): `identityAccents`
    (`accents.rs`, `accents.json`), `dayNight` (`schedule.rs`; one timer for the next boundary,
    re-checked on resume/time change), `paletteHotkey` (`RegisterHotKey`, no keyboard hook).
13. **Settings pages** (preview.2): Home, Fix this app, Appearance, Scenes, Rules, Safety, Diagnostics, Settings & help, in a left pane. Run
    `python3 scripts/check-controls.py` after any page change; it fails if a control is on no page.
14. **Command palette** (`src/commands.rs` + `platform/cmdpal.rs`, a child module of `settings`).

## Added in preview.2 (the UI pass; read `UI-GUIDELINES.md` first)

15. **Design system**: `src/uitheme.rs` (pure: palettes, type ramp, metrics, contrast math, tests),
    `platform/uistyle.rs` (fonts, icon font, thread-local theme `th()`), `platform/controls.rs`
    (one button, one focus ring, pointer hover/pressed registry). Pages never pick a color or draw a
    button themselves. `config.ui.appearance` (System/Light/Dark) is AllBlack11's own and is
    separate from every look it applies.
16. **Row model with measured heights**: a page is a `Vec<Laid>` of `Row`s. Heights come from
    `row_height`, which measures wrapped text with `DrawText(DT_CALCRECT)` inside a
    `MeasureScope` that holds the fonts, scale and content width. Painting uses the same
    `*_text_w` and `*_heights` functions; if you change one side, change the other. A new text row
    needs: a `Row` variant, `row_height`, `paint_row`, and the UI Automation arm in `uia_ui.rs`.
17. **Onboarding** (`src/onboarding.rs` pure state machine, `onboarding.json`; `platform/setup_ui.rs`
    screens): `fresh_install` is "no config file when this version first ran"; an upgrade is marked
    `existingUser` and never shown setup. **Help and walkthroughs** (`src/help.rs`, pure): steps
    name a real page and control, adapt to `Capabilities`, and are shown as an inline tip.
18. **UI Automation** (`platform/uia.rs` COM, `platform/uia_ui.rs` tree from the same rows):
    marshaled to the UI thread with a window message; events only when a client is listening;
    `ALLBLACK11_NO_UIA=1` disables it; `ALLBLACK11_UIA_SELFTEST=1` runs an in-process vtable
    self-test and writes a report (used under Wine).
19. **Trial bar** is part of the Tab ring and of the UI Automation tree (`ext.trial_focus`,
    `trial_message`). Enter or Space during a trial moves focus to Keep.
20. **Wine harness** (`scripts/ui-shot.sh`, `scripts/ui-drive.sh`): runs the x64 exe under Wine +
    Xvfb and takes screenshots / sends clicks and keys. It is for layout only; Wine is not Windows.
    `AB_W`/`AB_H` set the window size; the registry value `TextScaleFactor` simulates text size.

## Added in 3.7 (read `INVENTORY-3.7.md` and `BRAND.md` first)

21. **Brand pipeline**: `assets/brand/brand_icon.py` is the single source (geometry to SVG, ICOs, PNGs, mono tray marks; `check` runs in CI). Resources 1 and 10 to 13 in `assets/allblack11.rc`. `icon.rs` loads them; the tray picks white or near-black from `SystemUsesLightTheme` and reloads on `ImmersiveColorSet`. Do not edit the `.ico` files by hand: change the generator and run `build`.
22. **Credit**: `brand.rs` (pure, tested) holds the sentence, the URL and the accessible name; `Row::Credit` draws it, `registry::open_url` opens it after re-checking the URL. A new text row needs a `Row` variant, `row_height`, `paint_row` and a `uia_ui.rs` arm.
23. **Last known good** is a *candidate* (`lkg::Promotion`) offered on parse and promoted after 60 s or a clean exit after 10 s, only from an eligible run (`App::lkg_eligible`). Do not call `cfgio::save_lkg` anywhere else except `promote_lkg`.
24. **`write_atomic`** lives in `atomic.rs` (pure, flushes before rename); `platform::wide::write_atomic` forwards to it.
25. **Look history** (`history.rs`, `SESSION`): the host records in `apply_external_config` and, for kept trials, in `trial_end` (`history_kept` stops the Keep reload recording twice). Undo is in `settings_pages.rs::undo_look`.
26. **Playground and Contrast Lab** are pure (`playground.rs`, `contrast_lab.rs`) and shown as rows; `policy::exclusion_for` is tested against `decide`.
27. **CI** has `windows-arm64` and `windows-x64`; the build scripts try `llvm-rc`/`llvm-cvtres` then MSVC `rc`/`cvtres`.

## Next batch (see the end of `INVENTORY-3.7.md`)

Run section 3.7 of the smoke test first. Then Q01 (edit box names) on Windows, the exclusion manager UI, helper counters and the styling timeline, then Recipe Remixer and palette locking.

## UI deferred or open (see `UI-GUIDELINES.md` > Deviations and Known gaps)

- Edit boxes have no programmatic label; rows scrolled out of view are not marked off-screen.
- Setup / uninstall (`setup/`) is not on the design system. System message boxes are unchanged.
- `PAGE_MARGIN` (24) and the drawn margin (20) disagree.
- Not run on Windows at all: Narrator, high contrast, multi-monitor DPI, hover/pressed feel.

## Deferred in 3.6 (not stubs: nothing for them is in the UI or the code)

- Window Anatomy view (needs a non-activating, input-transparent overlay; risky without hardware).
- Per-window undo timeline (the single-level Restore exists; a timeline with "previous
  AllBlack11 look" vs "original app state" needs per-window history).
- Display-aware scenes (stable monitor identity, connect/disconnect handling).
- Controller-friendly Living Room *view* (the Living Room *scene* exists).
- Recipe Remixer.
- Saved Look Book thumbnails and capturing another app's window for comparison. Before / after
  uses two windows AllBlack11 owns instead.

## Milestones

See the task list in the PR description / `docs/dev/EVIDENCE.md`. Update the checklist there.

## Next steps (edit as you go)

- Everything is implemented and cross-compiled. Nothing 3.5 or 3.6 has run on Windows.
- Run the ordered pass at the top of `docs/dev/SMOKE-TEST.md` on ARM64 (native and the x64 build
  under emulation) and native x64.
- Fix what it finds, update `EVIDENCE.md`, then decide whether 3.5 can leave pre-release.
- Known gaps: UI Automation has never been exercised by a real client; whole-window opacity not offered.
- Run section **U** of `SMOKE-TEST.md` first (25 minutes), then the 3.6 pass.
