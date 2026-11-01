#!/usr/bin/env python3
"""AllBlack11 wallpapers: eight original, code-generated, OLED-friendly images.

Every pixel comes from the functions below. There are no photographs, stock images or scraped
art, and the output is deterministic: the same script gives the same bytes (seeded noise, fixed
quantization), so the manifest's SHA-256 values can be checked by anyone.

    python3 assets/wallpapers/generate.py            # writes PNGs to target/wallpapers/,
                                                     # thumbnails and manifest.json here
    python3 assets/wallpapers/generate.py --check    # also fails if an image breaks the OLED rules
    python3 assets/wallpapers/generate.py --verify   # compare existing PNGs to manifest.json

Needs only numpy and Pillow. Design rules (checked on every run, see `check_oled`):

  * a true-black foundation: most pixels are exactly black, so an OLED panel leaves them off;
  * no large bright fields: the mean luminance is tiny and only thin strokes are bright;
  * accents are the brand's moonlit silver and cool blue, kept dim and few.

Direction: "moonlit precision", the same as the app icon (docs/dev/BRAND.md).
"""
import hashlib
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

W, H = 3840, 2160
HERE = Path(__file__).resolve().parent
OUT_DIR = HERE.parents[1] / "target" / "wallpapers"
THUMB_DIR = HERE / "thumbs"
THUMB_SIZE = (480, 270)

SILVER = np.array([0.80, 0.84, 0.90], dtype=np.float32)  # moon silver, #A9B3C2 .. #F4F6FA family
BLUE = np.array([0.36, 0.71, 0.94], dtype=np.float32)    # window accent #5BB4F0
GRAPHITE = np.array([0.30, 0.32, 0.36], dtype=np.float32)

YY, XX = np.mgrid[0:H, 0:W].astype(np.float32)
CX, CY = W / 2.0, H / 2.0
S = H / 2160.0  # scale unit, so shapes keep their proportions if W/H are ever changed


def canvas():
    return np.zeros((H, W, 3), dtype=np.float32)


def smooth(x, a, b):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def glow(dist, radius, power=2.0):
    """1 at distance 0, falling to 0 at `radius` (and exactly 0 beyond it)."""
    t = np.clip(1.0 - dist / radius, 0.0, 1.0)
    return t**power


def line(dist, half_width, soft=1.5 * S):
    """An antialiased stroke around distance 0."""
    return 1.0 - smooth(np.abs(dist), half_width, half_width + soft)


def paint(img, mask, color, amount):
    img += (mask * amount)[..., None] * color


def rounded_box_sd(cx, cy, hw, hh, r, angle=0.0):
    """Signed distance to a rounded rectangle (negative inside)."""
    x, y = XX - cx, YY - cy
    if angle:
        c, s = np.cos(angle), np.sin(angle)
        x, y = x * c + y * s, -x * s + y * c
    qx, qy = np.abs(x) - (hw - r), np.abs(y) - (hh - r)
    return np.hypot(np.maximum(qx, 0), np.maximum(qy, 0)) + np.minimum(np.maximum(qx, qy), 0) - r


def lowpass_noise(seed, cells, octaves=3):
    """Smooth, repeatable noise in 0..1 from random values on a coarse grid, bicubic upsampled."""
    rng = np.random.default_rng(seed)
    total = np.zeros((H, W), dtype=np.float32)
    weight = 0.0
    for octave in range(octaves):
        n = cells * (2**octave)
        grid = rng.random((max(2, int(n * H / W)) + 1, n + 1)).astype(np.float32)
        layer = Image.fromarray(grid, mode="F").resize((W, H), Image.BICUBIC)
        amp = 0.5**octave
        total += np.asarray(layer) * amp
        weight += amp
    total /= weight
    lo, hi = float(total.min()), float(total.max())
    return (total - lo) / (hi - lo)


def to_8bit(img, seed):
    """Quantize with dither on lit pixels only, so black stays exactly black."""
    rng = np.random.default_rng(seed)
    img = np.clip(img, 0.0, 1.0) * 255.0
    lit = img.max(axis=2, keepdims=True) > 0.02
    noise = (rng.random(img.shape, dtype=np.float32) + rng.random(img.shape, dtype=np.float32)) - 1.0
    out = np.floor(img + 0.5 + np.where(lit, noise * 0.55, 0.0))
    return np.clip(out, 0, 255).astype(np.uint8)


# ---------------------------------------------------------------------------------------------
# The eight images. Each returns a float image (0..1). Names are the ids used by the app.
# ---------------------------------------------------------------------------------------------

def moonrise():
    img = canvas()
    cx, cy, r = CX + 420 * S, H + 80 * S, 560 * S
    d = np.hypot(XX - cx, YY - cy) - r
    disc = 1.0 - smooth(d, -1.5 * S, 1.5 * S)
    shade = 0.16 + 0.10 * lowpass_noise(11, 5)  # faint maria, nothing bright
    paint(img, disc, SILVER, shade * 0.9)
    paint(img, line(d, 2.0 * S, 3 * S), SILVER, 0.55)           # the thin lit rim
    paint(img, glow(np.maximum(d, 0), 380 * S, 2.6), BLUE, 0.050)  # cool atmosphere above it
    return img


def eclipse():
    img = canvas()
    d = np.hypot(XX - CX, YY - CY) - 430 * S
    corona = glow(np.maximum(d, 0), 300 * S, 2.8)
    paint(img, corona, SILVER, 0.16)
    paint(img, line(d, 3.0 * S, 4 * S), SILVER, 0.75)
    paint(img, glow(np.maximum(d, 0), 90 * S, 2.0), BLUE, 0.10)
    img *= (d > 0)[..., None]  # the disc itself stays true black
    paint(img, line(d, 3.0 * S, 4 * S), SILVER, 0.75)
    return img


def window_light():
    img = canvas()
    sd = rounded_box_sd(CX, CY, 760 * S, 440 * S, 70 * S)
    paint(img, line(sd, 3.0 * S, 4 * S), BLUE, 0.80)
    paint(img, glow(np.abs(sd), 260 * S, 2.4), BLUE, 0.045)
    bar = rounded_box_sd(CX, CY - 440 * S + 62 * S, 760 * S, 62 * S, 0)  # title band, drawn as a hairline
    paint(img, line(YY - (CY - 440 * S + 124 * S), 1.5 * S, 3 * S) * (np.abs(XX - CX) < 760 * S - 70 * S), BLUE, 0.40)
    # three small controls in the title band
    for i in range(3):
        bx = CX + 760 * S - 130 * S + i * 46 * S
        by = CY - 440 * S + 62 * S
        paint(img, 1.0 - smooth(np.hypot(XX - bx, YY - by), 5 * S, 7 * S), SILVER, 0.55)
    del bar
    return img


def horizon():
    img = canvas()
    y0 = H * 0.66
    above = np.clip(y0 - YY, 0, None)
    paint(img, glow(above, 520 * S, 2.8) * (YY < y0), BLUE, 0.090)
    paint(img, line(YY - y0, 1.5 * S, 3 * S), SILVER, 0.60)
    # a soft pool of light where the line is brightest, near the centre
    pool = np.exp(-(((XX - CX) / (1100 * S)) ** 2))
    paint(img, line(YY - y0, 2.5 * S, 5 * S) * pool, SILVER, 0.30)
    return img


def contour():
    img = canvas()
    field = lowpass_noise(23, 3, octaves=4)
    levels = 18
    band = (field * levels) % 1.0
    dist = np.minimum(band, 1.0 - band)  # distance to the nearest contour, in level units
    grad = np.hypot(*np.gradient(field * levels)) + 1e-6
    px = dist / grad  # approx distance in pixels
    stroke = line(px, 1.2 * S, 2.2 * S)
    vignette = smooth(1.0 - np.hypot((XX - CX) / (W * 0.62), (YY - CY) / (H * 0.62)), 0.0, 0.75)
    paint(img, stroke * vignette, GRAPHITE, 0.55)
    peak = smooth(field, 0.78, 0.95)
    paint(img, stroke * peak * vignette, BLUE, 0.45)
    return img


def orbit():
    img = canvas()
    tilt = np.deg2rad(-18)
    c, s = np.cos(tilt), np.sin(tilt)
    x, y = (XX - CX), (YY - CY)
    xr, yr = x * c + y * s, -x * s + y * c
    for i, (a, b) in enumerate([(380, 150), (620, 245), (860, 340), (1100, 435), (1340, 530)]):
        e = np.sqrt((xr / (a * S)) ** 2 + (yr / (b * S)) ** 2) - 1.0
        paint(img, line(e * b * S, 1.4 * S, 3 * S), GRAPHITE if i % 2 else SILVER, 0.34 - 0.04 * i)
    # the moon: a small silver dot on the third orbit
    t = np.deg2rad(212)
    mx = (860 * S) * np.cos(t)
    my = (340 * S) * np.sin(t)
    px, py = CX + mx * c - my * s, CY + mx * s + my * c
    d = np.hypot(XX - px, YY - py)
    paint(img, 1.0 - smooth(d, 17 * S, 19 * S), SILVER, 0.95)
    paint(img, glow(d, 150 * S, 2.6), BLUE, 0.11)
    # the centre
    paint(img, 1.0 - smooth(np.hypot(x, y), 6 * S, 8 * S), SILVER, 0.55)
    return img


def dot_matrix():
    img = canvas()
    pitch = 44 * S
    gx, gy = (XX % pitch) - pitch / 2, (YY % pitch) - pitch / 2
    dots = 1.0 - smooth(np.hypot(gx, gy), 2.6 * S, 4.2 * S)
    fade = glow(np.hypot(XX - 0.12 * W, YY - 0.92 * H), 2500 * S, 1.8)
    paint(img, dots * fade, GRAPHITE, 1.0)
    paint(img, dots * glow(np.hypot(XX - 0.12 * W, YY - 0.92 * H), 900 * S, 2.0), BLUE, 0.70)
    return img


def ridges():
    img = canvas()
    rng = np.random.default_rng(31)
    layers = 5
    for i in range(layers):
        base = H * (0.58 + 0.085 * i)
        phase = rng.random(3) * 6.28
        amp = (150 - 18 * i) * S
        curve = base + amp * (
            0.55 * np.sin(XX / (W / (1.3 + 0.5 * i)) * 6.28 + phase[0])
            + 0.30 * np.sin(XX / (W / (3.1 + 0.7 * i)) * 6.28 + phase[1])
            + 0.15 * np.sin(XX / (W / (7.3 + 1.1 * i)) * 6.28 + phase[2])
        )
        below = YY >= curve
        edge = line(YY - curve, 1.5 * S, 3 * S)
        # Each ridge hides the ones behind it (true black body) with a thin, dim rim light.
        img *= (~below)[..., None]
        paint(img, edge, SILVER, 0.62 - 0.11 * i)
        paint(img, glow(np.abs(YY - curve), 70 * S, 2.2) * (~below), BLUE, 0.020)
    return img


WALLPAPERS = [
    # id, name, alt text / description, default fit, builder, dither seed
    ("moonrise", "Moonrise", "A large silver moon rising from the bottom right edge, with a thin lit rim and a faint cool glow around it.", "fill", moonrise, 101),
    ("eclipse", "Eclipse", "A pure black disc at the centre ringed by a thin silver edge and a soft corona.", "fit", eclipse, 102),
    ("window-light", "Window light", "A single thin blue window frame with a title band and three small controls, glowing faintly on black.", "fit", window_light, 103),
    ("horizon", "Horizon", "One fine silver horizon line with a dim blue haze rising above it.", "fill", horizon, 104),
    ("contour", "Contour", "Graphite topographic contour lines fading to black at the edges, with blue on the highest ground.", "fill", contour, 105),
    ("orbit", "Orbit", "Five tilted elliptical orbits around a small centre point, with a tiny silver moon on one of them.", "fit", orbit, 106),
    ("dot-matrix", "Dot matrix", "A fine grid of graphite dots fading out from the lower left corner, with a blue core.", "fill", dot_matrix, 107),
    ("ridges", "Ridges", "Five black ridge lines layered one behind another, each edged with a thin moonlit rim.", "fill", ridges, 108),
]


# ---------------------------------------------------------------------------------------------
# Checks, output, manifest
# ---------------------------------------------------------------------------------------------

def luma(a):
    f = a.astype(np.float32)
    return 0.2126 * f[..., 0] + 0.7152 * f[..., 1] + 0.0722 * f[..., 2]


def check_oled(name, pixels):
    """The design rules, as numbers. Returns (stats, list of broken rules)."""
    y = luma(pixels)
    stats = {
        "mean_luma": float(y.mean()),
        "black_share": float((pixels.max(axis=2) <= 2).mean()),
        "bright_share": float((y > 96).mean()),
        "peak_luma": float(y.max()),
    }
    broken = []
    if stats["black_share"] < 0.70:
        broken.append(f"only {stats['black_share']:.0%} of pixels are black (need 70%)")
    if stats["mean_luma"] > 6.0:
        broken.append(f"mean luminance {stats['mean_luma']:.1f}/255 is too bright (max 6)")
    # Thin lit strokes (a rim, a hairline) are allowed; a bright field is not. 0.6% of a 4K frame
    # is about 50,000 pixels, a few hairlines' worth.
    if stats["bright_share"] > 0.006:
        broken.append(f"{stats['bright_share']:.2%} of pixels are bright (max 0.6%)")
    return stats, broken


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def render_all(check):
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    THUMB_DIR.mkdir(parents=True, exist_ok=True)
    entries, failed = [], False
    for ident, name, description, fit, builder, seed in WALLPAPERS:
        pixels = to_8bit(builder(), seed)
        stats, broken = check_oled(name, pixels)
        file = f"allblack11-wallpaper-{ident}-3840x2160.png"
        image = Image.fromarray(pixels, "RGB")
        image.save(OUT_DIR / file, optimize=True, compress_level=9)
        thumb = image.resize(THUMB_SIZE, Image.LANCZOS)
        thumb.save(THUMB_DIR / f"{ident}.jpg", quality=86, optimize=True, progressive=False, subsampling=0)
        entries.append({
            "id": ident,
            "name": name,
            "description": description,
            "file": file,
            "width": W,
            "height": H,
            "bytes": (OUT_DIR / file).stat().st_size,
            "sha256": sha256_file(OUT_DIR / file),
            "fit": fit,
            "oled": {k: round(v, 4) for k, v in stats.items()},
        })
        status = "ok" if not broken else "BREAKS: " + "; ".join(broken)
        print(f"{ident:12s} {entries[-1]['bytes']/1024:8.0f} KB  black {stats['black_share']:.1%}  mean {stats['mean_luma']:.2f}  bright {stats['bright_share']:.3%}  {status}")
        failed |= bool(broken)
    manifest = {"schema": 1, "generator": "assets/wallpapers/generate.py", "wallpapers": entries}
    (HERE / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    total = sum(e["bytes"] for e in entries)
    print(f"total {total/1024/1024:.1f} MB in {OUT_DIR}")
    if check and failed:
        sys.exit("an image breaks the OLED rules")


def verify():
    manifest = json.loads((HERE / "manifest.json").read_text())
    bad = 0
    for entry in manifest["wallpapers"]:
        path = OUT_DIR / entry["file"]
        if not path.exists():
            print(f"missing  {entry['file']}")
            bad += 1
        elif sha256_file(path) != entry["sha256"]:
            print(f"DIFFERENT {entry['file']}")
            bad += 1
        else:
            print(f"ok       {entry['file']}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    if "--verify" in sys.argv:
        verify()
    render_all(check="--check" in sys.argv)
