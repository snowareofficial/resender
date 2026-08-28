// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

//![allow(non_snake_case)]
//! 国密加解密原语（libsmx SM4-GCM + SM3 KDF），供 Rhai crypto 模块调用

use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use libsmx::sm2::{get_e, get_z, sign, verify, PrivateKey};
use libsmx::sm3::Sm3Hasher;
use libsmx::sm4::{sm4_decrypt_gcm_combined, sm4_encrypt_gcm_combined};
use rand::RngCore;

/// 由密码与盐派生 16 字节 SM4 密钥
pub fn derive_key(password: &str, salt: &[u8; 16]) -> [u8; 16] {
    let mut material = Vec::new();
    material.extend_from_slice(password.as_bytes());
    material.extend_from_slice(salt);
    let mut key = [0u8; 16];
    let mut buf: Vec<u8> = material.clone();
    for _ in 0..1000 {
        let mut h = Sm3Hasher::new();
        h.update(&buf);
        let digest = h.finalize();
        buf = digest.iter().copied().collect::<Vec<u8>>();
        if buf.len() < 16 {
            buf.extend_from_slice(&material);
        }
    }
    key.copy_from_slice(&buf[..16]);
    key
}

/// 加密：返回 "ct_b64|nonce_b64|salt_b64"
pub fn encrypt_with_password(plaintext: &str, password: &str) -> Result<String> {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let key = derive_key(password, &salt);
    let ct = sm4_encrypt_gcm_combined(&key, &nonce, b"resender", plaintext.as_bytes());
    Ok(format!("{}|{}|{}", B64.encode(ct), B64.encode(nonce), B64.encode(salt)))
}

/// 解密 "ct_b64|nonce_b64|salt_b64"
pub fn decrypt_with_password(payload: &str, password: &str) -> Result<String> {
    let parts: Vec<&str> = payload.split('|').collect();
    if parts.len() != 3 {
        bail!("密文格式错误");
    }
    let ct = B64.decode(parts[0])?;
    let nonce = B64.decode(parts[1])?;
    let salt = B64.decode(parts[2])?;
    if nonce.len() != 12 || salt.len() != 16 {
        bail!("密文元数据长度异常");
    }
    let mut nonce_a = [0u8; 12];
    nonce_a.copy_from_slice(&nonce);
    let mut salt_a = [0u8; 16];
    salt_a.copy_from_slice(&salt);
    let key = derive_key(password, &salt_a);
    let pt = sm4_decrypt_gcm_combined(&key, &nonce_a, b"resender", &ct)
        .map_err(|e| anyhow::anyhow!("解密失败（密码错误或数据损坏）: {e}"))?;
    String::from_utf8(pt).map_err(|_| anyhow::anyhow!("解密结果非 UTF-8"))
}

/// 从 base64 salt 派生密钥，返回 base64
pub fn derive_key_b64(password: &str, salt_b64: &str) -> Result<String> {
    let salt = B64.decode(salt_b64)?;
    if salt.len() != 16 {
        bail!("salt 长度异常");
    }
    let mut salt_a = [0u8; 16];
    salt_a.copy_from_slice(&salt);
    Ok(B64.encode(derive_key(password, &salt_a)))
}

// ===========================================================================
//  Rhai 脚本签名（信任机制）
//  - SM2 签名使用 GB/T 32918 标准：e = SM3(Z || M)，用户 ID 固定为
//    b"resender-script"（与验签端一致即可）。
//  - 公钥为 65 字节 04||x||y（hex 编码）。
//  - 后量子（pq）模式保留接口占位：当前回退为错误，待对接具体 PQ 算法。
// ===========================================================================

const SCRIPT_SIG_ID: &[u8] = b"resender-script";

/// 用 32 字节原始私钥（hex）对脚本内容签名，返回 64 字节签名的 hex
pub fn sign_script_hex(script: &str, priv_key_hex: &str) -> Result<String> {
    let raw = hex_decode(priv_key_hex)?;
    if raw.len() != 32 {
        bail!("私钥长度异常（应为 32 字节）");
    }
    let mut k = [0u8; 32];
    k.copy_from_slice(&raw);
    let pri = PrivateKey::from_bytes(&k)
        .map_err(|e| anyhow::anyhow!("私钥非法: {e}"))?;
    // e = SM3(Z || M)，Z 由 id + 公钥决定
    let pubk = pri.public_key();
    let z = get_z(SCRIPT_SIG_ID, &pubk);
    let e = get_e(&z, script.as_bytes());
    let sig = sign(&e, &pri, &mut rand::thread_rng());
    Ok(hex_encode(&sig))
}

/// 验证脚本签名：msg + 65 字节公钥(hex) + 64 字节签名(hex)
/// mode: "sm2" 使用 SM2 验签；"pq" 暂不支持（返回错误）
pub fn verify_script_sig(script: &str, pub_key_hex: &str, sig_hex: &str, mode: &str) -> Result<bool> {
    match mode {
        "sm2" => {
            let pk = hex_decode(pub_key_hex)?;
            if pk.len() != 65 {
                bail!("SM2 公钥长度异常（应为 65 字节）");
            }
            let mut pubk = [0u8; 65];
            pubk.copy_from_slice(&pk);
            let sig = hex_decode(sig_hex)?;
            if sig.len() != 64 {
                bail!("SM2 签名长度异常（应为 64 字节）");
            }
            let mut s = [0u8; 64];
            s.copy_from_slice(&sig);
            // e = SM3(Z || M)
            let z = get_z(SCRIPT_SIG_ID, &pubk);
            let e = get_e(&z, script.as_bytes());
            // 验签未通过（消息/签名不匹配）属于预期业务结果，返回 Ok(false)
            match verify(&e, &pubk, &s) {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }
        "pq" => {
            // 后量子签名接口占位：对接时替换为具体 PQ 算法（如 Dilithium/LMS）
            // 当前返回错误，避免静默放行
            bail!("后量子签名校验尚未接入（PQ 接口占位）")
        }
        _ => bail!("未知的签名校验模式: {mode}"),
    }
}

fn hex_decode(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        bail!("hex 字符串长度必须为偶数");
    }
    let bytes = (0..s.len()).step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<std::result::Result<Vec<u8>, _>>()
        .map_err(|e| anyhow::anyhow!("hex 解码失败: {e}"))?;
    Ok(bytes)
}

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sm2_sign_verify_roundtrip() {
        use libsmx::sm2::{generate_keypair, sign_message, verify_message};
        let mut rng = rand::thread_rng();
        let (pri, pubk) = generate_keypair(&mut rng);
        let script = b"fn send_mail() { }";
        // 官方接口
        let sig = sign_message(script, SCRIPT_SIG_ID, &pri, &mut rng);
        verify_message(script, SCRIPT_SIG_ID, &pubk, &sig).expect("official sign/verify must pass");
        // 手写 hex 往返版
        let priv_hex = hex_encode(pri.as_bytes());
        let pub_hex = hex_encode(&pubk);
        let sig_hex = sign_script_hex("fn send_mail() { }", &priv_hex).unwrap();
        assert!(verify_script_sig("fn send_mail() { }", &pub_hex, &sig_hex, "sm2").unwrap());
        assert!(!verify_script_sig("tampered", &pub_hex, &sig_hex, "sm2").unwrap());
    }
}
