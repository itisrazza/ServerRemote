#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["fonttools==4.68.0"]
# ///
"""Pin static instances of Nunito Sans for the app to embed.

iced can't set arbitrary variation axes, so the design's settings are baked
into static fonts here: lowercase height (YTLC) at its maximum, optical size
12, and the weights/widths the UI uses.

Source: NunitoSans[YTLC,opsz,wdth,wght].ttf from
https://github.com/google/fonts/tree/main/ofl/nunitosans (SIL OFL 1.1).

Usage: scripts/instance-fonts.py <variable font> <output dir>
"""

import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

FAMILY = "Nunito Sans"

# (file stem, style name, wght, wdth, OS/2 usWidthClass)
INSTANCES = [
    ("NunitoSans-Medium", "Medium", 500, 100, 5),
    ("NunitoSans-Bold", "Bold", 700, 100, 5),
    ("NunitoSans-CondensedMedium", "Condensed Medium", 500, 75, 3),
]


def set_names(font: TTFont, style: str, stem: str) -> None:
    """Give every instance the same family name so the app selects them by
    weight and stretch rather than by per-instance family names."""
    names = font["name"]
    for record in list(names.names):
        if record.nameID in (1, 2, 4, 6, 16, 17):
            names.removeNames(nameID=record.nameID)
    names.setName(FAMILY, 1, 3, 1, 0x409)
    names.setName("Regular", 2, 3, 1, 0x409)
    names.setName(f"{FAMILY} {style}", 4, 3, 1, 0x409)
    names.setName(stem, 6, 3, 1, 0x409)
    names.setName(FAMILY, 16, 3, 1, 0x409)
    names.setName(style, 17, 3, 1, 0x409)


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    source, out_dir = Path(sys.argv[1]), Path(sys.argv[2])
    out_dir.mkdir(parents=True, exist_ok=True)

    for stem, style, weight, width, width_class in INSTANCES:
        font = instancer.instantiateVariableFont(
            TTFont(source),
            {"wght": weight, "wdth": width, "opsz": 12, "YTLC": 540},
        )
        font["OS/2"].usWeightClass = weight
        font["OS/2"].usWidthClass = width_class
        set_names(font, style, stem)
        path = out_dir / f"{stem}.ttf"
        font.save(path)
        print(path)


if __name__ == "__main__":
    main()
