//! End-to-end install regression test.
//! Verifies that packaged plugin zips extract to the CORRECT layout (no
//! double-nesting) and that the Zip-Slip guard accepts them (regression for
//! the "Absolute path in archive entry" bug).

use std::io::Cursor;
use zip::ZipArchive;

fn read_zip(id: &str) -> Vec<u8> {
    // 动态查找 release/plugins/ 下任意版本的 {id}-*.zip(避免硬编码版本号与当前脱节)
    let plugin_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("release/plugins");
    let path = std::fs::read_dir(&plugin_dir).unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name().and_then(|n| n.to_str()).map_or(false, |n| {
                n.starts_with(&format!("{}-", id)) && n.ends_with(".zip")
            })
        })
        .unwrap_or_else(|| {
            panic!("no packaged {}-*.zip in release/plugins (run ./scripts/package-plugins.sh)", id)
        });
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e))
}

/// Validate that extraction into dest produces manifest.json at the TOP level
/// (not nested under a folder). Mirrors the logic the app now uses.
fn extract_and_verify_layout(id: &str) {
    let bytes = read_zip(id);
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).expect("valid zip");

    // Determine common top dir (same logic as plugin_manager::common_top_dir).
    let mut common: Option<String> = None;
    for i in 0..archive.len() {
        let name = archive.by_index(i).unwrap().name().trim_end_matches('/').to_string();
        if name.is_empty() || name.starts_with("__MACOSX") { continue; }
        let top = name.split('/').next().unwrap().to_string();
        match &common {
            None => common = Some(top),
            Some(existing) if *existing == top => {}
            Some(_) => { common = None; break; }
        }
    }

    // Every file must resolve to manifest.json at the root after stripping the
    // common top dir (or directly if no common dir).
    let strip_with_slash = common.as_ref().map(|p| format!("{}/", p));
    let mut saw_manifest = false;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        if entry.name().ends_with('/') { continue; }
        let name = entry.name().trim_end_matches('/');
        let rel = match &strip_with_slash {
            Some(prefix) => name.strip_prefix(prefix.as_str()).unwrap_or(name),
            None => name,
        };
        if rel == "manifest.json" { saw_manifest = true; }
        // No traversal / absolute entries allowed.
        assert!(!rel.starts_with('/') && !rel.split('/').any(|c| c == ".."),
            "unsafe entry path: {}", rel);
    }
    assert!(saw_manifest, "{}: expected manifest.json at archive root after strip", id);
}

#[test]
fn download_zip_layout_correct() { extract_and_verify_layout("download"); }

#[test]
fn convert_zip_layout_correct() { extract_and_verify_layout("convert"); }

/// Regression: the Zip-Slip guard must ACCEPT our packaged zips (they used to
/// fail with "Absolute path in archive entry").
#[test]
fn security_guard_accepts_packaged_zip() {
    let dest = std::path::Path::new("/tmp/plugkit-install-test");
    let _ = std::fs::remove_dir_all(dest);
    let bytes = read_zip("convert");
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).unwrap();

    let mut ok = true;
    for i in 0..archive.len() {
        let name = archive.by_index(i).unwrap().name().trim_end_matches('/').to_string();
        let full = dest.join(&name);
        if plugkit_lib::api::validate_unzip_path(dest, &full).is_err() {
            ok = false;
            break;
        }
    }
    assert!(ok, "packaged zip entries should pass the Zip-Slip guard");
}
