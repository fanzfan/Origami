//! Windows 经典右键菜单（注册表）安装/卸载。
//!
//! 写 HKCU\Software\Classes 下的级联 shell 菜单，对文件(`*`)与文件夹(`Directory`)
//! 都加上「用 Origami 压缩 ▸」子菜单，子项调用本程序：
//!   Origami.exe --compress=zip "%1"
//! 单实例插件会把每次调用转发给已运行的主实例（见 cli.rs）。
//!
//! 仅写 HKCU，不需要管理员权限。Windows 11 上经典菜单出现在
//! 「显示更多选项 / Shift+F10」里；顶层新菜单需 IExplorerCommand 稀疏包
//! （见 windows-extension/，休眠待签名打包）。

use anyhow::{Context, Result};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const ROOT: &str = "Software\\Classes";
const VERB: &str = "Origami";
/// 加菜单的目标类：所有文件、文件夹、文件夹背景（在文件夹空白处右键）。
const TARGETS: &[&str] = &["*", "Directory", "Directory\\Background"];

struct Item {
    /// 注册表子键名（决定排序，故加数字前缀）
    key: &'static str,
    title: &'static str,
    format: &'static str,
}

const ITEMS: &[Item] = &[
    Item { key: "01zip", title: "压缩为 ZIP", format: "zip" },
    Item { key: "02sevenz", title: "压缩为 7Z", format: "7z" },
    Item { key: "03ask", title: "压缩（详细设置…）", format: "ask" },
];

fn exe() -> Result<String> {
    Ok(std::env::current_exe()?.to_string_lossy().to_string())
}

/// 文件夹背景右键时用 `%V`（当前目录），其余用 `%1`（选中项）。
fn arg_token(target: &str) -> &'static str {
    if target == "Directory\\Background" {
        "%V"
    } else {
        "%1"
    }
}

fn base_path(target: &str) -> String {
    format!("{ROOT}\\{target}\\shell\\{VERB}")
}

pub fn install() -> Result<()> {
    let exe = exe()?;
    let icon = format!("\"{exe}\",0");
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    for target in TARGETS {
        let base = base_path(target);
        let (parent, _) = hkcu.create_subkey(&base).context("创建菜单父项失败")?;
        parent.set_value("MUIVerb", &"用 Origami 压缩")?;
        parent.set_value("Icon", &icon)?;
        // 空字符串 SubCommands + 嵌套 shell 子键 = 级联子菜单。
        parent.set_value("SubCommands", &"")?;

        let tok = arg_token(target);
        for it in ITEMS {
            let item_key = format!("{base}\\shell\\{}", it.key);
            let (k, _) = hkcu.create_subkey(&item_key)?;
            k.set_value("", &it.title)?;
            k.set_value("Icon", &icon)?;
            let (cmd, _) = hkcu.create_subkey(format!("{item_key}\\command"))?;
            let line = format!("\"{exe}\" --compress={} \"{tok}\"", it.format);
            cmd.set_value("", &line)?;
        }
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for target in TARGETS {
        let base = base_path(target);
        // delete_subkey_all 递归删除；不存在则忽略。
        let _ = hkcu.delete_subkey_all(&base);
    }
    Ok(())
}

pub fn installed() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    TARGETS
        .iter()
        .all(|t| hkcu.open_subkey(base_path(t)).is_ok())
}
