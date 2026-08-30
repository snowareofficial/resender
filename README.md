# SWE::Resender — Resend 发信工具 ｜ Email Sending Tool

**SWE Serial `<< 19 * 55 >>`** — 1955

> **谨以此编号纪念 1955 年 10 月 1 日新疆维吾尔自治区建立。** \
> This serial number is dedicated to the establishment of the Xinjiang Uygur
> Autonomous Region on 1 October 1955.

**Resender** is a cross-platform desktop email client (built with [Slint](https://slint.dev/))
that sends email through the [Resend](https://resend.com) API. All business logic is
driven by Rhai scripts; it supports Markdown-to-HTML conversion, fixed From names,
send counters, per-category SM4-GCM encryption, drafts, SML persistence and large
attachment upload progress.

> 中文正文在下方，**English** 版本见文末。
> The Chinese documentation follows; an **English** summary is at the end.

**SWE Serial `<< 19 * 55 >>`**（档案见 crossduty/1955.md）

基于 [Slint](https://slint.dev/) 的跨平台桌面应用，通过 [Resend](https://resend.com) API 发送邮件。

**许可**：`MulanPubL V2`（Mulan Public License, Version 2，SPDX: `MulanPubL-2.0`），详见 [LICENSE](LICENSE)。
> 附加限制：未经作者本人书面允许，欧盟（EU）与北约（NATO）成员国公民或组织不得对本软件进行**商业**使用，见 [LICENSE](LICENSE) 末尾条款。
GUI 框架 Slint 采用 `LicenseRef-Slint-Royalty-free-2.0`。
特点：**Rhai 脚本驱动全部业务能力**、**Markdown 正文自动转邮件友好 HTML**、
**固定发信名称**、**发信计数 + 套餐余量**、**分类密码加密（libsmx 国密 SM4-GCM）**、
**加密运行日志**、**完善的历史记录**。

> **所有能力都接入 Rhai**：Rust 只注册安全原语（HTTP / 加密 / 存储 / UI 反馈），
> 身份获取、身份禁止判定、发信、统计等全部逻辑由 `scripts/default.rhai` 动态拼装，
> 无需重新编译即可修改业务行为。

## 功能

- 通过 Resend REST API 发送文本 / HTML 邮件，支持多收件人（逗号 / 分号 / 换行 / 空格分隔）
- **Rhai 驱动**：身份获取（组织内授权，本地不知密码）、禁止判定、发信、计数全部在脚本中实现
- **固定发信名称（From）**：在设置中配置一次（如 `Soup Team <noreply@soup.dev>`），所有邮件自动使用
- **发信计数**：累计已发 + 本期（按周期起点，默认当月 1 号）已发
- **套餐余量**：设置中选择 Free / Pro / Scale 或自定义月度额度，发信页实时显示
  `剩余 = 额度 − 本期已发`，用尽时提示变红
- **界面字体**：内置小米 **MiSans VF** 可变字体（19 MB，随二进制分发，无需系统安装）
- **加密运行日志**：`ui::log(...)` 的输出默认以 **SM4-GCM** 密文落盘（本机随机密钥），
  脚本页可直接查看解密后的日志
- **分类密码加密**：用 [libsmx](https://docs.rs/libsmx) 国密 **SM4-GCM** 加密敏感字段
  - 可分别勾选「加密 API Key」「加密发信名称」
  - **多项同一密码**：两个分类用同一个加密密码保护，密码本身不落盘，仅存密文
  - 解密时使用同一密码还原
- **完善历史记录（永久保存）**：每次发信（成功 / 失败 / 被禁止）均写入本地 `history.sml`，
  历史页展示全部记录（上限 1000 条），可用「清空历史」（带二次确认）永久删除
- **草稿保存 / 恢复**：发信页可「存为草稿」，内容写入 `draft.sml`，**下次启动自动恢复**；
  可在设置中选择发送成功后是否保留表单内容（默认清空）
- **大附件上传进度**：发送时状态栏实时显示 `3.2 MB / 10.0 MB（32%）`，
  并带每秒刷新；请求带超时（连接 15s，总超时按附件大小缩放至多 300s），不会卡死
- **版本检查**：启动时静默比对远端 VersionFile（SML 格式），有更新则在状态栏提示
- **配置 / 草稿 / 历史统一用 SML 落盘**，并**应用 SML 契约**在读取时校验字段类型、
  补齐缺失默认值；配置被手改坏时会显式告警而非静默丢失

## 目录结构

```
src/
  main.rs       # Slint 宿主：注册 Rhai 原语、编译脚本、UI 回调、CLI 模式分发
  rhai_ext.rs   # Rhai 引擎 + 原语模块（http / crypto / store / ui / trust / api / markdown / sml）
  crypto.rs     # libsmx 国密 SM4-GCM + SM3 KDF 加解密
  resend.rs     # Resend API 请求/响应结构（SendRequest / SendResponse）与字段约束
  config.rs     # 配置持久化（SML + 契约校验）、套餐定义、日期算法
  draft.rs      # 草稿保存 / 恢复 / 清空（SML）
  history.rs    # 发信历史记录持久化（SML）
  log.rs        # 加密运行日志（本地随机密钥 + SM4-GCM 密文落盘）
  markdown.rs   # Markdown → 邮件友好 HTML（comrak 解析 + css-inline 内联）
  sml_store.rs  # 通用 SML 持久化层（原子写、JSON 迁移、契约）
  i18n.rs       # 消息目录（swi18n），自动识别系统语言
  update.rs     # 远端 VersionFile（SML）解析与版本比对
ui.slint        # 主窗口：布局装配 + 回调接线
ui/
  theme.slint       # 主题单例 Theme
  widgets.slint     # 通用控件（Card/PrimaryButton/FieldBox/NavItem/...）
  chrome.slint      # 自定义顶栏 TitleBar + 侧栏 NavRail
  fonts.slint       # 由 build.rs 生成：内嵌 MiSans VF 字体（无字体文件时为空壳）
  tabs/
    compose.slint     # 发信页
    config.slint      # 设置页
    history.slint     # 历史页
    script.slint      # 脚本页（含运行日志）
    info.slint        # 关于页（含 Slint 标识与许可信息）
    automation.slint  # 自动化页
scripts/
  default.rhai   # 默认业务脚本：身份获取 / 禁止判定 / 发信 / 统计 / 自动化
  default.rhai.sm3 # default.rhai 的 SM3 摘要（完整性校验用）
  pkg_size.py    # 打包体积辅助脚本
build.rs        # slint-build 编译 ui.slint；字体存在时生成 ui/fonts.slint 内嵌 MiSans VF
tools/
  fetch_license.py      # 由权威副本生成 LICENSE（带完整性校验）
  add_license_header.py # 批量维护源文件 SPDX 许可头（幂等）
  verify_version.py     # 校验版本号单一来源为 Cargo.toml
  verify_encrypted_logs.py # 校验加密运行日志可正常加解密
  verify_font.py        # 校验内置字体文件完整性与 SPDX 头
  font_info.py          # 打印内置字体元数据
  genlogo.py            # 生成 logo
  genico.py             # 生成应用图标（.ico）
  check_ico.py          # 校验生成的图标
  disk_report.py        # 磁盘占用 / 打包体积报告
  rehash_builtin.py     # 重新计算内置资源（脚本）的哈希
LICENCE-MulanPublV2     # 木兰许可证 v2 权威正文副本（LICENSE 的生成来源）
LICENSE                 # 版权声明 + 许可证全文 + 第三方组件许可
```

## Rhai 原语（Rust 注册，供脚本调用）

| 模块 | 函数 | 说明 |
|------|------|------|
| `http` | `post_json(url, bearer, map)` | 发送 JSON POST，返回 `[status, body]` |
| `http` | `get(url, bearer)` | GET 请求，返回 `[status, body]` |
| `crypto` | `encrypt(plain, pw)` | SM4-GCM 加密，返回 `ct\|nonce\|salt` |
| `crypto` | `decrypt(payload, pw)` | 解密，失败以 `ERR:` 开头 |
| `crypto` | `sm3_hex(text)` | SM3 摘要（hex） |
| `store` | `get_config(key)` / `set_config(key,val)` | 配置读写 |
| `store` | `bump_count(key)` | 计数自增（month_count/total_count） |
| `store` | `get_history(limit)` / `add_history(map)` | 历史读写 |
| `store` | `clear_history()` / `history_count()` | 清空历史 / 历史总条数 |
| `ui` | `set_status(text, is_err)` | 状态栏反馈 |
| `ui` | `log(text)` / `confirm(prompt)` | 日志 / 确认（日志同时进入 GUI「运行日志」面板） |
| `api` | `register(name, Fn("fn"), desc)` | 把脚本函数注册为命名自动化 handler |
| `api` | `list()` / `count()` | 列出（JSON）/ 统计已注册 handler |
| `api` | `call(name, args)` | 脚本内部调用另一个 handler |
| — | `SORG_BANNER` / `SNOWARE` / `SORG` | 组织标识常量 |

可在 `scripts/default.rhai` 中自由改写业务，或通过「脚本」页「重新加载脚本」即时生效。


## 构建与运行

```bash
cargo run --release
# 或仅构建
cargo build --release
```

> 桌面端依赖系统 WebView（Windows 自带 WebView2）。首次 `cargo build` 会下载并编译 Slint / libsmx 等 crate，耗时较长。

## 使用步骤

1. 打开 **设置** 页：
   - 填入 Resend API Key（`re_` 开头）与**固定发信名称**
   - （可选）设一个**加密密码**，勾选要加密的分类 → 点「保存设置」
   - 选择 **Resend 套餐**（或填自定义月度额度），设置周期起点（默认当月 1 号）
2. 回到 **发信** 页：
   - 填写收件人、主题、正文
   - 正文有三种模式，用编辑区右上角的标签切换：
     - **Markdown**（默认）：支持表格、删除线、任务列表、自动链接，
       发送时自动转为带内联样式的 HTML
     - **HTML**：原样发送，样式需自行内联
     - **纯文本**：以 `text` 字段发送
   - 正文编辑区下方的**分隔条可上下拖拽**，调整正文区高度（160px ~ 900px）
   - 点「浏览器预览」可用默认浏览器查看收件方看到的排版效果
   - 底部显示本期 / 累计已发与套餐剩余
   - 点「发送邮件」，底部状态栏显示结果（含返回的邮件 ID）
3. 若 API Key / 发信名称已加密：需先在设置页输入**同一加密密码**并保存，发送时程序会用该密码解密。

## 自动化（用 Rhai 构建 API）

脚本可把任意函数注册为**命名 handler**，之后由 GUI 或命令行触发，实现自动发信、批量任务、定时任务等，
无需改动或重新编译 Rust 代码。

在 `scripts/default.rhai` 中注册：

```rhai
fn my_task(args) {          // 约定：handler 统一接收一个数组参数 args
    let to = args[0];
    send_mail(to, "主题", "正文", false, "", []);
    return "ok";
}

api::register("my.task", Fn("my_task"), "我的自动化任务");
```

内置 3 个示例：`demo.ping`（回显测试）、`demo.send_one`（单封发信）、`demo.bulk`（批量发信）。

触发方式：

- **GUI**：左侧导航「自动化」页列出全部 handler，点「运行」并查看返回值
- **命令行**：`resender run <handler名称> [参数...]`，适合 cron / 计划任务

## 命令行

图形界面之外，同一二进制提供无 GUI 的命令行模式：

```bash
# 发信
resender send --to a@b.c --subject "主题" --body "正文" \
              [--html] [--from F] [--api-key K] [--password P] [--attach FILE]...

# 运行脚本注册的自动化 handler
resender run <handler名称> [参数...]

# 检查更新（需在设置中配置 update_url）
resender check-update

# 打印版本（与 GUI 关于页同源，均取自 Cargo.toml）
resender version

# 帮助
resender help
```

### 无黑框与命令行共存（Windows）

同一二进制，两种形态：

- **GUI**：`release` 构建为 Windows GUI 子系统——**双击启动不弹控制台黑框**
  （`debug` 构建保留控制台，便于查看日志）
- **命令行**：在 cmd / PowerShell 里直接运行 `resender send/run/version/help`，
  输出正常显示

实现：`windows_subsystem = "windows"` 使 release 无黑框；CLI 模式下若进程
没有有效 stdout（无控制台），通过 `AttachConsole(ATTACH_PARENT_PROCESS)`
附加到启动它的父终端并把 stdout/stderr 重定向到 `CONOUT$`。

只修复**无效**的句柄：若 stdout 已被重定向到文件/管道（`> file` / `|`），
保持原样，绝不覆盖，否则重定向的输出会凭空丢失。

跨平台：`AttachConsole` 是 Windows 专属，macOS / Linux 原生有控制台，
该逻辑为空实现，不影响。

## Markdown 正文

发信页默认用 **Markdown** 书写正文，发送时自动转为邮件友好的 HTML。

### 为什么必须做样式内联

不能直接用 `markdown_to_html` 的结果发信——主流邮件客户端对 CSS 支持极差：

- **Outlook** 用 Word 引擎渲染 HTML，不支持 flex / grid / 多数现代选择器
- **Gmail** 会直接剥离 `<style>` 标签

因此转换链是三段式（`src/markdown.rs`）：

```
Markdown --comrak(GFM)--> HTML --套用邮件安全样式表--> HTML --css-inline--> 样式内联的 HTML
```

样式表刻意只用邮件客户端普遍支持的属性（块级盒模型、系统字体栈），
配色沿用 SOrg 品牌色（青 `#22d3ee` → 蓝 `#3b82f6`）。

### 支持的语法

标准 CommonMark，外加 GFM 扩展：

| 语法 | 示例 |
|------|------|
| 表格 | `\| A \| B \|` |
| 删除线 | `~~text~~` |
| 任务列表 | `- [x] 已完成` |
| 自动链接 | `https://...` 自动识别为 `<a>` |

原始 HTML 中的 `<script>` 等标签会被转义，避免邮件注入。

### 在脚本中使用

转换能力也是 Rhai 原语，可用于自定义模板：

```rhai
// 完整 HTML 文档（可直接发给 Resend 的 html 字段）
let html = markdown::to_html("# 标题\n\n正文内容");

// 仅正文片段（不包装 <html>），便于嵌入自定义邮件模板
let frag = markdown::to_fragment("**加粗** 的内容");

send_mail(to, subject, html, true, password, []);
```

`markdown` 是纯本地计算，不触及网络与存储，因此**不受信任门控限制**。

> 版本号**单一来源**为 `Cargo.toml` 的 `version` 字段：GUI 关于页与 `--version` 都由
> 编译期常量 `APP_VERSION` 提供，发版后自动同步，无需手改两处。
> 可用 `python tools/verify_version.py` 校验这条链路。

## 加密说明

- 密钥派生：密码 + 每类随机盐，经 SM3 迭代 1000 次派生 16 字节 SM4 密钥
- 加密：`sm4_encrypt_gcm_combined`，输出 `ciphertext‖tag`，连同 `nonce`、`salt` 以
  `base64(ct)|base64(nonce)|base64(salt)` 形式存储
- 解密：同一密码 + 存储的盐/nonce 还原明文
- 密码不写入磁盘，仅密文落盘

## 配置文件位置

配置、草稿、历史均以 **SML** 落盘（旧版 `.json` 会在首次读取时自动迁移并删除）：

| 平台 | 目录 | 文件 |
|---|---|---|
| Windows | `%APPDATA%\resender\` | `config.sml`、`draft.sml`、`history.sml` |
| macOS | `~/Library/Application Support/resender/` | 同上 |
| Linux | `~/.config/resender/` | 同上 |

另有 `logs.enc`（SM4-GCM 加密的运行日志）与 `logkey.bin`（本机日志密钥）。

`config.sml` 内含**契约声明**，读取时校验字段类型并补齐缺失字段的默认值：

```sml
@contract ResenderConfig loose {
    api_key: str default ""
    plan_index: int min 0 default 0
    keep_after_send: bool default false
    ...
}
@is ResenderConfig
api_key: re_xxx
```

契约选用 `loose`（允许未声明字段）是刻意的：将来新增配置项后，旧配置文件
不会因含未知字段而被拒绝。

## 依赖

- `slint` 1.x（桌面）、`slint-build`
- `libsmx` 0.3（国密 SM2/SM3/SM4，本项目用 SM3 派生 + SM4-GCM）
- `reqwest`（rustls-tls，无系统 OpenSSL 依赖）
- `serde` / `serde_json` / `dirs` / `rand` / `base64`
- `rhai` 1.x（嵌入式脚本引擎）
- `rfd` 0.15（原生文件选择对话框，用于添加附件）
- `comrak` 0.54（Markdown 解析，GFM 完整支持；已关闭默认 feature 以跳过 CLI 与语法高亮依赖）
- `css-inline` 0.21（把 CSS 内联进 `style` 属性，邮件客户端兼容必需）

## 许可

Resender 本体采用 **MulanPubL V2**（Mulan Public License, Version 2，SPDX: `MulanPubL-2.0`），
完整中英双语条款见 [LICENSE](LICENSE)。

许可证正文由 `tools/fetch_license.py` 生成：优先读取仓库内的权威副本
`LICENCE-MulanPublV2`，缺失时才回退到官方站点（COSCL / SPDX），并在写入前做
**8 项完整性校验**（中英标题、条款起止、第 8 条语言条款、官方链接等），
校验不通过则拒绝写入，避免生成残缺或与官方不一致的法律文本。

```bash
python tools/fetch_license.py --check   # 只校验来源完整性
python tools/fetch_license.py           # 生成 LICENSE
```

各源文件头部均带 SPDX 声明（符合 Mulan PubL v2 附录建议），由
`tools/add_license_header.py` 批量维护（幂等）：

```bash
python tools/add_license_header.py --check   # 检查哪些文件缺少声明
python tools/add_license_header.py           # 为缺失文件补上声明
```

第三方组件按各自许可分发，其中：

| 组件 | 许可 |
|------|------|
| Slint（GUI 框架） | `GPL-3.0-only` OR `LicenseRef-Slint-Royalty-free-2.0`（本项目选用） OR `LicenseRef-Slint-Software-3.0` |
| MiSans VF（界面字体） | 《MiSans字体知识产权许可协议》（小米科技有限责任公司，保留所有权利），见 [ui/font-license.txt](ui/font-license.txt) |
| comrak（Markdown 解析） | `BSD-2-Clause` |
| css-inline（CSS 内联） | `MIT` |

> MiSans 许可要求「应在软件中特别注明使用了 MiSans 字体」——该声明显示在应用「关于」页，
> 并在技术栈中列出。
| 其余 Rust crate | 各自许可（多为 MIT / Apache-2.0），可用 `cargo tree` 查看 |

Slint 官方标识（"Made with Slint"）及其许可说明显示在应用的「关于」页。

---

# English

## Overview

**Resender** is a cross-platform desktop app (built with [Slint](https://slint.dev/))
that sends email through the [Resend](https://resend.com) API.

**License**: `MulanPubL-2.0`. Additional restriction: without the author's written
permission, citizens or organizations of EU / NATO member states may not use this
software for **commercial** purposes (see the end of [LICENSE](LICENSE)).
The Slint GUI framework uses `LicenseRef-Slint-Royalty-free-2.0`.

Key traits: **all business logic is driven by Rhai scripts**,
**Markdown bodies are converted to email-friendly HTML**,
**fixed From name**, **send counters + plan quota**,
**per-category password encryption (libsmx, SM4-GCM)**,
**encrypted run logs**, **detailed history**.

> **Everything is wired through Rhai**: Rust only registers safe primitives
> (HTTP / crypto / store / UI feedback); identity resolution, send gating,
> sending and accounting are assembled dynamically in `scripts/default.rhai`,
> so behaviour can change without recompiling.

## Features

- Send text / HTML email via the Resend REST API; multiple recipients
  (comma / semicolon / newline / space separated)
- **Rhai-driven**: identity resolution, send gating, sending and counting
- **Fixed From name** configured once in Settings
- **Send counters**: total and current-period (cycle start, default 1st of month)
- **Plan quota**: Free / Pro / Scale or a custom monthly limit;
  remaining = quota − current-period count, turns red when exhausted
- **Bundled font**: Xiaomi **MiSans VF** (shipped with the binary)
- **Encrypted run logs**: `ui::log(...)` output is stored as SM4-GCM ciphertext
  using a machine-local random key; viewable decrypted on the Script page
- **Per-category encryption** with libsmx SM4-GCM (API key and From name
  can be encrypted separately, sharing one password; the password is never stored)
- **Persistent history**: every attempt (success / failure / blocked) is written
  to `history.sml` (cap 1000), clearable with confirmation
- **Drafts**: save the compose form to `draft.sml` and restore it on next launch;
  Settings controls whether the form is kept after a successful send
- **Large attachment progress**: the status bar shows `3.2 MB / 10.0 MB (32%)`
  with per-second refresh; requests are time-bound (connect 15s, total scaled with
  attachment size up to 300s) so the UI can never hang
- **Update check**: on startup, a remote VersionFile (SML) is compared silently
- **SML persistence with contracts**: config / draft / history are stored as SML
  and validated against a contract on read (types checked, missing defaults filled);
  a corrupted config triggers an explicit warning instead of silent loss

## Build and run

```bash
cargo run --release
# or just build
cargo build --release
```

## Quick start

1. Open **Settings** and fill in the Resend API key (`re_...`) and the **From name**.
   Optionally set an **encryption password** and pick which fields to encrypt.
   Choose the **Resend plan** (or a custom monthly quota) and the cycle start.
2. Back on **Compose**: fill recipients, subject and body. The body has three modes
   (switch via the tabs at the top-right of the editor):
   - **Markdown** (default): tables, strikethrough, task lists, autolinks —
     converted to inline-styled HTML on send
   - **HTML**: sent as-is (you must inline styles yourself)
   - **Plain text**: sent in the `text` field
   Use **Browser preview** to see what the recipient will see.
3. If the API key / From name are encrypted, enter the **same password** in
   Settings and save first; it is used to decrypt at send time.

## Command line

```bash
resender send --to a@b.c --subject "Subject" --body "Body" \
              [--html] [--from F] [--api-key K] [--password P] [--attach FILE]...
resender run <handler> [args...]     # run a script-registered automation handler
resender check-update                # check for updates (needs update_url in Settings)
resender version | --version | -V
resender help
```

On Windows the release build uses the GUI subsystem (no console window on
double-click), while CLI output still works in cmd / PowerShell.

## Automation

Scripts can register any function as a **named handler**, triggered from the GUI
or the command line — no Rust changes or recompilation:

```rhai
fn my_task(args) {
    let to = args[0];
    send_mail(to, "Subject", "Body", false, "", []);
    return "ok";
}
api::register("my.task", Fn("my_task"), "My automated task");
```

Three demos ship with the app: `demo.ping`, `demo.send_one`, `demo.bulk`.

## Config file locations

Config, drafts and history are stored as **SML** (legacy `.json` files are
migrated and removed on first read):

| Platform | Directory | Files |
|---|---|---|
| Windows | `%APPDATA%\resender\` | `config.sml`, `draft.sml`, `history.sml` |
| macOS | `~/Library/Application Support/resender/` | same |
| Linux | `~/.config/resender/` | same |

## Dependencies

`slint` 1.x · `libsmx` 0.3 (SM3 KDF + SM4-GCM) · `reqwest` (rustls-tls, no system
OpenSSL) · `serde` / `serde_json` / `dirs` / `rand` / `base64` · `rhai` 1.x ·
`rfd` 0.15 · `comrak` 0.54 (Markdown, GFM) · `css-inline` 0.21 ·
`swi18n` 0.1 · `swsml` 0.1 (SML)

## License

MulanPubL-2.0 (full bilingual text in [LICENSE](LICENSE)). Third-party components
are distributed under their own licenses — notably Slint
(`LicenseRef-Slint-Royalty-free-2.0`) and MiSans VF
(see [ui/font-license.txt](ui/font-license.txt)).