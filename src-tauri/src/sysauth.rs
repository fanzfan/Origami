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
/// `hwnd`：宿主窗口句柄（仅 Windows 用，把 Hello 对话框关联到主窗口以确保前置；
/// 其它平台忽略）。在不支持的平台返回 `Ok(true)`（无认证可用时不阻断用户查看自己的密码）。
pub fn authenticate(reason: &str, hwnd: Option<isize>) -> anyhow::Result<bool> {
    #[cfg(target_os = "macos")]
    {
        let _ = hwnd;
        macos::authenticate(reason)
    }
    #[cfg(target_os = "windows")]
    {
        windows_impl::authenticate(reason, hwnd)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (reason, hwnd);
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
    //! Windows Hello（UserConsentVerifier）。未打包 / 普通 Win32 窗口应用直接调
    //! `RequestVerificationAsync` 时，同意对话框不会关联到任何窗口，常常**弹在后台**
    //! 而非最前。解决办法：用 interop 接口 `IUserConsentVerifierInterop`
    //! ::RequestVerificationForWindowAsync，把对话框 owner 设为主窗口 HWND，
    //! 系统就会把它前置并随主窗口模态显示。
    use windows::core::{factory, HSTRING};
    use windows::Foundation::IAsyncOperation;
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;

    pub fn available() -> bool {
        UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.get())
            .map(|a| a == UserConsentVerifierAvailability::Available)
            .unwrap_or(false)
    }

    pub fn authenticate(reason: &str, hwnd: Option<isize>) -> anyhow::Result<bool> {
        if !available() {
            return Ok(true);
        }
        let message = HSTRING::from(reason);

        // 有主窗口句柄时走 interop，把 Hello 对话框关联到主窗口（确保前置）；
        // 拿不到句柄再回退到无窗口版本。
        let op: IAsyncOperation<UserConsentVerificationResult> = match hwnd {
            Some(h) => {
                let interop: IUserConsentVerifierInterop =
                    factory::<UserConsentVerifier, IUserConsentVerifierInterop>()
                        .map_err(|e| anyhow::anyhow!("获取 Hello interop 失败: {e}"))?;
                unsafe {
                    interop
                        .RequestVerificationForWindowAsync(HWND(h as *mut _), &message)
                        .map_err(|e| anyhow::anyhow!("无法发起 Windows Hello 验证: {e}"))?
                }
            }
            None => UserConsentVerifier::RequestVerificationAsync(&message)
                .map_err(|e| anyhow::anyhow!("无法发起 Windows Hello 验证: {e}"))?,
        };

        let result = op
            .get()
            .map_err(|e| anyhow::anyhow!("Windows Hello 验证失败: {e}"))?;
        Ok(result == UserConsentVerificationResult::Verified)
    }
}
