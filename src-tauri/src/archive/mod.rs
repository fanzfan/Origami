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
    if let Some(f) = by_ext {
        return Ok(f);
    }

    // Fall back to magic bytes.
    use std::io::Read;
    let mut head = [0u8; 8];
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
    } else {
        anyhow::bail!("无法识别的归档格式: {}", path.display())
    };
    Ok(f)
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
