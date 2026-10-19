//! 手动安装/卸载 Finder 右键菜单：
//! cargo run --example install_services [-- uninstall]
#[cfg(target_os = "macos")]
fn main() {
    let uninstall = std::env::args().nth(1).as_deref() == Some("uninstall");
    if uninstall {
        origami_lib::services::uninstall().unwrap();
        println!("已移除 Finder 右键菜单");
    } else {
        origami_lib::services::install().unwrap();
        println!("已安装 Finder 右键菜单到 ~/Library/Services");
    }
}

// `services` 模块只在 macOS 编译；其它平台保留一个空壳，免得 `cargo test` 编译 examples 时失败。
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("install_services 仅支持 macOS");
    std::process::exit(1);
}
