use super::{detect_format, inner_name, ArchiveInfo, Entry, Format};
use crate::encoding::decode_name;
use anyhow::Context;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub struct ListOptions {
    pub password: Option<String>,
    pub encoding: String,
    /// 主密码失败时才惰性读取的已存密码（避免无谓地读凭据库 / 弹钥匙串）。
    pub fallback_passwords: crate::passwords::LazyPasswords,
}

pub fn list(path: &Path, opts: &ListOptions) -> anyhow::Result<ArchiveInfo> {
    let format = detect_format(path)?;
    let mut info = match format {
        Format::Zip => list_zip(path, opts)?,
        Format::SevenZ => list_7z(path, opts)?,
        Format::Rar => list_rar(path, opts)?,
        Format::Tar => list_tar(path, &opts.encoding)?,
        Format::TarGz | Format::TarBz2 | Format::TarXz | Format::TarZst => {
            list_tar_compressed(path, format, &opts.encoding)?
        }
        Format::Gz | Format::Bz2 | Format::Xz | Format::Zst => list_single(path, format)?,
    };
    info.format = format.label().to_string();
    info.total_size = info.entries.iter().map(|e| e.size).sum();
    info.total_compressed = info.entries.iter().map(|e| e.compressed).sum();
    info.has_encrypted = info.entries.iter().any(|e| e.encrypted) || info.has_encrypted;
    Ok(info)
}

fn empty_info() -> ArchiveInfo {
    ArchiveInfo {
        format: String::new(),
        entries: Vec::new(),
        has_encrypted: false,
        total_size: 0,
        total_compressed: 0,
        comment: None,
        used_password: None,
    }
}

fn list_zip(path: &Path, opts: &ListOptions) -> anyhow::Result<ArchiveInfo> {
    let file = File::open(path).context("打开文件失败")?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file)).context("不是有效的 ZIP 文件")?;
    let mut info = empty_info();
    let comment_raw = zip.comment().to_vec();
    if !comment_raw.is_empty() {
        info.comment = Some(decode_name(&comment_raw, &opts.encoding));
    }
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i).context("读取条目失败")?;
        let name = decode_name(f.name_raw(), &opts.encoding);
        info.entries.push(Entry {
            path: name,
            size: f.size(),
            compressed: f.compressed_size(),
            is_dir: f.is_dir(),
            mtime: zip_mtime(&f),
            encrypted: f.encrypted(),
            crc: Some(f.crc32()),
        });
    }
    Ok(info)
}

fn zip_mtime<R: std::io::Read>(f: &zip::read::ZipFile<'_, R>) -> Option<i64> {
    zipdt_to_unix(f.last_modified()?)
}

pub fn zipdt_to_unix(dt: zip::DateTime) -> Option<i64> {
    let date = time::Date::from_calendar_date(
        dt.year() as i32,
        time::Month::try_from(dt.month()).ok()?,
        dt.day(),
    )
    .ok()?;
    let t = time::Time::from_hms(dt.hour(), dt.minute(), dt.second()).ok()?;
    Some(time::PrimitiveDateTime::new(date, t).assume_utc().unix_timestamp())
}

pub fn nt_to_unix(nt: u64) -> Option<i64> {
    if nt == 0 {
        return None;
    }
    Some(nt as i64 / 10_000_000 - 11_644_473_600)
}

/// 用单个密码尝试打开 7z：Ok(Some)=成功，Ok(None)=密码错误（可换下一个），Err=致命错误。
fn try_list_7z(path: &Path, pw: &Option<String>) -> anyhow::Result<Option<ArchiveInfo>> {
    use sevenz_rust2::{Archive, Password};

    let password = pw.as_deref().map(Password::from).unwrap_or_else(Password::empty);
    match Archive::open_with_password(path, &password) {
        Ok(archive) => {
            let mut info = empty_info();
            info.used_password = pw.clone();
            for f in &archive.files {
                info.entries.push(Entry {
                    path: f.name.replace('\\', "/"),
                    size: f.size,
                    compressed: f.compressed_size,
                    is_dir: f.is_directory,
                    mtime: nt_to_unix(f.last_modified_date.into()),
                    encrypted: false,
                    crc: if f.has_crc { Some(f.crc as u32) } else { None },
                });
            }
            // Detect encrypted content blocks.
            let enc = archive.blocks.iter().any(|b| {
                b.coders
                    .iter()
                    .any(|c| c.encoder_method_id() == sevenz_rust2::EncoderMethod::ID_AES256_SHA256)
            });
            info.has_encrypted = enc;
            if enc {
                for e in &mut info.entries {
                    if !e.is_dir {
                        e.encrypted = true;
                    }
                }
            }
            Ok(Some(info))
        }
        Err(e) => {
            let msg = format!("{e:?}");
            if msg.contains("Password") || msg.contains("password") || msg.contains("Checksum") {
                Ok(None)
            } else {
                Err(anyhow::anyhow!("打开 7z 失败: {e}"))
            }
        }
    }
}

fn list_7z(path: &Path, opts: &ListOptions) -> anyhow::Result<ArchiveInfo> {
    // 主密码（用户给的 / 无）优先；只有它失败才惰性读取已存密码。
    if let Some(info) = try_list_7z(path, &opts.password)? {
        return Ok(info);
    }
    for p in opts.fallback_passwords.get() {
        if let Some(info) = try_list_7z(path, &Some(p.clone()))? {
            return Ok(info);
        }
    }
    Err(anyhow::anyhow!("PASSWORD_REQUIRED"))
}

/// 用单个密码尝试列出 RAR：Ok(Some)=成功，Ok(None)=密码错误，Err=致命错误。
fn try_list_rar(path: &Path, pw: &Option<String>) -> anyhow::Result<Option<ArchiveInfo>> {
    let result = (|| -> Result<ArchiveInfo, unrar::error::UnrarError> {
        let archive = match pw {
            Some(p) => unrar::Archive::with_password(path, p),
            None => unrar::Archive::new(path),
        };
        let mut open = archive.open_for_listing()?;
        let mut info = empty_info();
        info.used_password = pw.clone();
        loop {
            let Some(cursor) = open.read_header()? else { break };
            let h = cursor.entry();
            info.entries.push(Entry {
                path: h.filename.to_string_lossy().replace('\\', "/"),
                size: h.unpacked_size,
                compressed: 0,
                is_dir: cursor.entry().is_directory(),
                mtime: dos_to_unix(h.file_time),
                encrypted: cursor.entry().is_encrypted(),
                crc: Some(h.file_crc),
            });
            open = cursor.skip()?;
        }
        Ok(info)
    })();
    match result {
        Ok(info) => Ok(Some(info)),
        Err(e) => {
            use unrar::error::Code;
            if matches!(e.code, Code::MissingPassword | Code::BadPassword) {
                Ok(None)
            } else {
                Err(anyhow::anyhow!("打开 RAR 失败: {:?}", e.code))
            }
        }
    }
}

fn list_rar(path: &Path, opts: &ListOptions) -> anyhow::Result<ArchiveInfo> {
    if let Some(info) = try_list_rar(path, &opts.password)? {
        return Ok(info);
    }
    for p in opts.fallback_passwords.get() {
        if let Some(info) = try_list_rar(path, &Some(p.clone()))? {
            return Ok(info);
        }
    }
    Err(anyhow::anyhow!("PASSWORD_REQUIRED"))
}

fn dos_to_unix(dos: u32) -> Option<i64> {
    if dos == 0 {
        return None;
    }
    let sec = ((dos & 0x1F) * 2) as u8;
    let min = ((dos >> 5) & 0x3F) as u8;
    let hour = ((dos >> 11) & 0x1F) as u8;
    let day = ((dos >> 16) & 0x1F) as u8;
    let month = ((dos >> 21) & 0x0F) as u8;
    let year = ((dos >> 25) & 0x7F) as i32 + 1980;
    let date = time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()?;
    let t = time::Time::from_hms(hour, min, sec).ok()?;
    Some(time::PrimitiveDateTime::new(date, t).assume_utc().unix_timestamp())
}

fn tar_entries<R: std::io::Read>(mut ar: tar::Archive<R>, encoding: &str) -> anyhow::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for entry in ar.entries()? {
        let entry = entry?;
        let h = entry.header();
        let raw = entry.path_bytes();
        let name = decode_name(&raw, encoding);
        out.push(Entry {
            path: name,
            size: h.size().unwrap_or(0),
            compressed: h.size().unwrap_or(0),
            is_dir: h.entry_type().is_dir(),
            mtime: h.mtime().ok().map(|m| m as i64),
            encrypted: false,
            crc: None,
        });
    }
    Ok(out)
}

fn list_tar(path: &Path, encoding: &str) -> anyhow::Result<ArchiveInfo> {
    let f = BufReader::new(File::open(path)?);
    let mut info = empty_info();
    info.entries = tar_entries(tar::Archive::new(f), encoding)?;
    Ok(info)
}

pub fn open_decompressor(
    path: &Path,
    format: Format,
) -> anyhow::Result<Box<dyn std::io::Read + Send>> {
    let f = Box::new(BufReader::new(File::open(path)?));
    decompress_stream(f, format)
}

pub fn decompress_stream(
    f: Box<dyn std::io::BufRead + Send>,
    format: Format,
) -> anyhow::Result<Box<dyn std::io::Read + Send>> {
    let r: Box<dyn std::io::Read + Send> = match format {
        Format::TarGz | Format::Gz => Box::new(flate2::bufread::MultiGzDecoder::new(f)),
        Format::TarBz2 | Format::Bz2 => Box::new(bzip2::bufread::MultiBzDecoder::new(f)),
        Format::TarXz | Format::Xz => Box::new(liblzma::bufread::XzDecoder::new_multi_decoder(f)),
        Format::TarZst | Format::Zst => Box::new(zstd::stream::read::Decoder::with_buffer(f)?),
        _ => anyhow::bail!("不支持的流式格式"),
    };
    Ok(r)
}

fn list_tar_compressed(path: &Path, format: Format, encoding: &str) -> anyhow::Result<ArchiveInfo> {
    let r = open_decompressor(path, format)?;
    let mut info = empty_info();
    info.entries = tar_entries(tar::Archive::new(r), encoding)?;
    Ok(info)
}

fn list_single(path: &Path, _format: Format) -> anyhow::Result<ArchiveInfo> {
    let meta = std::fs::metadata(path)?;
    let mut info = empty_info();
    info.entries.push(Entry {
        path: inner_name(path),
        size: 0, // unknown until decompressed
        compressed: meta.len(),
        is_dir: false,
        mtime: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64),
        encrypted: false,
        crc: None,
    });
    Ok(info)
}
