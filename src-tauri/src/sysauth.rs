//! 系统级身份验证（生物识别 / 系统密码）。
//!
//! 用于在展示「密码管理器」中保存的明文密码前，要求用户通过操作系统的本地认证：
//! - macOS：LocalAuthentication（Touch ID，或回退到登录密码）。
//! - Windows：Windows Hello（UserConsentVerifier，指纹/面容/PIN）。
//! - 其它平台：不支持，调用方应直接放行。

/// 当前平台是否提供可用的本地认证（已配置 Touch ID / Windows Hello / 登录密码）。
pub fn available() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::available()
    }
    #[cfg(target_os = "windows")]
    {
        windows_impl::available()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// 触发一次系统认证，阻塞直到用户完成或取消。返回是否通过。
///
/// 在不支持的平台返回 `Ok(true)`（无认证可用时不阻断用户查看自己的密码）。
pub fn authenticate(reason: &str) -> anyhow::Result<bool> {
    #[cfg(target_os = "macos")]
    {
        macos::authenticate(reason)
    }
    #[cfg(target_os = "windows")]
    {
        windows_impl::authenticate(reason)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = reason;
        Ok(true)
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;
    use std::sync::mpsc;
    use std::time::Duration;

    // LAPolicyDeviceOwnerAuthentication：优先生物识别，可回退到设备登录密码。
    const LA_POLICY_DEVICE_OWNER_AUTH: isize = 2;

    fn new_context() -> Retained<AnyObject> {
        unsafe { msg_send![class!(LAContext), new] }
    }

    pub fn available() -> bool {
        unsafe {
            let ctx = new_context();
            let mut err: *mut AnyObject = std::ptr::null_mut();
            let can: Bool =
                msg_send![&*ctx, canEvaluatePolicy: LA_POLICY_DEVICE_OWNER_AUTH, error: &mut err];
            can.as_bool()
        }
    }

    pub fn authenticate(reason: &str) -> anyhow::Result<bool> {
        if !available() {
            // 无任何本地认证可用时不阻断。
            return Ok(true);
        }
        let reason = NSString::from_str(reason);
        let (tx, rx) = mpsc::channel::<bool>();
        let handler = RcBlock::new(move |success: Bool, _error: *mut AnyObject| {
            let _ = tx.send(success.as_bool());
        });
        unsafe {
            let ctx = new_context();
            let _: () = msg_send![
                &*ctx,
                evaluatePolicy: LA_POLICY_DEVICE_OWNER_AUTH,
                localizedReason: &*reason,
                reply: &*handler,
            ];
        }
        // 认证对话框由系统呈现，回调在私有队列触发；最多等待 2 分钟。
        Ok(rx.recv_timeout(Duration::from_secs(120)).unwrap_or(false))
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    //! 注意：未打包的 Win32 应用调用 RequestVerificationAsync 时，某些 Windows 版本
    //! 需要先把消费同意对话框关联到窗口句柄（IInitializeWithWindow）。若在目标机上
    //! 弹不出 Hello 对话框，需用 interop 设置主窗口 HWND 后再调用。
    use windows::core::HSTRING;
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };

    pub fn available() -> bool {
        UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.get())
            .map(|a| a == UserConsentVerifierAvailability::Available)
            .unwrap_or(false)
    }

    pub fn authenticate(reason: &str) -> anyhow::Result<bool> {
        if !available() {
            return Ok(true);
        }
        let message = HSTRING::from(reason);
        let op = UserConsentVerifier::RequestVerificationAsync(&message)
            .map_err(|e| anyhow::anyhow!("无法发起 Windows Hello 验证: {e}"))?;
        let result = op
            .get()
            .map_err(|e| anyhow::anyhow!("Windows Hello 验证失败: {e}"))?;
        Ok(result == UserConsentVerificationResult::Verified)
    }
}
