# Styling Microsoft Store, Calculator, Windows Settings and similar apps

**Goal (owner):** the dark, blurry look of the Start menu and the notification center on these apps
too. **Status:** researched from Microsoft's documentation and the owner's results; the open
questions are turned into probes the owner can run in minutes (section 5). Nothing here was run on
Windows by the author of this document, and each claim says how it is known.

Evidence labels: **Documented** (Microsoft Learn, linked at the end), **Observed** (the owner's PC,
2026-11-01), **Inferred** (follows from the above, not tested), **Unknown**.

## 1. What these apps are, and what draws what

| Part of the window | Who draws it | AllBlack11's reach today |
| --- | --- | --- |
| Frame: border, shadow, and, when the app does not extend into it, the title bar | The system's window manager (DWM) for the frame window; the host process `ApplicationFrameHost.exe` for a packaged app's frame | Yes: caption/border/text colors and a backdrop type are DWM window attributes (**Documented** API, Windows 11 build 22621 for the backdrop type) |
| Everything inside: navigation pane, pages, cards, and, in apps that extend into the title bar, the title bar too | The app's own XAML, in the app's own process, composed onto the frame | **No** from outside: it is another process's visual tree |
| The material behind the app's content (Mica) | In UWP and WinUI 2 apps, the app asks for it inside its own XAML (`BackdropMaterial`, **Documented**); in WinUI 3 apps, `Window.SystemBackdrop` | Not directly |

Windows Settings, Calculator and the Store are all in that family (**Observed** for the hosting: the
owner's pack found their windows as `ApplicationFrameWindow`; **Inferred** for who draws their
backdrop, because that depends on each app's code).

## 2. Findings

**F1. Mica is not blurry glass.** Microsoft describes Mica as *opaque*: it takes the desktop
wallpaper **once**, tints it with the theme, and falls back to a flat solid color when transparency
is off, Battery Saver is on, the hardware is low-end, **or the window is inactive**, and under high
contrast (**Documented**). Apps that follow Microsoft's layering use a low-opacity content layer on
top of it. So the "dark Mica" look in these apps is a tinted wallpaper, by design.

**F2. The Start-menu look is Acrylic, and Microsoft limits it.** Acrylic is translucent *blur* of what
is behind it. Microsoft's guidance is to use background acrylic for **transient surfaces** (flyouts,
menus), and explicitly says **"Don't put desktop acrylic on large background surfaces of your app"**;
background acrylic also turns solid when the window deactivates (**Documented**). That is why the
bodies of Settings, Calculator and the Store are not acrylic: it is a design rule, not an omission.

**F3. A backdrop set on the frame only shows through transparent layers.** The window-level
backdrop attribute draws "behind the entire window bounds", and Microsoft's own Mica guidance says
every layer between the UI and the window must be transparent for it to show (**Documented**). An app
that paints its own opaque Mica brush inside its XAML covers whatever the frame has behind it
(**Inferred**).

**F4. The packs asked for almost nothing.** Against the default look they changed a dark gray
(`#1A1A1A` to `#000000`) and a text white (**Observed** in the app's comparison line). "No visible
change" was the expected result even if every request was accepted.

**F5. Hypothesis H1.** For apps that extend their content into the title bar and paint their own
Mica, frame colors and a frame backdrop are covered completely. This explains the owner's result and
predicts that *loud* probe colors will also not show. **Unknown** until probe E1 is run.

**F6. The wallpaper is the input to Mica.** Mica "incorporates theme and desktop wallpaper"
(**Documented**). A true-black wallpaper therefore pushes every Mica app toward black without
touching the apps, and a wallpaper with broad, soft, dark glows gives them a visible, blurry-looking
depth. Fallback conditions in F1 apply (an inactive window shows the solid fallback color, not the
wallpaper tint).

**F7. Contrast themes override XAML apps at system level** (**Documented**): the Windows contrast
themes, including custom colors the user edits, change the colors these apps use. That can make them
pure black, at the price of flat colors, no Mica or acrylic, and AllBlack11 standing down (it
already does under high contrast).

**F8. Some apps have their own theme setting**, which is the supported way to change their insides
(for example Calculator's app theme option). Whether a given Store app has one is **Unknown** per app.

**F9. Version limits.** Mica for UWP needs Windows 11 build 22000; the DWM backdrop type needs
22621 (**Documented**). Older builds get solid colors.

## 3. Options

| # | Option | Documented? | What it can do for these apps | Blur? | Risk | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| O1 | Frame colors through DWM (what the packs do) | yes | Border and, where the frame shows, title bar color | no | low | Keep as a **probe** (E1); no more tuning until E1 says it is visible |
| O2 | A DWM backdrop type on the frame (Mica, Mica Alt, Acrylic) | yes | Only through transparent layers (F3) | Acrylic: yes | low | Probe (E2); useful where it works |
| O3 | Blur-behind with a tint through the window-composition call AllBlack11 already uses for tinted acrylic | no (widely used) | Same transparency limit as O2 | yes | low | Probe (E2) |
| O4 | System levers: dark app mode, transparency effects, accent color, Battery Saver state | Settings UI yes | Switch every Mica/acrylic app on, dark, consistently; explains why a window looks flat | no | low | **Build** a guided, reversible helper (phase 1) |
| O5 | **Wallpaper-driven Mica**: ship wallpapers designed for Mica (true black, and variants with soft low-frequency glows) | yes (F6) | Darkens or tints the base of every Mica app, no hacks | looks soft | none | **Build** (phase 1): it is the supported way to get "dark with depth" in these apps |
| O6 | A custom contrast theme as an optional "pure black apps" mode | yes | Pure black Settings, Calculator, Store | no | medium (flat look, stands down) | Guide only (phase 2) |
| O7 | The app's own theme option | yes | The app's insides | per app | none | Document per app in the pack text |
| O8 | Change the app's own visual tree or code from inside its process | — | The only way to turn an app's Mica into acrylic | yes | high | **Not pursued by AllBlack11** (see section 4) |
| O9 | Contribute upstream (Calculator is open source) or file Feedback Hub requests for a material option | — | The right place for "acrylic bodies" | yes | none | Worth doing for Calculator only |

## 4. The honest answer to "blurry like Start"

- What can be had **from outside, supported, today**: dark and tinted base layers in every Mica app
  (O4 + O5), pure black if the owner accepts contrast themes (O6), per-app theme options (O7), and
  whatever the probes show O1 to O3 can still reach.
- What **cannot** be had from outside: real blur in an app's body. Only the app's own process can
  swap its brushes (O8), and that is a different kind of product: it modifies other vendors' running
  software, which security tools flag, which breaks with every app update, and which is outside what
  AllBlack11 promises (frame styling, reversible, no changes to other apps' code or memory). This
  project will not design it. If the owner wants it, it needs its own decision, its own opt-in
  consent screen, its own kill switch and its own legal and security review, after the probes below
  show that nothing supported works; nobody should start there.
- Microsoft's own materials explain why the apps look the way they do (F1, F2). Asking for a material
  option in those apps (O9) is the route that fixes it for everyone.

## 5. Probes the owner can run in 15 minutes (new in this branch)

**Where:** Apps > Customize an app > pick the app (the crosshair) > open **Probe: can Windows style
this window at all?** > **Probe frame colors (20 s)** and **Probe backdrop (20 s)**. Each applies a
loud look to that one window for 20 seconds and puts it back. Nothing is saved. The style receipt
below shows what Windows answered.

| Probe | Do | If you SEE it | If you see NOTHING |
| --- | --- | --- | --- |
| **E1** frame colors on Calculator, Settings, Store | Probe frame colors | O1 reaches that app: tune the packs with real colors | H1 holds: stop tuning frame colors for that app |
| **E2** backdrop on the same three | Probe backdrop | O2/O3 reach it: build a backdrop option for that app | The app paints over its frame: O2/O3 are dead for it |
| **E3** system levers | In Windows: Colors > app mode Dark, Transparency effects On, Battery Saver off. Apply the *Eclipse* or *Horizon* wallpaper (Appearance > Wallpapers). Look at Settings and Calculator (active and inactive) | Record the base color: this is what O4+O5 give | If still gray, try E4 |
| **E4** contrast theme | Windows > Accessibility > Contrast themes > Night sky, then edit Background and Text to black and white | O6 is viable; record it | The app ignores it (unlikely) |
| **E5** pixel check | With the window in the foreground, compare the base color to a black wallpaper region next to it (a screenshot is enough) | Quantifies "how black" for the table in section 6 | n/a |

For each probe write down: app, Windows build, what you saw, and whether the window was active.
Use the notes buttons on the same page (Works, Partly, Fails). Send me the results, or the style
receipt text (Copy style receipt), and the next step is chosen from the table, not guessed.

## 6. Development plan

**Phase 0 (done in this branch):** the probes (E1, E2) and the existing evidence chain for packs.

**Phase 1: "Dark apps" helper (supported levers only)**, after E3:
1. A page under Appearance that shows, with plain words, the five things that decide how an app looks
   (app mode, transparency effects, Battery Saver, wallpaper, high contrast) and which one is
   limiting right now; each change is explicit, reversible and in Look history.
2. Mica-aware wallpapers: add variants of the shipped set designed for Mica (a true-black one and
   broad soft dark glows), and a preview that renders what Mica does to them.
3. Pack text updated per app from the probe results (what works, what does not, and the app's own
   theme option).
*Acceptance:* E3 recorded on ARM64 and x64; a unit test for every state the helper can report; no
write without a user action; Undo restores the previous values.

**Phase 2:** guided contrast-theme setup (O6) if E4 passes; a "Seen on screen" measurement in the
style receipt (sample a few pixels of the real window, locally, never saved) so the receipt can say
"measured" instead of "not verified".

**Phase 3:** upstream request and, for Calculator, a proposal (O9).

**Not planned:** O8.

## Sources

- Mica material: https://learn.microsoft.com/windows/apps/design/style/mica
- Acrylic material: https://learn.microsoft.com/windows/apps/design/style/acrylic
- DWM_SYSTEMBACKDROP_TYPE: https://learn.microsoft.com/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type
- Mica in UWP (WinUI 2): https://learn.microsoft.com/windows/uwp/ui-input/mica-uwp
- BackdropMaterial.ApplyToRootOrPageBackground: https://learn.microsoft.com/windows/winui/api/microsoft.ui.xaml.controls.backdropmaterial.applytorootorpagebackground
- Contrast themes: https://learn.microsoft.com/windows/apps/design/accessibility/high-contrast-themes
- Owner's results: `docs/dev/EVIDENCE.md`; the packs: `src/packs.rs`; the comparison line: Apps > Compatibility packs.
