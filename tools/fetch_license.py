"""生成仓库根目录的 LICENSE 文件（Mulan Public License Version 2）。

优先级：
  1. 仓库内的权威副本（LICENCE-MulanPublV2）——离线、可复现，首选
  2. 官方网络来源（COSCL / SPDX）——仅当本地副本缺失时回退

绝不凭记忆复述法律条文：正文必须来自上述来源之一，并在写入前做完整性校验。

用法：
    python tools/fetch_license.py               # 生成 LICENSE
    python tools/fetch_license.py --check       # 只校验来源完整性，不写文件
    python tools/fetch_license.py --debug       # 打印诊断信息
"""

import argparse
import html
import re
import ssl
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LICENSE = ROOT / "LICENSE"

# 仓库内的权威副本（无扩展名，按用户提供的文件名）
LOCAL_CANDIDATES = [
    "LICENCE-MulanPublV2",
    "LICENCE-MulanPubL-v2",
    "LICENSE-MulanPublV2",
    "assets/MulanPubL-2.0.txt",
]

NETWORK_SOURCES = [
    "http://license.coscl.org.cn/MulanPubL-2.0",
    "https://spdx.org/licenses/MulanPubL-2.0",
]

HEADER = """Copyright (C) 2026 ~ now S.A.

SPDX-License-Identifier: MulanPubL-2.0

Resender is licensed under Mulan Public License, Version 2
("MulanPubL V2" / "MulanPubL-2.0"). The full text of the license follows below.
"""

FOOTER = """

--------------------------------------------------------------------------
THIRD-PARTY COMPONENTS
--------------------------------------------------------------------------

This application bundles / depends on third-party components, each under
its own license. Their licenses take precedence over the above for the
respective component.

Slint <https://slint.dev>
    Copyright (C) SixtyFPS GmbH <info@slint.dev>
    Triple-licensed under any of the following, at your option:
      - GPL-3.0-only
      - LicenseRef-Slint-Royalty-free-2.0   <-- used by this project
      - LicenseRef-Slint-Software-3.0
    SPDX-License-Identifier: GPL-3.0-only OR
                             LicenseRef-Slint-Royalty-free-2.0 OR
                             LicenseRef-Slint-Software-3.0

All remaining Rust crate dependencies are distributed under their own
licenses (predominantly MIT and/or Apache-2.0), as reported by
`cargo metadata` / `cargo deny` / `cargo tree`.
"""

# 完整性校验用的标记（用 unicode 转义书写，避免源码受控制台编码影响）
MARKERS = [
    "\u6728\u5170\u516c\u5171\u8bb8\u53ef\u8bc1" ,            # 木兰公共许可证（中文标题）
    "Mulan Public License",                                    # 英文标题
    "\u6761\u6b3e\u7ed3\u675f",                                # 条款结束
    "END OF THE TERMS AND CONDITIONS",                         # 英文条款结束
    "\u6388\u4e88\u7248\u6743\u8bb8\u53ef",                    # 授予版权许可
    "Grant of Copyright License",
    "\u4ee5\u4e2d\u6587\u7248\u4e3a\u51c6",                    # 以中文版为准（第8条 语言）
    "http://license.coscl.org.cn/MulanPubL-2.0",
]

MIN_BODY_CHARS = 5000


def load_local() -> tuple[str, Path] | None:
    for name in LOCAL_CANDIDATES:
        p = ROOT / name
        if p.is_file():
            try:
                return p.read_text(encoding="utf-8"), p
            except UnicodeDecodeError:
                return p.read_text(encoding="utf-8", errors="replace"), p
    return None


def fetch(url: str, timeout: int = 30) -> str:
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(req, timeout=timeout, context=ctx) as resp:
        return resp.read().decode("utf-8", "replace")


def html_to_text(doc: str) -> str:
    t = re.sub(r"(?is)<(script|style|noscript)[^>]*>.*?</\1>", " ", doc)
    t = re.sub(r"(?is)<br\s*/?>", "\n", t)
    t = re.sub(r"(?is)</(p|div|li|h[1-6]|tr|section|article)>", "\n", t)
    t = re.sub(r"(?s)<[^>]+>", " ", t)
    t = html.unescape(t).replace("\xa0", " ")
    t = re.sub(r"[ \t]+", " ", t)
    t = "\n".join(ln.strip() for ln in t.split("\n"))
    return re.sub(r"\n{3,}", "\n\n", t).strip()


def normalize_body(body: str) -> str:
    """清理来源里的网页导航残留（语言切换链接等），保留许可证正文与双语声明。"""
    lines = body.split("\n")
    nav = re.compile(r"^\s*(CH&EN|EN|CN|ZH|中文|English|简体|繁體)\s*$", re.I)
    i = 0
    while i < len(lines) and (nav.match(lines[i]) or not lines[i].strip()):
        i += 1
    return "\n".join(lines[i:]).strip()


def verify(body: str) -> list[str]:
    """返回缺失的标记列表；为空表示通过完整性校验。"""
    return [m for m in MARKERS if m not in body]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="只校验，不写 LICENSE")
    ap.add_argument("--debug", action="store_true", help="打印诊断信息")
    args = ap.parse_args()

    body = None
    origin = None

    local = load_local()
    if local:
        body, path = local
        origin = f"local:{path.relative_to(ROOT).as_posix()}"
        print(f"loaded local copy: {origin} ({len(body)} chars)")
    else:
        print("no local copy found, falling back to network")
        for url in NETWORK_SOURCES:
            try:
                body = html_to_text(fetch(url))
                origin = f"network:{url}"
                print(f"fetched: {url} ({len(body)} chars)")
                break
            except Exception as e:
                print(f"failed : {url} -> {type(e).__name__}: {e}")

    if body is None:
        print("ERROR: 无法取得许可证正文", file=sys.stderr)
        return 1

    body = normalize_body(body)
    print(f"normalized: {len(body)} chars")

    missing = verify(body)
    too_short = len(body) < MIN_BODY_CHARS
    print(f"integrity: {len(MARKERS) - len(missing)}/{len(MARKERS)} markers present")
    if missing:
        print("missing markers:")
        for m in missing:
            print(f"  - {m!r}")
    print(f"length check: {len(body)} chars (min {MIN_BODY_CHARS}) -> "
          f"{'OK' if not too_short else 'TOO SHORT'}")

    if args.debug:
        idx = body.find("\u6728\u5170\u516c\u5171\u8bb8\u53ef\u8bc1")
        print(f"[debug] title index = {idx}")
        print(repr(body[max(0, idx):idx + 400]))

    if missing or too_short:
        print("ERROR: 完整性校验未通过，已放弃写入以避免污染 LICENSE", file=sys.stderr)
        return 1

    if args.check:
        print(f"PASS: 来源完整可用 ({origin})")
        return 0

    LICENSE.write_text(HEADER + "\n" + body.strip() + "\n" + FOOTER,
                       encoding="utf-8", newline="\n")
    print(f"written: {LICENSE} ({LICENSE.stat().st_size} bytes) from {origin}")
    return 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    sys.exit(main())
