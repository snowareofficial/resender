// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

// ===========================================================================
//  Resender 默认脚本 — 所有业务能力在此动态拼装
//  组织标识（仅展示 / 可用变量，不绑定具体协议）：
//    <<*>> SOrg :: -^v- SNOWARE
//    Copyright (C) 2026~now S.A. Licensed under Mulan PubL v2.
//
//  Rust 注册的原语（详见 src/rhai_ext.rs）：
//    http::post_json(url, bearer, map) -> [status, body]
//    http::get(url, bearer)            -> [status, body]
//    crypto::encrypt(plain, pw)        -> "ct|nonce|salt"
//    crypto::decrypt(payload, pw)      -> plain  (失败以 "ERR:" 开头)
//    crypto::sm3_hex(text)             -> hex
//    store::get_config(key)            -> string
//    store::set_config(key, value)     -> "ok" | "ERR:..."
//    store::bump_count(key)            -> new_count_string
//    store::get_history(limit)         -> json_array_string
//    store::add_history(entry_json)    -> "ok" | "ERR:..."
//    markdown::to_html(md)             -> 邮件友好 HTML（样式已内联，可直接发信）
//    markdown::to_fragment(md)         -> 仅正文片段的 HTML（不包装 <html>）
//    ui::set_status(text, is_err)
//    ui::log(text)                     -> text
//    ui::confirm(prompt)               -> bool
//    常量：SORG_BANNER, SNOWARE, SORG
//
//  注：markdown 为纯本地计算，不受信任门控影响。
//      发信页选择 Markdown 模式时，Rust 侧已在发送前自动转 HTML；
//      脚本里主动调用 markdown::to_html 可用于自定义模板后再发送。
//
//  信任机制原语（未解锁时 http/crypto/store 全部返回禁用错误）：
//    trust::is_trusted()               -> bool
//    trust::unlock(pwd, src, sig_hex)  -> ""(成功) | "ERR:..."
//    trust::lock()
//    trust::get_sig_mode()             -> "off"|"sm2"|"pq"
//    trust::sign_script(src, priv_hex) -> sig_hex（离线签发用）
// ===========================================================================

// RESEND_API 等常量由 Rust 侧注入（见 src/rhai_ext.rs build_engine 的 const_mod）

// ===========================================================================
//  国际化（i18n）— 自动识别系统语言（swi18n）
//
//  默认行为：Rust 侧按系统语言提供中/英文案（src/i18n.rs 目录），
//  本函数**不需要也不应**重复设置——留空即用系统语言。
//
//  自定义覆盖：如需强制某种语言或增改文案，在此用 ui::set_i18n 覆盖即可
//  （脚本定义优先级高于 Rust 默认目录）：
//      ui::set_i18n("t_send", "发送邮件");
//
//  当前识别到的语言可用 ui::get_lang() 读取（BCP-47，如 zh-CN / en）。
// ===========================================================================
fn setup_i18n() {
    // 默认：跟随系统语言，无需覆盖
}

// ===========================================================================
//  主题系统 — 亮/暗双套配色，支持脚本自定义任意颜色与半透明
//  SOrg 标识：左侧青色 + 右侧蓝色（按需求固定）
// ===========================================================================
fn apply_theme(dark) {
    if dark {
        ui::set_theme("theme_bg", "#1b1d23");
        ui::set_theme("theme_surface", "#23252e");
        ui::set_theme("theme_fg", "#e8eaed");
        ui::set_theme("theme_muted", "#9aa0ab");
        ui::set_theme("theme_accent", "#5b8def");
        ui::set_theme("theme_accent_soft", "#2a3350");
        ui::set_theme("theme_on_accent", "#ffffff");
        ui::set_theme("theme_border", "#33363f");
        ui::set_theme("theme_field_bg", "#2b2f38");
        ui::set_theme("theme_field_fg", "#e8eaed");
        ui::set_theme("theme_field_border", "#3a3f4b");
    } else {
        ui::set_theme("theme_bg", "#f6f7f9");
        ui::set_theme("theme_surface", "#ffffff");
        ui::set_theme("theme_fg", "#2b2f38");
        ui::set_theme("theme_muted", "#8a909c");
        ui::set_theme("theme_accent", "#3b5bdb");
        ui::set_theme("theme_accent_soft", "#eef2ff");
        ui::set_theme("theme_on_accent", "#ffffff");
        ui::set_theme("theme_border", "#e6e8ec");
        ui::set_theme("theme_field_bg", "#eef1f5");
        ui::set_theme("theme_field_fg", "#2b2f38");
        ui::set_theme("theme_field_border", "#c2c8d2");
    }
    // SOrg 标识固定配色：左侧青色、右侧蓝色
    ui::set_theme("theme_sorg_left", "#06b6d4");
    ui::set_theme("theme_sorg_right", "#3b82f6");
    // 半透明部件（0.1~1.0），可改为 <1.0 获得毛玻璃观感
    ui::set_opacity(1.0);
    ui::set_dark(dark);
}

// 在脚本加载时初始化 i18n 与默认（亮色）主题
setup_i18n();
apply_theme(false);

// ===========================================================================
//  自动化 / 可编程 API 示例
//  脚本可把任意函数注册为命名 handler，供 UI「自动化」面板或命令行
//  `resender run <name> [args...]` 触发，实现自动发信、批量任务等。
// ===========================================================================

// 示例 1：回显测试（无需 API Key）
// 约定：handler 以单个数组参数 args 接收调用参数（GUI/CLI 传入的数组）
fn demo_ping(args) {
    let who = if args.len() > 0 { args[0] } else { "world" };
    let msg = "pong from Rhai: hello " + who;
    ui::log(msg);
    return msg;
}

// 示例 2：用脚本拼接并发送一封邮件（需已配置 API Key）
fn demo_send_one(args) {
    let to = if args.len() > 0 { args[0] } else { "" };
    let subject = if args.len() > 1 { args[1] } else { "来自自动化脚本" };
    let body = if args.len() > 2 { args[2] } else { "这封邮件由 Rhai 自动化 handler 发出。" };
    if to == "" {
        ui::set_status("demo.send_one: 缺少收件人参数", true);
        return "ERR: 缺少收件人";
    }
    send_mail(to, subject, body, false, "", []);
    return "已提交发送: " + to;
}

// 示例 3：批量发送（从逗号分隔的收件人列表）
fn demo_bulk(args) {
    let list = if args.len() > 0 { args[0] } else { "" };
    if list == "" { return "ERR: 缺少收件人列表"; }
    let parts = list.split(",");
    let n = 0;
    for p in parts {
        let t = trim(p);
        if t != "" {
            send_mail(t, "批量通知", "这是批量自动化发送的一封邮件。", false, "", []);
            n = n + 1;
        }
    }
    return "已提交 " + n + " 封批量邮件";
}

// 注册到自动化注册表（UI 面板与 CLI 均可触发）
api::register("demo.ping", Fn("demo_ping"), "回显测试，无需 API Key");

// 带输入组件的 handler：这些组件会渲染在「自动化」页，填写后点运行
api::register_with_fields(
    "demo.send_one",
    Fn("demo_send_one"),
    "发送单封邮件（填写收件人 / 主题 / 正文）",
    [
        #{ key: "to", label: "收件人", kind: "text" },
        #{ key: "subject", label: "主题", kind: "text" },
        #{ key: "body", label: "正文", kind: "multiline" },
    ]
);

api::register_with_fields(
    "demo.bulk",
    Fn("demo_bulk"),
    "批量发送（填写逗号分隔的收件人列表）",
    [
        #{ key: "list", label: "收件人列表", kind: "text" },
    ]
);

// ---------------------------------------------------------------------------
//  身份获取：组织内授权，本地不知密码
//  - 默认实现：从 store 读取已保存的 api_key（可能已加密）
//  - 自定义：可替换为向 SOrg/SNOWARE 网关请求临时 token（用 http::get）
// ---------------------------------------------------------------------------
fn get_identity(crypto_password) {
    let enc = store::get_config("api_key_enc");
    let raw = store::get_config("api_key");
    if enc == "true" {
        if crypto_password == "" {
            ui::set_status("API Key 已加密，请提供密码", true);
            return "";
        }
        let dec = crypto::decrypt(raw, crypto_password);
        if dec.starts_with("ERR:") {
            ui::set_status("API Key 解密失败：" + dec, true);
            return "";
        }
        return dec;
    }
    return raw;
}

// 获取发信名称（From）
fn get_from_name(crypto_password) {
    let enc = store::get_config("from_name_enc");
    let raw = store::get_config("from_name");
    if enc == "true" {
        if crypto_password == "" {
            return "";
        }
        let dec = crypto::decrypt(raw, crypto_password);
        if dec.starts_with("ERR:") { return ""; }
        return dec;
    }
    return raw;
}

// ---------------------------------------------------------------------------
//  身份禁止判定：返回 "" 表示允许，否则返回禁止原因
//  - 默认：检查套餐余量；自定义：可接组织黑名单 / 合规网关
// ---------------------------------------------------------------------------
fn check_forbidden(to_list) {
    let used = store::get_config("month_count");
    let quota = store::get_config("plan_quota");  // 注：脚本内需用 compute
    // 简化：直接读 plan_index + custom
    let pidx = to_int(trim(store::get_config("plan_index")));
    let custom = to_int(trim(store::get_config("custom_quota")));
    let quota_val = if pidx >= 3 { custom } else {
        [3000, 50000, 100000, 0][pidx]
    };
    let used_val = to_int(trim(used));
    if quota_val > 0 && used_val >= quota_val {
        return "套餐额度已用尽（" + used_val + " / " + quota_val + "）";
    }
    return "";
}

// ---------------------------------------------------------------------------
//  发信主流程（被 Rust 调用）
//  参数：to_text, subject, body, use_html, crypto_password
// ---------------------------------------------------------------------------
fn send_mail(to_text, subject, body, use_html, crypto_password, attachments) {
    ui::log(SORG_BANNER);

    // 解析收件人
    let to_list = [];
    let parts = to_text.split(",;");
    for p in parts {
        let t = p.trim();
        if t != "" { to_list.push(t); }
    }
    if to_list.len() == 0 {
        ui::set_status("请填写至少一个收件人", true);
        return;
    }

    // 禁止判定
    let forbidden = check_forbidden(to_list);
    if forbidden != "" {
        ui::set_status("发送被禁止：" + forbidden, true);
        store::add_history(#{
            ts: now_iso(),
            to: to_text,
            subject: subject,
            from: "",
            status: "fail",
            detail: forbidden
        });
        return;
    }

    // 身份
    let api_key = get_identity(crypto_password);
    if api_key == "" {
        return;  // 状态已由 get_identity 设置
    }
    let from = get_from_name(crypto_password);
    if from == "" {
        from = "Resender <onboarding@resend.dev>";
    }

    // 构造请求体
    let req = #{
        from: from,
        to: to_list,
        subject: subject
    };
    if use_html {
        req["html"] = body;
    } else {
        req["text"] = body;
    }
    // 附件：Resend 接受 REST 层 attachments=[{content(base64), filename}]
    if attachments.len() > 0 {
        let atts = [];
        for a in attachments {
            atts.push(#{ filename: a["filename"], content: a["content"] });
        }
        req["attachments"] = atts;
        ui::log("附件数量: " + atts.len());
    }

    ui::set_status("发送中…", false);
    let res = http::post_json(RESEND_API, api_key, req);
    let status = res[0];
    let resp_body = res[1];

    if status >= 200 && status < 300 {
        // 成功：更新计数 + 历史
        let mid = parse_id(resp_body);
        store::bump_count("month_count");
        store::bump_count("total_count");
        let msg = if mid != "" { "发送成功！邮件 ID: " + mid } else { "发送成功！" };
        ui::set_status(msg, false);
        store::add_history(#{
            ts: now_iso(),
            to: to_text,
            subject: subject,
            from: from,
            status: "ok",
            detail: mid
        });
    } else {
        let err = parse_message(resp_body);
        ui::set_status("发送失败 (" + status + "): " + err, true);
        store::add_history(#{
            ts: now_iso(),
            to: to_text,
            subject: subject,
            from: from,
            status: "fail",
            detail: err
        });
    }
    return;
}

// ---------------------------------------------------------------------------
//  辅助
// ---------------------------------------------------------------------------
fn now_iso() {
    // Rhai 无内置时间，调用 Rust 注入的 now_iso（main 注册）
    return rust_now_iso();
}

fn parse_id(body) {
    // body 为 JSON 字符串，尝试提取 id
    let v = parse_json(body);
    if v.type_of() == "map" {
        return v["id"].to_string();
    }
    return "";
}

fn parse_message(body) {
    let v = parse_json(body);
    if v.type_of() == "map" {
        return v["message"].to_string();
    }
    return body;
}
