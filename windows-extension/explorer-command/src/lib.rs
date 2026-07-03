//! Origami 资源管理器右键菜单（IExplorerCommand 进程内 COM 服务器）。
//!
//! 实现 Windows 11「新版右键菜单」的顶层项 —— 各动作**直接平铺**在右键菜单里
//! （不套「用 Origami 压缩 ▸」级联子菜单，少一层）：
//!   Origami 压缩为 ZIP / 7Z / 详细设置        → Origami.exe --compress=<zip|7z|ask> "<路径>" …
//!   Origami 解压到当前文件夹 / 单独文件夹 / …  → Origami.exe --extract=<here|folder|ask> "<路径>" …
//! 解压项仅在选中项全是压缩包时显示（GetState 动态返回 ECS_HIDDEN）。
//! 主程序的单实例逻辑（src-tauri/src/cli.rs）会把它们汇入运行中的实例。
//!
//! 每个动作是一个独立的顶层命令，对应各自的 CLSID，全部由本 DLL 提供，
//! 并在 AppxManifest.xml 里各注册一条 Verb。换 windows crate 版本时
//! IExplorerCommand_Impl 等 trait 的方法签名可能需要再次微调（本文件按 0.58 编写）。

#![allow(non_snake_case)]

use std::ffi::{c_void, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

// 六个顶层命令的 CLSID（须与 AppxManifest.xml 中一致）。
const CLSID_ZIP: GUID = GUID::from_u128(0x6b3d8a1c_4f2e_4c7a_9e1d_7a2b5c8d9e11);
const CLSID_7Z: GUID = GUID::from_u128(0x6b3d8a1c_4f2e_4c7a_9e1d_7a2b5c8d9e12);
const CLSID_ASK: GUID = GUID::from_u128(0x6b3d8a1c_4f2e_4c7a_9e1d_7a2b5c8d9e13);
const CLSID_HERE: GUID = GUID::from_u128(0x6b3d8a1c_4f2e_4c7a_9e1d_7a2b5c8d9e21);
const CLSID_FOLDER: GUID = GUID::from_u128(0x6b3d8a1c_4f2e_4c7a_9e1d_7a2b5c8d9e22);
const CLSID_EXTRACT_ASK: GUID = GUID::from_u128(0x6b3d8a1c_4f2e_4c7a_9e1d_7a2b5c8d9e23);

/// 一个顶层命令的定义。
struct CmdDef {
    clsid: GUID,
    title: &'static str,
    /// 完整命令行开关，如 "--compress=zip" / "--extract=here"。
    arg: &'static str,
    /// 仅当选中项全为压缩包时显示（解压类动作）。
    archive_only: bool,
}

const COMMANDS: &[CmdDef] = &[
    CmdDef { clsid: CLSID_ZIP, title: "Origami 压缩为 ZIP", arg: "--compress=zip", archive_only: false },
    CmdDef { clsid: CLSID_7Z, title: "Origami 压缩为 7Z", arg: "--compress=7z", archive_only: false },
    CmdDef { clsid: CLSID_ASK, title: "Origami 压缩（详细设置…）", arg: "--compress=ask", archive_only: false },
    CmdDef { clsid: CLSID_HERE, title: "Origami 解压到当前文件夹", arg: "--extract=here", archive_only: true },
    CmdDef { clsid: CLSID_FOLDER, title: "Origami 解压到单独文件夹", arg: "--extract=folder", archive_only: true },
    CmdDef { clsid: CLSID_EXTRACT_ASK, title: "Origami 解压到…", arg: "--extract=ask", archive_only: true },
];

/// 视为压缩包的扩展名（决定解压类动作是否对当前选中项显示）。
const ARCHIVE_EXTS: &[&str] = &[
    ".zip", ".7z", ".rar", ".tar", ".gz", ".tgz", ".bz2", ".tbz2", ".xz", ".txz", ".zst", ".tzst",
    ".jar", ".apk",
];

fn is_archive(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    ARCHIVE_EXTS.iter().any(|e| lower.ends_with(e))
}

static mut DLL_HMODULE: HMODULE = HMODULE(std::ptr::null_mut());

// ---------------------------------------------------------------------------
// 工具函数
// ---------------------------------------------------------------------------

fn wide(s: &str) -> Vec<u16> {
    OsString::from(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// 解析与本 DLL 同目录的 Origami.exe。
fn origami_exe() -> Option<PathBuf> {
    unsafe {
        let mut buf = [0u16; 32768];
        let len = GetModuleFileNameW(DLL_HMODULE, &mut buf);
        if len == 0 {
            return None;
        }
        let dll = PathBuf::from(OsString::from_wide(&buf[..len as usize]));
        Some(dll.parent()?.join("Origami.exe"))
    }
}

/// 从 IShellItemArray 取出所有文件系统路径。
unsafe fn selected_paths(items: Option<&IShellItemArray>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(items) = items else { return out };
    let Ok(count) = items.GetCount() else { return out };
    for i in 0..count {
        if let Ok(item) = items.GetItemAt(i) {
            if let Ok(pw) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                if let Ok(s) = pw.to_string() {
                    out.push(s);
                }
                CoTaskMemFree(Some(pw.0 as *const c_void));
            }
        }
    }
    out
}

/// 启动 Origami.exe，`flag_arg` 为完整开关（如 "--compress=zip" / "--extract=here"），
/// 后接选中的所有路径。
unsafe fn launch(flag_arg: &str, items: Option<&IShellItemArray>) -> Result<()> {
    let Some(exe) = origami_exe() else {
        return Err(E_FAIL.into());
    };
    let mut params = flag_arg.to_string();
    for p in selected_paths(items) {
        params.push_str(&format!(" \"{p}\""));
    }
    let exe_w = wide(&exe.to_string_lossy());
    let params_w = wide(&params);
    let h = ShellExecuteW(
        None,
        w!("open"),
        PCWSTR(exe_w.as_ptr()),
        PCWSTR(params_w.as_ptr()),
        PCWSTR::null(),
        SW_SHOWNORMAL,
    );
    // ShellExecuteW 返回值 > 32 表示成功。
    if h.0 as isize > 32 {
        Ok(())
    } else {
        Err(E_FAIL.into())
    }
}

fn title_pwstr(s: &str) -> Result<PWSTR> {
    // 返回的字符串由调用方用 CoTaskMemFree 释放，故用 SHStrDupW 复制。
    unsafe {
        let out = SHStrDupW(PCWSTR(wide(s).as_ptr()))?;
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// 顶层命令（平铺，无子菜单）
// ---------------------------------------------------------------------------

#[windows::core::implement(IExplorerCommand)]
struct TopCommand {
    title: &'static str,
    arg: &'static str,
    archive_only: bool,
}

impl IExplorerCommand_Impl for TopCommand_Impl {
    fn GetTitle(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR> {
        title_pwstr(self.title)
    }
    fn GetIcon(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetToolTip(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(GUID::zeroed())
    }
    fn GetState(&self, items: Option<&IShellItemArray>, _slow: BOOL) -> Result<u32> {
        if self.archive_only {
            // 仅当选中项全部是压缩包才显示，否则隐藏（挂在 Type="*" 上需自行过滤）。
            let paths = unsafe { selected_paths(items) };
            let show = !paths.is_empty() && paths.iter().all(|p| is_archive(p));
            if show {
                Ok(ECS_ENABLED.0 as u32)
            } else {
                Ok(ECS_HIDDEN.0 as u32)
            }
        } else {
            Ok(ECS_ENABLED.0 as u32)
        }
    }
    fn Invoke(&self, items: Option<&IShellItemArray>, _bind: Option<&IBindCtx>) -> Result<()> {
        unsafe { launch(self.arg, items) }
    }
    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }
    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

// ---------------------------------------------------------------------------
// 类工厂 + DLL 导出
// ---------------------------------------------------------------------------

#[windows::core::implement(IClassFactory)]
struct Factory {
    /// 对应 COMMANDS 中的下标，决定生产哪个顶层命令。
    index: usize,
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Option<&IUnknown>,
        iid: *const GUID,
        object: *mut *mut c_void,
    ) -> Result<()> {
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let def = &COMMANDS[self.index];
        let cmd: IExplorerCommand = TopCommand {
            title: def.title,
            arg: def.arg,
            archive_only: def.archive_only,
        }
        .into();
        unsafe { cmd.query(iid, object).ok() }
    }
    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

#[no_mangle]
extern "system" fn DllMain(hmodule: HMODULE, reason: u32, _reserved: *mut c_void) -> BOOL {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        unsafe { DLL_HMODULE = hmodule };
    }
    TRUE
}

#[no_mangle]
extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    unsafe {
        let Some(index) = COMMANDS.iter().position(|c| c.clsid == *rclsid) else {
            return CLASS_E_CLASSNOTAVAILABLE;
        };
        let factory: IClassFactory = Factory { index }.into();
        factory.query(riid, ppv)
    }
}

#[no_mangle]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    // 保守起见保持加载（surrogate 进程会自行回收）。
    S_FALSE
}
