#!/usr/bin/env python3
"""Encode actual native workflow frames. Optional documentation dependency: Pillow."""
import argparse
import json
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("frames", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
files = sorted(args.frames.glob("[0-9][0-9][0-9][0-9].png"))
if len(files) != 304:
    raise SystemExit(f"Expected 304 frames, found {len(files)}")
actions = json.loads((args.frames.parent / "actions.json").read_text())
evidence = json.loads((args.frames.parent / "evidence.json").read_text())
if not all(any(key in row for row in evidence) for key in ("preview", "loaded", "errors", "searched", "event_details")):
    raise SystemExit("Capture did not demonstrate all required workflows")
with Image.open(files[0]) as image:
    width, height = image.size
font_dir = Path("/usr/share/fonts/truetype/dejavu")
title_font = ImageFont.truetype(str(font_dir / "DejaVuSans-Bold.ttf"), 17)
body_font = ImageFont.truetype(str(font_dir / "DejaVuSans.ttf"), 13)


def compose(index):
    with Image.open(files[index]) as source:
        result = Image.new("RGB", (width, height + 70), "#171a20")
        result.paste(source.convert("RGB"), (0, 0))
    draw = ImageDraw.Draw(result)
    if index < 16 or index >= 280:
        section, title, detail = "01", "See your system at a glance", "A native workspace for disk cleanup, performance and system logs."
    elif index < 48:
        section, title, detail = "02", "Review the cleanup scope", "One isolated sample folder. Age and filename rules decide eligibility."
    elif index < 112:
        section, title, detail = "02", "Run a cleanup preview", "Real file scan: 2 eligible files, 3 MiB. Recent files stay excluded. No deletion."
    elif index < 160:
        section, title, detail = "03", "Inspect live CPU and memory", "Switch resource views while actual counters continue updating."
    elif index < 200:
        section, title, detail = "04", "Load events to investigate", "Six labeled sample events, read through the normal journal decoder."
    elif index < 224:
        section, title, detail = "04", "Filter down to errors", "The actual severity filter narrows six loaded events to two errors."
    elif index < 248:
        section, title, detail = "04", "Search for the app", "Type editor to narrow the two errors to one matching event."
    else:
        section, title, detail = "05", "Inspect the event details", "Select the result to read the full message and process details. Sample logs."
    draw.line((0, height, width, height), fill="#303740")
    draw.rounded_rectangle((20, height+18, 54, height+52), radius=9, fill="#29382b")
    draw.text((27, height+26), section, font=body_font, fill="#a9d49a")
    draw.text((70, height+12), title, font=title_font, fill="#f2f3f5")
    draw.text((70, height+38), detail, font=body_font, fill="#abb3c0")
    # Cursor targets come from mapped widget bounds for each real callback;
    # they are explanatory overlays, not invented actions or fabricated output.
    prior = [action for action in actions if action["frame"] <= index]
    if prior and index < 280:
        current = prior[-1]
        previous = prior[-2] if len(prior) > 1 else current
        t = min(1, (index-current["frame"])/5)
        t = t*t*(3-2*t)
        x = previous["x"]+(current["x"]-previous["x"])*t
        y = previous["y"]+(current["y"]-previous["y"])*t
        if 5 <= index-current["frame"] <= 8:
            draw.ellipse((x-13,y-13,x+13,y+13),outline="#d8eabf",width=2)
        arrow = [(x,y),(x+2,y+24),(x+8,y+17),(x+13,y+27),(x+17,y+25),(x+12,y+15),(x+22,y+14)]
        draw.polygon(arrow,fill="#fbfbfc",outline="#111318",width=2)
    return result


atlas = Image.new("RGB", (550*6, 390))
for column,index in enumerate((8,64,128,184,212,264)):
    atlas.paste(compose(index).resize((550,390)),(column*550,0))
palette=atlas.quantize(colors=224,method=Image.Quantize.MEDIANCUT)
colors=palette.getpalette()[:224*3]
for hue in ((31,128,217),(158,82,204),(64,163,77),(217,122,31)):
    for step in range(1,9):
        colors.extend(round(45+(channel-45)*step/8) for channel in hue)
palette.putpalette(colors)
frames=[compose(i).quantize(palette=palette,dither=Image.Dither.NONE) for i in range(304)]
args.output.parent.mkdir(parents=True,exist_ok=True)
frames[0].save(args.output,save_all=True,append_images=frames[1:],duration=[120,130]*152,
               loop=0,optimize=True,disposal=1)
print(f"Wrote {args.output}: {args.output.stat().st_size/1024/1024:.2f} MiB, 38 seconds")
