// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//![allow(non_snake_case)]
//! 运行日志持久化（**默认加密落盘**）
//!
//! 日志可能包含收件人邮箱等敏感信息，因此**绝不落盘明文**。
//!
//! 密钥策略：首次运行时在配置目录生成随机本地密钥 `logkey.bin`（32 字节），
//! 之后一直复用。因此：
//! - **不依赖用户设置加密密码**：开箱即用，日志默认即为密文
//! - 日志不跨机器迁移（密钥留在本机，符合"本地日志"定位）
//! - 文件每行格式为 `ct_b64|nonce_b64|salt_b64`（与 crypto 模块密文格式一致），
//!   每条独立随机 salt/nonce
//!
//! 若历史行无法用当前密钥解密（密钥被删除/替换），保留占位提示而非静默丢弃。

use anyhow::Result;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::crypto;

/// 无法解密的历史日志占位提示
const UNREADABLE: &str = "（一条日志无法解密：本地密钥已变更或数据损坏）";

/// 单文件最多保留的日志条数
const MAX_LINES: usize = 2000;

pub struct LogStore {
    path: PathBuf,
    /// 本地日志密钥（hex 编码的随机字节），作为 KDF 口令使用
    key: String,
}

impl LogStore {
    pub fn new() -> Result<Self> {
        let dir = Self::config_dir()?;
        Ok(Self {
            path: dir.join("logs.enc"),
            key: load_or_create_key(&dir)?,
        })
    }

    fn config_dir() -> Result<PathBuf> {
        let mut dir = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("无法定位配置目录"))?;
        dir.push("resender");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 供测试使用：在临时目录下创建独立实例，避免污染真实配置目录
    #[cfg(test)]
    pub fn for_test(dir_name: &str) -> Self {
        let mut dir = std::env::temp_dir();
        dir.push(dir_name);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self {
            path: dir.join("logs.enc"),
            key: load_or_create_key(&dir).expect("create test key"),
        }
    }

    /// 追加一条日志（加密后落盘）
    pub fn append(&self, line: &str) -> Result<()> {
        let payload = crypto::encrypt_with_password(line, &self.key)?;
        {
            let mut f = OpenOptions::new().create(true).append(true).open(&self.path)?;
            writeln!(f, "{payload}")?;
        }
        self.trim_if_needed()?;
        Ok(())
    }

    /// 读取并解密全部日志；无法解密的行以占位提示代替
    pub fn read_all(&self) -> Result<Vec<String>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let f = File::open(&self.path)?;
        let mut out = Vec::new();
        for line in BufReader::new(f).lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match crypto::decrypt_with_password(line, &self.key) {
                Ok(s) => out.push(s),
                Err(_) => out.push(UNREADABLE.to_string()),
            }
        }
        Ok(out)
    }

    /// 清空日志文件（保留本地密钥，避免历史日志永久无法读取）
    pub fn clear(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    pub fn has_content(&self) -> bool {
        self.path.metadata().map(|m| m.len() > 0).unwrap_or(false)
    }

    /// 超过上限时截断最早的条目（留 20% 余量，避免频繁重写）
    fn trim_if_needed(&self) -> Result<()> {
        if !self.path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(&self.path)?;
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.len() <= MAX_LINES + MAX_LINES / 5 {
            return Ok(());
        }
        let keep = &lines[lines.len() - MAX_LINES..];
        let mut f = OpenOptions::new().write(true).truncate(true).open(&self.path)?;
        f.seek(SeekFrom::Start(0))?;
        for l in keep {
            writeln!(f, "{l}")?;
        }
        Ok(())
    }
}

/// 读取本地日志密钥；不存在则用 OS 随机源生成并保存
fn load_or_create_key(dir: &Path) -> Result<String> {
    let key_path = dir.join("logkey.bin");
    if let Ok(existing) = std::fs::read_to_string(&key_path) {
        let k = existing.trim().to_string();
        if !k.is_empty() {
            return Ok(k);
        }
    }
    let mut buf = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut buf);
    let key: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(&key_path, &key)?;
    Ok(key)
}
