#![allow(dead_code)]
//! web_serve — Runtime file serving từ thư mục `web/`
//!
//! Dùng `tower_http::services::ServeDir` để serve toàn bộ `web/` tại runtime.
//! Không cần `cargo build` khi sửa CSS/JS/PNG — chỉ cần refresh browser.
//!
//! Routes:
//!   GET /web/**          → web/  (CSS, JS, Icons, HTML)
//!   GET /address/:addr   → web/address/index.html
//!   GET /block/          → web/block/index.html
//!   GET /block/:height   → web/block/detail.html
//!   GET /rx/             → web/rx/index.html
//!   GET /rx/:txid        → web/rx/detail.html
//!
//! Nginx rewrite: /blockchain-rust/web/** → /web/** trước khi proxy vào port 8080.

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::collections::HashMap;
use tower_http::services::ServeDir;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Đường dẫn đến thư mục web/ tính từ working directory của process.
/// Trên VPS, binary chạy từ ~/blockchain-rust/ nên web/ nằm đúng vị trí.
fn web_dir() -> &'static str {
    "web"
}

/// Serve một file HTML cụ thể từ web/, trả 404 nếu không tìm thấy.
async fn serve_html(path: &str) -> impl IntoResponse {
    let full = format!("{}/{}", web_dir(), path);
    match tokio::fs::read(&full).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            bytes,
        ).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            b"404 Not Found".to_vec(),
        ).into_response(),
    }
}

// ── Page handlers ─────────────────────────────────────────────────────────────

async fn serve_address_page() -> impl IntoResponse {
    serve_html("address/index.html").await
}

async fn serve_block_list() -> impl IntoResponse {
    serve_html("block/index.html").await
}

async fn serve_block_detail() -> impl IntoResponse {
    serve_html("block/detail.html").await
}

async fn serve_rx_list() -> impl IntoResponse {
    serve_html("rx/index.html").await
}

async fn serve_rx_detail() -> impl IntoResponse {
    serve_html("rx/detail.html").await
}

async fn serve_playground() -> impl IntoResponse {
    serve_html("playground/index.html").await
}

async fn serve_webhooks_page(
    State(auth): State<crate::api_auth::AuthDb>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    // Check X-Api-Key header or ?api_key= query param
    let key = headers.get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| params.get("api_key").cloned());

    let authorized = match &key {
        None    => false,
        Some(k) => auth.lock().await.validate(k).is_some(),
    };

    if authorized {
        serve_html("webhooks/index.html").await.into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            r#"<!DOCTYPE html><html><head><title>Webhooks — Auth Required</title>
<meta name="robots" content="noindex,nofollow">
<style>body{font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#0d1117;color:#c9d1d9}
.box{background:#161b22;border:1px solid #30363d;border-radius:8px;padding:32px;max-width:400px;width:100%;text-align:center}
input{width:100%;padding:8px 12px;margin:12px 0;background:#21262d;border:1px solid #30363d;border-radius:6px;color:#c9d1d9;box-sizing:border-box}
button{width:100%;padding:10px;background:#238636;border:none;border-radius:6px;color:#fff;cursor:pointer}
</style></head><body>
<div class="box"><h2>🔐 Webhooks</h2><p>API key required</p>
<form onsubmit="location.href='/webhooks?api_key='+document.getElementById('k').value;return false">
<input id="k" type="password" placeholder="Paste API key…" autofocus>
<button type="submit">Access</button>
</form></div></body></html>"#,
        ).into_response()
    }
}

async fn serve_dev_page() -> impl IntoResponse {
    serve_html("dev/index.html").await
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Mount vào pktscan_api::serve() qua .merge(web_serve::web_router(auth_db))
pub fn web_router(auth: crate::api_auth::AuthDb) -> Router {
    Router::new()
        // Static assets: CSS, JS, Icons — ServeDir tự detect MIME type
        .nest_service("/web", ServeDir::new(web_dir()))
        // Page routes
        .route("/address/:addr",  get(serve_address_page))
        .route("/block",          get(serve_block_list))
        .route("/block/:height",  get(serve_block_detail))
        .route("/rx",             get(serve_rx_list))
        .route("/rx/:txid",       get(serve_rx_detail))
        .route("/playground",     get(serve_playground))
        .route("/webhooks",       get(serve_webhooks_page))
        .route("/dev",            get(serve_dev_page))
        .with_state(auth)
}
