#!/usr/bin/env python3
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Draw Brainstead's app icon source, design/icon.png.

The mark is a house (the stead) holding a brain, drawn once in design/mark.svg (Mark in
src/icons.tsx and the splash in index.html use the same paths). On the icon it fills most of a
charcoal tile, as Outlook's mark does; a white-tiled version is drawn for About in light mode.
The tile is drawn with PIL; the mark is rasterised by AppKit
(tools/svg2png.swift), so it needs swiftc.

    python3 tools/make_icons.py            # then: npx tauri icon design/icon.png
    make icons                             # does both
    python3 tools/make_icons.py --tray-win # only the Windows tray glyphs, from tray@2x.png (no swiftc)

The Windows tray glyphs, tray-win-{white,black}-{16,20,24,32}.png, are tray@2x.png's brain cropped
to its own edges, fitted to the width and centred, white for a dark taskbar and black for a light
one (src-tauri/src/platform/tray.rs picks the size and colour). A full run draws tray@2x.png first
and then these.
"""

import pathlib
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
SS = 4
# A charcoal tile, a touch lighter at the top, like Outlook's dark icon.
TILE_TOP = (44, 46, 51)
TILE_BOTTOM = (28, 29, 33)
# On the dark tile the house takes the dark theme's accent (--accent in src/styles.css), which stands out from navy.
ACCENT_LIGHT, ACCENT_DARK = "#1d5fd6", "#5b9bff"
# The light version, for About in light mode: a white tile, a touch greyer at the bottom, with a hairline edge.
LIGHT_TOP = (255, 255, 255)
LIGHT_BOTTOM = (236, 239, 244)
LIGHT_EDGE = (211, 217, 226)


def app_icon(light=False):
    S = 1024 * SS
    top, bottom = (LIGHT_TOP, LIGHT_BOTTOM) if light else (TILE_TOP, TILE_BOTTOM)
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    # Apple's icon grid: an 824px tile centred in 1024, corner radius about 185.
    grad = Image.new("RGBA", (1, S))
    for y in range(S):
        t = y / S
        grad.putpixel((0, y), tuple(int(a + (b - a) * t) for a, b in zip(top, bottom, strict=True)) + (255,))
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle((100 * SS, 100 * SS, 924 * SS, 924 * SS), radius=185 * SS, fill=255)
    img.paste(grad.resize((S, S)), (0, 0), mask)
    if light:
        ImageDraw.Draw(img).rounded_rectangle(
            (100 * SS, 100 * SS, 924 * SS, 924 * SS), radius=185 * SS, outline=LIGHT_EDGE + (255,), width=3 * SS
        )

    # The house fills most of the tile: 24 of the mark's 32 units across, at 24px a unit (576px),
    # centred on the house itself (x 4–28, y 4.5–28.5).
    u = 24 * SS
    size = 32 * u
    with tempfile.TemporaryDirectory() as tmp:
        exe = pathlib.Path(tmp) / "svg2png"
        subprocess.run(["swiftc", "-O", str(ROOT / "tools" / "svg2png.swift"), "-o", str(exe)], check=True)
        png = pathlib.Path(tmp) / "mark.png"
        svg = pathlib.Path(tmp) / "mark.svg"
        mark_svg = (ROOT / "design" / "mark.svg").read_text()
        svg.write_text(mark_svg if light else mark_svg.replace(ACCENT_LIGHT, ACCENT_DARK))
        subprocess.run([str(exe), str(svg), str(png), str(size)], check=True)
        mark = Image.open(png).convert("RGBA")
    img.alpha_composite(mark, (S // 2 - 16 * u, S // 2 - int(16.5 * u)))
    if light:
        # Only About uses it (Settings › About and the About window), at 256px.
        img.resize((256, 256), Image.LANCZOS).save(ROOT / "src-tauri" / "icons" / "about-light.png", optimize=True)
        return
    out = ROOT / "design" / "icon.png"
    out.parent.mkdir(exist_ok=True)
    img.resize((1024, 1024), Image.LANCZOS).save(out)


# The menu-bar icon: the mark's brain on its own, black on clear (macOS tints a template image),
# 22pt drawn at 44px. Busy adds three dots under it, attention a dot at the top right.
BRAIN_LOBES = (
    "M15.4 14.3c-.9-.7-2.4-.7-3.2.2-1.5-.2-2.8 1-2.6 2.5-1.2.6-1.6 2.1-.9 3.2-.6 1.2-.1 2.7 1.2 3.1.2 1.4 1.6 2.3 3 1.9"
    ".7.7 1.7.9 2.5.4V14.3z M16.6 14.3c.9-.7 2.4-.7 3.2.2 1.5-.2 2.8 1 2.6 2.5 1.2.6 1.6 2.1.9 3.2.6 1.2.1 2.7-1.2 3.1"
    "-.2 1.4-1.6 2.3-3 1.9-.7.7-1.7.9-2.5.4V14.3z"
)
BRAIN_FOLDS = (
    "M12.4 17.6c.7-.6 1.8-.4 2.1.4M11.2 20.4c.6.6 1.7.6 2.2 0 .4.7 1.3 1 2 .6M12.6 23.2c.3-.7 1.1-1 1.8-.7"
    "M19.6 17.6c-.7-.6-1.8-.4-2.1.4M20.8 20.4c-.6.6-1.7.6-2.2 0-.4.7-1.3 1-2 .6M19.4 23.2c-.3-.7-1.1-1-1.8-.7"
)


def tray_icons(exe):
    big = 44 * SS
    # The brain spans x 8.5-23.5, y 13.6-25.4 of the mark; a square around it, with a little room.
    view = "7.4 11.4 17.2 17.2"
    layers = {}
    with tempfile.TemporaryDirectory() as tmp:
        for name, body in (
            ("lobes", f'<path d="{BRAIN_LOBES}" fill="#000"/>'),
            ("folds", f'<path d="{BRAIN_FOLDS}" stroke="#000" stroke-width="1.1" fill="none" stroke-linecap="round"/>'),
        ):
            svg = pathlib.Path(tmp) / f"{name}.svg"
            png = pathlib.Path(tmp) / f"{name}.png"
            svg.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view}">{body}</svg>')
            subprocess.run([str(exe), str(svg), str(png), str(big)], check=True)
            layers[name] = Image.open(png).convert("RGBA").getchannel("A")
    # The folds are cut out of the lobes.
    alpha = Image.eval(layers["folds"], lambda a: 255 - a)
    brain = Image.composite(layers["lobes"], Image.new("L", (big, big), 0), alpha)
    for kind in ("tray", "tray-busy", "tray-alert"):
        a = brain.copy()
        d = ImageDraw.Draw(a)
        if kind == "tray-busy":
            r = 2.2 * SS
            for i in range(3):
                cx, cy = (30 + i * 5.5) * SS, 41 * SS
                d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=255)
        if kind == "tray-alert":
            r = 5 * SS
            cx, cy = 38 * SS, 6 * SS
            d.ellipse((cx - r - 2 * SS, cy - r - 2 * SS, cx + r + 2 * SS, cy + r + 2 * SS), fill=0)
            d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=255)
        img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
        img.putalpha(a)
        img.resize((44, 44), Image.LANCZOS).save(ROOT / "src-tauri" / "icons" / f"{kind}@2x.png", optimize=True)


# Windows' tray glyph sizes: the notification area's icon at 100, 125, 150 and 200% scale.
TRAY_WIN_SIZES = (16, 20, 24, 32)


def tray_win_icons():
    """Writes tray-win-{white,black}-{16,20,24,32}.png from tray@2x.png: its brain, cropped to its
    edges, fitted to the width with Lanczos and centred (rounding down) on a square clear canvas."""
    icons = ROOT / "src-tauri" / "icons"
    alpha = Image.open(icons / "tray@2x.png").convert("RGBA").getchannel("A")
    edges = alpha.getbbox()
    w, h = edges[2] - edges[0], edges[3] - edges[1]
    brain = alpha.crop(edges)
    for size in TRAY_WIN_SIZES:
        fitted = brain.resize((size, max(1, round(h * size / w))), Image.LANCZOS)
        a = Image.new("L", (size, size), 0)
        a.paste(fitted, (0, (size - fitted.height) // 2))
        for colour, rgb in (("white", 255), ("black", 0)):
            img = Image.new("RGBA", (size, size), (rgb, rgb, rgb, 0))
            img.putalpha(a)
            img.save(icons / f"tray-win-{colour}-{size}.png", optimize=True)


def svg2png(tmp):
    exe = pathlib.Path(tmp) / "svg2png"
    subprocess.run(["swiftc", "-O", str(ROOT / "tools" / "svg2png.swift"), "-o", str(exe)], check=True)
    return exe


if __name__ == "__main__":
    (ROOT / "src-tauri" / "icons").mkdir(parents=True, exist_ok=True)
    if "--tray-win" in sys.argv[1:]:
        tray_win_icons()
        print("drew the Windows tray glyphs from src-tauri/icons/tray@2x.png")
        sys.exit(0)
    app_icon()
    app_icon(light=True)
    with tempfile.TemporaryDirectory() as t:
        tray_icons(svg2png(t))
    tray_win_icons()
    print("drew design/icon.png, src-tauri/icons/about-light.png and the tray icons")
