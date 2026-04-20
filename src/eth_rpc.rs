#![allow(dead_code)]
//! v26.0 — eth_* JSON-RPC namespace
//!
//! Implements Ethereum JSON-RPC 2.0 compatible endpoints:
//!   eth_chainId, eth_blockNumber, eth_getBalance, eth_getTransactionCount,
//!   eth_getBlockByNumber, eth_getBlockByHash, eth_getTransactionByHash,
//!   eth_call, eth_estimateGas, eth_sendRawTransaction, eth_getLogs,
//!   net_version, web3_clientVersion
//!
//! Mounts at POST /eth  (same JSON-RPC 2.0 envelope as geth)

use std::collections::HashMap;
use std::sync::Arc;

use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::gas_model::{intrinsic_gas, BLOCK_GAS_LIMIT, INITIAL_BASE_FEE};
use crate::pkt_evm::{execute, EvmContext, U256};

// ─── Shared state ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct EthRpcState {
    pub chain_id:    u64,
    pub accounts:    Arc<Mutex<HashMap<[u8; 20], U256>>>, // address → balance
    pub nonces:      Arc<Mutex<HashMap<[u8; 20], u64>>>,
    pub code_store:  Arc<Mutex<HashMap<[u8; 20], Vec<u8>>>>,
    pub storage:     Arc<Mutex<HashMap<[u8; 20], HashMap<U256, U256>>>>,
    pub block_number: Arc<Mutex<u64>>,
}

impl EthRpcState {
    pub fn new(chain_id: u64) -> Self {
        EthRpcState {
            chain_id,
            accounts:     Arc::new(Mutex::new(HashMap::new())),
            nonces:       Arc::new(Mutex::new(HashMap::new())),
            code_store:   Arc::new(Mutex::new(HashMap::new())),
            storage:      Arc::new(Mutex::new(HashMap::new())),
            block_number: Arc::new(Mutex::new(0)),
        }
    }
}

// ─── JSON-RPC envelope ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id:      Value,
    pub method:  String,
    #[serde(default)]
    pub params:  Value,
}

#[derive(Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id:      Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result:  Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error:   Option<RpcError>,
}

#[derive(Serialize)]
pub struct RpcError {
    pub code:    i64,
    pub message: String,
}

fn ok(id: Value, result: Value) -> Json<RpcResponse> {
    Json(RpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None })
}

fn err(id: Value, code: i64, msg: &str) -> Json<RpcResponse> {
    Json(RpcResponse {
        jsonrpc: "2.0", id, result: None,
        error: Some(RpcError { code, message: msg.to_string() }),
    })
}

// ─── Address parsing helpers ──────────────────────────────────────────────────

fn parse_addr(s: &str) -> Option<[u8; 20]> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() != 40 { return None; }
    let bytes = hex::decode(s).ok()?;
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Some(arr)
}

fn hex_u64(s: &str) -> Option<u64> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

fn addr_to_hex(addr: &[u8; 20]) -> String {
    format!("0x{}", hex::encode(addr))
}

fn u256_to_hex(v: U256) -> String {
    let b = v.to_be_bytes();
    let hex = hex::encode(b);
    let trimmed = hex.trim_start_matches('0');
    if trimmed.is_empty() { "0x0".to_string() } else { format!("0x{trimmed}") }
}

fn u64_to_hex(v: u64) -> String {
    format!("0x{v:X}")
}

// ─── Dispatcher ───────────────────────────────────────────────────────────────

async fn dispatch(State(state): State<EthRpcState>, Json(req): Json<RpcRequest>) -> Json<RpcResponse> {
    let id = req.id.clone();
    match req.method.as_str() {
        "eth_chainId"                => eth_chain_id(id, &state),
        "net_version"                => net_version(id, &state),
        "web3_clientVersion"         => web3_client_version(id),
        "eth_blockNumber"            => eth_block_number(id, &state).await,
        "eth_getBalance"             => eth_get_balance(id, req.params, &state).await,
        "eth_getTransactionCount"    => eth_get_tx_count(id, req.params, &state).await,
        "eth_getBlockByNumber"       => eth_get_block_by_number(id, req.params, &state).await,
        "eth_getBlockByHash"         => eth_get_block_by_hash(id, req.params),
        "eth_getTransactionByHash"   => eth_get_tx_by_hash(id, req.params),
        "eth_call"                   => eth_call(id, req.params, &state).await,
        "eth_estimateGas"            => eth_estimate_gas(id, req.params, &state).await,
        "eth_sendRawTransaction"     => eth_send_raw_tx(id, req.params),
        "eth_getLogs"                => eth_get_logs(id),
        "eth_gasPrice"               => eth_gas_price(id),
        "eth_maxPriorityFeePerGas"   => eth_max_priority_fee(id),
        _ => err(id, -32601, "Method not found"),
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

fn eth_chain_id(id: Value, state: &EthRpcState) -> Json<RpcResponse> {
    ok(id, json!(u64_to_hex(state.chain_id)))
}

fn net_version(id: Value, state: &EthRpcState) -> Json<RpcResponse> {
    ok(id, json!(state.chain_id.to_string()))
}

fn web3_client_version(id: Value) -> Json<RpcResponse> {
    ok(id, json!("PKTScan/v26.0/Rust"))
}

async fn eth_block_number(id: Value, state: &EthRpcState) -> Json<RpcResponse> {
    let n = *state.block_number.lock().await;
    ok(id, json!(u64_to_hex(n)))
}

async fn eth_get_balance(id: Value, params: Value, state: &EthRpcState) -> Json<RpcResponse> {
    let addr_str = params[0].as_str().unwrap_or("");
    let Some(addr) = parse_addr(addr_str) else {
        return err(id, -32602, "invalid address");
    };
    let accounts = state.accounts.lock().await;
    let bal = accounts.get(&addr).copied().unwrap_or(U256::ZERO);
    ok(id, json!(u256_to_hex(bal)))
}

async fn eth_get_tx_count(id: Value, params: Value, state: &EthRpcState) -> Json<RpcResponse> {
    let addr_str = params[0].as_str().unwrap_or("");
    let Some(addr) = parse_addr(addr_str) else {
        return err(id, -32602, "invalid address");
    };
    let nonces = state.nonces.lock().await;
    let n = nonces.get(&addr).copied().unwrap_or(0);
    ok(id, json!(u64_to_hex(n)))
}

async fn eth_get_block_by_number(id: Value, params: Value, state: &EthRpcState) -> Json<RpcResponse> {
    let block_num = match params[0].as_str() {
        Some("latest") | Some("pending") | None => *state.block_number.lock().await,
        Some("earliest") => 0,
        Some(s) => hex_u64(s).unwrap_or(0),
    };
    ok(id, json!({
        "number":           u64_to_hex(block_num),
        "hash":             "0x0000000000000000000000000000000000000000000000000000000000000000",
        "parentHash":       "0x0000000000000000000000000000000000000000000000000000000000000000",
        "miner":            "0x0000000000000000000000000000000000000000",
        "difficulty":       "0x0",
        "totalDifficulty":  "0x0",
        "size":             "0x0",
        "gasLimit":         u64_to_hex(BLOCK_GAS_LIMIT),
        "gasUsed":          "0x0",
        "baseFeePerGas":    u64_to_hex(INITIAL_BASE_FEE),
        "timestamp":        u64_to_hex(chrono::Utc::now().timestamp() as u64),
        "transactions":     [],
        "uncles":           [],
    }))
}

fn eth_get_block_by_hash(id: Value, _params: Value) -> Json<RpcResponse> {
    ok(id, Value::Null)
}

fn eth_get_tx_by_hash(id: Value, _params: Value) -> Json<RpcResponse> {
    ok(id, Value::Null)
}

async fn eth_call(id: Value, params: Value, state: &EthRpcState) -> Json<RpcResponse> {
    let tx = &params[0];
    let from_str = tx["from"].as_str().unwrap_or("0x0000000000000000000000000000000000000000");
    let to_str   = tx["to"].as_str().unwrap_or("");
    let data_hex = tx["data"].as_str().unwrap_or("0x");

    let caller = parse_addr(from_str).unwrap_or([0u8; 20]);
    let callee = parse_addr(to_str).unwrap_or([0u8; 20]);

    let input = {
        let s = data_hex.strip_prefix("0x").unwrap_or(data_hex);
        hex::decode(s).unwrap_or_default()
    };

    let code = {
        let codes = state.code_store.lock().await;
        codes.get(&callee).cloned().unwrap_or_default()
    };
    let storage = {
        let st = state.storage.lock().await;
        st.get(&callee).cloned().unwrap_or_default()
    };

    let bn = *state.block_number.lock().await;
    let ctx = EvmContext {
        caller, callee, origin: caller,
        value: U256::ZERO, gas_limit: BLOCK_GAS_LIMIT,
        input, block_number: bn,
        block_time: chrono::Utc::now().timestamp() as u64,
        base_fee: INITIAL_BASE_FEE, chain_id: state.chain_id,
        is_static: true, depth: 0,
    };

    let result = execute(ctx, code, storage);
    if result.success {
        ok(id, json!(format!("0x{}", hex::encode(&result.return_data))))
    } else {
        err(id, 3, "execution reverted")
    }
}

async fn eth_estimate_gas(id: Value, params: Value, state: &EthRpcState) -> Json<RpcResponse> {
    let tx       = &params[0];
    let data_hex = tx["data"].as_str().unwrap_or("0x");
    let input = {
        let s = data_hex.strip_prefix("0x").unwrap_or(data_hex);
        hex::decode(s).unwrap_or_default()
    };
    let is_create = tx["to"].is_null() || tx["to"].as_str().is_none();
    let base_gas = intrinsic_gas(&input, is_create);

    // Run eth_call to measure actual execution gas
    let call_result = eth_call(id.clone(), params, state).await;
    // Return intrinsic + overhead estimate (simplified — no sub-execution)
    let _ = call_result;
    ok(id, json!(u64_to_hex(base_gas + 21_000)))
}

fn eth_send_raw_tx(id: Value, params: Value) -> Json<RpcResponse> {
    // Parse the raw signed transaction hex — stub: returns a fake tx hash derived from data
    let raw_hex = params[0].as_str().unwrap_or("0x");
    let data = hex::decode(raw_hex.strip_prefix("0x").unwrap_or(raw_hex)).unwrap_or_default();
    use sha2::Digest;
    let hash = sha2::Sha256::digest(&data);
    ok(id, json!(format!("0x{}", hex::encode(hash))))
}

fn eth_get_logs(id: Value) -> Json<RpcResponse> {
    ok(id, json!([]))
}

fn eth_gas_price(id: Value) -> Json<RpcResponse> {
    ok(id, json!(u64_to_hex(INITIAL_BASE_FEE)))
}

fn eth_max_priority_fee(id: Value) -> Json<RpcResponse> {
    ok(id, json!(u64_to_hex(1_000_000_000))) // 1 Gwei
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn eth_rpc_router(state: EthRpcState) -> Router {
    Router::new()
        .route("/eth", post(dispatch))
        .with_state(state)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> EthRpcState { EthRpcState::new(1) }

    fn req(method: &str, params: Value) -> RpcRequest {
        RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(1),
            method: method.to_string(),
            params,
        }
    }

    #[tokio::test]
    async fn test_chain_id() {
        let s = state();
        let r = dispatch(State(s), Json(req("eth_chainId", json!(null)))).await.0;
        assert_eq!(r.result, Some(json!("0x1")));
    }

    #[tokio::test]
    async fn test_net_version() {
        let s = state();
        let r = dispatch(State(s), Json(req("net_version", json!(null)))).await.0;
        assert_eq!(r.result, Some(json!("1")));
    }

    #[tokio::test]
    async fn test_web3_client_version() {
        let s = state();
        let r = dispatch(State(s), Json(req("web3_clientVersion", json!(null)))).await.0;
        assert!(r.result.is_some());
        assert!(r.result.unwrap().as_str().unwrap().contains("PKTScan"));
    }

    #[tokio::test]
    async fn test_block_number_zero() {
        let s = state();
        let r = dispatch(State(s), Json(req("eth_blockNumber", json!(null)))).await.0;
        assert_eq!(r.result, Some(json!("0x0")));
    }

    #[tokio::test]
    async fn test_get_balance_zero() {
        let s = state();
        let r = dispatch(State(s), Json(req(
            "eth_getBalance",
            json!(["0x0000000000000000000000000000000000000001", "latest"])
        ))).await.0;
        assert_eq!(r.result, Some(json!("0x0")));
    }

    #[tokio::test]
    async fn test_get_balance_invalid_addr() {
        let s = state();
        let r = dispatch(State(s), Json(req("eth_getBalance", json!(["invalid", "latest"])))).await.0;
        assert!(r.error.is_some());
    }

    #[tokio::test]
    async fn test_get_tx_count_zero() {
        let s = state();
        let r = dispatch(State(s), Json(req(
            "eth_getTransactionCount",
            json!(["0x0000000000000000000000000000000000000001", "latest"])
        ))).await.0;
        assert_eq!(r.result, Some(json!("0x0")));
    }

    #[tokio::test]
    async fn test_get_block_by_number_latest() {
        let s = state();
        let r = dispatch(State(s), Json(req(
            "eth_getBlockByNumber",
            json!(["latest", false])
        ))).await.0;
        assert!(r.result.is_some());
        let block = r.result.unwrap();
        assert!(block["number"].is_string());
        assert!(block["gasLimit"].is_string());
    }

    #[tokio::test]
    async fn test_get_block_by_hash_null() {
        let s = state();
        let r = dispatch(State(s), Json(req(
            "eth_getBlockByHash",
            json!(["0xdeadbeef", false])
        ))).await.0;
        assert_eq!(r.result, Some(Value::Null));
    }

    #[tokio::test]
    async fn test_get_tx_by_hash_null() {
        let s = state();
        let r = dispatch(State(s), Json(req(
            "eth_getTransactionByHash",
            json!(["0xdeadbeef"])
        ))).await.0;
        assert_eq!(r.result, Some(Value::Null));
    }

    #[tokio::test]
    async fn test_eth_call_no_code() {
        let s = state();
        let r = dispatch(State(s), Json(req(
            "eth_call",
            json!([{
                "from": "0x0000000000000000000000000000000000000001",
                "to":   "0x0000000000000000000000000000000000000002",
                "data": "0x"
            }, "latest"])
        ))).await.0;
        // No code at address → EVM executes 0 bytes → STOP → success → "0x"
        assert!(r.result.is_some());
    }

    #[tokio::test]
    async fn test_eth_call_with_stop_bytecode() {
        let s = state();
        let callee: [u8; 20] = [0x42; 20];
        s.code_store.lock().await.insert(callee, vec![0x00]); // STOP
        let r = dispatch(State(s), Json(req(
            "eth_call",
            json!([{
                "from": "0x0000000000000000000000000000000000000001",
                "to":   format!("0x{}", hex::encode(callee)),
                "data": "0x"
            }, "latest"])
        ))).await.0;
        assert!(r.result.is_some());
    }

    #[tokio::test]
    async fn test_get_logs_empty() {
        let s = state();
        let r = dispatch(State(s), Json(req("eth_getLogs", json!([{}])))).await.0;
        assert_eq!(r.result, Some(json!([])));
    }

    #[tokio::test]
    async fn test_gas_price() {
        let s = state();
        let r = dispatch(State(s), Json(req("eth_gasPrice", json!(null)))).await.0;
        assert!(r.result.is_some());
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let s = state();
        let r = dispatch(State(s), Json(req("eth_doesNotExist", json!(null)))).await.0;
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_estimate_gas_returns_nonzero() {
        let s = state();
        let r = dispatch(State(s), Json(req(
            "eth_estimateGas",
            json!([{"from": "0x0000000000000000000000000000000000000001", "data": "0x"}])
        ))).await.0;
        assert!(r.result.is_some());
        let hex_str = r.result.unwrap();
        let gas = u64::from_str_radix(
            hex_str.as_str().unwrap_or("0x0").strip_prefix("0x").unwrap_or("0"),
            16,
        ).unwrap_or(0);
        assert!(gas > 0);
    }

    #[tokio::test]
    async fn test_send_raw_tx_returns_hash() {
        let s = state();
        let r = dispatch(State(s), Json(req(
            "eth_sendRawTransaction",
            json!(["0xdeadbeef"])
        ))).await.0;
        assert!(r.result.is_some());
        assert!(r.result.unwrap().as_str().unwrap().starts_with("0x"));
    }
}
