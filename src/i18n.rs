// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//! 界面文案的本地化目录。
//!
//! 机制（swi18n）：系统语言自动识别 + 消息目录 + 回退链。
//! 本文件是 **Resender 的内容**（key × 语言）；swi18n 是**机制**（可复用于其他项目）。
//!
//! 优先级：脚本 `setup_i18n()` 定义 > 本目录（按系统语言）> Slint 缺省值。
//! 即：本目录填充系统语言的默认文案，脚本仍可覆盖任意 key。

use swi18n::Catalog;

/// 构建 Resender 的消息目录（中英双语）。
/// 新增文案时：key 必须同时有 zh-CN 与 en，否则非中文系统回退到 en 或 key 本身。
pub fn catalog() -> Catalog {
    let mut c = Catalog::new();
    // 导航
    c.set_many("t_send", &[("zh-CN", "发信"), ("en", "Compose")]);
    c.set_many("t_settings", &[("zh-CN", "设置"), ("en", "Settings")]);
    c.set_many("t_history", &[("zh-CN", "历史"), ("en", "History")]);
    c.set_many("t_script", &[("zh-CN", "脚本"), ("en", "Script")]);
    c.set_many("t_about", &[("zh-CN", "关于"), ("en", "About")]);
    // 表单
    c.set_many("t_to", &[("zh-CN", "收件人"), ("en", "To")]);
    c.set_many("t_subject", &[("zh-CN", "主题"), ("en", "Subject")]);
    c.set_many("t_body", &[("zh-CN", "正文"), ("en", "Body")]);
    c.set_many("t_from", &[("zh-CN", "发信名称（From）"), ("en", "From name")]);
    c.set_many("t_send_btn", &[("zh-CN", "发送"), ("en", "Send")]);
    c.set_many("t_attachments", &[("zh-CN", "附件"), ("en", "Attachments")]);
    c.set_many("t_add_attachment", &[("zh-CN", "添加附件"), ("en", "Add attachment")]);
    c.set_many("t_remove", &[("zh-CN", "移除"), ("en", "Remove")]);
    // 设置
    c.set_many("t_api_key", &[("zh-CN", "Resend API Key"), ("en", "Resend API Key")]);
    c.set_many("t_quota", &[("zh-CN", "套餐月度额度"), ("en", "Monthly plan quota")]);
    c.set_many("t_remaining", &[("zh-CN", "本期剩余"), ("en", "Remaining this period")]);
    c.set_many("t_save", &[("zh-CN", "保存设置"), ("en", "Save settings")]);
    c.set_many("t_unlock", &[("zh-CN", "解锁"), ("en", "Unlock")]);
    c
}

/// 把系统语言对应的文案填充进 `ctx.i18n`（脚本 setup_i18n 执行前调用，
/// 脚本定义会覆盖本目录）。
///
/// 返回检测到的语言标签（脚本可通过 `ui_lang` 变量感知当前语言）。
pub fn fill(ctx: &crate::rhai_ext::RhaiContext) -> String {
    let lang = swi18n::detect_language();
    let cat = catalog();
    let mut map = ctx.i18n.lock().unwrap();
    for key in cat.keys() {
        let text = cat.t(&key, &lang);
        map.insert(key, text);
    }
    lang
}
