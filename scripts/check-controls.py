#!/usr/bin/env python3
"""Every Settings control must still be reachable from some page.

Settings pages are built from rows (`b.push(Row::Toggle(Toggle::X))`, `Row::Buttons(vec![..])`).
Splitting or reorganizing pages can silently drop a control; this lists every Toggle, Choice,
Slider, Button and Color/AddField that no `rows_*`/`push_*` builder mentions.

  python3 scripts/check-controls.py [path-to-platform-dir]
"""
import re
import sys
from pathlib import Path

platform = Path(sys.argv[1] if len(sys.argv) > 1 else "src/platform")
settings = (platform / "settings.rs").read_text()
pages = (platform / "settings_pages.rs").read_text() + "\n" + (platform / "setup_ui.rs").read_text()


def enum_variants(name):
    match = re.search(r"enum %s \{(.*?)\n\}" % name, settings, re.S)
    if not match:
        return []
    return re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*)\s*,", match.group(1), re.M)


def builder_bodies(text):
    bodies = []
    for m in re.finditer(r"\n    (?:pub )?fn ((?:rows|push|with)_\w+)\(", text):
        start = text.index("{", m.end())
        depth = 0
        for i in range(start, len(text)):
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
                if depth == 0:
                    bodies.append(text[start : i + 1])
                    break
    return "\n".join(bodies)


bodies = builder_bodies(pages) + "\n" + builder_bodies(settings)
missing = []
total = 0
for enum in ("Toggle", "Choice", "Slider", "Button"):
    for variant in enum_variants(enum):
        total += 1
        if not re.search(r"\b%s::%s\b" % (enum, variant), bodies):
            missing.append("%s::%s" % (enum, variant))
print("controls checked: %d" % total)
if missing:
    print("NOT on any page:")
    for item in missing:
        print("  " + item)
    sys.exit(1)
print("every control is on a page")

# --- every search anchor lands on the page that holds the control -----------------------------
# src/commands.rs lists searchable settings as setting(title, hint, keywords, Dest::X, "kind:Name").
# Each must name a real control that the builders of that very page push, so a search result can
# never open a page that does not have the thing it promised.
PAGE_BUILDERS = {
    "Home": ["rows_home"],
    "AppearanceScenes": ["rows_scenes"],
    "AppearanceEffects": ["rows_appearance", "push_contrast_lab"],
    "AppearanceWallpapers": ["rows_gallery"],
    "AppearanceSchedule": ["rows_schedule"],
    "AppearanceTools": ["rows_tools"],
    "AppsFix": ["rows_fix", "push_pick", "push_fix_card"],
    "AppsRules": ["rows_rules"],
    "AppsPacks": ["rows_packs", "push_packs", "push_notebook"],
    "AppsExclusions": ["rows_exclusions", "push_exclusions"],
    "Safety": ["rows_safety"],
    "SafetyExperimental": ["rows_experimental", "push_legacy"],
    "Settings": ["rows_settings", "push_updates"],
    "HelpTopics": ["rows_help", "push_help"],
    "HelpDiagnostics": ["rows_diagnostics"],
    "HelpAbout": ["rows_about"],
}


def function_bodies(text):
    out = {}
    for m in re.finditer(r"\n    (?:pub )?fn ((?:rows|push|with)_\w+)\(", text):
        start = text.index("{", m.end())
        depth = 0
        for i in range(start, len(text)):
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
                if depth == 0:
                    out[m.group(1)] = text[start : i + 1]
                    break
    return out


bodies_by_name = function_bodies(pages)
commands = (platform.parent / "commands.rs").read_text()
enum_names = {"toggle": "Toggle", "choice": "Choice", "slider": "Slider", "button": "Button"}
bad = []
count = 0
for m in re.finditer(r'setting\(\s*"([^"]+)",\s*"[^"]*",\s*"[^"]*",\s*Dest::(\w+),\s*"([^"]+)"\s*,?\s*\)', commands):
    title, dest, anchor = m.groups()
    count += 1
    kind, _, name = anchor.partition(":")
    page_text = "\n".join(bodies_by_name.get(f, "") for f in PAGE_BUILDERS.get(dest, []))
    if dest not in PAGE_BUILDERS or not page_text:
        bad.append("%s: no builders known for Dest::%s" % (title, dest))
        continue
    if kind in enum_names:
        variant = enum_names[kind]
        if name not in enum_variants(variant):
            bad.append("%s: %s::%s is not an enum variant" % (title, variant, name))
        elif not re.search(r"\b%s::%s\b" % (variant, name), page_text):
            bad.append("%s: %s::%s is not pushed by the %s page" % (title, variant, name, dest))
    elif kind == "header":
        if 'Row::Header("%s")' % name not in page_text and 'Row::Expander("%s")' % name not in page_text:
            bad.append("%s: heading %r is not on the %s page" % (title, name, dest))
    elif kind == "color":
        if "ID_" + {"caption": "CAPTION", "border": "BORDER", "text": "TEXT", "tint": "TINT"}.get(name, "?") not in page_text:
            bad.append("%s: color box %r is not on the %s page" % (title, name, dest))
    elif kind == "presets":
        if "Row::Presets" not in page_text:
            bad.append("%s: Row::Presets is not on the %s page" % (title, dest))
    else:
        bad.append("%s: unknown anchor kind %r" % (title, kind))
print("search anchors checked: %d" % count)
if count < 40:
    bad.append("expected at least 40 search anchors, found %d (did the setting() format change?)" % count)
if bad:
    print("SEARCH ANCHORS THAT DO NOT LAND:")
    for item in bad:
        print("  " + item)
    sys.exit(1)
print("every search anchor lands on a control of its page")
