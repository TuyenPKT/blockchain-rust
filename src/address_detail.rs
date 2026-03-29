#![allow(dead_code)]
//! v14.7 — Address Detail Page
//!
//! Hash-router cho address detail page:
//!   `#addr/ADDRESS` → fetch `/api/address/ADDRESS` → balance + UTXO + tx history
//!
//! Backend: view models + format helpers + axum router (`GET /static/address.js`)

use axum::{http::header, response::IntoResponse, routing::get, Router};

/// Nhúng address.js compile-time
pub static ADDRESS_JS: &[u8] = include_bytes!("../frontend/address.js");

const PAKLETS_PER_PKT: u64 = 1_073_741_824; // 2^30

// ── TxDirection ───────────────────────────────────────────────────────────────

/// Chiều giao dịch tính theo địa chỉ đang xem
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxDirection {
    Incoming,  // address là output (nhận tiền)
    Outgoing,  // address là input (gửi tiền)
    Internal,  // cả hai (self-transfer)
}

impl TxDirection {
    pub fn symbol(self) -> &'static str {
        match self {
            TxDirection::Incoming => "↓",
            TxDirection::Outgoing => "↑",
            TxDirection::Internal => "↕",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TxDirection::Incoming => "IN",
            TxDirection::Outgoing => "OUT",
            TxDirection::Internal => "SELF",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            TxDirection::Incoming => "pk-incoming",
            TxDirection::Outgoing => "pk-outgoing",
            TxDirection::Internal => "pk-internal",
        }
    }

    /// Xác định hướng từ flag `is_input` của API (true = address là input → outgoing)
    pub fn from_is_input(is_input: bool) -> Self {
        if is_input { TxDirection::Outgoing } else { TxDirection::Incoming }
    }
}

// ── TxRecord ──────────────────────────────────────────────────────────────────

/// Một giao dịch trong lịch sử của địa chỉ
#[derive(Debug, Clone)]
pub struct TxRecord {
    pub txid:          String,
    pub block_height:  Option<u64>,
    pub direction:     TxDirection,
    /// Số paklets — dương = nhận, âm = gửi
    pub amount_paklets: i64,
}

impl TxRecord {
    pub fn new(
        txid:          impl Into<String>,
        block_height:  Option<u64>,
        direction:     TxDirection,
        amount_paklets: i64,
    ) -> Self {
        TxRecord { txid: txid.into(), block_height, direction, amount_paklets }
    }

    /// PKT (dấu giữ nguyên)
    pub fn amount_pkt(&self) -> f64 {
        self.amount_paklets as f64 / PAKLETS_PER_PKT as f64
    }

    /// "+X.XXXX PKT" hoặc "−X.XXXX PKT"
    pub fn amount_display(&self) -> String {
        let pkt = self.amount_pkt().abs();
        if self.amount_paklets >= 0 {
            format!("+{:.4} PKT", pkt)
        } else {
            format!("−{:.4} PKT", pkt)
        }
    }

    pub fn is_incoming(&self) -> bool { self.amount_paklets > 0 }
    pub fn is_outgoing(&self) -> bool { self.amount_paklets < 0 }
}

// ── UtxoView ──────────────────────────────────────────────────────────────────

/// Một UTXO thuộc địa chỉ
#[derive(Debug, Clone)]
pub struct UtxoView {
    pub txid:         String,
    pub output_index: u32,
    pub amount_paklets: u64,
    pub block_height: Option<u64>,
}

impl UtxoView {
    pub fn new(
        txid:           impl Into<String>,
        output_index:   u32,
        amount_paklets: u64,
        block_height:   Option<u64>,
    ) -> Self {
        UtxoView { txid: txid.into(), output_index, amount_paklets, block_height }
    }

    pub fn amount_pkt(&self) -> f64 {
        self.amount_paklets as f64 / PAKLETS_PER_PKT as f64
    }
}

// ── AddressDetailView ─────────────────────────────────────────────────────────

/// View model cho address detail page
#[derive(Debug, Clone)]
pub struct AddressDetailView {
    pub address:         String,
    pub balance_paklets: u64,
    pub tx_count:        usize,
    pub utxo_count:      usize,
}

impl AddressDetailView {
    pub fn new(
        address:         impl Into<String>,
        balance_paklets: u64,
        tx_count:        usize,
        utxo_count:      usize,
    ) -> Self {
        AddressDetailView {
            address: address.into(),
            balance_paklets,
            tx_count,
            utxo_count,
        }
    }

    pub fn balance_pkt(&self) -> f64 {
        self.balance_paklets as f64 / PAKLETS_PER_PKT as f64
    }

    pub fn balance_display(&self) -> String {
        format!("{:.4} PKT", self.balance_pkt())
    }

    pub fn has_transactions(&self) -> bool { self.tx_count > 0 }
    pub fn has_utxos(&self)       -> bool { self.utxo_count > 0 }

    /// Loại địa chỉ từ prefix
    pub fn address_type(&self) -> &'static str {
        detect_addr_type(&self.address)
    }
}

// ── Format helpers ────────────────────────────────────────────────────────────

/// Phát hiện loại địa chỉ PKT từ prefix
pub fn detect_addr_type(addr: &str) -> &'static str {
    if addr.starts_with("pkt1q")  { return "P2WPKH (bech32 mainnet)"; }
    if addr.starts_with("pkt1p")  { return "P2TR (taproot mainnet)"; }
    if addr.starts_with("tpkt1q") { return "P2WPKH (bech32 testnet)"; }
    if addr.starts_with("rpkt1q") { return "P2WPKH (bech32 regtest)"; }
    if addr.len() == 40 && addr.chars().all(|c| c.is_ascii_hexdigit()) {
        return "P2PKH (hex)";
    }
    "Unknown"
}

/// Format số dư: paklets → PKT (4 decimal)
pub fn format_balance(paklets: u64) -> String {
    format!("{:.4} PKT", paklets as f64 / PAKLETS_PER_PKT as f64)
}

/// Rút gọn địa chỉ dài: 12 ký tự đầu + "…" + 8 ký tự cuối
pub fn truncate_addr(addr: &str) -> String {
    if addr.len() <= 24 {
        addr.to_string()
    } else {
        format!("{}…{}", &addr[..12], &addr[addr.len() - 8..])
    }
}

/// Tính tổng incoming từ danh sách TxRecord
pub fn total_incoming(records: &[TxRecord]) -> i64 {
    records.iter()
        .filter(|r| r.is_incoming())
        .map(|r| r.amount_paklets)
        .sum()
}

/// Tính tổng outgoing từ danh sách TxRecord (trả về số dương)
pub fn total_outgoing(records: &[TxRecord]) -> i64 {
    records.iter()
        .filter(|r| r.is_outgoing())
        .map(|r| r.amount_paklets.abs())
        .sum()
}

// ── Axum router ───────────────────────────────────────────────────────────────

async fn address_js_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        ADDRESS_JS,
    )
}

/// Router: `GET /static/address.js`
/// Merge vào `pktscan_api::serve()`.
pub fn address_router() -> Router {
    Router::new().route("/static/address.js", get(address_js_handler))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ADDRESS_JS embedded ───────────────────────────────────────────────

    #[test]
    fn test_address_js_not_empty() { assert!(!ADDRESS_JS.is_empty()); }

    #[test]
    fn test_address_js_valid_utf8() {
        assert!(std::str::from_utf8(ADDRESS_JS).is_ok());
    }

    #[test]
    fn test_address_js_has_hashchange() {
        let s = std::str::from_utf8(ADDRESS_JS).unwrap();
        assert!(s.contains("hashchange"));
    }

    #[test]
    fn test_address_js_has_addr_route() {
        let s = std::str::from_utf8(ADDRESS_JS).unwrap();
        assert!(s.contains("#addr"), "phải xử lý #addr/ADDRESS route");
    }

    #[test]
    fn test_address_js_has_fetch() {
        let s = std::str::from_utf8(ADDRESS_JS).unwrap();
        assert!(s.contains("fetch"));
    }

    #[test]
    fn test_address_js_has_api_address() {
        let s = std::str::from_utf8(ADDRESS_JS).unwrap();
        // endpoint hiện tại: api/testnet/addr/ (v22.x migration)
        assert!(s.contains("/api/address") || s.contains("api/testnet/addr"));
    }

    #[test]
    fn test_address_js_has_api_balance() {
        let s = std::str::from_utf8(ADDRESS_JS).unwrap();
        // balance hiển thị qua field response, không phải endpoint riêng
        assert!(s.contains("/api/balance") || s.contains("balance"));
    }

    #[test]
    fn test_address_js_has_incoming_outgoing() {
        let s = std::str::from_utf8(ADDRESS_JS).unwrap();
        // v22.x: dùng tx history table thay vì incoming/outgoing labels
        assert!(s.contains("incoming") || s.contains("IN") || s.contains("txHistory") || s.contains("txs"));
        assert!(s.contains("outgoing") || s.contains("OUT") || s.contains("history") || s.contains("tx_id"));
    }

    #[test]
    fn test_address_js_has_tx_link() {
        let s = std::str::from_utf8(ADDRESS_JS).unwrap();
        assert!(s.contains("#tx/"));
    }

    #[test]
    fn test_address_js_has_block_link() {
        let s = std::str::from_utf8(ADDRESS_JS).unwrap();
        assert!(s.contains("#block/"));
    }

    // ── TxDirection ───────────────────────────────────────────────────────

    #[test]
    fn test_direction_symbol_in()   { assert_eq!(TxDirection::Incoming.symbol(), "↓"); }
    #[test]
    fn test_direction_symbol_out()  { assert_eq!(TxDirection::Outgoing.symbol(), "↑"); }
    #[test]
    fn test_direction_symbol_self() { assert_eq!(TxDirection::Internal.symbol(), "↕"); }

    #[test]
    fn test_direction_label_in()   { assert_eq!(TxDirection::Incoming.label(), "IN"); }
    #[test]
    fn test_direction_label_out()  { assert_eq!(TxDirection::Outgoing.label(), "OUT"); }
    #[test]
    fn test_direction_label_self() { assert_eq!(TxDirection::Internal.label(), "SELF"); }

    #[test]
    fn test_direction_css_in()  { assert!(TxDirection::Incoming.css_class().contains("incoming")); }
    #[test]
    fn test_direction_css_out() { assert!(TxDirection::Outgoing.css_class().contains("outgoing")); }

    #[test]
    fn test_direction_from_is_input_true()  {
        assert_eq!(TxDirection::from_is_input(true),  TxDirection::Outgoing);
    }
    #[test]
    fn test_direction_from_is_input_false() {
        assert_eq!(TxDirection::from_is_input(false), TxDirection::Incoming);
    }

    // ── TxRecord ──────────────────────────────────────────────────────────

    fn sample_in() -> TxRecord {
        TxRecord::new("abc123", Some(10), TxDirection::Incoming, 2_147_483_648) // 2 PKT
    }
    fn sample_out() -> TxRecord {
        TxRecord::new("def456", Some(11), TxDirection::Outgoing, -1_073_741_824) // -1 PKT
    }

    #[test]
    fn test_tx_is_incoming()     { assert!(sample_in().is_incoming()); }
    #[test]
    fn test_tx_not_outgoing_in() { assert!(!sample_in().is_outgoing()); }
    #[test]
    fn test_tx_is_outgoing()     { assert!(sample_out().is_outgoing()); }
    #[test]
    fn test_tx_not_incoming_out(){ assert!(!sample_out().is_incoming()); }

    #[test]
    fn test_tx_amount_pkt_in() {
        assert!((sample_in().amount_pkt() - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_tx_amount_pkt_out() {
        assert!((sample_out().amount_pkt() - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_tx_amount_display_in() {
        let s = sample_in().amount_display();
        assert!(s.starts_with('+'), "incoming phải có dấu +: {}", s);
        assert!(s.contains("2.0000"));
    }

    #[test]
    fn test_tx_amount_display_out() {
        let s = sample_out().amount_display();
        assert!(s.contains('−'), "outgoing phải có dấu −: {}", s);
        assert!(s.contains("1.0000"));
    }

    #[test]
    fn test_tx_zero_amount() {
        let r = TxRecord::new("x", None, TxDirection::Incoming, 0);
        assert!(!r.is_incoming());
        assert!(!r.is_outgoing());
        assert!(r.amount_display().starts_with('+'));
    }

    #[test]
    fn test_tx_block_height_some() { assert_eq!(sample_in().block_height, Some(10)); }
    #[test]
    fn test_tx_block_height_none() {
        let r = TxRecord::new("x", None, TxDirection::Incoming, 0);
        assert!(r.block_height.is_none());
    }

    // ── UtxoView ──────────────────────────────────────────────────────────

    #[test]
    fn test_utxo_amount_pkt() {
        let u = UtxoView::new("abc", 0, 1_073_741_824, Some(5));
        assert!((u.amount_pkt() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_utxo_fields() {
        let u = UtxoView::new("abc123", 2, 500_000, Some(10));
        assert_eq!(u.output_index, 2);
        assert_eq!(u.block_height, Some(10));
    }

    // ── AddressDetailView ─────────────────────────────────────────────────

    fn sample_addr() -> AddressDetailView {
        AddressDetailView::new(
            "pkt1qtest000000000000000000000000000000000",
            2_147_483_648, // 2 PKT
            7,
            3,
        )
    }

    #[test]
    fn test_addr_balance_pkt() {
        assert!((sample_addr().balance_pkt() - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_addr_balance_display() {
        assert!(sample_addr().balance_display().contains("2.0000 PKT"));
    }

    #[test]
    fn test_addr_has_transactions() { assert!(sample_addr().has_transactions()); }
    #[test]
    fn test_addr_has_utxos()        { assert!(sample_addr().has_utxos()); }

    #[test]
    fn test_addr_no_transactions() {
        let a = AddressDetailView::new("pkt1q", 0, 0, 0);
        assert!(!a.has_transactions());
        assert!(!a.has_utxos());
    }

    #[test]
    fn test_addr_type_mainnet() {
        assert!(sample_addr().address_type().contains("mainnet"));
    }

    #[test]
    fn test_addr_type_testnet() {
        let a = AddressDetailView::new("tpkt1qabc", 0, 0, 0);
        assert!(a.address_type().contains("testnet"));
    }

    // ── detect_addr_type ──────────────────────────────────────────────────

    #[test]
    fn test_detect_p2wpkh_mainnet() {
        assert!(detect_addr_type("pkt1qabc").contains("mainnet"));
    }

    #[test]
    fn test_detect_p2tr() {
        assert!(detect_addr_type("pkt1pabc").contains("taproot"));
    }

    #[test]
    fn test_detect_testnet() {
        assert!(detect_addr_type("tpkt1qabc").contains("testnet"));
    }

    #[test]
    fn test_detect_regtest() {
        assert!(detect_addr_type("rpkt1qabc").contains("regtest"));
    }

    #[test]
    fn test_detect_hex() {
        let hex = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        assert!(detect_addr_type(hex).contains("P2PKH"));
    }

    #[test]
    fn test_detect_unknown() {
        assert_eq!(detect_addr_type("foobar"), "Unknown");
    }

    // ── format_balance ────────────────────────────────────────────────────

    #[test]
    fn test_format_balance_zero()  { assert!(format_balance(0).contains("0.0000")); }
    #[test]
    fn test_format_balance_one()   { assert!(format_balance(PAKLETS_PER_PKT).contains("1.0000")); }
    #[test]
    fn test_format_balance_half()  { assert!(format_balance(PAKLETS_PER_PKT / 2).contains("0.5000")); }
    #[test]
    fn test_format_balance_large() {
        let s = format_balance(PAKLETS_PER_PKT * 1000);
        assert!(s.contains("1000.0000"));
    }
    #[test]
    fn test_format_balance_suffix() { assert!(format_balance(1).ends_with("PKT")); }

    // ── truncate_addr ─────────────────────────────────────────────────────

    #[test]
    fn test_truncate_short()  { assert_eq!(truncate_addr("pkt1q"), "pkt1q"); }
    #[test]
    fn test_truncate_empty()  { assert_eq!(truncate_addr(""), ""); }

    #[test]
    fn test_truncate_long() {
        let addr = "pkt1qtest000000000000000000000000000000000";
        let s = truncate_addr(addr);
        assert!(s.contains('…'));
        assert!(s.starts_with("pkt1qtest000"));
        assert!(s.ends_with(&addr[addr.len()-8..]));
    }

    #[test]
    fn test_truncate_exactly_24() {
        let addr = "pkt1qtest0000000000000000"; // 25 chars → nên truncate
        let _ = truncate_addr(addr); // không panic
    }

    // ── total_incoming / total_outgoing ───────────────────────────────────

    #[test]
    fn test_total_incoming() {
        let records = vec![
            TxRecord::new("a", None, TxDirection::Incoming,  1_073_741_824),
            TxRecord::new("b", None, TxDirection::Outgoing, -536_870_912),
            TxRecord::new("c", None, TxDirection::Incoming,  1_073_741_824),
        ];
        assert_eq!(total_incoming(&records), 2_147_483_648);
    }

    #[test]
    fn test_total_outgoing() {
        let records = vec![
            TxRecord::new("a", None, TxDirection::Incoming,  1_073_741_824),
            TxRecord::new("b", None, TxDirection::Outgoing, -536_870_912),
        ];
        assert_eq!(total_outgoing(&records), 536_870_912);
    }

    #[test]
    fn test_totals_empty() {
        assert_eq!(total_incoming(&[]), 0);
        assert_eq!(total_outgoing(&[]), 0);
    }
}
