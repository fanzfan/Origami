use super::volume;
use super::{detect_format, Format};
use crate::encoding::decode_name;
use base64::Engine;
use serde::Serialize;
use std::io::{BufReader, Read};
use std::path::Path;

const MAX_PREVIEW: u64 = 8 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// "text" | "image" | "binary"
    pub kind: String,
    pub text: Option<String>,
    /// base64 for images
    pub data: Option<String>,
    pub mime: Option<String>,
    pub truncated: bool,
    pub size: u64,
}

fn image_mime(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let m = if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else {
        return None;
    };
    Some(m)
}

fn build_preview(name: &str, data: Vec<u8>, truncated: bool, full_size: u64) -> Preview {
    if let Some(mime) = image_mime(name) {
        return Preview {
            kind: "image".into(),
            text: None,
            data: Some(base64::engine::general_purpose::STANDARD.encode(&data)),
            mime: Some(mime.to_string()),
            truncated,
            size: full_size,
        };
    }
    // Try text.
    let looks_binary = data.iter().take(8192).any(|&b| b == 0);
    if !looks_binary {
        let text = String::from_utf8(data.clone())
            .unwrap_or_else(|_| decode_name(&data, "auto"));
        return Preview {
            kind: "text".into(),
            text: Some(text),
            data: None,
            mime: None,
            truncated,
            size: full_size,
        };
    }
    Preview {
        kind: "binary".into(),
        text: None,
        data: None,
        mime: None,
        truncated,
        size: full_size,
    }
}

#[cfg(test)]
mod tests {
    use super::{read_capped, MAX_PREVIEW};
    use std::io::Cursor;

    #[test]
    fn capped_reader_reads_one_extra_byte_to_detect_truncation() {
        let mut input = vec![b'x'; MAX_PREVIEW as usize];
        let (data, truncated) = read_capped(&mut Cursor::new(&input)).unwrap();
        assert_eq!(data.len() as u64, MAX_PREVIEW);
        assert!(!truncated);

        input.push(b'x');
        let (data, truncated) = read_capped(&mut Cursor::new(&input)).unwrap();
        assert_eq!(data.len() as u64, MAX_PREVIEW);
        assert!(truncated);
    }
}

fn read_capped(r: &mut dyn Read) -> anyhow::Result<(Vec<u8>, bool)> {
    // 多读 1 字节来区分“恰好到上限”和“确实被截断”。
    let mut data = Vec::with_capacity(MAX_PREVIEW as usize + 1);
    r.take(MAX_PREVIEW + 1).read_to_end(&mut data)?;
    let truncated = data.len() as u64 > MAX_PREVIEW;
    data.truncate(MAX_PREVIEW as usize);
    Ok((data, truncated))
}

pub fn preview_entry(
    archive_path: &Path,
    entry_path: &str,
    password: Option<String>,
    fallbacks: crate::passwords::LazyPasswords,
    encoding: &str,
) -> anyhow::Result<Preview> {
    let format = detect_format(archive_path)?;
    match format {
        Format::Zip => {
            let mut zip = zip::ZipArchive::new(volume::open_reader(archive_path)?)?;
            for i in 0..zip.len() {
                let (name, encrypted, size) = {
                    let f = zip.by_index_raw(i)?;
                    (decode_name(f.name_raw(), encoding), f.encrypted(), f.size())
                };
                if name != entry_path {
                    continue;
                }
                let mut f = if encrypted {
                    let mut cands = Vec::new();
                    if let Some(p) = &password {
                        cands.push(p.clone());
                    }
                    cands.extend(fallbacks.get().iter().cloned());
                    let mut found = None;
                    for pw in cands {
                        if zip.by_index_decrypt(i, pw.as_bytes()).is_ok() {
                            found = Some(pw);
                            break;
                        }
                    }
                    let pw = found.ok_or_else(|| anyhow::anyhow!("PASSWORD_REQUIRED"))?;
                    zip.by_index_decrypt(i, pw.as_bytes())?
                } else {
                    zip.by_index(i)?
                };
                let (data, truncated) = read_capped(&mut f)?;
                return Ok(build_preview(&name, data, truncated, size));
            }
            anyhow::bail!("条目不存在: {entry_path}")
        }
        Format::SevenZ => {
            use sevenz_rust2::{ArchiveReader, Password};
            let mut cands: Vec<Option<String>> = vec![password.clone()];
            cands.extend(fallbacks.get().iter().cloned().map(Some));
            let mut last = anyhow::anyhow!("条目不存在: {entry_path}");
            for cand in cands {
                let pw = cand
                    .as_deref()
                    .map(Password::from)
                    .unwrap_or_else(Password::empty);
                let src = match volume::open_reader(archive_path) {
                    Ok(s) => s,
                    Err(e) => {
                        last = anyhow::anyhow!("{e}");
                        continue;
                    }
                };
                match ArchiveReader::new(src, pw) {
                    Ok(mut r) => {
                        let mut result: Option<Preview> = None;
                        let target = entry_path.to_string();
                        r.for_each_entries(|entry, reader| {
                            let name = entry.name.replace('\\', "/");
                            if name == target && !entry.is_directory {
                                let mut data = Vec::new();
                                let mut buf = [0u8; 64 * 1024];
                                let mut truncated = false;
                                loop {
                                    match reader.read(&mut buf) {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            data.extend_from_slice(&buf[..n]);
                                            if data.len() as u64 >= MAX_PREVIEW {
                                                truncated = true;
                                                break;
                                            }
                                        }
                                        Err(e) => return Err(sevenz_rust2::Error::Io(e, "".into())),
                                    }
                                }
                                let size = entry.size;
                                result = Some(build_preview(&name, data, truncated, size));
                                return Ok(false);
                            }
                            Ok(true)
                        })?;
                        if let Some(p) = result {
                            return Ok(p);
                        }
                        anyhow::bail!("条目不存在: {entry_path}");
                    }
                    Err(e) => last = anyhow::anyhow!("PASSWORD_REQUIRED: {e}"),
                }
            }
            Err(last)
        }
        Format::Rar => {
            // unrar 按路径工作：原生分卷给首卷，裸切分先合并到临时文件。
            let native = volume::native_path(archive_path)?;
            let archive_path = native.path();
            let mut cands: Vec<Option<String>> = vec![password.clone()];
            cands.extend(fallbacks.get().iter().cloned().map(Some));
            let mut last = anyhow::anyhow!("条目不存在: {entry_path}");
            for cand in cands {
                let temp_dir = tempfile::tempdir()?;
                let temp_file = temp_dir.path().join("preview.bin");
                let result =
                    (|| -> Result<Option<(String, u64, bool)>, unrar::error::UnrarError> {
                        let archive = match &cand {
                            Some(p) => unrar::Archive::with_password(archive_path, p),
                            None => unrar::Archive::new(archive_path),
                        };
                        let mut open = archive.open_for_processing()?;
                        loop {
                            let Some(cursor) = open.read_header()? else {
                                break;
                            };
                            let name = cursor.entry().filename.to_string_lossy().replace('\\', "/");
                            let size = cursor.entry().unpacked_size;
                            if name == entry_path {
                                // unrar 0.5.x 只提供 read()->Vec<u8>，没有带长度上限的流式读取。
                                // 大条目在解码前按头部声明大小拒绝，避免将完整内容读入 Rust 内存。
                                if size > MAX_PREVIEW {
                                    let split_after = cursor.entry().is_split_after();
                                    let encrypted = cursor.entry().is_encrypted();
                                    open = if encrypted {
                                        cursor.test()?
                                    } else {
                                        cursor.skip()?
                                    };
                                    if split_after && !encrypted {
                                        // 只跟进同名且带 split-before 标志的目标续卷；遇到其他条目
                                        // 立即停止，避免把后续条目的错误归到当前预览上。
                                        loop {
                                            let Some(next) = open.read_header()? else {
                                                break;
                                            };
                                            let next_name = next
                                                .entry()
                                                .filename
                                                .to_string_lossy()
                                                .replace('\\', "/");
                                            if next_name != name || !next.entry().is_split_before()
                                            {
                                                break;
                                            }
                                            let next_split_after = next.entry().is_split_after();
                                            open = if next.entry().is_encrypted() {
                                                next.test()?
                                            } else {
                                                next.skip()?
                                            };
                                            if !next_split_after {
                                                break;
                                            }
                                        }
                                    }
                                    return Ok(Some((name, size, true)));
                                }
                                cursor.extract_to(&temp_file)?;
                                return Ok(Some((name, size, false)));
                            }
                            open = cursor.skip()?;
                        }
                        Ok(None)
                    })();
                match result {
                    Ok(Some((_name, size, true))) => {
                        return Err(anyhow::anyhow!("RAR_PREVIEW_TOO_LARGE:{size}"));
                    }
                    Ok(Some((name, _size, false))) => {
                        let mut file = std::fs::File::open(&temp_file)?;
                        let size = file.metadata()?.len();
                        let (data, truncated) = read_capped(&mut file)?;
                        return Ok(build_preview(&name, data, truncated, size));
                    }
                    Ok(None) => anyhow::bail!("条目不存在: {entry_path}"),
                    Err(e) => {
                        use unrar::error::Code;
                        if volume::is_missing_rar_volume(&e) {
                            return Err(anyhow::anyhow!("RAR_VOLUME_REQUIRED"));
                        }
                        if matches!(e.code, Code::MissingPassword | Code::BadPassword) {
                            last = anyhow::anyhow!("PASSWORD_REQUIRED");
                            continue;
                        }
                        return Err(anyhow::anyhow!("读取失败: {:?}", e.code));
                    }
                }
            }
            Err(last)
        }
        Format::Tar | Format::TarGz | Format::TarBz2 | Format::TarXz | Format::TarZst => {
            let r: Box<dyn Read + Send> = match format {
                Format::Tar => Box::new(BufReader::new(volume::open_reader(archive_path)?)),
                f => super::list::open_decompressor(archive_path, f)?,
            };
            let mut ar = tar::Archive::new(r);
            for entry in ar.entries()? {
                let mut entry = entry?;
                let raw = entry.path_bytes().to_vec();
                let name = decode_name(&raw, encoding);
                if name == entry_path {
                    let size = entry.header().size().unwrap_or(0);
                    let (data, truncated) = read_capped(&mut entry)?;
                    return Ok(build_preview(&name, data, truncated, size));
                }
            }
            anyhow::bail!("条目不存在: {entry_path}")
        }
        Format::Gz | Format::Bz2 | Format::Xz | Format::Zst => {
            let mut r = super::list::open_decompressor(archive_path, format)?;
            let (data, truncated) = read_capped(&mut r)?;
            let size = data.len() as u64;
            Ok(build_preview(entry_path, data, truncated, size))
        }
    }
}
