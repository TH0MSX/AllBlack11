#!/usr/bin/env python3
"""README mockups, built from real AllBlack11 screenshots (Windows 11 ARM64, 3.8.0-preview.2).

    python3 docs/mockup-source/make_mockups.py       # writes docs/hero.png, docs/bento.png,
                                                     # docs/social-preview.png

The inputs are `shots/*.png`: window-only captures taken on a real PC, with the pop-ups and
anything personal cropped or blacked out before they were committed (the raw captures are not
kept). Nothing here invents UI: windows are only scaled, rounded, lit and arranged. Needs numpy and
Pillow; the font is Inter (SIL OFL), if it is installed, else a default.

Style: pitch-black stage, floating glass-edged windows with a thin rim light, one cool glow under
the hero, depth by scale, tilt and blur, large radii, a bento grid for the details.
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
SHOTS = HERE / "shots"
OUT = HERE.parent
FONTS = Path("/usr/share/fonts/opentype/inter")
BLUE = (91, 180, 240)


def font(name, size):
    path = FONTS / name
    try:
        return ImageFont.truetype(str(path), size)
    except OSError:
        return ImageFont.load_default()


def shot(name):
    return Image.open(SHOTS / name).convert("RGBA")


def rounded_mask(size, radius, scale=4):
    big = Image.new("L", (size[0] * scale, size[1] * scale), 0)
    ImageDraw.Draw(big).rounded_rectangle((0, 0, big.width - 1, big.height - 1), radius * scale, fill=255)
    return big.resize(size, Image.LANCZOS)


def rounded(im, radius):
    im = im.copy()
    mask = rounded_mask(im.size, radius)
    alpha = ImageChops.multiply(im.getchannel("A"), mask)
    im.putalpha(alpha)
    return im


def rim(im, radius, strength=0.5):
    """A one pixel light edge, brighter along the top (light from above)."""
    w, h = im.size
    mask = rounded_mask(im.size, radius)
    inner = rounded_mask((w - 2, h - 2), max(radius - 1, 1))
    inner_full = Image.new("L", im.size, 0)
    inner_full.paste(inner, (1, 1))
    edge = ImageChops.subtract(mask, inner_full)
    ramp = np.linspace(255 * strength, 255 * strength * 0.18, h, dtype=np.float32)[:, None]
    ramp = np.repeat(ramp, w, axis=1)
    light = Image.new("RGBA", im.size, (200, 220, 245, 0))
    light.putalpha(Image.fromarray((np.asarray(edge, dtype=np.float32) / 255 * ramp).astype(np.uint8)))
    return Image.alpha_composite(im, light)


def fade(im, right=0, bottom=0, left=0, top=0):
    """Fade chosen edges to transparent, so a crop looks like a window continuing off the tile."""
    w, h = im.size
    a = np.ones((h, w), dtype=np.float32)
    ramp = lambda n: np.linspace(0, 1, n, dtype=np.float32) ** 1.6
    if right:
        a[:, w - right :] *= ramp(right)[::-1][None, :]
    if left:
        a[:, :left] *= ramp(left)[None, :]
    if bottom:
        a[h - bottom :, :] *= ramp(bottom)[::-1][:, None]
    if top:
        a[:top, :] *= ramp(top)[:, None]
    out = im.copy()
    alpha = np.asarray(out.getchannel("A"), dtype=np.float32) * a
    out.putalpha(Image.fromarray(alpha.astype(np.uint8)))
    return out


def tilt(im, yaw, pad=40):
    """A small perspective turn about the vertical axis (positive: the right side moves away)."""
    w, h = im.size
    k = np.tan(np.radians(abs(yaw))) * 0.18
    far = h * k
    if yaw >= 0:
        dst = [(0, 0), (w, far), (w, h - far), (0, h)]
    else:
        dst = [(0, far), (w, 0), (w, h), (0, h - far)]
    src = [(0, 0), (w, 0), (w, h), (0, h)]
    # Coefficients mapping destination points back to the source rectangle.
    rows, vals = [], []
    for (x, y), (u, v) in zip(dst, src):
        rows.append([x, y, 1, 0, 0, 0, -u * x, -u * y])
        rows.append([0, 0, 0, x, y, 1, -v * x, -v * y])
        vals.extend([u, v])
    coeffs = np.linalg.solve(np.array(rows, dtype=np.float64), np.array(vals, dtype=np.float64))
    padded = Image.new("RGBA", (w + 2 * pad, h + 2 * pad), (0, 0, 0, 0))
    padded.paste(im, (pad, pad))
    # The padded canvas shifts the destination quad by the padding.
    shifted = [(x + pad, y + pad) for x, y in dst]
    rows, vals = [], []
    for (x, y), (u, v) in zip(shifted, [(u + pad, v + pad) for u, v in src]):
        rows.append([x, y, 1, 0, 0, 0, -u * x, -u * y])
        rows.append([0, 0, 0, x, y, 1, -v * x, -v * y])
        vals.extend([u, v])
    coeffs = np.linalg.solve(np.array(rows, dtype=np.float64), np.array(vals, dtype=np.float64))
    return padded.transform(padded.size, Image.PERSPECTIVE, tuple(coeffs), Image.BICUBIC)


def glow_under(canvas, box, color=BLUE, alpha=0.20, blur=90, grow=40):
    layer = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    l, t, r, b = box
    ImageDraw.Draw(layer).rounded_rectangle((l - grow, t - grow // 2, r + grow, b + grow), 70, fill=color + (int(255 * alpha),))
    layer = layer.filter(ImageFilter.GaussianBlur(blur))
    canvas.alpha_composite(layer)


def reflection(canvas, im, x, y, height=170, alpha=0.20):
    flipped = im.transpose(Image.FLIP_TOP_BOTTOM).crop((0, 0, im.width, height))
    ramp = np.linspace(alpha, 0, height, dtype=np.float32) ** 1.4
    a = np.asarray(flipped.getchannel("A"), dtype=np.float32) * ramp[:, None]
    flipped.putalpha(Image.fromarray(a.astype(np.uint8)))
    flipped = flipped.filter(ImageFilter.GaussianBlur(1.4))
    canvas.alpha_composite(flipped, (x, y + im.height + 8))


def stage(size, glow=(0.5, 0.45), seed=7):
    """True black with a barely-there cool light and fine grain so the dark does not band."""
    w, h = size
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    d = np.hypot((xx / w - glow[0]) / 0.62, (yy / h - glow[1]) / 0.55)
    light = np.clip(1 - d, 0, 1) ** 2.2
    base = np.zeros((h, w, 4), dtype=np.float32)
    base[..., 0] = light * 9
    base[..., 1] = light * 20
    base[..., 2] = light * 34
    base[..., 3] = 255
    rng = np.random.default_rng(seed)
    base[..., :3] += rng.normal(0, 0.9, (h, w, 1)).clip(-1.5, 1.5) * (light[..., None] > 0.01)
    return Image.fromarray(np.clip(base, 0, 255).astype(np.uint8), "RGBA")


def prepare(name, width, radius=22, yaw=0, blur=0.0, opacity=1.0, crop=None, edge_fade=None):
    im = shot(name)
    if crop:
        im = im.crop(crop)
    height = round(im.height * width / im.width)
    im = im.resize((width, height), Image.LANCZOS)
    im = rounded(im, radius)
    im = rim(im, radius)
    if edge_fade:
        im = fade(im, **edge_fade)
    if yaw:
        im = tilt(im, yaw)
    if blur:
        im = im.filter(ImageFilter.GaussianBlur(blur))
    if opacity < 1:
        a = np.asarray(im.getchannel("A"), dtype=np.float32) * opacity
        im.putalpha(Image.fromarray(a.astype(np.uint8)))
    return im


def hero():
    W, H = 1800, 1020
    canvas = stage((W, H), glow=(0.5, 0.5))
    # Far left and far right: other pages, turned away and softened.
    left = prepare("apps.png", 800, yaw=14, blur=1.1, opacity=0.78)
    right = prepare("wallpapers.png", 840, yaw=-14, blur=1.1, opacity=0.82)
    canvas.alpha_composite(left, (-40, 150))
    canvas.alpha_composite(right, (W - right.width + 50, 220))
    # The hero window, lit from below by a cool glow.
    main = prepare("home.png", 1060, radius=26)
    mx, my = (W - main.width) // 2, 100
    glow_under(canvas, (mx, my, mx + main.width, my + main.height), alpha=0.20, blur=110, grow=60)
    glow_under(canvas, (mx + 120, my + main.height - 80, mx + main.width - 120, my + main.height + 30), alpha=0.35, blur=60, grow=0)
    reflection(canvas, main, mx, my, height=200, alpha=0.16)
    canvas.alpha_composite(main, (mx, my))
    canvas.convert("RGB").save(OUT / "hero.png", optimize=True)


def wrap(draw, text, fnt, width):
    lines, line = [], ""
    for word in text.split():
        trial = (line + " " + word).strip()
        if draw.textlength(trial, font=fnt) <= width or not line:
            line = trial
        else:
            lines.append(line)
            line = word
    if line:
        lines.append(line)
    return lines[:2]


def tile(canvas, box, title, line, image, src, inset=40, fade_right=0):
    """One bento tile: a headline, one or two lines under it, and part of a real screenshot.

    `src` is (x, y, width) in the screenshot; the height shown is whatever fills the tile."""
    l, t, r, b = box
    w, h = r - l, b - t
    layer = Image.new("RGBA", (w, h), (7, 7, 8, 255))
    sheen = np.linspace(0.07, 0.0, h, dtype=np.float32)[:, None]
    over = Image.new("RGBA", (w, h), (170, 195, 230, 0))
    over.putalpha(Image.fromarray((np.repeat(sheen, w, axis=1) * 255).astype(np.uint8)))
    layer = Image.alpha_composite(layer, over)
    d = ImageDraw.Draw(layer)
    d.text((inset, 34), title, font=font("InterDisplay-SemiBold.otf", 34), fill=(244, 247, 252, 255))
    sub = font("InterDisplay-Regular.otf", 21)
    lines = wrap(d, line, sub, w - 2 * inset)
    for i, text in enumerate(lines):
        d.text((inset, 84 + i * 28), text, font=sub, fill=(140, 148, 162, 255))
    area_top = 84 + len(lines) * 28 + 24
    area_h = h - area_top - 0
    sx, sy, sw = src
    full = shot(image)
    iw = w - 2 * inset
    scale = iw / sw
    sh = min(full.height - sy, int(area_h / scale) + 1)
    piece = full.crop((sx, sy, min(sx + sw, full.width), sy + sh))
    piece = piece.resize((round(piece.width * scale), round(piece.height * scale)), Image.LANCZOS)
    piece = rounded(piece, 16)
    piece = rim(piece, 16, strength=0.4)
    piece = fade(piece, bottom=70 if piece.height >= area_h - 2 else 0, right=fade_right)
    layer.alpha_composite(piece, (inset, area_top))
    layer = rounded(layer, 34)
    layer = rim(layer, 34, strength=0.22)
    canvas.alpha_composite(layer, (l, t))


def bento():
    g, m = 20, 50
    colw = 566
    W = 3 * colw + 2 * g + 2 * m
    rows = [520, 360, 360]
    H = 2 * m + sum(rows) + 2 * g
    canvas = stage((W, H), glow=(0.5, 0.4), seed=11)
    x0, x1, x2 = m, m + colw + g, m + 2 * (colw + g)
    ys = [m, m + rows[0] + g, m + rows[0] + rows[1] + 2 * g]
    wide = 2 * colw + g
    tile(canvas, (x0, ys[0], x0 + wide, ys[0] + rows[0]), "Six clear categories",
         "Home, Appearance, Apps, Safety, Settings, Help and About. Search sits above them.",
         "home.png", (0, 0, 1383))
    tile(canvas, (x2, ys[0], x2 + colw, ys[0] + rows[0]), "Wallpapers made for OLED",
         "Eight original, true-black wallpapers. Preview, then apply.",
         "wallpapers.png", (0, 0, 1380))
    tile(canvas, (x0, ys[1], x0 + colw, ys[1] + rows[1]), "Try it for 15 seconds",
         "Keep it, or it goes back by itself.",
         "effects-left.png", (0, 0, 1170), fade_right=50)
    tile(canvas, (x1, ys[1], x1 + wide, ys[1] + rows[1]), "An undo that names itself",
         "Look history says what changed and when. Undo steps back one change at a time.",
         "safety-left.png", (262, 600, 908), fade_right=90)
    tile(canvas, (x0, ys[2], x0 + wide, ys[2] + rows[2]), "Updates only when you say so",
         "Check, read, download, verify the checksum, then install. Nothing runs in the background.",
         "settings-left.png", (262, 560, 908), fade_right=90)
    tile(canvas, (x2, ys[2], x2 + colw, ys[2] + rows[2]), "Customize one app",
         "Pick a window, see what can change, and why.",
         "apps.png", (0, 0, 1383))
    canvas.convert("RGB").save(OUT / "bento.png", optimize=True)


def social():
    W, H = 1280, 640
    canvas = stage((W, H), glow=(0.72, 0.5), seed=3)
    main = prepare("home.png", 760, radius=22, yaw=-9)
    x, y = W - main.width + 80, (H - main.height) // 2 + 8
    glow_under(canvas, (x + 60, y + 40, x + main.width - 60, y + main.height - 40), alpha=0.22, blur=80, grow=30)
    canvas.alpha_composite(main, (x, y))
    logo = Image.open(HERE.parent.parent / "assets" / "brand" / "png" / "allblack11-512.png").convert("RGBA").resize((104, 104), Image.LANCZOS)
    logo_glow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    logo_glow.paste(Image.new("RGBA", (104, 104), BLUE + (150,)), (84, 118))
    canvas.alpha_composite(logo_glow.filter(ImageFilter.GaussianBlur(34)))
    canvas.alpha_composite(rounded(logo, 24), (84, 118))
    d = ImageDraw.Draw(canvas)
    d.text((80, 250), "AllBlack11", font=font("InterDisplay-SemiBold.otf", 104), fill=(246, 248, 252, 255))
    d.text((84, 388), "Pitch-black Windows 11.", font=font("InterDisplay-Medium.otf", 34), fill=(222, 228, 240, 255))
    d.text((84, 436), "Native ARM64 and x64. Free and open source.", font=font("InterDisplay-Regular.otf", 26), fill=(130, 140, 158, 255))
    canvas.convert("RGB").save(OUT / "social-preview.png", optimize=True)


if __name__ == "__main__":
    which = sys.argv[1:] or ["hero", "bento", "social"]
    for name in which:
        {"hero": hero, "bento": bento, "social": social}[name]()
        print("wrote", name)
