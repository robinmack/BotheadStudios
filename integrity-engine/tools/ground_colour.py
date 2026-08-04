#!/usr/bin/env python3
"""Mean ground colour of the frames `web/rig/terra_ground_colour.mjs` writes.

★ It decodes the image. A PNG's BYTE mean is a statistic about compressed data, and measuring that
once gave 127.2 / 127.4 / 127.6 for noon, dusk and midnight — three frames a human can tell apart at a
glance — which would have "proved" that lighting does nothing.

The lower part of the frame is ground when the camera looks down; the sky fraction is reported so a
frame that is mostly sky says so instead of quietly averaging blue into "the ground".

    python3 tools/ground_colour.py /tmp/rigshot/ground-*.png
"""
import sys

from PIL import Image


def main(paths):
    print(f"{'frame':22s} {'R':>6} {'G':>6} {'B':>6}  {'luma':>6}  hue        sky%")
    for path in paths:
        im = Image.open(path).convert("RGB")
        w, h = im.size
        ground = im.crop((0, int(h * 0.55), w, h))
        px = list(ground.getdata())
        n = len(px)
        r = sum(p[0] for p in px) / n
        g = sum(p[1] for p in px) / n
        b = sum(p[2] for p in px) / n
        sky = sum(1 for p in px if p[2] > p[0] and p[2] > p[1]) / n
        luma = 0.2126 * r + 0.7152 * g + 0.0722 * b
        hue = "GREEN" if g > r and g > b else "red/brown" if r > g and r > b else "blue/other"
        name = path.rsplit("/", 1)[-1]
        print(f"{name:22s} {r:6.1f} {g:6.1f} {b:6.1f}  {luma:6.1f}  {hue:10s} {sky * 100:.0f}%")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    main(sys.argv[1:])
