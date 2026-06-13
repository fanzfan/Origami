//! 手动安装/卸载 Finder 右键菜单：
//! cargo run --example install_services [-- uninstall]
fn main() {
    let uninstall = std::env::args().nth(1).as_deref() == Some("uninstall");
    if uninstall {
        open_bandizip_lib::services::uninstall().unwrap();
        println!("已移除 Finder 右键菜单");
    } else {
        open_bandizip_lib::services::install().unwrap();
        println!("已安装 Finder 右键菜单到 ~/Library/Services");
    }
}
