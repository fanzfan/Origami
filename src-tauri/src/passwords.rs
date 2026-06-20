use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// 惰性、按需读取的已存密码集合：只有真正需要试密码（打开/解压加密归档失败回退）时
/// 才会去读系统凭据库，且整次操作内只读一次（memoized）。避免「每次打开归档都弹钥匙串」。
#[derive(Clone)]
pub struct LazyPasswords(Arc<LazyInner>);

struct LazyInner {
    provider: Box<dyn Fn() -> Vec<String> + Send + Sync>,
    cache: OnceLock<Vec<String>>,
}

impl LazyPasswords {
    pub fn new(f: impl Fn() -> Vec<String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(LazyInner { provider: Box::new(f), cache: OnceLock::new() }))
    }
    /// 空集合（无回退密码）。
    pub fn none() -> Self {
        Self::new(Vec::new)
    }
    /// 读取并缓存——首次调用才真正执行 provider（可能读凭据库）。
    pub fn get(&self) -> &[String] {
        self.0.cache.get_or_init(|| (self.0.provider)())
    }
}

/// 系统凭据库里的 service 名（每条密码用唯一 id 作为 account）。
const KR_SERVICE: &str = "dev.vela.origami.passwords";

/// 列表展示用的元数据：**不含明文**，因此读取它不会触碰系统凭据库（不弹钥匙串）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PwMeta {
    pub id: String,
    pub label: Option<String>,
    pub added_at: u64,
    pub last_used: Option<u64>,
}

/// 显示明文时按需返回（仅在用户主动「显示密码」时调用，会读凭据库）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealedPassword {
    pub id: String,
    pub password: String,
}

/// 磁盘索引条目：**不含密码明文**，密码存于系统凭据库。
/// - `fp`：明文指纹（FNV-1a），用于去重 / 标记使用，避免为此读取凭据库。
/// - 兼容旧版明文文件：旧条目无 `id`、含 `password`，load 时自动迁移。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    #[serde(default)]
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fp: Option<String>,
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

/// 明文指纹（FNV-1a 64-bit）。仅用于去重/匹配，跨进程稳定、不可逆、不落明文。
fn fingerprint(password: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in password.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
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

/// 把旧版明文条目迁移进系统凭据库，补齐缺失的 id/fp；有改动则回写索引。
/// 只有存在旧版明文（`password` 字段）时才会写凭据库，正常 id 化的索引此函数为纯文件操作（不弹钥匙串）。
fn migrate(app: &tauri::AppHandle, idx: &mut Vec<IndexEntry>) {
    let mut changed = false;
    for e in idx.iter_mut() {
        if e.id.is_empty() {
            e.id = new_id();
            changed = true;
        }
        if let Some(pw) = e.password.take() {
            let _ = kr_set(&e.id, &pw);
            if e.fp.is_none() {
                e.fp = Some(fingerprint(&pw));
            }
            changed = true;
        }
    }
    if changed {
        let _ = save_index(app, idx);
    }
}

/// 列表元数据（不含明文，不读凭据库）。
pub fn list_meta(app: &tauri::AppHandle) -> Vec<PwMeta> {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    idx.into_iter()
        .map(|e| PwMeta { id: e.id, label: e.label, added_at: e.added_at, last_used: e.last_used })
        .collect()
}

/// 读取全部明文（仅在用户主动「显示密码」时调用，会逐条读凭据库）。
pub fn reveal_all(app: &tauri::AppHandle) -> Vec<RevealedPassword> {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    idx.into_iter()
        .filter_map(|e| kr_get(&e.id).map(|password| RevealedPassword { id: e.id, password }))
        .collect()
}

pub fn add(app: &tauri::AppHandle, password: String, label: Option<String>) -> anyhow::Result<()> {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    let fp = fingerprint(&password);
    // 按指纹去重（不读凭据库）：已存在则只更新备注。
    if let Some(slot) = idx.iter_mut().find(|e| e.fp.as_deref() == Some(fp.as_str())) {
        slot.label = label.or(slot.label.take());
    } else {
        let id = new_id();
        kr_set(&id, &password)?;
        idx.push(IndexEntry { id, password: None, fp: Some(fp), label, added_at: now(), last_used: None });
    }
    save_index(app, &idx)
}

pub fn remove(app: &tauri::AppHandle, id: &str) -> anyhow::Result<()> {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    let mut kept = Vec::with_capacity(idx.len());
    for e in idx {
        if e.id == id {
            kr_del(&e.id);
        } else {
            kept.push(e);
        }
    }
    save_index(app, &kept)
}

/// 解压成功后标记某明文最近使用（按指纹匹配，不读凭据库）。
pub fn mark_used(app: &tauri::AppHandle, password: &str) {
    let fp = fingerprint(password);
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    if let Some(e) = idx.iter_mut().find(|e| e.fp.as_deref() == Some(fp.as_str())) {
        e.last_used = Some(now());
        let _ = save_index(app, &idx);
    }
}

/// 按用户给定的 id 顺序重排索引（不读凭据库）；未列出的条目保持相对顺序追加到末尾。
pub fn reorder(app: &tauri::AppHandle, order: &[String]) -> anyhow::Result<()> {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    let rank = |e: &IndexEntry| order.iter().position(|o| o == &e.id).unwrap_or(usize::MAX);
    idx.sort_by_key(rank);
    save_index(app, &idx)
}

/// 候选密码，按列表（用户可拖动调整的）顺序尝试。会读凭据库——仅用于打开/解压加密归档时自动试密码。
pub fn candidates(app: &tauri::AppHandle) -> Vec<String> {
    let mut idx = load_index(app);
    migrate(app, &mut idx);
    idx.into_iter().filter_map(|e| kr_get(&e.id)).collect()
}
