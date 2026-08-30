// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//! 版本检查：编译时记录自身版本（APP_VERSION），与仓库 VersionFile 比对。
//!
//! VersionFile 规范（SML，`@version v1`，放在仓库根或发行页）：
//!
//! ```sml
//! @version v1
//! project: resender
//! latest: "0.2.0"
//! min: "0.1.0"          # 最低可用版本（可选）
//! note: "新增 Markdown 正文"
//! url: "https://gitee.com/snoware/resender/releases"
//! ```
//!
//! 版本比较为语义化三段（`主.次.补丁`），按数值逐段比较；
//! 解析失败的段视为 0，保证任意版本串都能比较不 panic。

use anyhow::{Context, Result};

/// 远端版本信息
#[derive(Debug, Clone, Default)]
pub struct RemoteVersion {
    /// 项目名：VersionFile 契约字段，当前仅作标识，不参与版本比较
    #[allow(dead_code)]
    pub project: String,
    pub latest: String,
    /// 最低支持版本：契约字段，暂未用于强制升级判定
    #[allow(dead_code)]
    pub min: String,
    pub note: String,
    pub url: String,
}

/// 解析语义化版本为数值段。非数字段按 0 处理。
pub fn parse_version(s: &str) -> Vec<u64> {
    s.split('.')
        .map(|seg| seg.trim().parse::<u64>().unwrap_or(0))
        .collect()
}

/// 语义化版本比较：a > b 返回 Ordering
pub fn cmp_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let va = parse_version(a);
    let vb = parse_version(b);
    for i in 0..va.len().max(vb.len()) {
        let x = va.get(i).copied().unwrap_or(0);
        let y = vb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            std::cmp::Ordering::Equal => continue,
            ord => return ord,
        }
    }
    std::cmp::Ordering::Equal
}

/// 拉取并解析 VersionFile（SML）
pub fn fetch_remote(url: &str) -> Result<RemoteVersion> {
    let resp = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("构建 HTTP 客户端失败")?
        .get(url)
        .send()
        .context("获取 VersionFile 失败")?;
    let text = resp.text().context("读取 VersionFile 失败")?;
    parse_version_file(&text)
}

/// 解析 VersionFile SML 文本
pub fn parse_version_file(text: &str) -> Result<RemoteVersion> {
    let (v, _) = sml::parse_versioned(text).map_err(|e| anyhow::anyhow!("VersionFile 解析失败: {e}"))?;
    let get = |k: &str| -> String {
        v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string()
    };
    Ok(RemoteVersion {
        project: get("project"),
        latest: get("latest"),
        min: get("min"),
        note: get("note"),
        url: get("url"),
    })
}

/// 是否有新版本（latest > 当前）
pub fn has_update(current: &str, remote: &RemoteVersion) -> bool {
    cmp_versions(&remote.latest, current) == std::cmp::Ordering::Greater
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing_and_comparison() {
        assert_eq!(cmp_versions("0.1.0", "0.1.0"), std::cmp::Ordering::Equal);
        assert_eq!(cmp_versions("0.2.0", "0.1.0"), std::cmp::Ordering::Greater);
        assert_eq!(cmp_versions("0.1.0", "0.1.1"), std::cmp::Ordering::Less);
        assert_eq!(cmp_versions("1.0.0", "0.9.9"), std::cmp::Ordering::Greater);
        // 非数字段容错
        assert_eq!(cmp_versions("0.1.x", "0.1.0"), std::cmp::Ordering::Equal);
        // 段数不等
        assert_eq!(cmp_versions("1.0", "1.0.0"), std::cmp::Ordering::Equal);
    }

    #[test]
    fn parse_version_file_reads_fields() {
        let text = r#"@version v1
project: resender
latest: "0.2.0"
min: "0.1.0"
note: "修复若干问题"
url: "https://gitee.com/snoware/resender/releases"
"#;
        let rv = parse_version_file(text).unwrap();
        assert_eq!(rv.project, "resender");
        assert_eq!(rv.latest, "0.2.0");
        assert_eq!(rv.min, "0.1.0");
        assert_eq!(rv.note, "修复若干问题");
        assert_eq!(rv.url, "https://gitee.com/snoware/resender/releases");
    }

    #[test]
    fn has_update_detects_newer() {
        let rv = RemoteVersion {
            latest: "0.2.0".into(),
            ..Default::default()
        };
        assert!(has_update("0.1.0", &rv));
        assert!(!has_update("0.2.0", &rv));
        assert!(!has_update("0.3.0", &rv));
    }
}
