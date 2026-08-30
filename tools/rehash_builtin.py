#!/usr/bin/env python3
# Copyright (C) 2026~now S.A.
# SPDX-License-Identifier: MulanPubL-2.0
"""
重新生成为置脚本的 SM3 哈希快照。

适用场景：
  开发者修改了 scripts/default.rhai（内置脚本逻辑）后，编译会因
  「哈希与快照不符」而失败。若本次修改是预期的，运行本脚本重算快照：

      python tools/rehash_builtin.py

  随后重新 `cargo build` 即可通过。

安全约束：
  仅当修改是开发者自己预期的、可信的改动时才应重算快照。
  若脚本被外部篡改，绝不应重算快照，而应从版本控制恢复。
"""
import hashlib
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCRIPT = os.path.join(ROOT, "scripts", "default.rhai")
SNAPSHOT = os.path.join(ROOT, "scripts", "default.rhai.sm3")


def main() -> None:
    if not os.path.isfile(SCRIPT):
        raise SystemExit(f"找不到内置脚本: {SCRIPT}")
    with open(SCRIPT, "rb") as f:
        data = f.read()
    sm3 = hashlib.new("sm3", data).hexdigest()
    with open(SNAPSHOT, "w", encoding="utf-8") as f:
        f.write(sm3 + "\n")
    print(f"已更新快照: {SNAPSHOT}")
    print(f"SM3 = {sm3}")


if __name__ == "__main__":
    main()
