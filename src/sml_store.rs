// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//![allow(non_snake_case)]
//! 通用 SML 持久化层。
//!
//! resender 的结构化数据（配置 / 草稿 / 历史）统一以 SML 落盘，
//! 取代原先的 JSON。SML 相比 JSON 的优势（对齐项目「大规模使用 SML」的方向）：
//! - 引号可选、块冒号可省，人工编辑更友好
//! - `#` 行注释，可直接给配置项写说明
//! - `include` 指令：配置可拆分多文件（如 `config.sml` include `conf.d/*.sml`）
//! - `$env.VAR` 内联：API Key 等敏感项可指向环境变量而不落盘明文
//! - **契约**：可选 schema 层，在解析期校验字段类型、补齐默认值
//! - 与 Soup 生态的 `lib/sml.soup` 语法一致，跨语言可读
//!
//! # 向后兼容（关键）
//!
//! 老版本落盘的是 `*.json`。迁移策略：
//! - 读：优先读 `.sml`；不存在则读同名的旧 `.json`，成功解析后**自动改写为 `.sml`**
//! - 写：只写 `.sml`
//! 因此用户升级后旧数据不会丢失，且一次性完成迁移。

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value as JValue;
use sml::Value as SValue;
use std::fs;
use std::path::Path;

/// 任意可序列化的值 → SML 文本。
///
/// 走 serde 中转：`T` → `serde_json::Value` → `sml::Value` → `sml::to_sml`。
/// 之所以经过 JSON：SML 的 `Value` 实现了 serde 的 `Serialize`/`Deserialize`，
/// 借助 serde_json 即可复用现有的 `#[derive(Serialize)]` 结构体，无需手写映射。
pub fn to_sml_text<T: Serialize>(value: &T) -> Result<String> {
    let jv: JValue = serde_json::to_value(value).context("序列化为 JSON 中间值失败")?;
    let sv: SValue = serde_json::from_value(jv).context("JSON 中间值转 SML 值失败")?;
    Ok(sml::to_sml(&sv))
}

/// SML 文本 → 任意可反序列化的值。
///
/// 契约（若文本中含 `@contract` / `@is`）由 SML 解析器在解析期自动应用，
/// 因此这里拿到的已是校验并补齐默认值之后的数据。
pub fn from_sml_text<T: DeserializeOwned>(text: &str) -> Result<T> {
    let sv: SValue = sml::parse(text).map_err(|e| anyhow::anyhow!("SML 解析失败: {e}"))?;
    let jv: JValue = serde_json::to_value(&sv).context("SML 值转 JSON 中间值失败")?;
    serde_json::from_value(jv).context("SML 内容反序列化为目标类型失败")
}

/// 从 SML 文件读取并反序列化（含契约校验）。
///
/// 返回 `Ok(None)` 表示文件不存在。解析或契约校验失败返回 `Err`，
/// 由调用方决定告警/降级策略（不建议静默吞掉：那会掩盖配置被改坏的事实）。
pub fn from_sml_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).context("读取 SML 文件失败")?;
    from_sml_text::<T>(&text).map(Some)
}

/// 保存为 SML 文件（原子写：先写临时文件再重命名，避免中断导致文件损坏）。
pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = to_sml_text(value)?;
    save_text(path, &text)
}

/// 直接保存已生成的 SML 文本（原子写）。
///
/// 供需要在数据之外前置内容（如 `@contract` 契约声明 + `@is`）的调用方使用。
pub fn save_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("sml.tmp");
    fs::write(&tmp, text).context("写入临时文件失败")?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // 重命名失败（如跨设备）时退化为直接写，不丢数据
            let _ = fs::remove_file(&tmp);
            fs::write(path, text).map_err(|e2| anyhow::anyhow!("保存失败: {e} / {e2}"))
        }
    }
}

/// 载入 SML 文件；若不存在则回退读取旧 JSON 文件并自动迁移为 SML。
///
/// 返回 `Ok(None)` 表示两个文件都不存在（首次运行）。
pub fn load_migrating<T>(sml_path: &Path, legacy_json_path: &Path) -> Result<Option<T>>
where
    T: DeserializeOwned + Serialize,
{
    if sml_path.exists() {
        return from_sml_file::<T>(sml_path);
    }

    if legacy_json_path.exists() {
        let text = fs::read_to_string(legacy_json_path).context("读取旧 JSON 文件失败")?;
        let value: T = serde_json::from_str(&text).context("旧 JSON 解析失败")?;
        // 一次性迁移：写 SML，成功后删除旧 JSON
        if save(sml_path, &value).is_ok() {
            let _ = fs::remove_file(legacy_json_path);
        }
        return Ok(Some(value));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Sample {
        name: String,
        count: i64,
        enabled: bool,
        ratio: f64,
        items: Vec<String>,
    }

    fn sample() -> Sample {
        Sample {
            // 含空格与尖括号：验证 quote_if_needed 能正确加引号
            name: "sal <sal@mail.swebase.cn>".into(),
            count: 42,
            enabled: true,
            ratio: 1.5,
            items: vec!["a".into(), "b c".into()],
        }
    }

    #[test]
    fn roundtrip_scalar_fields() {
        let s = sample();
        let text = to_sml_text(&s).unwrap();
        let back: Sample = from_sml_text(&text).unwrap();
        assert_eq!(back, s, "含空格/尖括号的字符串必须能原样往返");
    }

    #[test]
    fn roundtrip_array_of_objects() {
        // 历史记录是「对象数组」，必须验证顶层数组的往返
        let v = vec![sample(), sample()];
        let text = to_sml_text(&v).unwrap();
        let back: Vec<Sample> = from_sml_text(&text).unwrap();
        assert_eq!(back.len(), 2, "数组元素数量应保持");
        assert_eq!(back[0], v[0], "数组元素内容应一致");
    }

    #[test]
    fn roundtrip_empty_and_special_strings() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct S {
            empty: String,
            with_hash: String,
            with_colon: String,
            windows_path: String,
            multiline: String,
        }
        let s = S {
            empty: String::new(),
            with_hash: "a#b".into(),
            with_colon: "C:\\Users\\x".into(),
            windows_path: "D:/data/file.txt".into(),
            multiline: "line1\nline2".into(),
        };
        let text = to_sml_text(&s).unwrap();
        let back: S = from_sml_text(&text).unwrap();
        assert_eq!(back, s, "空串/#/冒号/路径/换行都必须安全往返");
    }

    #[test]
    fn load_migrating_reads_legacy_json_and_rewrites_sml() {
        let dir = std::env::temp_dir().join("resender_sml_store_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let sml_path = dir.join("x.sml");
        let json_path = dir.join("x.json");

        let s = sample();
        fs::write(&json_path, serde_json::to_string(&s).unwrap()).unwrap();

        let loaded: Option<Sample> = load_migrating(&sml_path, &json_path).unwrap();
        assert_eq!(loaded.unwrap(), s, "应能从旧 JSON 读回数据");
        assert!(sml_path.exists(), "读取旧 JSON 后应自动写出 SML");
        assert!(!json_path.exists(), "迁移成功后应删除旧 JSON");

        let _ = fs::remove_dir_all(&dir);
    }

    // —— 契约（Contract）在 resender 配置中的应用 ——
    // 关键保障：契约不得让「原本能读的配置」读不出来，否则用户配置会静默丢失。

    /// 用配置契约解析一段 SML（结构与 AppConfig::save 写出的一致）
    fn parse_with_contract(body: &str) -> Result<crate::config::AppConfig> {
        let text = format!(
            "{}@is ResenderConfig\n{}",
            crate::config::CONFIG_CONTRACT,
            body
        );
        from_sml_text::<crate::config::AppConfig>(&text)
    }

    #[test]
    fn contract_accepts_existing_style_config() {
        // 现有 config.sml 的形态（空串、裸词 off、含 - 的字符串）必须被接受
        let body = concat!(
            "api_key: re_abc\n",
            "api_key_enc: false\n",
            "cycle_mark: -2686-07-01\n",
            "cycle_start: \"\"\n",
            "from_name: sal@mail.swebase.cn\n",
            "from_name_enc: false\n",
            "keep_after_send: true\n",
            "month_count: 6\n",
            "nav_collapsed: false\n",
            "plan_index: 0\n",
            "script_sig_verify: off\n",
            "script_trust_enabled: false\n",
            "total_count: 6\n",
            "update_url: \"\"\n",
            "zen_mode: false\n",
        );
        let c = parse_with_contract(body).expect("既有配置形态必须能通过契约校验");
        assert_eq!(c.api_key, "re_abc");
        assert_eq!(c.month_count, 6);
        assert_eq!(c.script_sig_verify, "off");
        assert!(c.keep_after_send);
    }

    #[test]
    fn contract_fills_missing_fields_with_defaults() {
        // 手写配置只写关心的字段，其余由契约 default 补齐
        let c = parse_with_contract("api_key: re_x\n").expect("缺失字段应由 default 补齐");
        assert_eq!(c.api_key, "re_x");
        assert_eq!(c.from_name, "");
        assert_eq!(c.plan_index, 0);
        assert!(!c.api_key_enc);
    }

    #[test]
    fn contract_rejects_wrong_type() {
        // plan_index 声明为 int，给字符串应报错（而不是静默变成 0）
        let e = parse_with_contract("plan_index: abc\n").unwrap_err();
        let msg = format!("{e}");
        assert!(msg.contains("plan_index"), "错误信息应指出字段，got: {msg}");
    }

    #[test]
    fn contract_allows_undeclared_field_because_loose() {
        // loose：允许未来新增字段，旧配置不会因未知字段被拒
        let c =
            parse_with_contract("some_future_field: 1\n").expect("loose 契约应允许未声明字段");
        assert_eq!(c.api_key, "");
    }

    #[test]
    fn config_roundtrip_with_contract() {
        let mut c = crate::config::AppConfig::default();
        c.api_key = "re_round".into();
        c.plan_index = 2;
        c.keep_after_send = true;
        let text = format!(
            "{}@is ResenderConfig\n{}",
            crate::config::CONFIG_CONTRACT,
            to_sml_text(&c).unwrap()
        );
        let back: crate::config::AppConfig = from_sml_text(&text).unwrap();
        assert_eq!(back.api_key, "re_round");
        assert_eq!(back.plan_index, 2);
        assert!(back.keep_after_send);
    }
}
