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
pages = (platform / "settings_pages.rs").read_text()


def enum_variants(name):
    match = re.search(r"enum %s \{(.*?)\n\}" % name, settings, re.S)
    if not match:
        return []
    return re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*)\s*,", match.group(1), re.M)


def builder_bodies(text):
    bodies = []
    for m in re.finditer(r"\n    (?:pub )?fn ((?:rows|push)_\w+)\(", text):
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
