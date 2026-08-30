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
    /// 当前草稿路径（SML）。
    pub fn path() -> Result<PathBuf> {
        let mut dir = Self::dir()?;
        dir.push("draft.sml");
        Ok(dir)
    }

    /// 旧版草稿路径（JSON），仅用于一次性迁移。
    fn legacy_path() -> Result<PathBuf> {
        let mut dir = Self::dir()?;
        dir.push("draft.json");
        Ok(dir)
    }

    fn dir() -> Result<PathBuf> {
        let mut dir = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("无法定位配置目录"))?;
        dir.push("resender");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// 载入草稿：优先 SML，回退旧 JSON 并自动迁移。
    pub fn load() -> Option<Draft> {
        let (sml_p, json_p) = (Self::path().ok()?, Self::legacy_path().ok()?);
        crate::sml_store::load_migrating::<Draft>(&sml_p, &json_p)
            .ok()
            .flatten()
    }

    /// 保存为 SML（原子写）。
    pub fn save(&self) -> Result<()> {
        let p = Self::path()?;
        crate::sml_store::save(&p, self)
    }

    /// 删除草稿（SML 与遗留 JSON 一并清理，避免旧文件复活）。
    pub fn clear(&self) -> Result<()> {
        for p in [Self::path().ok(), Self::legacy_path().ok()] {
            if let Some(p) = p {
                if p.exists() {
                    fs::remove_file(p)?;
                }
            }
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
