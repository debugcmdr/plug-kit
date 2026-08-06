//! Verify plugin manifests parse correctly with the real manifest model.
//! Regression test for the `missing field stdin_args` bug (camelCase mismatch).

use plugkit_lib::api::Manifest;

fn parse_plugin(id: &str) -> Manifest {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("plugins")
        .join(id)
        .join("manifest.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("parse {} manifest: {}", id, e))
}

#[test]
fn download_manifest_parses() {
    let m = parse_plugin("download");
    assert_eq!(m.id, "download");
    assert!(m.commands.contains_key("list-formats"));
    // stdin_args must deserialize from camelCase stdinArgs.
    let cmd = &m.commands["list-formats"];
    assert_eq!(cmd.stdin_args[0], "info", "stdinArgs should map to stdin_args");
    assert_eq!(cmd.output_mode, "json");
}

#[test]
fn convert_manifest_parses() {
    let m = parse_plugin("convert");
    assert!(m.commands.contains_key("convert"));
    assert_eq!(m.commands["convert"].output_mode, "progress+json");
}

#[test]
fn audio_extract_manifest_parses() {
    let m = parse_plugin("audio-extract");
    assert!(m.commands.contains_key("extract-audio"));
    assert!(m.commands.contains_key("convert-image"));
}

#[test]
fn download_manifest_has_no_binary_url() {
    // 运行时 manifest 不携带市场字段 binaryUrl(v5 设计:市场元数据在清单层)。
    let m = parse_plugin("download");
    assert!(m.binary_url.is_none(), "runtime manifest should not carry binaryUrl");
}
