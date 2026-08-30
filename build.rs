// Copyright (C) 2026~now S.A.
// SPDX-License-Identifier: MulanPubL-2.0

fn main() {
    // ── feature 分区：GUI 可选 ────────────────────────────────────────────
    // 见 Cargo.toml 的 [features]：`gui` 默认开启；
    // `--no-default-features` 编纯 CLI 时不编译 Slint 界面。
    //
    // **必须用环境变量而非 cfg!**：Cargo 不会把 package 的 feature 以 `--cfg`
    // 形式传给 build script（只设置 CARGO_FEATURE_GUI 环境变量），因此
    // build.rs 里的 `#[cfg(feature = "gui")]` 恒为 false —— 若用它门控函数，
    // 会出现「函数被裁掉却仍被调用」的 E0425。故这里读环境变量。
    let gui = std::env::var("CARGO_FEATURE_GUI").is_ok();
    if gui {
        compile_ui();
    } else {
        println!("cargo:warning=未启用 gui feature：跳过 Slint 界面编译（纯 CLI 构建）");
    }

    // —— 编译期内置脚本完整性校验 ——
    // 计算 scripts/default.rhai 的 SM3，与 scripts/default.rhai.sm3 中
    // 记录的「预期快照」比对。不一致（开发者改了脚本却没更新快照）
    // 则编译直接失败，明确指出差异并要求开发者决策：
    //   - 若本次修改是预期的，运行 `python tools/rehash_builtin.py` 重算快照后重新编译；
    //   - 若非预期（脚本被篡改），则禁止发布。
    // 这样「自动信任仅限自带未修改脚本」由编译器强制保证，而非靠运行时静默降级。
    builtin_script_integrity_check();

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

/// 编译 Slint 界面（仅 `gui` feature 启用时由 `main()` 调用）。
///
/// 本函数**不能**加 #[cfg(feature = "gui")]：Cargo 不向 build script 传
/// feature cfg，加了会导致函数恒被裁掉（E0425）。是否调用由 env var 决定。
fn compile_ui() {
    // 用 include_paths 编译：ui.slint 的 `import { FontsLoaded } from "fonts.slint"`
    // 会在 OUT_DIR 中找到该文件；fonts.slint 内 `import "MiSans VF.ttf"` 在 ui/ 中解析。
    let out_dir = std::path::PathBuf::from(
        std::env::var("OUT_DIR").expect("OUT_DIR 未设置"),
    );
    generate_font_import(&out_dir);

    let ui_dir = std::path::Path::new("ui")
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from("ui"));
    let cfg = slint_build::CompilerConfiguration::new()
        .with_include_paths(vec![out_dir, ui_dir]);
    slint_build::compile_with_config("ui.slint", cfg).expect("编译 ui.slint 失败");
}

/// 生成字体 import 的间接层文件（写入 OUT_DIR，供 Slint 的 include_paths 解析）。
///
/// 背景：界面字体 `ui/MiSans VF.ttf` 有 19 MiB，而 crates.io 的上传上限是
/// **10 MB**，故它不能随 crate 分发（否则上传被拒）。Slint 的 `import`
/// 无法条件化，因此把字体 import 抽到本文件，按字体是否实际存在来生成：
///   - 存在（从仓库源码构建）    -> 写入 import，使用 MiSans
///   - 不存在（从 crates.io 安装）-> 写入空文件，运行时回退系统字体
/// 这样两种来源都能构建成功，仅字体表现不同。
///
/// 见 `main()` 中的说明：字体 19 MiB 无法随 crate 分发（crates.io 上限 10 MB），
/// 故让该文件在字体缺失时为空，构建仍可成功，运行时回退系统字体。
///
/// **必须写 OUT_DIR 而非源目录**：cargo publish 的 verify 阶段禁止 build.rs
/// 修改源目录（"Source directory was modified by build.rs"），否则发布被拒。
fn generate_font_import(out_dir: &std::path::Path) {
    let font = std::path::Path::new("ui").join("MiSans VF.ttf");
    let out = out_dir.join("fonts.slint");
    let content = if font.exists() {
        // 从仓库源码构建：嵌入 MiSans。
        // 注意：字体 import 写 "MiSans VF.ttf"（相对 ui/ 目录），Slint 会
        // 在 include_paths（含 ui/）中解析它。
        // 额外导出 FontsLoaded 是为满足 Slint 的 import 语法（要求被导入
        // 文件有导出类型）；真正的字体注册来自上一行的资源导入。
        "// 由 build.rs 生成：字体存在，嵌入 MiSans VF\n\
         import \"MiSans VF.ttf\";\n\
         export component FontsLoaded inherits Rectangle { }\n"
    } else {
        // 从 crates.io 安装：无字体文件，回退系统字体
        "// 由 build.rs 生成：未找到 MiSans VF.ttf（crates.io 分发版不含字体），\n\
         // 运行时回退系统字体。从仓库源码构建即会使用 MiSans。\n\
         export component FontsLoaded inherits Rectangle { }\n"
    };
    std::fs::write(&out, content).expect("写入 fonts.slint 失败");
    if font.exists() {
        println!("cargo:rerun-if-changed=ui/MiSans VF.ttf");
    }
    println!("cargo:warning=字体模式: {}", if font.exists() { "内置 MiSans VF" } else { "系统字体回退" });
}

/// 编译期内置脚本完整性校验。
/// 计算 `scripts/default.rhai` 的 SM3 并与 `scripts/default.rhai.sm3` 的预期值比对。
/// 不一致则 panic，打印差异并要求开发者重新生成快照或排查篡改。
fn builtin_script_integrity_check() {
    let script = std::path::Path::new("scripts").join("default.rhai");
    let expected_file = std::path::Path::new("scripts").join("default.rhai.sm3");

    println!("cargo:rerun-if-changed={}", script.display());
    println!("cargo:rerun-if-changed={}", expected_file.display());

    if !script.exists() {
        panic!(
            "编译期校验失败：找不到内置脚本 {}。\n\
             \x20  resender 依赖 scripts/default.rhai 作为唯一内置脚本入口。",
            script.display()
        );
    }

    let bytes = std::fs::read(&script).expect("读取内置脚本失败");
    // 防御性归一化：无论文件在磁盘上是 CRLF 还是 LF，统一按 LF 计算哈希，
    // 避免不同平台/检出设置导致字节差异进而哈希不符（与仓库快照一致）。
    let normalized: Vec<u8> = bytes
        .iter()
        .copied()
        .filter(|&b| b != b'\r')
        .collect();
    let mut h = libsmx::sm3::Sm3Hasher::new();
    h.update(&normalized);
    let digest = h.finalize();
    let actual = digest.iter().map(|b| format!("{:02x}", b)).collect::<String>();

    // 把实际哈希注入编译环境，运行时直接复用（无需硬编码常量）
    println!("cargo:rustc-env=RESENDER_BUILTIN_SCRIPT_SM3={}", actual);

    let expected = if expected_file.exists() {
        std::fs::read_to_string(&expected_file)
            .expect("读取预期哈希快照失败")
            .trim()
            .to_string()
    } else {
        // 首次构建：自动生成快照文件，避免 CI/首次编译即失败。
        // 注意：这仅对「快照文件不存在」放宽；若文件存在但内容不符，仍强制失败。
        std::fs::write(&expected_file, format!("{}\n", actual))
            .expect("写入初始哈希快照失败");
        println!(
            "cargo:warning=已为内置脚本生成初始哈希快照 {}（请将其纳入版本控制）",
            actual
        );
        return;
    };

    if !actual.eq_ignore_ascii_case(&expected) {
        panic!(
            "内置脚本完整性校验失败：脚本哈希与已记录的快照不一致。\n\
             \x20  实际哈希  : {}\n\
             \x20  预期快照  : {}\n\
             \x20  脚本文件  : {}\n\
             \n\
             \x20  这说明 scripts/default.rhai 相对上次发布已被修改（或遭到篡改）。\n\
             \x20  请决策：\n\
             \x20    1) 若本次修改是预期的（你改了内置脚本逻辑）：\n\
             \x20       运行 `python tools/rehash_builtin.py` 重新生成快照，再重新编译。\n\
             \x20    2) 若非预期（脚本被外部改动/篡改）：\n\
             \x20       从版本控制恢复 scripts/default.rhai，不要发布。\n\
             \x20  安全约束：「自动信任仅限自带且未修改的内置脚本」，故哈希不符时\n\
             \x20  编译器拒绝放行，必须开发者显式决策。",
            actual, expected, script.display()
        );
    }
}
