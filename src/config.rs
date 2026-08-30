// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//![allow(non_snake_case)]
//! 配置持久化（本地 SML）
//!
//! 配置以 SML 落盘，并**应用 SML 契约**做字段校验（见 [`CONFIG_CONTRACT`]）。
//! 契约在解析期校验字段类型、填充缺失字段的默认值，避免配置文件被手改坏后
//! 到运行期才暴露问题。

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// API Key：明文（未勾选加密时）或密文
    pub api_key: String,
    pub api_key_enc: bool,

    /// 固定发信名称：明文或密文
    pub from_name: String,
    pub from_name_enc: bool,

    /// 套餐索引
    pub plan_index: usize,
    /// 自定义月度额度
    pub custom_quota: i64,
    /// 统计周期起点 YYYY-MM-DD
    pub cycle_start: String,

    /// 累计已发
    pub total_count: i64,
    /// 本期已开始日期 YYYY-MM-DD
    pub cycle_mark: String,
    /// 本期已发
    pub month_count: i64,

    /// 左侧导航栏是否收起为纯图标（隐藏文字）
    pub nav_collapsed: bool,
    /// 专注模式（禅模式）：隐藏顶栏+导航+状态栏，仅留内容
    pub zen_mode: bool,

    /// —— Rhai 脚本信任机制 ——
    /// 功能默认禁用；启用需用户设置并输入密码（单项 KDF 体系复用加密密码）
    pub script_trust_enabled: bool,
    /// 启用信任所需的密码（不落盘明文，仅内存校验；此处为占位，实际由 UI 解锁时比对）
    pub script_trust_password: String,
    /// 签名校验模式："off" | "sm2" | "pq"（后量子）
    pub script_sig_verify: String,
    /// 验证用公钥（hex 编码；sm2 为 65 字节 04||x||y，pq 为对应算法公钥）
    pub script_pubkey: String,

    /// 发送成功后是否保留表单内容（true 保留；false 自动清空）
    pub keep_after_send: bool,

    /// 更新检查的 VersionFile 地址（URL），为空则不检查
    pub update_url: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            api_key_enc: false,
            from_name: String::new(),
            from_name_enc: false,
            plan_index: 0,
            custom_quota: 0,
            cycle_start: String::new(),
            total_count: 0,
            cycle_mark: String::new(),
            month_count: 0,
            // 导航展开；非专注模式
            nav_collapsed: false,
            zen_mode: false,
            // 脚本信任机制：默认全部禁用（默认不信任、不校验签名）
            script_trust_enabled: false,
            script_trust_password: String::new(),
            script_sig_verify: "off".to_string(),
            script_pubkey: String::new(),
            // 发送成功后默认清空表单
            keep_after_send: false,
            update_url: String::new(),
        }
    }
}

/// 配置契约（SML）。
///
/// 定义每个字段的类型与默认值，由 SML 解析器在读取 config.sml 时自动应用：
/// - 类型不符（如 `plan_index: abc`）会在**读取时**报错，而不是到运行期才炸
/// - 缺失字段自动填 `default`，因此手写配置只写关心的字段即可
/// - `loose`：允许契约未声明的字段。这是**刻意**选择——未来新增配置项后，
///   旧版配置文件不应因为含未知字段而被拒绝（否则用户配置会静默丢失）
pub(crate) const CONFIG_CONTRACT: &str = r#"@contract ResenderConfig loose {
    api_key: str default ""
    api_key_enc: bool default false
    from_name: str default ""
    from_name_enc: bool default false
    plan_index: int min 0 default 0
    custom_quota: int default 0
    cycle_start: str default ""
    cycle_mark: str default ""
    total_count: int default 0
    month_count: int default 0
    nav_collapsed: bool default false
    zen_mode: bool default false
    script_trust_enabled: bool default false
    script_trust_password: str default ""
    script_sig_verify: str default "off"
    script_pubkey: str default ""
    keep_after_send: bool default false
    update_url: str default ""
}
"#;

impl AppConfig {
    /// 当前配置路径（SML）。
    pub fn path() -> Result<PathBuf> {
        let mut dir = Self::dir()?;
        dir.push("config.sml");
        Ok(dir)
    }

    /// 旧版配置路径（JSON），仅用于一次性迁移。
    fn legacy_path() -> Result<PathBuf> {
        let mut dir = Self::dir()?;
        dir.push("config.json");
        Ok(dir)
    }

    fn dir() -> Result<PathBuf> {
        let mut dir = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("无法定位配置目录"))?;
        dir.push("resender");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// 载入配置。
    ///
    /// 优先读 `config.sml`；不存在则读旧 `config.json` 并自动迁移为 SML。
    /// 若 config.sml 存在但解析/校验失败（含契约校验不通过），降级为默认配置
    /// 以保证程序可启动——但会**显式告警**，因为这意味着用户配置未被加载：
    /// 静默降级会让用户以为配置生效而实际丢失。
    pub fn load() -> AppConfig {
        let (sml_p, json_p) = match (Self::path(), Self::legacy_path()) {
            (Ok(a), Ok(b)) => (a, b),
            _ => return AppConfig::default(),
        };
        if !sml_p.exists() {
            // 无 SML：读旧 JSON 并迁移（迁移失败无需告警，本就无 SML 配置）
            return crate::sml_store::load_migrating::<AppConfig>(&sml_p, &json_p)
                .ok()
                .flatten()
                .unwrap_or_default();
        }
        match crate::sml_store::from_sml_file::<AppConfig>(&sml_p) {
            Ok(Some(c)) => c,
            Ok(None) => AppConfig::default(),
            Err(e) => {
                eprintln!(
                    "[警告] 读取配置失败，已回退为默认配置（用户设置未生效）: {}\n\
                     \x20 配置文件: {}\n\
                     \x20 请检查该文件是否被手工改坏（契约校验会拒绝类型不符的字段）",
                    e,
                    sml_p.display()
                );
                AppConfig::default()
            }
        }
    }

    /// 保存为 SML（原子写），并前置契约声明。
    ///
    /// 写出的文件形如：
    /// ```sml
    /// @contract ResenderConfig loose { ... }
    /// @is ResenderConfig
    /// api_key: ...
    /// ```
    /// 因此下次读取时会自动按契约校验类型并补齐缺失字段。
    pub fn save(&self) -> Result<()> {
        let p = Self::path()?;
        let body = crate::sml_store::to_sml_text(self)?;
        // 顶层写 `@is` 即可对整份配置应用契约（契约定义不进解析结果）
        let text = format!("{}@is ResenderConfig\n{}", CONFIG_CONTRACT, body);
        crate::sml_store::save_text(&p, &text)
    }
}

/// 套餐定义（名称 + 月度额度），最后一项为「自定义」
pub const PLANS: &[(&str, i64)] = &[
    ("Free (3,000/月)", 3000),
    ("Pro (50,000/月)", 50000),
    ("Scale (100,000/月)", 100000),
    ("自定义", 0),
];

pub fn compute_quota(plan_index: usize, custom: i64) -> i64 {
    if plan_index >= PLANS.len() - 1 {
        if custom > 0 { custom } else { 0 }
    } else {
        PLANS.get(plan_index).map(|(_, q)| *q).unwrap_or(0)
    }
}

/// Rata Die 日期算法
pub fn gregorian_std(rd: i64) -> (i32, u32, u32) {
    let a = rd + 32044;
    let b = (4 * a + 3) / 146097;
    let c = a - (146097 * b) / 4;
    let d = (4 * c + 3) / 1461;
    let e = c - (1461 * d) / 4;
    let m = (5 * e + 2) / 153;
    let day = (e - (153 * m + 2) / 5 + 1) as u32;
    let month = if m < 10 { (m + 3) as u32 } else { (m - 9) as u32 };
    let year = (100 * b + d - 4800 + (if m < 10 { 1 } else { 0 })) as i32;
    (year, month, day)
}
