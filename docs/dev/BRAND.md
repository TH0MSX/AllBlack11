# AllBlack11 brand: icon, palette, credit

Design direction: **moonlit precision**. Nocturnal, precise and quietly powerful; OLED-friendly
without losing readability; native to Windows but recognizably its own. Fluent supplies the
interaction language, AllBlack11 supplies the personality. There are no star fields, animated
moons, neon or gradients in the app; the only decoration is the mark, an accent line and one
credit line, and all of it gives way to a contrast theme.

## The icon

**Concept: a silver moon eclipsed by a window.** A disc with a rounded-square bite taken out of
its upper right, and a thin cool-blue window frame sitting in the bite. Two metaphors only (moon,
window); straight-on; no lettering; no tiny details. The window motif is an original rounded
square, not the Windows logo (the previous glossy four-pane icon looked too much like it and is
gone).

How it was chosen (all of this is reproducible: `python3 assets/brand/brand_icon.py concepts`):

| Concept | Idea | Verdict at 16 to 48 px |
| --- | --- | --- |
| A | Moon bitten by a window, window outline poking past the moon | Reads, but the frame looks like a notification badge |
| B | Eclipse inside a silver window frame | Muddy at 16 px (looks like a lens) |
| C | Window with title band and a crescent | Clear, but the band reads as a calendar icon |
| D | Moon with a window-shaped bite, no frame | Strongest silhouette, but the window idea is too faint |
| E | The plate is the window, crescent below the title band | Clear, but generic (reads as "night mode") |
| F | D plus an accent rim along the bite | Elegant, window implied only |
| **G** | **D plus a small window frame inside the bite** | **Chosen: moon and window both read at every size** |

The sources of A to F stay in `brand_icon.py` as the record of the comparison.

Palette (final values come from contrast testing, `brand_icon.py check`, also run in CI):

| Role | Value | Measured |
| --- | --- | --- |
| Plate top / bottom | `#1D222B` / `#07080B` | |
| Moon silver | `#F4F6FA` to `#A9B3C2` | darkest silver on the plate 7.5:1 |
| Window accent | `#5BB4F0` | on the plate 7.0:1 |
| Plate against a light desktop (`#F3F3F3`) | | 16.5:1 |
| Plate rim `#3A414D` against a dark desktop (`#202020`) | | **1.6:1**: the plate edge is faint on a dark desktop; the moon and the frame carry recognition there |
| White tray mark on a dark taskbar | | 16.3:1 |
| Near-black tray mark (`#141414`) on a light taskbar | | 16.6:1 |

Pixel tuning: the window frame snaps to whole device pixels at 16 to 32 px, the frame is never
thinner than one pixel, and the plate rim is only drawn from 20 px up.

## Assets and where they are used

| File | What | Used by |
| --- | --- | --- |
| `assets/brand/brand_icon.py` | Geometry, renderer, SVG writer, checks. Needs only numpy and Pillow | the build step below |
| `assets/brand/allblack11-icon.svg` | Editable vector source of the app icon (the same layers as the rasters) | designers, docs |
| `assets/brand/allblack11-mark-mono.svg` | The mono mark | designers |
| `assets/allblack11.ico` | App icon: 16, 20, 24, 32, 40, 48, 64, 96, 128, 256, each size rendered separately | resource 1: exe icon, Settings window class (big and small), taskbar, Alt+Tab, installer window and the installer header, shortcut (`IconLocation`), Apps and Features (`DisplayIcon`), harness |
| `assets/brand/tray-white-on.ico`, `-off.ico` | White mark, filled moon (styling on) and outline (off or paused) | resources 10 and 11: tray on a dark taskbar |
| `assets/brand/tray-black-on.ico`, `-off.ico` | Near-black mark, same two states | resources 12 and 13: tray on a light taskbar |
| `assets/brand/png/` | PNGs of the app icon (16 to 512) and the mono marks | docs, store listings |
| `docs/allblack11.png` | The 512 px icon | docs |

The tray icon follows the *system* theme (`SystemUsesLightTheme`) or, under a contrast theme, the
window color, and is reloaded when Windows signals a theme change. State is a shape difference
(filled or outline), never color alone. The same mono marks draw the in-app navigation mark.

Regenerate everything: `python3 assets/brand/brand_icon.py build` (then commit the outputs).
Check without writing: `python3 assets/brand/brand_icon.py check`. There is no packaging
manifest (the installer is the project's own Setup), so no extra PNG sets are needed for one.

## Where the brand shows up in the app

- **Home**: a brand header (the 56 px color icon, "AllBlack11" in the Title style, one plain line)
  and the creator credit at the foot of the page. Also the setup welcome step.
- **Navigation pane**: the mono mark beside the name, or alone on the icon rail.
- **Tokens**: `Palette::brand` (a cool blue, darker in Light, the system hot-light color in high
  contrast), contrast-tested at 4.5:1 on every surface it sits on. There is no second theme layer.
- **About**: the verification status line and the credit.
- **Installer**: the header carries the mark and the brand accent line (the rest of the installer
  is unchanged).
- **High contrast overrides decoration**: no icon on Home or in the pane, system colors only.
- Operational icons stay conventional (Segoe Fluent Icons for navigation); none was replaced by a
  moon.
- Scene choice cannot change AllBlack11's own theme: that is the separate System/Light/Dark choice.

## Creator credit

"Made with love by TH0MSX", where **TH0MSX** is a link to `https://github.com/TH0MSX`.

- Text and URL live in `src/brand.rs` and are unit-tested: exact sentence, exact URL, no query,
  fragment or credentials, `https://github.com/` only (`is_safe_profile_url`, checked again
  immediately before opening). Nothing opens automatically; only a click, Enter or Space.
- Placement: a footer row on Home, and again under About on Settings & help. It is a row in the
  page, not a status bar, so it never overlaps the trial bar or a status message; on a narrow page
  the link drops to its own line.
- Look: 14 px text in the muted color (4.5:1 or better), the link in the brand color (4.5:1 or
  better) with an underline so it is not color alone, a visible focus ring, hover lightening.
- Accessibility: one Tab stop; UI Automation exposes a text element and a Link element named
  "TH0MSX - creator's GitHub profile" with the address as its value and an Invoke pattern. The
  plain text is complete without any symbol (no heart glyph is used).
- Separate from the license and third-party notices, which are untouched.

## Not verified (no Windows hardware in this session)

- How the icon looks in the taskbar, Start, Alt+Tab, the shortcut, Apps and Features and the
  installer at 100% to 200%; whether Windows picks the tuned small sizes.
- The tray icon swapping when Windows changes between light and dark, and under contrast themes.
- The credit link opening the default browser; focus ring and Narrator wording.
- No `AppUserModelID` is set, so Windows groups the taskbar button by executable path. Setting one
  needs a matching shortcut property and was left until it can be tested.
- `docs/banner.png` and `docs/screenshots/*` still show the previous icon and UI.
