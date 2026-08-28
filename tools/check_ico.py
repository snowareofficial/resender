# Copyright (C) 2026~now S.A.
# SPDX-License-Identifier: MulanPubL-2.0

"""检查 exe 是否有关联图标（任务栏图标注入验证）。

用 Windows 原生 API（ExtractAssociatedIcon）验证最可靠：
返回非空图标即表示 exe 携带自定义图标资源。
"""

import ctypes
import ctypes.wintypes as wt
import sys


def check(exe: str) -> int:
    try:
        # ExtractAssociatedIconW 提取 exe 的关联图标
        out_path = ctypes.create_unicode_buffer(260)
        idx = wt.UINT(0)
        hicon = ctypes.windll.shell32.ExtractAssociatedIconW(
            None, exe, ctypes.byref(idx)
        )
        if hicon:
            # 读取图标尺寸
            iconinfo = ICONINFO()
            if ctypes.windll.user32.GetIconInfo(hicon, ctypes.byref(iconinfo)):
                print(f"{exe}: 关联图标存在 (bitmap={iconinfo.hbmColor or iconinfo.hbmMask})")
            else:
                print(f"{exe}: 关联图标存在")
            ctypes.windll.user32.DestroyIcon(hicon)
            return 0
        print(f"{exe}: 无关联图标")
        return 1
    except Exception as e:
        print(f"{exe}: 检查失败: {e}")
        return 2


class BITMAP(ctypes.Structure):
    _fields_ = [
        ("bmType", ctypes.c_long),
        ("bmWidth", ctypes.c_long),
        ("bmHeight", ctypes.c_long),
        ("bmWidthBytes", ctypes.c_long),
        ("bmPlanes", ctypes.c_ushort),
        ("bmBitsPixel", ctypes.c_ushort),
        ("bmBits", ctypes.c_void_p),
    ]


class ICONINFO(ctypes.Structure):
    _fields_ = [
        ("fIcon", wt.BOOL),
        ("xHotspot", wt.DWORD),
        ("yHotspot", wt.DWORD),
        ("hbmMask", wt.HBITMAP),
        ("hbmColor", wt.HBITMAP),
    ]


if __name__ == "__main__":
    sys.exit(check(sys.argv[1]))
