use super::extract::{CountingReader, Ctx, Progress, Tracker};
use anyhow::Context;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct CreateOptions {
    pub job_id: String,
    /// "zip" | "7z" | "tar" | "tar.gz" | "tar.bz2" | "tar.xz" | "tar.zst"
    pub format: String,
    /// 0 (store) ..= 9 (best)
    pub level: u32,
    /// 压缩算法。zip: deflate(默认)/bzip2/zstd/xz/ppmd/store；
    /// 7z: lzma2(默认)/bzip2/zstd/ppmd/copy。空串 = 默认。
    pub method: String,
    pub password: Option<String>,
    /// Split the result into volumes of this many bytes (.001/.002…). 0 = off.
    pub volume_size: u64,
    /// 跳过 .DS_Store / __MACOSX / Thumbs.db 等无意义的系统资源文件。
    pub exclude_junk: bool,
}

/// 这些是各操作系统自动生成、压缩时通常无需保留的资源/缓存文件。
/// 命中其路径中任意一段（文件名或目录名）即视为垃圾。
pub(crate) fn is_junk_component(name: &str) -> bool {
    // AppleDouble 资源派生文件（._foo）。
    if let Some(rest) = name.strip_prefix("._") {
        // 排除恰好叫 ".." 之类的边角；".__" 也按资源派生处理。
        let _ = rest;
        return true;
    }
    matches!(
        name,
        ".DS_Store"
            | "__MACOSX"
            | ".Spotlight-V100"
            | ".Trashes"
            | ".fseventsd"
            | ".DocumentRevisions-V100"
            | ".TemporaryItems"
            | ".apdisk"
            | "Thumbs.db"
            | "ehthumbs.db"
            | "desktop.ini"
    )
}

pub(crate) struct SourceFile {
    pub(crate) abs: PathBuf,
    pub(crate) rel: String,
    pub(crate) size: u64,
    pub(crate) is_dir: bool,
    pub(crate) is_symlink: bool,
}

pub(crate) fn collect_sources(
    sources: &[String],
    exclude_junk: bool,
) -> anyhow::Result<Vec<SourceFile>> {
    let mut out = Vec::new();
    for src in sources {
        let p = Path::new(src);
        let meta = std::fs::symlink_metadata(p)
            .with_context(|| format!("无法访问 {src}"))?;
        let base_name = p
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("非法路径 {src}"))?
            .to_string();
        if meta.is_dir() {
            for entry in walkdir::WalkDir::new(p)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| {
                    // 剪掉垃圾目录/文件，连同其子项一起跳过。
                    !exclude_junk
                        || e.file_name()
                            .to_str()
                            .map(|n| !is_junk_component(n))
                            .unwrap_or(true)
                })
            {
                let entry = entry?;
                let rel_inner = entry
                    .path()
                    .strip_prefix(p)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                let rel = if rel_inner.is_empty() {
                    base_name.clone()
                } else {
                    format!("{base_name}/{rel_inner}")
                };
                let ft = entry.file_type();
                let size = if ft.is_file() {
                    entry.metadata().map(|m| m.len()).unwrap_or(0)
                } else {
                    0
                };
                out.push(SourceFile {
                    abs: entry.path().to_path_buf(),
                    rel,
                    size,
                    is_dir: ft.is_dir(),
                    is_symlink: ft.is_symlink(),
                });
            }
        } else if !(exclude_junk && is_junk_component(&base_name)) {
            out.push(SourceFile {
                abs: p.to_path_buf(),
                rel: base_name,
                size: if meta.is_file() { meta.len() } else { 0 },
                is_dir: false,
                is_symlink: meta.file_type().is_symlink(),
            });
        }
    }
    Ok(out)
}

fn worker_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

pub fn create(
    ctx: &Ctx<impl tauri::Runtime>,
    dest: &Path,
    sources: &[String],
    opts: &CreateOptions,
) -> anyhow::Result<String> {
    let files = collect_sources(sources, opts.exclude_junk)?;
    // 进度按已读入的源文件字节计。
    let total: u64 = files.iter().map(|f| f.size).sum();
    let t = Tracker::new(ctx, &opts.job_id, total);
    match opts.format.as_str() {
        "zip" => create_zip(&t, dest, &files, opts)?,
        "7z" => create_7z(&t, dest, &files, opts)?,
        "tar" | "tar.gz" | "tar.bz2" | "tar.xz" | "tar.zst" => {
            create_tar(&t, dest, &files, opts)?
        }
        f => anyhow::bail!("不支持的压缩格式: {f}"),
    }

    let mut result = dest.to_string_lossy().to_string();
    if opts.volume_size > 0 {
        result = split_volumes(ctx, dest, &opts.job_id, opts.volume_size)?;
    }
    ctx_emit_done(ctx, &opts.job_id);
    Ok(result)
}

fn ctx_emit_done(ctx: &Ctx<impl tauri::Runtime>, job_id: &str) {
    use tauri::Emitter;
    let _ = ctx.app.emit(
        "job-progress",
        &Progress {
            job_id: job_id.to_string(),
            current: 1,
            total: 1,
            file: String::new(),
            done: true,
        },
    );
}

// ---------------- ZIP ----------------

/// 大于该值的文件不参与并行（避免内存爆炸），由主线程串行流式写入。
const ZIP_PAR_MAX: u64 = 32 * 1024 * 1024;

fn zip_method(opts: &CreateOptions) -> zip::CompressionMethod {
    use zip::CompressionMethod as M;
    if opts.level == 0 {
        return M::Stored;
    }
    match opts.method.as_str() {
        "store" => M::Stored,
        "bzip2" => M::Bzip2,
        "zstd" => M::Zstd,
        "xz" => M::Xz,
        "ppmd" => M::Ppmd,
        _ => M::Deflated,
    }
}

fn zip_options(
    f: &SourceFile,
    opts: &CreateOptions,
) -> zip::write::SimpleFileOptions {
    use zip::write::FileOptions;
    let method = zip_method(opts);
    let mut o: zip::write::SimpleFileOptions = FileOptions::default()
        .compression_method(method)
        .unix_permissions(file_mode(&f.abs).unwrap_or(0o644))
        .large_file(true);
    if method != zip::CompressionMethod::Stored {
        o = o.compression_level(Some(opts.level.clamp(1, 9) as i64));
    }
    if let Some(t) = file_mtime_zip(&f.abs) {
        o = o.last_modified_time(t);
    }
    o
}

fn create_zip<R: tauri::Runtime>(
    t: &Arc<Tracker<R>>,
    dest: &Path,
    files: &[SourceFile],
    opts: &CreateOptions,
) -> anyhow::Result<()> {
    let threads = worker_threads();
    // 加密时 AES 条目不宜 raw copy 合并，退回串行。
    if opts.password.is_some() || threads <= 1 {
        return create_zip_serial(t, dest, files, opts);
    }
    create_zip_parallel(t, dest, files, opts, threads)
}

pub(crate) fn zip_write_file_streamed<R: tauri::Runtime, W: Write + std::io::Seek>(
    t: &Arc<Tracker<R>>,
    w: &mut zip::ZipWriter<W>,
    f: &SourceFile,
    opts: &CreateOptions,
) -> anyhow::Result<()> {
    let o = zip_options(f, opts);
    let o = if let Some(pw) = &opts.password {
        o.with_aes_encryption(zip::AesMode::Aes256, pw)
    } else {
        o
    };
    if f.is_symlink {
        let target = std::fs::read_link(&f.abs)?;
        w.add_symlink(&f.rel, target.to_string_lossy(), o)?;
    } else if f.is_dir {
        w.add_directory(&f.rel, o)?;
    } else {
        t.set_file(&f.rel);
        w.start_file(&f.rel, o)?;
        let mut r = BufReader::new(File::open(&f.abs)?);
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n])?;
            t.add(n as u64)?;
        }
    }
    Ok(())
}

fn create_zip_serial<R: tauri::Runtime>(
    t: &Arc<Tracker<R>>,
    dest: &Path,
    files: &[SourceFile],
    opts: &CreateOptions,
) -> anyhow::Result<()> {
    let out = BufWriter::new(File::create(dest)?);
    let mut w = zip::ZipWriter::new(out);
    for f in files {
        t.check()?;
        zip_write_file_streamed(t, &mut w, f, opts)?;
    }
    w.finish()?.flush()?;
    Ok(())
}

/// 并行压缩：工作线程把小文件各自压成内存中的单条目 zip，主线程用
/// raw_copy_file 合并（无需重新压缩）；目录/符号链接/大文件由主线程串行写。
fn create_zip_parallel<R: tauri::Runtime>(
    t: &Arc<Tracker<R>>,
    dest: &Path,
    files: &[SourceFile],
    opts: &CreateOptions,
    threads: usize,
) -> anyhow::Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::sync_channel;

    let par_idx: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.is_dir && !f.is_symlink && f.size <= ZIP_PAR_MAX)
        .map(|(i, _)| i)
        .collect();
    let serial_idx: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, f)| f.is_dir || f.is_symlink || f.size > ZIP_PAR_MAX)
        .map(|(i, _)| i)
        .collect();

    let out = BufWriter::new(File::create(dest)?);
    let mut w = zip::ZipWriter::new(out);

    let next = AtomicUsize::new(0);
    let (tx, rx) = sync_channel::<anyhow::Result<Vec<u8>>>(threads * 2);
    let n_workers = threads.min(par_idx.len().max(1));

    let result: anyhow::Result<()> = std::thread::scope(|s| {
        for _ in 0..n_workers {
            let tx = tx.clone();
            let next = &next;
            let par_idx = &par_idx;
            let t = t.clone();
            s.spawn(move || loop {
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(&i) = par_idx.get(k) else { break };
                let f = &files[i];
                let res = (|| -> anyhow::Result<Vec<u8>> {
                    t.check()?;
                    t.set_file(&f.rel);
                    let cursor = std::io::Cursor::new(Vec::with_capacity(
                        (f.size / 2).min(4 * 1024 * 1024) as usize,
                    ));
                    let mut zw = zip::ZipWriter::new(cursor);
                    zw.start_file(&f.rel, zip_options(f, opts))?;
                    let mut r =
                        CountingReader::new(BufReader::new(File::open(&f.abs)?), t.clone());
                    std::io::copy(&mut r, &mut zw)?;
                    Ok(zw.finish()?.into_inner())
                })();
                let failed = res.is_err();
                if tx.send(res).is_err() || failed {
                    break;
                }
            });
        }
        drop(tx);

        // 主线程先写串行条目（目录/链接/大文件），同时接收并合并并行结果。
        for &i in &serial_idx {
            t.check()?;
            zip_write_file_streamed(t, &mut w, &files[i], opts)?;
        }
        let mut merged = 0usize;
        while merged < par_idx.len() {
            let buf = match rx.recv() {
                Ok(Ok(b)) => b,
                Ok(Err(e)) => return Err(e),
                Err(_) => anyhow::bail!("压缩线程异常退出"),
            };
            let mut ar = zip::ZipArchive::new(std::io::Cursor::new(buf))?;
            let entry = ar.by_index_raw(0)?;
            w.raw_copy_file(entry)?;
            merged += 1;
        }
        Ok(())
    });
    result?;
    w.finish()?.flush()?;
    Ok(())
}

fn file_mode(p: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::symlink_metadata(p).ok().map(|m| m.permissions().mode() & 0o7777)
    }
    // Windows 无 Unix 权限位，归档不写 mode。
    #[cfg(not(unix))]
    {
        let _ = p;
        None
    }
}

fn file_mtime_zip(p: &Path) -> Option<zip::DateTime> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    let odt = time::OffsetDateTime::from(m);
    zip::DateTime::try_from(time::PrimitiveDateTime::new(odt.date(), odt.time())).ok()
}

// ---------------- 7Z ----------------

fn create_7z<R: tauri::Runtime>(
    t: &Arc<Tracker<R>>,
    dest: &Path,
    files: &[SourceFile],
    opts: &CreateOptions,
) -> anyhow::Result<()> {
    use sevenz_rust2::encoder_options::{self, AesEncoderOptions, Lzma2Options};
    use sevenz_rust2::*;

    /// 0-9 等级到 zstd 等级的映射。
    const ZSTD_LEVELS: [u32; 10] = [1, 1, 3, 5, 7, 9, 12, 15, 19, 22];

    let mut w = ArchiveWriter::create(dest).context("创建 7z 失败")?;
    let level = opts.level.clamp(0, 9);
    let threads = worker_threads() as u32;
    let mut methods: Vec<EncoderConfiguration> = Vec::new();
    if let Some(pw) = &opts.password {
        methods.push(AesEncoderOptions::new(Password::from(pw.as_str())).into());
        w.set_encrypt_header(true);
    }
    let content: EncoderConfiguration = match opts.method.as_str() {
        "copy" => EncoderConfiguration::new(EncoderMethod::COPY),
        "bzip2" => encoder_options::Bzip2Options::from_level(level.max(1)).into(),
        "zstd" => encoder_options::ZstandardOptions::from_level(ZSTD_LEVELS[level as usize]).into(),
        "ppmd" => encoder_options::PpmdOptions::from_level(level).into(),
        _ => {
            if threads > 1 {
                Lzma2Options::from_level_mt(level, threads, 0).into()
            } else {
                Lzma2Options::from_level(level).into()
            }
        }
    };
    methods.push(content);
    w.set_content_methods(methods);

    for f in files {
        t.check()?;
        if f.is_dir {
            w.push_archive_entry::<File>(ArchiveEntry::new_directory(&f.rel), None)?;
        } else if f.is_symlink {
            // Store symlink target as file content (7z convention uses attributes; keep simple).
            let target = std::fs::read_link(&f.abs)?;
            let data = target.to_string_lossy().into_owned().into_bytes();
            let entry = ArchiveEntry::new_file(&f.rel);
            w.push_archive_entry(entry, Some(std::io::Cursor::new(data)))?;
        } else {
            t.set_file(&f.rel);
            let entry = ArchiveEntry::from_path(&f.abs, f.rel.clone());
            let file = CountingReader::new(File::open(&f.abs)?, t.clone());
            w.push_archive_entry(entry, Some(file))?;
        }
    }
    w.finish()?;
    Ok(())
}

// ---------------- TAR family ----------------

fn create_tar<R: tauri::Runtime>(
    t: &Arc<Tracker<R>>,
    dest: &Path,
    files: &[SourceFile],
    opts: &CreateOptions,
) -> anyhow::Result<()> {
    let out = BufWriter::new(File::create(dest)?);
    let level = opts.level.clamp(0, 9);
    let threads = worker_threads() as u32;
    let w: Box<dyn Write> = match opts.format.as_str() {
        "tar" => Box::new(out),
        "tar.gz" => Box::new(flate2::write::GzEncoder::new(
            out,
            flate2::Compression::new(level),
        )),
        "tar.bz2" => Box::new(bzip2::write::BzEncoder::new(
            out,
            bzip2::Compression::new(level.max(1)),
        )),
        "tar.xz" => {
            if threads > 1 {
                let stream = liblzma::stream::MtStreamBuilder::new()
                    .threads(threads)
                    .preset(level)
                    .encoder()
                    .context("初始化多线程 xz 失败")?;
                Box::new(liblzma::write::XzEncoder::new_stream(out, stream))
            } else {
                Box::new(liblzma::write::XzEncoder::new(out, level))
            }
        }
        "tar.zst" => {
            let mut enc = zstd::stream::write::Encoder::new(out, (level as i32).max(1))?;
            if threads > 1 {
                let _ = enc.multithread(threads);
            }
            Box::new(enc.auto_finish())
        }
        _ => unreachable!(),
    };
    let mut tar = tar::Builder::new(w);
    tar.follow_symlinks(false);

    for f in files {
        t.check()?;
        if f.is_dir {
            tar.append_dir(&f.rel, &f.abs)?;
        } else if f.is_symlink {
            tar.append_path_with_name(&f.abs, &f.rel)?;
        } else {
            t.set_file(&f.rel);
            let meta = std::fs::metadata(&f.abs)?;
            let mut header = tar::Header::new_gnu();
            header.set_metadata(&meta);
            let r = CountingReader::new(BufReader::new(File::open(&f.abs)?), t.clone());
            tar.append_data(&mut header, &f.rel, r)?;
        }
    }
    let w = tar.into_inner()?;
    drop(w); // flush encoders
    Ok(())
}

// ---------------- volume split ----------------

fn split_volumes(
    ctx: &Ctx<impl tauri::Runtime>,
    dest: &Path,
    job_id: &str,
    size: u64,
) -> anyhow::Result<String> {
    let meta = std::fs::metadata(dest)?;
    if meta.len() <= size {
        return Ok(dest.to_string_lossy().to_string());
    }
    let t = Tracker::new(ctx, job_id, meta.len());
    let mut input = BufReader::new(File::open(dest)?);
    let mut idx = 1u32;
    let mut first = String::new();
    loop {
        t.check()?;
        let part = format!("{}.{:03}", dest.to_string_lossy(), idx);
        t.set_file(&part);
        let mut out = BufWriter::new(File::create(&part)?);
        let mut remaining = size;
        let mut buf = [0u8; 64 * 1024];
        let mut wrote = 0u64;
        while remaining > 0 {
            let want = remaining.min(buf.len() as u64) as usize;
            let n = input.read(&mut buf[..want])?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            remaining -= n as u64;
            wrote += n as u64;
            t.add(n as u64)?;
        }
        out.flush()?;
        if idx == 1 {
            first = part.clone();
        }
        if wrote < size {
            if wrote == 0 {
                let _ = std::fs::remove_file(&part);
            }
            break;
        }
        idx += 1;
    }
    std::fs::remove_file(dest)?;
    Ok(first)
}
