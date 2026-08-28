// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//! 发信草稿：收件人 / 主题 / 正文 / 正文模式 / 附件列表。
//!
//! 独立于 config.json 存为 `draft.json`：草稿是会话状态而非配置，
//! 且恢复时不需要覆盖用户其他设置。

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Draft {
    pub to: String,
    pub subject: String,
    pub body: String,
    /// 正文模式：0=Markdown 1=HTML 2=纯文本
    pub body_mode: i32,
    /// 附件文件路径（供下次发送重新读取）
    pub attachments: Vec<String>,
}

impl Draft {
    pub fn path() -> Result<PathBuf> {
        let mut dir = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("无法定位配置目录"))?;
        dir.push("resender");
        fs::create_dir_all(&dir)?;
        dir.push("draft.json");
        Ok(dir)
    }

    pub fn load() -> Option<Draft> {
        Self::path()
            .ok()
            .filter(|p| p.exists())
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn save(&self) -> Result<()> {
        let p = Self::path()?;
        fs::write(p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let p = Self::path()?;
        if p.exists() {
            fs::remove_file(p)?;
        }
        Ok(())
    }

    /// 是否为空草稿（没有内容需要保存）
    pub fn is_empty(&self) -> bool {
        self.to.trim().is_empty()
            && self.subject.trim().is_empty()
            && self.body.trim().is_empty()
            && self.attachments.is_empty()
    }
}
