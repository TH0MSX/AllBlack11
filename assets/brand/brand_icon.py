#!/usr/bin/env python3
"""AllBlack11 app icon: one geometry definition, rendered to SVG and to pixel-tuned rasters.

Design direction "moonlit precision": a silver crescent whose inner curve is bitten out by a
window, on a graphite plate. Two metaphors only (moon, window), straight-on, no lettering.

The geometry lives in CONCEPTS as a declarative list of layers on the recommended 48x48 grid
(https://learn.microsoft.com/en-us/windows/apps/design/style/iconography/app-icon-design).
`render()` rasterizes it with signed-distance fields and 8x supersampling; `to_svg()` writes the
same layers as SVG, so the vector source and the raster sizes cannot drift apart.

    python3 assets/brand/brand_icon.py build      # SVG, ICOs, PNGs (see OUT below)
    python3 assets/brand/brand_icon.py concepts   # contact sheet used to choose the direction
    python3 assets/brand/brand_icon.py check      # legibility sheet + alpha/edge checks

Only numpy and Pillow are needed. Nothing here is downloaded or derived from another icon.
"""
import io
import math
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent

GRID = 48.0
SS = 8  # supersampling factor

# --- palette (see docs/dev/BRAND.md for the contrast figures) --------------------------------
PLATE_TOP = (0x1D, 0x22, 0x2B)
PLATE_BOTTOM = (0x07, 0x08, 0x0B)
PLATE_EDGE = (0x3A, 0x41, 0x4D)
SILVER_LIGHT = (0xF4, 0xF6, 0xFA)
SILVER_DARK = (0xA9, 0xB3, 0xC2)
ACCENT = (0x5B, 0xB4, 0xF0)  # restrained cool blue
ACCENT_DEEP = (0x2F, 0x7F, 0xB8)


# --- shapes (signed distance, negative inside) ------------------------------------------------
def circle(cx, cy, r):
    return ("circle", cx, cy, r)


def rrect(x0, y0, x1, y1, r):
    return ("rrect", x0, y0, x1, y1, r)


def sdf(shape, X, Y):
    kind = shape[0]
    if kind == "circle":
        _, cx, cy, r = shape
        return np.hypot(X - cx, Y - cy) - r
    _, x0, y0, x1, y1, r = shape
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    hx, hy = (x1 - x0) / 2 - r, (y1 - y0) / 2 - r
    qx, qy = np.abs(X - cx) - hx, np.abs(Y - cy) - hy
    return np.hypot(np.maximum(qx, 0), np.maximum(qy, 0)) + np.minimum(np.maximum(qx, qy), 0) - r


# --- layers -----------------------------------------------------------------------------------
def fill(shape, paint, cut=(), grow=0.0):
    return {"op": "fill", "shape": shape, "paint": paint, "cut": tuple(cut), "grow": grow,
            "clip": ()}


def stroke(shape, paint, width, cut=(), clip=()):
    return {"op": "stroke", "shape": shape, "paint": paint, "width": width, "cut": tuple(cut),
            "clip": tuple(clip)}


def solid(rgb, a=1.0):
    return ("solid", rgb, a)


def vgrad(top, bottom, y0=0.0, y1=GRID):
    return ("vgrad", top, bottom, y0, y1)


def dgrad(a, b, x0, y0, x1, y1):
    return ("dgrad", a, b, x0, y0, x1, y1)


# --- concepts ----------------------------------------------------------------------------------
PLATE = rrect(2, 2, 46, 46, 10)


def concept_a(px):
    """Bitten moon: a disc with a window-shaped bite taken out of its upper right."""
    s = max(1.0, round(2.0 * px)) / px  # window stroke, at least one device pixel
    win = rrect(24.5, 10.5, 40.5, 27.5, 2.8)
    layers = [
        fill(PLATE, vgrad(PLATE_TOP, PLATE_BOTTOM)),
        fill(circle(21, 26.5, 14.5), dgrad(SILVER_LIGHT, SILVER_DARK, 8, 12, 33, 40),
             cut=[rrect(23.0, 9.0, 42.0, 29.0, 4.2)]),
        stroke(win, solid(ACCENT), s),
    ]
    if px >= 0.5:  # 24 px and up: a title-bar rule makes the bite read as a window
        layers.append(fill(rrect(24.5, 10.5, 40.5, 10.5 + max(3.4, 1.2 / px * 2), 2.8),
                           solid(ACCENT), grow=0))
    return layers


def concept_b(px):
    """Eclipse in a window frame: silver rounded frame, dark disc, thin corona."""
    s = max(1.0, round(2.4 * px)) / px
    return [
        fill(PLATE, vgrad(PLATE_TOP, PLATE_BOTTOM)),
        stroke(rrect(9, 9, 39, 39, 5), solid(SILVER_LIGHT), s),
        fill(circle(24, 25, 8.5), dgrad(SILVER_LIGHT, SILVER_DARK, 15, 16, 33, 34)),
        fill(circle(27, 22, 7.6), solid((0x0B, 0x0D, 0x12))),
    ]


def concept_c(px):
    """Window with a moon in it: title band on top, crescent below."""
    return [
        fill(PLATE, vgrad(PLATE_TOP, PLATE_BOTTOM)),
        fill(rrect(8, 8, 40, 40, 4.5), solid((0x14, 0x17, 0x1D))),
        fill(rrect(8, 8, 40, 16, 4.5), solid(ACCENT_DEEP), cut=[rrect(8, 12, 40, 17, 0)]),
        fill(rrect(8, 12, 40, 16, 0), solid(ACCENT_DEEP)),
        fill(circle(23, 28, 8.2), dgrad(SILVER_LIGHT, SILVER_DARK, 15, 20, 31, 36),
             cut=[circle(27, 25, 7.2)]),
    ]


def concept_d(px):
    """Bitten moon without the accent window outline (silhouette only)."""
    return [
        fill(PLATE, vgrad(PLATE_TOP, PLATE_BOTTOM)),
        fill(circle(22, 25, 15), dgrad(SILVER_LIGHT, SILVER_DARK, 8, 10, 36, 40),
             cut=[rrect(24, 8, 44, 28, 4.5)]),
    ]


def concept_e(px):
    """The plate is the window: title band, crescent in the content area."""
    return [
        fill(PLATE, vgrad((0x16, 0x19, 0x20), PLATE_BOTTOM)),
        fill(rrect(2, 2, 46, 14, 10), solid((0x24, 0x2A, 0x35)),
             cut=[rrect(0, 12, 48, 20, 0)]),
        fill(rrect(2, 6, 46, 14, 0), solid((0x24, 0x2A, 0x35))),
        fill(circle(23, 29, 11.5), dgrad(SILVER_LIGHT, SILVER_DARK, 12, 18, 34, 40),
             cut=[circle(29, 25, 10)]),
    ]


def concept_f(px):
    """Moon bitten by a window corner, with a cool rim light along the bite."""
    s = max(1.0, round(1.7 * px)) / px
    bite = rrect(24, 8, 44, 28, 4.5)
    moon = circle(22, 25, 15)
    return [
        fill(PLATE, vgrad(PLATE_TOP, PLATE_BOTTOM)),
        fill(moon, dgrad(SILVER_LIGHT, SILVER_DARK, 8, 10, 36, 40), cut=[bite]),
        stroke(rrect(24, 8, 44, 28, 4.5), solid(ACCENT), s, clip=[circle(22, 25, 17.6)],
               cut=[rrect(24, 8, 44, 28, 4.5)]),
    ]


def concept_g(px):
    """Like F but the window is drawn complete, small, fully inside the bite."""
    s = max(1.0, round(1.7 * px)) / px
    bite = rrect(24, 8, 44, 28, 4.5)
    return [
        fill(PLATE, vgrad(PLATE_TOP, PLATE_BOTTOM)),
        fill(circle(22, 25, 15), dgrad(SILVER_LIGHT, SILVER_DARK, 8, 10, 36, 40), cut=[bite]),
        stroke(rrect(27.5, 11.5, 40.5, 24.5, 2.4), solid(ACCENT), s),
    ]


# --- the chosen direction ---------------------------------------------------------------------
MOON = circle(22, 25, 15)
BITE = rrect(24, 8, 44, 28, 4.5)
WINDOW = (27.5, 11.5, 40.5, 24.5, 2.4)


def _snap_center(v, unit, width_px):
    """Put a stroke centre where a stroke `width_px` wide covers whole device pixels."""
    if width_px % 2 == 1:
        return (math.floor(v / unit) + 0.5) * unit
    return round(v / unit) * unit


def moon_window(px):
    """FINAL. A silver moon bitten by a window-shaped bite; a thin cool window frame sits in the
    bite, so the window eclipses the moon. `px` is device pixels per grid unit."""
    unit = 1.0 / px
    width_px = max(1, round(1.7 * px * 1.0))
    sw = width_px * unit
    x0, y0, x1, y1, r = WINDOW
    if px < 0.8:  # 16 to 36 px: snap the frame to whole pixels so its edges stay crisp
        x0, x1, y0, y1 = (_snap_center(v, unit, width_px) for v in (x0, x1, y0, y1))
    layers = [fill(PLATE, vgrad(PLATE_TOP, PLATE_BOTTOM))]
    if px * GRID >= 20:  # a one-pixel rim keeps the plate visible on a dark desktop
        edge = unit
        layers.append(stroke(rrect(2 + edge / 2, 2 + edge / 2, 46 - edge / 2, 46 - edge / 2,
                                   10 - edge / 2), solid(PLATE_EDGE, 0.9), edge))
    layers += [
        fill(MOON, dgrad(SILVER_LIGHT, SILVER_DARK, 8, 10, 36, 40), cut=[BITE]),
        stroke(rrect(x0, y0, x1, y1, r), solid(ACCENT), sw),
    ]
    return layers


def _scaled(shape, k, dx, dy):
    if shape[0] == "circle":
        _, cx, cy, r = shape
        return circle(cx * k + dx, cy * k + dy, r * k)
    _, a, b, c, d, r = shape
    return rrect(a * k + dx, b * k + dy, c * k + dx, d * k + dy, r * k)


def glyph(px, rgb, on=True):
    """Single-colour mark for the tray and in-app use: the same moon and window, no plate.
    `on` is a filled moon; off (paused) is the moon's outline, so state is shape, not colour."""
    k = 1.3
    dx, dy = 24 - 23.75 * k, 24 - 25 * k
    unit = 1.0 / px
    width_px = max(1, round(1.6 * px * k))
    sw = width_px * unit
    moon, bite = _scaled(MOON, k, dx, dy), _scaled(BITE, k, dx, dy)
    x0, y0, x1, y1, r = WINDOW
    win = _scaled(rrect(x0, y0, x1, y1, r), k, dx, dy)
    if px < 0.8:
        _, a, b, c, d, rr = win
        a, c, b, d = (_snap_center(v, unit, width_px) for v in (a, c, b, d))
        win = rrect(a, b, c, d, rr)
    body = fill(moon, solid(rgb), cut=[bite]) if on else outline(moon, solid(rgb), sw, cut=[bite])
    return [body, stroke(win, solid(rgb), sw)]


def outline(shape, paint, width, cut=()):
    """Stroke of the outline of `shape` minus `cut` (not of the cut lines)."""
    return {"op": "outline", "shape": shape, "paint": paint, "width": width, "cut": tuple(cut),
            "clip": ()}


CONCEPTS = {"A": concept_a, "B": concept_b, "C": concept_c, "D": concept_d, "E": concept_e,
            "F": concept_f, "G": moon_window}
FINAL = "G"

# --- rasterizer --------------------------------------------------------------------------------
def _paint(paint, X, Y):
    kind = paint[0]
    if kind == "solid":
        _, rgb, a = paint
        out = np.zeros(X.shape + (4,), np.float64)
        out[..., :3] = rgb
        out[..., 3] = a
        return out
    if kind == "vgrad":
        _, top, bottom, y0, y1 = paint
        t = np.clip((Y - y0) / (y1 - y0), 0, 1)[..., None]
        rgb = np.array(top) * (1 - t) + np.array(bottom) * t
    else:
        _, a, b, x0, y0, x1, y1 = paint
        dx, dy = x1 - x0, y1 - y0
        t = np.clip(((X - x0) * dx + (Y - y0) * dy) / (dx * dx + dy * dy), 0, 1)[..., None]
        rgb = np.array(a) * (1 - t) + np.array(b) * t
    out = np.ones(X.shape + (4,), np.float64)
    out[..., :3] = rgb
    return out


def render(layers, size, plate=True):
    n = size * SS
    ys, xs = np.mgrid[0:n, 0:n]
    X = (xs + 0.5) / n * GRID
    Y = (ys + 0.5) / n * GRID
    canvas = np.zeros((n, n, 4), np.float64)  # premultiplied
    for layer in layers:
        d = sdf(layer["shape"], X, Y)
        if layer["op"] == "fill":
            mask = d <= layer.get("grow", 0.0)
            for c in layer["cut"]:
                mask &= sdf(c, X, Y) > 0
        elif layer["op"] == "outline":
            for c in layer["cut"]:
                d = np.maximum(d, -sdf(c, X, Y))
            mask = np.abs(d) <= layer["width"] / 2
        else:
            mask = np.abs(d) <= layer["width"] / 2
            for c in layer["cut"]:
                mask &= sdf(c, X, Y) > 0
        for c in layer["clip"]:
            mask &= sdf(c, X, Y) <= 0
        src = _paint(layer["paint"], X, Y)
        a = (mask * src[..., 3])[..., None]
        rgb = src[..., :3] / 255.0
        canvas[..., :3] = rgb * a + canvas[..., :3] * (1 - a)
        canvas[..., 3:] = a + canvas[..., 3:] * (1 - a)
    small = canvas.reshape(size, SS, size, SS, 4).mean(axis=(1, 3))
    alpha = small[..., 3]
    rgb = np.where(alpha[..., None] > 1e-6, small[..., :3] / np.maximum(alpha[..., None], 1e-6), 0)
    out = np.dstack([np.clip(rgb * 255 + 0.5, 0, 255), np.clip(alpha * 255 + 0.5, 0, 255)])
    return Image.fromarray(out.astype(np.uint8), "RGBA")


def layers_for(name, size):
    return CONCEPTS[name](size / GRID)


def on_background(img, rgb):
    bg = Image.new("RGBA", img.size, rgb + (255,))
    bg.alpha_composite(img)
    return bg


def sheet(names, sizes, backgrounds, path, scale=4):
    """Contact sheet: one row per concept, columns = sizes, on each background."""
    cell = max(sizes) * scale // 2 + 16
    rows = []
    for name in names:
        for bg in backgrounds:
            row = Image.new("RGBA", (cell * len(sizes), cell), bg + (255,))
            x = 8
            for size in sizes:
                im = render(layers_for(name, size), size)
                zoom = im.resize((size * scale // 2 or size, size * scale // 2 or size), Image.NEAREST)
                row.alpha_composite(zoom, (x, (cell - zoom.height) // 2))
                x += zoom.width + 8
            rows.append(row)
    w = max(r.width for r in rows)
    out = Image.new("RGBA", (w, sum(r.height for r in rows)), (128, 128, 128, 255))
    y = 0
    for r in rows:
        out.alpha_composite(r, (0, y))
        y += r.height
    out.convert("RGB").save(path)


# --- SVG ---------------------------------------------------------------------------------------
def _hex(rgb):
    return "#%02X%02X%02X" % tuple(rgb)


def _svg_shape(shape, extra=""):
    if shape[0] == "circle":
        _, cx, cy, r = shape
        return f'<circle cx="{cx:g}" cy="{cy:g}" r="{r:g}"{extra}/>'
    _, x0, y0, x1, y1, r = shape
    return (f'<rect x="{x0:g}" y="{y0:g}" width="{x1 - x0:g}" height="{y1 - y0:g}" '
            f'rx="{r:g}"{extra}/>')


def to_svg(layers, title="AllBlack11"):
    defs, body = [], []
    for i, layer in enumerate(layers):
        paint = layer["paint"]
        first = len(body)
        if paint[0] == "solid":
            color = _hex(paint[1])
        else:
            gid = f"g{i}"
            if paint[0] == "vgrad":
                _, top, bottom, y0, y1 = paint
                coords = f'x1="0" y1="{y0:g}" x2="0" y2="{y1:g}"'
                stops = (top, bottom)
            else:
                _, a, b, x0, y0, x1, y1 = paint
                coords = f'x1="{x0:g}" y1="{y0:g}" x2="{x1:g}" y2="{y1:g}"'
                stops = (a, b)
            defs.append(f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" {coords}>'
                        f'<stop offset="0" stop-color="{_hex(stops[0])}"/>'
                        f'<stop offset="1" stop-color="{_hex(stops[1])}"/></linearGradient>')
            color = f"url(#{gid})"
        if layer["op"] == "fill":
            attrs = f' fill="{color}"'
            if layer["cut"]:
                mid = f"m{i}"
                cuts = "".join(_svg_shape(c, ' fill="#000"') for c in layer["cut"])
                defs.append(f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="0" y="0" width="48" '
                            f'height="48">{_svg_shape(layer["shape"], chr(32) + "fill=" + chr(34) + "#fff" + chr(34))}'
                            f'{cuts}</mask>')
                body.append(f'<rect width="48" height="48" fill="{color}" mask="url(#{mid})"/>')
            else:
                body.append(_svg_shape(layer["shape"], attrs))
        elif layer["op"] == "outline":
            mid = f"m{i}"
            cuts = "".join(_svg_shape(c, ' fill="#000"') for c in layer["cut"])
            # Outline of (shape minus cut): stroke the shape, then erase the part of that stroke
            # that lies inside the cut region, and stroke the cut where it meets the shape.
            shape_svg = _svg_shape(layer["shape"], f' fill="none" stroke="{color}" '
                                   f'stroke-width="{layer["width"]:g}"')
            cut_svg = "".join(_svg_shape(c, f' fill="none" stroke="{color}" '
                                         f'stroke-width="{layer["width"]:g}"') for c in layer["cut"])
            clip_in = _svg_shape(layer["shape"], ' fill="#fff"')
            defs.append(f'<mask id="{mid}a" maskUnits="userSpaceOnUse" x="0" y="0" width="48" '
                        f'height="48"><rect width="48" height="48" fill="#fff"/>'
                        f'{"".join(_svg_shape(c, chr(32) + "fill=" + chr(34) + "#000" + chr(34)) for c in layer["cut"])}'
                        f'</mask>')
            defs.append(f'<mask id="{mid}b" maskUnits="userSpaceOnUse" x="0" y="0" width="48" '
                        f'height="48"><rect width="48" height="48" fill="#000"/>{clip_in}</mask>')
            body.append(f'<g mask="url(#{mid}a)">{shape_svg}</g>')
            body.append(f'<g mask="url(#{mid}b)">{cut_svg}</g>')
        else:
            body.append(_svg_shape(layer["shape"], f' fill="none" stroke="{color}" '
                                   f'stroke-width="{layer["width"]:g}"'))
        if paint[0] == "solid" and paint[2] < 1:
            body[first:] = [f'<g opacity="{paint[2]:g}">{"".join(body[first:])}</g>']
    return ('<?xml version="1.0" encoding="UTF-8"?>\n'
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" width="256" height="256" '
            f'role="img" aria-label="{title}"><title>{title}</title><defs>{"".join(defs)}</defs>'
            f'{"".join(body)}</svg>\n')


# --- ICO and PNG -------------------------------------------------------------------------------
APP_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
TRAY_SIZES = [16, 20, 24, 32, 40, 48, 64]
MONO_WHITE = (0xFF, 0xFF, 0xFF)
MONO_BLACK = (0x14, 0x14, 0x14)


def png_bytes(img):
    buf = io.BytesIO()
    img.save(buf, "PNG", optimize=True)
    return buf.getvalue()


def write_ico(path, images):
    """An ICO whose every entry is its own pixel-tuned PNG (Windows Vista and later)."""
    blobs = [(im.width, png_bytes(im)) for im in images]
    out = struct.pack("<HHH", 0, 1, len(blobs))
    offset = 6 + 16 * len(blobs)
    for size, blob in blobs:
        out += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    path.write_bytes(out + b"".join(blob for _, blob in blobs))


def app_images():
    return [render(layers_for(FINAL, n), n) for n in APP_SIZES]


def glyph_images(rgb, on):
    return [render(glyph(n / GRID, rgb, on), n) for n in TRAY_SIZES]


# resource ids in assets/allblack11.rc
OUT = {
    "ico": ROOT / "assets" / "allblack11.ico",  # 1: the app icon
    "tray_white_on": ROOT / "assets" / "brand" / "tray-white-on.ico",  # 10: dark taskbar
    "tray_white_off": ROOT / "assets" / "brand" / "tray-white-off.ico",  # 11
    "tray_black_on": ROOT / "assets" / "brand" / "tray-black-on.ico",  # 12: light taskbar
    "tray_black_off": ROOT / "assets" / "brand" / "tray-black-off.ico",  # 13
    "svg": HERE / "allblack11-icon.svg",
    "svg_mono": HERE / "allblack11-mark-mono.svg",
    "png_dir": HERE / "png",
}


def build():
    OUT["png_dir"].mkdir(exist_ok=True)
    OUT["svg"].write_text(to_svg(layers_for(FINAL, 48)), encoding="utf-8")
    OUT["svg_mono"].write_text(to_svg(glyph(1.0, (0, 0, 0), True), "AllBlack11 mark"),
                               encoding="utf-8")
    write_ico(OUT["ico"], app_images())
    write_ico(OUT["tray_white_on"], glyph_images(MONO_WHITE, True))
    write_ico(OUT["tray_white_off"], glyph_images(MONO_WHITE, False))
    write_ico(OUT["tray_black_on"], glyph_images(MONO_BLACK, True))
    write_ico(OUT["tray_black_off"], glyph_images(MONO_BLACK, False))
    for n in (16, 24, 32, 48, 64, 128, 256, 512):
        (OUT["png_dir"] / f"allblack11-{n}.png").write_bytes(png_bytes(render(layers_for(FINAL, n), n)))
    for name, rgb in (("white", MONO_WHITE), ("black", MONO_BLACK)):
        for n in (16, 24, 32):
            im = render(glyph(n / GRID, rgb, True), n)
            (OUT["png_dir"] / f"allblack11-mark-{name}-{n}.png").write_bytes(png_bytes(im))
    print("built", ", ".join(str(v.relative_to(ROOT)) for k, v in OUT.items() if k != "png_dir"))


# --- checks ------------------------------------------------------------------------------------
def luminance(rgb):
    def f(c):
        c /= 255.0
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = rgb
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)


def contrast(a, b):
    la, lb = sorted((luminance(a), luminance(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def check():
    problems = []
    plate_mid = tuple((a + b) // 2 for a, b in zip(PLATE_TOP, PLATE_BOTTOM))
    figures = {
        "silver on plate": contrast(SILVER_DARK, PLATE_TOP),
        "accent on plate": contrast(ACCENT, PLATE_TOP),
        "plate edge vs dark desktop (#202020)": contrast(PLATE_EDGE, (0x20, 0x20, 0x20)),
        "plate vs light desktop (#F3F3F3)": contrast(plate_mid, (0xF3, 0xF3, 0xF3)),
        "mono white on dark taskbar (#202020)": contrast(MONO_WHITE, (0x20, 0x20, 0x20)),
        "mono black on light taskbar (#F3F3F3)": contrast(MONO_BLACK, (0xF3, 0xF3, 0xF3)),
    }
    for name, value in figures.items():
        print(f"{value:5.2f}:1  {name}")
        if value < 3.0 and "plate edge" not in name:
            problems.append(name)
    for n in APP_SIZES:
        im = render(layers_for(FINAL, n), n)
        a = np.array(im)[..., 3]
        corner = a[0, 0]
        opaque = (a == 255).mean()
        print(f"app {n:3d}px  corner alpha {corner:3d}  opaque {opaque:.2f}")
        if corner != 0:
            problems.append(f"{n}px corner is not transparent")
    for rgb in (MONO_WHITE, MONO_BLACK):
        for on in (True, False):
            for n in TRAY_SIZES:
                a = np.array(render(glyph(n / GRID, rgb, on), n))[..., 3]
                if a.max() == 0:
                    problems.append(f"empty glyph {n}")
    sheet([FINAL], [16, 20, 24, 32, 48, 96], [(0x20, 0x20, 0x20), (0xF3, 0xF3, 0xF3),
                                              (0x1E, 0x4F, 0x7A), (0xC9, 0x7B, 0x3B)],
          Path("/tmp/legibility.png"))
    tray = Image.new("RGBA", (6 * 70, 4 * 70), (0, 0, 0, 255))
    for row, (bg, rgb, on) in enumerate([((0x20, 0x20, 0x20), MONO_WHITE, True),
                                         ((0x20, 0x20, 0x20), MONO_WHITE, False),
                                         ((0xF3, 0xF3, 0xF3), MONO_BLACK, True),
                                         ((0xF3, 0xF3, 0xF3), MONO_BLACK, False)]):
        for col, n in enumerate([16, 20, 24, 32, 48, 64]):
            cell = Image.new("RGBA", (70, 70), bg + (255,))
            g = render(glyph(n / GRID, rgb, on), n)
            cell.alpha_composite(g, ((70 - n) // 2, (70 - n) // 2))
            tray.alpha_composite(cell, (col * 70, row * 70))
    tray.resize((tray.width * 2, tray.height * 2), Image.NEAREST).convert("RGB").save("/tmp/tray.png")
    print("wrote /tmp/legibility.png /tmp/tray.png")
    if problems:
        print("PROBLEMS:", problems)
        sys.exit(1)
    print("icon checks passed")


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "build"
    if cmd == "concepts":
        sheet(list(sys.argv[2]) if len(sys.argv) > 2 else list(CONCEPTS),
              [16, 20, 24, 32, 48, 96], [(0x20, 0x20, 0x20), (0xF3, 0xF3, 0xF3)],
              Path("/tmp/concepts.png"))
        print("wrote /tmp/concepts.png")
    elif cmd == "check":
        check()
    else:
        build()
