use std::path::PathBuf;

#[test]
fn finder_workflows_are_valid_plists() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("HOME", tmp.path());
    std::env::set_var("ORIGAMI_NO_FINDER_RESTART", "1");

    open_bandizip_lib::services::install().unwrap();
    assert!(open_bandizip_lib::services::installed());

    let dir = tmp.path().join("Library/Services");
    let workflows: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(workflows.len(), 3, "expected 3 workflows: {workflows:?}");

    for wf in &workflows {
        for name in ["Info.plist", "document.wflow"] {
            let p = wf.join("Contents").join(name);
            let st = std::process::Command::new("/usr/bin/plutil")
                .arg("-lint")
                .arg(&p)
                .status()
                .unwrap();
            assert!(st.success(), "invalid plist: {p:?}");
        }
        let wflow = std::fs::read_to_string(wf.join("Contents/document.wflow")).unwrap();
        assert!(wflow.contains("origami://create?format="));
        assert!(wflow.contains("runWorkflowAsService") || wf.join("Contents/Info.plist").exists());
    }

    open_bandizip_lib::services::uninstall().unwrap();
    assert!(!open_bandizip_lib::services::installed());
}
