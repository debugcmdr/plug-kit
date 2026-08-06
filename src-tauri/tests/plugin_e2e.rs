//! End-to-end test for the plugin system.
//! Uses the `download` built-in plugin CLI (repo plugins/download).

use std::process::Command;

fn plugin_exe(id: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    format!("{}/plugins/{}/tool/plugkit-{}", root.display(), id, id)
}

#[test]
fn download_plugin_cli_echo_protocol() {
    let out = Command::new(plugin_exe("download"))
        .output()
        .expect("download plugin should run");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let last = stdout.trim().lines().last().unwrap();
    let v: serde_json::Value = serde_json::from_str(last).expect("valid JSON line");
    assert_eq!(v["type"], "result");
    assert_eq!(v["success"], true);
    // usage data returned
    assert!(v["data"]["usage"].as_str().is_some());
}

#[test]
fn convert_plugin_cli_progress_protocol() {
    let out = Command::new(plugin_exe("convert"))
        .output()
        .expect("convert plugin should run");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let last = stdout.trim().lines().last().unwrap();
    let v: serde_json::Value = serde_json::from_str(last).expect("valid JSON line");
    assert_eq!(v["type"], "result");
    assert_eq!(v["success"], true);
    assert!(v["data"]["usage"].as_str().is_some());
}
