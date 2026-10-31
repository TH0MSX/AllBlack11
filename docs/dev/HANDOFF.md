# AllBlack11 3.5 development handoff

Keep this file current. It exists so the work survives a context reset.

## State

- Baseline: `7b65598` (3.0.0-preview.2 plus two docs/banner merges). Tag `v3.0.0-preview.2`.
- Branch: `claude/peaceful-archimedes-ppfhwb`. Target version: `3.5.0-preview.1` (prerelease only).
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

## Milestones

See the task list in the PR description / `docs/dev/EVIDENCE.md`. Update the checklist there.

## Next steps (edit as you go)

- M2: explain engine, window picker/inspector, Effects Studio pages in Settings.
- M3: x64 helper (`helper/` crate), pipe IPC, packaging in the ARM64 zip.
- M4: trial preview, last-known-good, recipes, compatibility notebook, focus look, ownership-aware restore.
- M5: version bump, docs, smoke checklist, release as prerelease only.
