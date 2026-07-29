pub mod create;
pub mod edit;
pub mod extract;
pub mod list;
pub mod preview;
pub mod volume;

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
    /// 分卷信息；非分卷为 None。
    pub volumes: Option<VolumeInfo>,
}

/// 分卷归档的概要，用于界面提示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub kind: volume::VolumeKind,
    /// 现存卷数。
    pub count: usize,
    pub total_size: u64,
    /// 缺失的卷文件名（空表示完整）。
    pub missing: Vec<String>,
    /// 首卷路径（供界面展示 / 定位）。
    pub first: String,
}

impl From<&volume::VolumeSet> for VolumeInfo {
    fn from(s: &volume::VolumeSet) -> Self {
        Self {
            kind: s.kind,
            count: s.parts.len(),
            total_size: s.total_size,
            missing: s.missing.clone(),
            first: s.first.to_string_lossy().to_string(),
        }
    }
}

pub fn detect_format(path: &Path) -> anyhow::Result<Format> {
    // 分卷按「逻辑名」判定格式：foo.7z.001 看的是 foo.7z。
    let logical = volume::logical_path(path);
    let name = logical
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

    // 扩展名只是提示，内容才是依据：读文件头嗅探真实容器格式。
    // 两者冲突时以内容为准——名字改错、下载站乱改后缀、微信/邮件把 .7z 存成 .zip
    // 这类情况很常见，照着名字选解码器只会得到一句「不是有效的 XX 文件」。
    let magic = sniff_container(path);
    let prelim = match (by_ext, magic) {
        // 一致：保留扩展名，因为它比魔数多带了 tar.* 这层信息。
        (Some(e), Some(m)) if container_of(e) == m => e,
        (Some(_), Some(m)) => m,
        // 认不出内容（文件太短、或是本程序不支持的格式）：只能信名字。
        (Some(e), None) => e,
        (None, Some(m)) => m,
        // 名字和头部都没结论：可能是自解压包（前面挂着一段可执行代码），
        // 再往后扫一段找特征字段。
        (None, None) => deep_sniff(path)
            .ok_or_else(|| anyhow::anyhow!("无法识别的归档格式: {}", path.display()))?,
    };

    // Linux/UNIX 常见做法是「先 tar 打包再压缩」，而单层压缩的魔数看不出里面是不是
    // tar，扩展名也可能写错。所以对 .gz/.bz2/.xz/.zst 一律解出前 512 字节实测：
    // 是 tar 就用复合格式展开目录树（foo.zst 里其实是 tar），不是就按单文件处理
    // （foo.tar.gz 里其实只有一个文件，名字里的 .tar 是假的）。
    let refined = match prelim {
        Format::Gz | Format::TarGz => tar_variant(path, prelim, Format::Gz, Format::TarGz),
        Format::Bz2 | Format::TarBz2 => tar_variant(path, prelim, Format::Bz2, Format::TarBz2),
        Format::Xz | Format::TarXz => tar_variant(path, prelim, Format::Xz, Format::TarXz),
        Format::Zst | Format::TarZst => tar_variant(path, prelim, Format::Zst, Format::TarZst),
        other => other,
    };
    Ok(refined)
}

/// 复合格式对应的「外层容器」：魔数只看得到这一层。
fn container_of(f: Format) -> Format {
    match f {
        Format::TarGz => Format::Gz,
        Format::TarBz2 => Format::Bz2,
        Format::TarXz => Format::Xz,
        Format::TarZst => Format::Zst,
        other => other,
    }
}

/// 各格式的头部魔数。放在一处便于与 `deep_sniff` 共用。
const SIGNATURES: &[(&[u8], Format)] = &[
    (b"PK\x03\x04", Format::Zip),
    (b"PK\x05\x06", Format::Zip), // 空 zip
    (b"7z\xBC\xAF\x27\x1C", Format::SevenZ),
    (b"Rar!\x1A\x07", Format::Rar),
    (&[0x1F, 0x8B], Format::Gz),
    (b"BZh", Format::Bz2),
    (&[0xFD, b'7', b'z', b'X', b'Z', 0x00], Format::Xz),
    (&[0x28, 0xB5, 0x2F, 0xFD], Format::Zst),
];

/// 读文件头判定外层容器格式；认不出返回 None（不是错误，交给调用方决定怎么办）。
fn sniff_container(path: &Path) -> Option<Format> {
    use std::io::Read;
    // 读 512 字节：既覆盖各格式的短魔数，也够检查偏移 257 处的 tar「ustar」魔数。
    let mut head = [0u8; 512];
    let n = volume::open_reader(path).ok()?.read(&mut head).ok()?;
    let head = &head[..n];
    if let Some((_, f)) = SIGNATURES.iter().find(|(sig, _)| head.starts_with(sig)) {
        return Some(*f);
    }
    is_tar_header(head).then_some(Format::Tar)
}

/// 自解压包（SFX）会在归档前面挂一段可执行代码，头部魔数落在中间。
/// 只在前 `DEEP_SNIFF_LIMIT` 字节里找 7z / RAR 的 6 字节特征——够长，撞上随机数据
/// 的概率可以忽略。ZIP 的 `PK\x03\x04` 只有 4 字节，在任意二进制里都可能碰巧出现，
/// 所以不扫它，改用「真的打开一次」来判定（见下）。
const DEEP_SNIFF_LIMIT: usize = 1024 * 1024;

fn deep_sniff(path: &Path) -> Option<Format> {
    if let Some(f) = scan_signature(path) {
        return Some(f);
    }
    // ZIP 的中央目录在文件尾部，zip 库会自己跳过前面的 stub。能开出目录就是 zip，
    // 比在数据里碰运气找 PK 头可靠得多，代价也只是尾部几十 KB 的定位。
    let opened = volume::open_reader(path)
        .ok()
        .map(|r| zip::ZipArchive::new(r).is_ok())
        .unwrap_or(false);
    opened.then_some(Format::Zip)
}

fn scan_signature(path: &Path) -> Option<Format> {
    use std::io::Read;
    let mut buf = vec![0u8; DEEP_SNIFF_LIMIT];
    let mut r = volume::open_reader(path).ok()?;
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => break,
        }
    }
    let buf = &buf[..filled];
    // 取位置最靠前的那个特征，避免归档内部的字节被当成起点。
    SIGNATURES
        .iter()
        .filter(|(_, f)| matches!(f, Format::Rar | Format::SevenZ))
        .filter_map(|(sig, f)| find_bytes(buf, sig).map(|at| (at, *f)))
        .min_by_key(|(at, _)| *at)
        .map(|(_, f)| f)
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// tar 头部（POSIX ustar / GNU）在偏移 257 处含 "ustar" 魔数。
fn is_tar_header(buf: &[u8]) -> bool {
    buf.len() >= 262 && &buf[257..262] == b"ustar"
}

/// 实测单层压缩里装的是不是 tar：是则用 `tar_v`，不是则用 `single`。
/// 解码失败说明这个流本来就有问题，此时不该再改判断，原样返回 `prelim`。
fn tar_variant(path: &Path, prelim: Format, single: Format, tar_v: Format) -> Format {
    match decompressed_head(path, single, 512) {
        Ok(head) if is_tar_header(&head) => tar_v,
        Ok(_) => single,
        Err(_) => prelim,
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

#[cfg(test)]
mod detect_tests {
    use super::*;
    use std::io::Write;

    fn write(name: &str, bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        (dir, p)
    }

    fn gz(payload: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(payload).unwrap();
        e.finish().unwrap()
    }

    fn tar_bytes() -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        let data = b"hello";
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, "f.txt", &data[..]).unwrap();
        b.into_inner().unwrap()
    }

    fn zip_bytes() -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        w.start_file::<_, ()>("f.txt", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"hello").unwrap();
        w.finish().unwrap().into_inner()
    }

    // 名字说 .zip，内容是 gzip：以内容为准。
    #[test]
    fn content_wins_over_wrong_extension() {
        let (_d, p) = write("actually-gzip.zip", &gz(b"plain text payload"));
        assert_eq!(detect_format(&p).unwrap(), Format::Gz);
    }

    // 名字说 .7z，内容是 zip：以内容为准。
    #[test]
    fn content_wins_for_zip_named_7z() {
        let (_d, p) = write("mislabeled.7z", &zip_bytes());
        assert_eq!(detect_format(&p).unwrap(), Format::Zip);
    }

    // 名字带 .tar，里面其实只有一个普通文件：降级为单文件 gz，别用 tar 解码器去啃。
    #[test]
    fn downgrades_fake_tar_gz() {
        let (_d, p) = write("notes.tar.gz", &gz(b"just text, no tar header here at all"));
        assert_eq!(detect_format(&p).unwrap(), Format::Gz);
    }

    // 反向：名字只有 .gz，里面确实是 tar，仍要升级成 TAR.GZ。
    #[test]
    fn still_upgrades_real_tar_gz() {
        let (_d, p) = write("backup.gz", &gz(&tar_bytes()));
        assert_eq!(detect_format(&p).unwrap(), Format::TarGz);
    }

    // 自解压包：前面挂了一段可执行代码，头部魔数落在中间。
    #[test]
    fn finds_archive_behind_sfx_stub() {
        let mut bytes = vec![0x4Du8, 0x5A]; // "MZ"，PE 可执行头
        bytes.extend(std::iter::repeat_n(0u8, 4096));
        bytes.extend(zip_bytes());
        let (_d, p) = write("installer.exe", &bytes);
        assert_eq!(detect_format(&p).unwrap(), Format::Zip);
    }

    // 扩展名认得、内容读不出结论（文件太短）：保留扩展名的判断。
    #[test]
    fn keeps_extension_when_content_is_inconclusive() {
        let (_d, p) = write("tiny.tar", b"short");
        assert_eq!(detect_format(&p).unwrap(), Format::Tar);
    }

    // 名字和内容都给不出结论：明确报错，而不是随便挑一个格式。
    #[test]
    fn unknown_content_errors() {
        let (_d, p) = write("mystery.dat", b"not an archive by any measure");
        assert!(detect_format(&p).is_err());
    }

    // 普通二进制里碰巧出现 "PK\x03\x04" 不该被当成 zip：深度嗅探对 zip
    // 只认「真能打开中央目录」，不认散落的 4 字节。
    #[test]
    fn stray_pk_bytes_are_not_a_zip() {
        let mut bytes = vec![0u8; 8192];
        bytes[4000..4004].copy_from_slice(b"PK\x03\x04");
        let (_d, p) = write("video.bin", &bytes);
        assert!(detect_format(&p).is_err());
    }
}

/// Name of the inner file for single-file compressed formats (foo.gz -> foo).
pub fn inner_name(path: &Path) -> String {
    let logical = volume::logical_path(path);
    let name = logical.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let lower = name.to_lowercase();
    for ext in [".gz", ".bz2", ".xz", ".zst"] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    format!("{name}.out")
}
