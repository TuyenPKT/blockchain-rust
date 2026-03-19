#![allow(dead_code)]
//! v11.0 — Write API (authenticated POST endpoints)
//!
//! Tách biệt hoàn toàn khỏi read path (pktscan_api.rs):
//!   Read  path: GET  /api/*          → public, rate-limited
//!   Write path: POST /api/write/*    → yêu cầu X-API-Key với `write` role
//!
//! v11.0 scope: POST /api/write/tx
//!   - Validate input (không phải coinbase, fee > 0, outputs non-empty)
//!   - Verify script signature qua `Blockchain::validate_tx_script()`
//!   - Rate limit per API key: 60 req/phút
//!   - Audit log tự động qua audit_middleware đã mount ở pktscan_api::serve()
//!   - Yêu cầu `write` role
//!
//! v11.2 scope: POST /api/write/contract/deploy, POST /api/write/contract/call
//!   - Deploy: template name → WasmModule → ContractRegistry::deploy()
//!   - Call: address + function + args + gas_limit; dry_run → gas estimate only
//!   - Gas estimate: sum WasmInstr::gas_cost() trên function body
//!   - Yêu cầu `write` role + ECDSA signature
//!
//! Error codes:
//!   403 — thiếu write role
//!   429 — rate limit exceeded
//!   400 — invalid TX (coinbase / zero fee / empty outputs / script fail)
//!   200 — accepted into mempool

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    body::to_bytes,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde_json::json;
use tokio::sync::Mutex;

use serde::Deserialize;
use secp256k1::{PublicKey, Secp256k1};

use crate::api_auth::ApiRole;
use crate::contract_api::ContractDb;
use crate::script::Script;
use crate::smart_contract::{counter_contract, token_contract, voting_contract};
use crate::transaction::Transaction;

// ─── Rate limiter (per API key) ───────────────────────────────────────────────

/// Giới hạn write requests theo API key: `max_per_window` req / `window`.
pub struct WriteRateLimiter {
    /// key_id → (request count, window start)
    counters:       HashMap<String, (u32, Instant)>,
    pub max_per_window: u32,
    pub window:         Duration,
}

impl WriteRateLimiter {
    pub fn new(max_per_window: u32, window: Duration) -> Self {
        WriteRateLimiter { counters: HashMap::new(), max_per_window, window }
    }

    /// Kiểm tra + tăng counter. Trả về `true` nếu còn trong limit.
    pub fn check(&mut self, key_id: &str) -> bool {
        let now  = Instant::now();
        let entry = self.counters.entry(key_id.to_string()).or_insert((0, now));

        // Nếu window cũ → reset
        if now.duration_since(entry.1) >= self.window {
            *entry = (0, now);
        }

        if entry.0 >= self.max_per_window {
            return false;  // rate limit exceeded
        }
        entry.0 += 1;
        true
    }

    /// Reset counter cho key (dùng trong tests).
    pub fn reset(&mut self, key_id: &str) {
        self.counters.remove(key_id);
    }

    /// Số request đã dùng trong window hiện tại.
    pub fn used(&self, key_id: &str) -> u32 {
        self.counters.get(key_id).map(|(c, _)| *c).unwrap_or(0)
    }
}

impl Default for WriteRateLimiter {
    fn default() -> Self { Self::new(60, Duration::from_secs(60)) }
}

// ─── State ────────────────────────────────────────────────────────────────────

pub type WriteDb    = crate::pktscan_api::ScanDb;
pub type RateLimDb  = Arc<Mutex<WriteRateLimiter>>;

#[derive(Clone)]
pub struct WriteState {
    pub chain:    WriteDb,
    pub rate:     RateLimDb,
    pub contract: ContractDb,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn get_role(req: &Request) -> Option<&ApiRole> {
    req.extensions().get::<ApiRole>()
}

fn get_key_id(req: &Request) -> String {
    req.headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|k| k.chars().take(8).collect())
        .unwrap_or_else(|| "-".to_string())
}

fn err(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(json!({"error": msg}))).into_response()
}

/// Validate TX cơ bản — trả về `Err(reason)` nếu không hợp lệ.
pub fn validate_tx_basic(tx: &Transaction) -> Result<(), String> {
    if tx.is_coinbase {
        return Err("coinbase transactions not accepted via API".into());
    }
    if tx.outputs.is_empty() {
        return Err("transaction must have at least one output".into());
    }
    if tx.inputs.is_empty() {
        return Err("transaction must have at least one input".into());
    }
    if tx.fee == 0 {
        return Err("transaction fee must be > 0".into());
    }
    // Validate tx_id format (64-char hex)
    if tx.tx_id.len() != 64 || !tx.tx_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid tx_id format".into());
    }
    Ok(())
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// POST /api/write/tx
///
/// Body: JSON-serialized `Transaction`
/// Headers: `X-API-Key: <write-role key>`
///
/// Returns:
///   200 {"status":"accepted","tx_id":"..."} — added to mempool
///   400 {"error":"..."}                     — invalid TX
///   403 {"error":"write role required"}     — auth fail
///   429 {"error":"rate limit exceeded"}     — too many requests
async fn post_write_tx(
    State(state): State<WriteState>,
    req: Request,
) -> axum::response::Response {
    // ── 1. Auth check ─────────────────────────────────────────────────────────
    let role = match get_role(&req) {
        Some(r) if r.can_write() => r.clone(),
        _ => return err(StatusCode::FORBIDDEN, "write role required"),
    };
    let key_id = get_key_id(&req);

    // ── 2. Rate limit per API key ──────────────────────────────────────────────
    {
        let mut limiter = state.rate.lock().await;
        if !limiter.check(&key_id) {
            return err(StatusCode::TOO_MANY_REQUESTS,
                &format!("rate limit exceeded: max {} write requests per minute", limiter.max_per_window));
        }
    }

    // ── 3. Parse body ─────────────────────────────────────────────────────────
    let body = match to_bytes(req.into_body(), 256 * 1024).await {
        Ok(b)  => b,
        Err(_) => return err(StatusCode::BAD_REQUEST, "cannot read request body"),
    };
    let tx: Transaction = match serde_json::from_slice(&body) {
        Ok(t)  => t,
        Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid JSON: {e}")),
    };

    // ── 4. Basic validation ───────────────────────────────────────────────────
    if let Err(reason) = validate_tx_basic(&tx) {
        return err(StatusCode::BAD_REQUEST, &reason);
    }

    // ── 5. Script signature validation + mempool submission ──────────────────
    let tx_id = tx.tx_id.clone();
    let fee   = tx.fee;
    let output_total: u64 = tx.outputs.iter().map(|o| o.amount).sum();
    let input_total        = output_total + fee;

    let mut bc = state.chain.lock().await;

    // Verify script signatures (P2PKH / P2WPKH / P2TR / CTV)
    if !bc.verify_tx_scripts(&tx) {
        return err(StatusCode::BAD_REQUEST, "script signature verification failed");
    }

    match bc.mempool.add(tx, input_total) {
        Ok(_) => {
            tracing::info!(
                role = role.as_str(),
                key_id = key_id,
                tx_id = tx_id,
                fee = fee,
                "write_tx: accepted"
            );
            Json(json!({ "status": "accepted", "tx_id": tx_id, "fee": fee })).into_response()
        }
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

// ─── Token Write (v11.1) ──────────────────────────────────────────────────────

/// Request body cho POST /api/write/token/mint
#[derive(Debug, Deserialize)]
pub struct MintRequest {
    pub token_id:   String,
    pub to:         String,
    pub amount:     u128,
    /// Replay-protection nonce (monotonically increasing per pubkey recommended)
    pub nonce:      u64,
    /// Compressed secp256k1 pubkey hex (33 bytes = 66 chars) của token owner
    pub pubkey_hex: String,
    /// ECDSA signature hex over signing payload (see `mint_payload`)
    pub signature:  String,
}

/// Request body cho POST /api/write/token/transfer
#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub token_id:   String,
    pub from:       String,
    pub to:         String,
    pub amount:     u128,
    pub nonce:      u64,
    /// Compressed secp256k1 pubkey hex của sender (phải khớp with `from` address)
    pub pubkey_hex: String,
    pub signature:  String,
}

/// Compute signing payload cho mint operation.
pub fn mint_payload(token_id: &str, to: &str, amount: u128, nonce: u64) -> Vec<u8> {
    let mut v = b"token_mint_v111".to_vec();
    v.extend_from_slice(token_id.as_bytes());
    v.extend_from_slice(to.as_bytes());
    v.extend_from_slice(&amount.to_le_bytes());
    v.extend_from_slice(&nonce.to_le_bytes());
    v
}

/// Compute signing payload cho transfer operation.
pub fn transfer_payload(token_id: &str, from: &str, to: &str, amount: u128, nonce: u64) -> Vec<u8> {
    let mut v = b"token_transfer_v111".to_vec();
    v.extend_from_slice(token_id.as_bytes());
    v.extend_from_slice(from.as_bytes());
    v.extend_from_slice(to.as_bytes());
    v.extend_from_slice(&amount.to_le_bytes());
    v.extend_from_slice(&nonce.to_le_bytes());
    v
}

/// Derive canonical address (40-char hex) từ compressed pubkey hex.
/// address = hex(RIPEMD160(blake3(pubkey_bytes)))
pub fn pubkey_hex_to_address(pubkey_hex: &str) -> Result<String, String> {
    let pk_bytes = hex::decode(pubkey_hex).map_err(|_| "invalid pubkey_hex encoding".to_string())?;
    let _ = PublicKey::from_slice(&pk_bytes).map_err(|_| "invalid secp256k1 pubkey".to_string())?;
    let hash = Script::pubkey_hash(&pk_bytes);
    Ok(hex::encode(hash))
}

/// Verify ECDSA signature: blake3(payload) signed by pubkey.
/// Matches `Wallet::sign` / `Wallet::verify` convention.
pub fn verify_sig(pubkey_hex: &str, payload: &[u8], sig_hex: &str) -> Result<(), String> {
    let pk_bytes  = hex::decode(pubkey_hex).map_err(|_| "invalid pubkey_hex".to_string())?;
    let pk        = PublicKey::from_slice(&pk_bytes).map_err(|_| "invalid secp256k1 pubkey".to_string())?;
    let secp      = Secp256k1::new();
    let hash      = blake3::hash(payload);
    let msg       = secp256k1::Message::from_slice(hash.as_bytes()).map_err(|e| e.to_string())?;
    let sig_bytes = hex::decode(sig_hex).map_err(|_| "invalid signature encoding".to_string())?;
    let sig       = secp256k1::ecdsa::Signature::from_compact(&sig_bytes)
        .map_err(|_| "invalid ECDSA signature".to_string())?;
    secp.verify_ecdsa(&msg, &sig, &pk).map_err(|_| "signature verification failed".to_string())
}

/// POST /api/write/token/mint
/// Mint `amount` tokens to `to` — caller must be token owner (proven by ECDSA sig).
async fn post_token_mint(
    State(state): State<WriteState>,
    req: Request,
) -> axum::response::Response {
    // Auth + rate limit
    match get_role(&req) {
        Some(r) if r.can_write() => {}
        _ => return err(StatusCode::FORBIDDEN, "write role required"),
    }
    let key_id = get_key_id(&req);
    {
        let mut lim = state.rate.lock().await;
        if !lim.check(&key_id) {
            return err(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        }
    }

    // Parse body
    let body = match to_bytes(req.into_body(), 64 * 1024).await {
        Ok(b)  => b,
        Err(_) => return err(StatusCode::BAD_REQUEST, "cannot read request body"),
    };
    let r: MintRequest = match serde_json::from_slice(&body) {
        Ok(v)  => v,
        Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid JSON: {e}")),
    };

    // Input validation
    if r.amount == 0 { return err(StatusCode::BAD_REQUEST, "amount must be > 0"); }
    if r.to.is_empty() { return err(StatusCode::BAD_REQUEST, "to address required"); }

    // Derive owner address from pubkey
    let caller_addr = match pubkey_hex_to_address(&r.pubkey_hex) {
        Ok(a)  => a,
        Err(e) => return err(StatusCode::BAD_REQUEST, &e),
    };

    // Verify ECDSA signature
    let payload = mint_payload(&r.token_id, &r.to, r.amount, r.nonce);
    if let Err(e) = verify_sig(&r.pubkey_hex, &payload, &r.signature) {
        return err(StatusCode::BAD_REQUEST, &format!("signature invalid: {e}"));
    }

    // Mint via registry — owner must match caller_addr
    let mut bc = state.chain.lock().await;
    match bc.token_registry.mint_as_owner(&r.token_id, &caller_addr, &r.to, r.amount) {
        Ok(_)  => Json(json!({
            "status":   "minted",
            "token_id": r.token_id,
            "to":       r.to,
            "amount":   r.amount.to_string(),
        })).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

/// POST /api/write/token/transfer
/// Transfer `amount` tokens from `from` to `to` — sender must sign.
async fn post_token_transfer(
    State(state): State<WriteState>,
    req: Request,
) -> axum::response::Response {
    // Auth + rate limit
    match get_role(&req) {
        Some(r) if r.can_write() => {}
        _ => return err(StatusCode::FORBIDDEN, "write role required"),
    }
    let key_id = get_key_id(&req);
    {
        let mut lim = state.rate.lock().await;
        if !lim.check(&key_id) {
            return err(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        }
    }

    // Parse body
    let body = match to_bytes(req.into_body(), 64 * 1024).await {
        Ok(b)  => b,
        Err(_) => return err(StatusCode::BAD_REQUEST, "cannot read request body"),
    };
    let r: TransferRequest = match serde_json::from_slice(&body) {
        Ok(v)  => v,
        Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid JSON: {e}")),
    };

    // Input validation
    if r.amount == 0 { return err(StatusCode::BAD_REQUEST, "amount must be > 0"); }
    if r.from.is_empty() || r.to.is_empty() {
        return err(StatusCode::BAD_REQUEST, "from and to addresses required");
    }
    if r.from == r.to { return err(StatusCode::BAD_REQUEST, "from and to must differ"); }

    // Derive sender address from pubkey — must match `from`
    let sender_addr = match pubkey_hex_to_address(&r.pubkey_hex) {
        Ok(a)  => a,
        Err(e) => return err(StatusCode::BAD_REQUEST, &e),
    };
    if sender_addr != r.from {
        return err(StatusCode::BAD_REQUEST, "pubkey does not match from address");
    }

    // Verify ECDSA signature
    let payload = transfer_payload(&r.token_id, &r.from, &r.to, r.amount, r.nonce);
    if let Err(e) = verify_sig(&r.pubkey_hex, &payload, &r.signature) {
        return err(StatusCode::BAD_REQUEST, &format!("signature invalid: {e}"));
    }

    // Transfer via registry
    let mut bc = state.chain.lock().await;
    match bc.token_registry.transfer(&r.token_id, &r.from, &r.to, r.amount) {
        Ok(_)  => Json(json!({
            "status":   "transferred",
            "token_id": r.token_id,
            "from":     r.from,
            "to":       r.to,
            "amount":   r.amount.to_string(),
        })).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

// ─── Contract Write (v11.2) ───────────────────────────────────────────────────

/// Template names hỗ trợ khi deploy contract.
/// Mapping: tên chuỗi → WasmModule constructor.
const SUPPORTED_TEMPLATES: &[&str] = &["counter", "token", "voting"];

/// Request body cho POST /api/write/contract/deploy
#[derive(Debug, Deserialize)]
pub struct DeployRequest {
    /// Template: "counter" | "token" | "voting"
    pub template:   String,
    /// Địa chỉ của deployer (derived từ pubkey_hex)
    pub creator:    String,
    pub nonce:      u64,
    pub pubkey_hex: String,
    pub signature:  String,
}

/// Request body cho POST /api/write/contract/call
#[derive(Debug, Deserialize)]
pub struct CallRequest {
    pub address:    String,
    pub function:   String,
    pub args:       Vec<i64>,
    /// Gas limit cho call (max 1_000_000)
    pub gas_limit:  u64,
    /// Nếu true: chỉ ước tính gas, không commit state
    pub dry_run:    bool,
    pub nonce:      u64,
    pub pubkey_hex: String,
    pub signature:  String,
}

/// Signing payload cho deploy — chống replay giữa các networks.
pub fn deploy_payload(template: &str, creator: &str, nonce: u64) -> Vec<u8> {
    let mut v = b"contract_deploy_v112".to_vec();
    v.extend_from_slice(template.as_bytes());
    v.extend_from_slice(creator.as_bytes());
    v.extend_from_slice(&nonce.to_le_bytes());
    v
}

/// Signing payload cho call.
pub fn call_payload(address: &str, function: &str, args: &[i64], nonce: u64) -> Vec<u8> {
    let mut v = b"contract_call_v112".to_vec();
    v.extend_from_slice(address.as_bytes());
    v.extend_from_slice(function.as_bytes());
    for a in args { v.extend_from_slice(&a.to_le_bytes()); }
    v.extend_from_slice(&nonce.to_le_bytes());
    v
}

/// Ước tính gas cho một function dựa trên tổng gas_cost() của từng WasmInstr.
/// Trả về `None` nếu address hoặc function không tìm thấy.
pub fn estimate_gas_for(registry: &crate::smart_contract::ContractRegistry,
                         address: &str, fn_name: &str) -> Option<u64> {
    let instance = registry.contracts.get(address)?;
    let func     = instance.module.functions.get(fn_name)?;
    Some(func.body.iter().map(|i| i.gas_cost()).sum())
}

/// Chuyển template name → WasmModule. Trả về Err nếu template không hỗ trợ.
fn template_to_module(template: &str) -> Result<crate::smart_contract::WasmModule, String> {
    match template {
        "counter" => Ok(counter_contract()),
        "token"   => Ok(token_contract(0, 0)),
        "voting"  => Ok(voting_contract()),
        other     => Err(format!("unknown template '{}'; supported: {:?}", other, SUPPORTED_TEMPLATES)),
    }
}

/// POST /api/write/contract/deploy
/// Deploy contract từ built-in template — creator phải ký ECDSA.
async fn post_contract_deploy(
    State(state): State<WriteState>,
    req: Request,
) -> axum::response::Response {
    // Auth + rate limit
    match get_role(&req) {
        Some(r) if r.can_write() => {}
        _ => return err(StatusCode::FORBIDDEN, "write role required"),
    }
    let key_id = get_key_id(&req);
    {
        let mut lim = state.rate.lock().await;
        if !lim.check(&key_id) {
            return err(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        }
    }

    // Parse body
    let body = match to_bytes(req.into_body(), 64 * 1024).await {
        Ok(b)  => b,
        Err(_) => return err(StatusCode::BAD_REQUEST, "cannot read request body"),
    };
    let r: DeployRequest = match serde_json::from_slice(&body) {
        Ok(v)  => v,
        Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid JSON: {e}")),
    };

    // Validate template name
    if r.template.is_empty() { return err(StatusCode::BAD_REQUEST, "template required"); }

    // Derive creator address từ pubkey — phải khớp với r.creator
    let caller_addr = match pubkey_hex_to_address(&r.pubkey_hex) {
        Ok(a)  => a,
        Err(e) => return err(StatusCode::BAD_REQUEST, &e),
    };
    if caller_addr != r.creator {
        return err(StatusCode::BAD_REQUEST, "pubkey does not match creator address");
    }

    // Verify ECDSA signature
    let payload = deploy_payload(&r.template, &r.creator, r.nonce);
    if let Err(e) = verify_sig(&r.pubkey_hex, &payload, &r.signature) {
        return err(StatusCode::BAD_REQUEST, &format!("signature invalid: {e}"));
    }

    // Build WasmModule từ template
    let module = match template_to_module(&r.template) {
        Ok(m)  => m,
        Err(e) => return err(StatusCode::BAD_REQUEST, &e),
    };

    // Deploy vào registry
    let mut reg = state.contract.lock().await;
    let address = reg.deploy(module, &r.creator);

    tracing::info!(
        creator  = r.creator,
        template = r.template,
        address  = address,
        "contract_deploy: ok"
    );

    Json(json!({
        "status":   "deployed",
        "address":  address,
        "template": r.template,
        "creator":  r.creator,
    })).into_response()
}

/// POST /api/write/contract/call
/// Call contract function. Nếu `dry_run: true` → ước tính gas, không commit.
async fn post_contract_call(
    State(state): State<WriteState>,
    req: Request,
) -> axum::response::Response {
    // Auth + rate limit
    match get_role(&req) {
        Some(r) if r.can_write() => {}
        _ => return err(StatusCode::FORBIDDEN, "write role required"),
    }
    let key_id = get_key_id(&req);
    {
        let mut lim = state.rate.lock().await;
        if !lim.check(&key_id) {
            return err(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        }
    }

    // Parse body
    let body = match to_bytes(req.into_body(), 64 * 1024).await {
        Ok(b)  => b,
        Err(_) => return err(StatusCode::BAD_REQUEST, "cannot read request body"),
    };
    let r: CallRequest = match serde_json::from_slice(&body) {
        Ok(v)  => v,
        Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid JSON: {e}")),
    };

    // Input validation
    if r.address.is_empty()  { return err(StatusCode::BAD_REQUEST, "address required"); }
    if r.function.is_empty() { return err(StatusCode::BAD_REQUEST, "function required"); }
    let gas_limit = r.gas_limit.min(1_000_000).max(1_000);

    // Verify ECDSA signature
    let payload = call_payload(&r.address, &r.function, &r.args, r.nonce);
    if let Err(e) = verify_sig(&r.pubkey_hex, &payload, &r.signature) {
        return err(StatusCode::BAD_REQUEST, &format!("signature invalid: {e}"));
    }

    let mut reg = state.contract.lock().await;

    // dry_run → chỉ ước tính gas từ instruction count, không commit
    if r.dry_run {
        let gas_estimate = estimate_gas_for(&reg, &r.address, &r.function);
        return match gas_estimate {
            Some(est) => Json(json!({
                "status":       "dry_run",
                "gas_estimate": est,
                "function":     r.function,
                "address":      r.address,
            })).into_response(),
            None => err(StatusCode::BAD_REQUEST,
                &format!("contract or function not found: {}::{}", r.address, r.function)),
        };
    }

    // Live call — commit state
    match reg.call(&r.address, &r.function, r.args.clone(), gas_limit) {
        Ok(result) => {
            tracing::info!(
                address  = r.address,
                function = r.function,
                gas_used = result.gas_used,
                success  = result.success,
                "contract_call: ok"
            );
            Json(json!({
                "status":       if result.success { "ok" } else { "reverted" },
                "return_value": result.return_value,
                "gas_used":     result.gas_used,
                "storage_root": result.storage_root,
                "error":        result.error,
            })).into_response()
        }
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn write_router(chain: WriteDb, contract: ContractDb) -> Router {
    let state = WriteState {
        chain,
        rate: Arc::new(Mutex::new(WriteRateLimiter::default())),
        contract,
    };
    Router::new()
        .route("/api/write/tx",                post(post_write_tx))
        .route("/api/write/token/mint",        post(post_token_mint))
        .route("/api/write/token/transfer",    post(post_token_transfer))
        .route("/api/write/contract/deploy",   post(post_contract_deploy))
        .route("/api/write/contract/call",     post(post_contract_call))
        .with_state(state)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::{Transaction, TxInput, TxOutput};
    use crate::script::Script;

    // ── WriteRateLimiter ──────────────────────────────────────────────────────

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let mut lim = WriteRateLimiter::new(3, Duration::from_secs(60));
        assert!(lim.check("key1"));
        assert!(lim.check("key1"));
        assert!(lim.check("key1"));
    }

    #[test]
    fn test_rate_limiter_blocks_on_exceed() {
        let mut lim = WriteRateLimiter::new(2, Duration::from_secs(60));
        lim.check("key1");
        lim.check("key1");
        assert!(!lim.check("key1")); // 3rd request blocked
    }

    #[test]
    fn test_rate_limiter_independent_per_key() {
        let mut lim = WriteRateLimiter::new(1, Duration::from_secs(60));
        lim.check("key1");
        assert!(!lim.check("key1")); // key1 exhausted
        assert!(lim.check("key2"));  // key2 independent
    }

    #[test]
    fn test_rate_limiter_reset_clears_counter() {
        let mut lim = WriteRateLimiter::new(1, Duration::from_secs(60));
        lim.check("key1");
        assert!(!lim.check("key1")); // exhausted
        lim.reset("key1");
        assert!(lim.check("key1")); // allowed after reset
    }

    #[test]
    fn test_rate_limiter_used_count() {
        let mut lim = WriteRateLimiter::new(10, Duration::from_secs(60));
        assert_eq!(lim.used("key1"), 0);
        lim.check("key1");
        lim.check("key1");
        assert_eq!(lim.used("key1"), 2);
    }

    #[test]
    fn test_rate_limiter_window_reset_on_expiry() {
        // Window of 0 seconds → every check starts a new window
        let mut lim = WriteRateLimiter::new(1, Duration::from_secs(0));
        lim.check("key1"); // fills window
        // Immediately check again — window expired (0s), should reset
        assert!(lim.check("key1"));
    }

    // ── validate_tx_basic ────────────────────────────────────────────────────

    fn make_valid_tx() -> Transaction {
        Transaction {
            tx_id:      "a".repeat(64),
            wtx_id:     "b".repeat(64),
            inputs:     vec![TxInput {
                tx_id:        "c".repeat(64),
                output_index: 0,
                script_sig:   Script::empty(),
                sequence:     0xFFFFFFFF,
                witness:      vec![],
            }],
            outputs:    vec![TxOutput::p2pkh(1000, &"d".repeat(40))],
            is_coinbase: false,
            fee:         100,
        }
    }

    #[test]
    fn test_validate_tx_valid() {
        let tx = make_valid_tx();
        assert!(validate_tx_basic(&tx).is_ok());
    }

    #[test]
    fn test_validate_tx_rejects_coinbase() {
        let mut tx = make_valid_tx();
        tx.is_coinbase = true;
        let err = validate_tx_basic(&tx).unwrap_err();
        assert!(err.contains("coinbase"));
    }

    #[test]
    fn test_validate_tx_rejects_empty_outputs() {
        let mut tx = make_valid_tx();
        tx.outputs.clear();
        let err = validate_tx_basic(&tx).unwrap_err();
        assert!(err.contains("output"));
    }

    #[test]
    fn test_validate_tx_rejects_empty_inputs() {
        let mut tx = make_valid_tx();
        tx.inputs.clear();
        let err = validate_tx_basic(&tx).unwrap_err();
        assert!(err.contains("input"));
    }

    #[test]
    fn test_validate_tx_rejects_zero_fee() {
        let mut tx = make_valid_tx();
        tx.fee = 0;
        let err = validate_tx_basic(&tx).unwrap_err();
        assert!(err.contains("fee"));
    }

    #[test]
    fn test_validate_tx_rejects_invalid_txid_length() {
        let mut tx = make_valid_tx();
        tx.tx_id = "abc".into(); // too short
        let err = validate_tx_basic(&tx).unwrap_err();
        assert!(err.contains("tx_id"));
    }

    #[test]
    fn test_validate_tx_rejects_invalid_txid_chars() {
        let mut tx = make_valid_tx();
        tx.tx_id = "z".repeat(64); // non-hex chars
        let err = validate_tx_basic(&tx).unwrap_err();
        assert!(err.contains("tx_id"));
    }

    // ── Role helpers ─────────────────────────────────────────────────────────

    #[test]
    fn test_write_role_can_write() {
        assert!(ApiRole::Write.can_write());
        assert!(ApiRole::Admin.can_write());
        assert!(!ApiRole::Read.can_write());
    }

    // ── WriteRateLimiter default ─────────────────────────────────────────────

    #[test]
    fn test_default_rate_limiter_params() {
        let lim = WriteRateLimiter::default();
        assert_eq!(lim.max_per_window, 60);
        assert_eq!(lim.window, Duration::from_secs(60));
    }

    // ── Token Write helpers (v11.1) ───────────────────────────────────────────

    fn make_keypair() -> (secp256k1::SecretKey, secp256k1::PublicKey) {
        let secp = Secp256k1::new();
        secp.generate_keypair(&mut secp256k1::rand::thread_rng())
    }

    fn sign_payload(sk: &secp256k1::SecretKey, payload: &[u8]) -> String {
        let secp = Secp256k1::new();
        let hash = blake3::hash(payload);
        let msg  = secp256k1::Message::from_slice(hash.as_bytes()).unwrap();
        let sig  = secp.sign_ecdsa(&msg, sk);
        hex::encode(sig.serialize_compact())
    }

    #[test]
    fn test_mint_payload_deterministic() {
        let a = mint_payload("PKT", "addr1", 1000, 1);
        let b = mint_payload("PKT", "addr1", 1000, 1);
        assert_eq!(a, b);
    }

    #[test]
    fn test_mint_payload_different_nonce() {
        let a = mint_payload("PKT", "addr1", 1000, 1);
        let b = mint_payload("PKT", "addr1", 1000, 2);
        assert_ne!(a, b);
    }

    #[test]
    fn test_transfer_payload_deterministic() {
        let a = transfer_payload("PKT", "from1", "to1", 500, 7);
        let b = transfer_payload("PKT", "from1", "to1", 500, 7);
        assert_eq!(a, b);
    }

    #[test]
    fn test_transfer_payload_different_from_to() {
        let a = transfer_payload("PKT", "from1", "to1", 500, 1);
        let b = transfer_payload("PKT", "from2", "to1", 500, 1);
        assert_ne!(a, b);
    }

    #[test]
    fn test_pubkey_hex_to_address_valid() {
        let (_, pk) = make_keypair();
        let pk_hex  = hex::encode(pk.serialize());
        let addr    = pubkey_hex_to_address(&pk_hex).unwrap();
        // address = RIPEMD160(blake3(pubkey)) = 20 bytes = 40 hex chars
        assert_eq!(addr.len(), 40);
        assert!(addr.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_pubkey_hex_to_address_invalid_hex() {
        let err = pubkey_hex_to_address("zzzzzz").unwrap_err();
        assert!(err.contains("invalid pubkey_hex encoding"));
    }

    #[test]
    fn test_pubkey_hex_to_address_invalid_pubkey() {
        // Valid hex but not a valid EC point
        let err = pubkey_hex_to_address(&"aa".repeat(33)).unwrap_err();
        assert!(err.contains("invalid secp256k1 pubkey"));
    }

    #[test]
    fn test_pubkey_hex_to_address_deterministic() {
        let (_, pk) = make_keypair();
        let pk_hex  = hex::encode(pk.serialize());
        let addr1   = pubkey_hex_to_address(&pk_hex).unwrap();
        let addr2   = pubkey_hex_to_address(&pk_hex).unwrap();
        assert_eq!(addr1, addr2);
    }

    #[test]
    fn test_verify_sig_valid() {
        let (sk, pk) = make_keypair();
        let pk_hex   = hex::encode(pk.serialize());
        let payload  = b"hello world";
        let sig_hex  = sign_payload(&sk, payload);
        assert!(verify_sig(&pk_hex, payload, &sig_hex).is_ok());
    }

    #[test]
    fn test_verify_sig_wrong_payload() {
        let (sk, pk) = make_keypair();
        let pk_hex   = hex::encode(pk.serialize());
        let sig_hex  = sign_payload(&sk, b"correct payload");
        let result   = verify_sig(&pk_hex, b"wrong payload", &sig_hex);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_sig_wrong_key() {
        let (sk, _)  = make_keypair();
        let (_, pk2) = make_keypair();
        let pk2_hex  = hex::encode(pk2.serialize());
        let sig_hex  = sign_payload(&sk, b"data");
        let result   = verify_sig(&pk2_hex, b"data", &sig_hex);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_sig_invalid_sig_hex() {
        let (_, pk) = make_keypair();
        let pk_hex  = hex::encode(pk.serialize());
        let result  = verify_sig(&pk_hex, b"data", "notvalidhex!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_sig_with_mint_payload() {
        let (sk, pk) = make_keypair();
        let pk_hex   = hex::encode(pk.serialize());
        let payload  = mint_payload("PKT", "someaddr", 9999, 42);
        let sig_hex  = sign_payload(&sk, &payload);
        assert!(verify_sig(&pk_hex, &payload, &sig_hex).is_ok());
    }

    #[test]
    fn test_verify_sig_with_transfer_payload() {
        let (sk, pk) = make_keypair();
        let pk_hex   = hex::encode(pk.serialize());
        let payload  = transfer_payload("PKT", "from_addr", "to_addr", 100, 1);
        let sig_hex  = sign_payload(&sk, &payload);
        assert!(verify_sig(&pk_hex, &payload, &sig_hex).is_ok());
    }

    // ── Contract Write helpers (v11.2) ────────────────────────────────────────

    use crate::smart_contract::{ContractRegistry, counter_contract};

    #[test]
    fn test_deploy_payload_deterministic() {
        let a = deploy_payload("counter", "addr1", 1);
        let b = deploy_payload("counter", "addr1", 1);
        assert_eq!(a, b);
    }

    #[test]
    fn test_deploy_payload_different_template() {
        let a = deploy_payload("counter", "addr1", 1);
        let b = deploy_payload("token",   "addr1", 1);
        assert_ne!(a, b);
    }

    #[test]
    fn test_deploy_payload_different_nonce() {
        let a = deploy_payload("counter", "addr1", 1);
        let b = deploy_payload("counter", "addr1", 2);
        assert_ne!(a, b);
    }

    #[test]
    fn test_call_payload_deterministic() {
        let a = call_payload("0xabc", "increment", &[1, 2], 5);
        let b = call_payload("0xabc", "increment", &[1, 2], 5);
        assert_eq!(a, b);
    }

    #[test]
    fn test_call_payload_different_args() {
        let a = call_payload("0xabc", "increment", &[1], 5);
        let b = call_payload("0xabc", "increment", &[2], 5);
        assert_ne!(a, b);
    }

    #[test]
    fn test_template_to_module_counter() {
        let m = template_to_module("counter").unwrap();
        assert_eq!(m.name, "Counter");
        assert!(m.exports.contains(&"increment".to_string()));
    }

    #[test]
    fn test_template_to_module_token() {
        let m = template_to_module("token").unwrap();
        assert_eq!(m.name, "Token");
    }

    #[test]
    fn test_template_to_module_voting() {
        let m = template_to_module("voting").unwrap();
        assert_eq!(m.name, "Voting");
    }

    #[test]
    fn test_template_to_module_unknown() {
        let err = template_to_module("foobar").unwrap_err();
        assert!(err.contains("unknown template"));
    }

    #[test]
    fn test_estimate_gas_for_counter() {
        let mut reg = ContractRegistry::new();
        let module  = counter_contract();
        let addr    = reg.deploy(module, "creator");
        let gas     = estimate_gas_for(&reg, &addr, "increment");
        assert!(gas.is_some());
        assert!(gas.unwrap() > 0);
    }

    #[test]
    fn test_estimate_gas_for_unknown_function() {
        let mut reg = ContractRegistry::new();
        let module  = counter_contract();
        let addr    = reg.deploy(module, "creator");
        let gas     = estimate_gas_for(&reg, &addr, "nonexistent_fn");
        assert!(gas.is_none());
    }

    #[test]
    fn test_estimate_gas_for_unknown_address() {
        let reg  = ContractRegistry::new();
        let gas  = estimate_gas_for(&reg, "0xdeadbeef", "increment");
        assert!(gas.is_none());
    }

    #[test]
    fn test_verify_sig_with_deploy_payload() {
        let (sk, pk) = make_keypair();
        let pk_hex   = hex::encode(pk.serialize());
        let payload  = deploy_payload("counter", "some_addr", 42);
        let sig_hex  = sign_payload(&sk, &payload);
        assert!(verify_sig(&pk_hex, &payload, &sig_hex).is_ok());
    }

    #[test]
    fn test_verify_sig_with_call_payload() {
        let (sk, pk) = make_keypair();
        let pk_hex   = hex::encode(pk.serialize());
        let payload  = call_payload("0xabc123", "increment", &[], 10);
        let sig_hex  = sign_payload(&sk, &payload);
        assert!(verify_sig(&pk_hex, &payload, &sig_hex).is_ok());
    }
}
