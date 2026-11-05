"""Generate GiRustStatus icon: flat rounded-square, dark slate ground,
radar-sweep arc + shield cutout, signal-green accent. 1024px PNG + .ico."""
from PIL import Image, ImageDraw
import math

S = 1024
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

BG = (15, 23, 42, 255)      # #0F172A slate
GREEN = (34, 197, 94, 255)  # #22C55E
GREEN_DIM = (34, 197, 94, 90)
BORDER = (71, 85, 105, 255) # #475569

# Rounded-square background with margin for the transparent corners.
m = 64
r = 180
d.rounded_rectangle([m, m, S - m, S - m], radius=r, fill=BG, outline=BORDER, width=12)

cx, cy = S // 2, S // 2

# Radar sweep: concentric arcs + a sweep wedge, all green.
def arc(bbox, start, end, width, color):
    d.arc(bbox, start=start, end=end, fill=color, width=width)

pad1, pad2, pad3 = 190, 300, 410
# Sweep wedge: filled pie slice ~50 degrees, from top rotating clockwise.
sweep_start = -90
sweep_end = -20
d.pieslice([cx - pad3, cy - pad3, cx + pad3, cy + pad3],
           start=sweep_start, end=sweep_end, fill=GREEN_DIM)
# Ring outlines.
arc([cx - pad1, cy - pad1, cx + pad1, cy + pad1], 0, 360, 18, GREEN)
arc([cx - pad2, cy - pad2, cx + pad2, cy + pad2], 0, 360, 14, (71, 85, 105, 255))
arc([cx - pad3, cy - pad3, cx + pad3, cy + pad3], 0, 360, 10, (71, 85, 105, 255))
# Sweep leading edge line.
sx = cx + pad3 * math.cos(math.radians(sweep_end))
sy = cy + pad3 * math.sin(math.radians(sweep_end))
d.line([cx, cy, sx, sy], fill=GREEN, width=16)

# Blips on the radar (small green dots).
for (bx, by) in [(cx + 150, cy - 240), (cx - 260, cy + 120), (cx + 40, cy + 300)]:
    d.ellipse([bx - 22, by - 22, bx + 22, by + 22], fill=GREEN)

# Shield cutout at center: dark shield with green outline, sits "above" radar.
def shield(cx, cy, w, h):
    # Shield path: top corners, curved sides to a bottom point.
    pts = []
    top = cy - h // 2
    bot = cy + h // 2
    # Left side down, curved to point.
    steps = 40
    for i in range(steps + 1):
        t = i / steps
        x = cx - w / 2 + (w * t)  # top edge left->right
        y = top + 18 * math.sin(math.pi * t)  # slight top curve
        pts.append((x, y))
    # Right side down to bottom point with quadratic curve.
    for i in range(1, steps + 1):
        t = i / steps
        # Quadratic bezier from (cx+w/2, top+...) to (cx, bot), control (cx+w/2, bot*0.75)
        x0, y0 = cx + w / 2, top + 6
        x1, y1 = cx + w / 2, top + h * 0.62
        x2, y2 = cx, bot
        x = (1 - t) ** 2 * x0 + 2 * (1 - t) * t * x1 + t ** 2 * x2
        y = (1 - t) ** 2 * y0 + 2 * (1 - t) * t * y1 + t ** 2 * y2
        pts.append((x, y))
    for i in range(1, steps + 1):
        t = i / steps
        # Mirror back to the left side.
        x0, y0 = cx, bot
        x1, y1 = cx - w / 2, top + h * 0.62
        x2, y2 = cx - w / 2, top + 6
        x = (1 - t) ** 2 * x0 + 2 * (1 - t) * t * x1 + t ** 2 * x2
        y = (1 - t) ** 2 * y0 + 2 * (1 - t) * t * y1 + t ** 2 * y2
        pts.append((x, y))
    return pts

sh = shield(cx, cy - 20, 300, 360)
d.polygon(sh, fill=BG, outline=GREEN, width=20)
# Shield checkmark.
d.line([cx - 70, cy - 30, cx - 15, cy + 40], fill=GREEN, width=26)
d.line([cx - 15, cy + 40, cx + 80, cy - 60], fill=GREEN, width=26)

img.save("assets/icon.png")

# Multi-size .ico
sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
img.save("assets/icon.ico", format="ICO", sizes=sizes)
print("icon.png + icon.ico written")
