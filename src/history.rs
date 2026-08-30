// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//![allow(non_snake_case)]
//! 发信历史记录持久化（本地 JSON 文件，按时间倒序）

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value as JValue;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// ISO 时间戳
    pub ts: String,
    /// 收件人（逗号分隔）
    pub to: String,
    /// 主题
    pub subject: String,
    /// 发信名称
    pub from: String,
    /// 状态：ok / fail
    pub status: String,
    /// 详情（邮件 ID 或错误）
    pub detail: String,
}

impl HistoryEntry {
    pub fn to_json(&self) -> JValue {
        serde_json::json!({
            "ts": self.ts,
            "to": self.to,
            "subject": self.subject,
            "from": self.from,
            "status": self.status,
            "detail": self.detail,
        })
    }
}

pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    /// 当前历史路径（SML）。
    pub fn new() -> Result<Self> {
        let mut dir = Self::dir()?;
        dir.push("history.sml");
        Ok(Self { path: dir })
    }

    /// 旧版历史路径（JSON），仅用于一次性迁移。
    fn legacy_path(&self) -> Option<PathBuf> {
        Self::dir().ok().map(|mut d| {
            d.push("history.json");
            d
        })
    }

    fn dir() -> Result<PathBuf> {
        let mut dir = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("无法定位配置目录"))?;
        dir.push("resender");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn read_all(&self) -> Vec<HistoryEntry> {
        // 优先 SML；回退旧 JSON 并自动迁移
        match self.legacy_path() {
            Some(json_p) => crate::sml_store::load_migrating::<Vec<HistoryEntry>>(
                &self.path,
                &json_p,
            )
            .ok()
            .flatten()
            .unwrap_or_default(),
            None => Vec::new(),
        }
    }

    fn write_all(&self, entries: &[HistoryEntry]) {
        // 历史是对象数组，SML 顶层数组由 dump_inline 输出（单行内联）。
        // 这里改用「带键的块」形式更好读，但会改变结构，故保持数组语义不变。
        let _ = crate::sml_store::save(&self.path, &entries);
    }

    /// 追加一条记录（保留最多 1000 条，最新在前）
    pub fn append(&self, entry: &HistoryEntry) {
        let mut all = self.read_all();
        all.insert(0, entry.clone());
        if all.len() > 1000 {
            all.truncate(1000);
        }
        self.write_all(&all);
    }

    /// 取最近 limit 条
    pub fn get_recent(&self, limit: usize) -> Vec<HistoryEntry> {
        let all = self.read_all();
        all.into_iter().take(limit.max(1)).collect()
    }

    /// 清空
    pub fn clear(&self) {
        self.write_all(&[]);
    }

    /// 总条数
    pub fn len(&self) -> usize {
        self.read_all().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
