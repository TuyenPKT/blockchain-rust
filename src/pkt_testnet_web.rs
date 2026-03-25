#![allow(dead_code)]
//! v15.6 — Testnet Web Integration
//!
//! Wires the PKT testnet data API into `pktscan_api::serve()` and serves
//! `/static/testnet.js` (the frontend panel that shows real sync progress).
//!
//! Routes added (via `testnet_web_router()`):
//!   GET /static/testnet.js              → embedded JS
//!   GET /api/testnet/stats              → sync stats JSON
//!   GET /api/testnet/headers            → recent wire headers (paginated)
//!   GET /api/testnet/header/:h          → single header by height
//!   GET /api/testnet/balance/:s         → balance for script_pubkey prefix
//!   GET /api/testnet/utxos/:s           → UTXOs for script_pubkey prefix
//!   GET /api/testnet/sync-status        → sync progress JSON (for progress bar)
//!
//! Default DB paths: ~/.pkt/syncdb (headers) and ~/.pkt/utxodb (UTXOs).
//! Graceful degradation: if a DB cannot be opened at request time, the handler
//! returns 503 (frontend shows "Offline") but all routes are always registered.

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::pkt_addr_index::AddrIndexDb;
use crate::pkt_explorer_api::{testnet_router, HeaderListParams, TestnetState};
use crate::pkt_labels::LabelDb;
use crate::pkt_mempool_sync::MempoolDb;
use crate::pkt_sync::SyncDb;
use crate::pkt_sync_ui::{sync_status_router, SyncProgress, SyncUiState};
use crate::pkt_utxo_sync::UtxoSyncDb;

// ── Script helpers ─────────────────────────────────────────────────────────────

/// Decode a JSON-hex script (custom format used by this chain) to Base58Check P2PKH address.
/// Script JSON: {"ops":["OpDup","OpHash160",{"OpPushData":[b0,b1,...,b19]},"OpEqualVerify","OpCheckSig"]}
fn script_hex_to_address(script_hex: &str) -> Option<String> {
    let bytes = hex::decode(script_hex).ok()?;
    let json: Value = serde_json::from_slice(&bytes).ok()?;
    let ops = json["ops"].as_array()?;
    let hash160: Vec<u8> = ops.iter().find_map(|op| {
        op["OpPushData"].as_array().map(|arr| {
            arr.iter().filter_map(|b| b.as_u64().map(|x| x as u8)).collect()
        })
    })?;
    if hash160.len() != 20 { return None; }
    let mut payload = vec![0x00u8]; // version byte: mainnet P2PKH
    payload.extend_from_slice(&hash160);
    // wallet.rs uses BLAKE3 double-hash for checksum (not SHA256)
    let checksum = blake3::hash(blake3::hash(&payload).as_bytes());
    payload.extend_from_slice(&checksum.as_bytes()[..4]);
    Some(bs58::encode(payload).into_string())
}

/// Convert Base58Check P2PKH address → JSON-hex script key used by address index.
fn address_to_script_hex(addr: &str) -> Option<String> {
    let decoded = bs58::decode(addr).into_vec().ok()?;
    if decoded.len() != 25 { return None; }
    let (payload, checksum) = decoded.split_at(21);
    let expected = blake3::hash(blake3::hash(payload).as_bytes());
    if &expected.as_bytes()[..4] != checksum { return None; }
    let hash160: Vec<u8> = payload[1..21].to_vec();
    let script = json!({
        "ops": ["OpDup", "OpHash160", {"OpPushData": hash160}, "OpEqualVerify", "OpCheckSig"]
    });
    let script_bytes = serde_json::to_vec(&script).ok()?;
    Some(hex::encode(script_bytes))
}

// ── Per-request DB open state ──────────────────────────────────────────────────

/// Chỉ lưu paths, mở DB fresh mỗi request → không giữ lock, luôn thấy data mới.
#[derive(Clone)]
struct PathState {
    sync_path:    PathBuf,
    utxo_path:    PathBuf,
    addr_path:    PathBuf,
    mempool_path: PathBuf,
    label_path:   PathBuf,
}

impl PathState {
    fn open(&self) -> Option<(SyncDb, UtxoSyncDb)> {
        let sdb = SyncDb::open_read_only(&self.sync_path).ok()?;
        let udb = UtxoSyncDb::open_read_only(&self.utxo_path).ok()?;
        Some((sdb, udb))
    }

    fn open_addr(&self) -> Option<AddrIndexDb> {
        AddrIndexDb::open_read_only(&self.addr_path).ok()
    }

    fn open_mempool(&self) -> Option<MempoolDb> {
        MempoolDb::open_read_only(&self.mempool_path).ok()
    }

    fn open_label(&self) -> Option<LabelDb> {
        LabelDb::open_read_only(&self.label_path).ok()
    }
}

async fn ps_sync_status(State(ps): State<PathState>) -> impl IntoResponse {
    match ps.open() {
        None => (StatusCode::SERVICE_UNAVAILABLE,
                 Json(json!({"phase":"not_synced","overall_progress":0}))).into_response(),
        Some((sdb, udb)) => {
            let p = SyncProgress::from_dbs(&sdb, &udb);
            Json(crate::pkt_sync_ui::sync_status_json(&p)).into_response()
        }
    }
}

async fn ps_stats(State(ps): State<PathState>) -> impl IntoResponse {
    match ps.open() {
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"not synced"}))).into_response(),
        Some((sdb, udb)) => {
            let state = TestnetState::new(sdb, udb);
            let stats = crate::pkt_explorer_api::query_sync_stats(&state.sync_db, &state.utxo_db);
            Json(stats).into_response()
        }
    }
}

async fn ps_headers(
    State(ps): State<PathState>,
    Query(params): Query<HeaderListParams>,
) -> impl IntoResponse {
    match ps.open() {
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"not synced"}))).into_response(),
        Some((sdb, _udb)) => {
            match crate::pkt_explorer_api::query_headers(
                &sdb, params.limit.min(100), params.offset,
            ) {
                Ok((headers, tip)) => Json(json!({"headers": headers, "tip": tip})).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
            }
        }
    }
}

async fn ps_header(
    State(ps): State<PathState>,
    Path(height): Path<u64>,
) -> impl IntoResponse {
    match ps.open() {
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"not synced"}))).into_response(),
        Some((sdb, _udb)) => match crate::pkt_explorer_api::query_header(&sdb, height) {
            Ok(Some(v)) => Json(v).into_response(),
            Ok(None)    => (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))).into_response(),
            Err(e)      => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
        }
    }
}

async fn ps_balance(
    State(ps): State<PathState>,
    Path(script): Path<String>,
) -> impl IntoResponse {
    match ps.open_addr() {
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"not synced"}))).into_response(),
        Some(adb) => {
            let balance = adb.get_balance(&script).unwrap_or(0);
            let address = script_hex_to_address(&script);
            Json(json!({"balance": balance, "address": address})).into_response()
        }
    }
}

async fn ps_utxos(
    State(ps): State<PathState>,
    Path(script): Path<String>,
) -> impl IntoResponse {
    match ps.open() {
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"not synced"}))).into_response(),
        Some((_sdb, udb)) => {
            let utxos = crate::pkt_explorer_api::query_utxos(&udb, &script, 100);
            Json(json!({"utxos": utxos})).into_response()
        }
    }
}

/// Embedded testnet panel JS — served at /static/testnet.js.
const TESTNET_JS: &str = include_str!("../web/js/testnet.js");

// ── Address index handlers ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AddrTxsParams {
    cursor: Option<u64>,
    limit:  Option<usize>,
}

/// GET /api/testnet/address/:script/txs?cursor=HEIGHT&limit=N
async fn ps_addr_txs(
    State(ps):        State<PathState>,
    Path(script):     Path<String>,
    Query(params):    Query<AddrTxsParams>,
) -> impl IntoResponse {
    let adb = match ps.open_addr() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           Json(json!({"error":"address index not ready"}))).into_response(),
        Some(d) => d,
    };
    let limit = params.limit.unwrap_or(50).min(200);
    match adb.get_tx_history(&script, params.cursor, limit) {
        Ok(entries) => {
            let txs: Vec<_> = entries.iter().map(|e| json!({
                "height": e.height,
                "txid":   e.txid,
            })).collect();
            let count = txs.len();
            Json(json!({"address": script, "txs": txs, "count": count})).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
                   Json(json!({"error": e.to_string()}))).into_response(),
    }
}

/// GET /api/testnet/addr/:base58?limit=N
/// Accepts human-readable Base58Check address, returns balance + tx history.
async fn ps_addr_by_base58(
    State(ps):     State<PathState>,
    Path(addr):    Path<String>,
    Query(params): Query<AddrTxsParams>,
) -> impl IntoResponse {
    let script = match address_to_script_hex(&addr) {
        Some(s) => s,
        None    => return (StatusCode::BAD_REQUEST,
                           Json(json!({"error":"invalid address"}))).into_response(),
    };
    let adb = match ps.open_addr() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           Json(json!({"error":"address index not ready"}))).into_response(),
        Some(d) => d,
    };
    let balance = adb.get_balance(&script).unwrap_or(0);
    let limit   = params.limit.unwrap_or(50).min(200);
    let txs: Vec<_> = adb.get_tx_history(&script, params.cursor, limit)
        .unwrap_or_default()
        .iter()
        .map(|e| json!({"height": e.height, "txid": e.txid}))
        .collect();
    let count = txs.len();
    Json(json!({"address": addr, "balance": balance, "txs": txs, "count": count})).into_response()
}

// ── Analytics handler ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AnalyticsParams {
    metric: Option<String>,
    window: Option<usize>,
}

/// GET /api/testnet/analytics?metric=hashrate|block_time|difficulty&window=N
async fn ps_analytics(
    State(ps):     State<PathState>,
    Query(params): Query<AnalyticsParams>,
) -> impl IntoResponse {
    let sdb = match SyncDb::open_read_only(&ps.sync_path).ok() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           Json(json!({"error":"sync db not ready"}))).into_response(),
        Some(d) => d,
    };
    let metric = params.metric.as_deref().unwrap_or("hashrate");
    let window = params.window.unwrap_or(100);
    match crate::pkt_analytics::analytics(metric, &sdb, window) {
        Ok(series) => Json(serde_json::to_value(&series).unwrap_or_default()).into_response(),
        Err(e)     => (StatusCode::BAD_REQUEST,
                       Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct RichListParams {
    limit: Option<usize>,
}

/// GET /api/testnet/rich-list?limit=N
async fn ps_rich_list(
    State(ps):     State<PathState>,
    Query(params): Query<RichListParams>,
) -> impl IntoResponse {
    let adb = match ps.open_addr() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           Json(json!({"error":"address index not ready"}))).into_response(),
        Some(d) => d,
    };
    let ldb = ps.open_label();
    let limit = params.limit.unwrap_or(100).min(1000);
    match adb.get_rich_list(limit) {
        Ok(list) => {
            // PAKLETS_PER_PKT = 2^30 = 1_073_741_824
            let holders: Vec<_> = list.iter().map(|(script, bal)| {
                let address = script_hex_to_address(script);
                let label = ldb.as_ref().and_then(|db| {
                    db.get_label_for(script, address.as_deref())
                });
                json!({
                    "script":      script,
                    "address":     address,
                    "balance":     bal,
                    "balance_pkt": (*bal as f64) / 1_073_741_824.0,
                    "label":       label,
                })
            }).collect();
            let count = holders.len();
            Json(json!({"holders": holders, "count": count})).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
                   Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Mempool handlers ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct MempoolParams {
    limit: Option<usize>,
}

/// GET /api/testnet/mempool?limit=N
///
/// Returns pending transactions sorted by fee rate (highest first).
async fn ps_mempool(
    State(ps):     State<PathState>,
    Query(params): Query<MempoolParams>,
) -> impl IntoResponse {
    let mdb = match ps.open_mempool() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           Json(json!({"error":"mempool not ready"}))).into_response(),
        Some(d) => d,
    };
    let limit = params.limit.unwrap_or(50).min(500);
    match mdb.get_pending(limit) {
        Ok(txs) => {
            let count = txs.len();
            Json(json!({"txs": txs, "count": count})).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
                   Json(json!({"error": e.to_string()}))).into_response(),
    }
}

/// GET /api/testnet/mempool/fee-histogram
///
/// Returns fee rate distribution as [{lower_msat_vb, count}].
async fn ps_mempool_histogram(State(ps): State<PathState>) -> impl IntoResponse {
    let mdb = match ps.open_mempool() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           Json(json!({"error":"mempool not ready"}))).into_response(),
        Some(d) => d,
    };
    match mdb.fee_rate_histogram() {
        Ok(hist) => {
            let buckets: Vec<_> = hist.iter()
                .map(|(lower, count)| json!({"lower_msat_vb": lower, "count": count}))
                .collect();
            Json(json!({"buckets": buckets})).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
                   Json(json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Search handler ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SearchParams {
    q: Option<String>,
}

/// GET /api/testnet/search?q=<query>
/// Detect type và trả unified results.
async fn ps_search(
    State(ps):     State<PathState>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    use crate::pkt_search::{detect_kind, search_labels, QueryKind};
    use crate::pkt_explorer_api::format_header_json;
    use crate::pkt_wire::WireBlockHeader;

    let q = match params.q.as_deref() {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return Json(json!({"query": "", "results": []})).into_response(),
    };

    let mut results: Vec<Value> = Vec::new();

    match detect_kind(&q) {
        QueryKind::Height(h) => {
            if let Some(sdb) = SyncDb::open_read_only(&ps.sync_path).ok() {
                if let Ok(Some(raw)) = sdb.load_header(h) {
                    if let Ok(hdr) = WireBlockHeader::from_bytes(&raw) {
                        let data = format_header_json(&hdr, h);
                        results.push(json!({
                            "type":  "block",
                            "label": format!("Block #{}", h),
                            "value": h.to_string(),
                            "meta":  data,
                        }));
                    }
                }
            }
        }

        QueryKind::Txid(txid) => {
            let in_mempool = ps.open_mempool()
                .map(|mdb| mdb.has_tx(&txid))
                .unwrap_or(false);
            results.push(json!({
                "type":  "tx",
                "label": if in_mempool { "Transaction (mempool)" } else { "Transaction" },
                "value": txid,
                "meta":  { "in_mempool": in_mempool },
            }));
        }

        QueryKind::Address(addr) => {
            let script    = address_to_script_hex(&addr);
            let balance   = ps.open_addr()
                .and_then(|adb| script.as_deref().map(|s| adb.get_balance(s).unwrap_or(0)))
                .unwrap_or(0);
            let label     = ps.open_label()
                .and_then(|ldb| ldb.get_label_for(
                    script.as_deref().unwrap_or(""),
                    Some(&addr),
                ));
            results.push(json!({
                "type":  "address",
                "label": label.as_ref().map(|l| l.label.as_str()).unwrap_or("Address"),
                "value": addr,
                "meta":  {
                    "balance":     balance,
                    "balance_pkt": (balance as f64) / 1_073_741_824.0,
                    "label":       label,
                },
            }));
        }

        QueryKind::Label(text) => {
            let ldb = ps.open_label();
            let hits = search_labels(&text, ldb.as_ref());
            for (key, lbl, cat, verified) in hits {
                results.push(json!({
                    "type":  "label",
                    "label": lbl,
                    "value": key,
                    "meta":  { "category": cat, "verified": verified },
                }));
            }
        }

        QueryKind::Unknown => {}
    }

    Json(json!({"query": q, "results": results})).into_response()
}

// ── Label handler ──────────────────────────────────────────────────────────────

/// GET /api/testnet/label/:script
/// :script có thể là script_hex hoặc Base58Check address.
async fn ps_label(
    State(ps):    State<PathState>,
    Path(script): Path<String>,
) -> impl IntoResponse {
    // Thử preset trước (không cần DB)
    let preset = crate::pkt_labels::preset_by_address(&script);
    if let Some(e) = preset {
        return Json(json!({
            "key":      script,
            "label":    e.label,
            "category": e.category,
            "verified": e.verified,
            "source":   "preset",
        })).into_response();
    }
    // Thử DB (graceful: nếu DB chưa có thì 404)
    match ps.open_label() {
        None => (StatusCode::NOT_FOUND, Json(json!({"error":"label not found"}))).into_response(),
        Some(ldb) => {
            let entry = ldb.get_label(&script)
                .or_else(|| ldb.get_label_by_address(&script));
            match entry {
                Some(e) => Json(json!({
                    "key":      script,
                    "label":    e.label,
                    "category": e.category,
                    "verified": e.verified,
                    "source":   "db",
                })).into_response(),
                None => (StatusCode::NOT_FOUND,
                         Json(json!({"error":"label not found"}))).into_response(),
            }
        }
    }
}

// ── Path helpers ───────────────────────────────────────────────────────────────

/// Build a path under $HOME (falls back to "." if HOME/USERPROFILE unset).
pub fn home_path(suffix: &str) -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(suffix)
}

/// Default SyncDb path: `~/.pkt/syncdb` (matches `SyncConfig::testnet()` default).
pub fn default_sync_db_path() -> PathBuf {
    home_path(".pkt/syncdb")
}

///// Default UtxoSyncDb path: `~/.pkt/utxodb`.
pub fn default_utxo_db_path() -> PathBuf {
    home_path(".pkt/utxodb")
}

// ── Static JS handler ──────────────────────────────────────────────────────────

async fn serve_testnet_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        TESTNET_JS,
    )
}

// ── Router builders ────────────────────────────────────────────────────────────

/// Build the combined testnet router using caller-supplied DB handles.
/// Useful for tests and for custom DB paths.
pub fn testnet_web_router_with_dbs(sync_db: Arc<SyncDb>, utxo_db: Arc<UtxoSyncDb>) -> Router {
    let t_state = TestnetState {
        sync_db: Arc::clone(&sync_db),
        utxo_db: Arc::clone(&utxo_db),
    };
    let u_state = SyncUiState {
        sync_db: Arc::clone(&sync_db),
        utxo_db,
    };
    Router::new()
        .route("/static/testnet.js", get(serve_testnet_js))
        .merge(testnet_router(t_state))
        .merge(sync_status_router(u_state))
}

/// Build the combined testnet router using default DB paths.
///
/// Routes are always registered. If the DB is unavailable at request time,
/// handlers return 503 so the frontend shows "Offline" gracefully.
pub fn testnet_web_router() -> Router {
    let ps = PathState {
        sync_path:    default_sync_db_path(),
        utxo_path:    default_utxo_db_path(),
        addr_path:    crate::pkt_addr_index::default_addr_db_path(),
        mempool_path: crate::pkt_mempool_sync::default_mempool_db_path(),
        label_path:   crate::pkt_labels::default_label_db_path(),
    };
    Router::new()
        .route("/static/testnet.js", get(serve_testnet_js))
        .route("/api/testnet/sync-status", get(ps_sync_status))
        .route("/api/testnet/stats", get(ps_stats))
        .route("/api/testnet/headers", get(ps_headers))
        .route("/api/testnet/header/:h", get(ps_header))
        .route("/api/testnet/balance/:s", get(ps_balance))
        .route("/api/testnet/utxos/:s", get(ps_utxos))
        .route("/api/testnet/address/:s/txs", get(ps_addr_txs))
        .route("/api/testnet/addr/:base58", get(ps_addr_by_base58))
        .route("/api/testnet/rich-list", get(ps_rich_list))
        .route("/api/testnet/mempool", get(ps_mempool))
        .route("/api/testnet/mempool/fee-histogram", get(ps_mempool_histogram))
        .route("/api/testnet/analytics",             get(ps_analytics))
        .route("/api/testnet/search",                get(ps_search))
        .route("/api/testnet/label/:script",         get(ps_label))
        .with_state(ps)
}

// ── CLI ────────────────────────────────────────────────────────────────────────

/// Print testnet DB status to stdout (used by `cargo run -- testnet-web`).
pub fn cmd_testnet_web() {
    let sdb_path = default_sync_db_path();
    let udb_path = default_utxo_db_path();
    println!("PKT Testnet Web — v15.6");
    println!("  SyncDB : {}", sdb_path.display());
    println!("  UtxoDB : {}", udb_path.display());
    match SyncDb::open(&sdb_path) {
        Err(e) => println!("  SyncDB : unavailable — {}", e),
        Ok(sdb) => match UtxoSyncDb::open(&udb_path) {
            Err(e) => println!("  UtxoDB : unavailable — {}", e),
            Ok(udb) => {
                let p = crate::pkt_sync_ui::SyncProgress::from_dbs(&sdb, &udb);
                println!("  Status : {}", crate::pkt_sync_ui::format_sync_oneline(&p));
            }
        },
    }
    println!(
        "  Routes : GET /api/testnet/{{stats,headers,header/:h,balance/:s,utxos/:s,sync-status}}"
    );
    println!("  JS     : GET /static/testnet.js");
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── JS content tests ─────────────────────────────────────────────────────

    #[test]
    fn test_testnet_js_not_empty() {
        assert!(!TESTNET_JS.is_empty());
    }

    #[test]
    fn test_testnet_js_has_fetch_sync_status() {
        assert!(TESTNET_JS.contains("fetchSyncStatus"));
    }

    #[test]
    fn test_testnet_js_has_fetch_stats() {
        assert!(TESTNET_JS.contains("fetchTestnetStats"));
    }

    #[test]
    fn test_testnet_js_has_fetch_headers() {
        assert!(TESTNET_JS.contains("fetchTestnetHeaders"));
    }

    #[test]
    fn test_testnet_js_has_show_testnet() {
        assert!(TESTNET_JS.contains("showTestnet"));
    }

    #[test]
    fn test_testnet_js_has_refresh_testnet() {
        assert!(TESTNET_JS.contains("refreshTestnet"));
    }

    #[test]
    fn test_testnet_js_has_render_progress_bar() {
        assert!(TESTNET_JS.contains("renderProgressBar"));
    }

    #[test]
    fn test_testnet_js_hits_sync_status_endpoint() {
        assert!(TESTNET_JS.contains("api/testnet/sync-status"));
    }

    #[test]
    fn test_testnet_js_hits_stats_endpoint() {
        assert!(TESTNET_JS.contains("api/testnet/stats"));
    }

    #[test]
    fn test_testnet_js_hits_headers_endpoint() {
        assert!(TESTNET_JS.contains("api/testnet/headers"));
    }

    #[test]
    fn test_testnet_js_is_iife() {
        assert!(
            TESTNET_JS.contains("(function () {") || TESTNET_JS.contains("(function() {"),
            "JS must be wrapped in IIFE for scope isolation"
        );
    }

    #[test]
    fn test_testnet_js_exports_show_testnet_on_window() {
        assert!(TESTNET_JS.contains("window.showTestnet"));
    }

    #[test]
    fn test_testnet_js_has_autorefresh_interval() {
        assert!(
            TESTNET_JS.contains("setInterval"),
            "JS must auto-refresh via setInterval"
        );
    }

    #[test]
    fn test_testnet_js_references_testnet_page_dom_ids() {
        assert!(TESTNET_JS.contains("tn-sync-phase"));
        assert!(TESTNET_JS.contains("tn-headers-list"));
        assert!(TESTNET_JS.contains("tn-stat-height"));
    }

    // ── Path helper tests ─────────────────────────────────────────────────────

    #[test]
    fn test_default_sync_db_path_ends_with_syncdb() {
        let p = default_sync_db_path();
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), "syncdb");
    }

    #[test]
    fn test_default_utxo_db_path_ends_with_utxodb() {
        let p = default_utxo_db_path();
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), "utxodb");
    }

    #[test]
    fn test_sync_db_path_has_pkt_component() {
        let p = default_sync_db_path();
        let has_pkt = p.components().any(|c| c.as_os_str() == ".pkt");
        assert!(has_pkt, "syncdb path must be inside .pkt/");
    }

    #[test]
    fn test_utxo_db_path_has_pkt_component() {
        let p = default_utxo_db_path();
        let has_pkt = p.components().any(|c| c.as_os_str() == ".pkt");
        assert!(has_pkt, "utxodb path must be inside .pkt/");
    }

    #[test]
    fn test_home_path_custom_suffix() {
        let p = home_path("foo/bar/baz");
        assert!(p.ends_with("foo/bar/baz"));
    }

    #[test]
    fn test_sync_and_utxo_paths_differ() {
        assert_ne!(default_sync_db_path(), default_utxo_db_path());
    }

    // ── Router construction tests ─────────────────────────────────────────────
    // Serialize to avoid open_temp() collision (same issue as pkt_sync_ui tests).

    static ROUTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_router_with_empty_dbs_no_panic() {
        let _g  = ROUTER_LOCK.lock().unwrap();
        let sdb = Arc::new(crate::pkt_sync::SyncDb::open_temp().unwrap());
        let udb = Arc::new(crate::pkt_utxo_sync::UtxoSyncDb::open_temp().unwrap());
        let _r  = testnet_web_router_with_dbs(sdb, udb);
    }

    #[test]
    fn test_router_with_populated_dbs_no_panic() {
        let _g  = ROUTER_LOCK.lock().unwrap();
        let sdb = Arc::new(crate::pkt_sync::SyncDb::open_temp().unwrap());
        let udb = Arc::new(crate::pkt_utxo_sync::UtxoSyncDb::open_temp().unwrap());
        sdb.set_sync_height(500).unwrap();
        udb.set_utxo_height(400).unwrap();
        let _r  = testnet_web_router_with_dbs(sdb, udb);
    }

    #[test]
    fn test_router_two_separate_db_pairs_no_panic() {
        let _g   = ROUTER_LOCK.lock().unwrap();
        let sdb1 = Arc::new(crate::pkt_sync::SyncDb::open_temp().unwrap());
        let udb1 = Arc::new(crate::pkt_utxo_sync::UtxoSyncDb::open_temp().unwrap());
        let _r1  = testnet_web_router_with_dbs(sdb1, udb1);
        let sdb2 = Arc::new(crate::pkt_sync::SyncDb::open_temp().unwrap());
        let udb2 = Arc::new(crate::pkt_utxo_sync::UtxoSyncDb::open_temp().unwrap());
        let _r2  = testnet_web_router_with_dbs(sdb2, udb2);
    }
}
