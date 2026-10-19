fn main() {
    // tauri-build 只给 bin 目标嵌入应用清单（其中声明了 Common Controls v6）。集成测试链接
    // tauri 后会导入只有 comctl32 v6 才有的 TaskDialogIndirect，没有清单就加载 v5，进程启动即
    // STATUS_ENTRYPOINT_NOT_FOUND。这里给测试目标补上同样的依赖声明。
    // build.rs 跑在宿主机上，要看目标平台得读 CARGO_CFG_*，不能用 cfg!(windows)。
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    tauri_build::build()
}
