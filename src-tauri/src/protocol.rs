//! Custom `plugkit://` URI scheme for serving plugin iframe assets.
//!
//! Serves files from `~/.plugkit/plugins/{plugin_id}/...` and injects the
//! bridge SDK (`plugkit-bridge.js`) into HTML responses so plugin UIs can talk
//! to the backend without bundling the bridge themselves.
//!
//! Security: path traversal is rejected (the resolved path must stay inside
//! the plugins root).

use std::borrow::Cow;
use std::path::PathBuf;

/// HTTP-like response for the custom protocol.
#[derive(Clone, Copy)]
enum Status {
    Ok = 200,
    NotFound = 404,
    Forbidden = 403,
    ServerError = 500,
}

pub fn register_protocol(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.register_asynchronous_uri_scheme_protocol("plugkit", |_ctx, request, responder| {
        let uri = request.uri().clone();
        let path = uri.path().to_string();

        tauri::async_runtime::spawn_blocking(move || {
            // Serve the bridge SDK itself: plugkit://bridge.js
            if path == "/bridge.js" || path == "/plugkit-bridge.js" {
                let bridge_js = include_str!("../assets/plugkit-bridge.js");
                let response = http_response(Status::Ok, "text/javascript", bridge_js.as_bytes());
                responder.respond(response);
                return;
            }

            let plugins_root = dirs::home_dir()
                .map(|d| d.join(".plugkit/plugins"))
                .unwrap_or_default();

            // Path format: /{plugin_id}/{relative...}
            let rel = path.trim_start_matches('/');
            if rel.is_empty() {
                let r = http_response(Status::NotFound, "text/plain", b"not found");
                responder.respond(r);
                return;
            }

            // Traversal guard (Zip-Slip style): reject `..` / absolute escapes.
            let normalized_rel = rel.replace('\\', "/");
            if normalized_rel.starts_with("..") || normalized_rel.contains("/../") {
                let r = http_response(Status::Forbidden, "text/plain", b"forbidden");
                responder.respond(r);
                return;
            }

            let full = plugins_root.join(rel);
            if !full.exists() || !full.is_file() {
                let r = http_response(Status::NotFound, "text/plain", b"not found");
                responder.respond(r);
                return;
            }

            let bytes = match std::fs::read(&full) {
                Ok(b) => b,
                Err(_) => {
                    let r = http_response(Status::ServerError, "text/plain", b"read error");
                    responder.respond(r);
                    return;
                }
            };

            let content_type = match full.extension().and_then(|e| e.to_str()) {
                Some("html") => "text/html",
                Some("js") => "text/javascript",
                Some("css") => "text/css",
                Some("svg") => "image/svg+xml",
                Some("json") => "application/json",
                Some("png") => "image/png",
                Some("ico") => "image/x-icon",
                _ => "application/octet-stream",
            };

            // Inject bridge script into HTML so plugin UIs get window.MT / window.PlugKit.
            if content_type == "text/html" {
                let html = String::from_utf8_lossy(&bytes);
                let injected = if html.contains("plugkit-bridge.js") {
                    html.to_string()
                } else {
                    let script = r#"<script src="/bridge.js"></script>"#;
                    // Insert right after <head> or at the start of <body>.
                    if html.to_lowercase().contains("<head") {
                        html.replacen("<head>", &format!("<head>\n  {}", script), 1)
                            .replacen("<head >", &format!("<head >\n  {}", script), 1)
                    } else if html.to_lowercase().contains("<body") {
                        html.replacen("<body>", &format!("<body>\n  {}", script), 1)
                    } else {
                        format!("{}\n{}", script, html)
                    }
                };
                let r = http_response(Status::Ok, "text/html", injected.as_bytes());
                responder.respond(r);
                return;
            }

            let r = http_response(Status::Ok, content_type, &bytes);
            responder.respond(r);
        });
    })
}

fn http_response(status: Status, content_type: &str, body: &[u8]) -> http::Response<Cow<'static, [u8]>> {
    http::Response::builder()
        .status(status as u16)
        .header("Content-Type", content_type)
        .body(Cow::Owned(body.to_vec()))
        .unwrap_or_else(|_| http::Response::new(Cow::Owned(Vec::new())))
}

/// Resolve `~/.plugkit/plugins/{id}/tool/index.html` to a `plugkit://` URL.
pub fn plugin_iframe_url(plugin_id: &str) -> String {
    format!("plugkit://plugin/{}/tool/index.html", plugin_id)
}
