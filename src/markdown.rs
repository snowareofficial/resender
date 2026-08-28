// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//! Markdown → 邮件友好 HTML
//!
//! 为什么不能直接用 `markdown_to_html` 的结果发信：
//! 主流邮件客户端对 CSS 支持极差——Outlook 用 Word 引擎渲染 HTML（不支持 flex/grid、
//! 部分选择器），Gmail 会直接剥离 `<style>` 标签。因此必须把样式**内联**到每个标签的
//! `style` 属性上，否则收件方看到的是没有任何排版的裸 HTML。
//!
//! 处理链：
//!   1. comrak 解析 Markdown（启用 GFM：表格 / 删除线 / 任务列表 / 自动链接）
//!   2. 套用邮件安全的样式表（SOrg 品牌配色：青 `#22d3ee` → 蓝 `#3b82f6`）
//!   3. css-inline 把 CSS 规则内联进 `style` 属性
//!
//! 样式表刻意只使用邮件客户端普遍支持的属性，不用 flex/grid/外部字体/伪元素。

use comrak::{markdown_to_html, Options};

/// 邮件安全样式表。
///
/// 约束：只用 table/块级盒模型、系统字体栈、绝对保守的配色；
/// 不用 flex / grid / position:fixed / 外部字体 —— 这些在 Outlook 里会失效。
const EMAIL_CSS: &str = r#"
body {
    margin: 0;
    padding: 0;
    background-color: #f6f7f9;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Microsoft YaHei",
                 "PingFang SC", "Hiragino Sans GB", sans-serif;
    font-size: 15px;
    line-height: 1.75;
    color: #2b2f38;
}
.container {
    max-width: 640px;
    margin: 0 auto;
    padding: 28px;
    background-color: #ffffff;
}
h1, h2, h3, h4 {
    color: #1f232b;
    line-height: 1.35;
    font-weight: 600;
}
h1 { font-size: 24px; margin: 26px 0 14px; }
h2 { font-size: 20px; margin: 22px 0 12px; }
h3 { font-size: 17px; margin: 18px 0 10px; }
h4 { font-size: 15px; margin: 16px 0 8px; }
p  { margin: 12px 0; }
a  { color: #3b82f6; text-decoration: underline; }
strong { font-weight: 600; color: #1f232b; }
hr {
    border: none;
    border-top: 1px solid #e6e8ec;
    margin: 22px 0;
}
ul, ol { padding-left: 26px; margin: 12px 0; }
li { margin: 5px 0; }
code {
    background-color: #eef1f5;
    color: #c7254e;
    padding: 2px 6px;
    border-radius: 5px;
    font-family: Consolas, Monaco, "Courier New", monospace;
    font-size: 13px;
}
pre {
    background-color: #eef1f5;
    padding: 14px 16px;
    border-radius: 8px;
    overflow-x: auto;
    margin: 14px 0;
}
pre code {
    background-color: transparent;
    color: #2b2f38;
    padding: 0;
    font-size: 13px;
}
blockquote {
    margin: 16px 0;
    padding: 10px 16px;
    border-left: 3px solid #22d3ee;
    background-color: #f4fbfd;
    color: #4a5160;
}
blockquote p { margin: 4px 0; }
table {
    border-collapse: collapse;
    width: 100%;
    margin: 16px 0;
    font-size: 14px;
}
th, td {
    border: 1px solid #dfe3ea;
    padding: 9px 12px;
    text-align: left;
}
th { background-color: #eef2ff; font-weight: 600; color: #1f232b; }
tr:nth-child(even) td { background-color: #fafbfc; }
img { max-width: 100%; height: auto; border-radius: 6px; }
.muted { color: #8a909c; font-size: 13px; }
.signature {
    margin-top: 28px;
    padding-top: 16px;
    border-top: 1px solid #e6e8ec;
    color: #8a909c;
    font-size: 12px;
}
"#;

/// 启用 GFM 常用扩展的解析选项
/// （comrak 0.54 的 Options 带生命周期参数，此处不使用借用字段，固定为 'static）
fn gfm_options() -> Options<'static> {
    let mut options = Options::default();
    // 表格：邮件正文常用（报价单、统计表等）
    options.extension.table = true;
    // 删除线 ~~text~~
    options.extension.strikethrough = true;
    // 任务列表 - [x]
    options.extension.tasklist = true;
    // 裸链接自动识别为 <a>
    options.extension.autolink = true;
    options
}

/// Markdown → 可直接用于发信的完整 HTML 文档（样式已内联）。
///
/// 内联失败时降级为「带 `<style>` 标签」的文档——桌面邮件客户端大多仍可正常显示，
/// 不会返回空内容。
pub fn to_html(md: &str) -> String {
    let body = markdown_to_html(md, &gfm_options());
    let doc = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>
{EMAIL_CSS}
</style>
</head>
<body>
<div class="container">
{body}
</div>
</body>
</html>
"#
    );
    inline_css(&doc)
}

/// Markdown → 仅正文片段的 HTML（不包装 `<html>`，样式已内联）。
/// 适合脚本自行拼装邮件模板。
pub fn to_fragment(md: &str) -> String {
    let body = markdown_to_html(md, &gfm_options());
    let doc = format!("<style>\n{EMAIL_CSS}\n</style>\n<div class=\"container\">\n{body}\n</div>");
    inline_css(&doc)
}

/// 把 `<style>` 规则内联进各标签的 `style` 属性。
///
/// 失败时原样返回输入，保证调用方永远拿到可用的 HTML。
fn inline_css(doc: &str) -> String {
    match css_inline::CSSInliner::options().build().inline(doc) {
        Ok(inlined) => inlined,
        Err(_) => doc.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_and_emphasis_render() {
        let html = to_html("# t\n\n**b** and `code`");
        // 注意：样式被内联后标签形如 `<strong style="...">`，
        // 因此断言只匹配标签名，不带闭角括号
        assert!(html.contains("<h1"), "should render h1, got: {html}");
        assert!(html.contains("<strong"), "should render bold, got: {html}");
        assert!(html.contains("<code"), "should render inline code, got: {html}");
    }

    #[test]
    fn gfm_table_renders() {
        let html = to_html("| 项目 | 数量 |\n| --- | --- |\n| 邮件 | 12 |");
        assert!(html.contains("<table"), "GFM 表格应被渲染");
        assert!(html.contains("<th"), "表头应被渲染");
    }

    #[test]
    fn html_is_escaped_not_passthrough() {
        // 原始 HTML 中的脚本应被转义，避免邮件注入
        let html = to_html("<script>alert(1)</script>");
        assert!(
            !html.contains("<script>"),
            "脚本标签应被转义，不应原样保留"
        );
    }

    #[test]
    fn styles_are_inlined() {
        // css-inline 应把 .container 的规则内联到 style 属性
        let html = to_html("正文内容");
        assert!(
            html.contains("max-width") && html.contains("640px"),
            "容器样式应被内联到 style 属性，got: {html}"
        );
    }

    #[test]
    fn realistic_email_renders_all_blocks() {
        // 一封真实邮件的常见元素：标题 / 粗体 / 表格 / 任务列表 / 引用 / 代码 / 链接
        let md = r#"# 月度报表

你好，**张三**：

| 指标 | 数值 |
| --- | --- |
| 发送量 | 1234 |

- [x] 已完成
- [ ] 待处理

> 数据截至昨日。

详见 [控制台](https://example.com)。

```
cargo build --release
```
"#;
        let html = to_html(md);
        for tag in [
            "<h1",
            "<strong",
            "<table",
            "<th",
            "<blockquote",
            "<pre",
            "<a ",
            "<li",
        ] {
            assert!(html.contains(tag), "应渲染 {tag}，got: {html}");
        }
        // 邮件客户端会剥离 <style>，因此样式必须已内联到 style 属性
        assert!(html.contains("style="), "样式必须内联，got: {html}");
        // 任务列表的复选框
        assert!(html.contains("checkbox"), "任务列表应渲染复选框");
    }
}
