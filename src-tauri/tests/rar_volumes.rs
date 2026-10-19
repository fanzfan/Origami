use origami_lib::archive::extract::{extract, Ctx, ExtractOptions};
use origami_lib::archive::list::{list, ListOptions};
use origami_lib::archive::preview::preview_entry;
use origami_lib::passwords::LazyPasswords;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

// 官方 libarchive 的真实 RAR 原生分卷：一个 20111 字节的文件跨越三卷。
// 来源与许可见 fixtures/rar/NOTICE.txt；测试不依赖网络或本机 RAR 命令行工具。
const PARTS: [&[u8]; 3] = [
    include_bytes!("fixtures/rar/test_rar_multivolume_single_file.part1.rar"),
    include_bytes!("fixtures/rar/test_rar_multivolume_single_file.part2.rar"),
    include_bytes!("fixtures/rar/test_rar_multivolume_single_file.part3.rar"),
];
const ENTRY: &str = "LibarchiveAddingTest.html";

fn part_path(dir: &Path, part: usize, padded: bool) -> PathBuf {
    if padded {
        dir.join(format!("sample.part{part:02}.rar"))
    } else {
        dir.join(format!("sample.part{part}.rar"))
    }
}

fn write_parts(dir: &Path, numbers: &[usize], padded: bool) {
    for &number in numbers {
        std::fs::write(part_path(dir, number, padded), PARTS[number - 1]).unwrap();
    }
}

fn list_options() -> ListOptions {
    ListOptions {
        password: None,
        encoding: "auto".into(),
        fallback_passwords: LazyPasswords::none(),
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[test]
fn native_rar_extracts_complete_content_from_every_volume() {
    let app = tauri::test::mock_app();
    let ctx = Ctx {
        app: app.handle(),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    for padded in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        write_parts(dir.path(), &[1, 2, 3], padded);
        for part in 1..=3 {
            let path = part_path(dir.path(), part, padded);
            let info = list(&path, &list_options()).unwrap();
            assert_eq!(info.entries.len(), 1);
            assert_eq!(info.entries[0].path, ENTRY);
            assert_eq!(info.entries[0].size, 20111);
            assert_eq!(info.entries[0].crc, None);
            let volumes = info.volumes.unwrap();
            assert_eq!(volumes.count, 3);
            assert_eq!(Path::new(&volumes.first), part_path(dir.path(), 1, padded));
            assert!(volumes.missing.is_empty());

            let opts = ExtractOptions {
                job_id: format!("rar-{padded}-{part}"),
                password: None,
                fallback_passwords: LazyPasswords::none(),
                encoding: "auto".into(),
                entries: Vec::new(),
                smart: false,
            };
            let dest = dir.path().join(format!("out-{part}"));
            extract(&ctx, &path, &dest, &opts).unwrap();
            let data = std::fs::read(dest.join(ENTRY)).unwrap();
            assert_eq!(data.len(), 20111);
            // 完整文件 CRC 来自第三卷的最终文件头，前两卷只保存各自分段 CRC。
            assert_eq!(crc32(&data), 1_577_428_579);
            assert!(data.starts_with(b"<!DOCTYPE HTML PUBLIC"));
            assert!(data.ends_with(b"<P STYLE=\"margin-bottom: 0in\"><BR>\n</P>\n</BODY>\n</HTML>"));

            let preview = preview_entry(&path, ENTRY, None, LazyPasswords::none(), "auto").unwrap();
            assert_eq!(preview.text.unwrap().as_bytes(), data);
        }
    }
}

#[test]
fn native_rar_reports_missing_first_and_middle_volumes() {
    for (numbers, missing) in [(&[2, 3][..], 1), (&[1, 3][..], 2)] {
        let dir = tempfile::tempdir().unwrap();
        write_parts(dir.path(), numbers, false);
        let error = list(&part_path(dir.path(), 3, false), &list_options()).unwrap_err().to_string();
        assert!(error.contains("MISSING_VOLUMES"), "{error}");
        assert!(error.contains(&format!("sample.part{missing}.rar")), "{error}");
    }
}

#[test]
fn native_rar_reports_missing_last_volume_without_partial_success() {
    let dir = tempfile::tempdir().unwrap();
    write_parts(dir.path(), &[1, 2], false);
    let error = list(&part_path(dir.path(), 1, false), &list_options()).unwrap_err().to_string();
    assert_eq!(error, "RAR_VOLUME_REQUIRED");
    let path = part_path(dir.path(), 2, false);
    let error = preview_entry(&path, ENTRY, None, LazyPasswords::none(), "auto").err().expect("缺末卷时预览必须失败").to_string();
    assert_eq!(error, "RAR_VOLUME_REQUIRED");
    let app = tauri::test::mock_app();
    let ctx = Ctx {
        app: app.handle(),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let opts = ExtractOptions {
        job_id: "rar-missing-tail".into(),
        password: None,
        fallback_passwords: LazyPasswords::none(),
        encoding: "auto".into(),
        entries: Vec::new(),
        smart: false,
    };
    let dest = dir.path().join("out");
    let error = extract(&ctx, &path, &dest, &opts).unwrap_err().to_string();
    assert_eq!(error, "RAR_VOLUME_REQUIRED");
    assert!(!dest.join(ENTRY).exists());
}
