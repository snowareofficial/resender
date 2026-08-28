"""为仓库源文件统一添加许可证声明头（幂等，可重复执行）。

Mulan PubL v2 附录建议在每个源文件头部附上许可声明。本脚本采用
SPDX 简写形式（业界通用且不易失真）：

    // Copyright (C) 2026~now S.A.
    // SPDX-License-Identifier: MulanPubL-2.0

处理范围：src/*.rs、build.rs、ui/**/*.slint、scripts/*.rhai、tools/*.py
已包含 SPDX-License-Identifier 的文件会被跳过，不会重复添加。

用法：
    python tools/add_license_header.py           # 执行添加
    python tools/add_license_header.py --check   # 只检查哪些文件缺失
"""

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

COPYRIGHT = "Copyright (C) 2026~now S.A."
SPDX = "SPDX-License-Identifier: MulanPubL-2.0"

# 目标文件（相对仓库根）
TARGETS: list[Path] = []
TARGETS += sorted((ROOT / "src").glob("*.rs"))
TARGETS += [ROOT / "build.rs"]
TARGETS += sorted((ROOT / "ui").rglob("*.slint"))
TARGETS += sorted((ROOT / "scripts").glob("*.rhai"))
TARGETS += sorted((ROOT / "scripts").glob("*.rhai"))
TARGETS += sorted((ROOT / "tools").glob("*.py"))

# 各扩展名的注释前缀
PREFIX = {".rs": "// ", ".slint": "// ", ".rhai": "// ", ".py": "# "}


def header_for(path: Path) -> str:
    p = PREFIX.get(path.suffix.lower(), "// ")
    return f"{p}{COPYRIGHT}\n{p}{SPDX}\n\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="只检查缺失情况，不修改文件")
    args = ap.parse_args()

    seen: set[Path] = set()
    missing: list[Path] = []
    added: list[Path] = []
    skipped: list[Path] = []

    for path in TARGETS:
        path = path.resolve()
        if path in seen or not path.is_file():
            continue
        seen.add(path)

        text = path.read_text(encoding="utf-8")
        if SPDX in text:
            skipped.append(path)
            continue

        missing.append(path)
        if args.check:
            continue

        path.write_text(header_for(path) + text, encoding="utf-8", newline="\n")
        added.append(path)

    rel = lambda p: p.relative_to(ROOT).as_posix()  # noqa: E731

    print(f"scanned : {len(seen)} files")
    print(f"has SPDX: {len(skipped)}")
    print(f"missing : {len(missing)}")
    for p in missing:
        print(f"  - {rel(p)}")
    if not args.check and added:
        print(f"added header to {len(added)} files")

    if args.check:
        print("\n结果:", "PASS — 所有源文件均已声明许可" if not missing
              else f"FAIL — {len(missing)} 个文件缺少声明")
        return 0 if not missing else 1
    return 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    sys.exit(main())
