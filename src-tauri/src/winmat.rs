//! Windows 亚克力材质的透明度调节。
//!
//! Tauri 的 `setEffects(Acrylic)` 在 Win11 走 DWM 系统背景（DWMSBT_TRANSIENTWINDOW），
//! 那条路径**不支持自定义色调/透明度**（`WindowEffectsConfig.color` 在 Win11 无效）。
//! 想让亚克力可调透明度，只能绕过它，直接用旧版桌面合成接口
//! `SetWindowCompositionAttribute` + `ACCENT_ENABLE_ACRYLICBLURBEHIND`：其渐变色的
//! alpha 通道即色调不透明度——alpha 越小越透（桌面/模糊透出越多），越大越接近纯色。
//! 该接口在 Win10 v1809+ 与 Win11 上均可用。
//!
//! 由前端在「材质=亚克力」时调用；切到云母/无之前也会调用以清除本 accent 策略，
//! 避免残留的亚克力模糊与新状态打架。

#![cfg(target_os = "windows")]
#![allow(non_snake_case)]

use std::ffi::c_void;
use windows::core::s;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

// accent 状态（未公开，取自 window-vibrancy）。
const ACCENT_DISABLED: u32 = 0;
const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;
// WCA_ACCENT_POLICY：SetWindowCompositionAttribute 的属性 id。
const WCA_ACCENT_POLICY: u32 = 0x13;
// DWM 系统背景取值：1 = DWMSBT_DISABLE（关闭 Win11 的新版材质，让 accent 生效）。
const DWMSBT_DISABLE: u32 = 1;

#[repr(C)]
struct AccentPolicy {
    accent_state: u32,
    accent_flags: u32,
    gradient_color: u32,
    animation_id: u32,
}

#[repr(C)]
struct WinCompAttrData {
    attrib: u32,
    pv_data: *mut c_void,
    cb_data: usize,
}

type SetWindowCompositionAttributeFn =
    unsafe extern "system" fn(HWND, *mut WinCompAttrData) -> i32;

/// 动态解析 user32!SetWindowCompositionAttribute（未在导入库里公开，须运行时取址）。
unsafe fn resolve_swca() -> Option<SetWindowCompositionAttributeFn> {
    let module = LoadLibraryA(s!("user32.dll")).ok()?;
    let proc = GetProcAddress(module, s!("SetWindowCompositionAttribute"))?;
    Some(std::mem::transmute::<
        unsafe extern "system" fn() -> isize,
        SetWindowCompositionAttributeFn,
    >(proc))
}

/// 施加亚克力并设定色调不透明度。`(r,g,b)` 为色调基色（一般取窗口背景色），
/// `alpha` 为不透明度 0..=255（0 会被亚克力忽略，自动抬到 1）。
pub fn apply_acrylic(hwnd: isize, r: u8, g: u8, b: u8, alpha: u8) -> Result<(), String> {
    let hwnd = HWND(hwnd as *mut c_void);
    unsafe {
        // 先关掉 Win11 的 DWM 系统背景，避免与旧版 accent 亚克力叠加/互斥。
        // 旧系统上该属性未知，调用失败可忽略。
        let backdrop = DWMSBT_DISABLE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );

        let swca = resolve_swca().ok_or("系统不支持 SetWindowCompositionAttribute")?;
        let alpha = alpha.max(1); // 亚克力不接受 alpha=0
        let gradient =
            (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | ((alpha as u32) << 24);
        let mut policy = AccentPolicy {
            accent_state: ACCENT_ENABLE_ACRYLICBLURBEHIND,
            accent_flags: 0,
            gradient_color: gradient,
            animation_id: 0,
        };
        let mut data = WinCompAttrData {
            attrib: WCA_ACCENT_POLICY,
            pv_data: &mut policy as *mut _ as *mut c_void,
            cb_data: std::mem::size_of::<AccentPolicy>(),
        };
        swca(hwnd, &mut data as *mut _);
    }
    Ok(())
}

/// 清除本模块施加的 accent 亚克力（恢复到无 accent 状态）。
/// 云母/无材质由 Tauri 的 `setEffects` 自行处理，这里只负责撤掉 accent 策略。
pub fn clear_acrylic(hwnd: isize) -> Result<(), String> {
    let hwnd = HWND(hwnd as *mut c_void);
    unsafe {
        let swca = match resolve_swca() {
            Some(f) => f,
            None => return Ok(()), // 接口都没有，自然也没施加过，无需清除
        };
        let mut policy = AccentPolicy {
            accent_state: ACCENT_DISABLED,
            accent_flags: 0,
            gradient_color: 0,
            animation_id: 0,
        };
        let mut data = WinCompAttrData {
            attrib: WCA_ACCENT_POLICY,
            pv_data: &mut policy as *mut _ as *mut c_void,
            cb_data: std::mem::size_of::<AccentPolicy>(),
        };
        swca(hwnd, &mut data as *mut _);
    }
    Ok(())
}
