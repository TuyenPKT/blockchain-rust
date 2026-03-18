#![allow(dead_code)]
//! v9.2 — Contract API (GET only, Zero-Trust)
//!
//! REST endpoints exposing `ContractRegistry` to PKTScan.
//! Tất cả endpoints đều read-only (GET).
//! ZT middleware (rate limit + audit log) áp dụng ở router level.
//!
//! Endpoints:
//!   GET /api/contracts                        → list tất cả contracts
//!   GET /api/contract/:addr                   → contract detail + storage root
//!   GET /api/contract/:addr/state             → full storage key-value snapshot
//!   GET /api/contract/:addr/state/:key        → giá trị của một storage key
//!
//! Usage:
//!   let contract_db = ContractDb::new(registry);
//!   let app = router.merge(contract_api::contract_router(contract_db));

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::smart_contract::ContractRegistry;

pub type ContractDb = Arc<Mutex<ContractRegistry>>;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn contract_router(state: ContractDb) -> Router {
    Router::new()
        .route("/api/contracts",                          get(get_contracts))
        .route("/api/contract/:addr",                     get(get_contract))
        .route("/api/contract/:addr/state",               get(get_contract_state))
        .route("/api/contract/:addr/state/:key",          get(get_contract_state_key))
        .with_state(state)
}

// ─── GET /api/contracts ───────────────────────────────────────────────────────

/// List tất cả contracts, sorted by deploy_block asc.
async fn get_contracts(State(db): State<ContractDb>) -> Json<Value> {
    let reg = db.lock().await;

    let mut contracts: Vec<Value> = reg.contracts.values()
        .map(|c| {
            let fn_count = c.module.functions.len();
            json!({
                "address":      c.address,
                "name":         c.module.name,
                "creator":      c.creator,
                "deploy_block": c.deploy_block,
                "call_count":   c.call_count,
                "total_gas":    c.total_gas,
                "fn_count":     fn_count,
                "storage_root": c.storage.storage_root(),
            })
        })
        .collect();

    contracts.sort_by_key(|v| v["deploy_block"].as_u64().unwrap_or(0));

    Json(json!({
        "count":     contracts.len(),
        "contracts": contracts,
    }))
}

// ─── GET /api/contract/:addr ──────────────────────────────────────────────────

/// Contract detail: metadata + function list + storage root.
async fn get_contract(
    State(db):   State<ContractDb>,
    Path(addr):  Path<String>,
) -> (StatusCode, Json<Value>) {
    let reg = db.lock().await;

    match reg.contracts.get(&addr) {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("contract '{}' not found", addr) })),
        ),
        Some(c) => {
            let functions: Vec<Value> = c.module.functions.values()
                .map(|f| json!({
                    "name":        f.name,
                    "param_count": f.params.len(),
                    "params":      f.params,
                }))
                .collect();

            (StatusCode::OK, Json(json!({
                "address":      c.address,
                "name":         c.module.name,
                "creator":      c.creator,
                "deploy_block": c.deploy_block,
                "call_count":   c.call_count,
                "total_gas":    c.total_gas,
                "storage_root": c.storage.storage_root(),
                "functions":    functions,
            })))
        }
    }
}

// ─── GET /api/contract/:addr/state ───────────────────────────────────────────

/// Full storage snapshot (tất cả key-value pairs).
async fn get_contract_state(
    State(db):   State<ContractDb>,
    Path(addr):  Path<String>,
) -> (StatusCode, Json<Value>) {
    let reg = db.lock().await;

    match reg.contracts.get(&addr) {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("contract '{}' not found", addr) })),
        ),
        Some(c) => {
            let entries: Vec<Value> = c.storage.data.iter()
                .map(|(k, v)| json!({ "key": k, "value": v }))
                .collect();

            (StatusCode::OK, Json(json!({
                "address":      addr,
                "storage_root": c.storage.storage_root(),
                "entry_count":  entries.len(),
                "state":        entries,
            })))
        }
    }
}

// ─── GET /api/contract/:addr/state/:key ──────────────────────────────────────

/// Giá trị của một storage key cụ thể.
async fn get_contract_state_key(
    State(db):          State<ContractDb>,
    Path((addr, key)):  Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let reg = db.lock().await;

    if !reg.contracts.contains_key(&addr) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("contract '{}' not found", addr) })),
        );
    }

    let value = reg.storage_of(&addr, &key);

    (StatusCode::OK, Json(json!({
        "address": addr,
        "key":     key,
        "value":   value,
    })))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smart_contract::{ContractRegistry, counter_contract, token_contract};

    fn make_db() -> ContractDb {
        Arc::new(Mutex::new(ContractRegistry::new()))
    }

    fn populated_db() -> ContractDb {
        let mut reg = ContractRegistry::new();
        reg.deploy(counter_contract(), "alice");
        reg.deploy(token_contract(1000, 500), "bob");
        Arc::new(Mutex::new(reg))
    }

    // ── ContractDb type ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_contract_db_empty() {
        let db  = make_db();
        let reg = db.lock().await;
        assert!(reg.contracts.is_empty());
    }

    #[test]
    fn test_contract_router_builds() {
        let db = make_db();
        let _r = contract_router(db);
    }

    // ── GET /api/contracts (via direct logic) ────────────────────────────

    #[tokio::test]
    async fn test_contracts_list_count() {
        let db  = populated_db();
        let reg = db.lock().await;
        assert_eq!(reg.contracts.len(), 2);
    }

    #[tokio::test]
    async fn test_contracts_list_has_counter() {
        let db  = populated_db();
        let reg = db.lock().await;
        let has_counter = reg.contracts.values()
            .any(|c| c.module.name == "Counter");
        assert!(has_counter);
    }

    #[tokio::test]
    async fn test_contracts_list_has_token() {
        let db  = populated_db();
        let reg = db.lock().await;
        let has_token = reg.contracts.values()
            .any(|c| c.module.name == "Token");
        assert!(has_token);
    }

    #[tokio::test]
    async fn test_contracts_fields_present() {
        let db  = populated_db();
        let reg = db.lock().await;
        let c   = reg.contracts.values().next().unwrap();
        assert!(!c.address.is_empty());
        assert!(!c.creator.is_empty());
    }

    // ── GET /api/contract/:addr (via direct logic) ────────────────────────

    #[tokio::test]
    async fn test_contract_detail_found() {
        let db  = populated_db();
        let reg = db.lock().await;
        let addr = reg.contracts.values()
            .find(|c| c.module.name == "Counter")
            .unwrap()
            .address.clone();
        assert!(reg.contracts.contains_key(&addr));
    }

    #[tokio::test]
    async fn test_contract_detail_not_found() {
        let db  = make_db();
        let reg = db.lock().await;
        assert!(reg.contracts.get("0xdeadbeef").is_none());
    }

    #[tokio::test]
    async fn test_contract_has_functions() {
        let db  = populated_db();
        let reg = db.lock().await;
        let c   = reg.contracts.values()
            .find(|c| c.module.name == "Counter")
            .unwrap();
        assert!(!c.module.functions.is_empty());
    }

    #[tokio::test]
    async fn test_counter_has_increment_fn() {
        let db  = populated_db();
        let reg = db.lock().await;
        let c   = reg.contracts.values()
            .find(|c| c.module.name == "Counter")
            .unwrap();
        let has_increment = c.module.functions.values().any(|f| f.name == "increment");
        assert!(has_increment);
    }

    #[tokio::test]
    async fn test_contract_deploy_block_zero() {
        let db  = populated_db();
        let reg = db.lock().await;
        // block_height starts at 0, so deploy_block = 0 for all
        for c in reg.contracts.values() {
            assert_eq!(c.deploy_block, 0);
        }
    }

    #[tokio::test]
    async fn test_contract_initial_call_count_zero() {
        let db  = populated_db();
        let reg = db.lock().await;
        for c in reg.contracts.values() {
            assert_eq!(c.call_count, 0);
        }
    }

    // ── GET /api/contract/:addr/state (via direct logic) ──────────────────

    #[tokio::test]
    async fn test_state_empty_initially() {
        let db  = populated_db();
        let reg = db.lock().await;
        let c   = reg.contracts.values()
            .find(|c| c.module.name == "Counter")
            .unwrap();
        // Counter starts with no storage entries
        assert_eq!(c.storage.data.len(), 0);
    }

    #[tokio::test]
    async fn test_state_storage_root_deterministic() {
        let db   = populated_db();
        let reg  = db.lock().await;
        let c    = reg.contracts.values().next().unwrap();
        let root1 = c.storage.storage_root();
        let root2 = c.storage.storage_root();
        assert_eq!(root1, root2);
    }

    // ── GET /api/contract/:addr/state/:key (via direct logic) ─────────────

    #[tokio::test]
    async fn test_state_key_missing_returns_zero() {
        let db  = populated_db();
        let reg = db.lock().await;
        let c   = reg.contracts.values().next().unwrap();
        let val = c.storage.get("nonexistent_key");
        assert_eq!(val, 0);
    }

    #[tokio::test]
    async fn test_storage_of_unknown_contract_zero() {
        let db  = make_db();
        let reg = db.lock().await;
        let val = reg.storage_of("0xfake", "some_key");
        assert_eq!(val, 0);
    }

    // ── Token contract storage ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_token_contract_storage_after_init() {
        let mut reg = ContractRegistry::new();
        let addr = reg.deploy(token_contract(1000, 500), "alice");
        // Must call init() to populate storage (2x StorageStore @ 5000 each = 10002 gas minimum)
        reg.call(&addr, "init", vec![], 100_000).unwrap();
        let alice_bal = reg.storage_of(&addr, "balance_alice");
        let bob_bal   = reg.storage_of(&addr, "balance_bob");
        assert_eq!(alice_bal, 1000);
        assert_eq!(bob_bal, 500);
    }

    #[tokio::test]
    async fn test_token_contract_unknown_key_zero() {
        let mut reg = ContractRegistry::new();
        let addr = reg.deploy(token_contract(1000, 500), "alice");
        let val = reg.storage_of(&addr, "charlie");
        assert_eq!(val, 0);
    }

    // ── Address uniqueness ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_two_contracts_different_addresses() {
        let db  = populated_db();
        let reg = db.lock().await;
        let addrs: Vec<&str> = reg.contracts.keys().map(|s| s.as_str()).collect();
        assert_eq!(addrs.len(), 2);
        assert_ne!(addrs[0], addrs[1]);
    }

    #[tokio::test]
    async fn test_address_starts_with_0x() {
        let db  = populated_db();
        let reg = db.lock().await;
        for c in reg.contracts.values() {
            assert!(c.address.starts_with("0x"));
        }
    }

    // ── ContractRegistry call count ────────────────────────────────────────

    #[tokio::test]
    async fn test_call_count_increments() {
        let mut reg = ContractRegistry::new();
        let addr = reg.deploy(counter_contract(), "alice");
        reg.call(&addr, "increment", vec![], 10_000).unwrap();
        reg.call(&addr, "increment", vec![], 10_000).unwrap();
        assert_eq!(reg.contracts[&addr].call_count, 2);
    }

    #[tokio::test]
    async fn test_total_gas_accumulates() {
        let mut reg = ContractRegistry::new();
        let addr = reg.deploy(counter_contract(), "alice");
        reg.call(&addr, "increment", vec![], 10_000).unwrap();
        assert!(reg.contracts[&addr].total_gas > 0);
    }

    // ── fn_count helper ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_counter_fn_count() {
        let db  = populated_db();
        let reg = db.lock().await;
        let c   = reg.contracts.values()
            .find(|c| c.module.name == "Counter")
            .unwrap();
        // counter_contract has increment, decrement, get_count → 3
        assert_eq!(c.module.functions.len(), 3);
    }
}
