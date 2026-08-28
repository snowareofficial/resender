# Copyright (C) 2026~now S.A.
# SPDX-License-Identifier: MulanPubL-2.0

# 生成 Resender 矢量风格 logo：assets/logo.svg (母版) + assets/logo.png (透明，供 Slint)
# 使用 Pillow 绘制几何矢量图形（圆角方块 + 纸飞机线条 + 右下蓝色 S 角标）。
import math, os
from PIL import Image, ImageDraw

SIZE = 512
ACCENT = (59, 91, 219)       # #3b5bdb
ACCENT_SOFT = (238, 242, 255) # #eef2ff
BLUE = (59, 130, 246)         # #3b82f6
CYAN = (6, 182, 212)          # #06b6d4
WHITE = (255, 255, 255)

img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

M = 48  # 外边距
R = 96  # 圆角半径

# 浅色圆角底
d.rounded_rectangle([M, M, SIZE - M, SIZE - M], radius=R, fill=ACCENT_SOFT + (235,))
# 描边
d.rounded_rectangle([M, M, SIZE - M, SIZE - M], radius=R, outline=ACCENT, width=10)

# 纸飞机（线条艺术）：中心偏左上，三顶点
cx, cy = SIZE / 2 - 18, SIZE / 2 - 18
tip = (cx + 70, cy - 60)
bl = (cx - 80, cy + 40)
br = (cx - 40, cy + 80)
d.line([tip, bl], fill=ACCENT, width=12, joint="curve")
d.line([tip, br], fill=ACCENT, width=12, joint="curve")
d.line([bl, br], fill=ACCENT, width=12, joint="curve")
d.line([tip, (cx - 20, cy + 20)], fill=ACCENT, width=8)
# 青色高光点（SOrg 暗示）
d.ellipse([cx - 55 - 9, cy - 45 - 9, cx - 55 + 9, cy - 45 + 9], fill=CYAN)

# 右下蓝色 S 角标徽章
bx, by = SIZE - M - 92, SIZE - M - 92
d.rounded_rectangle([bx, by, bx + 92, by + 92], radius=28, fill=BLUE)
# 白色 S：用椭圆弧采样
sx, sy = bx + 46, by + 46
pts = []
for a in [i * math.pi / 20 for i in range(21)]:          # 上弧
    pts.append((sx + math.cos(a) * 22 - 4, sy - 14 + math.sin(a) * 12))
for a in [math.pi + i * math.pi / 20 for i in range(21)]:  # 下弧
    pts.append((sx + math.cos(a) * 22 + 4, sy + 14 + math.sin(a) * 12))
d.line(pts, fill=WHITE, width=9, joint="curve")

out_png = os.path.join(os.path.dirname(__file__), "..", "assets", "logo.png")
img.save(out_png, "PNG")
print("wrote", out_png, os.path.getsize(out_png), "bytes")

svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">
  <rect x="48" y="48" width="416" height="416" rx="96" fill="#eef2ff" stroke="#3b5bdb" stroke-width="10"/>
  <g fill="none" stroke="#3b5bdb" stroke-width="12" stroke-linecap="round" stroke-linejoin="round">
    <path d="M236 196 L130 296 L170 336 L236 236 Z"/>
    <path d="M236 196 L270 256"/>
  </g>
  <circle cx="181" cy="179" r="9" fill="#06b6d4"/>
  <rect x="372" y="372" width="92" height="92" rx="28" fill="#3b82f6"/>
  <path d="M414 360 q22 0 22 14 q0 12 -22 12 q-22 0 -22 14 q0 14 22 14" fill="none" stroke="#fff" stroke-width="9" stroke-linecap="round"/>
</svg>'''
out_svg = os.path.join(os.path.dirname(__file__), "..", "assets", "logo.svg")
with open(out_svg, "w", encoding="utf-8") as f:
    f.write(svg)
print("wrote", out_svg, os.path.getsize(out_svg), "bytes")
