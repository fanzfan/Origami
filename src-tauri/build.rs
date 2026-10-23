fn main() {
    // tauri-build 只给 bin 目标嵌入应用清单（其中声明了 Common Controls v6）。集成测试链接
    // tauri 后会导入只有 comctl32 v6 才有的 TaskDialogIndirect，没有清单就加载 v5，进程启动即
    // STATUS_ENTRYPOINT_NOT_FOUND。通用链接参数也覆盖 lib 单测；-tests 只覆盖集成测试，
    // 无法给以 --test 编译的库目标补上清单。
    // build.rs 跑在宿主机上，要看目标平台得读 CARGO_CFG_*，不能用 cfg!(windows)。
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
        // 清单统一由链接器生成，保留 tauri-build 的图标/版本资源，避免 bin 再嵌入同名清单。
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new_without_app_manifest(),
        ))
        .expect("构建 Tauri 资源失败");
    } else {
        tauri_build::build()
    }
}
