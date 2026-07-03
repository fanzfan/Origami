//! Windows 经典右键菜单（注册表）安装/卸载。
//!
//! 写 HKCU\Software\Classes 下的 shell 动词，直接把各动作平铺到右键菜单里
//! （不再套「用 Origami 压缩 ▸」级联子菜单，少一层）：
//!   压缩 → 文件(`*`)/文件夹(`Directory`)/文件夹背景；命令 `Origami.exe --compress=zip "%1"`
//!   解压 → 仅挂到各归档扩展名（SystemFileAssociations\.<ext>）；`--extract=here "%1"`
//! 单实例插件会把每次调用转发给已运行的主实例（见 cli.rs）。
//!
//! 仅写 HKCU，不需要管理员权限。Windows 11 上经典菜单出现在
//! 「显示更多选项 / Shift+F10」里；顶层新菜单需 IExplorerCommand 稀疏包
//! （见 windows-extension/）。

use anyhow::{Context, Result};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const ROOT: &str = "Software\\Classes";
/// 加压缩菜单的目标类：所有文件、文件夹、文件夹背景（在文件夹空白处右键）。
const TARGETS: &[&str] = &["*", "Directory", "Directory\\Background"];
/// 加解压菜单的归档扩展名：挂到 SystemFileAssociations\.<ext> 下，只对压缩包显示。
const ARCHIVE_EXTS: &[&str] = &[
    "zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "zst", "tzst", "jar", "apk",
];
/// 旧版本写入的级联父键名，卸载/重装时一并清除以便迁移。
const LEGACY_KEYS: &[&str] = &["Origami", "Origami.Extract"];

struct Verb {
    /// 注册表子键名（同源动作按名排序，故加数字前缀）。
    key: &'static str,
    title: &'static str,
    /// 完整命令行开关，如 "--compress=zip" / "--extract=here"。
    arg: &'static str,
}

/// 压缩动作，平铺到菜单顶层。
const COMPRESS_VERBS: &[Verb] = &[
    Verb { key: "Origami1Zip", title: "Origami 压缩为 ZIP", arg: "--compress=zip" },
    Verb { key: "Origami2Sevenz", title: "Origami 压缩为 7Z", arg: "--compress=7z" },
    Verb { key: "Origami3Ask", title: "Origami 压缩（详细设置…）", arg: "--compress=ask" },
];

/// 解压动作，平铺到压缩包的菜单顶层。
const EXTRACT_VERBS: &[Verb] = &[
    Verb { key: "Origami1ExtractHere", title: "Origami 解压到当前文件夹", arg: "--extract=here" },
    Verb { key: "Origami2ExtractFolder", title: "Origami 解压到单独文件夹", arg: "--extract=folder" },
    Verb { key: "Origami3ExtractAsk", title: "Origami 解压到…", arg: "--extract=ask" },
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

/// 压缩动作挂载点：某目标类的 shell 键。
fn compress_shell_base(target: &str) -> String {
    format!("{ROOT}\\{target}\\shell")
}

/// 解压动作挂载点：某归档扩展名的 SystemFileAssociations shell 键。
fn extract_shell_base(ext: &str) -> String {
    format!("{ROOT}\\SystemFileAssociations\\.{ext}\\shell")
}

/// 写一个顶层菜单项（无级联）：`{shell_base}\{key}` 带标题/图标，其下 command 为
/// `"exe" <arg> "<tok>"`。
fn write_verb(hkcu: &RegKey, shell_base: &str, v: &Verb, exe: &str, icon: &str, tok: &str) -> Result<()> {
    let vk = format!("{shell_base}\\{}", v.key);
    let (k, _) = hkcu.create_subkey(&vk).context("创建菜单项失败")?;
    k.set_value("", &v.title)?;
    k.set_value("Icon", &icon)?;
    let (cmd, _) = hkcu.create_subkey(format!("{vk}\\command"))?;
    cmd.set_value("", &format!("\"{exe}\" {} \"{tok}\"", v.arg))?;
    Ok(())
}

pub fn install() -> Result<()> {
    let exe = exe()?;
    let icon = format!("\"{exe}\",0");
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // 先清理旧版可能残留的级联父键，避免新旧并存。
    remove_all(&hkcu);

    // 压缩：文件 / 文件夹 / 文件夹背景，各自平铺三项。
    for target in TARGETS {
        let base = compress_shell_base(target);
        let tok = arg_token(target);
        for v in COMPRESS_VERBS {
            write_verb(&hkcu, &base, v, &exe, &icon, tok)?;
        }
    }

    // 解压：仅挂到各归档扩展名，只对压缩包显示；文件参数用 %1。
    for ext in ARCHIVE_EXTS {
        let base = extract_shell_base(ext);
        for v in EXTRACT_VERBS {
            write_verb(&hkcu, &base, v, &exe, &icon, "%1")?;
        }
    }
    Ok(())
}

/// 删除本应用写入的全部菜单项（含旧版级联父键）。
fn remove_all(hkcu: &RegKey) {
    for target in TARGETS {
        let base = compress_shell_base(target);
        for v in COMPRESS_VERBS {
            let _ = hkcu.delete_subkey_all(format!("{base}\\{}", v.key));
        }
        for legacy in LEGACY_KEYS {
            let _ = hkcu.delete_subkey_all(format!("{base}\\{legacy}"));
        }
    }
    for ext in ARCHIVE_EXTS {
        let base = extract_shell_base(ext);
        for v in EXTRACT_VERBS {
            let _ = hkcu.delete_subkey_all(format!("{base}\\{}", v.key));
        }
        for legacy in LEGACY_KEYS {
            let _ = hkcu.delete_subkey_all(format!("{base}\\{legacy}"));
        }
    }
}

pub fn uninstall() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    remove_all(&hkcu);
    Ok(())
}

pub fn installed() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let first = COMPRESS_VERBS[0].key;
    TARGETS
        .iter()
        .all(|t| hkcu.open_subkey(format!("{ROOT}\\{t}\\shell\\{first}")).is_ok())
}
