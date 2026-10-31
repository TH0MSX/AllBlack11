# Evidence for 3.5.0-preview.1

What was actually done to each feature. Categories, strongest last:

- **Static review**: read and reasoned about, nothing executed.
- **Unit-tested**: pure logic covered by `cargo test --lib` (110 tests, run on Linux).
- **Cross-compiled**: type-checked and linked for ARM64 and x64 from Linux (`./build.sh`). This
  proves it builds. It proves nothing about behavior.
- **Runtime ARM64**: run on a Snapdragon Windows 11 ARM64 laptop (the owner's).
- **Runtime x64-emulated**: an x64 build run on that laptop under emulation.
- **Runtime native x64**: run on a real x64 PC.
- **Not tested.**

**No 3.5 feature has been run on Windows yet.** The environment this was written in has no
Windows machine. "Runtime ARM64" below means only what the owner ran from the 3.0 Preview.
Every row that says "Not tested at runtime" needs the smoke test in `SMOKE-TEST.md`.

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

## Known gaps, stated plainly

- Screen readers: the Settings window is custom drawn. Native edit boxes are accessible; the
  cards, switches and lists are not exposed to UI Automation. Keyboard access exists
  (Tab, arrows, Space/Enter, Ctrl+Tab, Esc). This is a real accessibility gap.
- DPI: layout is logical-pixel based and scales per monitor. 100–200% has not been looked at.
- Whole-window opacity is not offered: changing another app's window style is invasive, and
  the tint-opacity slider is clearly labeled as material tint only.
- The x64 helper has never been started. Treat it as unproven until `SMOKE-TEST.md` section 5
  passes on ARM64 hardware.
