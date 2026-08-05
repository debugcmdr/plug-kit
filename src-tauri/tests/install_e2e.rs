//! End-to-end test: verify the plugin zip structure can be installed.
//! Uses the freshly packaged zips in the repo's dist/plugins/.

#[test]
fn packaged_zip_has_correct_structure() {
    let manifest_zip = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("dist/plugins/download-0.1.0.zip");

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
