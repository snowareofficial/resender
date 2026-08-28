// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

fn main() {
    slint_build::compile("ui.slint").expect("编译 ui.slint 失败");

    // Windows 下把 assets/logo.ico 嵌入 exe 资源：
    // 使任务栏 / 资源管理器 / 快捷方式显示正确图标
    // （Slint 的 Window.icon 在部分 Windows 版本不影响任务栏图标）。
    // 其他平台（macOS / Linux）无此机制，图标由平台约定提供。
    #[cfg(windows)]
    {
        let ico = std::path::Path::new("assets").join("logo.ico");
        if ico.exists() {
            // embed-resource 3：compile 返回 CompilationResult 枚举
            // （NotWindows/Ok/NotAttempted/Failed），官方示例直接忽略返回值；
            // 这里用 manifest_optional 把 Failed 转成显式错误。
            let result = embed_resource::compile("assets/logo.rc", embed_resource::NONE);
            // 诊断：打印 CompilationResult 的实际状态
            match &result {
                embed_resource::CompilationResult::NotWindows => {
                    println!("cargo:warning=embed-resource: 非 Windows，跳过");
                }
                embed_resource::CompilationResult::Ok => {
                    println!("cargo:warning=embed-resource: 图标注入成功");
                }
                embed_resource::CompilationResult::NotAttempted(e) => {
                    println!("cargo:warning=embed-resource 未尝试（缺工具）: {e}");
                }
                embed_resource::CompilationResult::Failed(e) => {
                    panic!("嵌入应用图标失败: {e:?}");
                }
            }
            println!("cargo:rerun-if-changed=assets/logo.ico");
            println!("cargo:rerun-if-changed=assets/logo.rc");
        } else {
            println!("cargo:warning=缺少 assets/logo.ico，跳过图标注入（运行 tools/genico.py 生成）");
        }
    }
}
