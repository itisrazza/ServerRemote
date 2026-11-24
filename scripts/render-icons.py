#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow==13.0.0", "resvg_py==0.5.0"]
# ///
"""Render the platform icons that can't be built from the SVG at build time.

Source: apps/desktop/assets/icon.icon/Assets/logo.svg (the same logo the
window icon and the macOS icon use). Outputs are committed; rerun this after
changing the logo.

- packaging/windows/icon.ico: the .exe and installer icon.

Usage: scripts/render-icons.py
"""

import io
from pathlib import Path

import resvg_py
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
LOGO = ROOT / "apps/desktop/assets/icon.icon/Assets/logo.svg"

# The sizes Windows asks for across Explorer views, the taskbar and the
# Start menu at 100–200% scale.
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 256]


def render(size: int) -> Image.Image:
    png = resvg_py.svg_to_bytes(svg_path=str(LOGO), width=size, height=size)
    return Image.open(io.BytesIO(bytes(png))).convert("RGBA")


def write_ico(path: Path) -> None:
    images = [render(size) for size in ICO_SIZES]
    path.parent.mkdir(parents=True, exist_ok=True)
    images[-1].save(
        path,
        format="ICO",
        sizes=[image.size for image in images],
        append_images=images[:-1],
    )


def main() -> None:
    write_ico(ROOT / "packaging/windows/icon.ico")


if __name__ == "__main__":
    main()
