use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPassword {
    pub password: String,
    pub label: Option<String>,
    pub added_at: u64,
    pub last_used: Option<u64>,
}

fn store_path(app: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_data_dir()?;
    fs::create_dir_all(&dir)?;
    Ok(dir.join("passwords.json"))
}

pub fn load(app: &tauri::AppHandle) -> Vec<SavedPassword> {
    store_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &tauri::AppHandle, list: &[SavedPassword]) -> anyhow::Result<()> {
    let p = store_path(app)?;
    fs::write(p, serde_json::to_string_pretty(list)?)?;
    Ok(())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn add(app: &tauri::AppHandle, password: String, label: Option<String>) -> anyhow::Result<()> {
    let mut list = load(app);
    if let Some(existing) = list.iter_mut().find(|e| e.password == password) {
        existing.label = label.or(existing.label.take());
    } else {
        list.push(SavedPassword { password, label, added_at: now(), last_used: None });
    }
    save(app, &list)
}

pub fn remove(app: &tauri::AppHandle, password: &str) -> anyhow::Result<()> {
    let mut list = load(app);
    list.retain(|e| e.password != password);
    save(app, &list)
}

pub fn mark_used(app: &tauri::AppHandle, password: &str) {
    let mut list = load(app);
    if let Some(e) = list.iter_mut().find(|e| e.password == password) {
        e.last_used = Some(now());
        let _ = save(app, &list);
    }
}

/// Candidate passwords to try, most-recently-used first.
pub fn candidates(app: &tauri::AppHandle) -> Vec<String> {
    let mut list = load(app);
    list.sort_by_key(|e| std::cmp::Reverse(e.last_used.unwrap_or(e.added_at)));
    list.into_iter().map(|e| e.password).collect()
}
