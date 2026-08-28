"""验证「加密日志」：日志**默认**加密落盘，且能用本地自动生成的密钥解密还原。

检查项：
  1. 日志加密后，密文中不含任何明文（收件人等敏感信息不泄漏）
  2. LogStore 默认自动产生本地密钥，不依赖用户设置的加密密码
  3. 用同一实例密钥可完整解密还原
  4. 同一明文两次加密结果不同（随机 salt/nonce）
  5. LogStore 真实落盘：写入多条后文件内无明文，且能按顺序解密还原
  6. 用错误密钥无法解密，但保留占位提示（不静默返回原文）

实现方式：把测试注入 src/ 下的一个临时 #[cfg(test)] 模块，用 `cargo test` 跑，
这样验证的是**真实生产代码路径**（crate::crypto / crate::log），而非 Python 重写。
脚本结束后自动清理临时文件。
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAIN = ROOT / "src" / "main.rs"
MARKER_BEGIN = "// >>> TEMP-VERIFY-ENCRYPTED-LOGS BEGIN >>>"
MARKER_END = "// <<< TEMP-VERIFY-ENCRYPTED-LOGS END <<<"

TEST_MODULE = MARKER_BEGIN + r'''
#[cfg(test)]
mod temp_verify_encrypted_logs {
    use crate::crypto;

    #[test]
    fn log_payload_is_ciphertext_and_roundtrips() {
        let pw = "correct horse battery staple";
        let line = "send to alice@example.com ok id=42";

        // 1) 往返 + 密文不含明文
        let payload = crypto::encrypt_with_password(line, pw).expect("encrypt");
        assert!(!payload.contains("alice@example.com"), "密文中不应出现明文收件人");
        let back = crypto::decrypt_with_password(&payload, pw).expect("decrypt");
        assert_eq!(back, line, "解密结果应与原文一致");

        // 2) 错误密钥必须失败
        assert!(
            crypto::decrypt_with_password(&payload, "wrong password").is_err(),
            "错误密钥应解密失败"
        );

        // 3) 随机 salt/nonce：两次加密同一明文结果不同
        let p2 = crypto::encrypt_with_password(line, pw).expect("encrypt2");
        assert_ne!(payload, p2, "同一明文两次加密结果应不同");
    }

    #[test]
    fn log_store_writes_only_ciphertext_by_default() {
        let store = crate::log::LogStore::for_test("resender_log_store_test");
        // 确保从空文件开始
        store.clear().expect("clear");

        let expect = vec![
            "line one secret".to_string(),
            "line two alice@example.com".to_string(),
        ];
        for l in &expect {
            // 默认即加密，不依赖外部密码
            store.append(l).expect("append");
        }

        // 落盘文件不得包含任何明文
        let raw = std::fs::read_to_string(store.path()).expect("read file");
        for secret in &expect {
            assert!(!raw.contains(secret.as_str()), "落盘文件不应包含明文: {secret}");
        }

        // 用同一把本地密钥能按顺序解密还原
        let got = store.read_all().expect("read_all");
        assert_eq!(got, expect, "解密还原的日志应与原文顺序一致");

        // 清空
        store.clear().expect("clear2");
        assert!(!store.path().exists(), "清空后文件应不存在");
    }

    #[test]
    fn log_store_with_wrong_key_returns_unreadable_marker() {
        let store_a = crate::log::LogStore::for_test("resender_log_store_test_key_a");
        store_a.clear().expect("clear");
        store_a.append("secret line").expect("append");

        // 另一个独立目录会生成不同密钥，无法解密原文件
        let mut dir_a = std::env::temp_dir();
        dir_a.push("resender_log_store_test_key_a");
        // 用新建的 store（不同密钥）读取 store_a 的文件应失败，且保留占位提示
        let store_b = crate::log::LogStore::for_test("resender_log_store_test_key_b");
        // 复制文件让 store_b 读到 store_a 的密文，但密钥不同
        std::fs::copy(store_a.path(), store_b.path()).expect("copy");
        let got = store_b.read_all().expect("read_all");
        assert!(got.len() == 1, "应返回一条占位提示");
        assert!(
            got[0].contains("无法解密"),
            "错误密钥下应提示无法解密，而不是静默返回原文"
        );

        // 清理
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("resender_log_store_test_key_a"));
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("resender_log_store_test_key_b"));
    }
}
''' + MARKER_END + "\n"


def main() -> int:
    original = MAIN.read_text(encoding="utf-8")
    if MARKER_BEGIN in original:
        print("检测到上次未清理的临时测试模块，先还原")
        original = original.split(MARKER_BEGIN)[0] + original.split(MARKER_END)[-1].lstrip("\n")

    patched = original.rstrip("\n") + "\n\n" + TEST_MODULE
    MAIN.write_text(patched, encoding="utf-8", newline="\n")
    try:
        r = subprocess.run(
            ["cargo", "test", "temp_verify_encrypted_logs", "--", "--nocapture"],
            cwd=ROOT, capture_output=True, text=True,
            encoding="utf-8", errors="replace", timeout=900,
        )
        out = (r.stdout or "") + (r.stderr or "")
        tail = out[-3500:]
        print(tail)
        ok = r.returncode == 0 and "test result: ok" in out
        print("\n结果:", "PASS — 加密日志：密文落盘 + 可解密还原 + 错密码拒绝"
              if ok else "FAIL")
        return 0 if ok else 1
    finally:
        MAIN.write_text(original, encoding="utf-8", newline="\n")
        print("已还原 src/main.rs")


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    sys.exit(main())
