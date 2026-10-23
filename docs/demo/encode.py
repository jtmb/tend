#!/usr/bin/env python3
"""Encode Tend's native documentation frames as a small, looping GIF.

Optional documentation dependency: Pillow (not an application dependency).
Usage: python3 docs/demo/encode.py /path/to/frames assets/demo.gif
"""
import argparse
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("frames", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
files = sorted(args.frames.glob("[0-9][0-9][0-9][0-9].png"))
if len(files) != 256:
    raise SystemExit(f"Expected 256 native frames, found {len(files)}")

font_dir = Path("/usr/share/fonts/truetype/dejavu")
title_font = ImageFont.truetype(str(font_dir / "DejaVuSans-Bold.ttf"), 17)
body_font = ImageFont.truetype(str(font_dir / "DejaVuSans.ttf"), 13)


def compose(index):
    with Image.open(files[index]) as source:
        app = source.convert("RGB")
        app.thumbnail((960, 760), Image.Resampling.LANCZOS)
    result = Image.new("RGB", (960, 830), "#171a20")
    result.paste(app, ((960 - app.width) // 2, 0))
    draw = ImageDraw.Draw(result)
    if index < 24 or index >= 232:
        section, title, detail = "01", "Your system at a glance", "Storage, maintenance and system health in one native window."
    elif index < 104:
        section, title, detail = "02", "Follow live performance", "Switch between CPU and memory. Readings shown are real, not benchmarks."
    elif index < 160:
        section, title, detail = "03", "Shape your cleanup policy", "Choose categories, age limits and additional folder rules."
    elif index < 200:
        section, title, detail = "04", "Focus your log search", "Filter by source and severity. Private logs are not loaded in this demo."
    else:
        section, title, detail = "05", "Tune with clear controls", "Review desktop and memory settings before applying changes."
    draw.line((0, 760, 960, 760), fill="#303740")
    draw.rounded_rectangle((20, 778, 54, 812), radius=9, fill="#29382b")
    draw.text((27, 786), section, font=body_font, fill="#a9d49a")
    draw.text((70, 772), title, font=title_font, fill="#f2f3f5")
    draw.text((70, 798), detail, font=body_font, fill="#abb3c0")
    return result


# One shared palette keeps static text stable between frames. No per-frame
# dithering, fabricated graphs, simulated data or decorative transitions.
atlas = Image.new("RGB", (480 * 6, 415))
for column, index in enumerate((8, 40, 72, 120, 184, 216)):
    atlas.paste(compose(index).resize((480, 415)), (column * 480, 0))
palette = atlas.quantize(colors=224, method=Image.Quantize.MEDIANCUT)
# Reserve the native chart hues so thin CPU/memory lines don't get mapped to
# gray merely because dark backgrounds occupy most of the screenshot.
colors = palette.getpalette()[:224 * 3]
for hue in ((31, 128, 217), (158, 82, 204), (64, 163, 77), (217, 122, 31)):
    for step in range(1, 9):
        colors.extend(round(45 + (channel - 45) * step / 8) for channel in hue)
palette.putpalette(colors)
frames = [compose(index).quantize(palette=palette, dither=Image.Dither.NONE) for index in range(256)]
args.output.parent.mkdir(parents=True, exist_ok=True)
frames[0].save(
    args.output,
    save_all=True,
    append_images=frames[1:],
    duration=[120, 130] * 128,
    loop=0,
    optimize=True,
    disposal=1,
)
print(f"Wrote {args.output}: {args.output.stat().st_size / 1024 / 1024:.2f} MiB, 32 seconds")
