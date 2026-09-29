#!/usr/bin/env python3
"""Draw the Debrief app icon (1024 px) in the DEADBOLT HUD look.

    scripts/make-icon.py design/app-icon.png && pnpm tauri icon design/app-icon.png

A dark panel inside the macOS icon grid (824 px, 185 px corners) with the
focused-panel corner brackets, three diff bars (+ kept, − dimmed, accent)
and the violet agent mark. Colours are the tailwind tokens.
"""
import sys
from PIL import Image, ImageDraw, ImageFilter

S = 4  # supersample
N = 1024 * S
AC, OK, DEL, AGENT = (61, 240, 255), (127, 212, 154), (217, 138, 125), (192, 139, 255)
BG_TOP, BG_BOT = (13, 22, 29), (5, 8, 11)


def px(v):
    return int(v * S)


def main(out):
    img = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    x0, y0, x1, y1 = px(100), px(100), px(924), px(924)

    # Panel: vertical gradient clipped to the rounded square.
    grad = Image.new("RGBA", (N, N))
    gd = ImageDraw.Draw(grad)
    for y in range(y0, y1):
        t = (y - y0) / (y1 - y0)
        c = tuple(int(BG_TOP[i] + (BG_BOT[i] - BG_TOP[i]) * t) for i in range(3))
        gd.line([(x0, y), (x1, y)], fill=c + (255,))
    mask = Image.new("L", (N, N), 0)
    ImageDraw.Draw(mask).rounded_rectangle([x0, y0, x1, y1], radius=px(185), fill=255)
    img.paste(grad, (0, 0), mask)

    # Accent glow, then crisp shapes on top.
    glow = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    g = ImageDraw.Draw(glow)
    d = ImageDraw.Draw(img)

    def bracket(draw, cx, cy, dx, dy, colour, w):
        L = px(120)
        draw.line([(cx, cy), (cx + dx * L, cy)], fill=colour, width=px(w))
        draw.line([(cx, cy), (cx, cy + dy * L)], fill=colour, width=px(w))

    m = px(215)
    corners = [(m, m, 1, 1), (N - m, m, -1, 1), (m, N - m, 1, -1), (N - m, N - m, -1, -1)]
    for cx, cy, dx, dy in corners:
        bracket(g, cx, cy, dx, dy, AC + (150,), 30)
    bars = [
        (px(300), px(410), px(640), OK),
        (px(300), px(500), px(560), DEL),
        (px(300), px(590), px(724), AC),
    ]
    for bx0, by, bx1, colour in bars:
        g.rounded_rectangle([bx0, by, bx1, by + px(46)], radius=px(8), fill=colour + (120,))
    glow = glow.filter(ImageFilter.GaussianBlur(px(18)))
    img = Image.alpha_composite(img, Image.composite(glow, Image.new("RGBA", (N, N)), mask))
    d = ImageDraw.Draw(img)

    for cx, cy, dx, dy in corners:
        bracket(d, cx, cy, dx, dy, AC + (255,), 16)
    for i, (bx0, by, bx1, colour) in enumerate(bars):
        alpha = 110 if i == 1 else 255  # the removed line is dimmed, like a reverted hunk
        d.rounded_rectangle([bx0, by, bx1, by + px(46)], radius=px(8), fill=colour + (alpha,))
        d.rectangle([bx0 - px(58), by + px(14), bx0 - px(26), by + px(32)], fill=colour + (alpha,))  # gutter sign
    d.rectangle([px(700), px(290), px(748), px(338)], fill=AGENT + (255,))  # the agent mark

    img.resize((1024, 1024), Image.LANCZOS).save(out)
    print("wrote", out)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "design/app-icon.png")
