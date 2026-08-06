//! Regression: ToolRunner must resolve the plugin's real executable (entry.executable),
//! NOT index.html (previously find_executable picked the first non-exe/dll file,
//! which was index.html -> CLI calls failed silently).

use plugkit_lib::api::PluginManager;

#[tokio::test]
async fn tool_runner_calls_download_cli_not_index() {
    // Ensure download plugin is installed (preinstall may have it; else install).
    let pm = PluginManager::new();
    let home = std::env::var("HOME").unwrap();
    let dl_dir = std::path::Path::new(&home).join(".plugkit/plugins/download");
    if !dl_dir.join("manifest.json").exists() {
        let r = pm.install("download").await.expect("install download");
        assert!(r.success);
    }

    // Invoke download plugin's no-arg command (returns usage JSON). If ToolRunner
    // wrongly executes index.html, this will fail with spawn/parse error.
    let runner = plugkit_lib::api::ToolRunner::new();
    let result = runner.invoke("download", "download", serde_json::json!({ "url": "https://example.com" }), None).await;
    // Even with a bad URL, the CLI should return a structured error (proving the
    // Python CLI ran), not a spawn failure.
    match result {
        Ok(v) => assert!(v.is_object(), "expected object result, got {:?}", v),
        Err(e) => {
            // Error from the CLI (e.g. MISSING_URL/YTDLP) is fine; a spawn error is not.
            assert!(!e.contains("Failed to spawn") && !e.contains("No executable"),
                "ToolRunner failed to spawn the real CLI: {}", e);
        }
    }
}
