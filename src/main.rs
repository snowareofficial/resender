// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//![allow(non_snake_case)]
//! Resender — Rhai 驱动的发信工具
//!
//! 架构：Rust 仅注册安全原语（http / crypto / store / ui），
//! 所有业务逻辑（身份获取、禁止判定、发信、统计）由 Rhai 脚本动态拼装。
//!
//! SOrg / SNOWARE 标识（仅展示，不绑定具体组织）：
//!   <<*>> SOrg :: -^v- SNOWARE
//!   Copyright (C) 2026~now S.A. Licensed under Mulan PubL v2.

//! 无黑框（仅 Windows / release）：
//! - debug 构建保留控制台窗口，便于查看日志
//! - release 构建隐藏控制台，双击启动 GUI 不再弹出黑框
//! - 命令行模式（send / run / version / help）在 release 下通过
//!   `attach_parent_console()` 附加父控制台，输出仍可见（见下方实现）
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// 纯 CLI 构建（--no-default-features）下，草稿 / 套餐额度 / 自动化面板等
// 仅为 GUI 服务的项不会被用到。同一套代码编出两种形态，为「另一种形态」
// 到处打 cfg 会严重破坏可读性，故在 crate 根统一允许死代码。
#![cfg_attr(not(feature = "gui"), allow(dead_code, unused_imports))]

mod config;
mod sml_store;
mod crypto;
mod draft;
mod history;
mod i18n;
mod log;
mod markdown;
mod rhai_ext;
mod update;

use anyhow::Result;
use base64::Engine as _;
#[cfg(feature = "gui")]
use slint::{Model, ModelRc, VecModel};
#[cfg(feature = "gui")]
use std::rc::Rc;
use std::path::PathBuf;
use std::sync::Arc;

use config::{AppConfig, PLANS, compute_quota, gregorian_std};
use history::HistoryStore;
use rhai_ext::{BUILTIN_SCRIPT, RhaiContext, SORG_BANNER, build_engine, compile_script, compile_script_from_str, make_scope};

/// 应用版本：唯一来源为 Cargo.toml 的 `version`（编译期展开）。
/// GUI 关于页与 CLI `--version` 均读取此常量，发版后自动同步。
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// SWE Serial 编号：`<< 19 * 55 >>` = 1955，本项目的档案标识。
/// 单一来源：GUI 关于页与 README 均引用此常量（crossduty/1955.md 档案与此对应）。
pub const SWE_SERIAL: &str = "<< 19 * 55 >>";
/// SWE Serial 编号来历（纪念性说明，显示于关于页）。
pub const SWE_SERIAL_NOTE: &str =
    "谨以此编号纪念 1955 年 10 月 1 日新疆维吾尔自治区建立。";

/// 全局 Rhai 上下文（供原语模块通过 RHAI_CTX.get() 访问）
pub static RHAI_CTX: std::sync::OnceLock<Arc<RhaiContext>> = std::sync::OnceLock::new();

#[cfg(feature = "gui")]
slint::include_modules!();

#[cfg(feature = "gui")]
fn ss<S: Into<slint::SharedString>>(s: S) -> slint::SharedString {
    s.into()
}

fn now_iso_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days = (secs / 86400) as i64 + 719163;
    let (y, m, d) = gregorian_std(days);
    let hh = (secs % 86400) / 3600;
    let mm = (secs % 3600) / 60;
    let ss = secs % 60;
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, hh, mm, ss)
}

fn scripts_dir() -> PathBuf {
    // 始终优先返回「可执行文件同级的 scripts/default.rhai」作为权威路径：
    // 这样自解压（运行时写出内置脚本）与热重载（reload）都落在 exe 同级，
    // 不再依赖启动时的 current_dir。
    let exe = std::env::current_exe().unwrap_or_default();
    if let Some(dir) = exe.parent() {
        return dir.join("scripts").join("default.rhai");
    }
    let mut p = std::env::current_dir().unwrap_or_default();
    p.push("scripts");
    p.push("default.rhai");
    p
}

/// 构建引擎、Rhai 上下文、编译脚本（GUI 与 CLI 共用）
fn setup() -> Result<(Arc<rhai::Engine>, rhai::AST, Arc<RhaiContext>, PathBuf)> {
    let history = Arc::new(HistoryStore::new()?);
    let ctx = Arc::new(RhaiContext::new(history)?);
    let _ = RHAI_CTX.set(ctx.clone());

    let mut engine = build_engine();
    engine.register_fn("rust_now_iso", now_iso_string);
    engine.register_fn("parse_json", |s: &str| -> rhai::Dynamic {
        match serde_json::from_str::<serde_json::Value>(s) {
            Ok(v) => rhai::serde::to_dynamic(v).unwrap_or(rhai::Dynamic::UNIT),
            Err(_) => rhai::Dynamic::UNIT,
        }
    });
    let engine = Arc::new(engine);

    let script_path = scripts_dir();
    // 解析脚本源：优先使用 exe 同级的外部脚本（允许部署后覆盖/热改）；
    // 若外部脚本缺失，则从编译期嵌入的内置副本自解压（写出到同级 scripts/
    // 目录）并直接使用内存内容，从而实现「单 exe 自包含、无需附带 scripts」。
    let script_src: String = if script_path.exists() {
        std::fs::read_to_string(&script_path)
            .map_err(|e| anyhow::anyhow!("读取脚本 {script_path:?} 失败: {e}"))?
    } else {
        if let Some(parent) = script_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&script_path, BUILTIN_SCRIPT) {
            eprintln!("提示：无法自解压脚本到 {script_path:?}（{e}），将使用内置副本运行");
        }
        BUILTIN_SCRIPT.to_string()
    };
    // 内置脚本完整性校验：计算 SM3 哈希，匹配官方快照则视为
    // 「自带且未修改」，自动授予信任（builtin_verified=true）。
    // 任何被篡改/替换的脚本哈希不匹配，必须显式 unlock 才能信任，
    // 从而「自动信任仅限自带未修改脚本」，不破坏安全能力。
    {
        // 与 build.rs 编译期校验保持一致：按 LF 归一化后计算哈希，
        // 避免磁盘 CRLF 与仓库 LF 差异导致运行期误判为「脚本被篡改」。
        let src: Vec<u8> = script_src
            .as_bytes()
            .iter()
            .copied()
            .filter(|&b| b != b'\r')
            .collect();
        let mut h = libsmx::sm3::Sm3Hasher::new();
        h.update(&src);
        let digest = h.finalize();
        let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
        // expected 由 build.rs 在编译期校验过「源码 == 快照」后注入，
        // 此处仅作为运行期篡改检测的基线（若发布后脚本被改，哈希会不符）。
        let expected = env!("RESENDER_BUILTIN_SCRIPT_SM3");
        let verified = hex.eq_ignore_ascii_case(expected);
        *ctx.builtin_verified.lock().unwrap() = verified;
        if !verified {
            eprintln!(
                "[builtin-check] 警告：运行期脚本哈希({})与编译期快照({})不符，自动信任已禁用（需手动解锁）",
                hex, expected
            );
        }
    }
    let ast = compile_script_from_str(&engine, &script_src)?;
    // 执行一次脚本顶层语句：触发 api::register 等初始化（i18n/主题/自动化注册）
    if let Err(e) = engine.run_ast(&ast) {
        eprintln!("脚本初始化执行警告: {e}");
    }
    // 自动识别系统语言，填充默认文案到 i18n 表（脚本 setup_i18n 可覆盖）
    // 注意：需在脚本 setup_i18n 执行前调用，否则脚本定义会被本目录覆盖
    let ui_lang = crate::i18n::fill(&ctx);
    *ctx.ui_lang.lock().unwrap() = ui_lang;
    // 让原语模块（api::call）能拿到引擎与脚本 AST 引用
    let _ = ctx.engine.set(engine.clone());
    let _ = ctx.ast.set(ast.clone().into());
    Ok((engine, ast, ctx, script_path))
}

/// Windows：GUI 子系统（release 无黑框）下，CLI 模式若 stdout 无效
/// （进程没有关联控制台），附加到启动它的父终端，让 `resender run ...`
/// 在 cmd / PowerShell 里仍能看到输出。
///
/// 原理：
/// - `windows_subsystem = "windows"` 的进程默认不关联控制台，stdout 句柄无效
/// - `AttachConsole(ATTACH_PARENT_PROCESS)` 附加到父进程的控制台
/// - Rust 的 `stdout()` 每次写入都会重新 `GetStdHandle`，因此替换句柄后
///   `println!` / `eprintln!` 立即生效（无需 freopen）
///
/// 非 Windows 平台为空实现：macOS / Linux 原生就有控制台，无需处理。
#[cfg(windows)]
fn attach_parent_console() {
    use std::os::windows::io::AsRawHandle;

    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    // GetStdHandle / SetStdHandle 的标准句柄标识（-11 / -12 的 u32 形式）
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;
    // GetStdHandle 失败时的返回值
    const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = usize::MAX as *mut std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetConsoleWindow() -> *mut std::ffi::c_void;
        fn GetStdHandle(n_std_handle: u32) -> *mut std::ffi::c_void;
        fn SetStdHandle(n_std_handle: u32, handle: *mut std::ffi::c_void) -> i32;
    }

    unsafe {
        // 已有控制台（debug 构建 / 已关联）则无需处理
        if !GetConsoleWindow().is_null() {
            return;
        }
        let is_valid = |h: *mut std::ffi::c_void| !h.is_null() && h != INVALID_HANDLE_VALUE;

        // 关键：只修复「无效」的句柄。
        // 若 stdout 已被重定向到文件/管道（`> file` / `|`），GetStdHandle 返回
        // 有效句柄，绝不能覆盖——否则重定向的输出会凭空丢失。
        let out = GetStdHandle(STD_OUTPUT_HANDLE);
        let err = GetStdHandle(STD_ERROR_HANDLE);
        let fix_out = !is_valid(out);
        let fix_err = !is_valid(err);
        if !fix_out && !fix_err {
            return;
        }
        // 附加父控制台；失败说明没有父终端（如从资源管理器启动），放弃
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        // 把无效的 stdout / stderr 重定向到控制台输出设备
        let redirect = |id: u32| {
            if let Ok(f) = std::fs::OpenOptions::new()
                .write(true)
                .open(r"\\.\CONOUT$")
            {
                SetStdHandle(id, f.as_raw_handle() as *mut std::ffi::c_void);
                // 进程生命周期内持有句柄；显式泄漏避免关闭导致后续写入失败
                std::mem::forget(f);
            }
        };
        if fix_out {
            redirect(STD_OUTPUT_HANDLE);
        }
        if fix_err {
            redirect(STD_ERROR_HANDLE);
        }
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // 命令行调用功能（无 GUI）
    if args.len() > 1 && (args[1] == "send" || args[1] == "run" || args[1] == "help" || args[1] == "--help" || args[1] == "-h"
        || args[1] == "version" || args[1] == "--version" || args[1] == "-V"
        || args[1] == "check-update") {
        // release 下本进程无控制台：附加父终端，让 CLI 输出可见
        attach_parent_console();
        return run_cli(&args[1..]);
    }

    #[cfg(feature = "gui")]
    return run_gui();

    #[cfg(not(feature = "gui"))]
    Err(anyhow::anyhow!(
        "本构建未启用 GUI（编译时未开启 `gui` feature）。\n\
         \x20  命令行用法：resender send | run | version | help | check-update\n\
         \x20  需要图形界面请用默认构建：cargo build --release"
    ))
}


/// GUI 入口（仅 `gui` feature 启用时编译）。
///
/// 原是 `main()` 的主体：不带 CLI 参数启动时走这里。
/// 抽成独立函数，便于纯 CLI 构建（--no-default-features）整体排除 Slint 代码。
#[cfg(feature = "gui")]
fn run_gui() -> Result<()> {

        let (engine, ast, ctx, script_path) = setup()?;

        // UI
        let ui = App::new()?;
        // 强制原生控件（输入框/复选框等）使用浅色配色方案，
        // 避免系统处于暗色模式时它们渲染成白字而与浅色背景冲突。
        ui.set_color_scheme(slint::private_unstable_api::re_exports::ColorScheme::Light);
        // 版本取自 Cargo.toml（APP_VERSION），发版后自动同步，无需在 UI 里手改
        ui.set_app_version(ss(APP_VERSION));
        // SWE Serial 编号（单一来源 SWE_SERIAL）+ 编号来历
        ui.set_swe_serial(ss(SWE_SERIAL));
        ui.set_swe_serial_note(ss(SWE_SERIAL_NOTE));
        let cfg = AppConfig::load();

        // 平台检测：用于决定窗口控制按钮的位置/顺序（≥3 平台）
        let platform = match std::env::consts::OS {
            "macos" => "macos",
            "windows" => "windows",
            "linux" => "linux",
            other => other,
        };
        ui.set_platform(ss(platform));

        // 初始化套餐列表
        let plan_strings: Vec<slint::SharedString> =
            PLANS.iter().map(|(n, _)| slint::SharedString::from(n.to_string())).collect();
        let model = Rc::new(VecModel::from(plan_strings));
        ui.set_plans(ModelRc::from(model.clone()));

        // 周期重置检查
        let mut cfg = cfg;
        let cycle_key = if cfg.cycle_start.is_empty() {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let days = (secs / 86400) as i64 + 719163;
            let (y, m, _) = gregorian_std(days);
            format!("{:04}-{:02}-01", y, m)
        } else {
            cfg.cycle_start.clone()
        };
        if cfg.cycle_mark != cycle_key {
            cfg.month_count = 0;
            cfg.cycle_mark = cycle_key.clone();
            let _ = cfg.save();
        }

        // 反映到 UI
        ui.set_api_key(ss(cfg.api_key.clone()));
        ui.set_from_name(ss(cfg.from_name.clone()));
        ui.set_keep_after_send(cfg.keep_after_send);
        ui.set_plan_index(cfg.plan_index as i32);
        ui.set_custom_quota_text(ss(cfg.custom_quota.to_string()));
        ui.set_cycle_start(ss(cfg.cycle_start.clone()));
        ui.set_month_count(cfg.month_count as i32);
        ui.set_total_count(cfg.total_count as i32);
        ui.set_from_display(ss(cfg.from_name.clone()));

        let quota = compute_quota(cfg.plan_index, cfg.custom_quota);
        ui.set_plan_quota(quota as i32);
        ui.set_plan_label(ss(PLANS.get(cfg.plan_index).map(|(n, _)| (*n).to_string()).unwrap_or_default()));

        // 设置脚本页面文本
        ui.set_script_path(ss(script_path.to_string_lossy().to_string()));
        ui.set_sorg_banner(ss(SORG_BANNER));

        // 刷新历史列表
        refresh_history(&ui, &ctx);
        // 刷新运行日志（内存 + 加密落盘文件）
        refresh_logs_ui_inner(&ui, &ctx);
        // 刷新自动化 handler 列表（脚本在加载时已注册）
        ui.set_automation_items(ModelRc::from(Rc::new(VecModel::from(automation_items(&ctx)))));

        // 恢复上次保存的草稿（发信页表单）
        restore_draft(&ui);

        // 启动时静默检查更新（配置了 update_url 才检查；失败不打扰用户）
        if !cfg.update_url.is_empty() {
            let url = cfg.update_url.clone();
            let ui_weak_upd = ui.as_weak();
            std::thread::spawn(move || {
                match crate::update::fetch_remote(&url) {
                    Ok(rv) if crate::update::has_update(APP_VERSION, &rv) => {
                        // 后台线程更新 UI 必须回到主线程事件循环，否则不刷新
                        let note = if rv.note.is_empty() {
                            String::new()
                        } else {
                            format!("\n{}", rv.note)
                        };
                        let msg = format!("发现新版本 {}（当前 {}）{}", rv.latest, APP_VERSION, note);
                        with_ui(ui_weak_upd, move |ui| {
                            set_status_state(&ui, "发现新版本", 0, Some(msg));
                        });
                    }
                    _ => {}
                }
            });
        }

        // 启动时执行脚本初始化（i18n / 默认主题），让界面文本立即生效
        {
            let mut scope = make_scope();
            let _ = engine.call_fn::<()>(&mut scope, &ast, "setup_i18n", ());
            let _ = engine.call_fn::<()>(&mut scope, &ast, "apply_theme", (false,));
        }

        // 应用脚本定义的 i18n / 主题 / 暗色 / 透明度
        apply_ui_state(&ui, &ctx);

        // 初始界面状态（导航/专注，从 config 读取）
        ui.set_nav_collapsed(cfg.nav_collapsed);
        ui.set_zen_mode(cfg.zen_mode);

        // 信任设置反映到 UI（密码不回填 UI，仅回填启用/模式/公钥）
        ui.set_trust_enabled(cfg.script_trust_enabled);
        ui.set_sig_mode(ss(cfg.script_sig_verify.clone()));
        ui.set_pubkey(ss(cfg.script_pubkey.clone()));
        ui.set_trust_unlocked(false);

        // ---- 保存设置（仍走 Rust，因为涉及 UI 字段 <-> config 映射）----
        let ui_weak = ui.as_weak();
        let ctx_save = ctx.clone();
        ui.on_save_settings(move |api_key, from_name, crypto_password, plan_index, enc_apikey, enc_from, custom_quota_text, cycle_start, keep_after_send| {
            let custom_quota: i64 = custom_quota_text.trim().parse().unwrap_or(0);
            let mut c = AppConfig::load();
            let mut api_key_out = api_key.to_string();
            let mut api_enc = false;
            if enc_apikey && !crypto_password.is_empty() {
                match crypto::encrypt_with_password(&api_key, &crypto_password) {
                    Ok(s) => { api_key_out = s; api_enc = true; }
                    Err(e) => {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_status_text(ss(format!("加密失败: {e}")));
                            ui.set_status_err(true);
                        }
                        return;
                    }
                }
            }
            let mut from_out = from_name.to_string();
            let mut from_enc = false;
            if enc_from && !from_name.is_empty() && !crypto_password.is_empty() {
                match crypto::encrypt_with_password(&from_name, &crypto_password) {
                    Ok(s) => { from_out = s; from_enc = true; }
                    Err(e) => {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_status_text(ss(format!("加密失败: {e}")));
                            ui.set_status_err(true);
                        }
                        return;
                    }
                }
            }
            c.api_key = api_key_out;
            c.api_key_enc = api_enc;
            c.from_name = from_out;
            c.from_name_enc = from_enc;
            c.plan_index = plan_index as usize;
            c.custom_quota = custom_quota;
            c.cycle_start = cycle_start.to_string();
            c.keep_after_send = keep_after_send;

            // 信任设置（从 UI 读取，单独获取）
            if let Some(ui) = ui_weak.upgrade() {
                c.script_trust_enabled = ui.get_trust_enabled();
                c.script_trust_password = ui.get_trust_password().to_string();
                c.script_sig_verify = ui.get_sig_mode().to_string();
                c.script_pubkey = ui.get_pubkey().to_string();
            }

            // 记忆密码供脚本使用（解锁）
            *ctx_save.crypto_password.lock().unwrap() = crypto_password.to_string();

            match c.save() {
                Ok(_) => {
                    if let Some(ui) = ui_weak.upgrade() {
                        let quota = compute_quota(c.plan_index, c.custom_quota);
                        ui.set_plan_quota(quota as i32);
                        ui.set_plan_label(ss(PLANS.get(c.plan_index).map(|(n,_)| (*n).to_string()).unwrap_or_default()));
                        ui.set_status_text(ss("设置已保存"));
                        ui.set_status_err(false);
                        ui.set_unlocked(!crypto_password.is_empty());
                    }
                }
                Err(e) => {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_status_text(ss(format!("保存失败: {e}")));
                        ui.set_status_err(true);
                    }
                }
            }
        });

        // ---- 解锁：记忆密码供脚本读取 ----
        let ui_weak2 = ui.as_weak();
        let ctx_unlock = ctx.clone();
        ui.on_unlock(move |pwd| {
            *ctx_unlock.crypto_password.lock().unwrap() = pwd.to_string();
            if let Some(ui) = ui_weak2.upgrade() {
                ui.set_unlocked(!pwd.is_empty());
            }
        });

        // ---- 脚本信任解锁：先持久化 UI 信任字段，再比对密码（必要时验签）----
        let ui_weak_t = ui.as_weak();
        let script_path_t = script_path.clone();
        ui.on_unlock_trust(move |pwd, sig_hex| {
            // 先把 UI 上的信任设置落盘，确保 unlock 读到的 config 与 UI 一致
            if let Some(ui) = ui_weak_t.upgrade() {
                let mut c = AppConfig::load();
                c.script_trust_enabled = ui.get_trust_enabled();
                c.script_trust_password = ui.get_trust_password().to_string();
                c.script_sig_verify = ui.get_sig_mode().to_string();
                c.script_pubkey = ui.get_pubkey().to_string();
                let _ = c.save();
            }
            let src = std::fs::read_to_string(&script_path_t).unwrap_or_default();
            let res = rhai_ext::trust_primitives::unlock(&pwd, &src, &sig_hex);
            let s = res.to_string();
            let ok = s.is_empty();
            if let Some(ui) = ui_weak_t.upgrade() {
                ui.set_trust_unlocked(ok);
                ui.set_status_text(ss(if ok { "脚本已受信任，功能已开放" } else { &s }));
                ui.set_status_err(!ok);
            }
        });

        // ---- 信任锁定：撤销信任态 ----
        let ui_weak_tl = ui.as_weak();
        let ctx_lock = ctx.clone();
        ui.on_lock_trust(move || {
            *ctx_lock.trusted.lock().unwrap() = false;
            *ctx_lock.trust_password.lock().unwrap() = String::new();
            if let Some(ui) = ui_weak_tl.upgrade() {
                ui.set_trust_unlocked(false);
                ui.set_status_text(ss("已锁定脚本信任"));
                ui.set_status_err(false);
            }
        });

        // ---- 发送（调用 Rhai 脚本 send_mail）----
        let ui_weak3 = ui.as_weak();
        let engine_send = engine.clone();
        let ast_send = ast.clone();
        let ctx_send = ctx.clone();
        ui.on_send_mail(move |to_text, subject, body, mode, attachments| {
            let pw = ctx_send.crypto_password.lock().unwrap().clone();
            let to_s = to_text.to_string();
            let subj_s = subject.to_string();
            let body_s = body.to_string();
            let attach_paths: Vec<String> = attachments.iter().map(|s| s.to_string()).collect();
            // 立即进入忙碌态（按钮禁用 + 状态栏进行中，蓝色保底 0.2s），保证反馈即时
            if let Some(ui) = ui_weak3.upgrade() {
                ui.set_sending(true);
                set_status_state(&ui, "发送中…", 0, None);
            }

            let ui_send = ui_weak3.clone();
            let eng = engine_send.clone();
            let ast_c = ast_send.clone();
            let ctx_spawn = ctx_send.clone();
            std::thread::spawn(move || {
                // 所有耗时操作（Markdown 转换、附件读取与 base64）都在后台线程，
                // 主线程只做 UI 反馈，杜绝大内容/大附件卡界面
                let mut scope = make_scope();
                // 设置全局 crypto_password
                scope.push("crypto_password", pw.clone());
                // 阶段 1：准备正文（Markdown/HTML 转换等）
                set_status_state_async(&ui_send, "正在准备正文…".to_string(), 0, None);
                let t_body = std::time::Instant::now();
                let (final_body, html_b) = resolve_body(&body_s, mode);
                let t_body_ms = t_body.elapsed().as_millis();

                // 阶段 2：逐个读取并编码附件，实时反馈「第几个 / 共几个 + 文件名」，
                // 让大附件的等待过程有可见进展，而不是干等一个「发送中…」。
                let mut attach_list: Vec<rhai::Dynamic> = Vec::new();
                let mut total_bytes: u64 = 0;
                let t_attach = std::time::Instant::now();
                let attach_total = attach_paths.len();
                for (idx, path_str) in attach_paths.iter().enumerate() {
                    let p = std::path::Path::new(path_str);
                    let filename = p
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if attach_total > 0 {
                        set_status_state_async(
                            &ui_send,
                            format!(
                                "正在读取附件 {}/{}: {}",
                                idx + 1,
                                attach_total,
                                filename
                            ),
                            0,
                            None,
                        );
                    }
                    match std::fs::read(p) {
                        Ok(bytes) => {
                            total_bytes += bytes.len() as u64;
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                            let mut m = rhai::Map::new();
                            m.insert("filename".into(), filename.clone().into());
                            m.insert("content".into(), b64.into());
                            attach_list.push(rhai::Dynamic::from(m));
                        }
                        Err(e) => {
                            // 经事件循环回到主线程更新 UI，否则界面不刷新
                            let msg = format!("读取附件失败: {e}");
                            with_ui(ui_send.clone(), move |ui| {
                                set_status_state(&ui, &msg, 2, None);
                                ui.set_sending(false);
                            });
                            return;
                        }
                    }
                }
                let attach_count = attach_list.len();
                let t_attach_ms = t_attach.elapsed().as_millis();

                // 阶段 3：上传（最耗时）。提前告知规模，让用户知道「正在进行」而非卡死。
                let size_desc = if attach_count > 0 {
                    format!("（{} 个附件，{}）", attach_count, human_size(total_bytes))
                } else {
                    String::new()
                };
                set_status_state_async(&ui_send, format!("正在上传{}…", size_desc), 0, None);

                // 上传心跳：脚本内的 HTTP 请求是同步的，期间无法细分进度；
                // 用每秒刷新「已用 Ns」让用户明确看到任务仍在进行（而非界面假死/卡死）。
                let hb_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let hb_flag = hb_stop.clone();
                let hb_ui = ui_send.clone();
                let hb_desc = size_desc.clone();
                let heartbeat = std::thread::spawn(move || {
                    let mut secs = 0u64;
                    loop {
                        // 每 0.5s 刷新一次，让 MB 进度变化看得见
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        if hb_flag.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        secs += 1;
                        // 优先展示真实上传进度（MB/MB）；尚未开始上传时退化为计时
                        let (sent, total) = rhai_ext::upload_progress();
                        let text = if total > 0 {
                            let pct = (sent as f64 / total as f64 * 100.0).min(100.0);
                            format!(
                                "正在上传{}… {:.1} MB / {:.1} MB（{:.0}%）",
                                hb_desc,
                                sent as f64 / MB_F,
                                total as f64 / MB_F,
                                pct
                            )
                        } else {
                            format!("正在上传{}… 已用 {}s", hb_desc, secs / 2)
                        };
                        set_status_state_async(&hb_ui, text, 0, None);
                    }
                });

                let t_upload_start = std::time::Instant::now();
                let result: Result<(), Box<dyn std::error::Error>> = (|| {
                    eng.call_fn(&mut scope, &ast_c, "send_mail",
                        (to_s, subj_s, final_body, html_b, pw, attach_list))?;
                    Ok(())
                })();
                // 记录各阶段耗时到运行日志，便于定位「发送慢」到底慢在哪一环
                let upload_ms = t_upload_start.elapsed().as_millis();
                {
                    let mut logs = ctx_spawn.logs.lock().unwrap();
                    logs.push(format!(
                        "[耗时] 准备正文 {}ms，读取附件 {}ms，网络发送 {}ms",
                        t_body_ms, t_attach_ms, upload_ms
                    ));
                    let _ = ctx_spawn.log_store.append(&format!(
                        "[耗时] 准备正文 {}ms，读取附件 {}ms，网络发送 {}ms",
                        t_body_ms, t_attach_ms, upload_ms
                    ));
                }
                // 停止心跳（上传已结束）
                hb_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = heartbeat.join();

                // 读取脚本写入的最终状态
                let (status, is_err) = ctx_spawn.status.lock().unwrap().clone();
                let final_state = if is_err { 2 } else { 1 };
                with_ui(ui_send.clone(), move |ui| {
                    set_status_state(&ui, &status, final_state, None);
                    ui.set_sending(false);
                    // 刷新计数 + 历史
                    let c = AppConfig::load();
                    ui.set_month_count(c.month_count as i32);
                    ui.set_total_count(c.total_count as i32);
                    refresh_history_ui(&ui, &ctx_spawn);
                    // 刷新运行日志
                    refresh_logs_ui_inner(&ui, &ctx_spawn);
                    // 发送成功且设置「不保留原件」时，清空表单与草稿
                    if !is_err {
                        let cfg_after = AppConfig::load();
                        if !cfg_after.keep_after_send {
                            clear_compose_form(&ui);
                        }
                    }
                });
                if let Err(e) = result {
                    let msg = format!("脚本执行错误: {e}");
                    with_ui(ui_send.clone(), move |ui| {
                        set_status_state(&ui, &msg, 2, None);
                        ui.set_sending(false);
                    });
                }
            });
        });

        // ---- 存为草稿 / 清空表单 ----
        let ui_weak_draft = ui.as_weak();
        ui.on_save_draft(move |to, subject, body, mode, attachments| {
            let d = draft::Draft {
                to: to.to_string(),
                subject: subject.to_string(),
                body: body.to_string(),
                body_mode: mode,
                attachments: attachments.iter().map(|s| s.to_string()).collect(),
            };
            if d.is_empty() {
                if let Some(ui) = ui_weak_draft.upgrade() {
                    ui.set_status_text(ss("表单为空，未保存草稿"));
                    ui.set_status_err(false);
                }
                return;
            }
            match d.save() {
                Ok(()) => {
                    if let Some(ui) = ui_weak_draft.upgrade() {
                        set_status_state(&ui, "草稿已保存", 1, Some("草稿已保存，下次启动自动恢复".into()));
                    }
                }
                Err(e) => {
                    if let Some(ui) = ui_weak_draft.upgrade() {
                        set_status_state(&ui, &format!("草稿保存失败: {e}"), 2, None);
                    }
                }
            }
        });
        let ui_weak_clear = ui.as_weak();
        ui.on_clear_form(move || {
            if let Some(ui) = ui_weak_clear.upgrade() {
                clear_compose_form(&ui);
                set_status_state(&ui, "表单已清空", 1, None);
            }
        });

        // ---- 正文预览：生成 HTML 写入临时文件，用系统默认浏览器打开 ----
        // Slint 没有 HTML 渲染组件，交给浏览器才能真正反映收件方看到的排版
        let ui_weak_preview = ui.as_weak();
        ui.on_preview_body(move |body, mode| {
            let body_s = body.to_string();
            let html = match mode {
                0 => crate::markdown::to_html(&body_s),
                1 => body_s.clone(),
                // 纯文本：包一层最小 HTML，转义后放进 <pre> 以保留换行
                _ => format!(
                    "<!DOCTYPE html><html><body><pre style=\"font-family: Consolas, \
                 monospace; white-space: pre-wrap; padding: 20px;\">{}\
                 </pre></body></html>",
                    html_escape(&body_s)
                ),
            };
            let path = std::env::temp_dir().join("resender_preview.html");
            let (text, is_err) = match std::fs::write(&path, &html).and_then(|_| open_in_browser(&path)) {
                Ok(()) => (format!("已在浏览器打开预览：{}", path.display()), false),
                Err(e) => (format!("预览失败：{e}"), true),
            };
            if let Some(ui) = ui_weak_preview.upgrade() {
                ui.set_status_text(ss(text));
                ui.set_status_err(is_err);
            }
        });

        // ---- 选择附件 ----
        let ui_weak_attach = ui.as_weak();
        ui.on_pick_attachment(move || {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                if let Some(ui) = ui_weak_attach.upgrade() {
                    let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string_lossy().to_string());
                    let mut paths: Vec<slint::SharedString> = ui.get_attachments().iter().collect();
                    let mut names: Vec<slint::SharedString> = ui.get_attachment_names().iter().collect();
                    paths.push(slint::SharedString::from(path.to_string_lossy().to_string()));
                    names.push(slint::SharedString::from(name));
                    ui.set_attachments(ModelRc::from(Rc::new(VecModel::from(paths))));
                    ui.set_attachment_names(ModelRc::from(Rc::new(VecModel::from(names))));
                }
            }
        });

        // ---- 移除附件 ----
        let ui_weak_remove = ui.as_weak();
        ui.on_remove_attachment(move |idx| {
            if let Some(ui) = ui_weak_remove.upgrade() {
                let mut paths: Vec<slint::SharedString> = ui.get_attachments().iter().collect();
                let mut names: Vec<slint::SharedString> = ui.get_attachment_names().iter().collect();
                if (idx as usize) < paths.len() {
                    paths.remove(idx as usize);
                    names.remove(idx as usize);
                    ui.set_attachments(ModelRc::from(Rc::new(VecModel::from(paths))));
                    ui.set_attachment_names(ModelRc::from(Rc::new(VecModel::from(names))));
                }
            }
        });

        // ---- 重新加载脚本 ----
        let ui_weak4 = ui.as_weak();
        let engine_reload = engine.clone();
        let ctx_reload = ctx.clone();
        ui.on_reload_script(move || {
            let p = scripts_dir();
            match compile_script(&engine_reload, &p) {
                Ok(_) => {
                    if let Some(ui) = ui_weak4.upgrade() {
                        ui.set_status_text(ss("脚本已重新加载"));
                        ui.set_status_err(false);
                        // 重新加载后刷新自动化列表（脚本会重新注册 handler）
                        ui.set_automation_items(ModelRc::from(Rc::new(VecModel::from(
                            automation_items(&ctx_reload),
                        ))));
                    }
                }
                Err(e) => {
                    if let Some(ui) = ui_weak4.upgrade() {
                        ui.set_status_text(ss(format!("脚本加载失败: {e}")));
                        ui.set_status_err(true);
                    }
                }
            }
        });

        // ---- 刷新自动化 handler 列表（含脚本声明的输入组件）----
        let ctx_list = ctx.clone();
        let ui_list = ui.as_weak();
        ui.on_refresh_automations(move || {
            let items = automation_items(&ctx_list);
            if let Some(ui) = ui_list.upgrade() {
                ui.set_automation_items(ModelRc::from(Rc::new(VecModel::from(items))));
            }
        });

        // ---- 记录某个 handler 输入组件的值 ----
        let ctx_fv = ctx.clone();
        ui.on_set_field_value(move |name, key, value| {
            ctx_fv.field_values.lock().unwrap().insert(
                (name.to_string(), key.to_string()),
                value.to_string(),
            );
        });

        // ---- 运行一个自动化 handler（参数来自界面填写的输入组件）----
        let ctx_run = ctx.clone();
        let engine_run = engine.clone();
        let ast_run = ast.clone(); // rhai::AST（owned）
        let ui_run = ui.as_weak();
        ui.on_run_automation(move |name| {
            let name_s = name.to_string();
            let args = collect_field_args(&ctx_run, &name_s);
            let (ok, msg) = run_automation(&*engine_run, &ast_run, &ctx_run, &name_s, args);
            if let Some(ui) = ui_run.upgrade() {
                ui.set_status_text(ss(format!("[自动化] {name}: {msg}")));
                ui.set_status_err(!ok);
                ui.set_automation_result(ss(msg));
                ui.set_automation_err(!ok);
                // 刷新运行日志
                refresh_logs_ui_inner(&ui, &ctx_run);
            }
        });

        // ---- 刷新运行日志（内存缓冲 + 加密落盘文件解密读取）----
        let ctx_logs = ctx.clone();
        let ui_logs = ui.as_weak();
        ui.on_refresh_logs(move || {
            refresh_logs_ui(&ui_logs, &ctx_logs);
        });

        // ---- 清空日志（内存 + 加密文件一并清除）----
        let ctx_clear_logs = ctx.clone();
        let ui_clear_logs = ui.as_weak();
        ui.on_clear_logs(move || {
            ctx_clear_logs.logs.lock().unwrap().clear();
            let _ = ctx_clear_logs.log_store.clear();
            if let Some(ui) = ui_clear_logs.upgrade() {
                refresh_logs_ui_inner(&ui, &ctx_clear_logs);
                ui.set_status_text(ss("运行日志已清空"));
                ui.set_status_err(false);
            }
        });

        // ---- 清空历史（带二次确认）----
        let ctx_clear = ctx.clone();
        let ui_clear = ui.as_weak();
        ui.on_clear_history(move || {
            ctx_clear.history.clear();
            if let Some(ui) = ui_clear.upgrade() {
                refresh_history_ui(&ui, &ctx_clear);
                ui.set_status_text(ss("发信历史已清空"));
                ui.set_status_err(false);
            }
        });

        // ---- 切换亮/暗主题（调用 Rhai apply_theme）----
        let ui_weak5 = ui.as_weak();
        let engine_theme = engine.clone();
        let ast_theme = ast.clone();
        let ctx_toggle = ctx.clone();
        ui.on_toggle_theme(move || {
            let new_dark = !*ctx_toggle.dark_mode.lock().unwrap();
            let mut scope = make_scope();
            let result: Result<(), Box<dyn std::error::Error>> = (|| {
                engine_theme.call_fn(&mut scope, &ast_theme, "apply_theme", (new_dark,))?;
                Ok(())
            })();
            if let Err(e) = result {
                if let Some(ui) = ui_weak5.upgrade() {
                    ui.set_status_text(ss(format!("主题切换失败: {e}")));
                    ui.set_status_err(true);
                }
                return;
            }
            if let Some(ui) = ui_weak5.upgrade() {
                apply_ui_state(&ui, &ctx_toggle);
            }
        });

        // ---- 窗口控制回调（无边框窗口，使用 Slint Window API）----
        let ui_min = ui.as_weak();
        ui.on_minimize(move || {
            if let Some(ui) = ui_min.upgrade() {
                ui.window().set_minimized(true);
            }
        });
        let ui_max = ui.as_weak();
        ui.on_toggle_maximize(move || {
            if let Some(ui) = ui_max.upgrade() {
                let w = ui.window();
                w.set_maximized(!w.is_maximized());
            }
        });
        ui.on_quit(|| { std::process::exit(0); });

        // ---- 窗口拖动（无边框下通过顶栏拖动）----
        let ui_drag = ui.as_weak();
        ui.on_drag_window(move |dx, dy| {
            if let Some(ui) = ui_drag.upgrade() {
                let w = ui.window();
                let pos = w.position();
                let sf = w.scale_factor() as f64;
                let np = slint::PhysicalPosition::new(
                    (pos.x as f64 + dx as f64 * sf) as i32,
                    (pos.y as f64 + dy as f64 * sf) as i32,
                );
                w.set_position(slint::WindowPosition::Physical(np));
            }
        });

        // ---- 高级面板开关 ----
        let ui_adv = ui.as_weak();
        ui.on_advanced(move || {
            if let Some(ui) = ui_adv.upgrade() {
                ui.set_advanced_open(!ui.get_advanced_open());
            }
        });

        // ---- 导航收起 / 专注模式（快捷键触发）----
        let ui_nav = ui.as_weak();
        ui.on_toggle_nav(move || {
            let new_v = !ui_nav.upgrade().map(|u| u.get_nav_collapsed()).unwrap_or(false);
            let mut c = AppConfig::load();
            c.nav_collapsed = new_v;
            let _ = c.save();
            if let Some(ui) = ui_nav.upgrade() {
                ui.set_nav_collapsed(new_v);
            }
        });

        let ui_zen = ui.as_weak();
        ui.on_toggle_zen(move || {
            let new_v = !ui_zen.upgrade().map(|u| u.get_zen_mode()).unwrap_or(false);
            let mut c = AppConfig::load();
            c.zen_mode = new_v;
            let _ = c.save();
            if let Some(ui) = ui_zen.upgrade() {
                ui.set_zen_mode(new_v);
            }
        });

        // ---- 设置页/高级面板 中持久化开关 ----
        let ui_nc2 = ui.as_weak();
        ui.on_set_nav_collapsed(move |v| {
            let mut c = AppConfig::load();
            c.nav_collapsed = v;
            let _ = c.save();
            if let Some(ui) = ui_nc2.upgrade() { ui.set_nav_collapsed(v); }
        });
        let ui_zn2 = ui.as_weak();
        ui.on_set_zen(move |v| {
            let mut c = AppConfig::load();
            c.zen_mode = v;
            let _ = c.save();
            if let Some(ui) = ui_zn2.upgrade() { ui.set_zen_mode(v); }
        });

        ui.run()?;
        Ok(())
}


/// 设置状态栏：文字 + 状态（0=进行中 1=成功 2=失败）+ 可选 toast 提示
///
/// 从任意线程安全地更新 UI：把更新闭包投递到 Slint 事件循环（主线程）执行。
///
/// Slint 要求所有属性修改发生在主线程。若在后台线程直接 `ui.set_xxx()`，
/// 界面通常**不会刷新**（表现为「一直卡在发送中」、状态永不变化），
/// 严重时可能 panic。因此发送线程里的一切 UI 更新都必须经此函数。
#[cfg(feature = "gui")]
fn with_ui<F: FnOnce(App) + Send + 'static>(weak: slint::Weak<App>, f: F) {
    if let Err(e) = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            f(ui);
        }
    }) {
        eprintln!("投递 UI 更新失败: {e}");
    }
}

/// `set_status_state` 的跨线程版本（供发送线程调用）。
#[cfg(feature = "gui")]
fn set_status_state_async(
    weak: &slint::Weak<App>,
    text: String,
    state: i32,
    toast: Option<String>,
) {
    with_ui(weak.clone(), move |ui| {
        set_status_state(&ui, &text, state, toast)
    });
}

/// 1 MB 的字节数（f64，用于进度换算）
#[cfg(feature = "gui")]
const MB_F: f64 = 1024.0 * 1024.0;

/// 字节数转人类可读大小（用于附件进度提示）
#[cfg(feature = "gui")]
fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// 进行中状态会**至少保持 0.2 秒**：操作即使瞬间完成，用户也能看到蓝色反馈，
/// 不会出现"点了没反应"的观感。实现：先置蓝，再调度一个 0.2s 后的延迟
/// 把状态改为最终结果（若期间又被置为新的进行中，则忽略旧调度）。
#[cfg(feature = "gui")]
fn set_status_state(
    ui: &App,
    text: &str,
    state: i32,
    toast: Option<String>,
) {
    ui.set_status_text(ss(text));
    ui.set_status_err(state == 2);
    if let Some(t) = toast {
        ui.set_toast_text(ss(t));
    }
    // 已经是最终状态（非进行中）则直接置位
    if state != 0 {
        ui.set_status_state(state);
        return;
    }
    // 进行中：立即置蓝，并保证最少 0.2s 才允许变为最终状态
    let weak = ui.as_weak();
    ui.set_status_state(0);
    let finish = move |final_state: i32| {
        if let Some(ui) = weak.upgrade() {
            // 若期间又开始了新的操作（又回到进行中），不覆盖
            if ui.get_status_state() == 0 {
                ui.set_status_state(final_state);
            }
        }
    };
    // 0.2s 后把"进行中"推进到最终态（由调用方在发送线程里调 set_status_state 最终态时本函数已用）
    let weak2 = ui.as_weak();
    slint::Timer::single_shot(std::time::Duration::from_millis(200), move || {
        if let Some(ui) = weak2.upgrade() {
            if ui.get_status_state() == 0 {
                ui.set_status_state(1);
            }
        }
        drop(finish);
    });
}

/// 清空发信表单（收件人/主题/正文/附件），并删除已保存草稿
#[cfg(feature = "gui")]
fn clear_compose_form(ui: &App) {
    ui.set_to_text(ss(""));
    ui.set_subject_text(ss(""));
    ui.set_body_text(ss(""));
    ui.set_body_mode(0);
    ui.set_attachments(ModelRc::from(Rc::new(VecModel::from(Vec::<slint::SharedString>::new()))));
    ui.set_attachment_names(ModelRc::from(Rc::new(VecModel::from(Vec::<slint::SharedString>::new()))));
    let _ = draft::Draft::default().clear();
}

/// 启动时恢复已保存草稿（若存在）
#[cfg(feature = "gui")]
fn restore_draft(ui: &App) {
    if let Some(d) = draft::Draft::load() {
        if d.is_empty() {
            return;
        }
        ui.set_to_text(ss(d.to));
        ui.set_subject_text(ss(d.subject));
        ui.set_body_text(ss(d.body));
        ui.set_body_mode(d.body_mode);
        let paths: Vec<slint::SharedString> =
            d.attachments.iter().map(|s| slint::SharedString::from(s.clone())).collect();
        let names: Vec<slint::SharedString> = d
            .attachments
            .iter()
            .filter_map(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| slint::SharedString::from(n.to_string_lossy().to_string()))
            })
            .collect();
        ui.set_attachments(ModelRc::from(Rc::new(VecModel::from(paths))));
        ui.set_attachment_names(ModelRc::from(Rc::new(VecModel::from(names))));
    }
}

/// 按正文模式决定最终发送内容与是否为 HTML。
///
/// - `0` = Markdown → 自动转为带内联样式的邮件友好 HTML（收件方看到排版，脚本按 HTML 发）
/// - `1` = HTML     → 原样发送
/// - 其他 = 纯文本  → 以 `text` 字段发送
///
/// 抽成独立函数而非内联在回调里，以便对这条发信链路做单元测试。
#[cfg(feature = "gui")]
fn resolve_body(body: &str, mode: i32) -> (String, bool) {
    match mode {
        0 => (crate::markdown::to_html(body), true),
        1 => (body.to_string(), true),
        _ => (body.to_string(), false),
    }
}

/// 最小限度转义 HTML 特殊字符，供纯文本安全地嵌入预览页
#[cfg(feature = "gui")]
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 用系统默认浏览器打开文件（跨平台）
#[cfg(feature = "gui")]
fn open_in_browser(path: &std::path::Path) -> std::io::Result<()> {
    let p = path.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        // `start` 需要一个空标题参数，否则含空格的路径会被误当作窗口标题
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &p])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&p).spawn()?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(&p).spawn()?;
    }
    Ok(())
}

/// 刷新历史记录到 UI 列表（显示全部永久保存的记录，最多 1000 条）
#[cfg(feature = "gui")]
fn refresh_history(ui: &App, ctx: &Arc<RhaiContext>) {
    refresh_history_ui(ui, ctx);
}

#[cfg(feature = "gui")]
fn refresh_history_ui(ui: &App, ctx: &Arc<RhaiContext>) {
    let hist = ctx.history.get_recent(1000);
    let items: Vec<slint::SharedString> = hist.iter().map(|h| {
        let status_icon = if h.status == "ok" { "✓" } else { "✗" };
        ss(format!("[{}] {} | {} | 主题:{} | {}", h.ts, status_icon, h.to, h.subject, h.detail))
    }).collect();
    let model = Rc::new(VecModel::from(items));
    ui.set_history_items(ModelRc::from(model));
    // 历史总数反映到 UI 属性
    ui.set_history_count(ctx.history.len() as i32);
}

/// 运行一个已注册的自动化 handler（供 GUI / CLI 调用）
/// 返回 (成功?, 返回值文本)
fn run_automation(
    engine: &rhai::Engine,
    ast: &rhai::AST,
    ctx: &Arc<RhaiContext>,
    name: &str,
    args: rhai::Array,
) -> (bool, String) {
    let fnptr = {
        let map = ctx.automations.lock().unwrap();
        match map.get(name) {
            Some(a) => a.handler.clone(),
            None => return (false, format!("未找到自动化 handler: {name}")),
        }
    };
    match fnptr.call::<rhai::Dynamic>(engine, ast, (args,)) {
        Ok(v) => (true, format!("{v}")),
        Err(e) => (false, format!("调用失败: {e}")),
    }
}

/// 把日志填充到 UI，并更新"已加密落盘"的状态与条数
#[cfg(feature = "gui")]
fn refresh_logs_ui_inner(ui: &App, ctx: &Arc<RhaiContext>) {
    let mut logs = ctx.logs.lock().unwrap().clone();
    // 合并磁盘上已加密的历史日志（去重：以内存缓冲为准）
    if let Ok(on_disk) = ctx.log_store.read_all() {
        for l in on_disk {
            if !logs.contains(&l) {
                logs.push(l);
            }
        }
    }
    let enc = ctx.log_store.has_content();
    let count = logs.len();
    let items: Vec<slint::SharedString> = logs.into_iter().map(slint::SharedString::from).collect();
    ui.set_log_lines(ModelRc::from(Rc::new(VecModel::from(items))));
    ui.set_logs_encrypted(enc);
    ui.set_log_count(count as i32);
}

#[cfg(feature = "gui")]
fn refresh_logs_ui(ui_weak: &slint::Weak<App>, ctx: &Arc<RhaiContext>) {
    if let Some(ui) = ui_weak.upgrade() {
        refresh_logs_ui_inner(&ui, ctx);
    }
}

/// 按 handler 的字段声明顺序，收集界面上填写好的值，作为 args 数组
fn collect_field_args(ctx: &Arc<RhaiContext>, name: &str) -> rhai::Array {
    let fields = {
        let map = ctx.automations.lock().unwrap();
        match map.get(name) {
            Some(a) => a.fields.clone(),
            None => Vec::new(),
        }
    };
    let values = ctx.field_values.lock().unwrap();
    fields
        .iter()
        .map(|f| {
            let raw = values
                .get(&(name.to_string(), f.key.clone()))
                .cloned()
                .unwrap_or_default();
            // bool 字段以 "true"/"false" 传入，转成布尔更便于脚本使用
            if f.kind == "bool" {
                rhai::Dynamic::from(raw == "true")
            } else {
                rhai::Dynamic::from(raw)
            }
        })
        .collect()
}

/// 把已注册的 handler 转成 Slint 结构模型（含脚本声明的输入组件与已填值）
#[cfg(feature = "gui")]
fn automation_items(ctx: &Arc<RhaiContext>) -> Vec<AutomationItem> {
    let map = ctx.automations.lock().unwrap();
    let values = ctx.field_values.lock().unwrap();
    map.iter()
        .map(|(k, v)| AutomationItem {
            name: k.clone().into(),
            desc: v.description.clone().into(),
            fields: ModelRc::from(Rc::new(VecModel::from(
                v.fields
                    .iter()
                    .map(|f| AutomationField {
                        key: f.key.clone().into(),
                        label: f.label.clone().into(),
                        kind: f.kind.clone().into(),
                        value: values
                            .get(&(k.clone(), f.key.clone()))
                            .cloned()
                            .unwrap_or_default()
                            .into(),
                    })
                    .collect::<Vec<_>>(),
            ))),
        })
        .collect()
}

/// 将脚本定义的 i18n / 主题 / 暗色 / 透明度 应用到 Slint 属性
#[cfg(feature = "gui")]
fn apply_ui_state(ui: &App, ctx: &Arc<RhaiContext>) {
    let i18n = ctx.i18n.lock().unwrap();
    let set = |ui: &App, k: &str, f: fn(&App, slint::SharedString)| {
        if let Some(v) = i18n.get(k) {
            f(ui, ss(v.clone()));
        }
    };
    set(ui, "t_send", |ui, v| ui.set_t_send(v));
    set(ui, "t_settings", |ui, v| ui.set_t_settings(v));
    set(ui, "t_history", |ui, v| ui.set_t_history(v));
    set(ui, "t_script", |ui, v| ui.set_t_script(v));
    set(ui, "t_about", |ui, v| ui.set_t_about(v));
    set(ui, "t_to", |ui, v| ui.set_t_to(v));
    set(ui, "t_subject", |ui, v| ui.set_t_subject(v));
    set(ui, "t_body", |ui, v| ui.set_t_body(v));
    set(ui, "t_from", |ui, v| ui.set_t_from(v));
    set(ui, "t_send_btn", |ui, v| ui.set_t_send_btn(v));
    set(ui, "t_api_key", |ui, v| ui.set_t_api_key(v));
    set(ui, "t_quota", |ui, v| ui.set_t_quota(v));
    set(ui, "t_remaining", |ui, v| ui.set_t_remaining(v));
    set(ui, "t_save", |ui, v| ui.set_t_save(v));
    set(ui, "t_unlock", |ui, v| ui.set_t_unlock(v));
    drop(i18n);

    let th = ui.global::<Theme>();
    let theme = ctx.theme.lock().unwrap();
    let col = |th: &Theme, k: &str, f: fn(&Theme, slint::Color)| {
        if let Some(v) = theme.get(k) {
            if let Ok(c) = parse_color(v) {
                f(th, c);
            }
        }
    };
    col(&th, "theme_bg", |th, c| th.set_bg(c));
    col(&th, "theme_surface", |th, c| th.set_surface(c));
    col(&th, "theme_fg", |th, c| th.set_fg(c));
    col(&th, "theme_muted", |th, c| th.set_muted(c));
    col(&th, "theme_accent", |th, c| th.set_accent(c));
    col(&th, "theme_accent_soft", |th, c| th.set_accent_soft(c));
    col(&th, "theme_on_accent", |th, c| th.set_on_accent(c));
    col(&th, "theme_border", |th, c| th.set_border(c));
    col(&th, "theme_field_bg", |th, c| th.set_field_bg(c));
    col(&th, "theme_field_fg", |th, c| th.set_field_fg(c));
    col(&th, "theme_field_border", |th, c| th.set_field_border(c));
    col(&th, "theme_sorg_left", |th, c| th.set_sorg_left(c));
    col(&th, "theme_sorg_right", |th, c| th.set_sorg_right(c));
    drop(theme);

    th.set_opacity(*ctx.opacity.lock().unwrap());
    ui.set_dark_mode(*ctx.dark_mode.lock().unwrap());
}

/// 解析 "#rrggbb" 为 Slint Color
#[cfg(feature = "gui")]
fn parse_color(s: &str) -> Result<slint::Color, std::num::ParseIntError> {
    let h = s.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16)?;
    let g = u8::from_str_radix(&h[2..4], 16)?;
    let b = u8::from_str_radix(&h[4..6], 16)?;
    Ok(slint::Color::from_rgb_u8(r, g, b))
}

/// 命令行调用功能（无 GUI）
/// 用法：resender send --to a@b.c --subject S --body B [--html] [--from F] [--api-key K] [--password P]
fn run_cli(args: &[String]) -> Result<()> {
    if args.first().map(|s| s.as_str()) == Some("send") {
        let mut to = String::new();
        let mut subject = String::new();
        let mut body = String::new();
        let mut from = String::new();
        let mut api_key = String::new();
        let mut password = String::new();
        let mut html = false;
        let mut attachments: Vec<String> = Vec::new();
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--to" => { to = args.get(i + 1).cloned().unwrap_or_default(); i += 2; }
                "--subject" => { subject = args.get(i + 1).cloned().unwrap_or_default(); i += 2; }
                "--body" => { body = args.get(i + 1).cloned().unwrap_or_default(); i += 2; }
                "--from" => { from = args.get(i + 1).cloned().unwrap_or_default(); i += 2; }
                "--api-key" => { api_key = args.get(i + 1).cloned().unwrap_or_default(); i += 2; }
                "--password" => { password = args.get(i + 1).cloned().unwrap_or_default(); i += 2; }
                "--html" => { html = true; i += 1; }
                // 附件：可重复传入多个 `--attach <路径>`
                "--attach" => { attachments.push(args.get(i + 1).cloned().unwrap_or_default()); i += 2; }
                _ => { i += 1; }
            }
        }
        if to.is_empty() || subject.is_empty() || body.is_empty() {
            eprintln!("用法: resender send --to <收件人> --subject <主题> --body <正文> [--html] [--from <发信名>] [--api-key <key>] [--password <解密密码>] [--attach <附件路径>]...");
            std::process::exit(2);
        }

        let (engine, ast, ctx, _sp) = setup()?;
        // 若命令行未给 api_key，则从 config 取
        let api_key = if api_key.is_empty() {
            let c = AppConfig::load();
            if c.api_key_enc {
                if password.is_empty() {
                    eprintln!("API Key 已加密，请用 --password 提供密码");
                    std::process::exit(3);
                }
                match crypto::decrypt_with_password(&c.api_key, &password) {
                    Ok(s) => s,
                    Err(e) => { eprintln!("解密失败: {e}"); std::process::exit(3); }
                }
            } else {
                c.api_key
            }
        } else {
            api_key
        };
        let from = if from.is_empty() {
            let c = AppConfig::load();
            if c.from_name_enc && !password.is_empty() {
                crypto::decrypt_with_password(&c.from_name, &password).unwrap_or_default()
            } else {
                c.from_name
            }
        } else {
            from
        };

        // 把 api_key / from 注入 store，让脚本 get_identity 能读到（脚本默认读 store）
        {
            let mut c = AppConfig::load();
            c.api_key = api_key.clone();
            c.api_key_enc = false;
            if !from.is_empty() { c.from_name = from.clone(); c.from_name_enc = false; }
            let _ = c.save();
        }
        *ctx.crypto_password.lock().unwrap() = password.clone();

        // 若启用了脚本信任，则尝试用提供的密码解锁（CLI 下跳过签名校验）
        let cfg_cli = AppConfig::load();
        if cfg_cli.script_trust_enabled {
            let r = rhai_ext::trust_primitives::unlock(
                &password,
                &std::fs::read_to_string(&_sp).unwrap_or_default(),
                "",
            );
            if !r.to_string().is_empty() {
                eprintln!("脚本信任解锁失败: {r}");
                std::process::exit(3);
            }
        }

        // 读取并编码附件（可多次 --attach）；失败立即报错，不静默跳过
        let mut attach_list: Vec<rhai::Dynamic> = Vec::new();
        for path_str in &attachments {
            let p = std::path::Path::new(path_str);
            match std::fs::read(p) {
                Ok(bytes) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    let filename = p
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let mut m = rhai::Map::new();
                    m.insert("filename".into(), filename.into());
                    m.insert("content".into(), b64.into());
                    attach_list.push(rhai::Dynamic::from(m));
                }
                Err(e) => {
                    eprintln!("读取附件失败 {path_str}: {e}");
                    std::process::exit(4);
                }
            }
        }

        let mut scope = make_scope();
        scope.push("crypto_password", password.clone());
        let result: Result<(), Box<dyn std::error::Error>> = (|| {
            engine.call_fn(&mut scope, &ast, "send_mail",
                (to.clone(), subject.clone(), body.clone(), html, password, attach_list.clone()))?;
            Ok(())
        })();
        if let Err(e) = result {
            eprintln!("发送失败（脚本错误）: {e}");
            std::process::exit(4);
        }
        let (status, is_err) = ctx.status.lock().unwrap().clone();
        if is_err {
            eprintln!("✗ {status}");
            std::process::exit(1);
        } else {
            println!("✓ {status}");
        }
        Ok(())
    } else if args.first().map(|s| s.as_str()) == Some("run") {
        // 运行一个已注册的自动化 handler：resender run <name> [arg1] [arg2] ...
        let name = args.get(1).cloned().unwrap_or_default();
        if name.is_empty() {
            eprintln!("用法: resender run <handler名称> [参数...]");
            std::process::exit(2);
        }
        let argv: Vec<String> = args.get(2..).unwrap_or(&[]).to_vec();
        let (engine, ast, ctx, _sp) = setup()?;
        let args_dyn: rhai::Array = argv.into_iter().map(|s| rhai::Dynamic::from(s)).collect();
        let (ok, msg) = run_automation(&*engine, &ast, &ctx, &name, args_dyn);
        if ok {
            println!("✓ {name}: {msg}");
            Ok(())
        } else {
            eprintln!("✗ {msg}");
            std::process::exit(1);
        }
    } else if matches!(args.first().map(|s| s.as_str()), Some("version") | Some("--version") | Some("-V")) {
        // 打印版本（与 GUI 关于页同源，均来自 Cargo.toml）
        println!("SWE::Resender {APP_VERSION}");
        Ok(())
    } else if args.first().map(|s| s.as_str()) == Some("check-update") {
        // 版本检查：读取本地配置的 VersionFile 地址，与当前版本比对
        let cfg = AppConfig::load();
        let url = cfg.update_url.clone();
        if url.is_empty() {
            eprintln!("未配置更新检查地址（设置 → update_url）");
            std::process::exit(2);
        }
        println!("当前版本: {APP_VERSION}");
        match crate::update::fetch_remote(&url) {
            Ok(rv) => {
                println!("远端版本: {}", rv.latest);
                if !rv.note.is_empty() {
                    println!("更新说明: {}", rv.note);
                }
                if crate::update::has_update(APP_VERSION, &rv) {
                    println!("发现新版本，可前往更新: {}", rv.url);
                } else {
                    println!("已是最新版本");
                }
            }
            Err(e) => {
                eprintln!("检查失败: {e}");
                std::process::exit(1);
            }
        }
        Ok(())
    } else {
        println!("Resender 命令行用法:");
        println!("  resender send --to <收件人> --subject <主题> --body <正文> [--html] [--from <发信名>] [--api-key <key>] [--password <解密密码>] [--attach <附件>]...");
        println!("  resender run <handler名称> [参数...]   运行脚本注册的自动化 handler");
        println!("  resender version | --version | -V      打印版本");
        println!("  resender check-update                   检查更新（需在设置中配置 update_url）");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_mode_markdown_converts_to_html() {
        let (body, is_html) = resolve_body("# t\n\n**b**", 0);
        assert!(is_html, "Markdown 模式应按 HTML 发送");
        assert!(body.contains("<h1"), "应转换为 HTML，got: {body}");
        assert!(!body.contains("# t"), "原文 Markdown 标记应已被解析掉");
    }

    #[test]
    fn body_mode_html_passes_through() {
        let src = "<b>bold</b>";
        let (body, is_html) = resolve_body(src, 1);
        assert!(is_html, "HTML 模式应按 HTML 发送");
        assert_eq!(body, src, "HTML 模式应原样发送，不做转换");
    }

    #[test]
    fn body_mode_text_is_plain() {
        let src = "plain text";
        let (body, is_html) = resolve_body(src, 2);
        assert!(!is_html, "纯文本模式应按 text 字段发送");
        assert_eq!(body, src, "纯文本模式应原样发送");
    }

    #[test]
    fn html_escape_neutralizes_tags() {
        assert_eq!(html_escape("<script>&"), "&lt;script&gt;&amp;");
    }
}
