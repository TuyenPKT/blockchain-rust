#![allow(dead_code)]
//! v9.1 — Token API (GET only, Zero-Trust)
//!
//! REST endpoints exposing `TokenRegistry` to PKTScan.
//! Tất cả endpoints đều read-only (GET).
//! ZT middleware (rate limit + audit log) áp dụng ở router level.
//!
//! Endpoints:
//!   GET /api/tokens                      → list tất cả tokens
//!   GET /api/token/:id                   → token detail + holder_count
//!   GET /api/token/:id/holders           → top holders (sorted by balance desc)
//!   GET /api/token/:id/balance/:addr     → balance của một địa chỉ cụ thể
//!
//! Usage:
//!   let token_db = TokenDb::new(registry);
//!   let app = router.merge(token_api::token_router(token_db));

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::token::TokenRegistry;

pub type TokenDb = Arc<Mutex<TokenRegistry>>;

// ─── Query params ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct HolderParams {
    /// Số lượng holders trả về (default 20, max 100).
    #[serde(default = "default_holder_limit")]
    pub limit: usize,
}
fn default_holder_limit() -> usize { 20 }

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn token_router(state: TokenDb) -> Router {
    Router::new()
        .route("/api/tokens",                        get(get_tokens))
        .route("/api/token/:id",                     get(get_token))
        .route("/api/token/:id/holders",             get(get_token_holders))
        .route("/api/token/:id/balance/:addr",       get(get_token_balance))
        .with_state(state)
}

// ─── GET /api/tokens ──────────────────────────────────────────────────────────

/// List tất cả tokens trong registry, sorted by total_supply desc.
async fn get_tokens(State(db): State<TokenDb>) -> Json<Value> {
    let reg = db.lock().await;

    let mut tokens: Vec<Value> = reg.tokens.values()
        .map(|t| {
            let holder_count = count_holders(&reg, &t.id);
            json!({
                "id":           t.id,
                "name":         t.name,
                "symbol":       t.symbol,
                "decimals":     t.decimals,
                "total_supply": t.total_supply.to_string(),
                "owner":        t.owner,
                "holder_count": holder_count,
            })
        })
        .collect();

    // Sort by total_supply descending
    tokens.sort_by(|a, b| {
        let sa = a["total_supply"].as_str().unwrap_or("0").parse::<u128>().unwrap_or(0);
        let sb = b["total_supply"].as_str().unwrap_or("0").parse::<u128>().unwrap_or(0);
        sb.cmp(&sa)
    });

    Json(json!({
        "count":  tokens.len(),
        "tokens": tokens,
    }))
}

// ─── GET /api/token/:id ───────────────────────────────────────────────────────

/// Token detail: metadata + holder count + circulating supply.
async fn get_token(
    State(db):  State<TokenDb>,
    Path(id):   Path<String>,
) -> (StatusCode, Json<Value>) {
    let reg = db.lock().await;

    match reg.tokens.get(&id) {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("token '{}' not found", id) })),
        ),
        Some(t) => {
            let holder_count = count_holders(&reg, &t.id);
            (StatusCode::OK, Json(json!({
                "id":           t.id,
                "name":         t.name,
                "symbol":       t.symbol,
                "decimals":     t.decimals,
                "total_supply": t.total_supply.to_string(),
                "owner":        t.owner,
                "holder_count": holder_count,
            })))
        }
    }
}

// ─── GET /api/token/:id/holders ───────────────────────────────────────────────

/// Top holders của một token, sorted by balance desc.
async fn get_token_holders(
    State(db):      State<TokenDb>,
    Path(id):       Path<String>,
    Query(params):  Query<HolderParams>,
) -> (StatusCode, Json<Value>) {
    let reg = db.lock().await;

    if !reg.tokens.contains_key(&id) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("token '{}' not found", id) })),
        );
    }

    let limit = params.limit.min(100);

    let mut holders: Vec<(&str, u128)> = reg.accounts.iter()
        .filter(|((tid, _), acc)| tid == &id && acc.balance > 0)
        .map(|((_, addr), acc)| (addr.as_str(), acc.balance))
        .collect();

    holders.sort_by(|a, b| b.1.cmp(&a.1));

    let total_supply = reg.tokens[&id].total_supply;
    let holder_count = holders.len();

    let result: Vec<Value> = holders.iter()
        .take(limit)
        .enumerate()
        .map(|(i, (addr, bal))| {
            let pct = if total_supply > 0 {
                (*bal as f64 / total_supply as f64) * 100.0
            } else {
                0.0
            };
            json!({
                "rank":    i + 1,
                "address": addr,
                "balance": bal.to_string(),
                "percent": format!("{:.4}", pct),
            })
        })
        .collect();

    (StatusCode::OK, Json(json!({
        "token_id":     id,
        "total_supply": total_supply.to_string(),
        "holder_count": holder_count,
        "limit":        limit,
        "holders":      result,
    })))
}

// ─── GET /api/token/:id/balance/:addr ────────────────────────────────────────

/// Balance của một địa chỉ cụ thể cho token.
async fn get_token_balance(
    State(db):          State<TokenDb>,
    Path((id, addr)):   Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let reg = db.lock().await;

    if !reg.tokens.contains_key(&id) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("token '{}' not found", id) })),
        );
    }

    let balance = reg.balance_of(&id, &addr);
    let total   = reg.tokens[&id].total_supply;
    let pct = if total > 0 { (balance as f64 / total as f64) * 100.0 } else { 0.0 };

    (StatusCode::OK, Json(json!({
        "token_id": id,
        "address":  addr,
        "balance":  balance.to_string(),
        "percent":  format!("{:.4}", pct),
    })))
}

// ─── Helper ───────────────────────────────────────────────────────────────────

/// Số địa chỉ có balance > 0 cho token.
fn count_holders(reg: &TokenRegistry, token_id: &str) -> usize {
    reg.accounts.iter()
        .filter(|((tid, _), acc)| tid == token_id && acc.balance > 0)
        .count()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenRegistry;

    fn make_db() -> TokenDb {
        Arc::new(Mutex::new(TokenRegistry::new()))
    }

    fn populated_db() -> TokenDb {
        let mut reg = TokenRegistry::new();
        reg.create_token("PKT", "PKT Token", "PKT", 9, 1_000_000, "alice").unwrap();
        reg.create_token("USDT", "Tether USD", "USDT", 6, 5_000_000, "bob").unwrap();
        reg.mint("PKT", "charlie", 50_000).unwrap();
        reg.mint("PKT", "dave", 10_000).unwrap();
        Arc::new(Mutex::new(reg))
    }

    // ── TokenDb type ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_token_db_empty() {
        let db  = make_db();
        let reg = db.lock().await;
        assert!(reg.tokens.is_empty());
    }

    #[test]
    fn test_token_router_builds() {
        let db = make_db();
        let _r = token_router(db);
    }

    // ── count_holders ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_count_holders_zero_for_empty() {
        let db  = make_db();
        let reg = db.lock().await;
        assert_eq!(count_holders(&reg, "NONE"), 0);
    }

    #[tokio::test]
    async fn test_count_holders_includes_initial_owner() {
        let db  = populated_db();
        let reg = db.lock().await;
        // alice has 1_000_000, charlie has 50_000, dave has 10_000 → 3 holders
        assert_eq!(count_holders(&reg, "PKT"), 3);
    }

    #[tokio::test]
    async fn test_count_holders_excludes_zero_balance() {
        let mut reg = TokenRegistry::new();
        reg.create_token("X", "X Token", "X", 0, 0, "owner").unwrap();
        // owner has balance=0 (initial_supply=0) → 0 holders
        assert_eq!(count_holders(&reg, "X"), 0);
    }

    #[tokio::test]
    async fn test_count_holders_after_burn_to_zero() {
        let mut reg = TokenRegistry::new();
        reg.create_token("X", "X", "X", 0, 100, "alice").unwrap();
        reg.burn("X", "alice", 100).unwrap();
        assert_eq!(count_holders(&reg, "X"), 0);
    }

    // ── GET /api/tokens (via direct logic) ────────────────────────────────

    #[tokio::test]
    async fn test_tokens_list_count() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.tokens.len(), 2);
    }

    #[tokio::test]
    async fn test_tokens_list_contains_pkt() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert!(reg.tokens.contains_key("PKT"));
    }

    // ── GET /api/token/:id (via direct logic) ─────────────────────────────

    #[tokio::test]
    async fn test_token_detail_known() {
        let db  = populated_db();
        let reg = db.lock().await;
        let t   = reg.tokens.get("PKT").unwrap();
        assert_eq!(t.symbol, "PKT");
        assert_eq!(t.decimals, 9);
    }

    #[tokio::test]
    async fn test_token_detail_total_supply() {
        let db  = populated_db();
        let reg = db.lock().await;
        // initial 1_000_000 + mint 50_000 + 10_000
        assert_eq!(reg.total_supply("PKT"), 1_060_000);
    }

    #[tokio::test]
    async fn test_token_unknown_not_found() {
        let db  = make_db();
        let reg = db.lock().await;
        assert!(reg.tokens.get("GHOST").is_none());
    }

    // ── GET /api/token/:id/holders (via direct logic) ─────────────────────

    #[tokio::test]
    async fn test_holders_sorted_desc() {
        let db  = populated_db();
        let reg = db.lock().await;
        let mut holders: Vec<u128> = reg.accounts.iter()
            .filter(|((tid, _), acc)| tid == "PKT" && acc.balance > 0)
            .map(|(_, acc)| acc.balance)
            .collect();
        holders.sort_by(|a, b| b.cmp(a));
        // alice=1_000_000 > charlie=50_000 > dave=10_000
        assert_eq!(holders[0], 1_000_000);
        assert_eq!(holders[1], 50_000);
        assert_eq!(holders[2], 10_000);
    }

    #[tokio::test]
    async fn test_holders_count_correct() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(count_holders(&reg, "PKT"), 3);
        assert_eq!(count_holders(&reg, "USDT"), 1); // only bob
    }

    #[tokio::test]
    async fn test_holders_limit_clamp() {
        let p = HolderParams { limit: 999 };
        assert_eq!(p.limit.min(100), 100);
    }

    #[tokio::test]
    async fn test_holders_percent_sums_to_100_single_holder() {
        let mut reg = TokenRegistry::new();
        reg.create_token("Y", "Y", "Y", 0, 1000, "solo").unwrap();
        let bal   = reg.balance_of("Y", "solo") as f64;
        let total = reg.total_supply("Y") as f64;
        let pct   = bal / total * 100.0;
        assert!((pct - 100.0).abs() < 0.001);
    }

    // ── GET /api/token/:id/balance/:addr (via direct logic) ───────────────

    #[tokio::test]
    async fn test_balance_known_address() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.balance_of("PKT", "alice"), 1_000_000);
        assert_eq!(reg.balance_of("PKT", "charlie"), 50_000);
        assert_eq!(reg.balance_of("PKT", "dave"), 10_000);
    }

    #[tokio::test]
    async fn test_balance_unknown_address_is_zero() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.balance_of("PKT", "nobody"), 0);
    }

    #[tokio::test]
    async fn test_balance_wrong_token_is_zero() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.balance_of("GHOST", "alice"), 0);
    }

    #[tokio::test]
    async fn test_balance_percent_calculation() {
        let db    = populated_db();
        let reg   = db.lock().await;
        let bal   = reg.balance_of("PKT", "charlie") as f64;
        let total = reg.total_supply("PKT") as f64;
        let pct   = bal / total * 100.0;
        // charlie = 50_000 / 1_060_000 ≈ 4.7%
        assert!(pct > 4.0 && pct < 5.0);
    }

    // ── Multiple tokens ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_two_tokens_independent() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.balance_of("PKT",  "alice"), 1_000_000);
        assert_eq!(reg.balance_of("USDT", "alice"), 0);
        assert_eq!(reg.balance_of("USDT", "bob"),   5_000_000);
    }

    #[tokio::test]
    async fn test_holder_count_independent_per_token() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(count_holders(&reg, "PKT"),  3);
        assert_eq!(count_holders(&reg, "USDT"), 1);
    }
}
