# Copyright (C) 2026~now S.A.
# SPDX-License-Identifier: MulanPubL-2.0

"""从 assets/logo.png 生成多尺寸 Windows 图标 assets/logo.ico。

build.rs 在 Windows 下用 winres 把 logo.ico 嵌入 exe 资源，
使任务栏 / 资源管理器显示正确图标。

Pillow 的 ICO 保存不支持多帧追加，因此手工组装 ICO 容器：
每个尺寸的帧以 **PNG 编码**（现代 Windows 支持 PNG-in-ICO），
ICO 头 = ICONDIR(6B) + 每帧 ICONDIRENTRY(16B) + 各帧 PNG 数据。

用法：uv run --with Pillow python tools/genico.py
"""

import io
import os
import struct

try:
    from PIL import Image
except ImportError:
    raise SystemExit("需要 Pillow：uv run --with Pillow python tools/genico.py")

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "..", "assets", "logo.png")
DST = os.path.join(HERE, "..", "assets", "logo.ico")

# Windows 图标常见尺寸（256 为 PNG-in-ICO 上限）
SIZES = [16, 24, 32, 48, 64, 128, 256]


def main() -> int:
    if not os.path.exists(SRC):
        print("ERR: 缺少", SRC)
        return 1
    img = Image.open(SRC).convert("RGBA")

    frames = []  # (width, png_bytes)
    for s in SIZES:
        small = img.resize((s, s), Image.LANCZOS)
        buf = io.BytesIO()
        small.save(buf, format="PNG")
        frames.append((s, buf.getvalue()))

    # ---- 组装 ICO ----
    header = struct.pack("<HHH", 0, 1, len(frames))  # reserved, type=1(icon), count
    entries = bytearray()
    image_data = b""
    offset = 6 + 16 * len(frames)
    for s, png in frames:
        # ICONDIRENTRY: width,height(0 表示 256), colors, reserved, planes, bpp,
        #               size, offset
        w = 0 if s >= 256 else s
        h = 0 if s >= 256 else s
        entries += struct.pack(
            "<BBBBHHII", w, h, 0, 0, 1, 32, len(png), offset
        )
        image_data += png
        offset += len(png)

    with open(DST, "wb") as f:
        f.write(header + bytes(entries) + image_data)
    print("wrote", DST, os.path.getsize(DST), "bytes; frames:", [s for s, _ in frames])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
