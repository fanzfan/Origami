use super::create::{self, CreateOptions, SourceFile};
use super::extract::{self, selected, Ctx, ExtractOptions, Tracker};
use super::{detect_format, sanitize_rel_path, Format};
use crate::encoding::decode_name;
use anyhow::Context;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

pub struct EditOptions {
    pub job_id: String,
    pub password: Option<String>,
    pub encoding: String,
}

/// 把文件/目录添加到压缩包内 `dir` 目录下（"" = 根目录）。
pub fn add_files(
    ctx: &Ctx<impl tauri::Runtime>,
    archive_path: &Path,
    sources: &[String],
    dir: &str,
    opts: &EditOptions,
) -> anyhow::Result<()> {
    let dir = dir.trim_matches('/');
    let mut files = create::collect_sources(sources, false)?;
    if !dir.is_empty() {
        for f in &mut files {
            f.rel = format!("{dir}/{}", f.rel);
        }
    }
    edit(ctx, archive_path, &[], &files, opts)
}

/// 从压缩包中删除条目（目录条目会连同其子项一起删除）。
pub fn remove_entries(
    ctx: &Ctx<impl tauri::Runtime>,
    archive_path: &Path,
    entries: &[String],
    opts: &EditOptions,
) -> anyhow::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    edit(ctx, archive_path, entries, &[], opts)
}

fn edit(
    ctx: &Ctx<impl tauri::Runtime>,
    archive_path: &Path,
    remove: &[String],
    add: &[SourceFile],
    opts: &EditOptions,
) -> anyhow::Result<()> {
    let format = detect_format(archive_path)?;
    match format {
        Format::Zip => edit_zip(ctx, archive_path, remove, add, opts),
        Format::SevenZ
        | Format::Tar
        | Format::TarGz
        | Format::TarBz2
        | Format::TarXz
        | Format::TarZst => edit_rebuild(ctx, archive_path, format, remove, add, opts),
        f => anyhow::bail!("{} 格式不支持编辑", f.label()),
    }
}

fn format_str(format: Format) -> &'static str {
    match format {
        Format::SevenZ => "7z",
        Format::Tar => "tar",
        Format::TarGz => "tar.gz",
        Format::TarBz2 => "tar.bz2",
        Format::TarXz => "tar.xz",
        Format::TarZst => "tar.zst",
        _ => unreachable!(),
    }
}

/// ZIP：原条目用 raw copy 重建到新文件（保留压缩方式与加密，无需密码），
/// 再追加新增文件，最后原子替换。
fn edit_zip(
    ctx: &Ctx<impl tauri::Runtime>,
    archive_path: &Path,
    remove: &[String],
    add: &[SourceFile],
    opts: &EditOptions,
) -> anyhow::Result<()> {
    let parent = archive_path.parent().unwrap_or(Path::new("."));
    let mut ar = zip::ZipArchive::new(BufReader::new(File::open(archive_path)?))
        .context("打开 ZIP 失败")?;

    // 新增文件覆盖同名旧条目。
    let added_names: Vec<&str> = add.iter().map(|f| f.rel.as_str()).collect();
    let mut keep: Vec<(usize, u64)> = Vec::new(); // (index, compressed size)
    for i in 0..ar.len() {
        let e = ar.by_index_raw(i)?;
        let name = decode_name(e.name_raw(), &opts.encoding);
        let name_norm = name.trim_end_matches('/');
        if selected(remove, name_norm) && !remove.is_empty() {
            continue;
        }
        if added_names.iter().any(|a| *a == name_norm) {
            continue;
        }
        keep.push((i, e.compressed_size()));
    }

    let total: u64 = keep.iter().map(|(_, c)| c).sum::<u64>()
        + add.iter().map(|f| f.size).sum::<u64>();
    let t = Tracker::new(ctx, &opts.job_id, total);

    let tmp = tempfile::Builder::new()
        .prefix(".origami-edit-")
        .suffix(".zip")
        .tempfile_in(parent)?;
    let mut w = zip::ZipWriter::new(BufWriter::new(tmp.reopen()?));

    for &(i, csize) in &keep {
        t.check()?;
        let e = ar.by_index_raw(i)?;
        t.set_file(e.name());
        w.raw_copy_file(e)?;
        t.add(csize)?;
    }

    let copts = CreateOptions {
        job_id: opts.job_id.clone(),
        format: "zip".into(),
        level: 6,
        method: String::new(),
        password: opts.password.clone(),
        volume_size: 0,
        exclude_junk: false,
    };
    for f in add {
        t.check()?;
        create::zip_write_file_streamed(&t, &mut w, f, &copts)?;
    }
    w.finish()?.flush()?;
    drop(ar);

    tmp.persist(archive_path)
        .map_err(|e| anyhow::anyhow!("替换原文件失败: {}", e.error))?;
    t.done();
    Ok(())
}

/// 7Z / TAR 系：解包到临时目录 → 增删 → 重新打包 → 原子替换。
fn edit_rebuild(
    ctx: &Ctx<impl tauri::Runtime>,
    archive_path: &Path,
    format: Format,
    remove: &[String],
    add: &[SourceFile],
    opts: &EditOptions,
) -> anyhow::Result<()> {
    let work = tempfile::Builder::new().prefix("origami-edit-").tempdir()?;
    let content = work.path().join("content");
    std::fs::create_dir(&content)?;

    let eopts = ExtractOptions {
        job_id: opts.job_id.clone(),
        password: opts.password.clone(),
        fallback_passwords: Vec::new(),
        encoding: opts.encoding.clone(),
        entries: Vec::new(),
        smart: false,
    };
    extract::extract(ctx, archive_path, &content, &eopts)?;

    for entry in remove {
        let Some(rel) = sanitize_rel_path(entry) else { continue };
        let p = content.join(rel);
        if p.is_dir() {
            std::fs::remove_dir_all(&p)?;
        } else if p.exists() || p.is_symlink() {
            std::fs::remove_file(&p)?;
        }
    }

    for f in add {
        let Some(rel) = sanitize_rel_path(&f.rel) else { continue };
        let dst = content.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if f.is_dir {
            std::fs::create_dir_all(&dst)?;
        } else if f.is_symlink {
            let target = std::fs::read_link(&f.abs)?;
            let _ = std::fs::remove_file(&dst);
            std::os::unix::fs::symlink(target, &dst)?;
        } else {
            std::fs::copy(&f.abs, &dst)?;
        }
    }

    let mut roots: Vec<String> = Vec::new();
    for e in std::fs::read_dir(&content)? {
        roots.push(e?.path().to_string_lossy().to_string());
    }

    let ext = archive_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("tmp");
    let out = work.path().join(format!("out.{ext}"));
    let copts = CreateOptions {
        job_id: opts.job_id.clone(),
        format: format_str(format).into(),
        level: 6,
        method: String::new(),
        password: opts.password.clone(),
        volume_size: 0,
        exclude_junk: false,
    };
    create::create(ctx, &out, &roots, &copts)?;

    // 跨设备时 rename 会失败，退回 copy + remove。
    if std::fs::rename(&out, archive_path).is_err() {
        std::fs::copy(&out, archive_path).context("替换原文件失败")?;
    }
    Ok(())
}
