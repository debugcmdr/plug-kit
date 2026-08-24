//! End-to-end test: verify the plugin zip structure can be installed.
//! Uses the freshly packaged zips in the repo's dist/plugins/.

#[test]
fn packaged_zip_has_correct_structure() {
    // 动态查找 release/plugins/ 下任意版本的 download-*.zip(避免硬编码版本号
    // 与当前插件版本脱节——曾锚定 0.1.0 而实际已 0.3.0)
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap();
    let plugin_dir = manifest_dir.join("release/plugins");
    let manifest_zip = std::fs::read_dir(&plugin_dir).unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name().and_then(|n| n.to_str()).map_or(false, |n| {
                n.starts_with("download-") && n.ends_with(".zip")
            })
        })
        .expect("packaged download zip should exist (run ./scripts/package-plugins.sh)");

    let bytes = std::fs::read(&manifest_zip).expect("packaged zip should exist");
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).expect("valid zip");

    let mut names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        names.push(archive.by_index(i).unwrap().name().to_string());
    }

    // Must contain manifest.json and the executable + html under a top-level dir.
    assert!(names.iter().any(|n| n.ends_with("manifest.json")), "missing manifest.json, got {names:?}");
    assert!(names.iter().any(|n| n.ends_with("plugkit-download")), "missing executable");
    assert!(names.iter().any(|n| n.ends_with("index.html")), "missing index.html");

    // No traversal entries.
    assert!(!names.iter().any(|n| n.contains("..")), "zip contains traversal!");
}
