#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow==13.0.0"]
# ///
"""Measure a mockup or snapshot PNG, reporting image pixels and logical points.

Commands (coordinates are image pixels; --scale converts to points, default 2):

  pixel  IMAGE X Y                      colour at a point
  box    IMAGE X0 Y0 X1 Y1 [--colour C]  bounds of pixels matching C (default:
                                        anything unlike the region's commonest
                                        colour, i.e. its background)
  glyph  IMAGE X0 Y0 X1 Y1              bounds of the first glyph (leftmost run
                                        of foreground columns) in the region,
                                        plus the font size its height implies

  --tolerance N   per-channel difference still counted as a match (default 24)

Font sizes assume Nunito Sans: cap height 0.705em, x-height 0.537em. Measure a
capital without a descender (H, E, L, M) for cap height.
"""

import argparse
import sys
from collections import Counter
from dataclasses import dataclass

from PIL import Image

CAP_HEIGHT = 0.705
X_HEIGHT = 0.537

Colour = tuple[int, int, int]


@dataclass(frozen=True)
class Bounds:
    left: int
    top: int
    right: int
    bottom: int

    def describe(self, scale: float) -> str:
        width, height = self.right - self.left, self.bottom - self.top
        return (
            f"image ({self.left}, {self.top})–({self.right}, {self.bottom}) {width}×{height}px; "
            f"logical ({self.left / scale:g}, {self.top / scale:g}) {width / scale:g}×{height / scale:g}pt"
        )


def parse_colour(text: str) -> Colour:
    hex_digits = text.removeprefix("#")
    if len(hex_digits) != 6:
        raise argparse.ArgumentTypeError(f"expected #rrggbb, got {text!r}")
    return (int(hex_digits[0:2], 16), int(hex_digits[2:4], 16), int(hex_digits[4:6], 16))


def close(a: Colour, b: Colour, tolerance: int) -> bool:
    return all(abs(x - y) <= tolerance for x, y in zip(a, b))


def region(image: Image.Image, x0: int, y0: int, x1: int, y1: int) -> dict[tuple[int, int], Colour]:
    return {(x, y): image.getpixel((x, y)) for y in range(y0, y1) for x in range(x0, x1)}


def matching(pixels: dict[tuple[int, int], Colour], colour: Colour | None, tolerance: int) -> list[tuple[int, int]]:
    if colour is not None:
        return [point for point, value in pixels.items() if close(value, colour, tolerance)]
    background = Counter(pixels.values()).most_common(1)[0][0]
    return [point for point, value in pixels.items() if not close(value, background, tolerance)]


def bounds(points: list[tuple[int, int]]) -> Bounds | None:
    if not points:
        return None
    xs, ys = [x for x, _ in points], [y for _, y in points]
    return Bounds(min(xs), min(ys), max(xs) + 1, max(ys) + 1)


def first_glyph(points: list[tuple[int, int]]) -> list[tuple[int, int]]:
    columns = sorted({x for x, _ in points})
    if not columns:
        return []
    run_end = columns[0]
    for column in columns[1:]:
        if column != run_end + 1:
            break
        run_end = column
    return [(x, y) for x, y in points if x <= run_end]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["pixel", "box", "glyph"])
    parser.add_argument("image")
    parser.add_argument("coordinates", type=int, nargs="+")
    parser.add_argument("--colour", type=parse_colour)
    parser.add_argument("--tolerance", type=int, default=24)
    parser.add_argument("--scale", type=float, default=2.0)
    args = parser.parse_args()

    image = Image.open(args.image).convert("RGB")
    if args.command == "pixel":
        if len(args.coordinates) != 2:
            sys.exit("pixel takes X Y")
        r, g, b = image.getpixel(tuple(args.coordinates))
        print(f"#{r:02x}{g:02x}{b:02x} rgb({r}, {g}, {b})")
        return

    if len(args.coordinates) != 4:
        sys.exit(f"{args.command} takes X0 Y0 X1 Y1")
    points = matching(region(image, *args.coordinates), args.colour, args.tolerance)
    if args.command == "glyph":
        points = first_glyph(points)
    found = bounds(points)
    if found is None:
        sys.exit(f"nothing matched in {args.coordinates} of {args.image}")
    print(found.describe(args.scale))
    if args.command == "glyph":
        height = (found.bottom - found.top) / args.scale
        print(f"if a capital: ~{height / CAP_HEIGHT:.1f}pt font; if x-height: ~{height / X_HEIGHT:.1f}pt font")


if __name__ == "__main__":
    main()
