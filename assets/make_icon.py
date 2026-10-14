"""Draws the AirDash icon: a slate ring around a slate disc with a white peak.

Usage: python assets/make_icon.py   (needs Pillow; writes icon.png and the Windows icon.ico next to this script)
"""
from pathlib import Path

from PIL import Image, ImageDraw

SLATE = (75, 90, 99, 255)
WHITE = (255, 255, 255, 255)
CLEAR = (0, 0, 0, 0)
BIG = 1024  # draw large, then downscale for smooth edges


def circle(d: ImageDraw.ImageDraw, c: float, r: float, fill) -> None:
    d.ellipse([c - r, c - r, c + r, c + r], fill=fill)


def draw() -> Image.Image:
    c = BIG / 2
    disc = 0.355 * BIG

    # Ring with a transparent gap, then the solid disc inside it.
    img = Image.new("RGBA", (BIG, BIG), CLEAR)
    d = ImageDraw.Draw(img)
    circle(d, c, 0.48 * BIG, SLATE)
    circle(d, c, 0.41 * BIG, CLEAR)
    circle(d, c, disc, SLATE)

    # The peak, drawn on its own layer and clipped to the disc: an "A" outline on top of a solid base.
    peak = Image.new("RGBA", (BIG, BIG), CLEAR)
    p = ImageDraw.Draw(peak)
    apex_y, base_y, half_base = c - 0.21 * BIG, c + 0.40 * BIG, 0.30 * BIG
    p.polygon([(c, apex_y), (c + half_base, base_y), (c - half_base, base_y)], fill=WHITE)
    slope = half_base / (base_y - apex_y)
    hole_top, hole_floor = apex_y + 0.13 * BIG, c + 0.08 * BIG
    hole_half = (hole_floor - hole_top) * slope
    p.polygon([(c, hole_top), (c + hole_half, hole_floor), (c - hole_half, hole_floor)], fill=CLEAR)

    mask = Image.new("L", (BIG, BIG), 0)
    circle(ImageDraw.Draw(mask), c, disc, 255)
    peak.putalpha(Image.composite(peak.getchannel("A"), mask, mask))
    return Image.alpha_composite(img, peak)


if __name__ == "__main__":
    here = Path(__file__).parent
    icon = draw()
    icon.resize((256, 256), Image.LANCZOS).save(here / "icon.png")
    icon.save(here / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
