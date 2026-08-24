//! Custom `plugkit://` URI scheme for serving plugin iframe assets.
//!
//! Serves files from `~/.plugkit/plugins/{plugin_id}/...` and injects the
//! bridge SDK (`plugkit-bridge.js`) into HTML responses so plugin UIs can talk
//! to the backend without bundling the bridge themselves.
//!
//! Security: path traversal is rejected (the resolved path must stay inside
//! the plugins root).

use std::borrow::Cow;

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
                    // 插件页面严格 CSP(插件是不可信代码,是主要威胁面):
                    // - script 仅允许 plugkit: 同源 + 内联(插件 HTML 多为内联脚本)
                    // - 禁止网络请求(connect-src 'none')——插件网络能力一律走后端桥接
                    // - 禁止 object/frame/base 注入
                    // 注:iframe 已去掉 allow-same-origin,origin 为 opaque,'self' 无法匹配
                    // plugkit:// 资源,故 script/img/font 用 scheme-source 显式放行。
                    let csp = "default-src 'none'; script-src plugkit: 'unsafe-inline'; \
                              style-src 'unsafe-inline'; img-src plugkit: data: https:; \
                              font-src plugkit: data:; connect-src 'none'; object-src 'none'; \
                              base-uri 'none'; form-action 'none'";
                    let meta = format!(
                        r#"<meta http-equiv="Content-Security-Policy" content="{}">"#,
                        csp
                    );
                    let script = r#"<script src="/bridge.js"></script>"#;
                    // 在 <head...> 标签之后注入,兼容大小写(<HEAD>)与带属性(<head id="x">)。
                    // 修复:原先 replacen("<head>") 大小写敏感,大写/带属性标签会注入失败
                    // → 插件拿不到 window.MT,UI 白屏。to_ascii_lowercase 逐字节处理,
                    // 索引与 html 严格一致,可安全切片。
                    let lower_ascii = html.to_ascii_lowercase();
                    let inject_after = if let Some(h) = lower_ascii.find("<head") {
                        html[h..].find('>').map(|gt| h + gt + 1)
                    } else if let Some(b) = lower_ascii.find("<body") {
                        html[b..].find('>').map(|gt| b + gt + 1)
                    } else {
                        None
                    };
                    match inject_after {
                        Some(at) => {
                            let mut out = String::with_capacity(
                                html.len() + meta.len() + script.len() + 16,
                            );
                            out.push_str(&html[..at]);
                            out.push_str("\n  ");
                            out.push_str(&meta);
                            out.push('\n');
                            out.push_str(script);
                            out.push('\n');
                            out.push_str(&html[at..]);
                            out
                        }
                        None => format!("{}\n{}\n{}", meta, script, html),
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
        // 插件文件禁用缓存:开发期插件文件频繁变更(热更新),若 WebView 缓存了
        // plugkit:// 响应,改完插件后 iframe 仍加载旧内容——曾导致"同步了 dev
        // 副本但界面不变"。bridge.js 为编译期内置,no-store 亦无副作用。
        .header("Cache-Control", "no-store")
        .body(Cow::Owned(body.to_vec()))
        .unwrap_or_else(|_| http::Response::new(Cow::Owned(Vec::new())))
}
