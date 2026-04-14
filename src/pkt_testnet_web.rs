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
use std::process::Child;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::pkt_addr_index::AddrIndexDb;
use crate::pkt_explorer_api::{testnet_router, HeaderCursorParams, TestnetState};
use crate::pkt_labels::LabelDb;
use crate::pkt_mempool_sync::MempoolDb;
use crate::pkt_sync::SyncDb;
use crate::pkt_sync_ui::{sync_status_router, SyncProgress, SyncUiState};
use crate::pkt_utxo_sync::UtxoSyncDb;

// ── Sync process control ──────────────────────────────────────────────────────

/// Global handle to the running sync child process (nếu có).
static SYNC_CHILD: Mutex<Option<Child>> = Mutex::new(None);

/// `POST /api/testnet/sync/start?peer=host:port`
/// Spawn `{current_exe} sync [peer]` nếu chưa có process đang chạy.
async fn ps_sync_start(
    Query(params): Query<SyncStartParams>,
) -> impl IntoResponse {
    let peer = params.peer.unwrap_or_else(|| "seed.testnet.oceif.com:8333".to_string());
    // Validate peer format: hostname:port — chặn ký tự đặc biệt
    {
        static PEER_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let re = PEER_RE.get_or_init(|| {
            regex::Regex::new(r"^[a-zA-Z0-9.\-]{1,253}:\d{1,5}$").unwrap()
        });
        if !re.is_match(&peer) {
            return (StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid peer format, expected host:port"}))).into_response();
        }
        // Validate port range
        if let Some(port_str) = peer.split(':').last() {
            let port: u32 = port_str.parse().unwrap_or(0);
            if port == 0 || port > 65535 {
                return (StatusCode::BAD_REQUEST,
                    Json(json!({"error": "port out of range (1-65535)"}))).into_response();
            }
        }
    }
    let mut guard = match SYNC_CHILD.lock() {
        Ok(g) => g,
        Err(_) => return Json(json!({"error": "lock poisoned"})).into_response(),
    };
    // Kiểm tra process cũ đã thoát chưa
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(None) => return Json(json!({"error": "sync already running", "pid": child.id()})).into_response(),
            _ => { *guard = None; } // đã thoát → clear
        }
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return Json(json!({"error": format!("cannot get exe path: {}", e)})).into_response(),
    };
    // Kiểm tra binary tồn tại — nếu không (binary đã rebuild/replace sau khi service start)
    // trả lỗi rõ ràng thay vì "No such file or directory"
    if !exe.exists() {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({
            "error": "binary not found at runtime path — sync is likely managed by fullnode service",
            "exe": exe.display().to_string(),
            "hint": "use `systemctl status fullnode` to check sync status"
        }))).into_response();
    }
    match std::process::Command::new(&exe).args(["sync", &peer]).spawn() {
        Ok(child) => {
            let pid = child.id();
            *guard = Some(child);
            Json(json!({"started": true, "pid": pid, "peer": peer})).into_response()
        }
        Err(e) => Json(json!({"error": format!("spawn failed: {}", e)})).into_response(),
    }
}

/// `POST /api/testnet/sync/stop`
/// Kill sync child process nếu đang chạy.
async fn ps_sync_stop() -> impl IntoResponse {
    let mut guard = match SYNC_CHILD.lock() {
        Ok(g) => g,
        Err(_) => return Json(json!({"error": "lock poisoned"})).into_response(),
    };
    match guard.as_mut() {
        None => Json(json!({"stopped": false, "reason": "not running"})).into_response(),
        Some(child) => {
            let pid = child.id();
            match child.kill() {
                Ok(_) => {
                    let _ = child.wait();
                    *guard = None;
                    Json(json!({"stopped": true, "pid": pid})).into_response()
                }
                Err(e) => Json(json!({"error": format!("kill failed: {}", e)})).into_response(),
            }
        }
    }
}

/// `GET /api/testnet/sync/status`
/// Trả về trạng thái process sync (running hay không).
async fn ps_sync_proc_status() -> impl IntoResponse {
    let mut guard = match SYNC_CHILD.lock() {
        Ok(g) => g,
        Err(_) => return Json(json!({"running": false})).into_response(),
    };
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(None) => return Json(json!({"running": true, "pid": child.id()})).into_response(),
            _ => { *guard = None; }
        }
    }
    Json(json!({"running": false})).into_response()
}

#[derive(Deserialize)]
struct SyncStartParams {
    peer: Option<String>,
}

// ── Script helpers ─────────────────────────────────────────────────────────────

/// Decode a JSON-hex script (custom format used by this chain) to EVM address.
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
    let raw: [u8; 20] = hash160.try_into().ok()?;
    Some(crate::evm_address::raw_to_evm_address(&raw))
}

/// Convert EVM address (`0x...`) → JSON-hex script key used by address index.
fn address_to_script_hex(addr: &str) -> Option<String> {
    let raw = crate::evm_address::parse_evm_address(addr).ok()?;
    hash160_to_script_hex(&raw)
}

/// Build raw P2PKH scriptPubKey hex from 20-byte hash160.
/// Format wire: OP_DUP OP_HASH160 <20> OP_EQUALVERIFY OP_CHECKSIG = 76 a9 14 <20 bytes> 88 ac
fn hash160_to_script_hex(hash160: &[u8]) -> Option<String> {
    if hash160.len() != 20 { return None; }
    let mut script = Vec::with_capacity(25);
    script.push(0x76u8); // OP_DUP
    script.push(0xa9);   // OP_HASH160
    script.push(0x14);   // push 20 bytes
    script.extend_from_slice(hash160);
    script.push(0x88);   // OP_EQUALVERIFY
    script.push(0xac);   // OP_CHECKSIG
    Some(hex::encode(script))
}

/// Legacy JSON script format (data indexed trước v22.x dùng format này).
fn hash160_to_script_hex_legacy(hash160: &[u8]) -> Option<String> {
    if hash160.len() != 20 { return None; }
    let script = json!({
        "ops": ["OpDup", "OpHash160", {"OpPushData": hash160}, "OpEqualVerify", "OpCheckSig"]
    });
    Some(hex::encode(serde_json::to_vec(&script).ok()?))
}

/// Accept any address format (EVM `0x...`, bech32, or raw script_hex)
/// and return the script_hex key used in AddrIndexDb (raw P2PKH wire format).
fn any_addr_to_script_hex(s: &str) -> Option<String> {
    let s = s.trim();
    // EVM address: 0x + 40 hex chars
    if s.starts_with("0x") || s.starts_with("0X") {
        return address_to_script_hex(s);
    }
    // bech32 legacy: tpkt1… / pkt1… / rpkt1…
    if s.starts_with("tpkt1") || s.starts_with("pkt1") || s.starts_with("rpkt1") {
        let pkt_addr = crate::pkt_address::decode_address(s).ok()?;
        let hash160  = pkt_addr.hash160()?;
        return hash160_to_script_hex(&hash160);
    }
    // Passthrough: assume caller already has script_hex
    if s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(s.to_string());
    }
    None
}

/// Như any_addr_to_script_hex nhưng trả legacy JSON format (data cũ trước v22.x).
fn any_addr_to_script_hex_legacy(s: &str) -> Option<String> {
    let s = s.trim();
    // EVM address
    if s.starts_with("0x") || s.starts_with("0X") {
        let raw = crate::evm_address::parse_evm_address(s).ok()?;
        return hash160_to_script_hex_legacy(&raw);
    }
    // bech32 legacy
    if s.starts_with("tpkt1") || s.starts_with("pkt1") || s.starts_with("rpkt1") {
        let pkt_addr = crate::pkt_address::decode_address(s).ok()?;
        let hash160  = pkt_addr.hash160()?;
        return hash160_to_script_hex_legacy(&hash160);
    }
    None
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
    Query(params): Query<HeaderCursorParams>,
) -> impl IntoResponse {
    match ps.open() {
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"not synced"}))).into_response(),
        Some((sdb, _udb)) => {
            match crate::pkt_explorer_api::query_headers(
                &sdb, params.limit.min(100), params.cursor,
            ) {
                Ok((headers, tip)) => {
                    let next_cursor = headers.last().and_then(|h| h["height"].as_u64());
                    Json(json!({
                        "headers":     headers,
                        "tip":         tip,
                        "next_cursor": next_cursor,
                    })).into_response()
                }
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
    Path(addr): Path<String>,
) -> impl IntoResponse {
    let script = match any_addr_to_script_hex(&addr) {
        Some(s) => s,
        None    => return (StatusCode::BAD_REQUEST,
                           Json(json!({"error":"invalid address"}))).into_response(),
    };
    match ps.open_addr() {
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"not synced"}))).into_response(),
        Some(adb) => {
            // Thử raw P2PKH format trước (data mới), fallback JSON format (data cũ)
            let mut balance = adb.get_balance(&script).unwrap_or(0);
            let effective_script = if balance == 0 {
                if let Some(legacy) = any_addr_to_script_hex_legacy(&addr) {
                    let legacy_bal = adb.get_balance(&legacy).unwrap_or(0);
                    if legacy_bal > 0 { balance = legacy_bal; legacy }
                    else { script.clone() }
                } else { script.clone() }
            } else { script.clone() };
            let address = script_hex_to_address(&effective_script).unwrap_or_else(|| addr.clone());
            Json(json!({
                "address": address,
                "balance": balance,
                "balance_pkt": (balance as f64) / 1_073_741_824.0,
            })).into_response()
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
            // Nếu input là bech32/Base58 address, decode sang script_hex trước
            let script_wire   = any_addr_to_script_hex(&script);
            let script_legacy = any_addr_to_script_hex_legacy(&script);
            let mut utxos = Vec::new();
            if let Some(ref s) = script_wire {
                utxos = crate::pkt_explorer_api::query_utxos(&udb, s, 500).unwrap_or_default();
            }
            if utxos.is_empty() {
                if let Some(ref s) = script_legacy {
                    utxos = crate::pkt_explorer_api::query_utxos(&udb, s, 500).unwrap_or_default();
                }
            }
            // Fallback: coi input là raw script_hex
            if utxos.is_empty() && script_wire.is_none() {
                utxos = crate::pkt_explorer_api::query_utxos(&udb, &script, 500).unwrap_or_default();
            }
            Json(json!({"utxos": utxos})).into_response()
        }
    }
}

/// GET /api/testnet/address/:addr/utxos
/// :addr accepts bech32, Base58Check, or raw script_hex.
/// Thử cả wire format (76a914…) và legacy JSON format để tương thích với
/// UTXOs indexed trước v22.x.
async fn ps_addr_utxos(
    State(ps):    State<PathState>,
    Path(addr):   Path<String>,
) -> impl IntoResponse {
    let script_wire   = any_addr_to_script_hex(&addr);
    let script_legacy = any_addr_to_script_hex_legacy(&addr);
    if script_wire.is_none() && script_legacy.is_none() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"invalid address"}))).into_response();
    }
    match ps.open() {
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"not synced"}))).into_response(),
        Some((_sdb, udb)) => {
            // Thử wire format trước, nếu rỗng thử legacy
            let mut utxos = Vec::new();
            if let Some(ref s) = script_wire {
                utxos = crate::pkt_explorer_api::query_utxos(&udb, s, 500).unwrap_or_default();
            }
            if utxos.is_empty() {
                if let Some(ref s) = script_legacy {
                    utxos = crate::pkt_explorer_api::query_utxos(&udb, s, 500).unwrap_or_default();
                }
            }
            let address = script_wire.as_deref()
                .and_then(|s| script_hex_to_address(s))
                .unwrap_or_else(|| addr.clone());
            Json(json!({"address": address, "utxos": utxos})).into_response()
        }
    }
}

/// Embedded testnet panel JS — served at /static/testnet.js.
const TESTNET_JS: &str = include_str!("../web/js/testnet.js");

// ── Address index handlers ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AddrTxsParams {
    cursor: Option<u64>,
    page:   Option<u64>,   // desktop sends page=0,1,2…; treated as cursor alias
    limit:  Option<usize>,
}

/// GET /api/testnet/address/:addr/txs?cursor=HEIGHT&limit=N  (or ?page=N&limit=N)
/// :addr accepts bech32, Base58Check, or raw script_hex.
async fn ps_addr_txs(
    State(ps):     State<PathState>,
    Path(addr):    Path<String>,
    Query(params): Query<AddrTxsParams>,
) -> impl IntoResponse {
    let script = match any_addr_to_script_hex(&addr) {
        Some(s) => s,
        None    => return (StatusCode::BAD_REQUEST,
                           Json(json!({"error":"invalid address"}))).into_response(),
    };
    let adb = match ps.open_addr() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           Json(json!({"error":"address index not ready"}))).into_response(),
        Some(d) => d,
    };
    let limit  = params.limit.unwrap_or(50).min(200);
    // cursor takes priority; page is converted: page N → skip N*limit entries via cursor=None
    // (simple approach: use cursor directly; page ignored if cursor present)
    let cursor = params.cursor;
    // Thử raw P2PKH format; nếu không có kết quả thử legacy JSON format
    let (effective_script, entries) = {
        let r = adb.get_tx_history(&script, cursor, limit).unwrap_or_default();
        if r.is_empty() {
            if let Some(legacy) = any_addr_to_script_hex_legacy(&addr) {
                let r2 = adb.get_tx_history(&legacy, cursor, limit).unwrap_or_default();
                if !r2.is_empty() { (legacy, r2) } else { (script, r) }
            } else { (script, r) }
        } else { (script, r) }
    };
    let address = script_hex_to_address(&effective_script).unwrap_or_else(|| addr.clone());
    let txs: Vec<_> = entries.iter().map(|e| json!({
        "height": e.height,
        "txid":   e.txid,
    })).collect();
    let count = txs.len();
    Json(json!({"address": address, "txs": txs, "count": count})).into_response()
}

/// GET /api/testnet/addr/:base58?limit=N
/// Accepts human-readable Base58Check address, returns balance + tx history.
async fn ps_addr_by_base58(
    State(ps):     State<PathState>,
    Path(addr):    Path<String>,
    Query(params): Query<AddrTxsParams>,
) -> impl IntoResponse {
    let script_wire   = any_addr_to_script_hex(&addr);
    let script_legacy = any_addr_to_script_hex_legacy(&addr);
    if script_wire.is_none() && script_legacy.is_none() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"invalid address"}))).into_response();
    }
    let adb = match ps.open_addr() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           Json(json!({"error":"address index not ready"}))).into_response(),
        Some(d) => d,
    };
    // Thử wire format trước, fallback legacy nếu balance = 0
    let (script, balance) = {
        let wire_bal = script_wire.as_ref()
            .map(|s| adb.get_balance(s).unwrap_or(0)).unwrap_or(0);
        if wire_bal > 0 {
            (script_wire.unwrap(), wire_bal)
        } else {
            let leg_script = script_legacy.unwrap_or_else(|| script_wire.unwrap_or_default());
            let leg_bal = adb.get_balance(&leg_script).unwrap_or(0);
            (leg_script, leg_bal)
        }
    };
    let limit = params.limit.unwrap_or(50).min(200);
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

// ── Block detail handler ───────────────────────────────────────────────────────

/// GET /api/testnet/block/:height
/// Header đầy đủ + block_time + difficulty + hashrate + tx list từ htx: index.
async fn ps_block_detail(
    State(ps):      State<PathState>,
    Path(height):   Path<u64>,
) -> impl IntoResponse {
    use crate::pkt_explorer_api::format_header_json;
    use crate::pkt_wire::WireBlockHeader;
    use crate::pkt_analytics::{bits_to_difficulty, estimate_hashrate_from};

    let sdb = match SyncDb::open_read_only(&ps.sync_path).ok() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           Json(json!({"error":"sync db not ready"}))).into_response(),
        Some(d) => d,
    };

    // Load this block's header
    let raw = match sdb.load_header(height) {
        Ok(Some(r)) => r,
        Ok(None)    => return (StatusCode::NOT_FOUND,
                               Json(json!({"error":"block not found"}))).into_response(),
        Err(e)      => return (StatusCode::INTERNAL_SERVER_ERROR,
                               Json(json!({"error": e.to_string()}))).into_response(),
    };
    let hdr = match WireBlockHeader::from_bytes(&raw) {
        Ok(h)  => h,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
                          Json(json!({"error": format!("corrupt header: {:?}", e)}))).into_response(),
    };

    let mut block_json = format_header_json(&hdr, height);

    // Block time vs previous block
    let block_time_secs: Option<f64> = if height > 0 {
        sdb.load_header(height - 1).ok().flatten()
            .and_then(|r| WireBlockHeader::from_bytes(&r).ok())
            .map(|prev| (hdr.timestamp as i64 - prev.timestamp as i64).max(0) as f64)
    } else {
        None
    };

    let difficulty = bits_to_difficulty(hdr.bits);
    let hashrate   = block_time_secs.map(|bt| estimate_hashrate_from(difficulty, bt));

    // TX list from htx: secondary index
    let txids: Vec<String> = ps.open_addr()
        .map(|adb| adb.get_txids_at_height(height, 200))
        .unwrap_or_default();

    // v22.2: tx_count từ sync_db (saved khi sync_blocks). Fallback = txids.len().
    let stored_tx_count = sdb.get_block_tx_count(height);
    let tx_count = if stored_tx_count > 0 { stored_tx_count } else { txids.len() as u64 };

    // Tip height for confirmations
    let tip = sdb.get_sync_height().ok().flatten().unwrap_or(height);
    let confirmations = tip.saturating_sub(height) + 1;

    if let Some(obj) = block_json.as_object_mut() {
        obj.insert("block_time_secs".into(), block_time_secs.map(|v| json!(v)).unwrap_or(Value::Null));
        obj.insert("difficulty".into(),      json!(difficulty));
        obj.insert("hashrate".into(),        hashrate.map(|v| json!(v)).unwrap_or(Value::Null));
        obj.insert("confirmations".into(),   json!(confirmations));
        obj.insert("txids".into(),           json!(txids));
        obj.insert("tx_count".into(),        json!(tx_count));
    }

    Json(block_json).into_response()
}

// ── TX detail handler ──────────────────────────────────────────────────────────

/// GET /api/testnet/tx/:txid
/// Tra cứu transaction từ mempool (full parse) hoặc UTXO db (unspent outputs only).
async fn ps_tx_detail(
    State(ps):    State<PathState>,
    Path(txid):   Path<String>,
) -> impl IntoResponse {
    use crate::pkt_block_sync::read_tx_s;
    use crate::pkt_utxo_sync::WireTxIn;
    use std::io::Cursor;

    let txid_lc = txid.trim().to_lowercase();

    // ── 1. Mempool (raw bytes available → full parse) ────────────────────────
    if let Some(mdb) = ps.open_mempool() {
        if let Some((raw, fee_rate_msat, ts_ns)) = mdb.get_tx_raw(&txid_lc) {
            let mut cur = Cursor::new(&raw);
            if let Ok(wtx) = read_tx_s(&mut cur) {
                let is_coinbase = wtx.is_coinbase();
                let size        = raw.len() as u64;

                let inputs: Vec<Value> = wtx.inputs.iter().map(|i: &WireTxIn| {
                    if i.is_coinbase() {
                        json!({"type": "coinbase"})
                    } else {
                        json!({
                            "prev_txid": hex::encode(i.prev_txid),
                            "prev_vout": i.prev_vout,
                        })
                    }
                }).collect();

                // Look up prev UTXOs to get input values
                let mut inputs_rich: Vec<Value> = Vec::with_capacity(inputs.len());
                let udb = ps.open().map(|(_, u)| u);
                for i in &wtx.inputs {
                    if i.is_coinbase() {
                        inputs_rich.push(json!({"type": "coinbase"}));
                        continue;
                    }
                    let utxo = udb.as_ref()
                        .and_then(|u| u.get_utxo(&i.prev_txid, i.prev_vout).ok().flatten());
                    let address = utxo.as_ref()
                        .and_then(|u| script_hex_to_address(&hex::encode(&u.script_pubkey)));
                    inputs_rich.push(json!({
                        "prev_txid": hex::encode(i.prev_txid),
                        "prev_vout": i.prev_vout,
                        "value":     utxo.as_ref().map(|u| u.value),
                        "address":   address,
                    }));
                }

                let outputs: Vec<Value> = wtx.outputs.iter().enumerate().map(|(idx, o)| {
                    let address = script_hex_to_address(&hex::encode(&o.script_pubkey));
                    json!({
                        "vout":    idx,
                        "value":   o.value,
                        "value_pkt": (o.value as f64) / 1_073_741_824.0,
                        "address": address,
                    })
                }).collect();

                let total_out: u64 = wtx.outputs.iter().map(|o| o.value).sum();
                let ts_secs = ts_ns / 1_000_000_000;

                return Json(json!({
                    "txid":          txid_lc,
                    "status":        "mempool",
                    "is_coinbase":   is_coinbase,
                    "size":          size,
                    "fee_rate_msat_vb": fee_rate_msat,
                    "timestamp":     if ts_secs > 0 { Value::Number(ts_secs.into()) } else { Value::Null },
                    "inputs":        inputs_rich,
                    "outputs":       outputs,
                    "total_out":     total_out,
                    "total_out_pkt": (total_out as f64) / 1_073_741_824.0,
                    "block_height":  Value::Null,
                    "confirmations": 0,
                })).into_response();
            }
        }
    }

    // ── 2. UTXO scan (confirmed tx — unspent outputs only) ──────────────────
    if let Some((sdb, udb)) = ps.open() {
        let utxos = udb.scan_tx_outputs(&txid_lc);
        if !utxos.is_empty() {
            let outputs: Vec<Value> = utxos.iter().map(|u| {
                let address = script_hex_to_address(&hex::encode(&u.script_pubkey));
                json!({
                    "vout":      u.vout,
                    "value":     u.value,
                    "value_pkt": (u.value as f64) / 1_073_741_824.0,
                    "address":   address,
                    "spent":     false,
                })
            }).collect();
            let total_out: u64 = utxos.iter().map(|u| u.value).sum();

            // ── Ưu tiên TX index (v24.1) → đủ size + fee_rate + height
            let tx_meta = udb.get_tx_meta(&txid_lc).ok().flatten();

            // Tìm block height: tx_meta → utxo.height → addr index
            let block_height: u64 = tx_meta.as_ref().map(|m| m.height).filter(|&h| h > 0)
                .or_else(|| {
                    let h = utxos[0].height;
                    if h > 0 { Some(h) } else { None }
                })
                .or_else(|| {
                    ps.open_addr().and_then(|adb| {
                        let script_hex = hex::encode(&utxos[0].script_pubkey);
                        adb.get_tx_height(&script_hex, &txid_lc)
                    })
                })
                .unwrap_or(0);

            let timestamp: Value = if block_height > 0 {
                match crate::pkt_explorer_api::query_header(&sdb, block_height) {
                    Ok(Some(hdr)) => hdr["timestamp"].clone(),
                    _ => Value::Null,
                }
            } else { Value::Null };
            let tip = sdb.get_sync_height().ok().flatten().unwrap_or(0);
            let confirmations: Value = if block_height > 0 && tip >= block_height {
                json!(tip - block_height + 1)
            } else { Value::Null };

            let (size_v, fee_rate_v, is_coinbase_v) = match &tx_meta {
                Some(m) => (
                    json!(m.size),
                    if m.fee_rate_msat_vb > 0 { json!(m.fee_rate_msat_vb) } else { Value::Null },
                    json!(m.is_coinbase),
                ),
                None => (Value::Null, Value::Null, Value::Null),
            };

            return Json(json!({
                "txid":             txid_lc,
                "status":           "confirmed",
                "is_coinbase":      is_coinbase_v,
                "size":             size_v,
                "fee_rate_msat_vb": fee_rate_v,
                "timestamp":        timestamp,
                "inputs":           [],
                "outputs":          outputs,
                "total_out":        total_out,
                "total_out_pkt":    (total_out as f64) / 1_073_741_824.0,
                "block_height":     if block_height > 0 { json!(block_height) } else { Value::Null },
                "confirmations":    confirmations,
                "note":             "confirmed tx — showing unspent outputs only",
            })).into_response();
        }
    }

    (StatusCode::NOT_FOUND, Json(json!({"error": "transaction not found"}))).into_response()
}

// ── Broadcast TX handler ───────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct BroadcastBody {
    raw_hex: String,
}

/// POST /api/testnet/tx/broadcast — nhận raw tx hex, relay lên PKT testnet peer, trả txid.
async fn ps_tx_broadcast(
    State(ps):  State<PathState>,
    Json(body): Json<BroadcastBody>,
) -> impl IntoResponse {
    use crate::pkt_block_sync::read_tx_s;
    use crate::pkt_utxo_sync::wire_txid;
    use crate::pkt_peer::{do_handshake, send_msg, PeerConfig};
    use crate::pkt_wire::{PktMsg, TESTNET_MAGIC};
    use std::io::{Cursor, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    /// ~1 MiB wire payload sau decode — đủ cho TX lớn, tránh DoS bộ nhớ từ hex khổng lồ.
    const MAX_BROADCAST_HEX: usize = 2_000_000;
    let hex_str = body.raw_hex.trim();
    if hex_str.len() > MAX_BROADCAST_HEX {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "raw_hex exceeds maximum size"})),
        )
            .into_response();
    }

    // 1. Decode hex
    let raw = match hex::decode(hex_str) {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": format!("hex decode: {}", e)}))).into_response(),
    };

    // 2. Parse để validate và lấy txid
    let txid = {
        let mut cur = Cursor::new(&raw);
        match read_tx_s(&mut cur) {
            Ok(tx) => hex::encode(wire_txid(&tx)),
            Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": format!("parse tx: {:?}", e)}))).into_response(),
        }
    };

    // 3. Relay lên PKT testnet peer (fire-and-forget, 10s timeout)
    let our_height = ps.open().and_then(|(sdb, _)| sdb.get_sync_height().ok().flatten()).unwrap_or(0) as i32;
    let cfg = PeerConfig { our_height, connect_timeout_secs: 10, read_timeout_secs: 10, max_retries: 0, ..Default::default() };
    let relay_err: Option<String> = tokio::task::spawn_blocking({
        let raw = raw.clone();
        let _txid_bytes: [u8; 32] = hex::decode(&txid).ok()
            .and_then(|b| b.try_into().ok()).unwrap_or([0u8; 32]);
        move || {
            let addr_str = format!("{}:{}", cfg.host, cfg.port);
            let sock_addr = {
                use std::net::ToSocketAddrs;
                match addr_str.to_socket_addrs().ok().and_then(|mut i| i.next()) {
                    Some(a) => a,
                    None    => return Err(format!("cannot resolve {}", addr_str)),
                }
            };
            let Ok(mut stream) = TcpStream::connect_timeout(&sock_addr, Duration::from_secs(cfg.connect_timeout_secs))
            else { return Err("connect failed".to_string()); };
            stream.set_read_timeout(Some(Duration::from_secs(cfg.read_timeout_secs))).ok();
            if do_handshake(&mut stream, &cfg).is_err() { return Err("handshake failed".to_string()); }

            // Gửi TX trực tiếp dưới dạng "tx" message
            let mut cmd = [0u8; 12];
            cmd[..2].copy_from_slice(b"tx");
            let tx_msg = PktMsg::Unknown { command: cmd, payload: raw };
            if send_msg(&mut stream, tx_msg, TESTNET_MAGIC).is_err() { return Err("send tx failed".to_string()); }
            let _ = stream.flush();

            // Đọc response 3s để bắt reject message
            stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
            loop {
                match crate::pkt_peer::recv_msg(&mut stream, TESTNET_MAGIC) {
                    Ok(PktMsg::Ping { nonce }) => {
                        let _ = send_msg(&mut stream, PktMsg::Pong { nonce }, TESTNET_MAGIC);
                    }
                    Ok(PktMsg::Unknown { command, payload }) => {
                        let cmd_str = std::str::from_utf8(&command)
                            .unwrap_or("?").trim_matches('\0');
                        if cmd_str == "reject" {
                            let reason = String::from_utf8_lossy(&payload).to_string();
                            return Err(format!("rejected: {}", reason));
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break, // timeout → không có reject → ok
                }
            }
            Ok::<(), String>(())
        }
    }).await.ok().and_then(|r| r.err());

    if let Some(err) = relay_err {
        (StatusCode::BAD_GATEWAY, Json(json!({"error": format!("relay: {}", err), "txid": txid}))).into_response()
    } else {
        // v23.6: store broadcast TX in local MempoolDb so miner template includes it
        if let Ok(mdb) = MempoolDb::open(&ps.mempool_path) {
            let ts_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            // fee_rate unknown without UTXO lookup — use min (1 msat/vB) so TX is included
            let _ = mdb.put_tx(&txid, &raw, 1, ts_ns);
        }
        Json(json!({"txid": txid, "status": "broadcast"})).into_response()
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

// ── Health handler (v18.8) ─────────────────────────────────────────────────────

/// GET /api/health/detailed
async fn ps_health(State(ps): State<PathState>) -> impl IntoResponse {
    let status = crate::pkt_health::query_health(
        &ps.sync_path,
        &ps.utxo_path,
        &ps.addr_path,
        &ps.mempool_path,
    );
    let code = if status.ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(serde_json::to_value(status).unwrap_or_default())).into_response()
}

// ── Summary handler (v18.6) ────────────────────────────────────────────────────

/// GET /api/testnet/summary
/// Single-request home page data: height + hashrate + mempool + rich top 5.
/// Optimised for mobile/low-bandwidth (1 round-trip instead of 5+).
async fn ps_summary(State(ps): State<PathState>) -> impl IntoResponse {
    use crate::pkt_analytics::{load_recent_headers, bits_to_difficulty, estimate_hashrate_from};

    // ── Chain stats ────────────────────────────────────────────────────────────
    let (height, tip_hash, synced, utxo_count, total_value_sat) =
        match (SyncDb::open_read_only(&ps.sync_path).ok(), ps.open()) {
            (Some(sdb), Some((_, udb))) => {
                // get_sync_height() có thể trả None nếu chưa ghi meta — fallback đọc từ headers
                let h = sdb.get_sync_height().ok().flatten()
                    .or_else(|| {
                        use crate::pkt_analytics::load_recent_headers;
                        load_recent_headers(&sdb, 1).ok()
                            .and_then(|v| v.into_iter().next())
                            .map(|(h, _)| h)
                    })
                    .unwrap_or(0);
                let tip = sdb.get_tip_hash().ok().flatten()
                    .map(hex::encode)
                    .unwrap_or_else(|| "0".repeat(64));
                let synced = h > 0;
                let cnt    = udb.count_utxos().unwrap_or(0);
                let val    = udb.total_value().unwrap_or(0);
                (h, tip, synced, cnt, val)
            }
            _ => (0u64, "0".repeat(64), false, 0u64, 0u64),
        };

    // ── Hashrate & block_time avg (last 10 blocks) ─────────────────────────────
    let (hashrate, block_time_avg, difficulty) = match SyncDb::open_read_only(&ps.sync_path).ok() {
        None => (0.0f64, 0.0f64, 0.0f64),
        Some(sdb) => {
            let headers = load_recent_headers(&sdb, 11).unwrap_or_default();
            if headers.len() < 2 {
                (0.0, 0.0, 0.0)
            } else {
                let mut total_hr = 0.0f64;
                let mut total_bt = 0.0f64;
                let n = (headers.len() - 1) as f64;
                for i in 1..headers.len() {
                    let (_, ref cur)  = headers[i];
                    let (_, ref prev) = headers[i - 1];
                    let dt   = (cur.timestamp as i64 - prev.timestamp as i64).max(1) as f64;
                    let diff = bits_to_difficulty(cur.bits);
                    total_hr += estimate_hashrate_from(diff, dt);
                    total_bt += dt;
                }
                let tip_diff = headers.last()
                    .map(|(_, h)| bits_to_difficulty(h.bits))
                    .unwrap_or(0.0);
                (total_hr / n, total_bt / n, tip_diff)
            }
        }
    };

    // ── Mempool ────────────────────────────────────────────────────────────────
    let (mempool_count, mempool_top_fee_msat_vb) = match ps.open_mempool() {
        None => (0u64, 0u64),
        Some(mdb) => {
            let cnt = mdb.count().unwrap_or(0) as u64;
            let top = mdb.get_pending(1).ok()
                .and_then(|txs| txs.into_iter().next().map(|t| t.fee_rate_msat_vb))
                .unwrap_or(0);
            (cnt, top)
        }
    };

    // ── Rich top 5 ─────────────────────────────────────────────────────────────
    let rich_top5: Vec<Value> = match ps.open_addr() {
        None => vec![],
        Some(adb) => {
            let ldb = ps.open_label();
            adb.get_rich_list(5).unwrap_or_default().into_iter()
                .map(|(script, bal)| {
                    let address = script_hex_to_address(&script);
                    let label   = ldb.as_ref().and_then(|db| {
                        db.get_label_for(&script, address.as_deref())
                    });
                    json!({
                        "script":      script,
                        "address":     address,
                        "balance":     bal,
                        "balance_pkt": (bal as f64) / 1_073_741_824.0,
                        "label":       label,
                    })
                })
                .collect()
        }
    };

    // Block reward thực tế từ coinbase TX của block mới nhất
    // Trả 0 nếu chưa có data — không dùng formula lý thuyết vì testnet params khác mainnet
    let block_reward: u64 = if height == 0 {
        0
    } else {
        ps.open_addr()
            .and_then(|adb| adb.get_txids_at_height(height, 1).into_iter().next())
            .and_then(|coinbase_txid| {
                ps.open().map(|(_, udb)| {
                    udb.scan_tx_outputs(&coinbase_txid)
                       .iter()
                       .map(|u| u.value)
                       .sum::<u64>()
                })
            })
            .unwrap_or(0)
    };

    Json(json!({
        "height":                  height,
        "tip_hash":                tip_hash,
        "synced":                  synced,
        "utxo_count":              utxo_count,
        "total_value_sat":         total_value_sat,
        "total_value_pkt":         (total_value_sat as f64) / 1_073_741_824.0,
        "hashrate":                hashrate,
        "block_time_avg":          block_time_avg,
        "difficulty":              difficulty,
        "mempool_count":           mempool_count,
        "mempool_top_fee_msat_vb": mempool_top_fee_msat_vb,
        "rich_top5":               rich_top5,
        "block_reward":            block_reward,
        "block_reward_pkt":        (block_reward as f64) / 1_073_741_824.0,
    })).into_response()
}

// ── TX list handler (v18.5) ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TxsListParams {
    cursor: Option<u64>,
    #[serde(default = "default_txs_limit")]
    limit: usize,
}
fn default_txs_limit() -> usize { 50 }

/// GET /api/testnet/txs?cursor=HEIGHT&limit=N
/// Newest TXs first, cursor-based pagination. cursor = last seen height (exclusive).
async fn ps_txs_list(
    State(ps):     State<PathState>,
    Query(params): Query<TxsListParams>,
) -> impl IntoResponse {
    use crate::pkt_wire::WireBlockHeader;

    let sdb = match SyncDb::open_read_only(&ps.sync_path).ok() {
        Some(db) => db,
        None => return (StatusCode::SERVICE_UNAVAILABLE,
                        Json(json!({"error":"not synced"}))).into_response(),
    };
    let adb = match ps.open_addr() {
        Some(db) => db,
        None => return (StatusCode::SERVICE_UNAVAILABLE,
                        Json(json!({"error":"not synced"}))).into_response(),
    };

    let limit = params.limit.min(100);
    let raw   = adb.get_recent_txids(params.cursor, limit);
    let next_cursor = raw.last().map(|(h, _)| *h);

    let mut txs: Vec<Value> = Vec::new();
    for (h, txid) in &raw {
        let ts = sdb.load_header(*h).ok().flatten()
            .and_then(|b| WireBlockHeader::from_bytes(&b).ok())
            .map(|hdr| hdr.timestamp as u64)
            .unwrap_or(0);
        txs.push(json!({ "txid": txid, "height": h, "timestamp": ts }));
    }

    Json(json!({ "txs": txs, "next_cursor": next_cursor })).into_response()
}

// ── Export handlers (v18.9) ────────────────────────────────────────────────────

/// GET /api/testnet/address/:s/export.csv
/// TX history của address dưới dạng CSV (height,txid).
async fn ps_export_address(
    State(ps):    State<PathState>,
    Path(script): Path<String>,
) -> impl IntoResponse {
    let adb = match ps.open_addr() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           [(header::CONTENT_TYPE, "text/plain")],
                           "address index not ready".to_string()).into_response(),
        Some(d) => d,
    };
    let csv = tokio::task::spawn_blocking(move || {
        crate::pkt_export::generate_address_csv(&adb, &script, crate::pkt_export::MAX_ADDR_EXPORT_ROWS)
    }).await.unwrap_or_default();
    (
        [(header::CONTENT_TYPE, "text/csv; charset=utf-8"),
         (header::CONTENT_DISPOSITION, "attachment; filename=\"transactions.csv\"")],
        csv,
    ).into_response()
}

#[derive(Deserialize)]
struct BlocksExportParams {
    from: Option<u64>,
    to:   Option<u64>,
}

/// GET /api/testnet/blocks/export.csv?from=H&to=H
/// Blocks trong khoảng [from, to] dưới dạng CSV.
/// Tối đa 10_000 blocks (MAX_EXPORT_BLOCKS). Nếu thiếu from/to dùng 0 / tip.
async fn ps_export_blocks(
    State(ps):     State<PathState>,
    Query(params): Query<BlocksExportParams>,
) -> impl IntoResponse {
    let sdb = match SyncDb::open_read_only(&ps.sync_path).ok() {
        None    => return (StatusCode::SERVICE_UNAVAILABLE,
                           [(header::CONTENT_TYPE, "text/plain")],
                           "sync db not ready".to_string()).into_response(),
        Some(d) => d,
    };
    let tip  = sdb.get_sync_height().ok().flatten().unwrap_or(0);
    let from = params.from.unwrap_or(0);
    let to   = params.to.unwrap_or(tip);
    let csv = tokio::task::spawn_blocking(move || {
        crate::pkt_export::generate_blocks_csv(&sdb, from, to)
    }).await.unwrap_or_default();
    (
        [(header::CONTENT_TYPE, "text/csv; charset=utf-8"),
         (header::CONTENT_DISPOSITION, "attachment; filename=\"blocks.csv\"")],
        csv,
    ).into_response()
}

// ── Path helpers ───────────────────────────────────────────────────────────────

/// Build a path under $HOME (falls back to "." if HOME/USERPROFILE unset).
pub fn home_path(suffix: &str) -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(suffix)
}

/// Default SyncDb path — testnet hoặc mainnet theo pkt_paths::is_mainnet().
pub fn default_sync_db_path() -> PathBuf {
    crate::pkt_paths::sync_db()
}

/// Default UtxoDb path — testnet hoặc mainnet theo pkt_paths::is_mainnet().
pub fn default_utxo_db_path() -> PathBuf {
    crate::pkt_paths::utxo_db()
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
    let auth = crate::api_auth::ApiKeyStore::load_default();

    // sync/start + sync/stop: require Write role (fork process từ xa — cần bảo vệ)
    let sync_control = Router::new()
        .route("/api/testnet/sync/start", post(ps_sync_start))
        .route("/api/testnet/sync/stop",  post(ps_sync_stop))
        .layer(axum::middleware::from_fn_with_state(
            auth,
            crate::api_auth::require_write_middleware,
        ));

    Router::new()
        .route("/static/testnet.js", get(serve_testnet_js))
        .route("/api/testnet/sync-status", get(ps_sync_status))
        .route("/api/testnet/stats", get(ps_stats))
        .route("/api/testnet/headers", get(ps_headers))
        .route("/api/testnet/header/:h", get(ps_header))
        .route("/api/testnet/balance/:s", get(ps_balance))
        .route("/api/testnet/utxos/:s", get(ps_utxos))
        .route("/api/testnet/address/:s/txs", get(ps_addr_txs))
        .route("/api/testnet/address/:s/utxos", get(ps_addr_utxos))
        .route("/api/testnet/addr/:base58", get(ps_addr_by_base58))
        .route("/api/testnet/rich-list", get(ps_rich_list))
        .route("/api/testnet/richlist", get(ps_rich_list))
        .route("/api/testnet/mempool", get(ps_mempool))
        .route("/api/testnet/mempool/fee-histogram", get(ps_mempool_histogram))
        .route("/api/health/detailed",               get(ps_health))
        .route("/api/testnet/summary",               get(ps_summary))
        .route("/api/testnet/analytics",             get(ps_analytics))
        .route("/api/testnet/block/:height",         get(ps_block_detail))
        .route("/api/testnet/tx/:txid",              get(ps_tx_detail))
        .route("/api/testnet/txs",                   get(ps_txs_list))
        .route("/api/testnet/search",                get(ps_search))
        .route("/api/testnet/label/:script",         get(ps_label))
        .route("/api/testnet/address/:s/export.csv", get(ps_export_address))
        .route("/api/testnet/blocks/export.csv",     get(ps_export_blocks))
        .route("/api/testnet/sync/proc-status",      get(ps_sync_proc_status))
        .merge(sync_control)
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
