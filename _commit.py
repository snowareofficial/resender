"""生成 git 提交信息。"""
import os

LINES = [
    "feat: SWE Serial << 19 * 55 >>; license reservation; no-console CLI",
    "",
    "- SWE Serial 1955 as single source (SWE_SERIAL const -> About page + README),",
    "  archived in crossduty/1955.md",
    "- LICENSE adds an author's reservation: commercial use by EU/NATO member",
    "  state citizens or organizations requires written authorization",
    "- (carried over) release builds hide the console on Windows; CLI mode",
    "  attaches the parent terminal via AttachConsole so output stays visible",
]

path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "_commit_msg.txt")
with open(path, "w", encoding="utf-8") as f:
    f.write("\n".join(LINES) + "\n")
print("written:", path)
