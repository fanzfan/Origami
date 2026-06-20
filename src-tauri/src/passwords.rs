use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 系统凭据库里的 service 名（每条密码用唯一 id 作为 account）。
const KR_SERVICE: &str = "dev.vela.origami.passwords";

/// 返回给前端的密码条目（明文仅在内存中重建，落盘只存索引）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPassword {
    pub password: String,
    pub label: Option<String>,
    pub added_at: u64,
    pub last_used: Option<u64>,
}

/// 磁盘索引条目：**不含密码明文**，密码存于系统凭据库。
/// 兼容旧版明文文件：旧条目无 `id`、含 `password`，load 时自动迁移。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    #[serde(default)]
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    label: Option<String>,
    added_at: u64,
    last_used: Option<u64>,
}

fn store_path(app: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_data_dir()?;
    fs::create_dir_all(&dir)?;
    Ok(dir.join("passwords.json"))
}

fn load_index(app: &tauri::AppHandle) -> Vec<IndexEntry> {
    store_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_index(app: &tauri::AppHandle, idx: &[IndexEntry]) -> anyhow::Result<()> {
    let p = store_path(app)?;
    fs::write(p, serde_json::to_string_pretty(idx)?)?;
    Ok(())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("pw-{nanos:x}-{c:x}")
}

fn kr_entry(id: &str) -> anyhow::Result<keyring::Entry> {
    Ok(keyring::Entry::new(KR_SERVICE, id)?)
}

fn kr_get(id: &str) -> Option<String> {
    kr_entry(id).ok().and_then(|e| e.get_password().ok())
}

fn kr_set(id: &str, password: &str) -> anyhow::Result<()> {
    kr_entry(id)?.set_password(password)?;
    Ok(())
}

fn kr_del(id: &str) {
    if let Ok(e) = kr_entry(id) {
        let _ = e.delete_credential();
    }
}

/// 把旧版明文条目迁移进系统凭据库，并补齐缺失的 id；有改动则回写索引。
fn migrate(app: &tauri::AppHandle, idx: &mut Vec<IndexEntry>) {
    let mut changed = false;
    for e in idx.iter_mut() {
        if e.id.is_empty() {
            e.id = new_id();
            changed = true;
        }
        if let Some(pw) = e.password.take() {
            let _ = kr_set(&e.id, &pw);
            changed = true;
        }
    }
    if changed {
        let _ = save_index(app, idx);
    }
}

pub fn load(app: &tauri::AppHandle) -> Vec<SavedPassword> {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    idx.into_iter()
        .filter_map(|e| {
            kr_get(&e.id).map(|password| SavedPassword {
                password,
                label: e.label,
                added_at: e.added_at,
                last_used: e.last_used,
            })
        })
        .collect()
}

pub fn add(app: &tauri::AppHandle, password: String, label: Option<String>) -> anyhow::Result<()> {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    // 按明文去重：已存在则只更新备注。
    if let Some(slot) = idx.iter_mut().find(|e| kr_get(&e.id).as_deref() == Some(password.as_str())) {
        slot.label = label.or(slot.label.take());
    } else {
        let id = new_id();
        kr_set(&id, &password)?;
        idx.push(IndexEntry { id, password: None, label, added_at: now(), last_used: None });
    }
    save_index(app, &idx)
}

pub fn remove(app: &tauri::AppHandle, password: &str) -> anyhow::Result<()> {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    let mut kept = Vec::with_capacity(idx.len());
    for e in idx {
        if kr_get(&e.id).as_deref() == Some(password) {
            kr_del(&e.id);
        } else {
            kept.push(e);
        }
    }
    save_index(app, &kept)
}

pub fn mark_used(app: &tauri::AppHandle, password: &str) {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    if let Some(e) = idx.iter_mut().find(|e| kr_get(&e.id).as_deref() == Some(password)) {
        e.last_used = Some(now());
        let _ = save_index(app, &idx);
    }
}

/// 按用户给定的明文顺序重排索引；未列出的条目保持相对顺序追加到末尾。
pub fn reorder(app: &tauri::AppHandle, order: &[String]) -> anyhow::Result<()> {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    let rank = |e: &IndexEntry| {
        kr_get(&e.id)
            .and_then(|pw| order.iter().position(|o| o == &pw))
            .unwrap_or(usize::MAX)
    };
    idx.sort_by_key(rank);
    save_index(app, &idx)
}

/// 候选密码，按列表（用户可拖动调整的）顺序尝试。
pub fn candidates(app: &tauri::AppHandle) -> Vec<String> {
    load(app).into_iter().map(|e| e.password).collect()
}
