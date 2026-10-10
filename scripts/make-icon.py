#!/usr/bin/env python3
"""Builds the app icon artwork (src-tauri/icons/source.png) from the clipart.

The clipart is a one-colour silhouette on a transparent background. Only its
shape is used: it is painted in a soft gradient around the app's accent
colour, with a faint glow behind it, on a rounded tile in the app's window
colour (see the theme in src/styles.css).

Usage, from the project root (needs Pillow):
    python3 scripts/make-icon.py src-tauri/icons/clipart.png src-tauri/icons/source.png
    npx tauri icon src-tauri/icons/source.png
"""
import sys
from PIL import Image, ImageDraw, ImageFilter

src, out = sys.argv[1], sys.argv[2]
N, SS = 1024, 2            # final size, supersampling factor for smooth edges
M = N * SS

ACCENT = (232, 163, 61)      # --color-accent  #e8a33d
ART_TOP = (247, 199, 112)    # a lighter amber, at the top of the picture
ART_BOTTOM = (211, 128, 38)  # a deeper amber, at the bottom
PANEL = (27, 30, 37)         # --color-panel   #1b1e25
WINDOW = (20, 22, 27)        # --color-window  #14161b
WIDTH = 0.86                 # how much of the tile's width the picture takes


def vertical_gradient(size, stops):
    """An image of `size` shading top to bottom through the colours in
    `stops`, which are evenly spaced."""
    width, height = size
    column = Image.new("RGB", (1, height))
    for y in range(height):
        t = y / (height - 1) * (len(stops) - 1)
        i = min(int(t), len(stops) - 2)
        a, b = stops[i], stops[i + 1]
        column.putpixel((0, y), tuple(round(p + (q - p) * (t - i)) for p, q in zip(a, b)))
    return column.resize(size).convert("RGBA")


# Rounded-square tile: the window colour, a touch lighter (the panel colour)
# at the top so that it does not look completely flat.
tile = vertical_gradient((M, M), [PANEL, WINDOW])
mask = Image.new("L", (M, M), 0)
ImageDraw.Draw(mask).rounded_rectangle((0, 0, M - 1, M - 1), radius=224 * SS, fill=255)

# The picture's shape (its alpha channel), scaled and centred on a
# full-size layer. A slight blur takes the hard, traced look off its edges.
shape = Image.open(src).convert("RGBA").getchannel("A")
w = int(M * WIDTH)
h = round(shape.height * w / shape.width)
left, top = (M - w) // 2, (M - h) // 2
shape_layer = Image.new("L", (M, M), 0)
shape_layer.paste(shape.resize((w, h), Image.LANCZOS), (left, top))
shape_layer = shape_layer.filter(ImageFilter.GaussianBlur(1.6 * SS))

# A faint glow of the accent colour behind the picture.
glow = Image.new("RGBA", (M, M), ACCENT + (255,))
glow.putalpha(shape_layer.filter(ImageFilter.GaussianBlur(30 * SS)).point(lambda a: a * 80 // 255))
tile = Image.alpha_composite(tile, glow)

# The picture itself: light amber at its top, through the accent colour, to
# a deeper amber at its bottom.
fill = Image.new("RGBA", (M, M), (0, 0, 0, 0))
fill.paste(vertical_gradient((w, h), [ART_TOP, ACCENT, ART_BOTTOM]), (left, top))
fill.putalpha(shape_layer)
tile = Image.alpha_composite(tile, fill)

tile.putalpha(mask)
tile.resize((N, N), Image.LANCZOS).save(out)
