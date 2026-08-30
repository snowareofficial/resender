#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
用 GitHub CLI 建立「Gitee 权威源 -> GitHub 镜像 + CI」的仓库。

Gitee 保持为唯一权威源；GitHub 侧只做镜像与 Actions 构建，
日常同步由 .github/workflows/sync-from-gitee.yml 定时完成。

**前置（需人工一次，脚本无法代劳）**：
    1) 安装 GitHub CLI
       Windows: winget install --id GitHub.cli -e
       macOS  : brew install gh
    2) 登录（浏览器交互）
       gh auth login --web

用法：
    python tools/setup_github_mirror.py --check          # 只检查环境是否就绪
    python tools/setup_github_mirror.py --public         # 建公开仓库并首次同步
    python tools/setup_github_mirror.py --private \
        --gitee-user <用户> --gitee-token <Gitee私人令牌>
"""

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# Windows 控制台默认 GBK，输出中文/符号会 UnicodeEncodeError，统一改 UTF-8。
# 标记一律用 ASCII（[OK]/[X]/[-]），即使编码切换失败也能正常打印。
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

ROOT = Path(__file__).resolve().parent.parent
GITEE_REPO_PATH = "snoware/resender"     # Gitee 上的 组织/仓库
DEFAULT_NAME = "resender"                # GitHub 仓库名


def run(cmd, cwd=None, check=False, capture=True):
    """执行命令，返回 (returncode, stdout+stderr)。"""
    proc = subprocess.run(
        cmd, cwd=cwd, capture_output=capture, text=True,
        encoding="utf-8", errors="replace", shell=(isinstance(cmd, str)),
    )
    out = (proc.stdout or "") + (proc.stderr or "")
    if check and proc.returncode != 0:
        raise SystemExit(f"命令失败 [{' '.join(cmd) if not isinstance(cmd, str) else cmd}]\n{out}")
    return proc.returncode, out


def has(tool):
    return shutil.which(tool) is not None


def find_gh():
    """定位 gh 可执行文件。

    gh 常出现「已安装但不在当前 PATH」的情况（winget 装完未重开终端、
    Scoop / 自定义目录等），这里按平台枚举常见安装位置兜底。
    """
    p = shutil.which("gh")
    if p:
        return p
    candidates = []
    if sys.platform == "win32":
        bases = [
            Path(r"C:\Program Files\GitHub CLI"),
            Path(r"C:\Program Files (x86)\GitHub CLI"),
            Path.home() / "AppData" / "Local" / "Programs" / "GitHub CLI",
            Path.home() / "scoop" / "apps" / "gh" / "current" / "bin",
            Path.home() / "scoop" / "shims",
        ]
        candidates += [b / "gh.exe" for b in bases]
    else:
        candidates += [
            Path("/usr/local/bin/gh"),
            Path("/opt/homebrew/bin/gh"),
            Path.home() / ".local" / "bin" / "gh",
        ]
    for c in candidates:
        if c.exists():
            return str(c)
    return None


#: gh 可执行文件路径（由 check_env 填充）
GH = None


def check_env():
    """检查 git / gh 是否可用且已登录。返回 GitHub 用户名。"""
    ok = True
    if not has("git"):
        print("[X] 未找到 git，请先安装 Git")
        ok = False
    else:
        rc, out = run(["git", "--version"])
        print(f"[OK] git: {out.strip()}")

    global GH
    GH = find_gh()
    if not GH:
        print("[X] 未找到 GitHub CLI (gh)")
        print("    Windows 安装：winget install --id GitHub.cli -e")
        print("    macOS   安装：brew install gh")
        print("    其他    ：https://cli.github.com/")
        ok = False
        user = None
    else:
        rc, out = run([GH, "--version"])
        print(f"[OK] gh: {out.splitlines()[0].strip() if out else '(版本未知)'}  <- {GH}")
        rc, out = run([GH, "auth", "status"])
        if rc != 0:
            print("[X] gh 尚未登录，请执行：gh auth login --web")
            ok = False
            user = None
        else:
            rc, out = run([GH, "api", "user", "--jq", ".login"])
            user = out.strip() if rc == 0 else None
            print(f"[OK] gh 已登录：{user or '(未知用户)'}")
    return ok, user


def ensure_repo(name, visibility, owner):
    """仓库已存在则复用，不存在则创建。返回 owner/name。"""
    full = f"{owner}/{name}"
    rc, out = run([GH, "repo", "view", full])
    if rc == 0:
        print(f"[OK] 仓库已存在，复用：{full}")
        return full
    cmd = [GH, "repo", "create", full, f"--{visibility}",
           "--description", "SWE::Resender — Rhai 驱动的 Resend 发信工具（Gitee 镜像）",
           "--source", ".", "--remote", "github"]
    # --source . 会立即推送当前仓库；这里先建空仓库再由 mirror 推送更干净，
    # 故不用 --source，改用 --confirm 建空仓库。
    cmd = [GH, "repo", "create", full, f"--{visibility}",
           "--description", "SWE::Resender — Rhai 驱动的 Resend 发信工具（Gitee 镜像）"]
    rc, out = run(cmd)
    if rc != 0:
        if "already exists" in out.lower():
            print(f"[OK] 仓库已存在，复用：{full}")
            return full
        raise SystemExit(f"创建仓库失败：\n{out}")
    print(f"[OK] 已创建仓库：{full} ({visibility})")
    return full


def setup_secrets(full, gitee_user, gitee_token):
    """配置 Gitee 拉取凭据（私有仓库才需要）。"""
    if not (gitee_user and gitee_token):
        print("[-] 未提供 Gitee 凭据，按「Gitee 公开仓库」配置（同步无需 secret）")
        return
    for name, value in (("GITEE_USER", gitee_user), ("GITEE_TOKEN", gitee_token)):
        rc, out = run([GH, "secret", "set", name, "--repo", full, "--body", value])
        print(f"{'[OK]' if rc == 0 else '[X]'} 设置 secret {name}")
        if rc != 0:
            print(out)


def ensure_workflows_enabled(full):
    """确认定时同步工作流已就位（仓库自带 .github/workflows 即自动启用）。"""
    wf = ROOT / ".github" / "workflows" / "sync-from-gitee.yml"
    if wf.exists():
        print(f"[OK] 同步工作流已就位：{wf.relative_to(ROOT).as_posix()}")
    else:
        print("[X] 未找到 .github/workflows/sync-from-gitee.yml，同步不会自动发生")


def first_mirror(full):
    """首次全量镜像推送：Gitee --mirror-> GitHub。"""
    print("\n开始首次镜像推送（包含 19 MiB 字体，视网速需要一些时间）…")
    run([GH, "auth", "setup-git"])     # 让 git 复用 gh 的凭据
    tmp = Path(tempfile.mkdtemp(prefix="resender-mirror-"))
    mirror = tmp / "repo.git"
    try:
        src = f"https://gitee.com/{GITEE_REPO_PATH}.git"
        rc, out = run(["git", "clone", "--mirror", src, str(mirror)])
        if rc != 0:
            print(f"[X] 克隆 Gitee 失败：\n{out}")
            print("  若是私有仓库，请先配置 Gitee 凭据或临时使用带令牌的 URL")
            return False

        rc, out = run(["git", "remote", "set-url", "--push", "origin",
                       f"https://github.com/{full}.git"], cwd=mirror)
        if rc != 0:
            print(f"[X] 设置推送地址失败：\n{out}")
            return False

        rc, out = run(["git", "push", "--mirror", "origin"], cwd=mirror)
        if rc != 0:
            print(f"[X] 镜像推送失败：\n{out}")
            return False
        print("[OK] 首次镜像推送完成")
        return True
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description="用 gh 建立 Gitee→GitHub 镜像仓库")
    ap.add_argument("--check", action="store_true", help="只检查环境")
    ap.add_argument("--name", default=DEFAULT_NAME, help=f"GitHub 仓库名（默认 {DEFAULT_NAME}）")
    ap.add_argument("--public", action="store_true", help="公开仓库")
    ap.add_argument("--private", action="store_true", help="私有仓库")
    ap.add_argument("--gitee-user", help="Gitee 用户名（私有仓库时用于 Actions 拉取）")
    ap.add_argument("--gitee-token", help="Gitee 私人令牌（只读即可）")
    args = ap.parse_args()

    ok, user = check_env()
    if args.check or not ok:
        print("\n环境就绪" if ok else "\n环境未就绪，请先完成上述步骤")
        return 0 if ok else 1

    if args.public == args.private:       # 都没给或都给了
        # 默认公开：字体已不进仓库（构建时从官方源获取），
        # 不再有「公开即等于分发字体」的合规顾虑；公开仓库的 Actions 免费额度不限。
        # 代码在 Gitee 上本就是公开的（gitee.com/snoware/resender），公开镜像不增加暴露面。
        visibility = "public"
        print("[-] 未指定可见性，默认 public（可用 --private 改为私有）")
    else:
        visibility = "private" if args.private else "public"

    full = ensure_repo(args.name, visibility, user)
    setup_secrets(full, args.gitee_user, args.gitee_token)
    ensure_workflows_enabled(full)
    pushed = first_mirror(full)

    print("\n" + "=" * 60)
    print(f"GitHub 仓库：https://github.com/{full}")
    print(f"可见性：{visibility}")
    print(f"首次同步：{'完成' if pushed else '未完成（见上方错误）'}")
    if pushed:
        print("\n后续：")
        print("  [-] 改代码仍提交到 Gitee，GitHub 每 6 小时自动同步")
        print(f"  [-] 立即同步：gh workflow run sync-from-gitee.yml --repo {full}")
        print(f"  [-] 查看构建：gh run list --repo {full}")
        print("  [-] 字体已随仓库分发（ui/MiSans VF.ttf），CI 构建会内嵌它")
    return 0 if pushed else 1


if __name__ == "__main__":
    sys.exit(main())
