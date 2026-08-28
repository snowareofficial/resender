"""验证 MiSans VF 字体确实被嵌入 Slint UI 并作为默认字体生效。

编译期不易察觉的失败模式：`import "xx.ttf"` 成功但 family 名对不上，
Slint 会**静默回退**到系统默认字体，界面照常显示但字体没换成本意图的。

因此本脚本做三层校验：
  1. 字体文件的 name 表里确有 family 名（用 tools/font_info.py 的解析器）
  2. ui.slint 中 import 了该字体，且 default-font-family 绑定的值与之一致
  3. 编译产物（二进制）中确实含有该字体的字节特征（证明字体被嵌入）

用法：
    python tools/verify_font.py
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
UI = ROOT / "ui.slint"
THEME = ROOT / "ui" / "theme.slint"
FONT = ROOT / "ui" / "MiSans VF.ttf"
EXE = ROOT / "target" / "debug" / "resender.exe"

sys.path.insert(0, str(ROOT / "tools"))
from font_info import parse_name_table  # noqa: E402


def check(cond: bool, desc: str, detail: str = "") -> bool:
    print(f"  [{'PASS' if cond else 'FAIL'}] {desc}")
    if detail and not cond:
        print(f"         {detail}")
    return cond


def main() -> int:
    print(f"字体文件: {FONT.name}")
    ok = True

    # 1) 字体真实 family 名
    if not FONT.exists():
        print("  [FAIL] 字体文件缺失")
        return 1
    try:
        names = parse_name_table(FONT)
    except Exception as e:
        print(f"  [FAIL] 无法解析字体: {e}")
        return 1
    family = names.get(16) or names.get(1)
    ok &= check(bool(family), "字体 name 表含 family 名", "字体文件可能损坏")
    print(f"        family = {family!r}")

    ui_src = UI.read_text(encoding="utf-8")
    theme_src = THEME.read_text(encoding="utf-8")

    # 2) import + default-font-family 绑定
    ok &= check(
        f'"{FONT.relative_to(ROOT).as_posix()}"' in ui_src,
        "ui.slint 中 import 了该字体文件",
        f"应包含: import \"{FONT.relative_to(ROOT).as_posix()}\";",
    )

    m = re.search(r'font_family:\s*"([^"]+)"', theme_src)
    bound = m.group(1) if m else None
    ok &= check(
        bound == family,
        f"Theme.font_family 与字体真实 family 一致",
        f"Theme.font_family={bound!r} 但字体 family={family!r}（不一致会静默回退）",
    )

    ok &= check(
        "default-font-family" in ui_src,
        "Window 设置了 default-font-family",
        "未设置则字体不会作为全局默认",
    )

    # 3) 二进制中确实嵌入了字体（比对文件首尾特征字节）
    subprocess.run(["cargo", "build"], cwd=ROOT, capture_output=True,
                   text=True, encoding="utf-8", errors="replace", timeout=900)
    if not EXE.exists():
        print("  [FAIL] 未找到编译产物，无法验证嵌入")
        return 1
    font_bytes = FONT.read_bytes()
    head = font_bytes[:64]
    tail = font_bytes[-256:]
    exe_bytes = EXE.read_bytes()
    ok &= check(head in exe_bytes, "二进制含字体头部特征（已嵌入）")
    ok &= check(tail in exe_bytes, "二进制含字体尾部特征（已嵌入）")

    size_mb = EXE.stat().st_size / 1024 / 1024
    print(f"\n  产物大小: {size_mb:.1f} MB（字体 {FONT.stat().st_size/1024/1024:.1f} MB 已计入）")
    print("\n结果:", "PASS — MiSans VF 已嵌入并作为默认字体" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    sys.exit(main())
