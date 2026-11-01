# Inventory for 3.8.0-preview.1

Branch `claude/next-3.8`, on top of the released `v3.7.0-preview.1`. Status words: **done** (code,
tests, UI), **partial** (what exists is said), **deferred** (not started, reason given). "Windows
verified" means someone ran it on Windows; **no item below is Windows verified.**

## A. In-app updater: done in code

| Requirement | Status | Where |
| --- | --- | --- |
| GitHub releases API over HTTPS, repo named in one place | done | `update.rs` (`REPO_OWNER`, `REPO_NAME`), `platform/http.rs` (WinHTTP, TLS 1.2+, https-only redirects, host allowlist) |
| Respect pre-release flag and a documented policy; stable vs preview; Settings choice | done | `update.rs` `Channel`, `select`; `docs/dev/RELEASE-POLICY.md`; Updates card |
| No network, rate limits, malformed JSON, missing assets, clock skew: fail quiet, manual "Check again" | done | `update.rs` (`describe_*`, `check_due`), tests for each |
| Exact-arch asset, never guess | done | `Arch`, `installer_name`; no match is "no installer for this PC" |
| Integrity: published digest or notes line, verified while streaming, shown to the user | done | `sha256.rs`, `download.rs`, `Integrity`; signing path in the policy |
| Stream with progress and cancel, bounded size, temp file, never replace the running exe | done | `download.rs` (`.part`, 96 MiB cap, cancel), `platform/updater.rs` |
| Hand off to the installer after explicit consent, then exit | done | `updater::install` re-hashes, then Setup; AllBlack11 closes |
| State persisted; stale downloads cleaned | done | `update-state.json`, `stale_files` |
| Non-intrusive UX; current/available, kind, notes excerpt, release link; Skip this version | done | Updates card, Home badge, one balloon per release |
| Updater failures never affect styling, recovery, uninstall | done by construction | separate worker thread and module; no shared state with the style path |
| Tests: parsing, digest pass and fail, cancel mid-download, disk full, stale cleanup | done | 45 tests in `update.rs`, `download.rs`, `sha256.rs` |
| **The release-flag fix** | **documented, owner action** | 3.7.0-preview.1 was published without "pre-release"; the policy says tick it. The updater treats a suffixed tag as a preview anyway, so it behaves correctly either way |
| Windows verification | **not done** | `SMOKE-TEST.md` 3.8 A |

**Policy decision:** a version with a suffix (`-preview.N`) is a preview and must be published as a
GitHub pre-release; the updater treats a release as a preview if either the flag or the tag says so.
Channels: Automatic, Stable only, Include previews. The daily check is off by default.

## B. Compatibility packs: done in code; behavior on Windows unknown

| Requirement | Status | Where |
| --- | --- | --- |
| Declarative, validated, bounded, no executable payload | done | `packs.rs` (`deny_unknown_fields`, recipe look allow-list, 24 KB, text limits) |
| Target identity (package and/or program, class), actions, version, explanations, limits | done | `Pack`, `Target` |
| Microsoft Store and Calculator packs, window level only, required wording | done | built in; verdict "partly supported" |
| "Unsupported with the observed reason" instead of forcing | done | `Verdict::Unsupported` cannot carry a look and cannot install |
| Page with installed packs, source, scope, actions, unavailable reasons; install, import, remove; history and Undo; user rules win | done | **a section on Rules, not a separate page** (deviation, to keep the navigation at nine) |
| Hosted-app identity for `ApplicationFrameHost` frames | done in code, **unverified** | `procinfo::hosted_app_pid` finds the `Windows.UI.Core.CoreWindow` child |
| Do caption and border colors actually apply to the Store and Calculator windows? | **unknown** | step 13 and 15 of the smoke test decide whether the packs become "Not supported" |

## C. Selected deferred features

| Item | Status | Notes |
| --- | --- | --- |
| C1 Window Anatomy (F01) | **deferred** | still no safe Wine-testable path; planned as an owned preview first |
| C2 Display-aware scenes (F03) | **deferred** | needs stable monitor identity and hot-plug handling |
| C3 Recipe Remixer (F05) | **deferred** | |
| C4 Exclusion manager (F18) | **done in code** | `exclusions.rs`; section on Rules; `class=Name` adds a window class; precedence text is tied to `policy::exclusion_for` by a test |
| C5 Wizards W01, W02 | **deferred** | W01 needs F16/F17; W02 needs palette locking |
| C6 UI Automation labels for native edit boxes (Q01) | **done in code, unverified** | `labels.rs`, `platform/accname.rs`; vtable checked against the SDK IDL; needs Narrator/NVDA/Accessibility Insights |

Order of work: C6 and C4 first (the 3.7 inventory ranked them highest and both are testable without
new platform surface), then the wallpaper gallery; C1, C2, C3 and C5 were not started.

## D. Wallpaper gallery: done in code; Windows service calls unverified

| Requirement | Status | Where |
| --- | --- | --- |
| Eight original code-generated OLED wallpapers; no stock or scraped art | done | `assets/wallpapers/generate.py` |
| Rules: true black foundation, no large bright fields, restrained brand accents | done, **checked on every run** and recorded in the manifest | `check_oled`; unit test on the recorded stats |
| Reproducible, sources committed, 3840x2160 | done | `python3 assets/wallpapers/generate.py --check` (about 70 s); a 2560x1440 set was not needed |
| Repo lean; assets on the release, downloaded with integrity and consent | done | PNGs (5.4 MB) are not in git; previews (3 to 13 KB) are; SHA-256 compiled into the exe |
| Gallery page: grid previews, name and description, full-screen preview, apply via `IDesktopWallpaper` to a chosen display, remember last applied, open file location, keyboard accessible, size shown before downloads | done | `platform/gallery.rs`, `platform/wallpreview.rs`, Wallpapers page, UIA list items |
| Fill/fit per wallpaper | done | remembered per wallpaper |
| Fill/fit **per monitor** | **not possible as asked** | Windows keeps one position for all displays; the page says so. A per-display fit would need AllBlack11 to render a separate image per display; not done |
| Wallpaper changes in Look history | **no** | they are not config changes; **Put back previous** is the undo, and the page says so |
| Windows verification | **not done** | `SMOKE-TEST.md` 3.8 D |

## Still deferred from 3.7 (unchanged)

Living Room, scene layers, palette locking and boards, variants, workspaces, shared profiles,
version guards, drift review, control overrides, state playground, screenshot sessions, contact
sheets, lineage, bundles, styling timeline, helper counters, performance monitor, conflict
troubleshooter, adapter test mode, CLI, the remaining wizards.

## Recommended next batch

1. Hardware pass of 3.8 (`SMOKE-TEST.md` 3.8), starting with step 13 (do caption colors apply to the
   Store window?) and step 20 (are the text boxes named?). Those two decide whether two features
   keep their wording.
2. Publish 3.8.0-preview.1 **as a pre-release** and attach the eight wallpaper PNGs.
3. Then F05 Recipe Remixer and palette locking (pure color math with the contrast lab as its safety
   check), then F03 with real monitor identity, then F01 on the owned preview.
