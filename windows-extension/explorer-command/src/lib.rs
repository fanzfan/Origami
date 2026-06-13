//! Origami 资源管理器右键菜单（IExplorerCommand 进程内 COM 服务器）。
//!
//! 实现 Windows 11「新版右键菜单」顶层项「用 Origami 压缩 ▸」及其三个子项。
//! 子项 Invoke 时启动同目录的 Origami.exe：
//!   Origami.exe --compress=<zip|7z|ask> "<选中路径>" ...
//! 主程序的单实例逻辑（src-tauri/src/cli.rs）会把它们汇入运行中的实例。
//!
//! 此 DLL 必须随一个 **MSIX 稀疏包** 注册，且包必须经过签名（自签名+本机信任
//! 即可用于开发；分发需 Authenticode/受信任证书）。注册方式见 ../README.md。
//!
//! 注意：本文件按 `windows` crate 0.58 的接口签名编写，已在 Windows 上编译通过
//! （依赖 `windows` 的 `implement` 特性）。换 crate 版本时 IExplorerCommand_Impl 等
//! trait 的方法签名（shell 项参数 Option<&IShellItemArray>、CreateInstance 的
//! Option<&IUnknown>、Skip/Reset 的 Result<()> 等）可能需要再次微调。

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

/// 本 COM 服务器的 CLSID（须与 AppxManifest.xml 中一致）。
pub const CLSID_ORIGAMI_COMMAND: GUID = GUID::from_u128(0x6b3d8a1c_4f2e_4c7a_9e1d_7a2b5c8d9e01);

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

/// 启动 Origami.exe 执行一次快捷压缩。
unsafe fn launch(format: &str, items: Option<&IShellItemArray>) -> Result<()> {
    let Some(exe) = origami_exe() else {
        return Err(E_FAIL.into());
    };
    let mut params = format!("--compress={format}");
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
// 子命令（zip / 7z / 详细设置）
// ---------------------------------------------------------------------------

#[windows::core::implement(IExplorerCommand)]
struct SubCommand {
    title: &'static str,
    format: &'static str,
}

impl IExplorerCommand_Impl for SubCommand_Impl {
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
    fn GetState(&self, _items: Option<&IShellItemArray>, _slow: BOOL) -> Result<u32> {
        Ok(ECS_ENABLED.0 as u32)
    }
    fn Invoke(&self, items: Option<&IShellItemArray>, _bind: Option<&IBindCtx>) -> Result<()> {
        unsafe { launch(self.format, items) }
    }
    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }
    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

// ---------------------------------------------------------------------------
// 子命令枚举器
// ---------------------------------------------------------------------------

#[windows::core::implement(IEnumExplorerCommand)]
struct SubEnum {
    items: Vec<IExplorerCommand>,
    index: std::cell::Cell<usize>,
}

fn make_subcommands() -> Vec<IExplorerCommand> {
    vec![
        SubCommand { title: "压缩为 ZIP", format: "zip" }.into(),
        SubCommand { title: "压缩为 7Z", format: "7z" }.into(),
        SubCommand { title: "压缩（详细设置…）", format: "ask" }.into(),
    ]
}

impl IEnumExplorerCommand_Impl for SubEnum_Impl {
    fn Next(
        &self,
        celt: u32,
        puielt: *mut Option<IExplorerCommand>,
        pceltfetched: *mut u32,
    ) -> HRESULT {
        let mut fetched = 0u32;
        let out = unsafe { std::slice::from_raw_parts_mut(puielt, celt as usize) };
        while fetched < celt {
            let i = self.index.get();
            if i >= self.items.len() {
                break;
            }
            out[fetched as usize] = Some(self.items[i].clone());
            self.index.set(i + 1);
            fetched += 1;
        }
        if !pceltfetched.is_null() {
            unsafe { *pceltfetched = fetched };
        }
        if fetched == celt {
            S_OK
        } else {
            S_FALSE
        }
    }
    fn Skip(&self, celt: u32) -> Result<()> {
        self.index.set((self.index.get() + celt as usize).min(self.items.len()));
        Ok(())
    }
    fn Reset(&self) -> Result<()> {
        self.index.set(0);
        Ok(())
    }
    fn Clone(&self) -> Result<IEnumExplorerCommand> {
        let e: IEnumExplorerCommand = SubEnum {
            items: self.items.clone(),
            index: std::cell::Cell::new(self.index.get()),
        }
        .into();
        Ok(e)
    }
}

// ---------------------------------------------------------------------------
// 顶层命令「用 Origami 压缩 ▸」
// ---------------------------------------------------------------------------

#[windows::core::implement(IExplorerCommand)]
struct CompressRoot;

impl IExplorerCommand_Impl for CompressRoot_Impl {
    fn GetTitle(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR> {
        title_pwstr("用 Origami 压缩")
    }
    fn GetIcon(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR> {
        // 可返回 "Origami.exe,0"（资源图标）。此处留空，用默认。
        Err(E_NOTIMPL.into())
    }
    fn GetToolTip(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(CLSID_ORIGAMI_COMMAND)
    }
    fn GetState(&self, _items: Option<&IShellItemArray>, _slow: BOOL) -> Result<u32> {
        Ok(ECS_ENABLED.0 as u32)
    }
    fn Invoke(&self, _items: Option<&IShellItemArray>, _bind: Option<&IBindCtx>) -> Result<()> {
        // 有子菜单时顶层不直接执行。
        Ok(())
    }
    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_HASSUBCOMMANDS.0 as u32)
    }
    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        let e: IEnumExplorerCommand = SubEnum {
            items: make_subcommands(),
            index: std::cell::Cell::new(0),
        }
        .into();
        Ok(e)
    }
}

// ---------------------------------------------------------------------------
// 类工厂 + DLL 导出
// ---------------------------------------------------------------------------

#[windows::core::implement(IClassFactory)]
struct Factory;

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
        let root: IExplorerCommand = CompressRoot.into();
        unsafe { root.query(iid, object).ok() }
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
        if *rclsid != CLSID_ORIGAMI_COMMAND {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = Factory.into();
        factory.query(riid, ppv)
    }
}

#[no_mangle]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    // 保守起见保持加载（surrogate 进程会自行回收）。
    S_FALSE
}
