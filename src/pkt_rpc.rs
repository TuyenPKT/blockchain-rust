#![allow(dead_code)]
//! v19.2 — JSON-RPC 2.0 Bitcoin-compatible API
//!
//! Endpoint: `POST /rpc`  (Content-Type: application/json)
//!
//! Ports mặc định (tương thích Bitcoin Core):
//!   testnet: 18332  mainnet: 8332
//!   (Khi chạy qua pktscan trên 8080, /rpc được proxy vào cùng server)
//!
//! ## Supported methods
//!
//! | Method                            | Params                     | Returns          |
//! |-----------------------------------|----------------------------|------------------|
//! | `getblockcount`                   | []                         | height: u64      |
//! | `getblockhash`                    | [height: u64]              | hash: String     |
//! | `getblock`                        | [hash: String, verb?: u32] | block JSON       |
//! | `getrawtransaction`               | [txid: String, verb?: bool]| tx JSON          |
//! | `getmininginfo`                   | []                         | mining JSON      |
//! | `getnetworkinfo`                  | []                         | network JSON     |
//! | `sendrawtransaction`              | [hex: String]              | error (stub)     |
//!
//! ## JSON-RPC 2.0 wire format
//!
//! Request:  `{"jsonrpc":"2.0","id":1,"method":"getblockcount","params":[]}`
//! Response: `{"jsonrpc":"2.0","id":1,"result":12345}`
//! Error:    `{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}`

use std::path::PathBuf;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::pkt_analytics::{bits_to_difficulty, estimate_hashrate_from, load_recent_headers};
use crate::pkt_mempool_sync::MempoolDb;
use crate::pkt_sync::SyncDb;
use crate::pkt_wire::WireBlockHeader;

// ── JSON-RPC error codes ───────────────────────────────────────────────────────

pub const ERR_PARSE:          i32 = -32700;
pub const ERR_INVALID_REQ:    i32 = -32600;
pub const ERR_METHOD_NOT_FOUND: i32 = -32601;
pub const ERR_INVALID_PARAMS: i32 = -32602;
pub const ERR_INTERNAL:       i32 = -32603;
// Bitcoin-compatible application errors
pub const ERR_BLOCK_NOT_FOUND: i32 = -5;
pub const ERR_TX_NOT_FOUND:    i32 = -5;
pub const ERR_NOT_SYNCED:      i32 = -1;
pub const ERR_UNSUPPORTED:     i32 = -32;

// ── RPC wire types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: Option<String>,
    pub method:  String,
    #[serde(default)]
    pub params:  Value,
    pub id:      Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result:  Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error:   Option<RpcErrorBody>,
    pub id:      Value,
}

#[derive(Debug, Serialize, Clone)]
pub struct RpcErrorBody {
    pub code:    i32,
    pub message: String,
}

impl RpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self { jsonrpc: "2.0", result: Some(result), error: None, id }
    }

    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            result:  None,
            error:   Some(RpcErrorBody { code, message: message.into() }),
            id,
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct RpcState {
    pub sync_path:    PathBuf,
    pub mempool_path: PathBuf,
}

impl RpcState {
    fn open_sync(&self) -> Option<SyncDb> {
        SyncDb::open_read_only(&self.sync_path).ok()
    }
    fn open_mempool(&self) -> Option<MempoolDb> {
        MempoolDb::open_read_only(&self.mempool_path).ok()
    }
}

// ── Method dispatch ───────────────────────────────────────────────────────────

pub fn dispatch(req: &RpcRequest, state: &RpcState) -> RpcResponse {
    let id = req.id.clone().unwrap_or(Value::Null);
    match req.method.as_str() {
        "getblockcount"      => method_getblockcount(id, state),
        "getblockhash"       => method_getblockhash(id, &req.params, state),
        "getblock"           => method_getblock(id, &req.params, state),
        "getrawtransaction"  => method_getrawtransaction(id, &req.params, state),
        "getmininginfo"      => method_getmininginfo(id, state),
        "getnetworkinfo"     => method_getnetworkinfo(id, state),
        "sendrawtransaction" => RpcResponse::err(id, ERR_UNSUPPORTED,
            "sendrawtransaction: peer relay not yet implemented"),
        _                    => RpcResponse::err(id, ERR_METHOD_NOT_FOUND,
            format!("Method not found: {}", req.method)),
    }
}

// ── getblockcount ─────────────────────────────────────────────────────────────

fn method_getblockcount(id: Value, state: &RpcState) -> RpcResponse {
    let sdb = match state.open_sync() {
        None    => return RpcResponse::err(id, ERR_NOT_SYNCED, "sync db not available"),
        Some(d) => d,
    };
    let height = sdb.get_sync_height().ok().flatten().unwrap_or(0);
    RpcResponse::ok(id, json!(height))
}

// ── getblockhash ──────────────────────────────────────────────────────────────

fn method_getblockhash(id: Value, params: &Value, state: &RpcState) -> RpcResponse {
    let height = match params.get(0).and_then(|v| v.as_u64()) {
        None    => return RpcResponse::err(id, ERR_INVALID_PARAMS,
            "getblockhash requires params: [height: number]"),
        Some(h) => h,
    };
    let sdb = match state.open_sync() {
        None    => return RpcResponse::err(id, ERR_NOT_SYNCED, "sync db not available"),
        Some(d) => d,
    };
    match sdb.load_header(height) {
        Ok(Some(raw)) => {
            let hash = hex::encode(WireBlockHeader::block_hash_of_bytes(&raw));
            RpcResponse::ok(id, json!(hash))
        }
        Ok(None) => RpcResponse::err(id, ERR_BLOCK_NOT_FOUND,
            format!("Block not found at height {}", height)),
        Err(e) => RpcResponse::err(id, ERR_INTERNAL, e.to_string()),
    }
}

// ── getblock ──────────────────────────────────────────────────────────────────

/// Hash → height: scan từ tip xuống, giới hạn MAX_HASH_SCAN blocks.
const MAX_HASH_SCAN: u64 = 100_000;

fn find_height_by_hash(sdb: &SyncDb, target_hash: &str) -> Option<u64> {
    let tip = sdb.get_sync_height().ok().flatten()?;
    let lo  = tip.saturating_sub(MAX_HASH_SCAN);
    for h in (lo..=tip).rev() {
        if let Ok(Some(raw)) = sdb.load_header(h) {
            let hash = hex::encode(WireBlockHeader::block_hash_of_bytes(&raw));
            if hash == target_hash { return Some(h); }
        }
    }
    None
}

fn method_getblock(id: Value, params: &Value, state: &RpcState) -> RpcResponse {
    let hash_or_height = match params.get(0) {
        None    => return RpcResponse::err(id, ERR_INVALID_PARAMS,
            "getblock requires params: [hash_or_height, verbosity?]"),
        Some(v) => v.clone(),
    };
    let verbosity = params.get(1).and_then(|v| v.as_u64()).unwrap_or(1);

    let sdb = match state.open_sync() {
        None    => return RpcResponse::err(id, ERR_NOT_SYNCED, "sync db not available"),
        Some(d) => d,
    };

    // Chấp nhận cả height (number) hoặc hash (string)
    let height = if let Some(h) = hash_or_height.as_u64() {
        h
    } else if let Some(s) = hash_or_height.as_str() {
        match find_height_by_hash(&sdb, s) {
            Some(h) => h,
            None    => return RpcResponse::err(id, ERR_BLOCK_NOT_FOUND,
                format!("Block not found: {}", s)),
        }
    } else {
        return RpcResponse::err(id, ERR_INVALID_PARAMS,
            "getblock: first param must be a hash (string) or height (number)");
    };

    let raw = match sdb.load_header(height) {
        Ok(Some(r)) => r,
        Ok(None)    => return RpcResponse::err(id, ERR_BLOCK_NOT_FOUND,
            format!("Block not found at height {}", height)),
        Err(e)      => return RpcResponse::err(id, ERR_INTERNAL, e.to_string()),
    };
    let hdr = match WireBlockHeader::from_bytes(&raw) {
        Ok(h)  => h,
        Err(e) => return RpcResponse::err(id, ERR_INTERNAL,
            format!("corrupt header: {:?}", e)),
    };
    let hash      = hex::encode(WireBlockHeader::block_hash_of_bytes(&raw));
    let prev_hash = hex::encode(hdr.prev_block);
    let merkle    = hex::encode(hdr.merkle_root);
    let tip       = sdb.get_sync_height().ok().flatten().unwrap_or(height);

    if verbosity == 0 {
        // verbosity 0: raw hex of the 80-byte wire header
        return RpcResponse::ok(id, json!(hex::encode(raw)));
    }

    RpcResponse::ok(id, json!({
        "hash":          hash,
        "height":        height,
        "confirmations": tip.saturating_sub(height) + 1,
        "version":       hdr.version,
        "previousblockhash": prev_hash,
        "merkleroot":    merkle,
        "time":          hdr.timestamp,
        "bits":          format!("{:08x}", hdr.bits),
        "nonce":         hdr.nonce,
        "difficulty":    bits_to_difficulty(hdr.bits),
        "chainwork":     "0".repeat(64),  // not tracked
        "nTx":           Value::Null,     // tx count not stored in header
    }))
}

// ── getrawtransaction ─────────────────────────────────────────────────────────

fn method_getrawtransaction(id: Value, params: &Value, state: &RpcState) -> RpcResponse {
    let txid = match params.get(0).and_then(|v| v.as_str()) {
        None    => return RpcResponse::err(id, ERR_INVALID_PARAMS,
            "getrawtransaction requires params: [txid: string, verbose?: bool]"),
        Some(s) => s.trim().to_lowercase(),
    };
    let verbose = params.get(1)
        .map(|v| v.as_bool().unwrap_or(false))
        .unwrap_or(false);

    // Mempool lookup (có raw bytes)
    if let Some(mdb) = state.open_mempool() {
        if let Some((raw, fee_rate, ts_ns)) = mdb.get_tx_raw(&txid) {
            if !verbose {
                return RpcResponse::ok(id, json!(hex::encode(&raw)));
            }
            let ts_secs = ts_ns / 1_000_000_000;
            return RpcResponse::ok(id, json!({
                "txid":              txid,
                "hex":               hex::encode(&raw),
                "size":              raw.len(),
                "fee_rate_msat_vb":  fee_rate,
                "time":              if ts_secs > 0 { json!(ts_secs) } else { Value::Null },
                "confirmations":     0,
                "status":            "mempool",
            }));
        }
    }

    RpcResponse::err(id, ERR_TX_NOT_FOUND,
        format!("No information available for transaction: {}", txid))
}

// ── getmininginfo ─────────────────────────────────────────────────────────────

fn method_getmininginfo(id: Value, state: &RpcState) -> RpcResponse {
    let sdb = match state.open_sync() {
        None    => return RpcResponse::err(id, ERR_NOT_SYNCED, "sync db not available"),
        Some(d) => d,
    };
    let height = sdb.get_sync_height().ok().flatten().unwrap_or(0);

    let (difficulty, networkhashps) = load_recent_headers(&sdb, 2)
        .ok()
        .and_then(|hdrs| {
            let cur  = hdrs.get(0)?;
            let prev = hdrs.get(1)?;
            let dt   = (cur.1.timestamp as i64 - prev.1.timestamp as i64).max(1) as f64;
            let diff = bits_to_difficulty(cur.1.bits);
            let hr   = estimate_hashrate_from(diff, dt);
            Some((diff, hr))
        })
        .unwrap_or((0.0, 0.0));

    RpcResponse::ok(id, json!({
        "blocks":        height,
        "difficulty":    difficulty,
        "networkhashps": networkhashps,
        "chain":         "testnet",
        "warnings":      "",
    }))
}

// ── getnetworkinfo ────────────────────────────────────────────────────────────

fn method_getnetworkinfo(id: Value, state: &RpcState) -> RpcResponse {
    let sdb        = state.open_sync();
    let synced     = sdb.as_ref()
        .and_then(|db| db.get_sync_height().ok().flatten())
        .map(|h| h > 0)
        .unwrap_or(false);
    let height     = sdb.as_ref()
        .and_then(|db| db.get_sync_height().ok().flatten())
        .unwrap_or(0);
    let mempool_cnt = state.open_mempool()
        .and_then(|mdb| mdb.count().ok())
        .unwrap_or(0);

    RpcResponse::ok(id, json!({
        "version":          190002,       // v19.2
        "subversion":       "/blockchain-rust:19.2/",
        "protocolversion":  70015,
        "localservices":    "0000000000000001",
        "localrelay":       true,
        "timeoffset":       0,
        "networkactive":    synced,
        "connections":      if synced { 1 } else { 0 },
        "relayfee":         0.00001,
        "incrementalfee":   0.00001,
        "chain":            "testnet",
        "blocks":           height,
        "mempool_count":    mempool_cnt,
        "warnings":         "",
    }))
}

// ── Axum handler ─────────────────────────────────────────────────────────────

async fn handle_rpc(
    State(state): State<RpcState>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Parse request body
    let req: RpcRequest = match serde_json::from_slice(&body) {
        Ok(r)  => r,
        Err(e) => {
            let resp = RpcResponse::err(
                Value::Null, ERR_PARSE,
                format!("Parse error: {}", e),
            );
            return (StatusCode::OK, Json(json!(resp))).into_response();
        }
    };

    // Validate jsonrpc version (warn but proceed)
    let response = dispatch(&req, &state);
    (StatusCode::OK, Json(json!(response))).into_response()
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Build JSON-RPC router với default DB paths.
pub fn rpc_router() -> Router {
    let state = RpcState {
        sync_path:    crate::pkt_testnet_web::default_sync_db_path(),
        mempool_path: crate::pkt_mempool_sync::default_mempool_db_path(),
    };
    Router::new()
        .route("/rpc", post(handle_rpc))
        .with_state(state)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn make_sync_db() -> SyncDb {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n   = SEQ.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("pkt_rpc_test_sync_{}_{}", pid, n));
        SyncDb::open(&path).unwrap()
    }

    fn make_state() -> (SyncDb, RpcState) {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n   = SEQ.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let sync_path    = std::env::temp_dir().join(format!("pkt_rpc_state_sync_{}_{}", pid, n));
        let mempool_path = std::env::temp_dir().join(format!("pkt_rpc_state_mem_{}_{}", pid, n));
        let sdb = SyncDb::open(&sync_path).unwrap();
        let state = RpcState { sync_path, mempool_path };
        (sdb, state)
    }

    fn req(method: &str, params: Value) -> RpcRequest {
        RpcRequest {
            jsonrpc: Some("2.0".to_string()),
            method:  method.to_string(),
            params,
            id:      Some(json!(1)),
        }
    }

    fn sample_header(ts: u32, merkle: u8) -> WireBlockHeader {
        WireBlockHeader {
            version: 1, prev_block: [0u8; 32],
            merkle_root: [merkle; 32],
            timestamp: ts, bits: 0x207fffff, nonce: 0,
        }
    }

    // ── RpcResponse helpers ───────────────────────────────────────────────────

    #[test]
    fn test_rpc_response_ok_has_result() {
        let r = RpcResponse::ok(json!(1), json!(42));
        assert!(r.result.is_some());
        assert!(r.error.is_none());
        assert_eq!(r.jsonrpc, "2.0");
    }

    #[test]
    fn test_rpc_response_err_has_error() {
        let r = RpcResponse::err(json!(1), -32601, "Method not found");
        assert!(r.result.is_none());
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, -32601);
    }

    // ── getblockcount ─────────────────────────────────────────────────────────

    #[test]
    fn test_getblockcount_empty_db_returns_zero() {
        let (_, state) = make_state();
        let r = dispatch(&req("getblockcount", json!([])), &state);
        assert!(r.result.is_some());
        assert_eq!(r.result.unwrap(), json!(0));
    }

    #[test]
    fn test_getblockcount_with_data() {
        let (sdb, state) = make_state();
        sdb.save_header(5, &sample_header(1_700_000_000, 1).to_bytes()).unwrap();
        sdb.set_sync_height(5).unwrap();
        let r = dispatch(&req("getblockcount", json!([])), &state);
        assert_eq!(r.result.unwrap(), json!(5));
    }

    // ── getblockhash ──────────────────────────────────────────────────────────

    #[test]
    fn test_getblockhash_valid_height() {
        let (sdb, state) = make_state();
        let hdr = sample_header(1_700_000_000, 2);
        sdb.save_header(10, &hdr.to_bytes()).unwrap();
        sdb.set_sync_height(10).unwrap();
        let r = dispatch(&req("getblockhash", json!([10])), &state);
        let hash = r.result.unwrap();
        let hash_str = hash.as_str().unwrap();
        assert_eq!(hash_str.len(), 64); // 32 bytes hex
    }

    #[test]
    fn test_getblockhash_missing_height_returns_error() {
        let (_, state) = make_state();
        let r = dispatch(&req("getblockhash", json!([9999])), &state);
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, ERR_BLOCK_NOT_FOUND);
    }

    #[test]
    fn test_getblockhash_no_params_returns_invalid_params() {
        let (_, state) = make_state();
        let r = dispatch(&req("getblockhash", json!([])), &state);
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, ERR_INVALID_PARAMS);
    }

    // ── getblock ──────────────────────────────────────────────────────────────

    #[test]
    fn test_getblock_by_height_returns_block() {
        let (sdb, state) = make_state();
        let hdr = sample_header(1_700_000_000, 3);
        sdb.save_header(7, &hdr.to_bytes()).unwrap();
        sdb.set_sync_height(7).unwrap();
        let r = dispatch(&req("getblock", json!([7, 1])), &state);
        let b = r.result.unwrap();
        assert_eq!(b["height"], json!(7));
        assert!(b["hash"].as_str().unwrap().len() == 64);
        assert!(b["difficulty"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn test_getblock_verbosity0_returns_hex() {
        let (sdb, state) = make_state();
        let hdr = sample_header(1_700_000_000, 4);
        sdb.save_header(1, &hdr.to_bytes()).unwrap();
        sdb.set_sync_height(1).unwrap();
        let r = dispatch(&req("getblock", json!([1, 0])), &state);
        let hex_str = r.result.unwrap();
        assert_eq!(hex_str.as_str().unwrap().len(), 160); // 80 bytes → 160 hex chars
    }

    #[test]
    fn test_getblock_by_hash() {
        let (sdb, state) = make_state();
        let hdr = sample_header(1_700_000_000, 5);
        let raw = hdr.to_bytes();
        sdb.save_header(3, &raw).unwrap();
        sdb.set_sync_height(3).unwrap();
        let hash = hex::encode(WireBlockHeader::block_hash_of_bytes(&raw));
        let r = dispatch(&req("getblock", json!([hash, 1])), &state);
        let b = r.result.unwrap();
        assert_eq!(b["height"], json!(3));
    }

    #[test]
    fn test_getblock_not_found_returns_error() {
        let (_, state) = make_state();
        let r = dispatch(&req("getblock", json!([9999])), &state);
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, ERR_BLOCK_NOT_FOUND);
    }

    // ── getmininginfo ─────────────────────────────────────────────────────────

    #[test]
    fn test_getmininginfo_has_required_fields() {
        let (_, state) = make_state();
        let r = dispatch(&req("getmininginfo", json!([])), &state);
        let v = r.result.unwrap();
        assert!(v.get("blocks").is_some());
        assert!(v.get("difficulty").is_some());
        assert!(v.get("networkhashps").is_some());
        assert_eq!(v["chain"], json!("testnet"));
    }

    // ── getnetworkinfo ────────────────────────────────────────────────────────

    #[test]
    fn test_getnetworkinfo_has_required_fields() {
        let (_, state) = make_state();
        let r = dispatch(&req("getnetworkinfo", json!([])), &state);
        let v = r.result.unwrap();
        assert!(v.get("version").is_some());
        assert!(v.get("subversion").is_some());
        assert!(v.get("connections").is_some());
        assert_eq!(v["chain"], json!("testnet"));
    }

    #[test]
    fn test_getnetworkinfo_version_number() {
        let (_, state) = make_state();
        let r = dispatch(&req("getnetworkinfo", json!([])), &state);
        assert_eq!(r.result.unwrap()["version"], json!(190002));
    }

    // ── sendrawtransaction ────────────────────────────────────────────────────

    #[test]
    fn test_sendrawtransaction_returns_unsupported() {
        let (_, state) = make_state();
        let r = dispatch(&req("sendrawtransaction", json!(["deadbeef"])), &state);
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, ERR_UNSUPPORTED);
    }

    // ── unknown method ────────────────────────────────────────────────────────

    #[test]
    fn test_unknown_method_returns_method_not_found() {
        let (_, state) = make_state();
        let r = dispatch(&req("unknownmethod", json!([])), &state);
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, ERR_METHOD_NOT_FOUND);
    }

    // ── getrawtransaction ─────────────────────────────────────────────────────

    #[test]
    fn test_getrawtransaction_not_found() {
        let (_, state) = make_state();
        let r = dispatch(&req("getrawtransaction", json!(["abcd1234"])), &state);
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, ERR_TX_NOT_FOUND);
    }

    #[test]
    fn test_getrawtransaction_no_params_returns_invalid() {
        let (_, state) = make_state();
        let r = dispatch(&req("getrawtransaction", json!([])), &state);
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, ERR_INVALID_PARAMS);
    }

    // ── Error constants ───────────────────────────────────────────────────────

    #[test]
    fn test_error_codes_standard() {
        assert_eq!(ERR_METHOD_NOT_FOUND, -32601);
        assert_eq!(ERR_INVALID_PARAMS,   -32602);
        assert_eq!(ERR_PARSE,            -32700);
    }
}
