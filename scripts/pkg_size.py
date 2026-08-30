#!/usr/bin/env python3
"""列出 cargo package 将包含的文件，按体积降序，定位体积大头。

用法：
  cargo package --allow-dirty --list | python scripts/pkg_size.py

用途：crates.io 对上传体积敏感（大包易触发 CDN 503），
      需清楚体积来自哪里，才好决定如何裁剪。
"""
import os
import sys

entries = []
for line in sys.stdin:
    p = line.strip()
    if not p:
        continue
    try:
        size = os.path.getsize(p)
    except OSError:
        size = -1
    entries.append((size, p))

entries.sort(reverse=True)
total = sum(s for s, _ in entries if s > 0)

print(f"{'size':>12}  {'%':>5}  file")
print("-" * 70)
for size, p in entries[:20]:
    pct = (size / total * 100) if total else 0
    if size >= 1024 * 1024:
        shown = f"{size / 1024 / 1024:.2f} MiB"
    elif size >= 1024:
        shown = f"{size / 1024:.1f} KiB"
    else:
        shown = f"{size} B"
    print(f"{shown:>12}  {pct:5.1f}%  {p}")

print("-" * 70)
print(f"{'TOTAL':>12}  {total / 1024 / 1024:.2f} MiB  ({len(entries)} files)")
