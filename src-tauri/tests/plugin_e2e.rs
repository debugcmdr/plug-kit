//! End-to-end test for the plugin system.
//! Requires the `hello` test plugin to be present at ~/.plugkit/plugins/hello.

use std::process::Command;

fn hello_exe() -> String {
    let home = std::env::var("HOME").unwrap();
    format!("{}/.plugkit/plugins/hello/tool/hello.py", home)
}

#[test]
fn hello_plugin_cli_echo_protocol() {
    let out = Command::new(hello_exe())
        .args(["--echo", "测试"])
        .output()
        .expect("hello plugin should run");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let last = stdout.trim().lines().last().unwrap();
    let v: serde_json::Value = serde_json::from_str(last).expect("valid JSON line");
    assert_eq!(v["type"], "result");
    assert_eq!(v["success"], true);
    assert_eq!(v["data"]["echo"], "测试");
}

#[test]
fn hello_plugin_cli_progress_protocol() {
    let out = Command::new(hello_exe())
        .args(["--download"])
        .output()
        .expect("hello plugin should run");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert!(lines.len() >= 3, "expected progress lines, got {}", lines.len());

    // Progress lines parse as tagged JSON.
    let progress: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(progress["type"], "progress");
    assert!(progress["percent"].as_f64().is_some());

    // Final line is a result.
    let result: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
    assert_eq!(result["type"], "result");
    assert_eq!(result["data"]["filename"], "test_video.mp4");
}
