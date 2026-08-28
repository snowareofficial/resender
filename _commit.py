"""生成 git 提交信息。"""
import os

LINES = [
    "feat: auto-detect system language via swi18n; license jurisdiction clause",
    "",
    "- src/i18n.rs: Resender message catalog (zh-CN + en) on top of the swi18n",
    "  crate (system-language detection + fallback chain, zero-dep)",
    "- startup fills the i18n table by detected language; script setup_i18n",
    "  now defaults to no-op and keeps override ability (script wins)",
    "- new ui::get_lang() primitive for scripts (BCP-47, e.g. zh-CN)",
    "- LICENSE adds a jurisdiction clause: disputes are under the exclusive",
    "  jurisdiction of courts at the principal contributor's place of",
    "  residence, the People's Republic of China",
]

path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "_commit_msg.txt")
with open(path, "w", encoding="utf-8") as f:
    f.write("\n".join(LINES) + "\n")
print("written:", path)
