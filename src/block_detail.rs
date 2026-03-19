#![allow(dead_code)]
//! v14.6 — Block Detail Page
//!
//! Hash-router cho block/tx detail pages:
//!   `#block/N`  → fetch `/api/chain/N`  → render block detail
//!   `#tx/TXID`  → fetch `/api/tx/TXID`  → render TX detail
//!
//! Backend: view models + format helpers + axum router (`GET /static/detail.js`)

use axum::{http::header, response::IntoResponse, routing::get, Router};

/// Nhúng detail.js compile-time
pub static DETAIL_JS: &[u8] = include_bytes!("../frontend/detail.js");

// ── BlockDetailView ───────────────────────────────────────────────────────────

/// View model cho block detail page
#[derive(Debug, Clone)]
pub struct BlockDetailView {
    pub height:      u64,
    pub hash:        String,
    pub prev_hash:   String,
    pub merkle_root: String,
    pub timestamp:   i64,
    pub nonce:       u64,
    pub difficulty:  u64,
    pub tx_count:    usize,
}

impl BlockDetailView {
    pub fn new(
        height:      u64,
        hash:        impl Into<String>,
        prev_hash:   impl Into<String>,
        merkle_root: impl Into<String>,
        timestamp:   i64,
        nonce:       u64,
        difficulty:  u64,
        tx_count:    usize,
    ) -> Self {
        BlockDetailView {
            height,
            hash:        hash.into(),
            prev_hash:   prev_hash.into(),
            merkle_root: merkle_root.into(),
            timestamp,
            nonce,
            difficulty,
            tx_count,
        }
    }

    /// Hash rút gọn: 16 ký tự đầu + "…"
    pub fn hash_short(&self)        -> String { short_hash(&self.hash) }
    pub fn prev_hash_short(&self)   -> String { short_hash(&self.prev_hash) }
    pub fn merkle_root_short(&self) -> String { short_hash(&self.merkle_root) }

    /// Timestamp Unix → "YYYY-MM-DD HH:MM:SS UTC"
    pub fn timestamp_display(&self) -> String { format_timestamp(self.timestamp) }
}

// ── TxDetailView ──────────────────────────────────────────────────────────────

/// View model cho TX detail page
#[derive(Debug, Clone)]
pub struct TxDetailView {
    pub txid:          String,
    pub block_height:  Option<u64>,
    pub confirmations: u64,
    pub input_count:   usize,
    pub output_count:  usize,
    pub total_output:  u64,   // paklets
    pub is_coinbase:   bool,
}

impl TxDetailView {
    pub fn new(
        txid:          impl Into<String>,
        block_height:  Option<u64>,
        confirmations: u64,
        input_count:   usize,
        output_count:  usize,
        total_output:  u64,
        is_coinbase:   bool,
    ) -> Self {
        TxDetailView {
            txid: txid.into(),
            block_height,
            confirmations,
            input_count,
            output_count,
            total_output,
            is_coinbase,
        }
    }

    pub fn txid_short(&self) -> String { short_hash(&self.txid) }

    /// Tổng output tính bằng PKT (1 PKT = 2^30 paklets)
    pub fn total_output_pkt(&self) -> f64 {
        self.total_output as f64 / 1_073_741_824.0
    }

    /// "confirmed" hoặc "pending"
    pub fn status(&self) -> &'static str {
        if self.confirmations > 0 { "confirmed" } else { "pending" }
    }
}

// ── Format helpers ────────────────────────────────────────────────────────────

/// Rút gọn hash/address: 16 ký tự đầu + "…" (hoặc nguyên nếu ≤16)
pub fn short_hash(h: &str) -> String {
    if h.len() <= 16 {
        h.to_string()
    } else {
        format!("{}…", &h[..16])
    }
}

/// Timestamp Unix → "YYYY-MM-DD HH:MM:SS UTC" (không dùng chrono)
pub fn format_timestamp(ts: i64) -> String {
    if ts < 0 {
        return "N/A".to_string();
    }
    let secs  = ts as u64;
    let s_day = secs % 86400;
    let days  = secs / 86400;

    let (y, m, d) = days_to_ymd(days);
    let hh = s_day / 3600;
    let mm = (s_day % 3600) / 60;
    let ss = s_day % 60;

    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", y, m, d, hh, mm, ss)
}

/// Gregorian calendar: ngày kể từ 1970-01-01 → (year, month, day)
/// Thuật toán Howard Hinnant (public domain)
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z   = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y   = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let m   = if mp < 10 { mp + 3 } else { mp - 9 };
    let y   = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Format paklets → PKT string (4 chữ số thập phân)
pub fn format_paklets(paklets: u64) -> String {
    format!("{:.4} PKT", paklets as f64 / 1_073_741_824.0)
}

// ── Axum router ───────────────────────────────────────────────────────────────

async fn detail_js_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        DETAIL_JS,
    )
}

/// Router: `GET /static/detail.js`
/// Merge vào `pktscan_api::serve()`.
pub fn detail_router() -> Router {
    Router::new().route("/static/detail.js", get(detail_js_handler))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── DETAIL_JS embedded ────────────────────────────────────────────────

    #[test]
    fn test_detail_js_not_empty() {
        assert!(!DETAIL_JS.is_empty());
    }

    #[test]
    fn test_detail_js_valid_utf8() {
        assert!(std::str::from_utf8(DETAIL_JS).is_ok());
    }

    #[test]
    fn test_detail_js_has_hashchange() {
        let s = std::str::from_utf8(DETAIL_JS).unwrap();
        assert!(s.contains("hashchange"), "hash-router phải lắng nghe hashchange");
    }

    #[test]
    fn test_detail_js_has_block_route() {
        let s = std::str::from_utf8(DETAIL_JS).unwrap();
        assert!(s.contains("#block"), "phải xử lý #block/N route");
    }

    #[test]
    fn test_detail_js_has_tx_route() {
        let s = std::str::from_utf8(DETAIL_JS).unwrap();
        assert!(s.contains("#tx"), "phải xử lý #tx/ID route");
    }

    #[test]
    fn test_detail_js_has_fetch() {
        let s = std::str::from_utf8(DETAIL_JS).unwrap();
        assert!(s.contains("fetch"));
    }

    #[test]
    fn test_detail_js_has_api_chain() {
        let s = std::str::from_utf8(DETAIL_JS).unwrap();
        assert!(s.contains("/api/chain"));
    }

    #[test]
    fn test_detail_js_has_api_tx() {
        let s = std::str::from_utf8(DETAIL_JS).unwrap();
        assert!(s.contains("/api/tx"));
    }

    #[test]
    fn test_detail_js_has_back_button() {
        let s = std::str::from_utf8(DETAIL_JS).unwrap();
        assert!(s.contains("Back") || s.contains("back"));
    }

    #[test]
    fn test_detail_js_has_location_hash() {
        let s = std::str::from_utf8(DETAIL_JS).unwrap();
        assert!(s.contains("location.hash"));
    }

    // ── BlockDetailView ───────────────────────────────────────────────────

    fn sample_block() -> BlockDetailView {
        BlockDetailView::new(
            42,
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            1_700_000_000,
            999_999,
            4,
            5,
        )
    }

    #[test]
    fn test_block_height()   { assert_eq!(sample_block().height, 42); }
    #[test]
    fn test_block_tx_count() { assert_eq!(sample_block().tx_count, 5); }
    #[test]
    fn test_block_nonce()    { assert_eq!(sample_block().nonce, 999_999); }
    #[test]
    fn test_block_difficulty() { assert_eq!(sample_block().difficulty, 4); }

    #[test]
    fn test_block_hash_short_has_ellipsis() {
        assert!(sample_block().hash_short().contains('…'));
    }

    #[test]
    fn test_block_hash_short_prefix() {
        assert!(sample_block().hash_short().starts_with("abcdef12345678"));
    }

    #[test]
    fn test_block_prev_hash_short() {
        assert!(sample_block().prev_hash_short().starts_with("000000000000000"));
    }

    #[test]
    fn test_block_merkle_root_short() {
        assert!(sample_block().merkle_root_short().starts_with("deadbeefdeadbee"));
    }

    #[test]
    fn test_block_timestamp_display_utc() {
        assert!(sample_block().timestamp_display().contains("UTC"));
    }

    #[test]
    fn test_block_timestamp_display_2023() {
        // 1_700_000_000 ≈ 2023-11-14
        let s = sample_block().timestamp_display();
        assert!(s.contains("2023"), "expected 2023 in '{}'", s);
    }

    #[test]
    fn test_block_timestamp_display_nov() {
        let s = sample_block().timestamp_display();
        assert!(s.contains("2023-11"), "expected November: '{}'", s);
    }

    // ── TxDetailView ──────────────────────────────────────────────────────

    fn sample_tx() -> TxDetailView {
        TxDetailView::new(
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            Some(42),
            6,
            2,
            3,
            4_294_967_296, // 4 PKT
            false,
        )
    }

    #[test]
    fn test_tx_block_height()    { assert_eq!(sample_tx().block_height, Some(42)); }
    #[test]
    fn test_tx_confirmations()   { assert_eq!(sample_tx().confirmations, 6); }
    #[test]
    fn test_tx_input_count()     { assert_eq!(sample_tx().input_count, 2); }
    #[test]
    fn test_tx_output_count()    { assert_eq!(sample_tx().output_count, 3); }
    #[test]
    fn test_tx_not_coinbase()    { assert!(!sample_tx().is_coinbase); }

    #[test]
    fn test_tx_total_output_pkt() {
        let pkt = sample_tx().total_output_pkt();
        assert!((pkt - 4.0).abs() < 0.001, "expected ~4 PKT, got {}", pkt);
    }

    #[test]
    fn test_tx_status_confirmed() { assert_eq!(sample_tx().status(), "confirmed"); }

    #[test]
    fn test_tx_status_pending() {
        let tx = TxDetailView::new("abc", None, 0, 1, 1, 0, false);
        assert_eq!(tx.status(), "pending");
    }

    #[test]
    fn test_tx_txid_short_ellipsis() {
        assert!(sample_tx().txid_short().contains('…'));
    }

    #[test]
    fn test_tx_txid_short_prefix() {
        assert!(sample_tx().txid_short().starts_with("abcdef12345678"));
    }

    #[test]
    fn test_tx_coinbase() {
        let tx = TxDetailView::new("0000", Some(0), 100, 0, 1, 1_073_741_824, true);
        assert!(tx.is_coinbase);
        assert!((tx.total_output_pkt() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_tx_no_block() {
        let tx = TxDetailView::new("abc", None, 0, 1, 1, 0, false);
        assert!(tx.block_height.is_none());
    }

    // ── short_hash ────────────────────────────────────────────────────────

    #[test]
    fn test_short_hash_long() {
        let h = "abcdef1234567890abcdef1234567890";
        let s = short_hash(h);
        assert!(s.ends_with('…'));
        assert_eq!(&s[..16], "abcdef1234567890");
    }

    #[test]
    fn test_short_hash_short()      { assert_eq!(short_hash("abc"), "abc"); }
    #[test]
    fn test_short_hash_exact_16()   { let h = "abcdef1234567890"; assert_eq!(short_hash(h), h); }
    #[test]
    fn test_short_hash_empty()      { assert_eq!(short_hash(""), ""); }

    // ── format_timestamp ──────────────────────────────────────────────────

    #[test]
    fn test_timestamp_epoch() {
        assert!(format_timestamp(0).contains("1970-01-01"));
    }

    #[test]
    fn test_timestamp_utc_label() {
        assert!(format_timestamp(1_700_000_000).contains("UTC"));
    }

    #[test]
    fn test_timestamp_format_length() {
        // "YYYY-MM-DD HH:MM:SS UTC" = 23 chars
        let s = format_timestamp(1_700_000_000);
        assert!(s.len() >= 23, "too short: '{}'", s);
    }

    #[test]
    fn test_timestamp_negative() {
        assert_eq!(format_timestamp(-1), "N/A");
    }

    #[test]
    fn test_timestamp_known_date() {
        // 86400 = 1970-01-02
        assert!(format_timestamp(86400).contains("1970-01-02"));
    }

    #[test]
    fn test_timestamp_2024_new_year() {
        // 2024-01-01 00:00:00 UTC = 1704067200
        let s = format_timestamp(1_704_067_200);
        assert!(s.contains("2024-01-01"), "got: '{}'", s);
    }

    // ── format_paklets ────────────────────────────────────────────────────

    #[test]
    fn test_format_paklets_one_pkt() {
        let s = format_paklets(1_073_741_824);
        assert!(s.contains("1.0000") && s.contains("PKT"));
    }

    #[test]
    fn test_format_paklets_zero() {
        assert!(format_paklets(0).contains("0.0000"));
    }

    #[test]
    fn test_format_paklets_half() {
        assert!(format_paklets(536_870_912).contains("0.5000"));
    }

    #[test]
    fn test_format_paklets_has_suffix() {
        assert!(format_paklets(1_000_000).contains("PKT"));
    }
}
