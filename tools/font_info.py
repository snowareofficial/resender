"""读取 TTF/OTF 的 name 表，输出字体的真实 family / subfamily / PostScript 名。

Slint 的 `default-font-family` 必须匹配字体**内部记录的 family 名**，
而不是文件名，因此在接入前需要用本脚本确认真实名称。

用法：
    python tools/font_info.py [字体路径...]
    # 不带参数时检查项目内的默认字体
"""

import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT = ROOT / "ui" / "MiSans VF.ttf"

# nameID -> 含义
NAMES = {
    1: "family",
    2: "subfamily",
    4: "full_name",
    6: "postscript",
    16: "typo_family",
    17: "typo_subfamily",
}

# 语言优先级：优先英文，其次中文，最后任意
LANG_PRIO = [0x0409, 0x0000, 0x0804, 0x0404]


def parse_name_table(path: Path) -> dict[int, str]:
    data = path.read_bytes()
    if data[:4] not in (b"\x00\x01\x00\x00", b"true", b"ttcf", b"OTTO"):
        raise ValueError(f"不是有效的 sfnt 字体: {data[:4]!r}")

    num_tables = struct.unpack(">H", data[4:6])[0]
    tables: dict[bytes, tuple[int, int]] = {}
    for i in range(num_tables):
        off = 12 + i * 16
        tag = data[off:off + 4]
        _checksum, toff, tlen = struct.unpack(">III", data[off + 4:off + 16])
        tables[tag] = (toff, tlen)

    if b"name" not in tables:
        raise ValueError("字体缺少 name 表")

    toff, _tlen = tables[b"name"]
    fmt, count, string_off = struct.unpack(">HHH", data[toff:toff + 6])

    records: list[tuple[int, int, int, int, int, int]] = []
    for i in range(count):
        r = toff + 6 + i * 12
        plat, enc, lang, nid, ln, off = struct.unpack(">HHHHHH", data[r:r + 12])
        records.append((plat, enc, lang, nid, ln, off))

    def decode(plat: int, enc: int, raw: bytes) -> str | None:
        try:
            if plat == 3 and enc in (1, 10):  # Windows UCS-2 / UCS-4
                return raw.decode("utf-16-be")
            if plat == 0:  # Unicode
                return raw.decode("utf-16-be")
            if plat == 1 and enc == 0:  # Mac Roman
                return raw.decode("mac_roman")
        except Exception:
            return None
        return None

    # 按 (nameID, 语言优先级, 平台) 收集候选
    best: dict[int, tuple[int, int, str]] = {}
    for plat, enc, lang, nid, ln, off in records:
        raw = data[toff + string_off + off: toff + string_off + off + ln]
        text = decode(plat, enc, raw)
        if not text:
            continue
        text = text.strip("\x00").strip()
        if not text:
            continue
        try:
            lprio = LANG_PRIO.index(lang)
        except ValueError:
            lprio = len(LANG_PRIO)
        pprio = 0 if plat in (3, 0) else 1
        key = (lprio, pprio)
        if nid not in best or key < best[nid][:2]:
            best[nid] = (lprio, pprio, text)

    return {nid: v[2] for nid, v in best.items()}


def report(path: Path) -> bool:
    print(f"\n=== {path.name} ===")
    print(f"  路径: {path}")
    if not path.exists():
        print("  [缺失]")
        return False
    size_mb = path.stat().st_size / 1024 / 1024
    print(f"  大小: {size_mb:.2f} MB")
    try:
        names = parse_name_table(path)
    except Exception as e:
        print(f"  [解析失败] {e}")
        return False

    for nid in sorted(NAMES):
        if nid in names:
            print(f"  {NAMES[nid]:<16} {names[nid]}")
    fam = names.get(16) or names.get(1)
    if fam:
        print(f"\n  -> default-font-family 应填: \"{fam}\"")
    else:
        print("\n  [警告] 未找到 family 名，Slint 可能无法按名匹配")
        return False
    return True


def main() -> int:
    args = [Path(a) for a in sys.argv[1:]] or [DEFAULT]
    ok = True
    for p in args:
        ok = report(p) and ok
    return 0 if ok else 1


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    sys.exit(main())
