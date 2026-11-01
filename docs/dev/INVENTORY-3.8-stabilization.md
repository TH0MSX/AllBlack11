# Stabilization pass (3.8.0-preview.2): what changed, what moved, what is still unproven

Branch `claude/stabilize-3.8`, from `claude/next-3.8` at `0b275e7` (3.8.0-preview.1), which stays
untouched as the recoverable baseline. Nothing is merged, tagged or published. **This pass added no
feature**: it reorganized what exists, fixed navigation, and corrected claims.

Evidence labels used below: **unit** (Linux, `cargo test --lib`, also run natively on the Windows CI
runners), **static** (`scripts/check-controls.py`), **cross** (type-checked and linked for ARM64 and
x64), **Wine** (the real exe rendered under Wine; not Windows), **Windows: pending** (nobody has run
it on Windows yet).

## 1. Information architecture

Before (3.8.0-preview.1): nine navigation items, the last one a footer:
`Home · Fix this app · Appearance · Scenes · Rules · Safety · Diagnostics · Wallpapers · Settings & help`,
each one a long page (Rules held per-app rules, exclusions, the exclusion manager, packs, the
notebook and the whole classic-dialogs layer; Settings & help held tutorials, theme, startup, the
palette, updates and About).

After: **six categories** in a landscape shell (navigation pane with the search box on the left,
content on the right, 1120x760 logical start size). Three categories are landing pages of cards that
open **subpages**; each subpage starts with a Back link and Alt+Left goes back.

```
Home                      status, Customize an app, Pause/Resume, looks, update badge, credit
Appearance                hub
  Scenes and looks          scenes, Look Book, compare
  Effects and colors        quick looks, window look; advanced options in sections
  Wallpapers                gallery
  Schedule and accents      day/night scenes, identity accents
  Palette and recipes       palette from a picture, recipes, remixer
Apps                      hub
  Customize an app          picker, "what can change", style receipt, notes
  Per-app rules             rules, priority, test without applying
  Compatibility packs       packs (experimental), check, notebook
  Exclusions                never-theme lists, exclusion manager
Safety                    pause, undo and history, restore, last-known-good, experimental stop
  Experimental features     classic dialogs layer, x64 helper, control harness
Settings                  AllBlack11's own theme, startup, shortcuts, updates
Help & About              hub
  Help and tutorials        help search, walkthroughs, run setup again
  Diagnostics and support   this PC, log, report, data folder
  About                     version, CPU, verification status, credits, licenses
```

Progressive disclosure: *Effects and colors* shows the window look first and puts Focus Look,
automatic reductions, title bar height and test windows, the taskbar/Start shell options and the
contrast check in four closed sections; *Customize an app* closes the full inspector report;
*Diagnostics* closes the Windows-build support table. A section opens on click or Enter, and a
search result inside a closed section opens it.

## 2. Controls moved (every control still exists; `check-controls.py` proves each is on a page)

| Control or section | Before | After |
| --- | --- | --- |
| Pick a window, fix card, style receipt, notes | Fix this app | Apps > Customize an app |
| Per-app rules, priority, playground | Rules | Apps > Per-app rules |
| Never theme these apps, exclusion manager, class list | Rules | Apps > Exclusions |
| Compatibility packs, notebook | Rules | Apps > Compatibility packs |
| Classic dialogs (experimental), emulated x64, harness | Rules (bottom) | Safety > Experimental features |
| Quick looks, window look, colors, Focus Look, reductions, title bar height, shell, contrast | Appearance | Appearance > Effects and colors (advanced ones in sections) |
| Scenes, Look Book, compare | Scenes | Appearance > Scenes and looks |
| Palette from a picture, recipes, remixer | Scenes | Appearance > Palette and recipes |
| Day / Night, identity accents | Scenes | Appearance > Schedule and accents |
| Wallpapers | Wallpapers | Appearance > Wallpapers |
| App theme, Start with Windows, palette hotkey, Open command palette, Updates | Settings & help | Settings |
| Help search, walkthroughs, run setup again | Settings & help | Help & About > Help and tutorials |
| About, credit | Settings & help | Help & About > About (credit also on Home) |
| Diagnostics, log, support table | Diagnostics | Help & About > Diagnostics and support |
| Theme windows (master switch), Pause/Resume, update badge | Home | Home (unchanged) |
| "Command palette" and "Explore" buttons on Home | Home | removed (the left pane and the search box cover them); `GoPalette` retired |
| Save/export/import recipe, remix | unchanged behavior | new place only |

Config compatibility: no config key changed; `window.json` (position and size) is a new small file
next to `config.json`. `--open-settings=N` with the old numbers still opens the page it always did;
ids such as `--open-settings=apps/packs` are the new form.

## 3. Navigation registry (item 4 of the brief)

`src/destinations.rs` is the one registry: typed `Dest` ids, category, parent, icon and words for
every place. It replaces the numeric page indices in the palette (`Action::Page(usize)`), Help
walkthrough steps, the first-run setup, the `--open-settings` switch and Settings itself.

The stale mapping that the owner suspected was real: the palette's "Fix an app" command carried
`Page(2)`, which meant Appearance after the pages were reordered, and its test only checked that
numbers 0 to 5 existed. Both are replaced (**unit**):

- every destination is described exactly once, ids are unique and round-trip, the tree is one level
  and every parent is a landing page;
- every destination is found by its own title; the result for "Customize an app" opens
  `AppsFix`;
- the old numeric switch (0 to 8) still resolves to the page it did before;
- walkthrough steps must name a page that has controls (not a hub);
- `rows_for_current_page` matches every `Dest` exhaustively, so a destination without a builder does
  not compile.

## 4. Search (item 3)

One engine: the existing command-palette index (`commands.rs`), extended. The Settings search box
(Ctrl+K moves to it; Enter opens the first result, Down enters the list, Escape clears; a clear
button and Tab) and the palette (tray, hotkey) use the same list and the same scoring.

Indexed (**unit**): all destinations (16), 55 settings entries with exact anchors, every scene and
saved look, the two compatibility packs, the eight wallpapers, every help topic, and the existing
actions. Synonyms are keywords on entries: blur finds Backdrop material, Blur while dragging, Tint
opacity; wallpaper/background finds the gallery and each wallpaper; restore/reset finds Restore last
known good, Reset all windows and Safe mode; updates/version finds Check for updates, the channel
and the daily check; store/calculator finds the packs; black/dark finds the scenes and the dark
title bar. Results show an icon, title, path (`Appearance › Effects and colors`) and a short line.

Selecting a result **navigates only**. An action such as Pause, Reset or Restore opens the page that
holds its button (`palette_destination`, **unit**); it never runs. The page scrolls to the control,
focuses it, and marks it for two and a half seconds with a double outline, a bar and the word
"Found" (not color alone). If the control sits in a closed section the section opens; if it only
exists in some states (the Safe mode heading), the page opens at the top and the status line says so.

Cost: the index (about 110 entries) is built when the box gains focus, not per keystroke; a search
is one pass over it (200 searches in under a second, tested). Local only.

Static proof: `check-controls.py` checks all 55 anchors against the builders of the page each one
names (55 of 55 land). **Not proven**: that the result list reads well with Narrator, and the focus
move on a real Windows PC (**Windows: pending**).

## 5. Icons (item 5)

One family: Segoe Fluent Icons, falling back to Segoe MDL2 Assets, as before; none installed means
text only, never boxes. Icons come from one table (`destinations.rs`) for navigation, hub cards and
results, plus a back arrow and chevrons; each sits beside a text label and the pane/cards have
accessible names. The old `uistyle::glyph` set was removed. An emoji-like warning sign in the color
box message was replaced by words. Left as is: the tray menu's text check mark (U+2713) and the
typographic arrows in change lists (non-blocking; listed below). The AllBlack11 moon/window mark
still leads the pane. Under Wine no icon font exists, so the Wine screenshots show no icons.

## 6. Microsoft Store and Calculator (item 7)

Owner report: both templates changed nothing visible. Findings, from the code and without Windows
runtime evidence:

1. **The pack asked for almost nothing.** Compared with the default look, each pack changes only
   the title bar color `#1A1A1A` to `#000000` and the text `#F0F0F0` to `#FFFFFF` (borders and dark
   mode are already those values). That is a difference of about 10% of one dark gray; "no visible
   change" is the expected result even if every request is accepted. The page now shows this
   comparison (`packs::delta`, **unit**).
2. **The window model is unverified.** Store and Calculator frames are `ApplicationFrameWindow`s
   hosted by `ApplicationFrameHost.exe`; the app's package is read from the `Windows.UI.Core.CoreWindow`
   child (`procinfo::hosted_app_pid`). Whether current versions still use that model, whether the
   child exists when the frame is first styled, and whether DWM honors caption colors on these
   frames are **unknown**; no code here proves them.
3. **Content is the app's.** Neither app lets the frame colors reach its pages, and the packs never
   claimed otherwise; "Make the Store dark" / "Dark Calculator" is not claimed anywhere (a test
   forbids those phrases).

What changed: both packs are now **Experimental (no visible effect shown)**: kept as a diagnostic,
not recommended ("Recommended templates: none yet"); their rule names are unchanged so installed
packs are still found, and nothing a user created is deleted. **Check now** walks the chain on the
windows open right now and says where it stops, with the nine distinguishable statuses
(**unit** for the logic; the enumeration is **Windows: pending**): target not detected, window
excluded, rule did not match, overridden by another rule, not applied right now, Windows rejected a
request (with the HRESULT), accepted by Windows with the visual result unverified, you verified no
visible change, verified visible change (the last two only after you press "I see no change" or "I
see a change"). Instead of promises the page offers the app's own route: a button that opens
Windows' Colors page (`ms-settings:colors`, fixed text) and the note that an app's own theme option
is in its own settings. No injection, patching or undocumented switch was added.

## 7. Window and layout (items 1 and 8)

- Start size 1120x760 logical, scaled to the monitor with the pointer, clamped to its work area with
  a margin; minimum 720x520; content is capped at 960 and left-aligned beside the pane; no
  horizontal scrolling; one scrolling region.
- Position and size are remembered (`window.json`, size in logical pixels) and restored by
  `windowbounds::restore`: on the monitor that still holds most of the window, pulled fully on
  screen, size corrected to the minimum and the screen; a missing monitor or a damaged file gives
  the landscape default. 9 tests (**unit**). The maximized state is remembered. **Windows: pending**:
  a real monitor change.
- Narrow windows keep the pane as an icon rail; the search box then sits on the results page.
- Wine renders at 100%, 150% and 200% scaling (1120x760, 1680x1140, 1900x1300 windows) showed no
  clipping or overlap on Home (**Wine**, not Windows).

## 8. Accessibility (item 8)

Done: native edit boxes labeled (ten, including search); Tab leaves every text box; search box is a
tab stop before the categories; categories, cards, results and sections are UIA buttons/toggles with
names and descriptions; `IsOffscreen` was hard-coded to false and now reports content scrolled out
of the visible page (found by this pass); focus rings; high-contrast and text-scale paths unchanged.
The provider self-test (our own provider, walked through its COM vtables under Wine) passed on the
packs page: 23 of 23 elements named. **That is not Narrator, NVDA or Accessibility Insights.** All of
item 8's "verify" list is **Windows: pending**: names and states as announced, focus order with a
screen reader, high contrast, 125% and 150% on real hardware, multi-monitor DPI changes, long paths.

## 9. Gates

| Gate | Status |
| --- | --- |
| Unit tests | 366 pass (Linux) and on both native Windows CI jobs |
| Every navigation/search destination | **unit** + static (55 anchors against the page builders) |
| Settings persistence and migration | unchanged code; config tests pass; `window.json` is new and tolerant |
| Trial Keep/Revert, timeout; Undo; recovery; helper shutdown | unchanged code, unit-tested pure parts; **Windows: pending** (smoke test section S) |
| Scene and wallpaper actions; updater check and cancel | unchanged code, unit-tested pure parts; **Windows: pending** |
| Resizing, page transitions, tray actions | **Wine** for resize and transitions; tray unchanged; **Windows: pending** |

## 10. Blockers and limits

No known blocker in navigation, recovery or persistence in the code. The release **cannot be called
ready**: the owner's acceptance checklist (`SMOKE-TEST.md`, section S) is unrun, and four earlier
unknowns remain open (the Store/Calculator window model, text-box names with Narrator, the
`IDesktopWallpaper` calls, real GitHub updates).

Non-blocking limits: no per-display wallpaper fit (Windows has one setting); the tray menu uses a
text check mark; the Effects page still pins its live preview strip (about a fifth of a 760-pixel
window) because it is how a look is judged; the narrow rail needs one extra step to search; Ctrl+K in
Settings now focuses search (the palette that runs actions stays on the tray and its hotkey);
search results do not offer actions, by design.
