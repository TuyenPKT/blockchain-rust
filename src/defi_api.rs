#![allow(dead_code)]
//! v9.4 — DeFi API (GET only, Zero-Trust)
//!
//! REST endpoints exposing `OracleRegistry` và `LendingProtocol` to PKTScan.
//! Tất cả endpoints đều read-only (GET).
//! ZT middleware (rate limit + audit log) áp dụng ở router level.
//!
//! Endpoints:
//!   GET /api/defi/feeds                         → list tất cả oracle price feeds
//!   GET /api/defi/feed/:id                      → feed detail + latest round + history
//!   GET /api/defi/feed/:id/history              → lịch sử price rounds
//!   GET /api/defi/loans                         → list tất cả loans + liquidation status
//!   GET /api/defi/loans/liquidatable            → loans có thể bị liquidate ngay
//!
//! Usage:
//!   let defi_db = DefiDb::new(registry, protocol);
//!   let app = router.merge(defi_api::defi_router(defi_db));

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::oracle::{LendingProtocol, OracleRegistry};

/// Shared state gồm oracle registry + lending protocol.
pub struct DefiState {
    pub oracle:   OracleRegistry,
    pub lending:  LendingProtocol,
}

pub type DefiDb = Arc<Mutex<DefiState>>;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn defi_router(state: DefiDb) -> Router {
    Router::new()
        .route("/api/defi/feeds",                    get(get_feeds))
        .route("/api/defi/feed/:id",                 get(get_feed))
        .route("/api/defi/feed/:id/history",         get(get_feed_history))
        .route("/api/defi/loans",                    get(get_loans))
        .route("/api/defi/loans/liquidatable",       get(get_liquidatable))
        .with_state(state)
}

// ─── GET /api/defi/feeds ──────────────────────────────────────────────────────

/// List tất cả oracle feeds + latest price.
async fn get_feeds(State(db): State<DefiDb>) -> Json<Value> {
    let state = db.lock().await;

    let feeds: Vec<Value> = state.oracle.feeds.values()
        .map(|f| {
            let latest = f.latest_answer();
            json!({
                "feed_id":         f.feed_id,
                "description":     f.description,
                "decimals":        f.decimals,
                "current_round":   f.current_round,
                "round_count":     f.history.len(),
                "latest_price":    latest.map(|r| r.answer_f64),
                "latest_timestamp":latest.map(|r| r.timestamp),
                "authorized_nodes":f.authorized_nodes.len(),
            })
        })
        .collect();

    Json(json!({
        "count": feeds.len(),
        "feeds": feeds,
    }))
}

// ─── GET /api/defi/feed/:id ───────────────────────────────────────────────────

/// Feed detail: metadata + latest round data.
async fn get_feed(
    State(db): State<DefiDb>,
    Path(id):  Path<String>,
) -> (StatusCode, Json<Value>) {
    let state = db.lock().await;

    match state.oracle.feeds.get(&id) {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("feed '{}' not found", id) })),
        ),
        Some(f) => {
            let latest = f.latest_answer().map(|r| json!({
                "round":         r.round,
                "answer":        r.answer,
                "answer_f64":    r.answer_f64,
                "timestamp":     r.timestamp,
                "report_count":  r.report_count,
                "deviation_pct": format!("{:.4}", r.deviation_pct),
                "twap_answer":   r.twap_answer,
            }));

            (StatusCode::OK, Json(json!({
                "feed_id":             f.feed_id,
                "description":         f.description,
                "decimals":            f.decimals,
                "min_submissions":     f.min_submissions,
                "deviation_threshold": f.deviation_threshold,
                "heartbeat":           f.heartbeat,
                "current_round":       f.current_round,
                "round_count":         f.history.len(),
                "pending_reports":     f.round_reports.len(),
                "authorized_nodes":    f.authorized_nodes,
                "latest":              latest,
            })))
        }
    }
}

// ─── GET /api/defi/feed/:id/history ──────────────────────────────────────────

/// Lịch sử price rounds của một feed.
async fn get_feed_history(
    State(db): State<DefiDb>,
    Path(id):  Path<String>,
) -> (StatusCode, Json<Value>) {
    let state = db.lock().await;

    match state.oracle.feeds.get(&id) {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("feed '{}' not found", id) })),
        ),
        Some(f) => {
            let rounds: Vec<Value> = f.history.iter().rev()
                .map(|r| json!({
                    "round":         r.round,
                    "answer":        r.answer,
                    "answer_f64":    r.answer_f64,
                    "timestamp":     r.timestamp,
                    "report_count":  r.report_count,
                    "deviation_pct": format!("{:.4}", r.deviation_pct),
                    "twap_answer":   r.twap_answer,
                }))
                .collect();

            (StatusCode::OK, Json(json!({
                "feed_id":    id,
                "count":      rounds.len(),
                "rounds":     rounds,
            })))
        }
    }
}

// ─── GET /api/defi/loans ──────────────────────────────────────────────────────

/// List tất cả loans với collateral ratio hiện tại.
async fn get_loans(State(db): State<DefiDb>) -> Json<Value> {
    let state = db.lock().await;

    let loans: Vec<Value> = state.lending.loans.iter()
        .map(|l| json!({
            "borrower":         l.borrower,
            "collateral_btc":   l.collateral_btc,
            "borrowed_usd":     l.borrowed_usd,
            "collateral_ratio": format!("{:.4}", l.collateral_ratio),
            "liquidatable":     l.liquidatable,
        }))
        .collect();

    let liquidatable_count = state.lending.loans.iter()
        .filter(|l| l.liquidatable)
        .count();

    Json(json!({
        "count":              loans.len(),
        "liquidatable_count": liquidatable_count,
        "min_collateral":     state.lending.min_collateral,
        "oracle":             state.lending.oracle,
        "loans":              loans,
    }))
}

// ─── GET /api/defi/loans/liquidatable ────────────────────────────────────────

/// Chỉ các loans đang dưới collateral threshold.
async fn get_liquidatable(State(db): State<DefiDb>) -> Json<Value> {
    let state = db.lock().await;

    let loans: Vec<Value> = state.lending.liquidatable_loans().iter()
        .map(|l| json!({
            "borrower":         l.borrower,
            "collateral_btc":   l.collateral_btc,
            "borrowed_usd":     l.borrowed_usd,
            "collateral_ratio": format!("{:.4}", l.collateral_ratio),
        }))
        .collect();

    Json(json!({
        "count": loans.len(),
        "loans": loans,
    }))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::{OracleFeed, OracleRegistry, OracleReport, LendingProtocol, price_from_f64};

    fn make_db() -> DefiDb {
        Arc::new(Mutex::new(DefiState {
            oracle:  OracleRegistry::new(),
            lending: LendingProtocol::new("BTC/USD", 1.5),
        }))
    }

    fn populated_db() -> DefiDb {
        let mut oracle = OracleRegistry::new();

        // Feed 1: BTC/USD với 1 round settled
        let mut btc_feed = OracleFeed::new("BTC/USD", "Bitcoin / US Dollar", 1, 10.0, 3600);
        btc_feed.authorize("node1");
        let report = OracleReport::new("BTC/USD", price_from_f64(50_000.0), 1_000_000, "node1", 1);
        btc_feed.submit(report).unwrap();
        oracle.add_feed(btc_feed);

        // Feed 2: ETH/USD (no history yet)
        let eth_feed = OracleFeed::new("ETH/USD", "Ethereum / US Dollar", 2, 10.0, 3600);
        oracle.add_feed(eth_feed);

        // Lending protocol với 2 loans
        let mut lending = LendingProtocol::new("BTC/USD", 1.5);
        // alice: 5.0 BTC, $10k borrowed, $50k price → ratio=25.0 ✓
        lending.create_loan("alice", 5.0, 10_000.0, 50_000.0).unwrap();
        // bob:   0.4 BTC, $10k borrowed, $50k price → ratio=2.0  ✓
        lending.create_loan("bob",   0.4, 10_000.0, 50_000.0).unwrap();
        // Price crash to $30k: alice=15.0 ✓, bob=1.2 → liquidatable
        lending.update_prices(30_000.0);

        Arc::new(Mutex::new(DefiState { oracle, lending }))
    }

    // ── DefiDb type ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_defi_db_empty() {
        let db    = make_db();
        let state = db.lock().await;
        assert!(state.oracle.feeds.is_empty());
        assert!(state.lending.loans.is_empty());
    }

    #[test]
    fn test_defi_router_builds() {
        let db = make_db();
        let _r = defi_router(db);
    }

    // ── GET /api/defi/feeds ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_feeds_count() {
        let db    = populated_db();
        let state = db.lock().await;
        assert_eq!(state.oracle.feeds.len(), 2);
    }

    #[tokio::test]
    async fn test_feeds_has_btc_usd() {
        let db    = populated_db();
        let state = db.lock().await;
        assert!(state.oracle.feeds.contains_key("BTC/USD"));
    }

    #[tokio::test]
    async fn test_feeds_has_eth_usd() {
        let db    = populated_db();
        let state = db.lock().await;
        assert!(state.oracle.feeds.contains_key("ETH/USD"));
    }

    #[tokio::test]
    async fn test_feed_description() {
        let db    = populated_db();
        let state = db.lock().await;
        let f = &state.oracle.feeds["BTC/USD"];
        assert_eq!(f.description, "Bitcoin / US Dollar");
    }

    // ── GET /api/defi/feed/:id ────────────────────────────────────────────

    #[tokio::test]
    async fn test_feed_detail_found() {
        let db    = populated_db();
        let state = db.lock().await;
        assert!(state.oracle.feeds.get("BTC/USD").is_some());
    }

    #[tokio::test]
    async fn test_feed_detail_not_found() {
        let db    = make_db();
        let state = db.lock().await;
        assert!(state.oracle.feeds.get("GHOST").is_none());
    }

    #[tokio::test]
    async fn test_feed_has_round_history() {
        let db    = populated_db();
        let state = db.lock().await;
        let f = &state.oracle.feeds["BTC/USD"];
        assert_eq!(f.history.len(), 1);
    }

    #[tokio::test]
    async fn test_feed_eth_no_history() {
        let db    = populated_db();
        let state = db.lock().await;
        let f = &state.oracle.feeds["ETH/USD"];
        assert_eq!(f.history.len(), 0);
    }

    #[tokio::test]
    async fn test_feed_latest_price() {
        let db    = populated_db();
        let state = db.lock().await;
        let price = state.oracle.latest_price("BTC/USD");
        assert!(price.is_some());
        let p = price.unwrap();
        assert!(p > 40_000.0 && p < 60_000.0);
    }

    #[tokio::test]
    async fn test_feed_no_price_eth() {
        let db    = populated_db();
        let state = db.lock().await;
        assert!(state.oracle.latest_price("ETH/USD").is_none());
    }

    #[tokio::test]
    async fn test_feed_decimals() {
        let db    = populated_db();
        let state = db.lock().await;
        let f = &state.oracle.feeds["BTC/USD"];
        assert_eq!(f.decimals, 8);
    }

    // ── GET /api/defi/feed/:id/history ────────────────────────────────────

    #[tokio::test]
    async fn test_history_btc_one_round() {
        let db    = populated_db();
        let state = db.lock().await;
        let f = &state.oracle.feeds["BTC/USD"];
        assert_eq!(f.history.len(), 1);
        assert_eq!(f.history[0].round, 1);
    }

    #[tokio::test]
    async fn test_history_round_report_count() {
        let db    = populated_db();
        let state = db.lock().await;
        let r = &state.oracle.feeds["BTC/USD"].history[0];
        assert_eq!(r.report_count, 1);
    }

    // ── GET /api/defi/loans ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_loans_count() {
        let db    = populated_db();
        let state = db.lock().await;
        assert_eq!(state.lending.loans.len(), 2);
    }

    #[tokio::test]
    async fn test_loan_alice_healthy() {
        let db    = populated_db();
        let state = db.lock().await;
        let alice = state.lending.loans.iter().find(|l| l.borrower == "alice").unwrap();
        assert!(!alice.liquidatable);
        // after price crash to 30k: ratio = 5.0 * 30_000 / 10_000 = 15.0
        assert!((alice.collateral_ratio - 15.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_loan_bob_liquidatable() {
        let db    = populated_db();
        let state = db.lock().await;
        let bob = state.lending.loans.iter().find(|l| l.borrower == "bob").unwrap();
        assert!(bob.liquidatable);
        // after price crash to 30k: ratio = 0.4 * 30_000 / 10_000 = 1.2 < min_collateral 1.5
        assert!((bob.collateral_ratio - 1.2).abs() < 0.01);
    }

    // ── GET /api/defi/loans/liquidatable ──────────────────────────────────

    #[tokio::test]
    async fn test_liquidatable_count() {
        let db    = populated_db();
        let state = db.lock().await;
        let count = state.lending.liquidatable_loans().len();
        assert_eq!(count, 1); // only bob
    }

    #[tokio::test]
    async fn test_liquidatable_is_bob() {
        let db    = populated_db();
        let state = db.lock().await;
        let loans = state.lending.liquidatable_loans();
        assert_eq!(loans[0].borrower, "bob");
    }

    #[tokio::test]
    async fn test_no_liquidatable_when_price_rises() {
        let mut lending = LendingProtocol::new("BTC/USD", 1.5);
        // Create at $50k: ratio = 0.5*50k/10k = 2.5 ✓
        lending.create_loan("carol", 0.5, 10_000.0, 50_000.0).unwrap();
        // Crash to $20k: ratio = 0.5*20k/10k = 1.0 < 1.5 → liquidatable
        lending.update_prices(20_000.0);
        assert_eq!(lending.liquidatable_loans().len(), 1);
        // Recovery to $100k: ratio = 0.5*100k/10k = 5.0 > 1.5 → healthy again
        lending.update_prices(100_000.0);
        assert_eq!(lending.liquidatable_loans().len(), 0);
    }

    #[tokio::test]
    async fn test_all_liquidatable_when_price_crashes() {
        let mut lending = LendingProtocol::new("BTC/USD", 1.5);
        lending.create_loan("alice", 1.0, 20_000.0, 50_000.0).unwrap(); // ratio=2.5
        lending.create_loan("bob",   0.5, 10_000.0, 50_000.0).unwrap(); // ratio=2.5
        lending.update_prices(10_000.0); // BTC crashes to 10k
        // alice: 1.0 * 10_000 / 20_000 = 0.5 < 1.5 → liquidatable
        // bob:   0.5 * 10_000 / 10_000 = 0.5 < 1.5 → liquidatable
        assert_eq!(lending.liquidatable_loans().len(), 2);
    }

    // ── Collateral validation ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_loan_creation_fails_low_collateral() {
        let mut lending = LendingProtocol::new("BTC/USD", 1.5);
        let result = lending.create_loan("alice", 0.01, 10_000.0, 50_000.0); // ratio=0.05 < 1.5
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_loan_creation_succeeds_exact_min() {
        let mut lending = LendingProtocol::new("BTC/USD", 1.5);
        // ratio = 0.3 * 50_000 / 10_000 = 1.5 exactly
        let result = lending.create_loan("alice", 0.3, 10_000.0, 50_000.0);
        assert!(result.is_ok());
    }

    // ── Oracle report verification ─────────────────────────────────────────

    #[tokio::test]
    async fn test_report_signature_valid() {
        let report = OracleReport::new("BTC/USD", price_from_f64(50_000.0), 1_000_000, "node1", 1);
        assert!(report.verify());
    }

    #[tokio::test]
    async fn test_report_unauthorized_rejected() {
        let mut feed = OracleFeed::new("BTC/USD", "BTC", 1, 10.0, 3600);
        // Don't authorize "node1"
        let report = OracleReport::new("BTC/USD", price_from_f64(50_000.0), 1_000_000, "node1", 1);
        let result = feed.submit(report);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_price_from_f64_roundtrip() {
        use crate::oracle::price_to_f64;
        let original = 50_000.0_f64;
        let encoded  = price_from_f64(original);
        let decoded  = price_to_f64(encoded);
        assert!((decoded - original).abs() < 0.01);
    }
}
