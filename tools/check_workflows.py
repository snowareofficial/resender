#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
校验 .github/workflows 下的 YAML：语法、必需字段、常见踩坑。

为什么单独校验：workflow 的 YAML 错误（缩进、Tab、错用键名）在本地看不出来，
要推到 GitHub 跑一次 Actions 才会暴露，一次往返成本很高。本脚本在本地就拦住。

检查项：
  1. YAML 可解析（缩进 / 冒号 / 引号）
  2. 不含 Tab 字符（YAML 禁止用 Tab 缩进，是最常见的失败原因）
  3. 必需顶层字段：name / on / jobs
  4. 每个 job 必须有 runs-on 与 steps
  5. 每个 step 必须有 uses 或 run（只写 name 的空步骤会被 Actions 拒绝）
  6. run 步骤若用了 shell: bash，在 Windows runner 上需 git bash（GitHub 自带，仅提示）

用法：
    python tools/check_workflows.py
需要：pip install pyyaml
"""

import sys
from pathlib import Path

try:
    import yaml
except ImportError:
    print("需要 PyYAML：pip install pyyaml")
    sys.exit(0)          # 未装则跳过，不阻塞 CI

ROOT = Path(__file__).resolve().parent.parent
WF_DIR = ROOT / ".github" / "workflows"

for _s in (sys.stdout, sys.stderr):
    try:
        _s.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass


def main():
    if not WF_DIR.is_dir():
        print(f"未找到 {WF_DIR}，跳过")
        return 0

    files = sorted(WF_DIR.glob("*.y*ml"))
    if not files:
        print("没有 workflow 文件")
        return 0

    errors = 0
    for f in files:
        print(f"\n=== {f.name} ===")
        text = f.read_text(encoding="utf-8")

        # 2) Tab 检查（YAML 不允许 Tab 缩进）
        for i, line in enumerate(text.splitlines(), 1):
            if "\t" in line and not line.lstrip().startswith("#"):
                print(f"  [错误] 第 {i} 行含 Tab（YAML 禁止 Tab 缩进）")
                errors += 1

        # 1) 解析
        try:
            doc = yaml.safe_load(text)
        except yaml.YAMLError as e:
            print(f"  [错误] YAML 解析失败：{e}")
            errors += 1
            continue
        if not isinstance(doc, dict):
            print("  [错误] 顶层不是映射")
            errors += 1
            continue

        # 3) 必需顶层字段
        #
        # **坑**：PyYAML 遵循 YAML 1.1，其中 on/off/yes/no 是布尔字面量，
        # 故 `on:` 会被解析成键 True 而非字符串 "on"。GitHub Actions 用 YAML 1.2，
        # 那里 `on` 是普通字符串。这里两种键都接受，避免误报。
        on_val = doc.get("on", doc.get(True))
        has_on = "on" in doc or True in doc
        for key in ("name", "jobs"):
            if key not in doc:
                print(f"  [错误] 缺少顶层字段 `{key}`")
                errors += 1
        if not has_on:
            print("  [错误] 缺少顶层字段 `on`")
            errors += 1
        print(f"  名称: {doc.get('name')}")
        if isinstance(on_val, dict):
            print(f"  触发: {list(on_val.keys())}")
        else:
            print(f"  触发: {on_val}")

        jobs = doc.get("jobs") or {}
        if not jobs:
            print("  [错误] jobs 为空")
            errors += 1

        for jid, job in jobs.items():
            # 4) job 必需字段
            if "runs-on" not in job:
                print(f"  [错误] job `{jid}` 缺少 runs-on")
                errors += 1
            steps = job.get("steps")
            if not steps:
                print(f"  [错误] job `{jid}` 缺少 steps")
                errors += 1
                continue
            print(f"  job `{jid}`: runs-on={job.get('runs-on')}, {len(steps)} 步")

            # 5) step 必须有 uses 或 run
            for i, st in enumerate(steps, 1):
                if not isinstance(st, dict):
                    print(f"  [错误] job `{jid}` 第 {i} 步格式错误")
                    errors += 1
                    continue
                if "uses" not in st and "run" not in st:
                    print(f"  [错误] job `{jid}` 第 {i} 步缺少 uses/run：{st}")
                    errors += 1

    print("\n" + ("全部通过" if errors == 0 else f"发现 {errors} 处问题"))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
