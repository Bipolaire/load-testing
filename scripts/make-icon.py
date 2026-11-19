#!/usr/bin/env python3
"""Génère les icônes de l'application (PNG 1024, ICNS macOS, ICO Windows).

Usage : python3 scripts/make-icon.py
Dépendance : Pillow (`pip install pillow`).
"""

from __future__ import annotations

import math
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"

TOP = (99, 102, 241)  # indigo 500
BOTTOM = (67, 56, 202)  # indigo 700
NEEDLE = (255, 255, 255)
TRACK = (255, 255, 255, 70)
NEEDLE_TAIL = (244, 114, 182)  # rose 400


def rounded_mask(size: int, radius: int) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return mask


def gradient(size: int) -> Image.Image:
    image = Image.new("RGB", (size, size))
    pixels = image.load()
    for y in range(size):
        t = y / (size - 1)
        color = tuple(round(TOP[i] + (BOTTOM[i] - TOP[i]) * t) for i in range(3))
        for x in range(size):
            pixels[x, y] = color
    return image


def draw_gauge(size: int) -> Image.Image:
    scale = size / 1024
    image = gradient(size).convert("RGBA")
    draw = ImageDraw.Draw(image, "RGBA")

    cx, cy = size / 2, size * 0.56
    radius = size * 0.30
    width = max(2, round(size * 0.075))
    start, end = 140, 400  # degrés, sens horaire

    draw.arc(
        (cx - radius, cy - radius, cx + radius, cy + radius),
        start=start,
        end=end,
        fill=TRACK,
        width=width,
    )
    draw.arc(
        (cx - radius, cy - radius, cx + radius, cy + radius),
        start=end - 90,
        end=end,
        fill=NEEDLE,
        width=width,
    )

    angle = math.radians(end - 26)
    needle_len = radius * 0.66
    tip = (cx + math.cos(angle) * needle_len, cy + math.sin(angle) * needle_len)
    draw.line([(cx, cy), tip], fill=NEEDLE, width=max(3, round(size * 0.055)))
    draw.ellipse(
        (
            tip[0] - size * 0.045,
            tip[1] - size * 0.045,
            tip[0] + size * 0.045,
            tip[1] + size * 0.045,
        ),
        fill=NEEDLE_TAIL,
    )

    hub = size * 0.048
    draw.ellipse((cx - hub, cy - hub, cx + hub, cy + hub), fill=NEEDLE)
    inner = hub * 0.42
    draw.ellipse((cx - inner, cy - inner, cx + inner, cy + inner), fill=BOTTOM)

    image.putalpha(rounded_mask(size, round(size * 0.22)))
    return image


def write_icns(source: Image.Image, target: Path) -> None:
    if sys.platform != "darwin" or shutil.which("iconutil") is None:
        return
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "icon.iconset"
        iconset.mkdir()
        for base in (16, 32, 128, 256, 512):
            for suffix, factor in (("", 1), ("@2x", 2)):
                pixels = base * factor
                name = f"icon_{base}x{base}{suffix}.png"
                source.resize((pixels, pixels), Image.LANCZOS).save(iconset / name)
        subprocess.run(
            ["iconutil", "-c", "icns", str(iconset), "-o", str(target)],
            check=True,
        )


def main() -> None:
    ASSETS.mkdir(exist_ok=True)
    image = draw_gauge(1024)
    image.resize((512, 512), Image.LANCZOS).save(ASSETS / "icon.png")
    image.save(ASSETS / "icon@2x.png")
    image.save(
        ASSETS / "icon.ico",
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    write_icns(image, ASSETS / "icon.icns")
    print(f"icônes écrites dans {ASSETS}")


if __name__ == "__main__":
    main()
