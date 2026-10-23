use origami_lib::archive::preview::preview_entry;
use origami_lib::passwords::LazyPasswords;
use std::path::{Path, PathBuf};

const PARTS: [&[u8]; 3] = [
    include_bytes!("fixtures/rar/test_rar_multivolume_single_file.part1.rar"),
    include_bytes!("fixtures/rar/test_rar_multivolume_single_file.part2.rar"),
    include_bytes!("fixtures/rar/test_rar_multivolume_single_file.part3.rar"),
];
const ENTRY: &str = "LibarchiveAddingTest.html";
const MAX_PREVIEW: u64 = 8 * 1024 * 1024;

fn part_path(dir: &Path, index: usize) -> PathBuf {
    dir.join(format!("oversized.part{index}.rar"))
}

fn rar_crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn write_parts_with_declared_size(dir: &Path, count: usize, size: u64) {
    for (index, source) in PARTS.iter().take(count).enumerate() {
        let mut bytes = source.to_vec();
        // 该官方 RAR4 fixture 的文件头从偏移 20 开始，声明大小位于 31..35。
        bytes[31..35].copy_from_slice(&(size as u32).to_le_bytes());
        let header_size = u16::from_le_bytes([bytes[25], bytes[26]]) as usize;
        let checksum = rar_crc32(&bytes[22..20 + header_size]) as u16;
        bytes[20..22].copy_from_slice(&checksum.to_le_bytes());
        std::fs::write(part_path(dir, index + 1), bytes).unwrap();
    }
}

#[test]
fn oversized_rar_preview_is_rejected_before_materializing_entry() {
    let dir = tempfile::tempdir().unwrap();
    write_parts_with_declared_size(dir.path(), 3, MAX_PREVIEW + 1);

    let error = match preview_entry(
        &part_path(dir.path(), 1),
        ENTRY,
        None,
        LazyPasswords::none(),
        "auto",
    ) {
        Ok(_) => panic!("超限 RAR 预览应在解码前被拒绝"),
        Err(error) => error.to_string(),
    };

    assert!(error.contains("RAR_PREVIEW_TOO_LARGE"), "{error}");
    assert!(error.contains(&(MAX_PREVIEW + 1).to_string()), "{error}");
}

#[test]
fn oversized_split_rar_keeps_missing_volume_error() {
    let dir = tempfile::tempdir().unwrap();
    write_parts_with_declared_size(dir.path(), 2, MAX_PREVIEW + 1);

    let error = match preview_entry(
        &part_path(dir.path(), 1),
        ENTRY,
        None,
        LazyPasswords::none(),
        "auto",
    ) {
        Ok(_) => panic!("缺末卷的超限 RAR 应返回缺卷错误"),
        Err(error) => error.to_string(),
    };

    assert_eq!(error, "RAR_VOLUME_REQUIRED");
}
