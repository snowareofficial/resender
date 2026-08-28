# Copyright (C) 2026~now S.A.
# SPDX-License-Identifier: MulanPubL-2.0

"""验证 UI 版本号确实来自 Cargo.toml（单一来源，发版自动同步）。

检查分两部分：
  1. 静态接线：src/main.rs 定义 APP_VERSION = env!("CARGO_PKG_VERSION")，
     并把它传给 Slint 的 app_version 属性；ui.slint 把 root.app_version
     绑定到关于页。
  2. 动态比对：运行 `resender --version`，输出必须与 Cargo.toml 的
     version 字段一致。

不修改任何源码，纯只读验证。
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAIN = ROOT / "src" / "main.rs"
UI = ROOT / "ui.slint"
CARGO = ROOT / "Cargo.toml"


def cargo_version() -> str:
    m = re.search(r'^version\s*=\s*"([^"]+)"', CARGO.read_text(encoding="utf-8"), re.M)
    if not m:
        raise SystemExit("无法从 Cargo.toml 解析 version")
    return m.group(1)


def main() -> int:
    ok = True
    ver = cargo_version()
    print(f"Cargo.toml version = {ver}")

    main_src = MAIN.read_text(encoding="utf-8")
    ui_src = UI.read_text(encoding="utf-8")

    # --- 静态接线检查 ---
    checks = [
        ("main.rs 定义 APP_VERSION 常量取自 CARGO_PKG_VERSION",
         r'APP_VERSION:\s*&str\s*=\s*env!\("CARGO_PKG_VERSION"\)', main_src),
        ("main.rs 把 APP_VERSION 注入 Slint 的 app_version 属性",
         r'set_app_version\(ss\(APP_VERSION\)\)', main_src),
        ("ui.slint 声明 app_version 属性",
         r'in-out property <string> app-version', ui_src),
        ("ui.slint 把 root.app-version 绑定到关于页",
         r'app-version:\s*root\.app-version', ui_src),
    ]
    for desc, pattern, src in checks:
        hit = re.search(pattern, src) is not None
        print(f"  [{'PASS' if hit else 'FAIL'}] {desc}")
        ok = ok and hit

    # --- 动态比对 ---
    subprocess.run(["cargo", "build"], cwd=ROOT, capture_output=True,
                   text=True, encoding="utf-8", errors="replace", check=True)
    exe = ROOT / "target" / "debug" / "resender.exe"
    r = subprocess.run([str(exe), "--version"], cwd=ROOT, capture_output=True,
                       text=True, encoding="utf-8", errors="replace", timeout=60)
    out = ((r.stdout or "") + (r.stderr or "")).strip()

    want = f"SWE::Resender {ver}"
    hit = want == out
    print(f"  [{'PASS' if hit else 'FAIL'}] `resender --version` 输出与 Cargo.toml 一致 "
          f"(got: {out!r}, want: {want!r})")
    ok = ok and hit

    print("\n结果:", "PASS — 版本号单一来源为 Cargo.toml" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    # Windows 控制台默认 GBK，无法编码 ✓ 等字符，强制 UTF-8
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    sys.exit(main())
