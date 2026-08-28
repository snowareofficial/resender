// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//![allow(non_snake_case)]
//! Rhai 脚本引擎封装：所有业务能力（身份获取、禁止判定、发信、统计）
//! 都经由 Rhai 脚本动态拼装。Rust 只注册「安全原语」，不写死任何协议。
//!
//! SOrg / SNOWARE 标识（仅作展示与脚本可用变量，不绑定具体组织）：
//!   <<*>> SOrg :: -^v- SNOWARE
//!   Copyright (C) 2026~now S.A. Licensed under Mulan PubL v2.

use anyhow::Result;
use rhai::plugin::*;
use rhai::{Engine, Scope, AST, FnPtr};
use serde_json::Value as JValue;
use std::sync::{Arc, Mutex, OnceLock};
use std::path::PathBuf;
use std::collections::HashMap;

use crate::history::HistoryStore;
use crate::log::LogStore;

pub const SORG_BANNER: &str = "<<*>> SOrg :: -^v- SNOWARE\nCopyright (C) 2026~now S.A. Licensed under Mulan PubL v2.";

/// 自动化 handler 的一个输入组件（由脚本在 api::register_with_fields 中声明）
#[derive(Clone, Debug)]
pub struct AutomationField {
    /// 字段标识（提交时按声明顺序传入 handler）
    pub key: String,
    /// 界面上的标签文字
    pub label: String,
    /// 控件类型：text（默认）/ password / bool / multiline
    pub kind: String,
}

/// 自动化 handler 注册表条目
#[derive(Clone)]
pub struct Automation {
    /// 函数指针（指向脚本中定义的函数）
    pub handler: FnPtr,
    /// 描述（用于 UI 展示）
    pub description: String,
    /// 输入组件声明（脚本可注册必要的输入控件）
    pub fields: Vec<AutomationField>,
}

/// 从 Rhai 数组解析字段声明：`[#{key:"to", label:"收件人", kind:"text"}, ...]`
fn parse_fields(arr: &rhai::Array) -> Vec<AutomationField> {
    let mut out = Vec::new();
    for item in arr {
        if let Some(map) = item.clone().try_cast::<rhai::Map>() {
            let get_str = |k: &str| -> String {
                map.get(k)
                    .and_then(|v| v.clone().try_cast::<rhai::ImmutableString>())
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            };
            let key = get_str("key");
            if key.is_empty() {
                continue;
            }
            let label = get_str("label");
            let kind = get_str("kind");
            let label = if label.is_empty() { key.clone() } else { label };
            out.push(AutomationField {
                key,
                label,
                kind: if kind.is_empty() { "text".to_string() } else { kind },
            });
        }
    }
    out
}

/// 引擎对外暴露的全局共享状态（供原语读写）
pub struct RhaiContext {
    /// 历史记录存储
    pub history: Arc<HistoryStore>,
    /// 主密码（用于加解密原语，由 UI 解锁时设置）
    pub crypto_password: Arc<Mutex<String>>,
    /// 最近一次状态文本（供 UI 读取）
    pub status: Arc<Mutex<(String, bool)>>,
    /// i18n 字段表（key -> 本地化文本）
    pub i18n: Arc<Mutex<std::collections::HashMap<String, String>>>,
    /// 主题色表（key -> 颜色十六进制，如 #rrggbb）
    pub theme: Arc<Mutex<std::collections::HashMap<String, String>>>,
    /// 是否为暗色模式
    pub dark_mode: Arc<Mutex<bool>>,
    /// 部件不透明度（0.0~1.0），实现半透明
    pub opacity: Arc<Mutex<f32>>,
    /// —— 脚本信任状态 ——
    /// 是否已通过信任校验（解锁 + 验签通过）。门控所有敏感原语。
    pub trusted: Arc<Mutex<bool>>,
    /// 信任解锁密码（内存态，由 UI 解锁时比对 config.script_trust_password）
    pub trust_password: Arc<Mutex<String>>,
    /// 签名校验模式（来自 config，缓存供运行期读取）
    pub sig_verify: Arc<Mutex<String>>,
    /// 引擎引用（供 api 模块调用已注册的 FnPtr）
    pub engine: OnceLock<Arc<Engine>>,
    /// 脚本 AST 引用（供 api 模块调用已注册的 FnPtr）
    pub ast: OnceLock<Arc<AST>>,
    /// 自动化 handler 注册表（脚本用 api::register 注册）
    pub automations: Arc<Mutex<HashMap<String, Automation>>>,
    /// 运行日志缓冲（ui::log 追加，GUI 日志面板读取显示）
    pub logs: Arc<Mutex<Vec<String>>>,
    /// 加密日志存储（落盘为密文，绝不写明文）
    pub log_store: Arc<LogStore>,
    /// 日志落盘是否使用本地自动生成的密钥加密（始终为 true，仅作状态展示）
    pub logs_encrypted: Arc<Mutex<bool>>,
    /// 自动化输入组件的当前值：键为 (handler 名, 字段 key)
    pub field_values: Arc<Mutex<HashMap<(String, String), String>>>,
}

impl RhaiContext {
    pub fn new(history: Arc<HistoryStore>) -> anyhow::Result<Self> {
        Ok(Self {
            history,
            log_store: Arc::new(LogStore::new()?),
            logs_encrypted: Arc::new(Mutex::new(false)),
            crypto_password: Arc::new(Mutex::new(String::new())),
            status: Arc::new(Mutex::new(("就绪".into(), false))),
            i18n: Arc::new(Mutex::new(std::collections::HashMap::new())),
            theme: Arc::new(Mutex::new(std::collections::HashMap::new())),
            dark_mode: Arc::new(Mutex::new(false)),
            opacity: Arc::new(Mutex::new(1.0)),
            trusted: Arc::new(Mutex::new(false)),
            trust_password: Arc::new(Mutex::new(String::new())),
            sig_verify: Arc::new(Mutex::new("off".to_string())),
            engine: OnceLock::new(),
            ast: OnceLock::new(),
            automations: Arc::new(Mutex::new(HashMap::new())),
            logs: Arc::new(Mutex::new(Vec::new())),
            field_values: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

// ---------------------------------------------------------------------------
// 原语：HTTP（仅封装 Resend 发信，组织身份接口由脚本自定义 URL 调用）
// ---------------------------------------------------------------------------

#[export_module]
pub mod http_primitives {
    /// 信任门控：未通过信任校验时，所有敏感原语返回禁用错误
    fn guard() -> Option<rhai::Array> {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        if !*ctx.trusted.lock().unwrap() {
            return Some(make_err("脚本未受信任：功能已禁用，请在「设置 → 脚本信任」中启用并解锁"));
        }
        None
    }

    /// 发送 HTTP POST（JSON），返回 (status_code, body_string)
    pub fn post_json(url: &str, bearer: &str, body: rhai::Map) -> rhai::Array {
        if let Some(e) = guard() { return e; }
        let rt = match tokio::runtime::Runtime::new() {
            Ok(r) => r,
            Err(e) => return make_err(&format!("runtime: {e}")),
        };
        rt.block_on(async move {
            let client = reqwest::Client::new();
            let mut req = client.post(url).header("Content-Type", "application/json");
            if !bearer.is_empty() {
                req = req.bearer_auth(bearer);
            }
            // body 是 rhai::Map，转 serde_json
            let mut b = body.clone();
            let json_val: JValue = map_to_json(&mut b);
            let resp = req.json(&json_val).send().await;
            match resp {
                Ok(r) => {
                    let status = r.status().as_u16() as i64;
                    let txt = r.text().await.unwrap_or_default();
                    rhai::Array::from([
                        rhai::Dynamic::from(status),
                        rhai::Dynamic::from(txt),
                    ])
                }
                Err(e) => make_err(&format!("request: {e}")),
            }
        })
    }

    /// GET 请求，返回 (status_code, body_string)
    pub fn get(url: &str, bearer: &str) -> rhai::Array {
        if let Some(e) = guard() { return e; }
        let rt = match tokio::runtime::Runtime::new() {
            Ok(r) => r,
            Err(e) => return make_err(&format!("runtime: {e}")),
        };
        rt.block_on(async move {
            let client = reqwest::Client::new();
            let mut req = client.get(url);
            if !bearer.is_empty() {
                req = req.bearer_auth(bearer);
            }
            match req.send().await {
                Ok(r) => {
                    let status = r.status().as_u16() as i64;
                    let txt = r.text().await.unwrap_or_default();
                    rhai::Array::from([
                        rhai::Dynamic::from(status),
                        rhai::Dynamic::from(txt),
                    ])
                }
                Err(e) => make_err(&format!("request: {e}")),
            }
        })
    }

    fn make_err(msg: &str) -> rhai::Array {
        rhai::Array::from([
            rhai::Dynamic::from(-1i64),
            rhai::Dynamic::from(format!("ERR: {msg}")),
        ])
    }

    fn map_to_json(m: &mut rhai::Map) -> JValue {
        let mut obj = serde_json::Map::new();
        for (k, v) in m {
            obj.insert(k.to_string(), dyn_to_json(v));
        }
        JValue::Object(obj)
    }

    fn dyn_to_json(d: &mut rhai::Dynamic) -> JValue {
        if d.is_int() {
            JValue::from(d.as_int().unwrap_or(0))
        } else if d.is_float() {
            JValue::from(d.as_float().unwrap_or(0.0))
        } else if d.is_bool() {
            JValue::from(d.as_bool().unwrap_or(false))
        } else if d.is_string() {
            JValue::from(d.clone().into_string().unwrap_or_default())
        } else if d.is_array() {
            let mut arr = d.clone().into_array().unwrap_or_default();
            JValue::Array(arr.iter_mut().map(dyn_to_json).collect())
        } else {
            JValue::Null
        }
    }
}

// ---------------------------------------------------------------------------
// 原语：crypto（libsmx 国密 SM4-GCM + SM3 KDF）
// ---------------------------------------------------------------------------

#[export_module]
pub mod crypto_primitives {
    use crate::crypto::{decrypt_with_password, derive_key_b64, encrypt_with_password};

    fn guard() -> Option<rhai::ImmutableString> {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        if !*ctx.trusted.lock().unwrap() {
            return Some("ERR: 脚本未受信任：加密原语已禁用".into());
        }
        None
    }

    /// 用给定密码加密明文，返回 "ct|nonce|salt" 三个 base64 拼接
    pub fn encrypt(plaintext: &str, password: &str) -> rhai::ImmutableString {
        if let Some(e) = guard() { return e; }
        match encrypt_with_password(plaintext, password) {
            Ok(s) => s.into(),
            Err(e) => format!("ERR: {e}").into(),
        }
    }

    /// 解密 "ct|nonce|salt"，失败返回以 ERR: 开头的字符串
    pub fn decrypt(payload: &str, password: &str) -> rhai::ImmutableString {
        if let Some(e) = guard() { return e; }
        match decrypt_with_password(payload, password) {
            Ok(s) => s.into(),
            Err(e) => format!("ERR: {e}").into(),
        }
    }

    /// 派生密钥（base64），供高级用途
    pub fn derive_key(password: &str, salt_b64: &str) -> rhai::ImmutableString {
        if let Some(e) = guard() { return e; }
        match derive_key_b64(password, salt_b64) {
            Ok(s) => s.into(),
            Err(e) => format!("ERR: {e}").into(),
        }
    }

    /// 简单 SM3 摘要（hex）
    pub fn sm3_hex(text: &str) -> rhai::ImmutableString {
        use libsmx::sm3::Sm3Hasher;
        let mut h = Sm3Hasher::new();
        h.update(text.as_bytes());
        let d = h.finalize();
        let hex: String = d.iter().map(|b| format!("{b:02x}")).collect();
        hex.into()
    }
}

// ---------------------------------------------------------------------------
// 原语：store（配置与历史读写，全部经 JSON 文件）
// ---------------------------------------------------------------------------

#[export_module]
pub mod store_primitives {
    use crate::config::AppConfig;

    fn guard() -> Option<rhai::ImmutableString> {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        if !*ctx.trusted.lock().unwrap() {
            return Some("ERR: 脚本未受信任：存储原语已禁用".into());
        }
        None
    }

    /// 读取配置字段（返回字符串；加密字段返回密文）
    pub fn get_config(key: &str) -> rhai::ImmutableString {
        if let Some(e) = guard() { return e; }
        let c = AppConfig::load();
        let v = match key {
            "api_key" => c.api_key,
            "api_key_enc" => c.api_key_enc.to_string(),
            "from_name" => c.from_name,
            "from_name_enc" => c.from_name_enc.to_string(),
            "plan_index" => c.plan_index.to_string(),
            "custom_quota" => c.custom_quota.to_string(),
            "cycle_start" => c.cycle_start,
            "cycle_mark" => c.cycle_mark,
            "month_count" => c.month_count.to_string(),
            "total_count" => c.total_count.to_string(),
            _ => String::new(),
        };
        v.into()
    }

    /// 写入配置字段
    pub fn set_config(key: &str, value: &str) -> rhai::ImmutableString {
        if let Some(e) = guard() { return e; }
        let mut c = AppConfig::load();
        match key {
            "api_key" => c.api_key = value.into(),
            "api_key_enc" => c.api_key_enc = value == "true",
            "from_name" => c.from_name = value.into(),
            "from_name_enc" => c.from_name_enc = value == "true",
            "plan_index" => c.plan_index = value.parse().unwrap_or(0),
            "custom_quota" => c.custom_quota = value.parse().unwrap_or(0),
            "cycle_start" => c.cycle_start = value.into(),
            "cycle_mark" => c.cycle_mark = value.into(),
            "month_count" => c.month_count = value.parse().unwrap_or(0),
            "total_count" => c.total_count = value.parse().unwrap_or(0),
            _ => {}
        }
        match c.save() {
            Ok(_) => "ok".into(),
            Err(e) => format!("ERR: {e}").into(),
        }
    }

    /// 自增计数（键：month_count / total_count），返回新值
    pub fn bump_count(key: &str) -> rhai::ImmutableString {
        if let Some(e) = guard() { return e; }
        let mut c = AppConfig::load();
        let v = match key {
            "month_count" => { c.month_count += 1; c.month_count }
            "total_count" => { c.total_count += 1; c.total_count }
            _ => 0,
        };
        let _ = c.save();
        v.to_string().into()
    }

    /// 读取历史记录（返回 JSON 字符串数组）
    pub fn get_history(limit: i64) -> rhai::ImmutableString {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        let hist = ctx.history.get_recent(limit as usize);
        let json: Vec<JValue> = hist.iter().map(|h| h.to_json()).collect();
        serde_json::to_string(&json).unwrap_or_default().into()
    }

    /// 追加一条历史记录（接受 Rhai Map）
    pub fn add_history(entry: rhai::Map) -> rhai::ImmutableString {
        if let Some(e) = guard() { return e; }
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        let get = |k: &str| -> String {
            entry.get(k)
                .and_then(|v| v.clone().into_string().ok())
                .unwrap_or_default()
        };
        let hist_entry = crate::history::HistoryEntry {
            ts: get("ts"),
            to: get("to"),
            subject: get("subject"),
            from: get("from"),
            status: get("status"),
            detail: get("detail"),
        };
        ctx.history.append(&hist_entry);
        "ok".into()
    }

    /// 清空全部发信历史（永久删除本地 history.json 内容）
    pub fn clear_history() -> rhai::ImmutableString {
        if let Some(e) = guard() { return e; }
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        ctx.history.clear();
        "ok".into()
    }

    /// 历史总条数
    pub fn history_count() -> i64 {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        ctx.history.len() as i64
    }
}

// ---------------------------------------------------------------------------
// 原语：ui（状态反馈 / 确认弹窗 / 日志）
// ---------------------------------------------------------------------------

#[export_module]
pub mod ui_primitives {
    /// 设置状态栏文本；is_err=true 标红
    pub fn set_status(text: &str, is_err: bool) {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        *ctx.status.lock().unwrap() = (text.to_string(), is_err);
    }

    /// 写日志（控制台 + 内存缓冲 + 加密落盘）
    /// 落盘一律为密文（SM4-GCM），使用本地自动生成的随机密钥。
    pub fn log(text: &str) -> rhai::ImmutableString {
        println!("[rhai] {text}");
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        {
            let mut logs = ctx.logs.lock().unwrap();
            logs.push(text.to_string());
            let over = logs.len().saturating_sub(500);
            if over > 0 { logs.drain(0..over); }
        }
        // 默认加密落盘，不依赖用户设置的加密密码
        let _ = ctx.log_store.append(text);
        *ctx.logs_encrypted.lock().unwrap() = true;
        text.into()
    }

    /// 请求用户确认（同步弹窗），返回 bool
    pub fn confirm(prompt: &str) -> bool {
        // 终端模式确认；GUI 下由 main 注入的回调覆盖
        use std::io::Write;
        print!("{prompt} [y/N]: ");
        let _ = std::io::stdout().flush();
        let mut s = String::new();
        std::io::stdin().read_line(&mut s).ok();
        let s = s.trim().to_lowercase();
        s == "y" || s == "yes"
    }

    /// 设置 i18n 字段：key -> 本地化文本
    pub fn set_i18n(key: &str, value: &str) {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        ctx.i18n.lock().unwrap().insert(key.to_string(), value.to_string());
    }

    /// 读取 i18n 字段，未定义返回 key 本身（便于缺省显示）
    pub fn get_i18n(key: &str) -> rhai::ImmutableString {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        let map = ctx.i18n.lock().unwrap();
        map.get(key).cloned().unwrap_or_else(|| key.to_string()).into()
    }

    /// 设置主题色：key -> 十六进制颜色（如 #3b82f6）
    pub fn set_theme(key: &str, value: &str) {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        ctx.theme.lock().unwrap().insert(key.to_string(), value.to_string());
    }

    /// 读取主题色
    pub fn get_theme(key: &str) -> rhai::ImmutableString {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        let map = ctx.theme.lock().unwrap();
        map.get(key).cloned().unwrap_or_default().into()
    }

    /// 设置暗色模式
    pub fn set_dark(dark: bool) {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        *ctx.dark_mode.lock().unwrap() = dark;
    }

    /// 当前是否暗色
    pub fn is_dark() -> bool {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        *ctx.dark_mode.lock().unwrap()
    }

    /// 设置整体部件不透明度（0.0~1.0），实现半透明自定义
    pub fn set_opacity(v: f64) {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        let mut o = v as f32;
        if o < 0.1 { o = 0.1; }
        if o > 1.0 { o = 1.0; }
        *ctx.opacity.lock().unwrap() = o;
    }
}

// ---------------------------------------------------------------------------
// 原语：trust（脚本信任机制）
//  - 所有 http/crypto/store 敏感原语均被 trusted 门控
//  - 此处提供解锁与验签查询原语
// ---------------------------------------------------------------------------

#[export_module]
pub mod trust_primitives {
    use crate::crypto::{verify_script_sig, sign_script_hex};

    /// 当前是否已受信任（解锁 + 验签通过）
    pub fn is_trusted() -> bool {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        *ctx.trusted.lock().unwrap()
    }

    /// 解锁：比对密码（与 config.script_trust_password 一致）。
    /// 成功且（若启用签名校验）通过验签后置 trusted=true。
    /// 参数：password, script_source, script_sig_hex
    /// 返回 "" 表示成功，否则为错误原因
    pub fn unlock(password: &str, script_source: &str, script_sig_hex: &str) -> rhai::ImmutableString {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        let cfg = crate::config::AppConfig::load();
        // 1) 密码比对（单项体系：复用加密密码 KDF，此处直接比对待存明文密码）
        if !cfg.script_trust_enabled {
            return "ERR: 脚本信任未启用，请先在设置中启用".into();
        }
        if cfg.script_trust_password != password {
            return "ERR: 信任密码错误".into();
        }
        // 2) 签名校验（若启用）
        let mode = cfg.script_sig_verify.clone();
        if mode != "off" {
            match verify_script_sig(script_source, &cfg.script_pubkey, script_sig_hex, &mode) {
                Ok(true) => {}
                Ok(false) => return "ERR: 脚本签名校验未通过".into(),
                Err(e) => return format!("ERR: 签名校验错误: {e}").into(),
            }
        }
        // 3) 通过
        *ctx.trusted.lock().unwrap() = true;
        *ctx.trust_password.lock().unwrap() = password.to_string();
        *ctx.sig_verify.lock().unwrap() = mode;
        "".into()
    }

    /// 锁定：撤销信任态（清空密码）
    pub fn lock() {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        *ctx.trusted.lock().unwrap() = false;
        *ctx.trust_password.lock().unwrap() = String::new();
    }

    /// 读取当前签名校验模式（"off"/"sm2"/"pq"）
    pub fn get_sig_mode() -> rhai::ImmutableString {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        ctx.sig_verify.lock().unwrap().clone().into()
    }

    /// 用私钥(hex)对脚本内容签名，返回签名(hex)。仅用于离线签发，运行时一般由外部工具完成
    pub fn sign_script(script_source: &str, priv_key_hex: &str) -> rhai::ImmutableString {
        match sign_script_hex(script_source, priv_key_hex) {
            Ok(s) => s.into(),
            Err(e) => format!("ERR: {e}").into(),
        }
    }
}

// ---------------------------------------------------------------------------
// 原语：markdown（Markdown → 邮件友好 HTML）
//  - 纯本地计算，无网络/存储副作用，因此不需要信任门控
//  - 输出已内联样式的 HTML，可直接作为 Resend 的 html 字段发送
// ---------------------------------------------------------------------------

#[export_module]
pub mod markdown_primitives {
    /// Markdown → 完整 HTML 文档（样式已内联，可直接发信）
    /// 支持 GFM：表格 / 删除线 / 任务列表 / 自动链接
    /// 示例：let html = markdown::to_html("# 标题\n\n正文");
    pub fn to_html(md: &str) -> rhai::ImmutableString {
        crate::markdown::to_html(md).into()
    }

    /// Markdown → 仅正文片段的 HTML（不包装 <html>，便于自行拼装模板）
    pub fn to_fragment(md: &str) -> rhai::ImmutableString {
        crate::markdown::to_fragment(md).into()
    }
}

// ---------------------------------------------------------------------------
// 原语：api（自动化 / 可编程 API 入口）
//  - 脚本可用 api::register 把任意函数注册为命名 handler
//  - 之后可由 UI 面板、CLI `run <name>` 或脚本内部 api::call 触发
//  - 这是「用 Rhai 构建 API、实现自动化」的入口
// ---------------------------------------------------------------------------

#[export_module]
pub mod api_primitives {
    /// 注册一个自动化 handler（无输入组件）。
    /// 参数：name（标识）、handler（Fn("函数名") 取得的函数指针）、desc（描述）
    /// 示例：api::register("demo.ping", Fn("demo_ping"), "回显测试");
    pub fn register(name: &str, handler: FnPtr, desc: &str) {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        let mut map = ctx.automations.lock().unwrap();
        map.insert(name.to_string(), crate::rhai_ext::Automation {
            handler,
            description: desc.to_string(),
            fields: Vec::new(),
        });
    }

    /// 注册一个自动化 handler，并声明它需要的输入组件。
    /// 参数：name、handler、desc、fields（组件声明数组）
    ///
    /// fields 每项为 map：`#{ key: "to", label: "收件人", kind: "text" }`
    /// - key：字段标识（提交时按声明顺序把值作为数组传给 handler）
    /// - label：界面标签
    /// - kind：`text`（默认）/ `password` / `bool` / `multiline`
    ///
    /// 示例：
    ///   api::register_with_fields("demo.send_one", Fn("demo_send_one"), "发送单封邮件",
    ///       [#{ key: "to", label: "收件人", kind: "text" },
    ///        #{ key: "subject", label: "主题", kind: "text" }]);
    pub fn register_with_fields(name: &str, handler: FnPtr, desc: &str, fields: rhai::Array) {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        let parsed = crate::rhai_ext::parse_fields(&fields);
        let mut map = ctx.automations.lock().unwrap();
        map.insert(name.to_string(), crate::rhai_ext::Automation {
            handler,
            description: desc.to_string(),
            fields: parsed,
        });
    }

    /// 列出所有已注册 handler（返回 JSON 字符串，每项 {name, desc}）
    pub fn list() -> rhai::ImmutableString {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        let map = ctx.automations.lock().unwrap();
        let items: Vec<JValue> = map.iter().map(|(k, v)| {
            serde_json::json!({ "name": k, "desc": v.description })
        }).collect();
        serde_json::to_string(&items).unwrap_or_default().into()
    }

    /// 调用一个已注册 handler（脚本内部调用）。
    /// 参数：name、args（数组，作为 handler 的单个数组参数传入）。
    /// 返回 handler 的返回值（Dynamic）。
    pub fn call(name: &str, args: rhai::Array) -> rhai::Dynamic {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        let fnptr = {
            let map = ctx.automations.lock().unwrap();
            match map.get(name) {
                Some(a) => a.handler.clone(),
                None => return rhai::Dynamic::from(format!("ERR: 未找到自动化 handler: {name}")),
            }
        };
        let engine = match ctx.engine.get() {
            Some(e) => e.clone(),
            None => return rhai::Dynamic::from("ERR: 引擎未就绪"),
        };
        let ast = match ctx.ast.get() {
            Some(a) => a.clone(),
            None => return rhai::Dynamic::from("ERR: 脚本未就绪"),
        };
        // handler 以 (args,) 单参数为约定：args 为数组
        match fnptr.call::<rhai::Dynamic>(&*engine, &*ast, (args,)) {
            Ok(v) => v,
            Err(e) => rhai::Dynamic::from(format!("ERR: 调用失败: {e}")),
        }
    }

    /// 当前已注册数量
    pub fn count() -> i64 {
        let ctx = crate::RHAI_CTX.get().expect("rhai ctx");
        ctx.automations.lock().unwrap().len() as i64
    }
}

// ---------------------------------------------------------------------------
// 引擎构建
// ---------------------------------------------------------------------------

pub fn build_engine() -> Engine {
    let mut engine = Engine::new();
    // 同步模式，便于 GUI 调用
    engine.set_max_call_levels(64);
    engine.set_max_expr_depths(128, 128);

    // 基础辅助函数（脚本常用类型转换）
    engine.register_fn("to_int", |s: &str| -> i64 {
        s.trim().parse().unwrap_or(0)
    });
    // 字符串 trim 辅助（部分 Rhai 默认包未含）
    engine.register_fn("trim", |s: &str| -> rhai::ImmutableString {
        s.trim().to_string().into()
    });

    // 注册原语模块（以命名空间形式：http:: / crypto:: / store:: / ui:: / trust::）
    let http_mod: rhai::Shared<rhai::Module> = exported_module!(http_primitives).into();
    let crypto_mod: rhai::Shared<rhai::Module> = exported_module!(crypto_primitives).into();
    let store_mod: rhai::Shared<rhai::Module> = exported_module!(store_primitives).into();
    let ui_mod: rhai::Shared<rhai::Module> = exported_module!(ui_primitives).into();
    let trust_mod: rhai::Shared<rhai::Module> = exported_module!(trust_primitives).into();
    let api_mod: rhai::Shared<rhai::Module> = exported_module!(api_primitives).into();
    let md_mod: rhai::Shared<rhai::Module> = exported_module!(markdown_primitives).into();
    engine.register_static_module("http", http_mod);
    engine.register_static_module("crypto", crypto_mod);
    engine.register_static_module("store", store_mod);
    engine.register_static_module("ui", ui_mod);
    engine.register_static_module("trust", trust_mod);
    engine.register_static_module("api", api_mod);
    engine.register_static_module("markdown", md_mod);

    // 提供 SOrg 标识常量（通过模块注册，供脚本使用）
    let mut const_mod = rhai::Module::new();
    const_mod.set_var("SORG_BANNER", SORG_BANNER.to_string());
    const_mod.set_var("SNOWARE", "SNOWARE".to_string());
    const_mod.set_var("SORG", "SOrg".to_string());
    const_mod.set_var("RESEND_API", "https://api.resend.com/emails".to_string());
    engine.register_global_module(const_mod.into());

    engine
}

/// 加载脚本文件（默认 scripts/default.rhai），返回 AST
pub fn compile_script(engine: &Engine, path: &PathBuf) -> Result<AST> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("读取脚本 {path:?} 失败: {e}"))?;
    let ast = engine.compile(&src)
        .map_err(|e| anyhow::anyhow!("脚本编译失败: {e}"))?;
    Ok(ast)
}

/// 在脚本作用域中注入标准变量（to/from/subject/body 等）
pub fn make_scope() -> Scope<'static> {
    Scope::new()
}
