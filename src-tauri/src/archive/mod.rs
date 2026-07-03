pub mod create;
pub mod edit;
pub mod extract;
pub mod list;
pub mod preview;

use serde::Serialize;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Zip,
    SevenZ,
    Rar,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    TarZst,
    Gz,
    Bz2,
    Xz,
    Zst,
}

impl Format {
    pub fn label(&self) -> &'static str {
        match self {
            Format::Zip => "ZIP",
            Format::SevenZ => "7Z",
            Format::Rar => "RAR",
            Format::Tar => "TAR",
            Format::TarGz => "TAR.GZ",
            Format::TarBz2 => "TAR.BZ2",
            Format::TarXz => "TAR.XZ",
            Format::TarZst => "TAR.ZST",
            Format::Gz => "GZ",
            Format::Bz2 => "BZ2",
            Format::Xz => "XZ",
            Format::Zst => "ZST",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    pub size: u64,
    pub compressed: u64,
    pub is_dir: bool,
    pub mtime: Option<i64>,
    pub encrypted: bool,
    pub crc: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    pub format: String,
    pub entries: Vec<Entry>,
    pub has_encrypted: bool,
    pub total_size: u64,
    pub total_compressed: u64,
    pub comment: Option<String>,
    /// Password that successfully opened the archive (provided or from the saved store).
    pub used_password: Option<String>,
}

pub fn detect_format(path: &Path) -> anyhow::Result<Format> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    let by_ext = if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some(Format::TarGz)
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tbz") {
        Some(Format::TarBz2)
    } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        Some(Format::TarXz)
    } else if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
        Some(Format::TarZst)
    } else if name.ends_with(".tar") {
        Some(Format::Tar)
    } else if name.ends_with(".zip") || name.ends_with(".jar") || name.ends_with(".apk") {
        Some(Format::Zip)
    } else if name.ends_with(".7z") {
        Some(Format::SevenZ)
    } else if name.ends_with(".rar") {
        Some(Format::Rar)
    } else if name.ends_with(".gz") {
        Some(Format::Gz)
    } else if name.ends_with(".bz2") {
        Some(Format::Bz2)
    } else if name.ends_with(".xz") {
        Some(Format::Xz)
    } else if name.ends_with(".zst") {
        Some(Format::Zst)
    } else {
        None
    };

    // 先按扩展名，取不到再看魔数。魔数还能识别无扩展名的裸 tar（ustar 魔数在偏移 257）。
    let prelim = match by_ext {
        Some(f) => f,
        None => detect_by_magic(path)?,
    };

    // Linux/UNIX 常见做法是「先 tar 打包再压缩」。若单层压缩（.gz/.bz2/.xz/.zst，
    // 或仅凭魔数判定）的解压内容其实是个 tar，则升级为对应的 tar.* 复合格式，
    // 这样即便文件名只是 foo.zst / foo.gz，也能直接展开里面的目录树而非当成单文件。
    let upgraded = match prelim {
        Format::Gz => upgrade_if_tar(path, Format::Gz, Format::TarGz),
        Format::Bz2 => upgrade_if_tar(path, Format::Bz2, Format::TarBz2),
        Format::Xz => upgrade_if_tar(path, Format::Xz, Format::TarXz),
        Format::Zst => upgrade_if_tar(path, Format::Zst, Format::TarZst),
        other => other,
    };
    Ok(upgraded)
}

/// 仅凭文件头魔数判定格式（无扩展名/扩展名不符时用）。
fn detect_by_magic(path: &Path) -> anyhow::Result<Format> {
    use std::io::Read;
    // 读 512 字节：既覆盖各压缩格式的短魔数，也够检查偏移 257 处的 tar「ustar」魔数。
    let mut head = [0u8; 512];
    let n = std::fs::File::open(path)?.read(&mut head)?;
    let head = &head[..n];
    let f = if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        Format::Zip
    } else if head.starts_with(b"7z\xBC\xAF\x27\x1C") {
        Format::SevenZ
    } else if head.starts_with(b"Rar!") {
        Format::Rar
    } else if head.starts_with(&[0x1F, 0x8B]) {
        Format::Gz
    } else if head.starts_with(b"BZh") {
        Format::Bz2
    } else if head.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        Format::Xz
    } else if head.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        Format::Zst
    } else if is_tar_header(head) {
        Format::Tar
    } else {
        anyhow::bail!("无法识别的归档格式: {}", path.display());
    };
    Ok(f)
}

/// tar 头部（POSIX ustar / GNU）在偏移 257 处含 "ustar" 魔数。
fn is_tar_header(buf: &[u8]) -> bool {
    buf.len() >= 262 && &buf[257..262] == b"ustar"
}

/// 若 `single` 单层压缩解出来的开头是个 tar，则返回 `tar_variant`，否则返回 `single`。
/// 只读前 512 字节解压结果，代价很低；任何读取/解压错误都安全回退到 `single`。
fn upgrade_if_tar(path: &Path, single: Format, tar_variant: Format) -> Format {
    match decompressed_head(path, single, 512) {
        Ok(head) if is_tar_header(&head) => tar_variant,
        _ => single,
    }
}

/// 用与 `single` 对应的解码器解压出至多 `n` 字节。
fn decompressed_head(path: &Path, single: Format, n: usize) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    let mut r = list::open_decompressor(path, single)?;
    let mut buf = vec![0u8; n];
    let mut filled = 0;
    while filled < n {
        let m = r.read(&mut buf[filled..])?;
        if m == 0 {
            break;
        }
        filled += m;
    }
    buf.truncate(filled);
    Ok(buf)
}

/// Sanitize an entry path coming from an archive: strip absolute prefixes and `..`.
pub fn sanitize_rel_path(name: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Name of the inner file for single-file compressed formats (foo.gz -> foo).
pub fn inner_name(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let lower = name.to_lowercase();
    for ext in [".gz", ".bz2", ".xz", ".zst"] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    format!("{name}.out")
}
