# AllBlack11 wallpapers

Eight original wallpapers, drawn by `generate.py` (numpy and Pillow only). There are no photographs,
stock images or scraped art, and the output is deterministic: the same script gives the same bytes.

| Id | Name | What it is |
| --- | --- | --- |
| `moonrise` | Moonrise | A silver moon rising from the bottom right edge |
| `eclipse` | Eclipse | A black disc with a thin silver edge and a soft corona |
| `window-light` | Window light | One thin blue window frame, glowing faintly |
| `horizon` | Horizon | A single silver horizon line with a dim blue haze |
| `contour` | Contour | Graphite contour lines fading to black |
| `orbit` | Orbit | Five tilted orbits with a tiny moon |
| `dot-matrix` | Dot matrix | A fine dot grid fading from the lower left |
| `ridges` | Ridges | Five black ridge lines with a thin moonlit rim |

Regenerating on the author's machine reproduced `manifest.json` (every SHA-256) and the previews
exactly. PNG bytes depend on the zlib in the Pillow build, so a different Pillow may give different
bytes with the same pixels; the digests in `manifest.json` are what the app checks, so attach the
files produced by `--check` on the machine that wrote the manifest, or regenerate the manifest
there and commit it.

## Design rules (checked on every run)

- A true-black foundation: at least 70% of pixels are exactly black (an OLED panel leaves them off).
- No large bright fields: mean luminance at most 6/255 and at most 0.6% of pixels brighter than
  96/255 (thin strokes only).
- Accents are the brand's moonlit silver and cool blue (`docs/dev/BRAND.md`), dim and few.

The measured values are recorded per image in `manifest.json` (`oled`), and a unit test in
`src/wallpaper.rs` fails if a recorded value breaks a rule.

## Reproduce

```bash
python3 assets/wallpapers/generate.py --check    # writes target/wallpapers/*.png (3840x2160),
                                                 # thumbs/*.jpg and manifest.json; fails if a
                                                 # rule above is broken (about 70 seconds)
python3 assets/wallpapers/generate.py --verify   # compare target/wallpapers/ to manifest.json
```

## Where things live

| What | Where | Why there |
| --- | --- | --- |
| Generator, manifest, README | this folder, in git | small, reviewable |
| Previews (480x270 JPEG, 3 to 13 KB each) | `thumbs/`, in git, compiled into the exe | the gallery shows them with no download |
| Full images (3840x2160 PNG, 5.4 MB together) | `target/wallpapers/` after generating; **attach to the GitHub release** | keeps the repository lean |

The app downloads one image at a time, only when the user presses **Download image** (the button's
page shows the size first), from
`https://github.com/TH0MSX/AllBlack11/releases/download/<this version's tag>/<file>`. The file is
written to `%LOCALAPPDATA%\AllBlack11\wallpapers`, checked against the SHA-256 in the manifest that is
compiled into the exe (so the check does not depend on the server), and only then renamed into place.
It is checked again immediately before it is applied.

**Release step (owner):** after `generate.py`, attach the eight `allblack11-wallpaper-*.png` files
from `target/wallpapers/` to the release whose tag matches the app version (for example
`v3.8.0-preview.1`). Until they are attached the gallery says so instead of failing silently.
Check `generate.py --verify` first: the digests must match `manifest.json`.
