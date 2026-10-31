# AllBlack11 development handoff (3.6 preview)

Keep this file current. It exists so the work survives a context reset.

## State

- Baseline: `7b65598` (3.0.0-preview.2 plus two docs/banner merges). Tag `v3.0.0-preview.2`.
- Crates: root exe, `hook/` (DLL, built for both CPUs), `setup/` (installer), `harness/` (test dialog, two manifests), `helper/` (x64-only helper shipped in the ARM64 package).
- Branches: `claude/peaceful-archimedes-ppfhwb` is the **3.5.0-preview.1 checkpoint** (head `7372140`, untouched). `claude/next-preview-3.6` continues from it: **3.6.0-preview.1** (prerelease only; nothing merged, tagged or published).
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
13. **Settings pages**: Overview, Effects, Apps, Scenes, Legacy, Safety. Run
    `python3 scripts/check-controls.py` after any page change; it fails if a control is on no page.
14. **Command palette** (`src/commands.rs` + `platform/cmdpal.rs`, a child module of `settings`).

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
- Known gaps: UI Automation for the custom-drawn Settings; whole-window opacity not offered.
