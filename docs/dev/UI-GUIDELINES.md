# UI guidelines: what AllBlack11's own windows follow, and where they do not

This file maps the major design decisions in AllBlack11's own UI (Settings, setup, Help, the command
palette, the tray menu) to the Microsoft guidance they come from, and records every place they
deviate. It exists so the next person can tell a deliberate choice from an accident.

**What this is not.** It is not a compliance statement. There is no "Windows 12 guideline" and none
is invented here. The Microsoft pages below are the Windows app design guidelines for WinUI/XAML
apps; AllBlack11 is a native Win32 + GDI program, so it can follow their *principles, numbers and
interaction patterns*, and cannot be "compliant" with their controls. Where an exact behavior needs
WinUI (Mica, theme brushes, animations, automation peers, TeachingTip), the gap is listed in
[Deviations](#deviations) and not hidden behind rounded rectangles.

**What was read.** The pages in [Sources](#sources) were fetched and read on 2026-11-01 (the
`https://learn.microsoft.com/en-us/windows/apps/design/` pages are revised often; re-read before
relying on a number). A claim is marked *"not from the pages read"* when AllBlack11 chose a value
and Microsoft's text does not give one. Nothing here was checked against a running Windows.

## Sources

All under `https://learn.microsoft.com/en-us/windows/apps/design/` unless noted.

| Short name | Path | Used for |
| --- | --- | --- |
| Overview | `guidelines-overview` | Starting point; the list of guidance areas below |
| Principles | `design-principles` | Calm, coherent, consistent surfaces; geometry and layering as part of one system |
| Typography | `signature-experiences/typography` | Segoe UI Variable, the type ramp (Small 12/16, Text 14/20, Text semibold 14/20, Display semibold 20/28 and 28/36), epx units |
| Geometry | `signature-experiences/geometry` | Corner radius: 8 px for top-level containers, flyouts, dialogs and overlays; 4 px for in-page elements such as buttons and list backplates; 4 px for tooltips |
| Color | `signature-experiences/color` | Light and dark themes; accent used to emphasize important elements; darker colors mean less important background surfaces |
| Layering | `signature-experiences/layering` | Surfaces step up in importance |
| Materials | `signature-experiences/materials` | Mica and Acrylic are the system materials (occluding); neither is used here (see deviations) |
| Motion | `signature-experiences/motion` | Motion is reactive, direct, context appropriate; avoid custom animation where possible |
| Iconography | `signature-experiences/iconography` | Segoe Fluent Icons is the Windows 11 system icon font, monoline |
| Segoe Fluent Icons | `style/segoe-fluent-icons-font` | Code points and names (every glyph AllBlack11 draws was checked against this list) and the note that Segoe MDL2 Assets remains the Windows 10 font |
| Layout | `layout/` and `layout/screen-sizes-and-breakpoints-for-responsive-design` | Sizes, margins and positions in multiples of 4 epx; breakpoints Small ≤ 640, Medium 641 to 1007, Large ≥ 1008 |
| Navigation basics | `basics/navigation-basics` | Navigation as a small set of task destinations |
| NavigationView | `controls/navigationview` | Left pane; expanded, icon-only compact and menu-only minimal modes; optional settings item; adaptive thresholds 1008 / 641 to 1007 / 640 |
| Content basics | `basics/content-basics` | Content layout and alignment |
| Buttons | `controls/buttons` | Button text length guidance (26 characters before wrapping), single-button alignment |
| Toggles | `controls/toggles` | Label a toggle with one or two words describing what it controls |
| Text box | `controls/text-box` | Labels, validation next to the field |
| Tooltips | `controls/tooltips` | Tooltips appear on focus as well as hover; text must be supplemental |
| Dialogs and flyouts | `controls/dialogs-and-flyouts/dialogs` | Say the blocking question simply; buttons that map to the choices |
| Teaching tip | `controls/dialogs-and-flyouts/teaching-tip` | When to teach in context: new or important features, light-dismiss or explicit close |
| Accessibility | `accessibility/accessibility` | Accessible name is required on every interactive element; UI Automation as the foundation; work with text and display scaling |
| Accessibility checklist | `accessibility/accessibility-checklist` | Names and descriptions, labels as LabeledBy targets |
| Accessible text | `accessibility/accessible-text-requirements` | 4.5:1 text contrast (WCAG technique G18); "do not treat high contrast mode as the primary mitigation for low readability"; text scales with the user's text size |
| Keyboard | `accessibility/keyboard-accessibility` | Focusable and reachable by Tab; actionable things are tab stops, static content is not; composite controls manage inner focus with arrows; F6 moves between regions and Shift+F6 reverses |
| High contrast | `accessibility/high-contrast-themes` | Use the system colors in contrast themes; they are typically 7:1 or better |
| Writing | `style/writing-style` | Plain, short, active voice; capitalize the start of every sentence, label and title |
| Inclusive software | `accessibility/designing-inclusive-software` | Background for the pass; no number taken from it |

Win32 documentation consulted for the accessibility implementation (not design guidance): UI
Automation provider interfaces (`IRawElementProviderSimple`, `IRawElementProviderFragment`,
`IRawElementProviderFragmentRoot`, the Invoke/Toggle/SelectionItem/Value pattern providers). IIDs
were taken from Wine's `uiautomationcore.idl` and checked in a self-test (see EVIDENCE.md).

## Decisions

### Typography

- **Faces.** `Segoe UI Variable Small` / `Text` / `Display` by family name, falling back to
  `Segoe UI`. The page describes one variable font with a weight and an optical-size axis; GDI
  selects the static optical-size instances by name, so the optical-size axis is not driven
  continuously (deviation). Fallback is tested for the case where a face is missing
  (`uitheme::face_candidates`, `uistyle::first_installed`).
- **Ramp.** Caption 12/16, Body 14/20, Body strong 14/20 semibold, Subtitle 20/28, Title 28/36.
  These are five entries from the page's table (`uitheme` test:
  `the_type_ramp_matches_microsofts_numbers_and_never_goes_below_the_minimums`). The 18/24, 40/52
  and 68/92 entries are not used: nothing in a utility window needs them.
- **Roles.** Page title = Title. Section heading = Body strong. Card title = Body. Description and
  helper text = Caption, in the muted color, which is verified at 4.5:1 on every surface it sits on
  (no "tiny gray text": the smallest text is 12 px, and it is never lower contrast than 4.5:1).
- **Sentence case** for every label and title (Writing).

### Spacing, layout and DPI

- 4 px unit; card padding 16; gap between cards 4; page margin 20 (Layout: multiples of 4 epx).
  (`uitheme::metrics::PAGE_MARGIN` says 24 and Settings uses 20: a leftover, tracked in Known
  gaps.)
- Everything is laid out in logical pixels and multiplied by one scale factor (display DPI ×
  Windows *Text size*; see Deviations). Row heights are **measured** with the same font, width and
  flags used to paint (`settings.rs`, `wrapped_height`), at the current content width, so a card is
  exactly as tall as its text for any window width, text size or translation. Before this pass they
  were character-count estimates.
- Content stops growing at 880 logical px (product choice; the pages read give no line length).
- The window has a minimum track size derived from the layout, so content never needs to scroll
  horizontally. Vertical overflow scrolls with a visible scrollbar and keyboard focus scrolls the
  focused row into view.
- Titles are single-line and end in an ellipsis if they cannot fit. Descriptions, status lines and
  notes wrap. The full text is always available to UI Automation.

### Geometry and surfaces

- 4 px radius for in-page controls and cards, 8 px for overlays (the command palette gets 8 px
  from DWM; the teaching tip card uses 8 px) (Geometry).
- Surface roles (`uitheme::Palette`): page, navigation pane, card, hovered/selected card, edge,
  control edge. Dark surfaces step *up* in lightness as they become more important (Color,
  Layering).
- **No shadows, no gradients, no blur, no transparency.** Hierarchy comes from surface lightness,
  a hairline edge and spacing.

### Color and the accent

- Light and dark palettes plus a high-contrast palette built from the system colors only.
- The accent is the user's Windows accent, adjusted by `accent_on` until it has 3:1 against the
  page and against a card; text on an accent fill is black or white, whichever has more contrast.
  Accent is used for: the primary button, the selected navigation indicator, switches that are on,
  the focus ring on those, the teaching-tip outline. Nothing else is tinted.
- Status colors (ok, warning, error) are always paired with a word ("Customization is paused",
  "could not save: …"); there is no status carried by color alone.
- **Contrast**: text 4.5:1 (Accessible text, WCAG G18); the boundary of anything you operate 3:1
  (WCAG 2.1 success criterion 1.4.11, *not from the pages read*); both are unit-tested for every
  palette pair (`uitheme` tests).
- **AllBlack11's appearance is separate from the appearance it applies.** `ui.appearance` (System,
  Light, Dark) changes only AllBlack11's own windows and the tray menu. It is stored in its own
  `ui` section of `config.json` and written only when it differs from the default. The looks, rules
  and recipes it applies to other apps never read it.

### Navigation

- Eight task-focused destinations in a left pane (NavigationView pattern): **Home**, **Fix this
  app**, **Appearance**, **Scenes**, **Rules**, **Safety**, **Diagnostics**, with **Settings & help**
  as the footer item. Rules and compatibility share a page; recovery is its own destination so it
  is one click from anywhere. No page exists for a deferred feature.
- The pane has three forms: expanded labels (216), icon-only rail (48) with system tooltips, and a
  text-only form (168) when no icon font is installed, so an unlabeled rail is never shown.
- Progressive disclosure: Home and Fix this app are plain-language; HRESULTs, hooks, pipes and
  capability tables are on Diagnostics and in the "Full inspector report" row, never the default.
- Home states: whether customization is on, paused, off or in safe mode; the current look; which
  experimental features are on; the primary next action (Fix an app); and Pause or Resume as the
  second button of the same row. Pause is also on Safety and in the tray menu and command palette.
- Keyboard: Tab and Shift+Tab walk one ring (pane, the page's controls in reading order, then
  **Keep and Revert while a trial runs**, then back to the pane); Ctrl+Tab and Ctrl+Shift+Tab change
  page; F6 / Shift+F6 move between the pane and the content (Keyboard); arrows move inside the pane
  and inside rows of buttons and lists; Left and Right change a focused slider; Enter and Space
  activate; Escape reverts a running trial, and otherwise closes Settings; Ctrl+K opens the command
  palette. A trial that starts from the keyboard puts focus on Keep.

### Controls

- Edit boxes are native `EDIT` controls (themed with `SetWindowTheme`), so text entry, IME,
  selection, clipboard and caret come from Windows.
- Buttons, switches, sliders, chevrons, cards, list rows and the navigation pane are drawn by
  `platform/controls.rs` and `settings.rs`. One button routine draws every button in every state
  (rest, hover, pressed, focused, disabled, primary). Hover and pressed come from one pointer
  registry (`controls::Pointer`), so hit regions and painted regions are the same rectangles.
- One focus ring routine, used for every focusable thing, in the focus color (3:1 against every
  surface); in high contrast it is the system hot-light color.
- **Why custom drawing at all:** the pre-existing Settings window was already custom drawn and
  adding a framework (WinUI, Electron, webview) was ruled out. Moving every control to Win32
  common controls would give accessibility for free but not the Windows 11 appearance (the classic
  controls are not Fluent-styled without theme hooks). The cost of custom drawing is accessibility
  work, which is done through UI Automation (below).

### Icons

- Navigation only, from **Segoe Fluent Icons** (Segoe MDL2 Assets as the Windows 10 fallback). It is
  a system font: AllBlack11 ships no icon asset, so there is no icon license to track. If neither
  font is installed the pane shows text labels. Every code point was checked against Microsoft's
  list: Home E80F, Search E721, Color E790, Photo E91B, Tag E8EC, Shield EA18, Info E946, Settings
  E713.
- Nothing is icon-only except the compact rail, and each rail item has a tooltip and a UI Automation
  name.

### Motion and materials

- **There is no animation.** No transitions, no progress spinners, no pulsing. Timers exist only to
  do work: the trial countdown (a number, repainted four times a second while a trial runs), a short
  re-read of the inspector report after a change, the day/night scheduler and the tray's
  double-click timer. Nothing moves on its own, so "reduce motion" is satisfied by construction; the
  Windows animation setting is not read (`SPI_GETCLIENTAREAANIMATION` is not queried) because there
  is nothing for it to turn off.
- **No transparency in AllBlack11's own UI**, so the Windows *Transparency effects* setting has
  nothing to turn off. (The reduce-effects feature in Settings is something else: it governs the
  backdrops AllBlack11 *applies to other apps' windows*.)
- Every surface is opaque. The Materials page's Mica/Acrylic are not used in the UI.

### Writing

- Short sentences, active voice, sentence-case labels (Writing). Errors say what happened and what
  is unchanged ("Could not save: … Your settings were not changed").
- No word claims more than was verified: "accepted" is never "confirmed", "paused" is not "off".
  `fixapp.rs` carries a test that bans overstated phrases.

### Guided setup and tutorials (Teaching tip, Dialogs)

- Setup is seven steps: Welcome, Choose a starting look, Try it safely, Pick an app, Keep or
  revert, Preferences, Ready. Back, Next and Skip are on every step; closing Settings keeps the
  step in `onboarding.json`; it is `completed` only after Ready or an explicit dismissal.
  Upgraders (an existing config with no onboarding file) are marked completed without being shown
  anything. Nothing is enabled silently: startup, hooks, helper and automation are choices on step
  6 and default to off. Setup is replayable from Settings & help.
- Walkthroughs use the **real controls on the real page**. A step is an inline card at the top of
  the page with Next tip / Close tip, and an accent outline around the control it is about. The
  outline is painted only: it never captures input, and the control stays focusable and invokable
  from the keyboard and by UI Automation. Steps that need a window picked are replaced by "pick a
  window" steps when none is, and the pause walkthrough starts from whether AllBlack11 is paused
  (`help::Capabilities`: a picked window, paused). No other optional feature changes a step.

## UI Automation

The Settings window is custom drawn, so it implements UI Automation providers by hand
(`platform/uia.rs`, `uia_ui.rs`): `IRawElementProviderSimple`, `…Fragment`, `…FragmentRoot` with
Invoke, Toggle, SelectionItem and Value patterns; names, roles, states, values, bounding rectangles,
focus and layout-changed events. Navigation items are exposed as tab items with selection state.
The tree is rebuilt from the same row model that is painted, so what is announced and what is drawn
cannot drift. A kill switch (`ALLBLACK11_NO_UIA=1`) turns the provider off if it ever misbehaves.

What is verified: the provider's COM tables were exercised in-process by a self-test (QueryInterface,
reference counting, navigation order, patterns, names) under Wine. What is **not** verified:
Narrator, Accessibility Insights, any real UI Automation client, high contrast with a screen reader,
and whether Narrator announces the roles the way users expect. See EVIDENCE.md and SMOKE-TEST.md.

## Deviations

Everything here is a place where AllBlack11 does not follow the guidance or cannot deliver the
WinUI behavior. Each is a known cost, not a hidden one.

| # | Guidance | What AllBlack11 does | Why | Impact |
| --- | --- | --- | --- | --- |
| 1 | Windows apps are WinUI/XAML; controls come with Fluent styling, automation peers and VisualStateManager | Native Win32 + GDI, controls custom drawn, UI Automation written by hand | Keeps the program small, native and dependency-free; a framework migration was out of scope | Rendering differs subtly from WinUI (no animation, GDI text, no per-pixel anti-aliased rounded corners on controls); accessibility rests on hand-written providers that must be validated on Windows |
| 2 | Dark surfaces are dark gray; the page background is not pure black | **True black** page (`#000000`), cards `#161616`, navigation `#0C0C0C` | OLED-first product identity (the app exists to make frames black) | Looks slightly different from system apps in dark mode; contrast tests still pass |
| 3 | Use Mica / Acrylic | Opaque surfaces | Needs DWM backdrop plumbing on a custom-drawn window; no benefit for an OLED-black design | None for accessibility (opaque is the transparency fallback) |
| 4 | NavigationView: expanded ≥ 1008, compact 641 to 1007, minimal ≤ 640; pane widths are WinUI's | Expanded pane (216) at client widths ≥ 720, icon rail (48) below; **no minimal (menu-button) mode**; the window cannot be narrower than the pane plus 480 content (544 logical px with the rail, 664 with the text-only pane) | The window has a hard minimum size, so a menu-only mode is never reached, and a 1008 threshold would force the rail on most laptop windows | Different breakpoint and pane widths from the guidance |
| 5 | Text size and display scaling are separate (text scales, layout reflows) | One scale factor = display DPI × Windows *Text size* (100 to 225%); the whole layout scales | GDI-drawn rows are sized from their text; scaling only fonts would need every geometry constant to be text-relative | Increasing *Text size* also enlarges non-text parts (switches, buttons). Nothing clips (checked at 150% under Wine). Documented in code (`layout_scale`) |
| 6 | Segoe UI Variable with continuous optical size | Static instances chosen by family name, `Segoe UI` fallback | GDI cannot drive the font's axes | Slightly different glyph shapes at some sizes |
| 7 | TeachingTip: a popup with a tail pointing at the target, light-dismiss | An inline card at the top of the page plus a painted outline on the target | A popup covers the control being taught and needs another window and its own keyboard model; the guidance allows explicit-close tips | No tail; the connection is the outline and the step text naming the control |
| 8 | Dialogs: ContentDialog | System `MessageBox` for confirmations (restart Explorer, etc.); no AllBlack11-styled dialog | Native dialogs are accessible and consistent; restyling them was out of scope | Message boxes follow the system theme, not AllBlack11's own Light/Dark choice |
| 9 | Tray menu follows Windows 11 context menu | Owner-drawn menu built in 3.0 with Windows 11 metrics; colors now follow AllBlack11's own appearance; high contrast uses the system menu | Rewriting the tray menu was not part of this pass | Not pixel-matched to the shell's menu; no keyboard-accelerator column; owner-drawn items are exposed by the system menu to UI Automation only to the extent Windows provides (not verified) |
| 10 | Edit boxes should have a programmatic label (LabeledBy) | The nearby text is painted, not associated; edit boxes have no UI Automation label | Native `EDIT` windows are children whose name cannot be set without a custom wrapper | **Open accessibility gap**: a screen reader may announce an unlabeled edit box. Candidate fix: `WM_GETOBJECT` on a subclass, or a `STATIC` label placed before it in the tab order |
| 11 | Elements outside the viewport should report `IsOffscreen` | Rows scrolled out of view are exposed with their real rectangles and not flagged off-screen | Not implemented | Screen reader navigation may land on a row that is scrolled out of view; focus moves scroll it into view |
| 12 | Switches expose as `Button` with Toggle in WinUI | Exposed as `CheckBox` with Toggle and the localized name "switch" | Closest standard type; not verified against Narrator | May be announced as "check box" |
| 13 | Setup/uninstall UI | `setup/` (the installer) uses native `STATIC`/`BUTTON` controls with a fixed Segoe UI font and the system light theme; it is **not** on the design system | Separate crate, not in the scope of the Settings redesign; native controls keep it accessible | Visually different from Settings; no dark mode, no text-size scaling beyond the system's DPI |
| 14 | Motion principles | None implemented | See Motion | Nothing to evaluate |
| 15 | High contrast: system colors | Palette is exactly the system colors; selection uses `COLOR_HIGHLIGHT`; focus ring uses `COLOR_HOTLIGHT` | As guidance | Not run on a real high-contrast theme |

## Known gaps in this pass

- No part of the UI has run on Windows; every visual statement in EVIDENCE.md is "Wine-rendered"
  at best (Wine ≠ Windows: fonts, DWM, DPI handling and UI Automation clients differ).
- `PAGE_MARGIN` in `uitheme::metrics` is 24 while Settings draws a 20 px margin; reconcile on
  Windows once the pages have been seen.
- A trailing period was clipped at a wrap boundary in one Wine screenshot (Scenes description at
  the minimum width). The measured height and painted wrap agreed (same function, font, flags and
  width). Not seen to repeat; check on Windows.
- Multi-monitor DPI change is handled (`WM_DPICHANGED` re-creates fonts and relayouts) and has not
  been exercised.
- Right-to-left layout is not mirrored.
- Translations: all strings are English and live in code; long-text behavior was tested by
  resizing the window and raising the text size, not with a real translation.

## Inventory of surfaces

| Surface | Kind | Status in this pass |
| --- | --- | --- |
| Settings: Home, Fix this app, Appearance, Scenes, Rules, Safety, Diagnostics, Settings & help | Custom drawn pages | Rebuilt on the shared row model and design system |
| First-run setup (7 steps) | Same window, no pane | New |
| Walkthroughs (6) and Help search (8 topics) | Same window | New |
| Command palette (Ctrl+K, tray, optional hotkey) | Own popup window | Uses the shared theme and 8 px corners |
| Window picker crosshair and inspector report | Picker is the existing native crosshair; report is rows on Fix this app | Report restyled as rows |
| Recipe import, diff and confirmation | Recipes list, "It would change" lines and Try / Import / Export buttons on the Scenes page | Restyled, behavior unchanged |
| Color selection | Native edit boxes (caption, border, text, tint) with an inline validation message, plus Quick looks | Validation message and state added |
| Trial bar: countdown, Keep, Revert | Bar pinned above the page while a trial runs | Rebuilt on the shared button with hover, pressed and focus states; in the Tab order; exposed to UI Automation; the page and its edit boxes follow it |
| Errors, warnings, confirmations, empty states | Status line, inline rows, `MessageBox` | Wording and placement unified; message boxes unchanged |
| Safe mode, emergency pause, helper status | Safety page, Home | Reorganized; behavior unchanged |
| Demo and comparison windows (materials demo, before/after pair, preview panel) | Owned windows | Not restyled: they are samples of what AllBlack11 applies, not UI |
| Control harness | Separate test program | Unchanged |
| Tray menu | Owner-drawn menu | Follows AllBlack11's appearance choice; otherwise unchanged (deviation 9) |
| Setup / uninstall (`setup/`) | Native controls | Unchanged (deviation 13) |

`python3 scripts/check-controls.py` checks that every control that existed before the page
restructure (94 now, including the walkthrough and setup controls) is still reachable on a page.
