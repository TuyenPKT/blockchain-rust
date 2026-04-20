#![allow(dead_code)]
//! v15.4 — Explorer Live Data
//!
//! Adapter layer: SyncDb (wire headers) + UtxoSyncDb (UTXOs) → JSON API.
//!
//! Adds new Axum routes under /api/testnet/* mounted alongside pktscan_api:
//!
//!   GET /api/testnet/stats                  → sync state, height, UTXO count
//!   GET /api/testnet/headers?limit&offset   → list wire block headers (newest first)
//!   GET /api/testnet/header/:height         → single header detail
//!   GET /api/testnet/balance/:script_hex    → UTXO balance for a script
//!   GET /api/testnet/utxos/:script_hex      → list UTXOs for a script
//!
//! Pure data-layer functions are unit-tested directly (no HTTP).
//! Axum router is thin glue — correctness lives in the query functions.
//!
//! CLI: `cargo run -- pktscan [port] --testnet`  (starts with testnet overlay)

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::pkt_sync::SyncDb;
use crate::pkt_utxo_sync::{UtxoEntry, UtxoSyncDb};
use crate::pkt_wire::WireBlockHeader;

// ── Shared state ──────────────────────────────────────────────────────────────

/// Axum state: both DBs wrapped in Arc for sharing across handlers.
#[derive(Clone)]
pub struct TestnetState {
    pub sync_db: Arc<SyncDb>,
    pub utxo_db: Arc<UtxoSyncDb>,
}

impl TestnetState {
    pub fn new(sync_db: SyncDb, utxo_db: UtxoSyncDb) -> Self {
        Self {
            sync_db: Arc::new(sync_db),
            utxo_db: Arc::new(utxo_db),
        }
    }
}

// ── Query params ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct HeaderListParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct HeaderCursorParams {
    pub cursor: Option<u64>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize { 20 }

// ── Pure data-layer functions (unit-testable, no async) ───────────────────────

/// Convert a WireBlockHeader to a JSON block-like object.
pub fn format_header_json(header: &WireBlockHeader, height: u64) -> Value {
    let hash     = header.block_hash();
    let hash_hex = hex::encode(hash);
    let prev_hex = hex::encode(header.prev_block);
    let merkle   = hex::encode(header.merkle_root);
    json!({
        "height":      height,
        "hash":        hash_hex,
        "prev_hash":   prev_hex,
        "merkle_root": merkle,
        "timestamp":   header.timestamp,
        "bits":        header.bits,
        "nonce":       header.nonce,
        "version":     header.version,
    })
}

/// Fetch the latest N headers from SyncDb, newest first.
///
/// `cursor`: None = from tip; Some(h) = start from h-1 (exclusive cursor for next-page).
/// Returns `(headers_json, total_synced_height)`.
pub fn query_headers(
    sync_db: &SyncDb,
    limit:   usize,
    cursor:  Option<u64>,
) -> Result<(Vec<Value>, u64), String> {
    let tip = sync_db.get_sync_height()
        .map_err(|e| e.to_string())?
        .unwrap_or(0);

    if tip == 0 {
        return Ok((vec![], 0));
    }

    let start   = match cursor {
        None    => tip,
        Some(h) => h.saturating_sub(1),
    };
    let mut out = Vec::new();
    let mut h   = start;

    while out.len() < limit && h >= 1 {
        match sync_db.load_header(h) {
            Ok(Some(raw)) => {
                match WireBlockHeader::from_bytes(&raw) {
                    Ok(hdr) => out.push(format_header_json(&hdr, h)),
                    Err(_)  => {} // skip corrupt entries
                }
            }
            _ => {}
        }
        if h == 0 { break; }
        h -= 1;
    }

    Ok((out, tip))
}

/// Fetch a single header from SyncDb by height.
pub fn query_header(sync_db: &SyncDb, height: u64) -> Result<Option<Value>, String> {
    match sync_db.load_header(height).map_err(|e| e.to_string())? {
        None      => Ok(None),
        Some(raw) => match WireBlockHeader::from_bytes(&raw) {
            Ok(hdr) => Ok(Some(format_header_json(&hdr, height))),
            Err(e)  => Err(format!("corrupt header at {}: {:?}", height, e)),
        },
    }
}

/// Convert a UtxoEntry to JSON.
pub fn format_utxo_json(entry: &UtxoEntry) -> Value {
    json!({
        "txid":          entry.txid,
        "vout":          entry.vout,
        "amount":        entry.value,
        "value":         entry.value,
        "script_pubkey": hex::encode(&entry.script_pubkey),
        "height":        entry.height,
    })
}

/// List all UTXOs from UtxoSyncDb whose script_pubkey starts with `script_prefix` bytes.
///
/// `script_hex` is hex-encoded bytes to match against the start of script_pubkey.
/// Empty string → returns all UTXOs (up to limit).
pub fn query_utxos(
    utxo_db:    &UtxoSyncDb,
    script_hex: &str,
    limit:      usize,
) -> Result<Vec<Value>, String> {
    let prefix = hex::decode(script_hex).unwrap_or_default();
    let mut out = Vec::new();

    for (_, v) in utxo_db.raw_kv().scan_prefix(b"utxo:") {
        if let Ok(entry) = serde_json::from_slice::<UtxoEntry>(&v) {
            if prefix.is_empty() || entry.script_pubkey.starts_with(&prefix) {
                out.push(format_utxo_json(&entry));
                if out.len() >= limit { break; }
            }
        }
    }
    Ok(out)
}

/// Sum of all UTXO values whose script_pubkey starts with `script_hex`.
pub fn query_balance(utxo_db: &UtxoSyncDb, script_hex: &str) -> Result<u64, String> {
    let utxos  = query_utxos(utxo_db, script_hex, usize::MAX)?;
    let total  = utxos.iter()
        .filter_map(|v| v["value"].as_u64())
        .sum();
    Ok(total)
}

/// Combined sync stats.
pub fn query_sync_stats(sync_db: &SyncDb, utxo_db: &UtxoSyncDb) -> Value {
    let sync_height  = sync_db.get_sync_height().ok().flatten().unwrap_or(0);
    let utxo_height  = utxo_db.get_utxo_height().ok().flatten().unwrap_or(0);
    let utxo_count   = utxo_db.count_utxos().unwrap_or(0);
    let total_value  = utxo_db.total_value().unwrap_or(0);
    let tip_hash     = sync_db.get_tip_hash().ok().flatten()
        .map(|h| hex::encode(h))
        .unwrap_or_else(|| "0".repeat(64));

    json!({
        "network":        "testnet",
        "sync_height":    sync_height,
        "utxo_height":    utxo_height,
        "utxo_count":     utxo_count,
        "total_value_sat": total_value,
        "tip_hash":       tip_hash,
        "synced":         sync_height > 0,
    })
}


// ── Axum handlers (thin wrappers over pure functions) ─────────────────────────

async fn handle_stats(State(s): State<TestnetState>) -> Json<Value> {
    Json(query_sync_stats(&s.sync_db, &s.utxo_db))
}

async fn handle_headers(
    State(s): State<TestnetState>,
    Query(p): Query<HeaderCursorParams>,
) -> impl IntoResponse {
    let limit = p.limit.min(100);
    match query_headers(&s.sync_db, limit, p.cursor) {
        Ok((headers, total)) => {
            let next_cursor = headers.last().and_then(|h| h["height"].as_u64());
            Json(json!({
                "headers":     headers,
                "total":       total,
                "limit":       limit,
                "next_cursor": next_cursor,
            })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        ).into_response(),
    }
}

async fn handle_header(
    State(s): State<TestnetState>,
    Path(height): Path<u64>,
) -> impl IntoResponse {
    match query_header(&s.sync_db, height) {
        Ok(Some(v)) => (StatusCode::OK, Json(v)).into_response(),
        Ok(None)    => (StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("header {} not found", height) }))).into_response(),
        Err(e)      => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e }))).into_response(),
    }
}

async fn handle_balance(
    State(s): State<TestnetState>,
    Path(script_hex): Path<String>,
) -> impl IntoResponse {
    match query_balance(&s.utxo_db, &script_hex) {
        Ok(bal) => Json(json!({ "script_hex": script_hex, "balance_sat": bal })).into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e }))).into_response(),
    }
}

async fn handle_utxos(
    State(s): State<TestnetState>,
    Path(script_hex): Path<String>,
    Query(p): Query<HeaderListParams>,
) -> impl IntoResponse {
    let limit = p.limit.min(100);
    match query_utxos(&s.utxo_db, &script_hex, limit) {
        Ok(utxos) => Json(json!({
            "script_hex": script_hex,
            "utxos":      utxos,
            "count":      utxos.len(),
        })).into_response(),
        Err(e)    => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e }))).into_response(),
    }
}

// ── Router factory ────────────────────────────────────────────────────────────

/// Build the /api/testnet/* sub-router.
pub fn testnet_router(state: TestnetState) -> Router {
    Router::new()
        .route("/api/testnet/stats",              get(handle_stats))
        .route("/api/testnet/headers",            get(handle_headers))
        .route("/api/testnet/header/:height",     get(handle_header))
        .route("/api/testnet/balance/:script_hex",get(handle_balance))
        .route("/api/testnet/utxos/:script_hex",  get(handle_utxos))
        .with_state(state)
}

// ── CLI integration ───────────────────────────────────────────────────────────

/// Print the explorer status for testnet sync data.
pub fn cmd_explorer_status() {
    let sync_path = {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home).join(".pkt").join("syncdb")
    };
    let utxo_path = {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home).join(".pkt").join("utxodb")
    };

    let sync_info = SyncDb::open_read_only(&sync_path).ok().and_then(|db| {
        let h   = db.get_sync_height().ok()??;
        let tip = db.get_tip_hash().ok()?
            .map(|h| hex::encode(&h[..8]))
            .unwrap_or_else(|| "?".into());
        Some(format!("height={} tip={}…", h, tip))
    });

    let utxo_info = UtxoSyncDb::open_read_only(&utxo_path).ok().and_then(|db| {
        let h   = db.get_utxo_height().ok()??;
        let cnt = db.count_utxos().ok()?;
        let val = db.total_value().ok()?;
        Some(format!("height={} utxos={} value={} sat", h, cnt, val))
    });

    println!();
    println!("  PKT Testnet Explorer Status");
    println!("  ───────────────────────────");
    match sync_info {
        Some(s) => println!("  Headers: {}", s),
        None    => println!("  Headers: (not synced — run: cargo run -- sync)"),
    }
    match utxo_info {
        Some(s) => println!("  UTXOs  : {}", s),
        None    => println!("  UTXOs  : (not synced)"),
    }
    println!();
    println!("  API routes available after `cargo run -- pktscan --testnet`:");
    println!("    GET /api/testnet/stats");
    println!("    GET /api/testnet/headers?limit=20&offset=0");
    println!("    GET /api/testnet/header/:height");
    println!("    GET /api/testnet/balance/:script_hex");
    println!("    GET /api/testnet/utxos/:script_hex");
    println!();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkt_sync::SyncDb;
    use crate::pkt_utxo_sync::{UtxoSyncDb, WireTxOut, wire_txid, apply_wire_tx, WireTx, WireTxIn};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_header(height: u64, prev: [u8; 32]) -> WireBlockHeader {
        WireBlockHeader {
            version:     1,
            prev_block:  prev,
            merkle_root: [height as u8; 32],
            timestamp:   1_700_000_000 + height as u32,
            bits:        0x207fffff,
            nonce:       height as u64,
        }
    }

    fn populate_sync_db(n: u64) -> SyncDb {
        let db   = SyncDb::open_temp().unwrap();
        let mut prev = [0u8; 32];
        for h in 1..=n {
            let hdr = make_header(h, prev);
            db.save_header(h, &hdr.to_bytes()).unwrap();
            prev = hdr.block_hash();
        }
        db.set_sync_height(n).unwrap();
        db.set_tip_hash(&prev).unwrap();
        db
    }

    fn populate_utxo_db(n: u8) -> UtxoSyncDb {
        let db = UtxoSyncDb::open_temp().unwrap();
        for i in 0..n {
            let script  = vec![0x76, 0xa9, i]; // fake P2PKH-like script
            let out     = WireTxOut { value: (i as u64 + 1) * 1000, script_pubkey: script };
            let fake_tx = WireTx {
                version:  1,
                inputs:   vec![WireTxIn {
                    prev_txid: [0u8; 32], prev_vout: 0xffff_ffff,
                    script_sig: vec![], sequence: 0xffff_ffff,
                }],
                outputs:  vec![out],
                locktime: 0,
            };
            let txid = wire_txid(&fake_tx);
            apply_wire_tx(&db, &fake_tx, &txid, 0).unwrap();
        }
        db.set_utxo_height(n as u64).unwrap();
        db
    }

    // ── format_header_json tests ──────────────────────────────────────────────

    #[test]
    fn test_format_header_has_height() {
        let hdr = make_header(42, [0u8; 32]);
        let v   = format_header_json(&hdr, 42);
        assert_eq!(v["height"].as_u64(), Some(42));
    }

    #[test]
    fn test_format_header_has_hash() {
        let hdr = make_header(1, [0u8; 32]);
        let v   = format_header_json(&hdr, 1);
        let hash_str = v["hash"].as_str().unwrap();
        assert_eq!(hash_str.len(), 64); // 32 bytes hex
    }

    #[test]
    fn test_format_header_hash_matches_block_hash() {
        let hdr  = make_header(5, [0xabu8; 32]);
        let v    = format_header_json(&hdr, 5);
        let expected = hex::encode(hdr.block_hash());
        assert_eq!(v["hash"].as_str().unwrap(), expected);
    }

    #[test]
    fn test_format_header_prev_hash() {
        let prev = [0xcdu8; 32];
        let hdr  = make_header(2, prev);
        let v    = format_header_json(&hdr, 2);
        assert_eq!(v["prev_hash"].as_str().unwrap(), hex::encode(prev));
    }

    #[test]
    fn test_format_header_timestamp() {
        let hdr = make_header(3, [0u8; 32]);
        let v   = format_header_json(&hdr, 3);
        assert_eq!(v["timestamp"].as_u64(), Some(1_700_000_003));
    }

    #[test]
    fn test_format_header_bits() {
        let hdr = make_header(1, [0u8; 32]);
        let v   = format_header_json(&hdr, 1);
        assert_eq!(v["bits"].as_u64(), Some(0x207fffff));
    }

    #[test]
    fn test_format_header_nonce() {
        let hdr = make_header(7, [0u8; 32]);
        let v   = format_header_json(&hdr, 7);
        assert_eq!(v["nonce"].as_u64(), Some(7));
    }

    // ── query_headers tests ───────────────────────────────────────────────────

    #[test]
    fn test_query_headers_empty_db() {
        let db          = SyncDb::open_temp().unwrap();
        let (hdrs, tip) = query_headers(&db, 10, None).unwrap();
        assert_eq!(hdrs.len(), 0);
        assert_eq!(tip, 0);
    }

    #[test]
    fn test_query_headers_returns_tip_height() {
        let db          = populate_sync_db(5);
        let (_, tip)    = query_headers(&db, 10, None).unwrap();
        assert_eq!(tip, 5);
    }

    #[test]
    fn test_query_headers_count_limited() {
        let db         = populate_sync_db(10);
        let (hdrs, _)  = query_headers(&db, 3, None).unwrap();
        assert_eq!(hdrs.len(), 3);
    }

    #[test]
    fn test_query_headers_newest_first() {
        let db        = populate_sync_db(5);
        let (hdrs, _) = query_headers(&db, 5, None).unwrap();
        // First entry should be height=5 (tip)
        assert_eq!(hdrs[0]["height"].as_u64(), Some(5));
        assert_eq!(hdrs[1]["height"].as_u64(), Some(4));
    }

    #[test]
    fn test_query_headers_with_cursor() {
        let db        = populate_sync_db(10);
        // cursor=9 → start from height 8 (exclusive: 9-1=8)
        let (hdrs, _) = query_headers(&db, 3, Some(9)).unwrap();
        assert_eq!(hdrs[0]["height"].as_u64(), Some(8));
    }

    #[test]
    fn test_query_headers_all_have_hashes() {
        let db        = populate_sync_db(5);
        let (hdrs, _) = query_headers(&db, 10, None).unwrap();
        for h in &hdrs {
            assert_eq!(h["hash"].as_str().unwrap().len(), 64);
        }
    }

    // ── query_header tests ────────────────────────────────────────────────────

    #[test]
    fn test_query_header_existing() {
        let db = populate_sync_db(5);
        let v  = query_header(&db, 3).unwrap();
        assert!(v.is_some());
        assert_eq!(v.unwrap()["height"].as_u64(), Some(3));
    }

    #[test]
    fn test_query_header_missing() {
        let db = populate_sync_db(5);
        let v  = query_header(&db, 999).unwrap();
        assert!(v.is_none());
    }

    #[test]
    fn test_query_header_height_1() {
        let db = populate_sync_db(3);
        let v  = query_header(&db, 1).unwrap().unwrap();
        assert_eq!(v["height"].as_u64(), Some(1));
    }

    #[test]
    fn test_query_header_hash_deterministic() {
        let db = populate_sync_db(3);
        let v1 = query_header(&db, 2).unwrap().unwrap();
        let v2 = query_header(&db, 2).unwrap().unwrap();
        assert_eq!(v1["hash"], v2["hash"]);
    }

    // ── format_utxo_json tests ────────────────────────────────────────────────

    #[test]
    fn test_format_utxo_has_txid() {
        let e = UtxoEntry {
            txid: "abcd".to_string(), vout: 0, value: 5000, script_pubkey: vec![0x51], height: 0,
        };
        let v = format_utxo_json(&e);
        assert_eq!(v["txid"].as_str(), Some("abcd"));
    }

    #[test]
    fn test_format_utxo_has_value() {
        let e = UtxoEntry {
            txid: "aa".to_string(), vout: 1, value: 99999, script_pubkey: vec![], height: 0,
        };
        let v = format_utxo_json(&e);
        assert_eq!(v["value"].as_u64(), Some(99999));
    }

    #[test]
    fn test_format_utxo_script_hex_encoded() {
        let e = UtxoEntry {
            txid: "bb".to_string(), vout: 0, value: 0, script_pubkey: vec![0xde, 0xad], height: 0,
        };
        let v = format_utxo_json(&e);
        assert_eq!(v["script_pubkey"].as_str(), Some("dead"));
    }

    // ── query_utxos tests ─────────────────────────────────────────────────────

    #[test]
    fn test_query_utxos_empty_db() {
        let db    = UtxoSyncDb::open_temp().unwrap();
        let utxos = query_utxos(&db, "", 100).unwrap();
        assert_eq!(utxos.len(), 0);
    }

    #[test]
    fn test_query_utxos_all_no_filter() {
        let db    = populate_utxo_db(4);
        let utxos = query_utxos(&db, "", 100).unwrap();
        assert_eq!(utxos.len(), 4);
    }

    #[test]
    fn test_query_utxos_limit_respected() {
        let db    = populate_utxo_db(10);
        let utxos = query_utxos(&db, "", 3).unwrap();
        assert_eq!(utxos.len(), 3);
    }

    #[test]
    fn test_query_utxos_script_filter() {
        let db = UtxoSyncDb::open_temp().unwrap();
        // Insert two UTXOs with different scripts
        let scripts: &[&[u8]] = &[&[0x51, 0xaa], &[0x52, 0xbb]];
        for (i, &script) in scripts.iter().enumerate() {
            let out = WireTxOut { value: 1000, script_pubkey: script.to_vec() };
            let tx  = WireTx {
                version: 1,
                inputs: vec![WireTxIn {
                    prev_txid: [0u8;32], prev_vout: 0xffff_ffff,
                    script_sig: vec![], sequence: 0xffff_ffff,
                }],
                outputs: vec![out],
                locktime: i as u32,
            };
            let txid = wire_txid(&tx);
            apply_wire_tx(&db, &tx, &txid, 0).unwrap();
        }
        // Filter by 0x51 prefix
        let result = query_utxos(&db, "51", 100).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["script_pubkey"].as_str().unwrap(), "51aa");
    }

    #[test]
    fn test_query_utxos_no_match_filter() {
        let db    = populate_utxo_db(3);
        let utxos = query_utxos(&db, "ffff", 100).unwrap();
        assert_eq!(utxos.len(), 0);
    }

    // ── query_balance tests ───────────────────────────────────────────────────

    #[test]
    fn test_query_balance_zero_empty() {
        let db  = UtxoSyncDb::open_temp().unwrap();
        let bal = query_balance(&db, "").unwrap();
        assert_eq!(bal, 0);
    }

    #[test]
    fn test_query_balance_sums_all() {
        let db  = populate_utxo_db(3); // values: 1000, 2000, 3000
        let bal = query_balance(&db, "").unwrap();
        assert_eq!(bal, 6000);
    }

    #[test]
    fn test_query_balance_filtered_by_script() {
        let db = UtxoSyncDb::open_temp().unwrap();
        // Script 0x76 → 1000 sat; script 0x51 → 5000 sat
        for (script, value) in [(&[0x76u8][..], 1000u64), (&[0x51u8][..], 5000u64)] {
            let out = WireTxOut { value, script_pubkey: script.to_vec() };
            let tx  = WireTx {
                version: 1,
                inputs: vec![WireTxIn {
                    prev_txid: [0u8;32], prev_vout: 0xffff_ffff,
                    script_sig: vec![], sequence: 0xffff_ffff,
                }],
                outputs: vec![out],
                locktime: value as u32,
            };
            let txid = wire_txid(&tx);
            apply_wire_tx(&db, &tx, &txid, 0).unwrap();
        }
        let bal = query_balance(&db, "76").unwrap();
        assert_eq!(bal, 1000);
    }

    // ── query_sync_stats tests ────────────────────────────────────────────────

    #[test]
    fn test_query_sync_stats_zero_when_empty() {
        let sdb = SyncDb::open_temp().unwrap();
        let udb = UtxoSyncDb::open_temp().unwrap();
        let v   = query_sync_stats(&sdb, &udb);
        assert_eq!(v["sync_height"].as_u64(), Some(0));
        assert_eq!(v["utxo_count"].as_u64(), Some(0));
    }

    #[test]
    fn test_query_sync_stats_network_testnet() {
        let sdb = SyncDb::open_temp().unwrap();
        let udb = UtxoSyncDb::open_temp().unwrap();
        let v   = query_sync_stats(&sdb, &udb);
        assert_eq!(v["network"].as_str(), Some("testnet"));
    }

    #[test]
    fn test_query_sync_stats_synced_flag_false() {
        let sdb = SyncDb::open_temp().unwrap();
        let udb = UtxoSyncDb::open_temp().unwrap();
        let v   = query_sync_stats(&sdb, &udb);
        assert_eq!(v["synced"].as_bool(), Some(false));
    }

    #[test]
    fn test_query_sync_stats_synced_flag_true_when_data() {
        let sdb = populate_sync_db(3);
        let udb = UtxoSyncDb::open_temp().unwrap();
        let v   = query_sync_stats(&sdb, &udb);
        assert_eq!(v["synced"].as_bool(), Some(true));
    }

    #[test]
    fn test_query_sync_stats_height_matches_db() {
        let sdb = populate_sync_db(7);
        let udb = UtxoSyncDb::open_temp().unwrap();
        let v   = query_sync_stats(&sdb, &udb);
        assert_eq!(v["sync_height"].as_u64(), Some(7));
    }

    #[test]
    fn test_query_sync_stats_utxo_count() {
        let sdb = SyncDb::open_temp().unwrap();
        let udb = populate_utxo_db(5);
        let v   = query_sync_stats(&sdb, &udb);
        assert_eq!(v["utxo_count"].as_u64(), Some(5));
    }

    #[test]
    fn test_query_sync_stats_tip_hash_length() {
        let sdb = populate_sync_db(2);
        let udb = UtxoSyncDb::open_temp().unwrap();
        let v   = query_sync_stats(&sdb, &udb);
        let tip = v["tip_hash"].as_str().unwrap();
        assert_eq!(tip.len(), 64); // 32-byte hash as hex
    }

    #[test]
    fn test_query_sync_stats_total_value() {
        let sdb = SyncDb::open_temp().unwrap();
        let udb = populate_utxo_db(3); // 1000 + 2000 + 3000
        let v   = query_sync_stats(&sdb, &udb);
        assert_eq!(v["total_value_sat"].as_u64(), Some(6000));
    }
}
