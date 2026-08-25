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
                // 插件页面严格 CSP(插件是不可信代码,是主要威胁面):
                // - script 仅允许 plugkit: 同源 + 内联(插件 HTML 多为内联脚本)
                // - 禁止网络请求(connect-src 'none')——插件网络能力一律走后端桥接
                // - 禁止 object/frame/base 注入
                // 注:iframe 已去掉 allow-same-origin,origin 为 opaque,'self' 无法匹配
                // plugkit:// 资源,故 script/img/font 用 scheme-source 显式放行。
                // CSP 必须无条件注入(R-12):此前 HTML 含 "plugkit-bridge.js" 字符串即
                // 豁免,任何插件只要自己引 bridge 就绕过网络限制——威胁面敞口。
                let csp = "default-src 'none'; script-src plugkit: 'unsafe-inline'; \
                          style-src 'unsafe-inline'; img-src plugkit: data: https:; \
                          font-src plugkit: data:; connect-src 'none'; object-src 'none'; \
                          base-uri 'none'; form-action 'none'";
                let meta = format!(
                    r#"<meta http-equiv="Content-Security-Policy" content="{}">"#,
                    csp
                );
                // 插件自己已引用 bridge.js 时不再注入 script(避免重复加载);
                // CSP meta 无论如何都注入。
                let script = if html.contains("plugkit-bridge.js") {
                    ""
                } else {
                    r#"<script src="/bridge.js"></script>"#
                };
                // 在 <head...> 标签之后注入,兼容大小写(<HEAD>)与带属性(<head id="x">)。
                // ⚠️ 必须精确匹配 <head 标签(R-13):朴素前缀匹配会误命中 <header> 等
                // 标签,把 meta CSP 注入到 body 区域——HTML 规范中 meta CSP 仅在
                // <head> 内生效,注入到 body 即防护失效。
                let lower_ascii = html.to_ascii_lowercase();
                let inject_after = find_head_tag(&lower_ascii)
                    .or_else(|| find_body_tag(&lower_ascii))
                    .and_then(|h| html[h..].find('>').map(|gt| h + gt + 1));
                let injected = match inject_after {
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

/// 定位 `<head` 标签起始下标(小写输入)。要求 <head 后紧跟空白/`>`/`/`(自闭合)或
/// 文本结束——排除 `<header`/`<headline` 等前缀相同的其他标签(meta CSP 只有注入
/// 到 <head> 内才生效)。返回字节偏移;找不到返回 None。
fn find_head_tag(lower: &str) -> Option<usize> {
    let bytes = lower.as_bytes();
    const NEEDLE: &[u8] = b"<head";
    let mut i = 0;
    while i + NEEDLE.len() <= bytes.len() {
        if &bytes[i..i + NEEDLE.len()] == NEEDLE {
            let after = bytes.get(i + NEEDLE.len()).copied();
            let boundary = match after {
                None => true,                                  // 文本结束(<head 后无字符)
                Some(c) if c.is_ascii_whitespace() => true,    // <head id=...>
                Some(c) if c == b'>' => true,                  // <head>
                Some(c) if c == b'/' => true,                  // <head/>
                _ => false,                                    // <header 等 → 跳过
            };
            if boundary {
                return Some(i);
            }
            i += NEEDLE.len();
        } else {
            i += 1;
        }
    }
    None
}

/// 定位 `<body` 标签起始下标(小写输入),规则同 find_head_tag。
fn find_body_tag(lower: &str) -> Option<usize> {
    lower
        .find("<body")
        .filter(|&i| {
            lower.as_bytes().get(i + 5).copied().map_or(true, |c| {
                c.is_ascii_whitespace() || c == b'>' || c == b'/'
            })
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
