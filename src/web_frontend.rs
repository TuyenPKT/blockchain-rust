#![allow(dead_code)]
//! v14.2 — Web Frontend (PKTScan embedded)
//!
//! Nhúng toàn bộ frontend assets vào binary lúc compile-time dùng
//! `include_bytes!`. Không cần crate thêm, không cần static file server.
//!
//! Assets được nhúng:
//!   - `index.html`            → GET /
//!   - `frontend/app.js`       → GET /static/app.js
//!   - `frontend/style.css`    → GET /static/style.css
//!
//! Router tích hợp vào axum hiện có qua `frontend_router()`.

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

// ── Embedded assets ─────────────────────────────────────────────────────────

/// Bytes nhúng compile-time — lỗi nếu file không tồn tại
static INDEX_HTML: &[u8] = include_bytes!("../index.html");
static APP_JS:     &[u8] = include_bytes!("../frontend/app.js");
static STYLE_CSS:  &[u8] = include_bytes!("../frontend/style.css");

// ── Asset registry ───────────────────────────────────────────────────────────

/// Metadata của một embedded asset
#[derive(Debug, Clone)]
pub struct EmbeddedAsset {
    pub path:         &'static str,
    pub content_type: &'static str,
    pub bytes:        &'static [u8],
}

impl EmbeddedAsset {
    pub fn size(&self) -> usize { self.bytes.len() }
    pub fn is_empty(&self) -> bool { self.bytes.is_empty() }
}

/// Toàn bộ danh sách assets nhúng
pub const ASSETS: &[EmbeddedAsset] = &[
    EmbeddedAsset { path: "/",                 content_type: "text/html; charset=utf-8",       bytes: INDEX_HTML },
    EmbeddedAsset { path: "/static/app.js",    content_type: "application/javascript; charset=utf-8", bytes: APP_JS },
    EmbeddedAsset { path: "/static/style.css", content_type: "text/css; charset=utf-8",        bytes: STYLE_CSS },
];

/// Tìm asset theo path (trả về None nếu không tìm thấy)
pub fn find_asset(path: &str) -> Option<&'static EmbeddedAsset> {
    ASSETS.iter().find(|a| a.path == path)
}

/// Tổng kích thước tất cả assets (bytes)
pub fn total_size() -> usize {
    ASSETS.iter().map(|a| a.size()).sum()
}

// ── MIME type detection ───────────────────────────────────────────────────────

/// Lấy MIME type từ file extension
pub fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "html" | "htm" => "text/html; charset=utf-8",
        "js"           => "application/javascript; charset=utf-8",
        "css"          => "text/css; charset=utf-8",
        "json"         => "application/json",
        "png"          => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg"          => "image/svg+xml",
        "ico"          => "image/x-icon",
        "woff2"        => "font/woff2",
        "woff"         => "font/woff",
        "txt"          => "text/plain; charset=utf-8",
        _              => "application/octet-stream",
    }
}

/// Lấy extension từ filename
pub fn file_ext(filename: &str) -> &str {
    filename.rsplit('.').next().unwrap_or("")
}

// ── Axum handlers ────────────────────────────────────────────────────────────

async fn serve_index() -> impl IntoResponse {
    asset_response(INDEX_HTML, "text/html; charset=utf-8")
}

async fn serve_app_js() -> impl IntoResponse {
    asset_response(APP_JS, "application/javascript; charset=utf-8")
}

async fn serve_style_css() -> impl IntoResponse {
    asset_response(STYLE_CSS, "text/css; charset=utf-8")
}

async fn serve_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/plain")],
        "404 Not Found",
    )
}

fn asset_response(bytes: &'static [u8], content_type: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE,  content_type),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        bytes,
    ).into_response()
}

// ── Router ───────────────────────────────────────────────────────────────────

/// Router đầy đủ: / + /index.html + /static/* + fallback 404
/// Dùng khi chạy frontend standalone (không có pktscan_api).
pub fn frontend_router() -> Router {
    Router::new()
        .route("/",                 get(serve_index))
        .route("/index.html",       get(serve_index))
        .route("/static/app.js",    get(serve_app_js))
        .route("/static/style.css", get(serve_style_css))
        .fallback(serve_not_found)
}

/// Router chỉ có static assets (/static/app.js, /static/style.css).
/// Merge vào pktscan_api::serve() — không conflict với route "/" đã có sẵn.
pub fn static_router() -> Router {
    Router::new()
        .route("/static/app.js",    get(serve_app_js))
        .route("/static/style.css", get(serve_style_css))
}

/// Handler trả về embedded index.html (compile-time bytes).
/// pktscan_api::serve_index có thể gọi hàm này thay vì đọc filesystem.
pub async fn embedded_index_handler() -> impl IntoResponse {
    asset_response(INDEX_HTML, "text/html; charset=utf-8")
}

// ── Frontend manifest ─────────────────────────────────────────────────────────

/// Thông tin tổng hợp về frontend build
#[derive(Debug)]
pub struct FrontendManifest {
    pub asset_count: usize,
    pub total_bytes: usize,
    pub routes:      Vec<&'static str>,
}

impl FrontendManifest {
    pub fn build() -> Self {
        FrontendManifest {
            asset_count: ASSETS.len(),
            total_bytes: total_size(),
            routes:      ASSETS.iter().map(|a| a.path).collect(),
        }
    }

    pub fn has_route(&self, path: &str) -> bool {
        self.routes.contains(&path)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Embedded assets ───────────────────────────────────────────────────

    #[test]
    fn test_index_html_not_empty() {
        assert!(!INDEX_HTML.is_empty());
    }

    #[test]
    fn test_app_js_not_empty() {
        assert!(!APP_JS.is_empty());
    }

    #[test]
    fn test_style_css_not_empty() {
        assert!(!STYLE_CSS.is_empty());
    }

    #[test]
    fn test_index_html_contains_pktscan() {
        let html = std::str::from_utf8(INDEX_HTML).unwrap();
        assert!(html.contains("PKTScan") || html.contains("pkt"), "index.html phải chứa PKTScan hoặc pkt");
    }

    #[test]
    fn test_app_js_contains_fetch() {
        let js = std::str::from_utf8(APP_JS).unwrap();
        assert!(js.contains("fetch"), "app.js phải có fetch API call");
    }

    #[test]
    fn test_style_css_contains_root() {
        let css = std::str::from_utf8(STYLE_CSS).unwrap();
        assert!(css.contains(".container") || css.contains(":root") || css.contains(".stat-card"),
            "style.css phải có CSS rules");
    }

    // ── Asset registry ────────────────────────────────────────────────────

    #[test]
    fn test_asset_count() {
        assert_eq!(ASSETS.len(), 3);
    }

    #[test]
    fn test_find_asset_root() {
        let a = find_asset("/").unwrap();
        assert_eq!(a.path, "/");
        assert!(a.content_type.contains("html"));
    }

    #[test]
    fn test_find_asset_js() {
        let a = find_asset("/static/app.js").unwrap();
        assert!(a.content_type.contains("javascript"));
    }

    #[test]
    fn test_find_asset_css() {
        let a = find_asset("/static/style.css").unwrap();
        assert!(a.content_type.contains("css"));
    }

    #[test]
    fn test_find_asset_unknown_returns_none() {
        assert!(find_asset("/nonexistent.xyz").is_none());
    }

    #[test]
    fn test_asset_size_nonzero() {
        for a in ASSETS {
            assert!(a.size() > 0, "asset '{}' phải có size > 0", a.path);
        }
    }

    #[test]
    fn test_asset_is_not_empty() {
        for a in ASSETS {
            assert!(!a.is_empty(), "asset '{}' không được empty", a.path);
        }
    }

    #[test]
    fn test_total_size_positive() {
        assert!(total_size() > 0);
    }

    #[test]
    fn test_total_size_sum_of_parts() {
        let expected: usize = ASSETS.iter().map(|a| a.size()).sum();
        assert_eq!(total_size(), expected);
    }

    // ── MIME type detection ───────────────────────────────────────────────

    #[test]
    fn test_mime_html() {
        assert!(mime_for_ext("html").contains("html"));
    }

    #[test]
    fn test_mime_js() {
        assert!(mime_for_ext("js").contains("javascript"));
    }

    #[test]
    fn test_mime_css() {
        assert!(mime_for_ext("css").contains("css"));
    }

    #[test]
    fn test_mime_json() {
        assert!(mime_for_ext("json").contains("json"));
    }

    #[test]
    fn test_mime_png() {
        assert!(mime_for_ext("png").contains("png"));
    }

    #[test]
    fn test_mime_svg() {
        assert!(mime_for_ext("svg").contains("svg"));
    }

    #[test]
    fn test_mime_woff2() {
        assert!(mime_for_ext("woff2").contains("woff2"));
    }

    #[test]
    fn test_mime_unknown_octet_stream() {
        assert!(mime_for_ext("xyz").contains("octet-stream"));
    }

    #[test]
    fn test_mime_htm_same_as_html() {
        assert_eq!(mime_for_ext("htm"), mime_for_ext("html"));
    }

    // ── file_ext ──────────────────────────────────────────────────────────

    #[test]
    fn test_file_ext_html() {
        assert_eq!(file_ext("index.html"), "html");
    }

    #[test]
    fn test_file_ext_js() {
        assert_eq!(file_ext("app.js"), "js");
    }

    #[test]
    fn test_file_ext_no_ext() {
        // rsplit('.') khi không có '.' → returns whole string
        assert_eq!(file_ext("Makefile"), "Makefile");
    }

    #[test]
    fn test_file_ext_double_ext() {
        assert_eq!(file_ext("archive.tar.gz"), "gz");
    }

    // ── FrontendManifest ──────────────────────────────────────────────────

    #[test]
    fn test_manifest_asset_count() {
        let m = FrontendManifest::build();
        assert_eq!(m.asset_count, ASSETS.len());
    }

    #[test]
    fn test_manifest_has_root_route() {
        let m = FrontendManifest::build();
        assert!(m.has_route("/"));
    }

    #[test]
    fn test_manifest_has_js_route() {
        let m = FrontendManifest::build();
        assert!(m.has_route("/static/app.js"));
    }

    #[test]
    fn test_manifest_has_css_route() {
        let m = FrontendManifest::build();
        assert!(m.has_route("/static/style.css"));
    }

    #[test]
    fn test_manifest_unknown_route_false() {
        let m = FrontendManifest::build();
        assert!(!m.has_route("/nonexistent"));
    }

    #[test]
    fn test_manifest_total_bytes_positive() {
        let m = FrontendManifest::build();
        assert!(m.total_bytes > 0);
    }

    #[test]
    fn test_manifest_routes_count() {
        let m = FrontendManifest::build();
        assert_eq!(m.routes.len(), ASSETS.len());
    }

    // ── Router build (không cần server thật) ─────────────────────────────

    #[test]
    fn test_frontend_router_builds_without_panic() {
        let _r = frontend_router();
    }
}
