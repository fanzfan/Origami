use origami_lib::archive::create::{create, CreateOptions};
use origami_lib::archive::extract::{extract, test_archive, Ctx, ExtractOptions};
use origami_lib::archive::list::{list, ListOptions};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn ctx(app: &tauri::AppHandle<tauri::test::MockRuntime>) -> Ctx<'_, tauri::test::MockRuntime> {
    Ctx {
        app,
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

fn make_sources(dir: &Path) -> Vec<String> {
    let root = dir.join("数据目录");
    fs::create_dir_all(root.join("子目录")).unwrap();
    fs::write(root.join("说明文件.txt"), "你好，世界！hello world\n".repeat(100)).unwrap();
    fs::write(root.join("子目录/data.bin"), vec![0u8, 1, 2, 3, 255, 128]).unwrap();
    fs::write(dir.join("单独文件.md"), "# 标题\n内容").unwrap();
    vec![
        root.to_string_lossy().to_string(),
        dir.join("单独文件.md").to_string_lossy().to_string(),
    ]
}

fn create_opts(format: &str, password: Option<&str>) -> CreateOptions {
    CreateOptions {
        job_id: "t".into(),
        format: format.into(),
        level: 6,
        method: String::new(),
        password: password.map(String::from),
        volume_size: 0,
    }
}

fn extract_opts(password: Option<&str>, smart: bool) -> ExtractOptions {
    ExtractOptions {
        job_id: "t".into(),
        password: password.map(String::from),
        fallback_passwords: vec![],
        encoding: "auto".into(),
        entries: vec![],
        smart,
    }
}

fn list_opts(password: Option<&str>) -> ListOptions {
    ListOptions {
        password: password.map(String::from),
        encoding: "auto".into(),
        fallback_passwords: vec![],
    }
}

fn roundtrip(format: &str, ext: &str, password: Option<&str>) {
    roundtrip_m(format, ext, password, "");
}

fn roundtrip_m(format: &str, ext: &str, password: Option<&str>, method: &str) {
    let app = tauri::test::mock_app();
    let handle = app.handle();
    let tmp = tempfile::tempdir().unwrap();
    let sources = make_sources(tmp.path());
    let dest = tmp.path().join(format!("out.{ext}"));

    let mut copts = create_opts(format, password);
    copts.method = method.into();
    create(&ctx(handle), &dest, &sources, &copts).unwrap();
    assert!(dest.exists(), "{format}: archive not created");

    let info = list(&dest, &list_opts(password)).unwrap();
    let paths: Vec<&str> = info.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.contains("说明文件.txt")),
        "{format}: missing utf8 名称, got {paths:?}"
    );
    assert!(paths.iter().any(|p| p.contains("data.bin")), "{format}: missing data.bin");

    let out = tmp.path().join("extracted");
    fs::create_dir_all(&out).unwrap();
    let final_dest = extract(&ctx(handle), &dest, &out, &extract_opts(password, true)).unwrap();
    let base = PathBuf::from(&final_dest);
    // smart mode with two top-level roots -> wraps in "out" folder
    assert!(base.ends_with("out"), "{format}: smart dir expected, got {final_dest}");
    let content = fs::read_to_string(base.join("数据目录/说明文件.txt")).unwrap();
    assert!(content.contains("你好，世界"), "{format}: content mismatch");
    let bin = fs::read(base.join("数据目录/子目录/data.bin")).unwrap();
    assert_eq!(bin, vec![0u8, 1, 2, 3, 255, 128], "{format}: binary mismatch");
    assert!(base.join("单独文件.md").exists(), "{format}: second source missing");

    // integrity test
    test_archive(&ctx(handle), &dest, "t", password.map(String::from), vec![]).unwrap();
}

#[test]
fn zip_roundtrip() {
    roundtrip("zip", "zip", None);
}

#[test]
fn zip_encrypted_roundtrip() {
    roundtrip("zip", "zip", Some("s3cret!密码"));
}

#[test]
fn sevenz_roundtrip() {
    roundtrip("7z", "7z", None);
}

#[test]
fn sevenz_encrypted_roundtrip() {
    roundtrip("7z", "7z", Some("s3cret!密码"));
}

#[test]
fn tar_gz_roundtrip() {
    roundtrip("tar.gz", "tar.gz", None);
}

#[test]
fn tar_xz_roundtrip() {
    roundtrip("tar.xz", "tar.xz", None);
}

#[test]
fn tar_zst_roundtrip() {
    roundtrip("tar.zst", "tar.zst", None);
}

#[test]
fn tar_bz2_roundtrip() {
    roundtrip("tar.bz2", "tar.bz2", None);
}

#[test]
fn plain_tar_roundtrip() {
    roundtrip("tar", "tar", None);
}

#[test]
fn zip_wrong_password_fails() {
    let app = tauri::test::mock_app();
    let handle = app.handle();
    let tmp = tempfile::tempdir().unwrap();
    let sources = make_sources(tmp.path());
    let dest = tmp.path().join("out.zip");
    create(&ctx(handle), &dest, &sources, &create_opts("zip", Some("right"))).unwrap();

    let out = tmp.path().join("x");
    fs::create_dir_all(&out).unwrap();
    let err = extract(&ctx(handle), &dest, &out, &extract_opts(Some("wrong"), true)).unwrap_err();
    assert!(format!("{err}").contains("PASSWORD_REQUIRED"), "got: {err}");
}

#[test]
fn fallback_password_is_tried() {
    let app = tauri::test::mock_app();
    let handle = app.handle();
    let tmp = tempfile::tempdir().unwrap();
    let sources = make_sources(tmp.path());
    let dest = tmp.path().join("out.7z");
    create(&ctx(handle), &dest, &sources, &create_opts("7z", Some("hidden"))).unwrap();

    let out = tmp.path().join("x");
    fs::create_dir_all(&out).unwrap();
    let mut opts = extract_opts(None, true);
    opts.fallback_passwords = vec!["nope".into(), "hidden".into()];
    extract(&ctx(handle), &dest, &out, &opts).unwrap();
    assert!(out.join("out/数据目录/说明文件.txt").exists());
}

#[test]
fn selective_extract() {
    let app = tauri::test::mock_app();
    let handle = app.handle();
    let tmp = tempfile::tempdir().unwrap();
    let sources = make_sources(tmp.path());
    let dest = tmp.path().join("out.zip");
    create(&ctx(handle), &dest, &sources, &create_opts("zip", None)).unwrap();

    let out = tmp.path().join("x");
    fs::create_dir_all(&out).unwrap();
    let mut opts = extract_opts(None, false);
    opts.entries = vec!["数据目录/子目录".into()];
    extract(&ctx(handle), &dest, &out, &opts).unwrap();
    assert!(out.join("数据目录/子目录/data.bin").exists());
    assert!(!out.join("数据目录/说明文件.txt").exists());
    assert!(!out.join("单独文件.md").exists());
}

#[test]
fn smart_extract_single_root_no_wrap() {
    let app = tauri::test::mock_app();
    let handle = app.handle();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("only");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.txt"), "x").unwrap();
    let dest = tmp.path().join("single.zip");
    create(
        &ctx(handle),
        &dest,
        &[root.to_string_lossy().to_string()],
        &create_opts("zip", None),
    )
    .unwrap();

    let out = tmp.path().join("x");
    fs::create_dir_all(&out).unwrap();
    let final_dest = extract(&ctx(handle), &dest, &out, &extract_opts(None, true)).unwrap();
    // single root "only" -> no wrapper folder
    assert_eq!(PathBuf::from(final_dest), out);
    assert!(out.join("only/a.txt").exists());
}

#[test]
fn volume_split_and_preview() {
    let app = tauri::test::mock_app();
    let handle = app.handle();
    let tmp = tempfile::tempdir().unwrap();
    let big = tmp.path().join("big.bin");
    fs::write(&big, vec![7u8; 300 * 1024]).unwrap();
    let dest = tmp.path().join("vol.zip");
    let mut opts = create_opts("zip", None);
    opts.volume_size = 100 * 1024;
    opts.level = 0; // store so it stays big
    let first = create(
        &ctx(handle),
        &dest,
        &[big.to_string_lossy().to_string()],
        &opts,
    )
    .unwrap();
    assert!(first.ends_with(".001"), "got {first}");
    assert!(tmp.path().join("vol.zip.002").exists());
    assert!(!dest.exists());

    // preview on a text file in a fresh zip
    let txtdir = tmp.path().join("t");
    fs::create_dir_all(&txtdir).unwrap();
    fs::write(txtdir.join("hello.txt"), "预览内容 OK").unwrap();
    let dest2 = tmp.path().join("p.zip");
    create(
        &ctx(handle),
        &dest2,
        &[txtdir.join("hello.txt").to_string_lossy().to_string()],
        &create_opts("zip", None),
    )
    .unwrap();
    let pv = origami_lib::archive::preview::preview_entry(&dest2, "hello.txt", None, vec![], "auto").unwrap();
    assert_eq!(pv.kind, "text");
    assert!(pv.text.unwrap().contains("预览内容"));
}

#[test]
fn zip_method_roundtrips() {
    for m in ["bzip2", "zstd", "xz", "ppmd", "store"] {
        roundtrip_m("zip", "zip", None, m);
    }
}

#[test]
fn sevenz_method_roundtrips() {
    for m in ["bzip2", "zstd", "ppmd", "copy"] {
        roundtrip_m("7z", "7z", None, m);
    }
}

fn edit_roundtrip(format: &str, ext: &str, password: Option<&str>) {
    use origami_lib::archive::edit::{add_files, remove_entries, EditOptions};
    let app = tauri::test::mock_app();
    let handle = app.handle();
    let tmp = tempfile::tempdir().unwrap();
    let sources = make_sources(tmp.path());
    let dest = tmp.path().join(format!("e.{ext}"));
    create(&ctx(handle), &dest, &sources, &create_opts(format, password)).unwrap();

    let eopts = EditOptions {
        job_id: "t".into(),
        password: password.map(String::from),
        encoding: "auto".into(),
    };
    // 添加一个新文件到 数据目录/ 下
    let newfile = tmp.path().join("新增.txt");
    fs::write(&newfile, "added content 新增").unwrap();
    add_files(
        &ctx(handle),
        &dest,
        &[newfile.to_string_lossy().to_string()],
        "数据目录",
        &eopts,
    )
    .unwrap();
    // 删除原有文件与一个目录
    remove_entries(
        &ctx(handle),
        &dest,
        &["数据目录/子目录".into(), "单独文件.md".into()],
        &eopts,
    )
    .unwrap();

    let info = list(&dest, &list_opts(password)).unwrap();
    let paths: Vec<&str> = info.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.trim_end_matches('/') == "数据目录/新增.txt"),
        "{format}: added file missing, got {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.contains("data.bin") || p.contains("单独文件.md")),
        "{format}: removed entries still present, got {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.contains("说明文件.txt")),
        "{format}: untouched entry lost, got {paths:?}"
    );

    // 解压验证内容
    let out = tmp.path().join("edited-out");
    fs::create_dir_all(&out).unwrap();
    extract(&ctx(handle), &dest, &out, &extract_opts(password, false)).unwrap();
    let added = fs::read_to_string(out.join("数据目录/新增.txt")).unwrap();
    assert!(added.contains("新增"), "{format}: added content mismatch");
    assert!(out.join("数据目录/说明文件.txt").exists());
    assert!(!out.join("数据目录/子目录").exists());
}

#[test]
fn zip_edit() {
    edit_roundtrip("zip", "zip", None);
}

#[test]
fn zip_edit_encrypted() {
    edit_roundtrip("zip", "zip", Some("pw编辑"));
}

#[test]
fn sevenz_edit() {
    edit_roundtrip("7z", "7z", None);
}

#[test]
fn tar_gz_edit() {
    edit_roundtrip("tar.gz", "tar.gz", None);
}

#[test]
fn gbk_filename_detection() {
    // Build a zip with GBK-encoded filename bytes (no UTF-8 flag).
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("gbk.zip");
    {
        use std::io::Write;
        let f = fs::File::create(&dest).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let (gbk_bytes, _, _) = encoding_rs::GBK.encode("中文文档.txt");
        let name = unsafe { String::from_utf8_unchecked(gbk_bytes.into_owned()) };
        let opts: zip::write::SimpleFileOptions = Default::default();
        w.start_file(name, opts).unwrap();
        w.write_all("gbk test".as_bytes()).unwrap();
        w.finish().unwrap();
    }
    let info = list(&dest, &list_opts(None)).unwrap();
    assert_eq!(info.entries.len(), 1);
    assert_eq!(info.entries[0].path, "中文文档.txt", "auto-detect GBK failed: {:?}", info.entries[0].path);
}
