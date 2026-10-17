"""Draws the AirDash icon: a white ring and a white "A" peak on a navy disc.

Usage: python src/airdash/assets/make_icon.py   (needs Pillow; writes icon.png and the Windows icon.ico next to this script)
"""
import math
from pathlib import Path

from PIL import Image, ImageDraw

NAVY = (31, 41, 55, 255)
WHITE = (238, 241, 245, 255)
CLEAR = (0, 0, 0, 0)
BIG = 1024  # draw large, then downscale for smooth edges


def circle(d: ImageDraw.ImageDraw, c: float, r: float, fill) -> None:
    """A circle centred at (c, c), the middle of the icon."""
    dot(d, c, c, r, fill)


def dot(d: ImageDraw.ImageDraw, x: float, y: float, r: float, fill) -> None:
    d.ellipse([x - r, y - r, x + r, y + r], fill=fill)


def rounded_triangle(d: ImageDraw.ImageDraw, points, radius: float, fill) -> None:
    """A filled triangle whose corners are rounded with `radius`."""
    inset = []
    for i, (x, y) in enumerate(points):
        # Move each corner along its angle bisector so the edges sit `radius` further in.
        ax, ay = points[i - 1]
        bx, by = points[(i + 1) % 3]
        u = (ax - x, ay - y)
        v = (bx - x, by - y)
        lu, lv = math.hypot(*u), math.hypot(*v)
        angle = math.acos((u[0] * v[0] + u[1] * v[1]) / (lu * lv))
        bis = (u[0] / lu + v[0] / lv, u[1] / lu + v[1] / lv)
        lb = math.hypot(*bis)
        dist = radius / math.sin(angle / 2)
        inset.append((x + bis[0] / lb * dist, y + bis[1] / lb * dist))
    d.polygon(inset, fill=fill)
    for i in range(3):
        d.line([inset[i], inset[(i + 1) % 3]], fill=fill, width=round(radius * 2))
        dot(d, inset[i][0], inset[i][1], radius, fill)


def draw() -> Image.Image:
    c = BIG / 2
    inner = 0.385 * BIG

    # Navy disc, white ring, navy inside.
    img = Image.new("RGBA", (BIG, BIG), CLEAR)
    d = ImageDraw.Draw(img)
    circle(d, c, 0.5 * BIG, NAVY)
    circle(d, c, 0.465 * BIG, WHITE)
    circle(d, c, inner, NAVY)

    # The "A": a white peak with a rounded top whose legs run into the ring, so the bottom is solid.
    peak = Image.new("RGBA", (BIG, BIG), CLEAR)
    p = ImageDraw.Draw(peak)
    rounded_triangle(p, [(c, c - 0.30 * BIG), (c + 0.50 * BIG, c + 0.62 * BIG), (c - 0.50 * BIG, c + 0.62 * BIG)], 0.085 * BIG, WHITE)
    # The hollow at the top of the "A", with softly rounded corners.
    rounded_triangle(p, [(c, c - 0.135 * BIG), (c + 0.115 * BIG, c + 0.115 * BIG), (c - 0.115 * BIG, c + 0.115 * BIG)], 0.018 * BIG, NAVY)

    mask = Image.new("L", (BIG, BIG), 0)
    circle(ImageDraw.Draw(mask), c, inner + 2, 255)
    peak.putalpha(Image.composite(peak.getchannel("A"), mask, mask))
    return Image.alpha_composite(img, peak)


if __name__ == "__main__":
    here = Path(__file__).parent
    icon = draw()
    icon.resize((256, 256), Image.LANCZOS).save(here / "icon.png")
    icon.save(here / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
