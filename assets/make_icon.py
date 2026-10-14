"""Draws the AirDash icon: a slate ring around a slate disc with white air-flow waves.

Usage: python assets/make_icon.py assets/icon.png   (needs Pillow)
"""
import math
import sys

from PIL import Image, ImageDraw

SLATE = (75, 90, 99, 255)
WHITE = (255, 255, 255, 255)
BIG = 1024  # draw large, then downscale for smooth edges


def draw() -> Image.Image:
    img = Image.new("RGBA", (BIG, BIG), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    c = BIG / 2
    outer, ring = 0.48 * BIG, 0.07 * BIG
    d.ellipse([c - outer, c - outer, c + outer, c + outer], fill=SLATE)
    inner_gap = outer - ring
    d.ellipse([c - inner_gap, c - inner_gap, c + inner_gap, c + inner_gap], fill=(0, 0, 0, 0))
    disc = 0.355 * BIG
    d.ellipse([c - disc, c - disc, c + disc, c + disc], fill=SLATE)

    # Two offset air-flow waves, like a breeze. Stamped as discs for smooth, even strokes.
    r = 0.03 * BIG
    for dy, dx, half_len in ((-0.08, -0.03, 0.19), (0.08, 0.05, 0.15)):
        y0, x0 = c + dy * BIG, c + dx * BIG
        steps = 1500
        for i in range(steps + 1):
            t = -1 + 2 * i / steps
            x = x0 + t * half_len * BIG
            y = y0 - math.sin(t * math.pi * 1.25) * 0.032 * BIG
            d.ellipse([x - r, y - r, x + r, y + r], fill=WHITE)
    return img


if __name__ == "__main__":
    out = sys.argv[1]
    draw().resize((256, 256), Image.LANCZOS).save(out)
