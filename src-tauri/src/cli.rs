//! 命令行参数解析（Windows / Linux 启动与单实例转发用）。
//!
//! 资源管理器双击关联文件 → `Origami.exe "C:\path\file.zip"`（打开）。
//! 经典右键菜单压缩 → `Origami.exe --compress=zip "C:\f1" "C:\f2"`（快捷压缩）。
//! 也接受 `origami://create?...` 深链字符串，与 macOS 流程保持一致。

use crate::{parse_deep_link, PendingAction};

pub fn parse_args(args: &[String]) -> Vec<PendingAction> {
    let mut format: Option<String> = None;
    let mut paths: Vec<String> = Vec::new();
    let mut out: Vec<PendingAction> = Vec::new();

    for a in args {
        if let Some(rest) = a.strip_prefix("--compress=") {
            format = Some(rest.to_string());
        } else if a == "--compress" {
            format = Some("ask".into());
        } else if a.starts_with("origami://") {
            if let Ok(u) = tauri::Url::parse(a) {
                if let Some(act) = parse_deep_link(&u) {
                    out.push(act);
                }
            }
        } else if !a.starts_with("--") {
            paths.push(a.clone());
        }
    }

    if !paths.is_empty() {
        match format {
            Some(f) => out.push(PendingAction::Create { format: f, paths }),
            None => out.push(PendingAction::Open { paths }),
        }
    }
    out
}
