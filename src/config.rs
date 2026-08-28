// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//![allow(non_snake_case)]
//! 配置持久化（本地 JSON）

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
        }
    }
}

impl AppConfig {
    pub fn path() -> Result<PathBuf> {
        let mut dir = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("无法定位配置目录"))?;
        dir.push("resender");
        fs::create_dir_all(&dir)?;
        dir.push("config.json");
        Ok(dir)
    }

    pub fn load() -> AppConfig {
        match Self::path() {
            Ok(p) if p.exists() => fs::read_to_string(p)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            _ => AppConfig::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let p = Self::path()?;
        fs::write(p, serde_json::to_string_pretty(self)?)?;
        Ok(())
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
