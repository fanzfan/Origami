use super::extract::{CountingReader, Ctx, Progress, Tracker};
use anyhow::Context;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct CreateOptions {
    pub job_id: String,
    /// "zip" | "7z" | "tar" | "tar.gz" | "tar.bz2" | "tar.xz" | "tar.zst"
    pub format: String,
    /// 0 (store) ..= 9 (best)
    pub level: u32,
    /// 压缩线程数。0 表示自动；显式值会按本机逻辑核数和安全上限裁剪。
    pub threads: usize,
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
                // 归档条目名必须用「/」分隔。直接 to_string_lossy 在 Windows 上会保留
                // 反斜杠（如 sub\file），导致条目名非法、其它工具解压时层级错乱。
                // 按路径分量重组，平台无关地拼成正斜杠路径。
                let rel_inner = entry
                    .path()
                    .strip_prefix(p)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
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

const MAX_WORKER_THREADS: usize = 64;

fn worker_threads(requested: usize) -> usize {
    let available = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, MAX_WORKER_THREADS);
    if requested == 0 {
        available
    } else {
        requested.clamp(1, available)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        create_zip_parallel_with_budget, raw_copy_reader, worker_threads, CreateOptions,
        SourceFile, Tracker, ZipSpool, ZipSpoolReader, MAX_WORKER_THREADS, ZIP_PAR_MAX,
    };
    use crate::archive::extract::Ctx;
    use std::io::{Cursor, Read, Seek, Write};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    struct CancelAfterRead<R> {
        inner: R,
        cancel: Arc<AtomicBool>,
    }

    impl<R: Read> Read for CancelAfterRead<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(buf)?;
            if n > 0 {
                self.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(n)
        }
    }

    impl<R: Seek> Seek for CancelAfterRead<R> {
        fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(from)
        }
    }

    fn source(path: PathBuf, rel: &str, size: u64) -> SourceFile {
        SourceFile {
            abs: path,
            rel: rel.into(),
            size,
            is_dir: false,
            is_symlink: false,
        }
    }

    fn opts() -> CreateOptions {
        CreateOptions {
            job_id: "zip-budget-test".into(),
            format: "zip".into(),
            level: 6,
            threads: 2,
            method: String::new(),
            password: None,
            volume_size: 0,
            exclude_junk: false,
        }
    }

    fn incompressible_bytes(len: usize, seed: u64) -> Vec<u8> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect()
    }

    #[test]
    fn worker_threads_normalizes_auto_and_explicit_values() {
        let logical = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .clamp(1, MAX_WORKER_THREADS);

        assert_eq!(worker_threads(0), logical);
        assert_eq!(worker_threads(1), 1);
        assert_eq!(worker_threads(2), logical.min(2));
        assert_eq!(worker_threads(usize::MAX), logical);
    }

    #[test]
    fn zip_spool_spills_without_growing_past_its_memory_limit() {
        let mut spool = ZipSpool::new(super::ZIP_PAR_CHUNK_SIZE);
        let payload = incompressible_bytes(2 * super::ZIP_PAR_CHUNK_SIZE, 0x91ab_2c3d);
        spool.write_all(&payload).unwrap();
        assert!(spool.spilled());

        let mut reader = match spool.into_reader() {
            ZipSpoolReader::Memory(_) => panic!("oversized result stayed in memory"),
            ZipSpoolReader::Disk(mut file) => {
                file.seek(std::io::SeekFrom::Start(0)).unwrap();
                file
            }
        };
        let mut actual = Vec::new();
        reader.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, payload);
    }

    #[test]
    fn zip_parallel_late_cancel_during_raw_copy_keeps_cancelled_error() {
        let app = tauri::test::mock_app();
        let cancel = Arc::new(AtomicBool::new(false));
        let ctx = Ctx {
            app: app.handle(),
            cancel: cancel.clone(),
        };
        let payload = incompressible_bytes(128 * 1024, 0x72ab_8c4d);
        let tracker = Tracker::new(&ctx, "zip-late-cancel", payload.len() as u64);
        tracker.add(payload.len() as u64).unwrap();

        let mut source_zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let entry_options: zip::write::SimpleFileOptions = Default::default();
        source_zip.start_file("entry.bin", entry_options).unwrap();
        source_zip.write_all(&payload).unwrap();
        let entry_spool = source_zip.finish().unwrap().into_inner();

        let mut output_zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let late_cancel = CancelAfterRead {
            inner: Cursor::new(entry_spool),
            cancel,
        };
        let error = raw_copy_reader(&tracker, &mut output_zip, late_cancel).unwrap_err();
        assert!(format!("{error:#}").contains("CANCELLED"), "got {error:#}");
    }

    #[test]
    fn zip_parallel_budget_spills_expanding_entries_and_roundtrips() {
        let app = tauri::test::mock_app();
        let cancel = Arc::new(AtomicBool::new(false));
        let ctx = Ctx {
            app: app.handle(),
            cancel,
        };
        let tracker = Tracker::new(&ctx, "zip-budget-test", 1024 * 1024);
        let tmp = tempfile::tempdir().unwrap();
        let payloads = [
            incompressible_bytes(512 * 1024, 0x1234_5678),
            incompressible_bytes(512 * 1024, 0x9876_5432),
        ];
        let mut files = Vec::new();
        for (index, payload) in payloads.iter().enumerate() {
            let path = tmp.path().join(format!("random-{index}.bin"));
            std::fs::write(&path, payload).unwrap();
            files.push(source(
                path,
                &format!("random-{index}.bin"),
                payload.len() as u64,
            ));
        }
        let dest = tmp.path().join("spooled.zip");

        // 两个 worker 各只保留一个 64 KiB 块，超过即转匿名临时文件。
        create_zip_parallel_with_budget(
            &tracker,
            &dest,
            &files,
            &opts(),
            2,
            4 * super::ZIP_PAR_CHUNK_SIZE,
        )
        .unwrap();

        let mut archive = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        for (index, expected) in payloads.iter().enumerate() {
            let mut entry = archive.by_name(&format!("random-{index}.bin")).unwrap();
            let mut actual = Vec::new();
            entry.read_to_end(&mut actual).unwrap();
            assert_eq!(&actual, expected);
        }
    }

    #[test]
    fn zip_parallel_worker_error_cleans_partial_archive_and_joins_peers() {
        let app = tauri::test::mock_app();
        let ctx = Ctx {
            app: app.handle(),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let tracker = Tracker::new(&ctx, "zip-worker-error", 8);
        let tmp = tempfile::tempdir().unwrap();
        let valid = tmp.path().join("valid.txt");
        std::fs::write(&valid, b"valid worker entry").unwrap();
        let files = vec![
            source(valid, "valid.txt", 19),
            source(tmp.path().join("missing-worker.txt"), "missing.txt", 8),
        ];
        let dest = tmp.path().join("worker-error.zip");

        let error =
            create_zip_parallel_with_budget(&tracker, &dest, &files, &opts(), 2, 128 * 1024)
                .unwrap_err();
        assert!(!dest.exists(), "failed ZIP output should be removed");
        assert!(
            error.to_string().contains("missing-worker.txt"),
            "got {error}"
        );
    }

    #[test]
    fn zip_parallel_main_error_releases_waiting_worker_and_cleans_output() {
        let app = tauri::test::mock_app();
        let ctx = Ctx {
            app: app.handle(),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let tracker = Tracker::new(&ctx, "zip-main-error", 8);
        let tmp = tempfile::tempdir().unwrap();
        let valid = tmp.path().join("valid.txt");
        std::fs::write(&valid, b"valid worker entry").unwrap();
        let files = vec![
            source(valid, "valid.txt", 19),
            source(
                tmp.path().join("missing-serial.bin"),
                "missing-serial.bin",
                ZIP_PAR_MAX + 1,
            ),
        ];
        let dest = tmp.path().join("main-error.zip");

        let error =
            create_zip_parallel_with_budget(&tracker, &dest, &files, &opts(), 2, 128 * 1024)
                .unwrap_err();
        assert!(!dest.exists(), "failed ZIP output should be removed");
        assert!(
            error.to_string().contains("missing-serial.bin"),
            "got {error}"
        );
    }

    #[test]
    fn zip_parallel_cancellation_releases_workers_and_cleans_output() {
        let app = tauri::test::mock_app();
        let cancel = Arc::new(AtomicBool::new(true));
        let ctx = Ctx {
            app: app.handle(),
            cancel,
        };
        let tracker = Tracker::new(&ctx, "zip-cancel", 8);
        let tmp = tempfile::tempdir().unwrap();
        let source_path = tmp.path().join("cancel.txt");
        std::fs::write(&source_path, b"cancel me").unwrap();
        let files = vec![source(source_path, "cancel.txt", 9)];
        let dest = tmp.path().join("cancel.zip");

        let error = create_zip_parallel_with_budget(&tracker, &dest, &files, &opts(), 2, 64 * 1024)
            .unwrap_err();
        assert!(!dest.exists(), "cancelled ZIP output should be removed");
        assert!(error.to_string().contains("CANCELLED"), "got {error}");
    }
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
/// 并行 ZIP 条目缓冲的总内存预算；只约束结果缓冲，不含压缩器 workspace。
const ZIP_PAR_MEMORY_BUDGET: usize = 64 * 1024 * 1024;
/// 用定长块管理并行结果，避免 Vec 扩容超过已分配的 worker 预算。
const ZIP_PAR_CHUNK_SIZE: usize = 64 * 1024;
/// 为每个 worker 的块索引和分配器元数据预留空间。
const ZIP_PAR_METADATA_RESERVE: usize = 64 * 1024;

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
    let threads = worker_threads(opts.threads);
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
        let mut r = BufReader::new(
            File::open(&f.abs).with_context(|| format!("无法读取 {}", f.abs.display()))?,
        );
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
    create_zip_parallel_with_budget(t, dest, files, opts, threads, ZIP_PAR_MEMORY_BUDGET)
}

fn create_zip_parallel_with_budget<R: tauri::Runtime>(
    t: &Arc<Tracker<R>>,
    dest: &Path,
    files: &[SourceFile],
    opts: &CreateOptions,
    threads: usize,
    memory_budget: usize,
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

    if par_idx.is_empty() {
        return create_zip_serial(t, dest, files, opts);
    }

    let out = BufWriter::new(File::create(dest)?);
    let mut w = zip::ZipWriter::new(out);

    let next = AtomicUsize::new(0);
    let n_workers = threads.min(par_idx.len().max(1));
    let payload_budget = memory_budget.saturating_sub(memory_budget.min(ZIP_PAR_METADATA_RESERVE));
    let worker_budget = (payload_budget / n_workers / ZIP_PAR_CHUNK_SIZE) * ZIP_PAR_CHUNK_SIZE;
    let (tx, rx) = sync_channel::<(usize, anyhow::Result<ZipSpool>)>(n_workers);

    let result: anyhow::Result<()> = std::thread::scope(|s| {
        let mut acknowledgements = Vec::with_capacity(n_workers);
        for worker_id in 0..n_workers {
            let tx = tx.clone();
            let next = &next;
            let par_idx = &par_idx;
            let t = t.clone();
            let (ack_tx, ack_rx) = sync_channel::<()>(0);
            acknowledgements.push(ack_tx);
            s.spawn(move || loop {
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(&i) = par_idx.get(k) else { break };
                let f = &files[i];
                let res = compress_zip_entry(&t, f, opts, worker_budget);
                let failed = res.is_err();
                if tx.send((worker_id, res)).is_err() || failed || ack_rx.recv().is_err() {
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
            let (worker_id, spool) = match rx.recv() {
                Ok((worker_id, Ok(spool))) => (worker_id, spool),
                Ok((_, Err(e))) => return Err(e),
                Err(_) => anyhow::bail!("压缩线程异常退出"),
            };
            raw_copy_spool(t, &mut w, spool)?;
            // worker 持有该预算槽直到主线程完成 raw copy 并释放结果。
            acknowledgements[worker_id]
                .send(())
                .map_err(|_| anyhow::anyhow!("压缩线程异常退出"))?;
            merged += 1;
        }
        Ok(())
    });
    let finish_result = match result {
        Err(error) => {
            // scope 返回时接收端及 ack sender 已关闭，worker 会退出。
            drop(w);
            Err(error)
        }
        Ok(()) => {
            if let Err(error) = t.check() {
                drop(w);
                Err(error)
            } else {
                w.finish()
                    .and_then(|mut out| {
                        out.flush()?;
                        Ok(())
                    })
                    .map_err(anyhow::Error::from)
                    .and_then(|()| t.check())
            }
        }
    };
    if finish_result.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    finish_result
}

fn compress_zip_entry<R: tauri::Runtime>(
    t: &Arc<Tracker<R>>,
    f: &SourceFile,
    opts: &CreateOptions,
    memory_limit: usize,
) -> anyhow::Result<ZipSpool> {
    t.check()?;
    t.set_file(&f.rel);
    let mut zw = zip::ZipWriter::new(ZipSpool::new(memory_limit));
    zw.start_file(&f.rel, zip_options(f, opts))?;
    let mut r = CountingReader::new(
        BufReader::new(
            File::open(&f.abs).with_context(|| format!("无法读取 {}", f.abs.display()))?,
        ),
        t.clone(),
    );
    std::io::copy(&mut r, &mut zw)?;
    Ok(zw.finish()?)
}

fn raw_copy_spool<R: tauri::Runtime, W: Write + std::io::Seek>(
    t: &Arc<Tracker<R>>,
    w: &mut zip::ZipWriter<W>,
    spool: ZipSpool,
) -> anyhow::Result<()> {
    match spool.into_reader() {
        ZipSpoolReader::Memory(reader) => raw_copy_reader(t, w, reader),
        ZipSpoolReader::Disk(reader) => raw_copy_reader(t, w, reader),
    }
}

fn raw_copy_reader<T: tauri::Runtime, W: Write + std::io::Seek, R: Read + std::io::Seek>(
    t: &Arc<Tracker<T>>,
    w: &mut zip::ZipWriter<W>,
    reader: R,
) -> anyhow::Result<()> {
    t.check()?;
    let reader = CancelCheckingReader {
        inner: reader,
        tracker: t.clone(),
    };
    let mut ar = zip::ZipArchive::new(reader)?;
    let entry = ar.by_index_raw(0)?;
    t.check()?;
    w.raw_copy_file(entry)?;
    t.check()?;
    Ok(())
}

/// ZIP 合并阶段只检查取消，不重复累计已经由源文件读取器统计过的进度。
struct CancelCheckingReader<Rd, T: tauri::Runtime> {
    inner: Rd,
    tracker: Arc<Tracker<T>>,
}

impl<Rd: Read, T: tauri::Runtime> Read for CancelCheckingReader<Rd, T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.tracker
            .check()
            .map_err(|_| std::io::Error::other("CANCELLED"))?;
        let n = self.inner.read(buf)?;
        self.tracker
            .check()
            .map_err(|_| std::io::Error::other("CANCELLED"))?;
        Ok(n)
    }
}

impl<Rd: std::io::Seek, T: tauri::Runtime> std::io::Seek for CancelCheckingReader<Rd, T> {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        self.tracker
            .check()
            .map_err(|_| std::io::Error::other("CANCELLED"))?;
        let position = self.inner.seek(from)?;
        self.tracker
            .check()
            .map_err(|_| std::io::Error::other("CANCELLED"))?;
        Ok(position)
    }
}

enum ZipSpoolStorage {
    Memory(Vec<Box<[u8]>>),
    Disk(File),
}

struct ZipSpool {
    storage: ZipSpoolStorage,
    memory_limit: usize,
    position: u64,
    len: u64,
}

impl ZipSpool {
    fn new(memory_limit: usize) -> Self {
        Self {
            storage: ZipSpoolStorage::Memory(Vec::new()),
            memory_limit: memory_limit / ZIP_PAR_CHUNK_SIZE * ZIP_PAR_CHUNK_SIZE,
            position: 0,
            len: 0,
        }
    }

    fn spill_to_disk(&mut self) -> std::io::Result<()> {
        if let ZipSpoolStorage::Memory(chunks) = &self.storage {
            let mut file = tempfile::tempfile()?;
            let mut remaining = self.len as usize;
            for chunk in chunks {
                let n = remaining.min(chunk.len());
                file.write_all(&chunk[..n])?;
                remaining -= n;
                if remaining == 0 {
                    break;
                }
            }
            file.seek(std::io::SeekFrom::Start(self.position))?;
            self.storage = ZipSpoolStorage::Disk(file);
        }
        Ok(())
    }

    fn into_reader(self) -> ZipSpoolReader {
        match self.storage {
            ZipSpoolStorage::Memory(chunks) => {
                ZipSpoolReader::Memory(ChunkedReader::new(chunks, self.len))
            }
            ZipSpoolStorage::Disk(file) => ZipSpoolReader::Disk(file),
        }
    }

    #[cfg(test)]
    fn spilled(&self) -> bool {
        matches!(self.storage, ZipSpoolStorage::Disk(_))
    }
}

impl Write for ZipSpool {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let end = self
            .position
            .checked_add(buf.len() as u64)
            .ok_or_else(|| std::io::Error::other("ZIP 临时条目过大"))?;
        if end > self.memory_limit as u64 {
            self.spill_to_disk()?;
        }

        match &mut self.storage {
            ZipSpoolStorage::Memory(chunks) => {
                let start = usize::try_from(self.position)
                    .map_err(|_| std::io::Error::other("ZIP 临时条目过大"))?;
                let end =
                    usize::try_from(end).map_err(|_| std::io::Error::other("ZIP 临时条目过大"))?;
                let needed = end.div_ceil(ZIP_PAR_CHUNK_SIZE);
                while chunks.len() < needed {
                    chunks.push(vec![0; ZIP_PAR_CHUNK_SIZE].into_boxed_slice());
                }
                let mut copied = 0;
                while copied < buf.len() {
                    let offset = start + copied;
                    let chunk_index = offset / ZIP_PAR_CHUNK_SIZE;
                    let chunk_offset = offset % ZIP_PAR_CHUNK_SIZE;
                    let count = (ZIP_PAR_CHUNK_SIZE - chunk_offset).min(buf.len() - copied);
                    chunks[chunk_index][chunk_offset..chunk_offset + count]
                        .copy_from_slice(&buf[copied..copied + count]);
                    copied += count;
                }
                self.position = end as u64;
                self.len = self.len.max(self.position);
                Ok(buf.len())
            }
            ZipSpoolStorage::Disk(file) => {
                file.seek(std::io::SeekFrom::Start(self.position))?;
                let n = file.write(buf)?;
                self.position += n as u64;
                self.len = self.len.max(self.position);
                Ok(n)
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.storage {
            ZipSpoolStorage::Memory(_) => Ok(()),
            ZipSpoolStorage::Disk(file) => file.flush(),
        }
    }
}

impl std::io::Seek for ZipSpool {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        match &mut self.storage {
            ZipSpoolStorage::Memory(_) => {
                self.position = seek_position(self.position, self.len, from)?;
                Ok(self.position)
            }
            ZipSpoolStorage::Disk(file) => {
                self.position = file.seek(from)?;
                Ok(self.position)
            }
        }
    }
}

enum ZipSpoolReader {
    Memory(ChunkedReader),
    Disk(File),
}

struct ChunkedReader {
    chunks: Vec<Box<[u8]>>,
    len: u64,
    position: u64,
}

impl ChunkedReader {
    fn new(chunks: Vec<Box<[u8]>>, len: u64) -> Self {
        Self {
            chunks,
            len,
            position: 0,
        }
    }
}

impl Read for ChunkedReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let count = self.len.saturating_sub(self.position).min(buf.len() as u64) as usize;
        let mut copied = 0;
        while copied < count {
            let offset = self.position as usize + copied;
            let chunk_index = offset / ZIP_PAR_CHUNK_SIZE;
            let chunk_offset = offset % ZIP_PAR_CHUNK_SIZE;
            let n = (ZIP_PAR_CHUNK_SIZE - chunk_offset).min(count - copied);
            buf[copied..copied + n]
                .copy_from_slice(&self.chunks[chunk_index][chunk_offset..chunk_offset + n]);
            copied += n;
        }
        self.position += copied as u64;
        Ok(copied)
    }
}

impl std::io::Seek for ChunkedReader {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        self.position = seek_position(self.position, self.len, from)?;
        Ok(self.position)
    }
}

fn seek_position(position: u64, len: u64, from: std::io::SeekFrom) -> std::io::Result<u64> {
    let base = match from {
        std::io::SeekFrom::Start(pos) => return Ok(pos),
        std::io::SeekFrom::Current(_) => position as i128,
        std::io::SeekFrom::End(_) => len as i128,
    };
    let offset = match from {
        std::io::SeekFrom::Current(offset) | std::io::SeekFrom::End(offset) => offset as i128,
        std::io::SeekFrom::Start(_) => unreachable!(),
    };
    let target = base + offset;
    if !(0..=u64::MAX as i128).contains(&target) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid seek to ZIP temporary entry",
        ));
    }
    Ok(target as u64)
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
    let threads = worker_threads(opts.threads) as u32;
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
    let threads = worker_threads(opts.threads) as u32;
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

/// 分卷下限：再小就只会切出成千上万个碎片文件，多半是误操作。
pub const MIN_VOLUME_SIZE: u64 = 64 * 1024;

fn part_path(dest: &Path, idx: u32) -> PathBuf {
    PathBuf::from(format!("{}.{:03}", dest.to_string_lossy(), idx))
}

/// 删除 `from` 号起残留的旧分卷（上次分卷比这次多时会留下尾巴，
/// 不清掉会被当成本次分卷的一部分，解压出来就是坏包）。
fn remove_stale_parts(dest: &Path, from: u32) {
    let mut gap = 0u32;
    let mut i = from;
    while gap < 3 {
        let p = part_path(dest, i);
        if p.exists() {
            let _ = std::fs::remove_file(&p);
            gap = 0;
        } else {
            gap += 1;
        }
        i += 1;
    }
}

fn split_volumes(
    ctx: &Ctx<impl tauri::Runtime>,
    dest: &Path,
    job_id: &str,
    size: u64,
) -> anyhow::Result<String> {
    if size < MIN_VOLUME_SIZE {
        anyhow::bail!("分卷大小不能小于 {} KB", MIN_VOLUME_SIZE / 1024);
    }
    let meta = std::fs::metadata(dest)?;
    if meta.len() <= size {
        // 单卷装得下就不分卷；但仍要清掉可能残留的旧分卷。
        remove_stale_parts(dest, 1);
        return Ok(dest.to_string_lossy().to_string());
    }
    let t = Tracker::new(ctx, job_id, meta.len());
    let mut input = BufReader::new(File::open(dest)?);
    let mut idx = 1u32;
    let mut first = String::new();
    let res = (|| -> anyhow::Result<()> {
        loop {
            t.check()?;
            let part = part_path(dest, idx);
            let part_str = part.to_string_lossy().to_string();
            t.set_file(&part_str);
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
                first = part_str;
            }
            if wrote < size {
                if wrote == 0 {
                    let _ = std::fs::remove_file(&part);
                    idx -= 1;
                }
                break;
            }
            idx += 1;
        }
        Ok(())
    })();
    if let Err(e) = res {
        // 取消/出错：把这次已经写出的分卷清干净，不留半成品。
        remove_stale_parts(dest, 1);
        return Err(e);
    }
    remove_stale_parts(dest, idx + 1);
    std::fs::remove_file(dest)?;
    Ok(first)
}
