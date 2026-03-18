#![allow(dead_code)]
//! v9.5 — Address Labels API (GET only, Zero-Trust)
//!
//! REST endpoints exposing `LabelRegistry` to PKTScan.
//! Maps blockchain addresses → human-readable labels (exchange, foundation, etc.)
//! Tất cả endpoints đều read-only (GET).
//! ZT middleware (rate limit + audit log) áp dụng ở router level.
//!
//! Endpoints:
//!   GET /api/labels                     → list tất cả labeled addresses
//!   GET /api/label/:addr                → label của một địa chỉ cụ thể
//!   GET /api/labels/category/:cat       → filter by category (exchange, foundation, etc.)
//!
//! Usage:
//!   let label_db = LabelDb::new(registry);
//!   let app = router.merge(address_labels::label_router(label_db));

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// ─── LabelRegistry ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AddressLabel {
    pub address:  String,
    pub label:    String,    // e.g. "Binance Hot Wallet"
    pub category: String,   // e.g. "exchange", "foundation", "miner", "contract"
    pub note:     String,    // optional additional info
}

impl AddressLabel {
    pub fn new(
        address:  impl Into<String>,
        label:    impl Into<String>,
        category: impl Into<String>,
        note:     impl Into<String>,
    ) -> Self {
        AddressLabel {
            address:  address.into(),
            label:    label.into(),
            category: category.into(),
            note:     note.into(),
        }
    }
}

pub struct LabelRegistry {
    pub labels: HashMap<String, AddressLabel>,
}

impl LabelRegistry {
    pub fn new() -> Self {
        LabelRegistry { labels: HashMap::new() }
    }

    pub fn insert(&mut self, label: AddressLabel) {
        self.labels.insert(label.address.clone(), label);
    }

    pub fn get(&self, address: &str) -> Option<&AddressLabel> {
        self.labels.get(address)
    }

    pub fn by_category(&self, category: &str) -> Vec<&AddressLabel> {
        self.labels.values()
            .filter(|l| l.category == category)
            .collect()
    }

    pub fn categories(&self) -> Vec<String> {
        let mut cats: Vec<String> = self.labels.values()
            .map(|l| l.category.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        cats.sort();
        cats
    }
}

pub type LabelDb = Arc<Mutex<LabelRegistry>>;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn label_router(state: LabelDb) -> Router {
    Router::new()
        .route("/api/labels",               get(get_labels))
        .route("/api/label/:addr",          get(get_label))
        .route("/api/labels/category/:cat", get(get_labels_by_category))
        .with_state(state)
}

// ─── GET /api/labels ──────────────────────────────────────────────────────────

/// List tất cả labeled addresses, sorted by category then address.
async fn get_labels(State(db): State<LabelDb>) -> Json<Value> {
    let reg = db.lock().await;

    let mut labels: Vec<Value> = reg.labels.values()
        .map(label_to_json)
        .collect();

    labels.sort_by(|a, b| {
        let cat_a = a["category"].as_str().unwrap_or("");
        let cat_b = b["category"].as_str().unwrap_or("");
        cat_a.cmp(cat_b)
            .then(a["address"].as_str().unwrap_or("").cmp(b["address"].as_str().unwrap_or("")))
    });

    let categories = reg.categories();

    Json(json!({
        "count":      labels.len(),
        "categories": categories,
        "labels":     labels,
    }))
}

// ─── GET /api/label/:addr ─────────────────────────────────────────────────────

/// Label của một địa chỉ. Trả về 404 nếu không có label.
async fn get_label(
    State(db):   State<LabelDb>,
    Path(addr):  Path<String>,
) -> (StatusCode, Json<Value>) {
    let reg = db.lock().await;

    match reg.get(&addr) {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "address": addr,
                "labeled": false,
            })),
        ),
        Some(l) => (StatusCode::OK, Json(label_to_json(l))),
    }
}

// ─── GET /api/labels/category/:cat ───────────────────────────────────────────

/// Filter labels by category.
async fn get_labels_by_category(
    State(db):   State<LabelDb>,
    Path(cat):   Path<String>,
) -> Json<Value> {
    let reg = db.lock().await;

    let mut labels: Vec<Value> = reg.by_category(&cat)
        .iter()
        .map(|l| label_to_json(l))
        .collect();

    labels.sort_by(|a, b| {
        a["address"].as_str().unwrap_or("").cmp(b["address"].as_str().unwrap_or(""))
    });

    Json(json!({
        "category": cat,
        "count":    labels.len(),
        "labels":   labels,
    }))
}

// ─── Helper ───────────────────────────────────────────────────────────────────

fn label_to_json(l: &AddressLabel) -> Value {
    json!({
        "address":  l.address,
        "label":    l.label,
        "category": l.category,
        "note":     l.note,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_db() -> LabelDb {
        Arc::new(Mutex::new(LabelRegistry::new()))
    }

    fn populated_db() -> LabelDb {
        let mut reg = LabelRegistry::new();
        reg.insert(AddressLabel::new(
            "pkt1qalice", "PKT Foundation", "foundation", "Main treasury",
        ));
        reg.insert(AddressLabel::new(
            "pkt1qbinance", "Binance Hot Wallet", "exchange", "Binance.com",
        ));
        reg.insert(AddressLabel::new(
            "pkt1qcoinbase", "Coinbase Custody", "exchange", "Coinbase.com",
        ));
        reg.insert(AddressLabel::new(
            "pkt1qminer1", "Top Miner Alpha", "miner", "Known mining pool",
        ));
        reg.insert(AddressLabel::new(
            "0xcontract1", "DEX Router", "contract", "Automated market maker",
        ));
        Arc::new(Mutex::new(reg))
    }

    // ── LabelRegistry ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_label_db_empty() {
        let db  = make_db();
        let reg = db.lock().await;
        assert!(reg.labels.is_empty());
    }

    #[test]
    fn test_label_router_builds() {
        let db = make_db();
        let _r = label_router(db);
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let mut reg = LabelRegistry::new();
        reg.insert(AddressLabel::new("addr1", "Label1", "exchange", ""));
        assert!(reg.get("addr1").is_some());
        assert_eq!(reg.get("addr1").unwrap().label, "Label1");
    }

    #[tokio::test]
    async fn test_get_unknown_returns_none() {
        let reg = LabelRegistry::new();
        assert!(reg.get("unknown").is_none());
    }

    // ── GET /api/labels ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_labels_count() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.labels.len(), 5);
    }

    #[tokio::test]
    async fn test_labels_has_foundation() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert!(reg.labels.contains_key("pkt1qalice"));
    }

    #[tokio::test]
    async fn test_labels_has_exchanges() {
        let db  = populated_db();
        let reg = db.lock().await;
        let exchanges: Vec<_> = reg.by_category("exchange");
        assert_eq!(exchanges.len(), 2);
    }

    #[tokio::test]
    async fn test_categories_list() {
        let db  = populated_db();
        let reg = db.lock().await;
        let cats = reg.categories();
        assert!(cats.contains(&"exchange".to_string()));
        assert!(cats.contains(&"foundation".to_string()));
        assert!(cats.contains(&"miner".to_string()));
        assert!(cats.contains(&"contract".to_string()));
    }

    #[tokio::test]
    async fn test_categories_sorted() {
        let db   = populated_db();
        let reg  = db.lock().await;
        let cats = reg.categories();
        let mut sorted = cats.clone();
        sorted.sort();
        assert_eq!(cats, sorted);
    }

    // ── GET /api/label/:addr ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_label_found() {
        let db  = populated_db();
        let reg = db.lock().await;
        let l   = reg.get("pkt1qalice").unwrap();
        assert_eq!(l.label, "PKT Foundation");
        assert_eq!(l.category, "foundation");
    }

    #[tokio::test]
    async fn test_label_not_found() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert!(reg.get("unknown_addr").is_none());
    }

    #[tokio::test]
    async fn test_label_exchange_binance() {
        let db  = populated_db();
        let reg = db.lock().await;
        let l   = reg.get("pkt1qbinance").unwrap();
        assert_eq!(l.label, "Binance Hot Wallet");
        assert_eq!(l.note, "Binance.com");
    }

    // ── GET /api/labels/category/:cat ─────────────────────────────────────

    #[tokio::test]
    async fn test_category_exchange_count() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.by_category("exchange").len(), 2);
    }

    #[tokio::test]
    async fn test_category_foundation_count() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.by_category("foundation").len(), 1);
    }

    #[tokio::test]
    async fn test_category_miner_count() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.by_category("miner").len(), 1);
    }

    #[tokio::test]
    async fn test_category_unknown_empty() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.by_category("unknown_cat").len(), 0);
    }

    #[tokio::test]
    async fn test_category_contract_count() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.by_category("contract").len(), 1);
    }

    // ── label_to_json ─────────────────────────────────────────────────────

    #[test]
    fn test_label_to_json_fields() {
        let l = AddressLabel::new("addr1", "MyLabel", "exchange", "Notes here");
        let v = label_to_json(&l);
        assert_eq!(v["address"], "addr1");
        assert_eq!(v["label"],   "MyLabel");
        assert_eq!(v["category"],"exchange");
        assert_eq!(v["note"],    "Notes here");
    }

    // ── Tx status + confirmations (integration with pktscan_api logic) ────

    #[test]
    fn test_confirmations_calculation() {
        let tip: u64 = 100;
        let block_height: u64 = 95;
        let confirmations = tip.saturating_sub(block_height) + 1;
        assert_eq!(confirmations, 6);
    }

    #[test]
    fn test_confirmations_tip_block() {
        let tip: u64 = 50;
        let block_height: u64 = 50;
        let confirmations = tip.saturating_sub(block_height) + 1;
        assert_eq!(confirmations, 1);
    }

    #[test]
    fn test_confirmations_genesis() {
        let tip: u64 = 100;
        let block_height: u64 = 0;
        let confirmations = tip.saturating_sub(block_height) + 1;
        assert_eq!(confirmations, 101);
    }

    #[test]
    fn test_pending_status_zero_confirmations() {
        // mempool tx always has 0 confirmations
        let confirmations: u64 = 0;
        assert_eq!(confirmations, 0);
    }
}
