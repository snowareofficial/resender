"""报告 C 盘占用大户，帮助判断可清理项（只读，不删除任何内容）。"""
import shutil
from pathlib import Path

GB = 1024 ** 3


def dir_size(p: Path) -> int:
    if not p.exists():
        return 0
    total = 0
    for f in p.rglob("*"):
        try:
            if f.is_file():
                total += f.stat().st_size
        except OSError:
            pass
    return total


def main() -> None:
    free, total = shutil.disk_usage("C:/")[1:] if False else (
        shutil.disk_usage("C:/").free, shutil.disk_usage("C:/").total)
    print(f"C: free {free/GB:.2f} GB / total {total/GB:.2f} GB")

    targets = [
        ("cargo registry", Path(r"C:\Users\sakeen\.cargo\registry")),
        ("cargo bin", Path(r"C:\Users\sakeen\.cargo\bin")),
        ("project target", Path.cwd() / "target"),
        ("user temp", Path(r"C:\Users\sakeen\AppData\Local\Temp")),
        ("rustup", Path(r"C:\Users\sakeen\.rustup")),
    ]
    for name, p in targets:
        print(f"  {name:<16} {dir_size(p)/GB:>8.2f} GB   {p}")


if __name__ == "__main__":
    main()
