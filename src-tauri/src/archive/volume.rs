//! 分卷压缩包的识别与读取。
//!
//! 常见的分卷有两类：
//!
//! 1. **裸切分**：`foo.7z.001` / `foo.zip.002` / `foo.001`……把单个归档文件按字节
//!    平均切成若干片（7-Zip 的 `-v` 参数、本程序的分卷输出都属于这种）。把各片
//!    按序首尾相接即还原出原始归档，因此用 [`ConcatReader`] 拼成一条虚拟的
//!    `Read + Seek` 流交给各解码器即可，无需落盘。
//! 2. **格式原生分卷**：RAR 的 `foo.part1.rar` / `foo.rar` + `foo.r00`，以及 ZIP 的
//!    `foo.z01` + `foo.zip`。每一片自带卷头，不能简单拼接。RAR 由 unrar 自己续卷，
//!    我们只要把第一卷的路径交给它；ZIP 跨卷 zip 库不支持，识别出来给一句人话提示。

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VolumeKind {
    /// `.001` / `.002`……字节切分，可直接拼接。
    Numeric,
    /// `foo.partN.rar`，由 unrar 自行续卷。
    RarPart,
    /// `foo.rar` + `foo.r00` / `foo.r01`……老式 RAR 分卷，同样由 unrar 续卷。
    RarOld,
    /// `foo.zip` + `foo.z01`……跨卷 ZIP，本程序暂不支持。
    ZipSpanned,
}

/// RAR 卷号可能来自损坏或恶意文件名；缺卷提示只保留有限条目，避免在错误消息中
/// 分配/遍历数十亿个卷号。`VolumeInfo.missing` 因此可能是截断后的文件名列表。
const MAX_REPORTED_RAR_MISSING: u32 = 32;

impl VolumeKind {
    /// 该类型能否用 [`ConcatReader`] 直接拼接读取。
    pub fn concatenable(self) -> bool {
        matches!(self, VolumeKind::Numeric)
    }
}

#[derive(Debug, Clone)]
pub struct VolumeSet {
    pub kind: VolumeKind,
    /// 按卷号升序排列的现存分卷（对 RAR/ZIP 原生分卷仅用于展示与缺卷检查）。
    pub parts: Vec<PathBuf>,
    /// 逻辑归档路径：用于判定格式、生成解压目录名。
    /// 如 `foo.7z.001` → `foo.7z`；`foo.part1.rar` → `foo.rar`。
    pub base: PathBuf,
    /// 交给按路径工作的原生库（unrar）的入口卷。
    pub first: PathBuf,
    pub total_size: u64,
    /// 中间缺失的卷文件名（按序列推断），非空说明分卷不完整。
    pub missing: Vec<String>,
}

impl VolumeSet {
    pub fn is_split(&self) -> bool {
        self.parts.len() > 1 || !self.missing.is_empty()
    }
}

/// 把用户点中的任意一卷解析成完整的分卷集合；不是分卷则返回 `None`。
pub fn detect(path: &Path) -> Option<VolumeSet> {
    let name = path.file_name()?.to_str()?;
    let dir = path.parent().unwrap_or(Path::new("."));
    detect_numeric(dir, name)
        .or_else(|| detect_rar_part(dir, name))
        .or_else(|| detect_rar_old(dir, name))
        .or_else(|| detect_zip_spanned(dir, name))
}

/// 逻辑归档路径：分卷集合取 `base`，否则就是它自己。
pub fn logical_path(path: &Path) -> PathBuf {
    detect(path).map(|s| s.base).unwrap_or_else(|| path.to_path_buf())
}

/// 交给 unrar 之类按路径工作的库的入口卷路径。
pub fn entry_path(path: &Path) -> PathBuf {
    detect(path).map(|s| s.first).unwrap_or_else(|| path.to_path_buf())
}

/// 打开归档字节流：裸切分自动跨卷拼接，其余就是普通文件。
pub fn open_reader(path: &Path) -> anyhow::Result<VolumeReader> {
    match detect(path) {
        Some(set) if set.kind.concatenable() => {
            ensure_complete(&set)?;
            Ok(VolumeReader::Multi(ConcatReader::open(&set.parts)?))
        }
        Some(set) if set.kind == VolumeKind::ZipSpanned => {
            let _ = set;
            anyhow::bail!("SPANNED_ZIP_UNSUPPORTED")
        }
        Some(set) => Ok(VolumeReader::Single(BufReader::new(File::open(&set.first)?))),
        None => Ok(VolumeReader::Single(BufReader::new(File::open(path)?))),
    }
}

/// 归档总字节数（分卷则为各卷之和）。
pub fn total_len(path: &Path) -> anyhow::Result<u64> {
    match detect(path) {
        Some(set) => Ok(set.total_size),
        None => Ok(std::fs::metadata(path)?.len()),
    }
}

pub fn ensure_complete(set: &VolumeSet) -> anyhow::Result<()> {
    if set.missing.is_empty() {
        return Ok(());
    }
    anyhow::bail!("MISSING_VOLUMES: {}", set.missing.join(", "))
}

/// unrar 在列表自动续卷或处理文件续卷时，以 `EOpen@Read/Process` 报告缺少尾卷；
/// 此时目录扫描无法推断缺少的卷号，统一交给上层显示稳定错误标记。
pub fn is_missing_rar_volume(error: &unrar::error::UnrarError) -> bool {
    use unrar::error::{Code, When};
    matches!((error.code, error.when), (Code::EOpen, When::Read | When::Process))
}

/// unrar 只能按路径打开。若 RAR 是裸切分（`foo.rar.001`），先合并到临时文件，
/// 返回的 `TempPath` 在作用域结束时自动删除。
pub fn materialize(path: &Path) -> anyhow::Result<Option<tempfile::TempPath>> {
    let Some(set) = detect(path) else { return Ok(None) };
    if !set.kind.concatenable() || set.parts.len() <= 1 {
        return Ok(None);
    }
    ensure_complete(&set)?;
    let ext = set
        .base
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_string();
    let mut tmp = tempfile::Builder::new()
        .prefix("origami-vol-")
        .suffix(&format!(".{ext}"))
        .tempfile()?;
    let mut r = ConcatReader::open(&set.parts)?;
    std::io::copy(&mut r, &mut tmp)?;
    Ok(Some(tmp.into_temp_path()))
}

/// 供只能按路径工作的原生库（unrar）使用的归档路径。
/// 原生分卷直接给首卷；裸切分则先合并到临时文件，`NativePath` 析构时删除。
pub struct NativePath {
    path: PathBuf,
    _tmp: Option<tempfile::TempPath>,
}

impl NativePath {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn native_path(path: &Path) -> anyhow::Result<NativePath> {
    let set = detect(path);
    if let Some(set) = &set {
        if matches!(set.kind, VolumeKind::RarPart | VolumeKind::RarOld) {
            ensure_complete(&set)?;
        }
    }
    // 原生分卷直接复用本次检测的首卷，避免在同一个目录重复扫描三次。
    if set.as_ref().is_some_and(|s| s.kind.concatenable()) {
        if let Some(tmp) = materialize(path)? {
            return Ok(NativePath { path: tmp.to_path_buf(), _tmp: Some(tmp) });
        }
    }
    Ok(NativePath {
        path: set.map(|s| s.first).unwrap_or_else(|| path.to_path_buf()),
        _tmp: None,
    })
}

// ---------------- 各类分卷的命名规则 ----------------

/// `base.NNN`（NNN 至少两位数字）。要求 1 号卷存在，否则不是分卷。
fn detect_numeric(dir: &Path, name: &str) -> Option<VolumeSet> {
    let (stem, digits) = name.rsplit_once('.')?;
    if digits.len() < 2 || !digits.bytes().all(|b| b.is_ascii_digit()) || stem.is_empty() {
        return None;
    }
    let width = digits.len();
    let part_name = |i: u32| format!("{stem}.{i:0width$}", width = width);
    if !dir.join(part_name(1)).exists() {
        return None;
    }
    let (parts, missing) = collect_sequence(dir, 1, &part_name);
    let total_size = parts.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
    Some(VolumeSet {
        kind: VolumeKind::Numeric,
        base: dir.join(stem),
        first: parts[0].clone(),
        parts,
        total_size,
        missing,
    })
}

/// `foo.partN.rar` / `foo.partNN.rar`（大小写不敏感）。
///
/// RAR 工具通常不会为未补零的卷号保持固定宽度（`part1`、`part2`、`part10`），
/// 所以不能根据用户点中的那一卷直接格式化出 `part01`。这里扫描同目录下相同前缀
/// 的卷名，按卷号排序后再推断中间缺失项。
fn detect_rar_part(dir: &Path, name: &str) -> Option<VolumeSet> {
    // 只折叠 ASCII 大小写，避免 Unicode lower-case 改变字节偏移，后面可以安全地用
    // 原文件名切片生成逻辑路径。
    let lower = name.to_ascii_lowercase();
    let rest = lower.strip_suffix(".rar")?;
    let idx = rest.rfind(".part")?;
    let selected_digits = &rest[idx + 5..];
    if selected_digits.is_empty() || !selected_digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let stem = &name[..idx];
    let prefix = format!("{}.part", stem.to_ascii_lowercase());
    let mut by_number: BTreeMap<u32, (PathBuf, String)> = BTreeMap::new();
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let file_name = match entry.file_name().into_string() {
            Ok(file_name) => file_name,
            Err(_) => continue,
        };
        let lower_name = file_name.to_ascii_lowercase();
        let Some(number_text) = lower_name
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(".rar"))
        else {
            continue;
        };
        if number_text.is_empty() || !number_text.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(number) = number_text.parse::<u32>() else {
            continue;
        };
        if number == 0 {
            continue;
        }
        by_number
            .entry(number)
            .or_insert_with(|| (entry.path(), file_name));
    }

    let selected_number = selected_digits.parse::<u32>().ok()?;
    // `read_dir` 的顺序不稳定；若大小写不同的重复文件存在，优先保留用户点中的那一卷。
    let selected_path = dir.join(name);
    if selected_path.is_file() {
        by_number.insert(selected_number, (selected_path, name.to_string()));
    }
    let first = by_number
        .get(&1)
        .or_else(|| by_number.values().next())
        .map(|(path, _)| path.clone())?;

    let max_number = *by_number.keys().next_back()?;
    let padded = by_number.values().any(|(_, n)| {
        let lower = n.to_ascii_lowercase();
        lower
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(".rar"))
            .is_some_and(|digits| digits.len() > 1 && digits.starts_with('0'))
    });
    let width = by_number
        .values()
        .filter_map(|(_, n)| {
            let lower = n.to_ascii_lowercase();
            lower
                .strip_prefix(&prefix)
                .and_then(|s| s.strip_suffix(".rar"))
                .map(str::len)
        })
        .max()
        .unwrap_or(selected_digits.len());
    let missing_name = |number: u32| {
        let digits = if padded {
            format!("{number:0width$}", width = width)
        } else {
            number.to_string()
        };
        format!("{stem}.part{digits}.rar")
    };
    // 只沿现有卷号之间的间隙取前 N 个缺卷名；不遍历到 max_number，避免 part
    // 后跟一个极大的恶意卷号时卡住。即使前 4096 卷齐全，最后一个间隙也会被找到。
    let mut missing = Vec::new();
    let mut candidate = 1u64;
    for &number in by_number.keys() {
        let number = number as u64;
        while candidate < number && missing.len() < MAX_REPORTED_RAR_MISSING as usize {
            missing.push(missing_name(candidate as u32));
            candidate += 1;
        }
        candidate = number.saturating_add(1);
        if missing.len() == MAX_REPORTED_RAR_MISSING as usize {
            break;
        }
    }
    while candidate <= max_number as u64 && missing.len() < MAX_REPORTED_RAR_MISSING as usize {
        missing.push(missing_name(candidate as u32));
        candidate += 1;
    }
    let parts = by_number
        .into_values()
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    let total_size = parts.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
    Some(VolumeSet {
        kind: VolumeKind::RarPart,
        base: dir.join(format!("{stem}.rar")),
        first,
        parts,
        total_size,
        missing,
    })
}

/// 老式 RAR 分卷：`foo.rar` + `foo.r00` + `foo.r01`……
fn detect_rar_old(dir: &Path, name: &str) -> Option<VolumeSet> {
    let lower = name.to_lowercase();
    let stem = if lower.ends_with(".rar") {
        &name[..name.len() - 4]
    } else {
        let (s, ext) = name.rsplit_once('.')?;
        let ext_l = ext.to_lowercase();
        let digits = ext_l.strip_prefix('r')?;
        if digits.len() != 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s
    };
    let head = dir.join(format!("{stem}.rar"));
    if !head.exists() || !dir.join(format!("{stem}.r00")).exists() {
        return None;
    }
    let part_name = |i: u32| format!("{stem}.r{:02}", i);
    let (mut parts, missing) = collect_sequence(dir, 0, &part_name);
    parts.insert(0, head.clone());
    let total_size = parts.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
    Some(VolumeSet {
        kind: VolumeKind::RarOld,
        base: head.clone(),
        first: head,
        parts,
        total_size,
        missing,
    })
}

/// 跨卷 ZIP：`foo.z01`…… + 末卷 `foo.zip`。
fn detect_zip_spanned(dir: &Path, name: &str) -> Option<VolumeSet> {
    let lower = name.to_lowercase();
    let stem = if lower.ends_with(".zip") {
        &name[..name.len() - 4]
    } else {
        let (s, ext) = name.rsplit_once('.')?;
        let ext_l = ext.to_lowercase();
        let digits = ext_l.strip_prefix('z')?;
        if digits.len() < 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s
    };
    let last = dir.join(format!("{stem}.zip"));
    if !last.exists() || !dir.join(format!("{stem}.z01")).exists() {
        return None;
    }
    let part_name = |i: u32| format!("{stem}.z{:02}", i);
    let (mut parts, missing) = collect_sequence(dir, 1, &part_name);
    parts.push(last.clone());
    let total_size = parts.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
    Some(VolumeSet {
        kind: VolumeKind::ZipSpanned,
        base: last.clone(),
        first: last,
        parts,
        total_size,
        missing,
    })
}

/// 从 `start` 起按名字生成器顺序收集现存文件；连续缺 3 个视为序列结束。
fn collect_sequence(
    dir: &Path,
    start: u32,
    name_of: &dyn Fn(u32) -> String,
) -> (Vec<PathBuf>, Vec<String>) {
    let mut parts = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut missing = Vec::new();
    let mut gap = 0u32;
    let mut i = start;
    while gap < 3 {
        let n = name_of(i);
        let p = dir.join(&n);
        if p.exists() {
            missing.append(&mut pending);
            parts.push(p);
            gap = 0;
        } else {
            gap += 1;
            pending.push(n);
        }
        i += 1;
    }
    (parts, missing)
}

// ---------------- 拼接读取 ----------------

pub enum VolumeReader {
    Single(BufReader<File>),
    Multi(ConcatReader),
}

impl Read for VolumeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            VolumeReader::Single(r) => r.read(buf),
            VolumeReader::Multi(r) => r.read(buf),
        }
    }
}

impl Seek for VolumeReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            VolumeReader::Single(r) => r.seek(pos),
            VolumeReader::Multi(r) => r.seek(pos),
        }
    }
}

/// 把若干分卷首尾相接，对外表现为一条连续的 `Read + Seek` 流。
pub struct ConcatReader {
    /// (路径, 长度)
    parts: Vec<(PathBuf, u64)>,
    /// 每卷在虚拟流中的起始偏移。
    offsets: Vec<u64>,
    total: u64,
    pos: u64,
    /// 当前打开的卷；其文件游标始终与 `pos` 对齐。
    cur: Option<(usize, BufReader<File>)>,
}

impl ConcatReader {
    pub fn open(paths: &[PathBuf]) -> std::io::Result<Self> {
        let mut parts = Vec::with_capacity(paths.len());
        let mut offsets = Vec::with_capacity(paths.len());
        let mut total = 0u64;
        for p in paths {
            let len = std::fs::metadata(p)?.len();
            offsets.push(total);
            total += len;
            parts.push((p.clone(), len));
        }
        Ok(Self { parts, offsets, total, pos: 0, cur: None })
    }

    fn index_of(&self, pos: u64) -> usize {
        match self.offsets.binary_search(&pos) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }
}

impl Read for ConcatReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if self.pos >= self.total {
                return Ok(0);
            }
            let idx = self.index_of(self.pos);
            if self.cur.as_ref().map(|(i, _)| *i) != Some(idx) {
                let mut br = BufReader::with_capacity(64 * 1024, File::open(&self.parts[idx].0)?);
                br.seek(SeekFrom::Start(self.pos - self.offsets[idx]))?;
                self.cur = Some((idx, br));
            }
            let part_end = self.offsets[idx] + self.parts[idx].1;
            let want = buf.len().min((part_end - self.pos) as usize);
            let n = self.cur.as_mut().unwrap().1.read(&mut buf[..want])?;
            if n == 0 {
                // 这一卷比登记的长度短（读取期间被改动），跳到下一卷继续。
                self.pos = part_end;
                self.cur = None;
                continue;
            }
            self.pos += n as u64;
            return Ok(n);
        }
    }
}

impl Seek for ConcatReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::End(n) => self.total as i128 + n as i128,
            SeekFrom::Current(n) => self.pos as i128 + n as i128,
        };
        if target < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek 到负偏移",
            ));
        }
        let target = target as u64;
        // 落在当前卷内则就地移动游标，否则丢弃、下次读取时重开。
        match self.cur.as_mut() {
            Some((i, br))
                if target >= self.offsets[*i] && target < self.offsets[*i] + self.parts[*i].1 =>
            {
                br.seek(SeekFrom::Start(target - self.offsets[*i]))?;
            }
            _ => self.cur = None,
        }
        self.pos = target;
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn split_fixture(name: &str, chunks: &[&[u8]]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        for (i, c) in chunks.iter().enumerate() {
            let p = dir.path().join(format!("{name}.{:03}", i + 1));
            std::fs::File::create(&p).unwrap().write_all(c).unwrap();
        }
        let first = dir.path().join(format!("{name}.001"));
        (dir, first)
    }

    #[test]
    fn detects_numeric_set_from_any_part() {
        let (dir, _) = split_fixture("a.7z", &[b"aaa", b"bbb", b"ccc"]);
        let set = detect(&dir.path().join("a.7z.002")).unwrap();
        assert_eq!(set.kind, VolumeKind::Numeric);
        assert_eq!(set.parts.len(), 3);
        assert_eq!(set.base, dir.path().join("a.7z"));
        assert_eq!(set.first, dir.path().join("a.7z.001"));
        assert_eq!(set.total_size, 9);
        assert!(set.missing.is_empty());
    }

    #[test]
    fn reports_missing_middle_volume() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["b.zip.001", "b.zip.003"] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let set = detect(&dir.path().join("b.zip.001")).unwrap();
        assert_eq!(set.missing, vec!["b.zip.002".to_string()]);
        assert!(ensure_complete(&set).is_err());
    }

    #[test]
    fn plain_file_is_not_a_volume() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("plain.zip"), b"x").unwrap();
        assert!(detect(&dir.path().join("plain.zip")).is_none());
    }

    // 形如 backup.2024 的普通文件不该被当成分卷（不存在 backup.0001）。
    #[test]
    fn year_suffix_is_not_a_volume() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("backup.2024"), b"x").unwrap();
        assert!(detect(&dir.path().join("backup.2024")).is_none());
    }

    #[test]
    fn concat_reads_and_seeks_across_parts() {
        let (dir, first) = split_fixture("c.bin", &[b"0123", b"4567", b"89"]);
        let set = detect(&first).unwrap();
        let mut r = ConcatReader::open(&set.parts).unwrap();
        let mut all = Vec::new();
        r.read_to_end(&mut all).unwrap();
        assert_eq!(all, b"0123456789");

        r.seek(SeekFrom::Start(3)).unwrap();
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"3456");
        r.seek(SeekFrom::End(-2)).unwrap();
        let mut tail = String::new();
        r.read_to_string(&mut tail).unwrap();
        assert_eq!(tail, "89");
        drop(dir);
    }

    #[test]
    fn detects_rar_part_set() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["d.part1.rar", "d.part2.rar"] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let set = detect(&dir.path().join("d.part2.rar")).unwrap();
        assert_eq!(set.kind, VolumeKind::RarPart);
        assert_eq!(set.first, dir.path().join("d.part1.rar"));
        assert_eq!(set.base, dir.path().join("d.rar"));
    }

    #[test]
    fn detects_unpadded_rar_parts_from_a_later_volume() {
        let dir = tempfile::tempdir().unwrap();
        for i in 1..=10 {
            std::fs::write(dir.path().join(format!("d.part{i}.rar")), b"x").unwrap();
        }
        let picked = dir.path().join("d.part10.rar");
        let set = detect(&picked).unwrap();
        assert_eq!(set.kind, VolumeKind::RarPart);
        assert_eq!(set.first, dir.path().join("d.part1.rar"));
        assert_eq!(
            set.parts
                .iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            (1..=10)
                .map(|i| format!("d.part{i}.rar"))
                .collect::<Vec<_>>()
        );
        assert!(set.missing.is_empty());
        assert_eq!(
            native_path(&picked).unwrap().path(),
            dir.path().join("d.part1.rar")
        );
    }

    #[test]
    fn reports_only_missing_rar_parts_between_existing_volumes() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["d.part1.rar", "d.part3.rar"] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let set = detect(&dir.path().join("d.part3.rar")).unwrap();
        assert_eq!(set.missing, vec!["d.part2.rar"]);
        assert!(ensure_complete(&set)
            .unwrap_err()
            .to_string()
            .contains("d.part2.rar"));
    }

    #[test]
    fn missing_first_rar_part_is_reported_before_unrar() {
        let dir = tempfile::tempdir().unwrap();
        let picked = dir.path().join("d.part2.rar");
        std::fs::write(&picked, b"x").unwrap();
        let set = detect(&picked).unwrap();
        assert_eq!(set.first, picked);
        assert_eq!(set.missing, vec!["d.part1.rar"]);
        let err = match native_path(&picked) {
            Ok(_) => panic!("缺首卷时 native_path 应失败"),
            Err(err) => err.to_string(),
        };
        assert!(err.contains("MISSING_VOLUMES") && err.contains("d.part1.rar"));
    }

    #[test]
    fn recognizes_only_missing_next_rar_volume_error() {
        use unrar::error::{Code, UnrarError, When};

        assert!(is_missing_rar_volume(&UnrarError::from(Code::EOpen, When::Process)));
        assert!(!is_missing_rar_volume(&UnrarError::from(Code::EOpen, When::Open)));
        assert!(!is_missing_rar_volume(&UnrarError::from(Code::ERead, When::Process)));
    }

    #[test]
    fn bounds_missing_rar_part_report_for_large_volume_number() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("d.part1.rar"), b"x").unwrap();
        let picked = dir.path().join("d.part4294967295.rar");
        std::fs::write(&picked, b"x").unwrap();
        let set = detect(&picked).unwrap();
        assert_eq!(set.missing.len(), MAX_REPORTED_RAR_MISSING as usize);
        assert_eq!(set.missing.first().unwrap(), "d.part2.rar");
        assert_eq!(set.missing.last().unwrap(), "d.part33.rar");
    }

    #[test]
    fn materializes_split_rar_fixture_before_native_processing() {
        use base64::Engine;

        // unrar crate 自带的真实 RAR（包含 VERSION = "unrar-0.4.0"），在测试中按
        // 字节切成两卷，验证只能按路径工作的 unrar 仍能读回跨卷后的完整内容。
        const RAR: &str = "UmFyIRoHAM+QcwAADQAAAAAAAAAPDHQggCcAFQAAAAsAAAADRfN9xqSKB0cdMwcApIEAAFZFUlNJT04MAI/sikXMI8hICINi/l/dXFOI8HLEPXsAQAcA";
        let bytes = base64::engine::general_purpose::STANDARD.decode(RAR).unwrap();
        let split_at = bytes.len() / 2;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("fixture.rar.001"), &bytes[..split_at]).unwrap();
        std::fs::write(dir.path().join("fixture.rar.002"), &bytes[split_at..]).unwrap();

        let native = native_path(&dir.path().join("fixture.rar.002")).unwrap();
        assert_eq!(std::fs::read(native.path()).unwrap(), bytes);
        let archive = unrar::Archive::new(native.path())
            .open_for_processing()
            .unwrap();
        let cursor = archive.read_header().unwrap().unwrap();
        assert_eq!(cursor.entry().filename, PathBuf::from("VERSION"));
        let (data, _next) = cursor.read().unwrap();
        assert_eq!(data, b"unrar-0.4.0");
    }

    #[test]
    fn detects_zero_padded_rar_parts_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["D.PART01.RAR", "D.PART02.RAR"] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let set = detect(&dir.path().join("D.PART02.RAR")).unwrap();
        assert_eq!(set.first, dir.path().join("D.PART01.RAR"));
        assert_eq!(set.base, dir.path().join("D.rar"));
        assert!(set.missing.is_empty());
    }

    #[test]
    fn detects_old_rar_set() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["e.rar", "e.r00", "e.r01"] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let set = detect(&dir.path().join("e.r01")).unwrap();
        assert_eq!(set.kind, VolumeKind::RarOld);
        assert_eq!(set.parts.len(), 3);
        assert_eq!(set.first, dir.path().join("e.rar"));
    }

    #[test]
    fn detects_spanned_zip() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["f.z01", "f.z02", "f.zip"] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let set = detect(&dir.path().join("f.zip")).unwrap();
        assert_eq!(set.kind, VolumeKind::ZipSpanned);
        assert_eq!(set.parts.len(), 3);
        assert!(open_reader(&dir.path().join("f.zip")).is_err());
    }

    // 单卷 .001（7-Zip 只切出一卷时的产物）也应能识别并正常读取。
    #[test]
    fn single_numeric_part_is_readable() {
        let (dir, first) = split_fixture("g.7z", &[b"hello"]);
        let set = detect(&first).unwrap();
        assert!(!set.is_split());
        let mut r = open_reader(&first).unwrap();
        let mut s = String::new();
        r.read_to_string(&mut s).unwrap();
        assert_eq!(s, "hello");
        drop(dir);
    }
}
