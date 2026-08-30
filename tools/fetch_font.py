#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
从官方源获取界面字体 MiSans VF，供 build.rs 内嵌。

**为什么不在仓库里放字体**：MiSans 许可禁止「单独分发字体」，19 MiB 的字体
文件放在公开仓库里等于把字体本身分发出去了。改为构建时从官方源获取，仓库
只留获取脚本 —— 得到的仍是同一个官方可变字体，但不再分发字体文件本身。

**为什么能只下 13.7 MB 而不是整个 217 MB 的包**：ZIP 的中央目录在文件尾部，
且服务器支持 Range 请求。流程是：
    1) Range 取尾部 256 KB -> 解析 EOCD -> 定位中央目录
    2) 从中央目录查到目标条目的压缩数据偏移与大小
    3) Range 只取那一段（约 13.8 MB）-> inflate 还原
官方整包 217 MB，按需取用可省 94% 流量与时间。

字体缺失时 build.rs 仍能构建（回退系统字体），因此本脚本失败不应中断构建。

用法：
    python tools/fetch_font.py                 # 字体不存在时才下载
    python tools/fetch_font.py --force         # 无论是否存在都重新下载
    python tools/fetch_font.py --list          # 只列出包内文件名，不下载
    python tools/fetch_font.py --diff          # 与现有字体比对（SM3/大小），不写入
"""

import argparse
import hashlib
import shutil
import struct
import sys
import tempfile
import urllib.request
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FONT_REL = Path("ui") / "MiSans VF.ttf"
ZIP_URL = "https://hyperos.mi.com/font-download/MiSans.zip"
# 官方包内目标条目（可变字体）。若小米改了包内结构，用 --list 查新名字。
TARGET_SUFFIX = "可变字体/MiSansVF.ttf"

UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120 Safari/537.36"
TAIL = 256 * 1024
EOCD_SIG = b"PK\x05\x06"
CEN_SIG = b"PK\x01\x02"
LOC_SIG = b"PK\x03\x04"

for _s in (sys.stdout, sys.stderr):
    try:
        _s.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass


def log(msg):
    print(msg, flush=True)


class ZipEntry:
    __slots__ = ("name", "usize", "csize", "method", "lho", "nlen", "elen")

    def __init__(self, name, usize, csize, method, lho, nlen, elen):
        self.name, self.usize, self.csize = name, usize, csize
        self.method, self.lho, self.nlen, self.elen = method, lho, nlen, elen


def fetch(start=None, length=None, timeout=120):
    """Range 下载。start=None 时取尾部 length 字节。"""
    rng = f"bytes=-{length}" if start is None else f"bytes={start}-{start + length - 1}"
    req = urllib.request.Request(ZIP_URL, headers={"User-Agent": UA, "Range": rng})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def total_size():
    req = urllib.request.Request(ZIP_URL, method="HEAD", headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=60) as r:
        return int(r.headers["Content-Length"])


def read_central_directory():
    """解析 ZIP 中央目录，返回条目列表。"""
    total = total_size()
    tail = fetch(None, TAIL, timeout=60)
    tail_off = total - len(tail)

    pos = tail.rfind(EOCD_SIG)
    if pos < 0:
        raise SystemExit("未找到 EOCD：ZIP 结构异常（或远端已改版）")
    entries, cen_size, cen_off = struct.unpack_from("<HII", tail, pos + 10)

    if cen_off >= tail_off:
        cen = tail[cen_off - tail_off: cen_off - tail_off + cen_size]
    else:
        cen = fetch(cen_off, cen_size, timeout=60)
    if len(cen) < cen_size:
        raise SystemExit(f"中央目录不完整：{len(cen)}/{cen_size}")

    out, p = [], 0
    for _ in range(entries):
        if cen[p:p + 4] != CEN_SIG:
            break
        method = struct.unpack_from("<H", cen, p + 10)[0]
        csize, usize = struct.unpack_from("<II", cen, p + 20)
        nlen, elen, clen = struct.unpack_from("<HHH", cen, p + 28)
        lho = struct.unpack_from("<I", cen, p + 42)[0]
        name = cen[p + 46: p + 46 + nlen].decode("utf-8", "replace")
        out.append(ZipEntry(name, usize, csize, method, lho, nlen, elen))
        p += 46 + nlen + elen + clen
    return out


def pick(entries):
    """挑出目标字体条目（排除 macOS 元数据 __MACOSX/._xxx）。"""
    cands = [e for e in entries
             if e.name.endswith(TARGET_SUFFIX) and not e.name.startswith("__MACOSX/")]
    if not cands:
        names = "\n  ".join(e.name for e in entries if e.name.lower().endswith(".ttf"))
        raise SystemExit(
            f"包内找不到 {TARGET_SUFFIX}。\n包内的 ttf 有：\n  {names}\n"
            f"若小米改了目录结构，请用 --list 查看并更新 TARGET_SUFFIX。"
        )
    return cands[0]


def download_entry(entry):
    """只下载目标条目的压缩数据并解压。返回原始字节。"""
    # 先取 local header（含实际 nlen/elen，可能与中央目录不同）
    head = fetch(entry.lho, 30 + 512, timeout=60)
    if head[:4] != LOC_SIG:
        raise SystemExit("local header 签名错误")
    lnlen, lelen = struct.unpack_from("<HH", head, 26)
    data_off = entry.lho + 30 + lnlen + lelen

    # 再取压缩数据（多留 64 字节余量）
    blob = fetch(data_off, entry.csize + 64, timeout=180)
    if entry.method == 0:               # stored
        raw = blob[:entry.usize]
    elif entry.method == 8:             # deflate
        raw = zlib.decompressobj(-15).decompress(blob, entry.usize + 1)
    else:
        raise SystemExit(f"不支持的压缩方法: {entry.method}")

    if len(raw) != entry.usize:
        raise SystemExit(f"解压后大小不符：{len(raw)} != {entry.usize}")
    return raw


def check_ttf(raw):
    """基本校验：SFNT 魔数 + 非空。"""
    if len(raw) < 12:
        raise SystemExit(f"文件过小：{len(raw)} 字节")
    magic = raw[:4]
    if magic not in (b"\x00\x01\x00\x00", b"true", b"ttcf", b"OTTO"):
        raise SystemExit(f"不是有效的字体文件（魔数 {magic.hex()}）")


def quick_fingerprint(data):
    """快速指纹：首尾各 1 MiB + 总大小的 SHA-256。

    **不用完整摘要**：对 19 MiB 数据做完整哈希（尤其纯 Python 实现的 SM3）
    要跑好几分钟；这里只需区分「是不是同一份字体」，采样 + 大小已足够。
    """
    h = hashlib.sha256()
    h.update(len(data).to_bytes(8, "big"))
    h.update(data[:1024 * 1024])
    if len(data) > 2 * 1024 * 1024:
        h.update(data[-1024 * 1024:])
    return h.hexdigest()


def main():
    ap = argparse.ArgumentParser(description="从官方源获取 MiSans VF 字体")
    ap.add_argument("--force", action="store_true", help="已存在也重新下载")
    ap.add_argument("--list", action="store_true", help="只列出包内文件，不下载")
    ap.add_argument("--diff", action="store_true", help="与现有字体比对，不写入")
    ap.add_argument("--output", help=f"输出路径（默认 {FONT_REL.as_posix()}）")
    args = ap.parse_args()

    dest = Path(args.output) if args.output else ROOT / FONT_REL

    log("读取官方包中央目录…")
    entries = read_central_directory()
    log(f"包内 {len(entries)} 个条目")

    if args.list:
        for e in entries:
            if e.usize and ("ttf" in e.name.lower() or "otf" in e.name.lower()):
                log(f"  {e.name}  ({e.usize/1048576:.1f} MB)")
        return 0

    entry = pick(entries)
    log(f"目标: {entry.name}")
    log(f"  原始 {entry.usize/1048576:.1f} MB / 压缩 {entry.csize/1048576:.1f} MB")

    if args.diff or (dest.exists() and not args.force):
        if not dest.exists():
            log("本地无字体，继续下载")
        else:
            local = dest.read_bytes()
            log(f"本地: {dest.relative_to(ROOT).as_posix()} "
                f"({len(local)/1048576:.2f} MB, 指纹 {quick_fingerprint(local)[:16]}…)")
            log(f"官方: {entry.usize/1048576:.2f} MB (包内记录)")
            if len(local) == entry.usize:
                log("大小一致；未下载远端数据，无法确认内容是否逐字节相同。")
            else:
                log(f"大小不同：本地 {len(local)} != 官方 {entry.usize}")
                log("  -> 官方包可能已更新；用 --force 重新下载可取得最新版。")
            if args.diff:
                log("\n--diff：未写入。确认无误后可用 --force 更新。")
                return 0
            if not args.force:
                log("\n字体已存在，跳过下载（用 --force 强制更新）")
                return 0

    log(f"下载压缩数据（约 {entry.csize/1048576:.1f} MB，非整包 217 MB）…")
    raw = download_entry(entry)
    check_ttf(raw)
    log(f"解压完成：{len(raw)/1048576:.2f} MB，指纹 {quick_fingerprint(raw)[:16]}…")

    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = dest.with_suffix(dest.suffix + ".tmp")
    tmp.write_bytes(raw)
    tmp.replace(dest)
    log(f"已写入 {dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
