//! Windows 文件关联管理（HKCU\Software\Classes，无需管理员权限）。
//!
//! 对每个扩展名 `.ext`：
//!   - 建一个 ProgID `Origami.ext`，带友好名、默认图标、`shell\open\command`；
//!   - 在 `.ext\OpenWithProgids` 注册该 ProgID（让 Origami 出现在「打开方式」）；
//!   - 把 `.ext` 的默认值指向该 ProgID（设为默认打开程序）。
//!
//! 仅写 HKCU。Win10+ 的 UserChoice 哈希保护意味着「默认」在某些情况下系统仍会
//! 弹一次「如何打开」确认，但 ProgID + OpenWithProgids 注册始终生效。更改在重新
//! 登录或重启资源管理器后稳定生效（与经典右键菜单一致）。

use anyhow::{Context, Result};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const ROOT: &str = "Software\\Classes";

fn exe() -> Result<String> {
    Ok(std::env::current_exe()?.to_string_lossy().to_string())
}

/// 本应用为某扩展名使用的 ProgID。
fn progid(ext: &str) -> String {
    format!("Origami.{ext}")
}

/// 注册 ProgID 并把 `.ext` 默认打开程序设为它。
pub fn associate(ext: &str) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let exe = exe()?;
    let pid = progid(ext);

    // ProgID 本体。
    let (prog, _) = hkcu
        .create_subkey(format!("{ROOT}\\{pid}"))
        .context("创建 ProgID 失败")?;
    prog.set_value("", &format!("Origami {} 压缩文件", ext.to_uppercase()))?;
    let (icon, _) = hkcu.create_subkey(format!("{ROOT}\\{pid}\\DefaultIcon"))?;
    icon.set_value("", &format!("\"{exe}\",0"))?;
    let (cmd, _) = hkcu.create_subkey(format!("{ROOT}\\{pid}\\shell\\open\\command"))?;
    cmd.set_value("", &format!("\"{exe}\" \"%1\""))?;

    // 扩展名 → ProgID。
    let (extk, _) = hkcu
        .create_subkey(format!("{ROOT}\\.{ext}"))
        .context("创建扩展名键失败")?;
    extk.set_value("", &pid)?; // 默认打开程序
    let (owp, _) = hkcu.create_subkey(format!("{ROOT}\\.{ext}\\OpenWithProgids"))?;
    owp.set_value(&pid, &"")?; // 出现在「打开方式」列表

    Ok(())
}

/// 取消关联：删除 ProgID、从 OpenWithProgids 移除，若默认值是我们则清空。
pub fn remove(ext: &str) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let pid = progid(ext);

    let _ = hkcu.delete_subkey_all(format!("{ROOT}\\{pid}"));

    if let Ok(owp) =
        hkcu.open_subkey_with_flags(format!("{ROOT}\\.{ext}\\OpenWithProgids"), winreg::enums::KEY_ALL_ACCESS)
    {
        let _ = owp.delete_value(&pid);
    }

    if let Ok(extk) =
        hkcu.open_subkey_with_flags(format!("{ROOT}\\.{ext}"), winreg::enums::KEY_ALL_ACCESS)
    {
        if let Ok(cur) = extk.get_value::<String, _>("") {
            if cur == pid {
                let _ = extk.set_value("", &"");
            }
        }
    }
    Ok(())
}

/// 当前 `.ext` 的默认 ProgID（HKCU 视角），无则 None。
pub fn current(ext: &str) -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let extk = hkcu.open_subkey(format!("{ROOT}\\.{ext}")).ok()?;
    let pid: String = extk.get_value("").ok()?;
    if pid.is_empty() {
        None
    } else {
        Some(pid)
    }
}

/// 是否已由本应用关联（ProgID 存在且为该扩展名默认）。
pub fn is_associated(ext: &str) -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let pid = progid(ext);
    let prog_exists = hkcu.open_subkey(format!("{ROOT}\\{pid}")).is_ok();
    prog_exists && current(ext).as_deref() == Some(pid.as_str())
}
