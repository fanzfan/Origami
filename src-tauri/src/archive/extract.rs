use super::{detect_format, inner_name, sanitize_rel_path, Format};
use crate::encoding::decode_name;
use anyhow::Context;
use serde::Serialize;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub job_id: String,
    pub current: u64,
    pub total: u64,
    pub file: String,
    pub done: bool,
}

pub struct ExtractOptions {
    pub job_id: String,
    pub password: Option<String>,
    pub fallback_passwords: Vec<String>,
    pub encoding: String,
    /// Selected entry paths (decoded). Empty = all.
    pub entries: Vec<String>,
    /// Smart mode: create a subfolder unless the archive already has a single root.
    pub smart: bool,
}

pub struct Ctx<'a, R: tauri::Runtime> {
    pub app: &'a tauri::AppHandle<R>,
    pub cancel: Arc<AtomicBool>,
}

impl<R: tauri::Runtime> Ctx<'_, R> {
    fn emit(&self, p: &Progress) {
        let _ = self.app.emit("job-progress", p);
    }
}

/// 字节级进度跟踪器，线程安全，发射限频 ~80ms。current/total 单位为字节。
pub struct Tracker<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    cancel: Arc<AtomicBool>,
    job_id: String,
    total: AtomicU64,
    current: AtomicU64,
    file: Mutex<String>,
    last: Mutex<Instant>,
}

impl<R: tauri::Runtime> Tracker<R> {
    pub fn new(ctx: &Ctx<R>, job_id: &str, total: u64) -> Arc<Self> {
        Arc::new(Self {
            app: ctx.app.clone(),
            cancel: ctx.cancel.clone(),
            job_id: job_id.to_string(),
            total: AtomicU64::new(total),
            current: AtomicU64::new(0),
            file: Mutex::new(String::new()),
            last: Mutex::new(Instant::now() - Duration::from_secs(1)),
        })
    }

    pub fn check(&self) -> anyhow::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            anyhow::bail!("CANCELLED");
        }
        Ok(())
    }

    pub fn set_file(&self, name: &str) {
        *self.file.lock().unwrap() = name.to_string();
        self.emit_throttled();
    }

    pub fn add(&self, n: u64) -> anyhow::Result<()> {
        self.current.fetch_add(n, Ordering::Relaxed);
        self.check()?;
        self.emit_throttled();
        Ok(())
    }

    fn emit_throttled(&self) {
        {
            let mut last = self.last.lock().unwrap();
            if last.elapsed() < Duration::from_millis(80) {
                return;
            }
            *last = Instant::now();
        }
        let _ = self.app.emit(
            "job-progress",
            &Progress {
                job_id: self.job_id.clone(),
                current: self.current.load(Ordering::Relaxed),
                total: self.total.load(Ordering::Relaxed),
                file: self.file.lock().unwrap().clone(),
                done: false,
            },
        );
    }

    pub fn done(&self) {
        let t = self.total.load(Ordering::Relaxed);
        let _ = self.app.emit(
            "job-progress",
            &Progress {
                job_id: self.job_id.clone(),
                current: t,
                total: t,
                file: String::new(),
                done: true,
            },
        );
    }
}

/// 包装 Read，把读取的字节数计入 Tracker（取消时返回 io 错误，消息为 CANCELLED）。
pub struct CountingReader<Rd, R: tauri::Runtime> {
    inner: Rd,
    tracker: Arc<Tracker<R>>,
}

impl<Rd: Read, R: tauri::Runtime> CountingReader<Rd, R> {
    pub fn new(inner: Rd, tracker: Arc<Tracker<R>>) -> Self {
        Self { inner, tracker }
    }
}

impl<Rd: Read, R: tauri::Runtime> Read for CountingReader<Rd, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.tracker
            .add(n as u64)
            .map_err(|_| std::io::Error::other("CANCELLED"))?;
        Ok(n)
    }
}

pub(crate) fn selected(entries: &[String], path: &str) -> bool {
    if entries.is_empty() {
        return true;
    }
    entries.iter().any(|sel| {
        path == sel || path.starts_with(&format!("{}/", sel.trim_end_matches('/')))
    })
}

/// Decide the real output dir. Smart mode: if all entries share a single root
/// component, extract as-is; otherwise wrap in a folder named after the archive.
fn resolve_dest(archive: &Path, dest: &Path, smart: bool, roots: &[String]) -> PathBuf {
    if !smart {
        return dest.to_path_buf();
    }
    let mut top: Option<&str> = None;
    let mut single = true;
    for r in roots {
        let first = r.split('/').next().unwrap_or("");
        match top {
            None => top = Some(first),
            Some(t) if t == first => {}
            _ => {
                single = false;
                break;
            }
        }
    }
    if single && top.is_some() {
        dest.to_path_buf()
    } else {
        let stem = archive_stem(archive);
        dest.join(stem)
    }
}

pub fn archive_stem(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("archive");
    let lower = name.to_lowercase();
    for ext in [
        ".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tgz", ".tbz2", ".txz", ".tzst", ".zip",
        ".7z", ".rar", ".tar", ".gz", ".bz2", ".xz", ".zst", ".jar", ".apk",
    ] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

fn set_mtime(path: &Path, mtime: Option<i64>) {
    if let Some(m) = mtime {
        let t = filetime::FileTime::from_unix_time(m, 0);
        let _ = filetime::set_file_mtime(path, t);
    }
}

fn write_streamed<R: tauri::Runtime>(
    t: &Tracker<R>,
    reader: &mut dyn Read,
    out_path: &Path,
) -> anyhow::Result<u64> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::io::BufWriter::new(File::create(out_path)?);
    let mut buf = [0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        written += n as u64;
        t.add(n as u64)?;
    }
    out.flush()?;
    Ok(written)
}

pub fn extract(
    ctx: &Ctx<impl tauri::Runtime>,
    archive_path: &Path,
    dest: &Path,
    opts: &ExtractOptions,
) -> anyhow::Result<String> {
    let format = detect_format(archive_path)?;
    let final_dest = match format {
        Format::Zip => extract_zip(ctx, archive_path, dest, opts)?,
        Format::SevenZ => extract_7z(ctx, archive_path, dest, opts)?,
        Format::Rar => extract_rar(ctx, archive_path, dest, opts)?,
        Format::Tar | Format::TarGz | Format::TarBz2 | Format::TarXz | Format::TarZst => {
            // 进度按已消耗的压缩字节计：total = 包文件大小
            let t = Tracker::new(ctx, &opts.job_id, std::fs::metadata(archive_path)?.len());
            let raw = std::io::BufReader::new(CountingReader::new(
                BufReader::new(File::open(archive_path)?),
                t.clone(),
            ));
            let r: Box<dyn Read + Send> = if format == Format::Tar {
                Box::new(raw)
            } else {
                super::list::decompress_stream(Box::new(raw), format)?
            };
            extract_tar(&t, r, archive_path, dest, opts)?
        }
        Format::Gz | Format::Bz2 | Format::Xz | Format::Zst => {
            let t = Tracker::new(ctx, &opts.job_id, std::fs::metadata(archive_path)?.len());
            t.set_file(&inner_name(archive_path));
            let raw = std::io::BufReader::new(CountingReader::new(
                BufReader::new(File::open(archive_path)?),
                t.clone(),
            ));
            let mut r = super::list::decompress_stream(Box::new(raw), format)?;
            let out = dest.join(inner_name(archive_path));
            copy_plain(&mut r, &out)?;
            dest.to_path_buf()
        }
    };
    ctx.emit(&Progress {
        job_id: opts.job_id.clone(),
        current: 1,
        total: 1,
        file: String::new(),
        done: true,
    });
    Ok(final_dest.to_string_lossy().to_string())
}

fn copy_plain(reader: &mut dyn Read, out_path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::io::BufWriter::new(File::create(out_path)?);
    std::io::copy(reader, &mut out)?;
    out.flush()?;
    Ok(())
}

// ---------------- ZIP ----------------

fn extract_zip(
    ctx: &Ctx<impl tauri::Runtime>,
    archive_path: &Path,
    dest: &Path,
    opts: &ExtractOptions,
) -> anyhow::Result<PathBuf> {
    let file = File::open(archive_path)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file))?;

    // Collect decoded names + encrypted flag + size first.
    let mut names: Vec<(String, bool, u64)> = Vec::with_capacity(zip.len());
    let mut any_encrypted = false;
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i)?;
        let name = decode_name(f.name_raw(), &opts.encoding);
        any_encrypted |= f.encrypted();
        names.push((name, f.encrypted(), f.size()));
    }

    // Find a working password if needed.
    let mut password: Option<String> = None;
    if any_encrypted {
        let mut candidates: Vec<Option<String>> = Vec::new();
        if opts.password.is_some() {
            candidates.push(opts.password.clone());
        }
        for p in &opts.fallback_passwords {
            candidates.push(Some(p.clone()));
        }
        let test_idx = names.iter().position(|(_, enc, _)| *enc).unwrap_or(0);
        let mut ok = false;
        for cand in candidates {
            let Some(pw) = cand else { continue };
            let res = zip.by_index_decrypt(test_idx, pw.as_bytes());
            match res {
                Ok(mut f) => {
                    let mut sink = [0u8; 512];
                    if f.read(&mut sink).is_ok() {
                        password = Some(pw);
                        ok = true;
                        break;
                    }
                }
                Err(_) => continue,
            }
        }
        if !ok {
            anyhow::bail!("PASSWORD_REQUIRED");
        }
    }

    let roots: Vec<String> = names
        .iter()
        .map(|(n, _, _)| n.clone())
        .filter(|n| selected(&opts.entries, n))
        .collect();
    let out_base = resolve_dest(archive_path, dest, opts.smart, &roots);
    let total_bytes: u64 = names
        .iter()
        .filter(|(n, _, _)| selected(&opts.entries, n))
        .map(|(_, _, s)| *s)
        .sum();
    let t = Tracker::new(ctx, &opts.job_id, total_bytes);

    for i in 0..zip.len() {
        t.check()?;
        let (name, encrypted, _) = names[i].clone();
        if !selected(&opts.entries, &name) {
            continue;
        }
        let Some(rel) = sanitize_rel_path(&name) else { continue };
        let out_path = out_base.join(rel);
        t.set_file(&name);

        let mut f = if encrypted {
            zip.by_index_decrypt(i, password.as_deref().unwrap_or("").as_bytes())?
        } else {
            zip.by_index(i)?
        };

        if f.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        if f.is_symlink() {
            let mut target = Vec::new();
            f.read_to_end(&mut target)?;
            let target = String::from_utf8_lossy(&target).to_string();
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let _ = std::fs::remove_file(&out_path);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &out_path)?;
            // Windows 创建符号链接需特权；失败则把目标路径写成普通文件，避免解压中断。
            #[cfg(windows)]
            if std::os::windows::fs::symlink_file(&target, &out_path).is_err() {
                std::fs::write(&out_path, target.as_bytes())?;
            }
            continue;
        }
        write_streamed(&t, &mut f, &out_path)?;
        #[cfg(unix)]
        if let Some(mode) = f.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode));
        }
        let mtime = f.last_modified().and_then(super::list::zipdt_to_unix);
        set_mtime(&out_path, mtime);
    }
    Ok(out_base)
}

// ---------------- 7Z ----------------

fn extract_7z(
    ctx: &Ctx<impl tauri::Runtime>,
    archive_path: &Path,
    dest: &Path,
    opts: &ExtractOptions,
) -> anyhow::Result<PathBuf> {
    use sevenz_rust2::{ArchiveReader, Password};

    let mut candidates: Vec<Option<String>> = vec![opts.password.clone()];
    for p in &opts.fallback_passwords {
        candidates.push(Some(p.clone()));
    }

    let mut reader = None;
    let mut last_err = anyhow::anyhow!("打开 7z 失败");
    for cand in candidates {
        let password = cand
            .as_deref()
            .map(Password::from)
            .unwrap_or_else(Password::empty);
        match ArchiveReader::open(archive_path, password) {
            Ok(r) => {
                reader = Some((r, cand));
                break;
            }
            Err(e) => {
                last_err = anyhow::anyhow!("PASSWORD_REQUIRED: {e}");
            }
        }
    }
    let (mut reader, _used) = reader.ok_or(last_err)?;

    let roots: Vec<String> = reader
        .archive()
        .files
        .iter()
        .map(|f| f.name.replace('\\', "/"))
        .filter(|n| selected(&opts.entries, n))
        .collect();
    let out_base = resolve_dest(archive_path, dest, opts.smart, &roots);
    let total_bytes: u64 = reader
        .archive()
        .files
        .iter()
        .filter(|f| selected(&opts.entries, &f.name.replace('\\', "/")))
        .map(|f| f.size)
        .sum();
    let t = Tracker::new(ctx, &opts.job_id, total_bytes);
    let mut failed: Option<anyhow::Error> = None;

    reader.for_each_entries(|entry, r| {
        let name = entry.name.replace('\\', "/");
        if !selected(&opts.entries, &name) {
            // Must drain reader? for_each gives reader positioned per entry; skipping read is fine.
            return Ok(true);
        }
        let inner = (|| -> anyhow::Result<()> {
            t.check()?;
            let Some(rel) = sanitize_rel_path(&name) else { return Ok(()) };
            let out_path = out_base.join(rel);
            if entry.is_directory {
                std::fs::create_dir_all(&out_path)?;
                return Ok(());
            }
            t.set_file(&name);
            write_streamed(&t, r, &out_path)?;
            set_mtime(&out_path, super::list::nt_to_unix(entry.last_modified_date.into()));
            Ok(())
        })();
        match inner {
            Ok(()) => Ok(true),
            Err(e) => {
                failed = Some(e);
                Ok(false)
            }
        }
    })?;
    if let Some(e) = failed {
        return Err(e);
    }
    Ok(out_base)
}

// ---------------- RAR ----------------

fn extract_rar(
    ctx: &Ctx<impl tauri::Runtime>,
    archive_path: &Path,
    dest: &Path,
    opts: &ExtractOptions,
) -> anyhow::Result<PathBuf> {
    let mut candidates: Vec<Option<String>> = vec![opts.password.clone()];
    for p in &opts.fallback_passwords {
        candidates.push(Some(p.clone()));
    }

    // Pre-list to compute roots for smart mode.
    let list_opts = super::list::ListOptions {
        password: opts.password.clone(),
        encoding: opts.encoding.clone(),
        fallback_passwords: opts.fallback_passwords.clone(),
    };
    let info = super::list::list(archive_path, &list_opts)?;
    let roots: Vec<String> = info
        .entries
        .iter()
        .map(|e| e.path.clone())
        .filter(|n| selected(&opts.entries, n))
        .collect();
    let out_base = resolve_dest(archive_path, dest, opts.smart, &roots);
    let total_bytes: u64 = info
        .entries
        .iter()
        .filter(|e| selected(&opts.entries, &e.path))
        .map(|e| e.size)
        .sum();
    let t = Tracker::new(ctx, &opts.job_id, total_bytes);

    let mut last_err: Option<anyhow::Error> = None;
    for cand in candidates {
        let result = (|| -> Result<(), unrar::error::UnrarError> {
            let archive = match &cand {
                Some(p) => unrar::Archive::with_password(archive_path, p),
                None => unrar::Archive::new(archive_path),
            };
            let mut open = archive.open_for_processing()?;
            loop {
                if ctx.cancel.load(Ordering::Relaxed) {
                    break;
                }
                let Some(cursor) = open.read_header()? else { break };
                let name = cursor.entry().filename.to_string_lossy().replace('\\', "/");
                if selected(&opts.entries, &name) {
                    let size = cursor.entry().unpacked_size;
                    t.set_file(&name);
                    open = cursor.extract_with_base(&out_base)?;
                    let _ = t.add(size);
                } else {
                    open = cursor.skip()?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                if ctx.cancel.load(Ordering::Relaxed) {
                    anyhow::bail!("CANCELLED");
                }
                return Ok(out_base);
            }
            Err(e) => {
                use unrar::error::Code;
                if matches!(e.code, Code::MissingPassword | Code::BadPassword) {
                    last_err = Some(anyhow::anyhow!("PASSWORD_REQUIRED"));
                    continue;
                }
                return Err(anyhow::anyhow!("RAR 解压失败: {:?}", e.code));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("RAR 解压失败")))
}

// ---------------- TAR ----------------

fn extract_tar(
    t: &Tracker<impl tauri::Runtime>,
    reader: Box<dyn Read + Send>,
    archive_path: &Path,
    dest: &Path,
    opts: &ExtractOptions,
) -> anyhow::Result<PathBuf> {
    // First pass for smart mode requires re-reading; to avoid double decompression,
    // when smart mode is on we wrap in a folder unless the tar starts with a single dir.
    let mut ar = tar::Archive::new(reader);
    ar.set_preserve_permissions(true);
    ar.set_preserve_mtime(true);

    let out_base = if opts.smart {
        // Cheap heuristic: peek the listing from a fresh decompressor.
        let list_opts = super::list::ListOptions {
            password: None,
            encoding: opts.encoding.clone(),
            fallback_passwords: Vec::new(),
        };
        let info = super::list::list(archive_path, &list_opts)?;
        let roots: Vec<String> = info
            .entries
            .iter()
            .map(|e| e.path.clone())
            .filter(|n| selected(&opts.entries, n))
            .collect();
        resolve_dest(archive_path, dest, true, &roots)
    } else {
        dest.to_path_buf()
    };
    std::fs::create_dir_all(&out_base)?;

    for entry in ar.entries()? {
        t.check()?;
        let mut entry = entry.context("读取 tar 条目失败")?;
        let raw = entry.path_bytes().to_vec();
        let name = decode_name(&raw, &opts.encoding);
        if !selected(&opts.entries, &name) {
            continue;
        }
        t.set_file(&name);
        entry.unpack_in(&out_base).context("解包失败")?;
    }
    Ok(out_base)
}

// ---------------- TEST ----------------

pub fn test_archive(
    ctx: &Ctx<impl tauri::Runtime>,
    archive_path: &Path,
    job_id: &str,
    password: Option<String>,
    fallbacks: Vec<String>,
) -> anyhow::Result<()> {
    let format = detect_format(archive_path)?;
    let result = match format {
        Format::Zip => {
            let file = File::open(archive_path)?;
            let mut zip = zip::ZipArchive::new(BufReader::new(file))?;
            let mut total = 0u64;
            for i in 0..zip.len() {
                total += zip.by_index_raw(i)?.size();
            }
            let t = Tracker::new(ctx, job_id, total);
            for i in 0..zip.len() {
                t.check()?;
                let raw = zip.by_index_raw(i)?;
                let encrypted = raw.encrypted();
                let name = decode_name(raw.name_raw(), "auto");
                drop(raw);
                t.set_file(&name);
                let mut f = if encrypted {
                    let mut ok = None;
                    let mut cands = Vec::new();
                    if let Some(p) = &password {
                        cands.push(p.clone());
                    }
                    cands.extend(fallbacks.iter().cloned());
                    for pw in cands {
                        if zip.by_index_decrypt(i, pw.as_bytes()).is_ok() {
                            ok = Some(pw);
                            break;
                        }
                    }
                    let pw = ok.ok_or_else(|| anyhow::anyhow!("PASSWORD_REQUIRED"))?;
                    zip.by_index_decrypt(i, pw.as_bytes())?
                } else {
                    zip.by_index(i)?
                };
                let mut buf = [0u8; 64 * 1024];
                loop {
                    match f.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => t.add(n as u64)?,
                        Err(e) => anyhow::bail!("校验失败: {e}"),
                    }
                }
            }
            Ok(())
        }
        Format::SevenZ => {
            use sevenz_rust2::{ArchiveReader, Password};
            let mut cands: Vec<Option<String>> = vec![password.clone()];
            cands.extend(fallbacks.iter().cloned().map(Some));
            let mut last = anyhow::anyhow!("测试失败");
            let mut ok = false;
            for cand in cands {
                let pw = cand
                    .as_deref()
                    .map(Password::from)
                    .unwrap_or_else(Password::empty);
                match ArchiveReader::open(archive_path, pw) {
                    Ok(mut r) => {
                        let total: u64 = r.archive().files.iter().map(|f| f.size).sum();
                        let t = Tracker::new(ctx, job_id, total);
                        let res = r.for_each_entries(|entry, reader| {
                            t.set_file(&entry.name.replace('\\', "/"));
                            let mut buf = [0u8; 64 * 1024];
                            loop {
                                match reader.read(&mut buf) {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        if t.add(n as u64).is_err() {
                                            return Err(sevenz_rust2::Error::Io(
                                                std::io::Error::other("CANCELLED"),
                                                "".into(),
                                            ));
                                        }
                                    }
                                    Err(e) => return Err(sevenz_rust2::Error::Io(e, "".into())),
                                }
                            }
                            Ok(true)
                        });
                        match res {
                            Ok(()) => {
                                ok = true;
                                break;
                            }
                            Err(e) => last = anyhow::anyhow!("校验失败: {e}"),
                        }
                    }
                    Err(e) => last = anyhow::anyhow!("PASSWORD_REQUIRED: {e}"),
                }
            }
            if ok { Ok(()) } else { Err(last) }
        }
        Format::Rar => {
            let mut cands: Vec<Option<String>> = vec![password.clone()];
            cands.extend(fallbacks.iter().cloned().map(Some));
            let mut last = anyhow::anyhow!("测试失败");
            let mut ok = false;
            let list_opts = super::list::ListOptions {
                password: password.clone(),
                encoding: "auto".into(),
                fallback_passwords: fallbacks.clone(),
            };
            let total: u64 = super::list::list(archive_path, &list_opts)
                .map(|i| i.entries.iter().map(|e| e.size).sum())
                .unwrap_or(0);
            let t = Tracker::new(ctx, job_id, total);
            for cand in cands {
                let result = (|| -> Result<(), unrar::error::UnrarError> {
                    let archive = match &cand {
                        Some(p) => unrar::Archive::with_password(archive_path, p),
                        None => unrar::Archive::new(archive_path),
                    };
                    let mut open = archive.open_for_processing()?;
                    loop {
                        let Some(cursor) = open.read_header()? else { break };
                        let name = cursor.entry().filename.to_string_lossy().replace('\\', "/");
                        let size = cursor.entry().unpacked_size;
                        t.set_file(&name);
                        open = cursor.test()?;
                        let _ = t.add(size);
                    }
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        ok = true;
                        break;
                    }
                    Err(e) => {
                        use unrar::error::Code;
                        if matches!(e.code, Code::MissingPassword | Code::BadPassword) {
                            last = anyhow::anyhow!("PASSWORD_REQUIRED");
                        } else {
                            last = anyhow::anyhow!("校验失败: {:?}", e.code);
                        }
                    }
                }
            }
            if ok { Ok(()) } else { Err(last) }
        }
        _ => {
            // Streamed formats: read through, progress = consumed compressed bytes.
            let t = Tracker::new(ctx, job_id, std::fs::metadata(archive_path)?.len());
            t.set_file(&inner_name(archive_path));
            let raw = BufReader::new(CountingReader::new(
                BufReader::new(File::open(archive_path)?),
                t.clone(),
            ));
            let mut r: Box<dyn Read + Send> = if format == Format::Tar {
                Box::new(raw)
            } else {
                super::list::decompress_stream(Box::new(raw), format)?
            };
            let mut buf = [0u8; 64 * 1024];
            loop {
                match r.read(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(e) => anyhow::bail!("校验失败: {e}"),
                }
            }
            Ok(())
        }
    };
    if result.is_ok() {
        ctx.emit(&Progress {
            job_id: job_id.to_string(),
            current: 1,
            total: 1,
            file: String::new(),
            done: true,
        });
    }
    result
}
