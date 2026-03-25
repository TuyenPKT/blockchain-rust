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
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
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

// ── Router ────────────────────────────────────────────────────────────────────

/// Mount vào pktscan_api::serve() qua .merge(web_serve::web_router())
pub fn web_router() -> Router {
    Router::new()
        // Static assets: CSS, JS, Icons — ServeDir tự detect MIME type
        .nest_service("/web", ServeDir::new(web_dir()))
        // Page routes
        .route("/address/:addr",  get(serve_address_page))
        .route("/block",          get(serve_block_list))
        .route("/block/:height",  get(serve_block_detail))
        .route("/rx",             get(serve_rx_list))
        .route("/rx/:txid",       get(serve_rx_detail))
}
