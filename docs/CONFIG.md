# Configuration

AllBlack11 reads `%APPDATA%\AllBlack11\config.json`. Edit it while the app is running, then choose **Reload config**. The tray Shell toggles write `shell.oledTaskbar` and `shell.blackShell` for you.

A rule overrides only the fields it sets. The first matching exe name wins. Names are compared without case.

## Example

```json
{
  "enabled": true,
  "logging": true,
  "reapplyOnForeground": true,
  "default": {
    "enabled": true,
    "darkMode": true,
    "backdrop": "mica",
    "captionColor": "#000000",
    "borderColor": "accent",
    "textColor": "#FFFFFF",
    "corners": "default",
    "acrylicTint": "#000000",
    "acrylicOpacity": 0.55,
    "tintedAcrylicOnMoveFallback": true
  },
  "shell": {
    "oledTaskbar": false,
    "blackShell": false
  },
  "captionHeight": null,
  "experimentalDarkLegacy": false,
  "rules": [
    {
      "process": "mmc.exe",
      "backdrop": "tintedAcrylic",
      "acrylicOpacity": 0.7
    }
  ],
  "exclude": ["GeForceNOW.exe"],
  "excludeClasses": []
}
```

`mmc.exe` is Control Panel. That rule keeps the default black caption and accent border, and uses a darker tinted acrylic (opacity `0.7`).

## What is new in schema 2 (3.5)

Files from 1.x to 3.0 load unchanged. The next time AllBlack11 saves, it writes `"schemaVersion": 2` and keeps a one-time copy of your old file as `config.pre-3.5.json`, because a 3.0 build cannot read a 3.5 file. A file with a *newer* schema than this build understands is refused with a message instead of being half-read. A file that fails to parse falls back to `config.lkg.json` (the last config that loaded) and your file is left untouched.

```json
{
  "schemaVersion": 2,
  "rules": [
    {
      "name": "Windows Terminal",
      "class": "CASCADIA_HOSTING_WINDOW_CLASS",
      "priority": 5,
      "backdrop": "acrylic"
    },
    { "package": "Microsoft.WindowsNotepad_*", "backdrop": "none" },
    { "process": "app.exe", "path": "C:\\Tools\\*", "title": "report" }
  ],
  "focus": { "enabled": true, "activeBorder": "accent", "inactiveBorder": "none", "inactiveText": "#9A9A9A" },
  "reduceEffects": { "onBatterySaver": true, "onRemoteSession": true, "onTransparencyOff": true },
  "legacy": { "palette": "graphite", "emulatedX64": false }
}
```

**Rule matchers** (a rule needs at least one, and must match on every one it sets): `process` (file name), `class` (exact), `path` (glob on the full path), `package` (glob on the package family name), `title` (substring). Globs use `*` and `?`, ignore case, and treat `/` and `\` alike. A `path` or `package` rule never matches a window whose path or package is unknown. A `title` rule only matches when the title was read, which only happens while some rule uses one.

**Precedence** is deterministic: higher `priority` (−1000..1000), then the rule that matched more specifically (package 30, title 25, class 20, path 15, executable 10 points each), then the earlier rule. Settings > Apps shows why a rule won and warns about rules that can only be told apart by file order.

**Focus Look**: when `enabled`, the active window gets `activeBorder`, others get `inactiveBorder` and, if it stays readable on the caption, `inactiveText`. Anything a rule sets itself wins. It runs on focus changes only.

**Reduce effects**: each condition switches backdrop materials to plain colors while it holds. The active reason is shown in Settings. Colors, corners and dark mode are unchanged.

**Legacy**: `palette` is `graphite` or `black`. `emulatedX64` is ARM64-only and needs the packaged helper.

## Recipes

A recipe is a small JSON file (`%APPDATA%\AllBlack11\recipes\<id>.json`, or built in) with a **closed schema**: `schema`, `id`, `name`, `description`, `author`, `minBuild`, `look` (only `darkMode`, `backdrop`, `captionColor`, `borderColor`, `textColor`, `corners`, `acrylicTint`, `acrylicOpacity`, `tintedAcrylicOnMoveFallback`), an optional `match` (`process`, `class`, `package`) and `notes`. Unknown fields are refused. There is no field that runs anything, writes the registry, changes the shell, or points outside the look. Imported recipes are validated, shown as a diff, and tried for 15 seconds before they can be kept.

## Keys

| Key | Where | Values |
| --- | --- | --- |
| `enabled` | root | Master switch for window frames |
| `logging` | root | Writes `%APPDATA%\AllBlack11\allblack11.log` |
| `reapplyOnForeground` | root | Re-apply when a window is focused |
| `darkMode` | `default`, rule | `true` / `false` |
| `backdrop` | `default`, rule | `mica`, `micaAlt`, `acrylic`, `tintedAcrylic`, `none`, `auto` |
| `captionColor`, `borderColor`, `textColor` | `default`, rule | `#RRGGBB`, `accent`, `none`, `default` |
| `corners` | `default`, rule | `default`, `round`, `small`, `square` |
| `acrylicTint` | `default`, rule | `#RRGGBB`. Default `#000000` |
| `acrylicOpacity` | `default`, rule | `0.0`–`1.0`. Default `0.55` (alpha byte `0x8C`) |
| `tintedAcrylicOnMoveFallback` | `default`, rule | Use blur instead of acrylic while dragging. Default `true` |
| `shell.oledTaskbar` | `shell` | OLED taskbar transparency |
| `shell.blackShell` | `shell` | Black Start, taskbar, and notification center |
| `captionHeight` | root | Legacy title-bar height in pixels, `16`–`64`. `null` leaves Windows alone. Needs a sign-out. The previous `CaptionHeight` is saved in `metrics-backup.json` |
| `experimentalDarkLegacy` | root | `false` by default. Opt-in hook that recolors classic dialogs. Same-architecture processes only. Never elevated, protected, or excluded |
| `exclude`, `excludeClasses` | root | Extra skips. The built-in shell and service list is not in this file. The experimental hook uses `exclude` too |
| `rules[].process` | rule | Exe file name |
| `rules[].enabled` | rule | `false` skips that exe. With `default.enabled` set to `false`, only rules set to `true` are themed |

`borderColor: "accent"` reads `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Accent\AccentColorMenu`. The dword `0xFF748501` is stored as `0xAABBGGRR`, which is RGB `#018574`.

`AllBlack11.exe --check` prints the resolved config. It attaches to the console it was launched from and also writes `%APPDATA%\AllBlack11\check.txt`.

## Tinted acrylic

`backdrop: "acrylic"` is Desktop Window Manager acrylic. It has no tint control. `backdrop: "tintedAcrylic"` calls `SetWindowCompositionAttribute` (`WCA_ACCENT_POLICY` = 19) with `ACCENT_ENABLE_ACRYLICBLURBEHIND` (4). The tint is one `AABBGGRR` value: the alpha byte comes from `acrylicOpacity`, then blue, green, and red from `acrylicTint`.

While that mode is on, `DWMWA_SYSTEMBACKDROP_TYPE` is set to none (`1`) so the two effects do not stack. Caption, border, text, corners, and dark mode are still applied. If the composition call returns false, that process falls back to plain DWM acrylic and the miss is logged once.

With `tintedAcrylicOnMoveFallback` left on, a drag or resize switches that window to `ACCENT_ENABLE_BLURBEHIND` (3) with the same tint, then back to acrylic when the move ends. On Exit, Disable, or Reset, every window that received an accent policy is set to `ACCENT_DISABLED` (0).

## Shell backup

Before the first change, AllBlack11 writes the current HKCU values to `%APPDATA%\AllBlack11\shell-backup.json`. Uncheck, or `uninstall.ps1`, puts those values back. If a value did not exist, it is deleted.

If you turn a toggle on when the registry **already** matches what AllBlack11 would write (for example you edited the palette by hand earlier), there is no older value to save. The new backup slot is marked `"alreadyApplied": true`. Uncheck then uses Windows defaults instead of writing the modified values back:

- OLED taskbar: delete `UseOLEDTaskbarTransparency`.
- Black shell: delete `AccentPalette` and broadcast `ImmersiveColorSet` so Windows rebuilds the palette, and set Personalize `ColorPrevalence` to `0`.

A `shell-backup.json` that already contains slots is not overwritten. If that file has real originals from an earlier enable, uncheck still restores those bytes and DWORDs.

Shipped excludes cover GeForce NOW, Steam, Epic, GOG Galaxy, Battle.net, Ubisoft, EA, Riot, and the NVIDIA overlay. File Explorer is themed only for folder windows (`CabinetWClass`).

## Presets

The tray **Presets** menu and the top of Settings rewrite `default` (and `shell`, for Pure OLED and Factory defaults) and leave `rules`, `exclude`, and the startup options alone. Pure OLED: backdrop `none`, border `none`, black caption, white text, both shell flags on. Accent glass: `tintedAcrylic`, black tint, opacity `0.70`, accent border. Classic Mica: `mica`, black caption, accent border. Factory defaults: the shipped `default` block and both shell flags off.

## Settings window

The tray **Settings…** item, and a double-click on the icon, open a native Win32 window. It is drawn with GDI in the AllBlack11 process. There is no WebView. The frame uses Mica and immersive dark mode. The client is pure black so the text stays sharp and OLED pixels stay off.

Every coordinate is logical (96 dpi) and scaled once, so layout, hit-testing and the edit boxes stay aligned at any display scale and when the window moves between monitors. The preview at the top is drawn from the same `Style` the theming pass uses, so it matches what a window gets.

Adding or removing an app in a list, or picking a preset, rewrites `config.json` the same way the tray does. The file is written to a temporary name and renamed, so a crash cannot leave a half-written config.

Edits write `config.json` immediately. Shell toggles update HKCU the same way the tray menu does. The caption-height slider writes `CaptionHeight` only when you release it, and only after the original value has been copied to `metrics-backup.json`. Set the slider back to **System** to restore that value. Sign out for Windows to pick the height up.

## Experimental dark legacy windows

`experimentalDarkLegacy` defaults to `false`.

Without the hook, AllBlack11 still gives `#32770` dialogs a dark caption (DWM) and calls `SetWindowTheme` on the dialog and its children (`DarkMode_CFD` for dialogs and edits, `DarkMode_Explorer` otherwise). Programs often ignore a theme name set from another process, so the client area can stay light.

With the switch on, `AllBlack11Hook.dll` (shipped next to the exe, same architecture) is injected with `SetWindowsHookEx(WH_CALLWNDPROC)` into the UI thread of:

- classic dialogs (`#32770`), including Run and message boxes
- `mmc.exe` consoles such as Device Manager

It does not enter elevated processes, processes that refuse `PROCESS_QUERY_LIMITED_INFORMATION`, a different CPU architecture, or any exe in `exclude` or the built-in skip list. Inside the target it subclasses `#32770` windows and handles `WM_CTLCOLOR*` / `WM_ERASEBKGND`. List views get `LVM_SETBKCOLOR` and `DarkMode_Explorer`. Turning the switch off broadcasts `AllBlack11.HookStop`, restores window procedures and theme names, then unhooks. Exit does the same.

What stays light: owner-drawn DirectUI and XAML surfaces, HTML control-panel pages, and custom title bars. The hook does not call `SetSysColors`. On a Snapdragon PC the ARM64 zip covers native ARM64 dialogs; x64 programs running under emulation are left alone.
