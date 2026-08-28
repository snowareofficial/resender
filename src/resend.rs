// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

const RESEND_API: &str = "https://api.resend.com/emails";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendRequest {
    pub from: String,
    pub to: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendResponse {
    pub id: Option<String>,
    pub error: Option<String>,
}

/// 通过 Resend API 发送一封邮件。
/// `from` 即固定发信名称（已包含显示名与邮箱）。
pub async fn send_email(api_key: &str, req: &SendRequest) -> Result<SendResponse> {
    if api_key.trim().is_empty() {
        bail!("API Key 未配置");
    }
    if req.to.is_empty() {
        bail!("收件人不能为空");
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(RESEND_API)
        .bearer_auth(api_key.trim())
        .header("Content-Type", "application/json")
        .json(&json!({
            "from": req.from,
            "to": req.to,
            "subject": req.subject,
            "text": req.text,
            "html": req.html,
        }))
        .send()
        .await?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        let msg = body
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("发送失败");
        bail!("Resend 错误 ({}): {}", status, msg);
    }

    let response: SendResponse = serde_json::from_value(body)?;
    if let Some(err) = &response.error {
        bail!("发送失败: {}", err);
    }
    Ok(response)
}
