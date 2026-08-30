#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
校验两种 feature 形态都能构建：

    default（含 gui）      -> 完整桌面应用（Slint GUI + CLI）
    --no-default-features  -> 纯 CLI 二进制（不含 Slint 运行时）

只汇报 **本 crate 自身** 的诊断：第三方 / path 依赖（swsml 等）的警告
不计入，避免它们的变化掩盖本项目的问题。

用法：
    python tools/check_builds.py            # cargo check（快）
    python tools/check_builds.py --build    # cargo build（慢，但能验链接）

顺带校验 .github/workflows 的 YAML（语法错误要到 Actions 跑一次才暴露，
本地先拦住更省事）；PyYAML 未安装时自动跳过该步。
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# 本 crate 的诊断行形如：warning: ... --> src\main.rs:12:5
# 依赖的诊断指向 ..\sml\rust\src\... 或 registry，据此区分。
OWN_SRC = re.compile(r"-->\s+(src[\\/]|\.\.[\\/])")

VARIANTS = [
    ("纯 CLI (--no-default-features)", ["--no-default-features"]),
    ("默认 (gui)", []),
]


def run(args):
    cmd = ["cargo", "check", "--message-format=short"] + args
    proc = subprocess.run(
        cmd, cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    return proc.returncode, proc.stdout + proc.stderr


def own_diagnostics(output):
    """挑出指向本 crate 源码的 error / warning 行。"""
    lines = output.splitlines()
    keep = []
    for i, ln in enumerate(lines):
        if not re.match(r"(src[\\/].*|.*):\d+:\d+:\s+(error|warning)", ln):
            continue
        if OWN_SRC.search(ln) or re.match(r"src[\\/]", ln):
            keep.append(ln.strip())
    # 短格式（--message-format=short）是 "path:line:col: level: msg"
    if keep:
        return keep
    # 回退：非短格式时，warning/error 标题行 + 紧随的 --> src/... 行
    out = []
    for i, ln in enumerate(lines):
        if re.match(r"^(warning|error)(\[E\d+\])?:", ln):
            for j in range(i + 1, min(i + 4, len(lines))):
                if OWN_SRC.search(lines[j]):
                    out.append(f"{ln.strip()}  ({lines[j].strip()})")
                    break
    return out


def main():
    build_mode = "--build" in sys.argv
    if build_mode:
        VARIANTS_CMD = [(n, a) for n, a in VARIANTS]
    failed = False

    for name, extra in VARIANTS:
        cmd = ["cargo", "build" if build_mode else "check"] + extra
        print(f"\n{'=' * 62}\n{name}\n  $ {' '.join(cmd)}\n{'=' * 62}")
        proc = subprocess.run(
            cmd, cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace"
        )
        output = proc.stdout + proc.stderr
        ok = proc.returncode == 0
        print("结果：", "通过" if ok else f"失败 (rc={proc.returncode})")
        if not ok:
            failed = True
            # 只打印 error 相关片段
            errs = [l for l in output.splitlines() if l.startswith("error")]
            for e in errs[:30]:
                print("   ", e)
            continue

        diags = own_diagnostics(output)
        if diags:
            print(f"本 crate 诊断 {len(diags)} 条：")
            for d in diags[:40]:
                print("   ", d)
        else:
            print("本 crate 诊断：无")
        for l in output.splitlines():
            if l.strip().startswith("Finished"):
                print("   ", l.strip())

    # 顺带校验 workflow YAML（缺 PyYAML 时脚本内部已降级为跳过）
    print(f"\n{'=' * 62}\nworkflow YAML 校验\n{'=' * 62}")
    wf = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "check_workflows.py")],
        cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    print((wf.stdout or "") + (wf.stderr or ""))
    if wf.returncode != 0:
        failed = True

    print("\n" + ("全部通过" if not failed else "存在失败"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
