pub mod archive;
#[cfg(not(target_os = "macos"))]
pub mod cli;
pub mod encoding;
#[cfg(target_os = "macos")]
pub mod macassoc;
pub mod passwords;
#[cfg(target_os = "macos")]
pub mod services;
#[cfg(target_os = "macos")]
pub mod sysicon;
pub mod sysauth;
#[cfg(target_os = "windows")]
pub mod winassoc;
#[cfg(target_os = "windows")]
pub mod winmenu;

use archive::create::CreateOptions;
use archive::extract::{Ctx, ExtractOptions};
use archive::list::ListOptions;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Jobs(Mutex<HashMap<String, Arc<AtomicBool>>>);

impl Jobs {
    fn register(&self, id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.0.lock().unwrap().insert(id.to_string(), flag.clone());
        flag
    }
    fn finish(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
    fn cancel(&self, id: &str) {
        if let Some(f) = self.0.lock().unwrap().get(id) {
            f.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", rename_all_fields = "camelCase")]
pub enum PendingAction {
    Open { paths: Vec<String> },
    Create { format: String, paths: Vec<String> },
}

#[derive(Default)]
pub struct Pending(Mutex<Vec<PendingAction>>);

/// 通过 Finder 右键快捷压缩启动时置位：此时不显示主窗口，只弹迷你进度窗。
#[derive(Default)]
pub struct QuickLaunch(AtomicBool);

/// 解析 origami://create?format=zip&p=<base64url(path)>&p=… 深链。
fn parse_deep_link(url: &tauri::Url) -> Option<PendingAction> {
    use base64::Engine;
    if url.scheme() != "origami" || url.host_str() != Some("create") {
        return None;
    }
    let mut format = String::from("ask");
    let mut paths = Vec::new();
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "format" => format = v.to_string(),
            "p" => {
                if let Ok(bytes) =
                    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(v.as_bytes())
                {
                    if let Ok(s) = String::from_utf8(bytes) {
                        paths.push(s);
                    }
                }
            }
            _ => {}
        }
    }
    if paths.is_empty() {
        None
    } else {
        Some(PendingAction::Create { format, paths })
    }
}

/// 把一批动作投递给前端：决定是否静默快捷压缩、必要时显示主窗，入队并通知。
/// macOS 由 RunEvent::Opened 调用；Windows/Linux 由启动参数与单实例转发调用。
fn dispatch_actions(app: &tauri::AppHandle, actions: Vec<PendingAction>) {
    use tauri::{Emitter, Manager};
    if actions.is_empty() {
        return;
    }
    let all_quick = actions
        .iter()
        .all(|a| matches!(a, PendingAction::Create { format, .. } if format != "ask"));
    let main = app.get_webview_window("main");
    let main_visible = main
        .as_ref()
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false);
    if all_quick && !main_visible {
        app.state::<Arc<QuickLaunch>>()
            .0
            .store(true, Ordering::Relaxed);
    } else if let Some(w) = main {
        let _ = w.show();
        let _ = w.set_focus();
    }
    app.state::<Arc<Pending>>().0.lock().unwrap().extend(actions);
    let _ = app.emit("deep-link-available", ());
}

type CmdResult<T> = Result<T, String>;

fn err_str(e: anyhow::Error) -> String {
    let s = format!("{e:#}");
    if s.contains("PASSWORD_REQUIRED") {
        "PASSWORD_REQUIRED".to_string()
    } else if s.contains("CANCELLED") {
        "CANCELLED".to_string()
    } else {
        s
    }
}

#[tauri::command]
async fn list_archive(
    app: tauri::AppHandle,
    path: String,
    password: Option<String>,
    encoding: Option<String>,
) -> CmdResult<archive::ArchiveInfo> {
    let fallbacks = passwords::candidates(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let opts = ListOptions {
            password,
            encoding: encoding.unwrap_or_else(|| "auto".into()),
            fallback_passwords: fallbacks,
        };
        archive::list::list(&PathBuf::from(path), &opts).map_err(err_str)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn extract_archive(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    job_id: String,
    path: String,
    dest: String,
    password: Option<String>,
    encoding: Option<String>,
    entries: Option<Vec<String>>,
    smart: Option<bool>,
) -> CmdResult<String> {
    let cancel = jobs.register(&job_id);
    let fallbacks = passwords::candidates(&app);
    let jobs2 = jobs.inner().clone();
    let jid = job_id.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let ctx = Ctx { app: &app, cancel };
        let opts = ExtractOptions {
            job_id: jid,
            password: password.clone(),
            fallback_passwords: fallbacks,
            encoding: encoding.unwrap_or_else(|| "auto".into()),
            entries: entries.unwrap_or_default(),
            smart: smart.unwrap_or(true),
        };
        let r = archive::extract::extract(&ctx, &PathBuf::from(path), &PathBuf::from(dest), &opts);
        if r.is_ok() {
            if let Some(pw) = &password {
                let _ = passwords::add(&app, pw.clone(), None);
                passwords::mark_used(&app, pw);
            }
        }
        r.map_err(err_str)
    })
    .await
    .map_err(|e| e.to_string())?;
    jobs2.finish(&job_id);
    res
}

#[tauri::command]
async fn create_archive(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    job_id: String,
    dest: String,
    sources: Vec<String>,
    format: String,
    level: u32,
    method: Option<String>,
    password: Option<String>,
    volume_size: Option<u64>,
    exclude_junk: Option<bool>,
) -> CmdResult<String> {
    let cancel = jobs.register(&job_id);
    let jobs2 = jobs.inner().clone();
    let jid = job_id.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let ctx = Ctx { app: &app, cancel };
        let opts = CreateOptions {
            job_id: jid,
            format,
            level,
            method: method.unwrap_or_default(),
            password,
            volume_size: volume_size.unwrap_or(0),
            exclude_junk: exclude_junk.unwrap_or(false),
        };
        let dest_path = PathBuf::from(&dest);
        let r = archive::create::create(&ctx, &dest_path, &sources, &opts);
        if r.is_err() {
            let _ = std::fs::remove_file(&dest_path);
        }
        r.map_err(err_str)
    })
    .await
    .map_err(|e| e.to_string())?;
    jobs2.finish(&job_id);
    res
}

#[tauri::command]
async fn archive_add(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    job_id: String,
    path: String,
    sources: Vec<String>,
    dir: Option<String>,
    password: Option<String>,
    encoding: Option<String>,
) -> CmdResult<()> {
    let cancel = jobs.register(&job_id);
    let jobs2 = jobs.inner().clone();
    let jid = job_id.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let ctx = Ctx { app: &app, cancel };
        let opts = archive::edit::EditOptions {
            job_id: jid,
            password,
            encoding: encoding.unwrap_or_else(|| "auto".into()),
        };
        archive::edit::add_files(
            &ctx,
            &PathBuf::from(path),
            &sources,
            dir.as_deref().unwrap_or(""),
            &opts,
        )
        .map_err(err_str)
    })
    .await
    .map_err(|e| e.to_string())?;
    jobs2.finish(&job_id);
    res
}

#[tauri::command]
async fn archive_remove(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    job_id: String,
    path: String,
    entries: Vec<String>,
    password: Option<String>,
    encoding: Option<String>,
) -> CmdResult<()> {
    let cancel = jobs.register(&job_id);
    let jobs2 = jobs.inner().clone();
    let jid = job_id.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let ctx = Ctx { app: &app, cancel };
        let opts = archive::edit::EditOptions {
            job_id: jid,
            password,
            encoding: encoding.unwrap_or_else(|| "auto".into()),
        };
        archive::edit::remove_entries(&ctx, &PathBuf::from(path), &entries, &opts).map_err(err_str)
    })
    .await
    .map_err(|e| e.to_string())?;
    jobs2.finish(&job_id);
    res
}

#[tauri::command]
async fn test_archive(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    job_id: String,
    path: String,
    password: Option<String>,
) -> CmdResult<()> {
    let cancel = jobs.register(&job_id);
    let fallbacks = passwords::candidates(&app);
    let jobs2 = jobs.inner().clone();
    let jid = job_id.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let ctx = Ctx { app: &app, cancel };
        archive::extract::test_archive(&ctx, &PathBuf::from(path), &jid, password, fallbacks)
            .map_err(err_str)
    })
    .await
    .map_err(|e| e.to_string())?;
    jobs2.finish(&job_id);
    res
}

#[tauri::command]
async fn preview_entry(
    app: tauri::AppHandle,
    path: String,
    entry: String,
    password: Option<String>,
    encoding: Option<String>,
) -> CmdResult<archive::preview::Preview> {
    let fallbacks = passwords::candidates(&app);
    tauri::async_runtime::spawn_blocking(move || {
        archive::preview::preview_entry(
            &PathBuf::from(path),
            &entry,
            password,
            fallbacks,
            &encoding.unwrap_or_else(|| "auto".into()),
        )
        .map_err(err_str)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_job(jobs: tauri::State<'_, Arc<Jobs>>, job_id: String) {
    jobs.cancel(&job_id);
}

/// 返回某文件类型/目录的系统图标（base64 PNG）。仅 macOS。
#[cfg(target_os = "macos")]
#[tauri::command]
async fn system_icon(app: tauri::AppHandle, ext: String, is_dir: bool) -> Option<String> {
    use base64::Engine;
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(sysicon::icon_png(&ext, is_dir));
    })
    .ok()?;
    let png = rx.recv().ok()??;
    Some(base64::engine::general_purpose::STANDARD.encode(png))
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
async fn system_icon(_ext: String, _is_dir: bool) -> Option<String> {
    None
}

#[tauri::command]
fn pw_list(app: tauri::AppHandle) -> Vec<passwords::SavedPassword> {
    passwords::load(&app)
}

#[tauri::command]
fn pw_add(app: tauri::AppHandle, password: String, label: Option<String>) -> CmdResult<()> {
    passwords::add(&app, password, label).map_err(err_str)
}

#[tauri::command]
fn pw_remove(app: tauri::AppHandle, password: String) -> CmdResult<()> {
    passwords::remove(&app, &password).map_err(err_str)
}

/// 当前平台是否提供可用的系统级身份验证（Touch ID / Windows Hello / 登录密码）。
#[tauri::command]
fn system_auth_available() -> bool {
    sysauth::available()
}

/// 触发系统认证（用于在展示已保存密码前校验本人）。返回是否通过。
#[tauri::command]
async fn system_auth(reason: String) -> CmdResult<bool> {
    tauri::async_runtime::spawn_blocking(move || sysauth::authenticate(&reason).map_err(err_str))
        .await
        .map_err(|e| e.to_string())?
}

/// 把单个条目解压到缓存临时目录并返回其完整路径，供前端用系统默认程序打开。
#[tauri::command]
async fn extract_entry_to_temp(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    path: String,
    entry: String,
    password: Option<String>,
    encoding: Option<String>,
) -> CmdResult<String> {
    use tauri::Manager;
    let job_id = format!("open-{}", std::process::id());
    let cancel = jobs.register(&job_id);
    let fallbacks = passwords::candidates(&app);
    let jobs2 = jobs.inner().clone();
    let jid = job_id.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let base = app
            .path()
            .app_cache_dir()
            .map_err(|e| e.to_string())?
            .join("open");
        // 用条目相对路径的哈希做子目录，避免多次打开互相覆盖。
        let sub = format!("{:x}", seahash_like(&entry));
        let dest = base.join(sub);
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        let ctx = Ctx { app: &app, cancel };
        let opts = ExtractOptions {
            job_id: jid,
            password: password.clone(),
            fallback_passwords: fallbacks,
            encoding: encoding.unwrap_or_else(|| "auto".into()),
            entries: vec![entry.clone()],
            smart: false,
        };
        archive::extract::extract(&ctx, &PathBuf::from(&path), &dest, &opts).map_err(err_str)?;
        let rel =
            archive::sanitize_rel_path(&entry).ok_or_else(|| "条目路径非法".to_string())?;
        let out = dest.join(rel);
        if !out.exists() {
            return Err("解压后未找到目标文件".to_string());
        }
        Ok(out.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    jobs2.finish(&job_id);
    res
}

/// 轻量非加密哈希，仅用于生成临时目录名。
fn seahash_like(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[tauri::command]
fn take_pending_actions(pending: tauri::State<'_, Arc<Pending>>) -> Vec<PendingAction> {
    std::mem::take(&mut *pending.0.lock().unwrap())
}

#[tauri::command]
fn install_shell_menu() -> CmdResult<()> {
    #[cfg(target_os = "macos")]
    {
        return services::install().map_err(err_str);
    }
    #[cfg(target_os = "windows")]
    {
        return winmenu::install().map_err(err_str);
    }
    #[allow(unreachable_code)]
    Err("当前平台不支持右键菜单集成".into())
}

#[tauri::command]
fn uninstall_shell_menu() -> CmdResult<()> {
    #[cfg(target_os = "macos")]
    {
        return services::uninstall().map_err(err_str);
    }
    #[cfg(target_os = "windows")]
    {
        return winmenu::uninstall().map_err(err_str);
    }
    #[allow(unreachable_code)]
    Err("当前平台不支持右键菜单集成".into())
}

#[tauri::command]
fn shell_menu_installed() -> bool {
    #[cfg(target_os = "macos")]
    {
        return services::installed();
    }
    #[cfg(target_os = "windows")]
    {
        return winmenu::installed();
    }
    #[allow(unreachable_code)]
    false
}

#[tauri::command]
fn app_platform() -> &'static str {
    std::env::consts::OS
}

// ---------------- 文件关联管理 ----------------

/// 可由本应用接管的压缩包扩展名（与 tauri.conf.json 的 fileAssociations 对齐）。
const ASSOC_EXTS: &[&str] = &["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "zst"];

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AssocEntry {
    ext: String,
    /// 当前是否由 Origami 关联为默认。
    associated: bool,
    /// 当前默认打开程序的标识（win: ProgID；mac: bundle id），仅供展示。
    current_app: Option<String>,
}

/// 当前平台是否支持运行时文件关联管理。
#[tauri::command]
fn file_assoc_supported() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

#[tauri::command]
fn file_assoc_list() -> Vec<AssocEntry> {
    ASSOC_EXTS
        .iter()
        .map(|&ext| {
            #[cfg(target_os = "macos")]
            {
                AssocEntry {
                    ext: ext.to_string(),
                    associated: macassoc::is_associated(ext),
                    current_app: macassoc::current(ext),
                }
            }
            #[cfg(target_os = "windows")]
            {
                AssocEntry {
                    ext: ext.to_string(),
                    associated: winassoc::is_associated(ext),
                    current_app: winassoc::current(ext),
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                AssocEntry {
                    ext: ext.to_string(),
                    associated: false,
                    current_app: None,
                }
            }
        })
        .collect()
}

#[tauri::command]
fn file_assoc_set(exts: Vec<String>, associate: bool) -> CmdResult<()> {
    for ext in &exts {
        let ext = ext.trim_start_matches('.');
        #[cfg(target_os = "macos")]
        {
            if associate {
                macassoc::associate(ext).map_err(err_str)?;
            } else {
                macassoc::remove(ext).map_err(err_str)?;
            }
        }
        #[cfg(target_os = "windows")]
        {
            if associate {
                winassoc::associate(ext).map_err(err_str)?;
            } else {
                winassoc::remove(ext).map_err(err_str)?;
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = (ext, associate);
            return Err("当前平台不支持文件关联管理".into());
        }
    }
    Ok(())
}

/// 为快速压缩计算默认输出路径：与源同目录、按选中内容命名、避免覆盖已有文件。
#[tauri::command]
fn default_create_dest(sources: Vec<String>, ext: String) -> CmdResult<String> {
    let first = sources.first().ok_or("没有输入文件")?;
    let first = PathBuf::from(first);
    let dir = first.parent().unwrap_or(&first).to_path_buf();
    let name = if sources.len() == 1 {
        let stem = if first.is_dir() {
            first.file_name().map(|s| s.to_string_lossy().to_string())
        } else {
            first.file_stem().map(|s| s.to_string_lossy().to_string())
        };
        stem.unwrap_or_else(|| "归档".into())
    } else {
        dir.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "归档".into())
    };
    let mut dest = dir.join(format!("{name}.{ext}"));
    let mut i = 1;
    while dest.exists() {
        dest = dir.join(format!("{name} ({i}).{ext}"));
        i += 1;
    }
    Ok(dest.to_string_lossy().to_string())
}

/// 前端就绪后调用：除非是快捷压缩启动，否则显示主窗口。
#[tauri::command]
fn frontend_ready(app: tauri::AppHandle, quick: tauri::State<'_, Arc<QuickLaunch>>) {
    use tauri::Manager;
    if !quick.0.load(Ordering::Relaxed) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

/// 快捷压缩开始：主窗口可见则返回 false（用应用内进度弹窗）；
/// 否则创建迷你进度窗口并返回 true。
#[tauri::command]
fn begin_quick_job(app: tauri::AppHandle) -> CmdResult<bool> {
    use tauri::Manager;
    let main_visible = app
        .get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false);
    if main_visible {
        return Ok(false);
    }
    if app.get_webview_window("mini").is_none() {
        tauri::WebviewWindowBuilder::new(
            &app,
            "mini",
            tauri::WebviewUrl::App("index.html?mini=1".into()),
        )
        .title("Origami")
        .inner_size(420.0, 148.0)
        .resizable(false)
        .always_on_top(true)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    }
    Ok(true)
}

/// 快捷压缩结束：关闭迷你窗口。主窗口未显示时，成功则退出应用，
/// 失败则改为显示主窗口（让用户看到错误提示）。
#[tauri::command]
fn end_quick_job(
    app: tauri::AppHandle,
    pending: tauri::State<'_, Arc<Pending>>,
    quick: tauri::State<'_, Arc<QuickLaunch>>,
    ok: bool,
) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("mini") {
        let _ = w.close();
    }
    let main_visible = app
        .get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false);
    if main_visible {
        return;
    }
    let has_pending = !pending.0.lock().unwrap().is_empty();
    if ok && !has_pending {
        app.exit(0);
    } else if !ok {
        quick.0.store(false, Ordering::Relaxed);
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

#[tauri::command]
fn default_extract_dir(path: String) -> String {
    PathBuf::from(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // 单实例：第二次启动（关联文件/右键菜单）把参数转发给已运行的主实例。
    // macOS 走 RunEvent::Opened，无需此插件。需在其它插件之前注册。
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let actions = cli::parse_args(argv.get(1..).unwrap_or(&[]));
            dispatch_actions(app, actions);
        }));
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 首次启动若带参数（Windows/Linux 关联打开或右键压缩），在此投递。
            #[cfg(not(target_os = "macos"))]
            {
                use tauri::Manager;
                let args: Vec<String> = std::env::args().skip(1).collect();
                let actions = cli::parse_args(&args);
                dispatch_actions(app.handle(), actions);
            }
            let _ = app;
            Ok(())
        })
        .manage(Arc::new(Jobs::default()))
        .manage(Arc::new(Pending::default()))
        .manage(Arc::new(QuickLaunch::default()))
        .invoke_handler(tauri::generate_handler![
            list_archive,
            extract_archive,
            create_archive,
            archive_add,
            archive_remove,
            test_archive,
            preview_entry,
            cancel_job,
            system_icon,
            pw_list,
            pw_add,
            pw_remove,
            system_auth_available,
            system_auth,
            extract_entry_to_temp,
            default_extract_dir,
            default_create_dest,
            take_pending_actions,
            frontend_ready,
            begin_quick_job,
            end_quick_job,
            install_shell_menu,
            uninstall_shell_menu,
            shell_menu_installed,
            app_platform,
            file_assoc_supported,
            file_assoc_list,
            file_assoc_set,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = event {
                let mut actions = Vec::new();
                let mut file_paths = Vec::new();
                for u in &urls {
                    if let Some(a) = parse_deep_link(u) {
                        actions.push(a);
                    } else if let Ok(p) = u.to_file_path() {
                        file_paths.push(p.to_string_lossy().to_string());
                    }
                }
                if !file_paths.is_empty() {
                    actions.push(PendingAction::Open { paths: file_paths });
                }
                dispatch_actions(app, actions);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
