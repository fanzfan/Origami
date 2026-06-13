//! macOS 文件关联管理（Launch Services）。
//!
//! 通过 Launch Services 把某扩展名对应内容类型(UTI)的默认打开程序设为 Origami，
//! 或查询当前默认程序。设置即时生效，不需要签名/公证；但应用需已被 Launch
//! Services 注册（即已安装的 .app 至少被系统识别过一次）。
//!
//! 说明：macOS 没有「清除默认程序、还给系统决定」的公开接口；因此「取消关联」
//! 的语义是把默认程序还原为系统归档工具 com.apple.archiveutility。

use anyhow::{bail, Context, Result};
use objc2::rc::Retained;
use objc2_foundation::NSString;
use std::ffi::c_void;

/// 本应用 bundle identifier（与 tauri.conf.json 的 identifier 一致）。
const BUNDLE_ID: &str = "dev.vela.origami";
/// 取消关联时还原到的系统默认程序。
const ARCHIVE_UTILITY: &str = "com.apple.archiveutility";

type CFStringRef = *const c_void;
type OSStatus = i32;
const K_LS_ROLES_ALL: u32 = 0xFFFF_FFFF; // kLSRolesAll

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn LSCopyDefaultRoleHandlerForContentType(content_type: CFStringRef, role: u32) -> CFStringRef;
    fn LSSetDefaultRoleHandlerForContentType(
        content_type: CFStringRef,
        role: u32,
        handler_bundle_id: CFStringRef,
    ) -> OSStatus;
    fn UTTypeCreatePreferredIdentifierForTag(
        tag_class: CFStringRef,
        tag: CFStringRef,
        conforming_to: CFStringRef,
    ) -> CFStringRef;
}

fn cfstr(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

/// NSString* 与 CFStringRef 是 toll-free bridged，可直接当指针传递。
fn as_cf(ns: &NSString) -> CFStringRef {
    ns as *const NSString as CFStringRef
}

/// 接管一个 +1 的 CFStringRef（toll-free 桥接为 NSString），转成 Rust String。
fn take_cf_string(ptr: CFStringRef) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // from_raw 取得所有权，drop 时释放，避免泄漏。
    let ns = unsafe { Retained::from_raw(ptr as *mut NSString)? };
    Some(ns.to_string())
}

/// 扩展名 → 首选 UTI。
fn uti_for_ext(ext: &str) -> Option<Retained<NSString>> {
    let tag_class = cfstr("public.filename-extension");
    let tag = cfstr(ext);
    let ptr = unsafe {
        UTTypeCreatePreferredIdentifierForTag(as_cf(&tag_class), as_cf(&tag), std::ptr::null())
    };
    if ptr.is_null() {
        return None;
    }
    unsafe { Retained::from_raw(ptr as *mut NSString) }
}

/// 当前该扩展名的默认打开程序 bundle id。
pub fn current(ext: &str) -> Option<String> {
    let uti = uti_for_ext(ext)?;
    let ptr = unsafe { LSCopyDefaultRoleHandlerForContentType(as_cf(&uti), K_LS_ROLES_ALL) };
    take_cf_string(ptr)
}

pub fn is_associated(ext: &str) -> bool {
    current(ext)
        .map(|h| h.eq_ignore_ascii_case(BUNDLE_ID))
        .unwrap_or(false)
}

fn set_handler(ext: &str, bundle_id: &str) -> Result<()> {
    let uti = uti_for_ext(ext).with_context(|| format!("无法解析 .{ext} 的内容类型"))?;
    let bid = cfstr(bundle_id);
    let status =
        unsafe { LSSetDefaultRoleHandlerForContentType(as_cf(&uti), K_LS_ROLES_ALL, as_cf(&bid)) };
    if status == 0 {
        Ok(())
    } else {
        bail!("Launch Services 设置默认程序失败（OSStatus {status}）");
    }
}

pub fn associate(ext: &str) -> Result<()> {
    set_handler(ext, BUNDLE_ID)
}

/// 取消关联：仅当当前默认是 Origami 时，还原为系统归档工具。
pub fn remove(ext: &str) -> Result<()> {
    if is_associated(ext) {
        set_handler(ext, ARCHIVE_UTILITY)?;
    }
    Ok(())
}
