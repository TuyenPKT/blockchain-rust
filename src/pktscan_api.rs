#![allow(dead_code)]
//! v9.6 — PKTScan REST Backend + Tx Filter + CORS Allowlist
//!
//! Axum server phục vụ `index.html` (PKTScan frontend) với đầy đủ
//! dữ liệu blockchain qua JSON API.
//!
//! Endpoints:
//!   GET /                         → serve index.html (or built-in fallback)
//!   GET /api/stats                → network stats
//!   GET /api/blocks               → latest blocks (limit/offset/cursor)
//!   GET /api/block/:height        → block detail
//!   GET /api/txs                  → transactions với filter: min_amount/max_amount/since/until
//!   GET /api/tx/:txid             → transaction detail + status/confirmations
//!   GET /api/address/:addr        → balance + UTXOs + tx history
//!   GET /api/mempool              → pending transactions + fee stats
//!   GET /api/search?q=            → search blocks/txs/addresses
//!   GET /api/analytics/:metric    → chain analytics time series
//!   GET /api/blocks.csv           → CSV export of blocks
//!   GET /api/txs.csv              → CSV export of transactions
//!   GET /api/pool/stats           → mining pool stats
//!   GET /api/pool/miners          → per-miner breakdown
//!   WS  /ws                       → live block/tx feed
//!
//! Caching: GET /api/* responses are cached for 5 s with ETag/304 support.
//! CORS: allowlist-based (configurable via CorsConfig), default: localhost:3000/8080 + pktscan.io
//! CLI: `cargo run -- pktscan [port]` (default 8080)

use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::chain::Blockchain;

pub type ScanDb  = Arc<Mutex<Blockchain>>;
pub type CacheDb = Arc<Mutex<crate::response_cache::ResponseCache>>;

// ─── CORS Config + Middleware (v9.6) ─────────────────────────────────────────

/// Allowlist-based CORS config.  Default origins: localhost + pktscan.io.
/// Pass `"*"` as a single entry to allow all origins (wildcard).
#[derive(Debug, Clone)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
}

impl CorsConfig {
    pub fn new(origins: Vec<impl Into<String>>) -> Self {
        CorsConfig { allowed_origins: origins.into_iter().map(|s| s.into()).collect() }
    }

    /// Allow a specific origin?
    pub fn is_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.iter().any(|o| o == "*" || o == origin)
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        CorsConfig {
            allowed_origins: vec![
                "http://localhost:3000".to_string(),
                "http://localhost:8080".to_string(),
                "https://pktscan.io".to_string(),
            ],
        }
    }
}

pub type CorsState = Arc<CorsConfig>;

/// CORS middleware: reflect allowed origin, reject others (no header set).
async fn cors_layer(cors: Arc<CorsConfig>, request: Request<Body>, next: Next) -> Response {
    let origin = request
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    if origin.is_empty() || cors.is_allowed(&origin) {
        let allow_val = if origin.is_empty() { "*".to_string() } else { origin };
        if let Ok(hv) = HeaderValue::from_str(&allow_val) {
            headers.insert("Access-Control-Allow-Origin", hv);
        }
        if !allow_val.eq("*") {
            headers.insert("Vary", HeaderValue::from_static("Origin"));
        }
    }
    headers.insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("GET, OPTIONS"),
    );
    headers.insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("Content-Type"),
    );
    response
}

// ─── Query params ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PageParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
    /// Cursor: start from this block height (inclusive), going backwards.
    /// Overrides `offset` when present.
    pub from: Option<u64>,
}
fn default_limit() -> usize { 20 }

/// Query params cho GET /api/txs với bộ lọc bổ sung (v9.6).
#[derive(Debug, Deserialize)]
pub struct TxFilterParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
    /// Cursor: bắt đầu từ block height này (giảm dần).
    pub from: Option<u64>,
    /// Chỉ trả về txs có output_total >= min_amount (satoshi).
    pub min_amount: Option<u64>,
    /// Chỉ trả về txs có output_total <= max_amount (satoshi).
    pub max_amount: Option<u64>,
    /// Chỉ trả về txs có block_timestamp >= since (unix seconds).
    pub since: Option<i64>,
    /// Chỉ trả về txs có block_timestamp <= until (unix seconds).
    pub until: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}
fn default_search_limit() -> usize { 10 }

#[derive(Debug, Deserialize)]
pub struct AnalyticsParams {
    #[serde(default = "default_analytics_window")]
    pub window: usize,
}
fn default_analytics_window() -> usize { 100 }

// ─── Cache Middleware (v8.8) ──────────────────────────────────────────────────

/// Per-request middleware: cache GET /api/* responses for TTL seconds.
/// Returns 304 Not Modified when the client sends a matching ETag.
pub async fn api_cache_middleware(
    cache:   CacheDb,
    request: Request<Body>,
    next:    Next,
) -> Response {
    // Only cache GET /api/* requests
    let is_api_get = request.method() == Method::GET
        && request.uri().path().starts_with("/api/");
    if !is_api_get {
        return next.run(request).await;
    }

    let key = request.uri().to_string();

    // Extract If-None-Match from client
    let client_etag: Option<String> = request
        .headers()
        .get("if-none-match")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Cache hit?
    {
        let guard = cache.lock().await;
        if let Some(entry) = guard.get(&key) {
            if client_etag.as_deref() == Some(entry.etag.as_str()) {
                let mut r = StatusCode::NOT_MODIFIED.into_response();
                if let Ok(hv) = HeaderValue::from_str(&entry.etag) {
                    r.headers_mut().insert("ETag", hv);
                }
                return r;
            }
            let etag  = entry.etag.clone();
            let body  = entry.body.clone();
            let mut r = (StatusCode::OK, body).into_response();
            r.headers_mut().insert("Content-Type", HeaderValue::from_static("application/json"));
            if let Ok(hv) = HeaderValue::from_str(&etag) {
                r.headers_mut().insert("ETag", hv);
            }
            r.headers_mut().insert("X-Cache", HeaderValue::from_static("HIT"));
            return r;
        }
    }

    // Cache miss — run the real handler
    let response = next.run(request).await;

    if response.status() == StatusCode::OK {
        let (parts, body_stream) = response.into_parts();
        match axum::body::to_bytes(body_stream, 4 * 1024 * 1024).await {
            Ok(bytes) => {
                let body_str = String::from_utf8_lossy(&bytes).to_string();
                let etag     = crate::response_cache::ResponseCache::make_etag(&body_str);
                cache.lock().await.set(key, body_str);
                let mut r = Response::from_parts(parts, Body::from(bytes));
                if let Ok(hv) = HeaderValue::from_str(&etag) {
                    r.headers_mut().insert("ETag", hv);
                }
                r.headers_mut().insert("X-Cache", HeaderValue::from_static("MISS"));
                r
            }
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    } else {
        response
    }
}

// ─── / (static index) v8.9 ───────────────────────────────────────────────────

/// Serve `index.html` from the working directory, or a built-in fallback page.
async fn serve_index() -> impl IntoResponse {
    // Dùng embedded bytes từ web_frontend (compile-time) — không cần filesystem
    crate::web_frontend::embedded_index_handler().await
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn router(state: ScanDb) -> Router {
    Router::new()
        .route("/",                           get(serve_index))
        .route("/api/stats",                  get(get_stats))
        .route("/api/blocks",                 get(get_blocks))
        .route("/api/block/:height",          get(get_block))
        .route("/api/txs",                    get(get_txs))
        .route("/api/tx/:txid",               get(get_tx))
        .route("/api/address/:addr",          get(get_address))
        .route("/api/mempool",                get(get_mempool))
        .route("/api/search",                 get(get_search))
        .route("/api/analytics/:metric",      get(get_analytics))
        .route("/api/blocks.csv",             get(get_blocks_csv))
        .route("/api/txs.csv",                get(get_txs_csv))
        .with_state(state)
}

// ─── /api/stats ───────────────────────────────────────────────────────────────

async fn get_stats(State(db): State<ScanDb>) -> Json<Value> {
    let bc = db.lock().await;
    let height = bc.chain.len().saturating_sub(1) as u64;

    let avg_block_time = avg_block_time_secs(&bc.chain);
    let hashrate = estimate_hashrate(bc.difficulty, avg_block_time);
    let block_reward = crate::reward::RewardEngine::subsidy_at(height);

    Json(json!({
        "height":          height,
        "difficulty":      bc.difficulty,
        "hashrate":        hashrate,
        "block_reward":    block_reward,
        "total_supply":    bc.utxo_set.total_supply(),
        "utxo_count":      bc.utxo_set.utxos.len(),
        "mempool_count":   bc.mempool.entries.len(),
        "avg_block_time_s": avg_block_time,
        "block_count":     bc.chain.len(),
    }))
}

// ─── /api/blocks ──────────────────────────────────────────────────────────────

async fn get_blocks(
    State(db): State<ScanDb>,
    Query(page): Query<PageParams>,
) -> Json<Value> {
    let bc         = db.lock().await;
    let limit      = page.limit.min(100);
    let total      = bc.chain.len();
    let difficulty = bc.difficulty;

    let (blocks, next_cursor) = if let Some(from) = page.from {
        // Cursor-based pagination
        let slice = crate::pagination::paginate_blocks(&bc.chain, Some(from), limit);
        let next  = crate::pagination::next_block_cursor(&bc.chain, Some(from), limit);
        let blks: Vec<Value> = slice.iter().rev().map(|b| block_summary(b, difficulty)).collect();
        (blks, next)
    } else {
        // Offset-based (legacy)
        let blks: Vec<Value> = bc.chain.iter().rev()
            .skip(page.offset)
            .take(limit)
            .map(|b| block_summary(b, difficulty))
            .collect();
        (blks, None)
    };

    Json(json!({
        "blocks":      blocks,
        "total":       total,
        "limit":       limit,
        "offset":      page.offset,
        "next_cursor": next_cursor,
    }))
}

// ─── /api/block/:height ───────────────────────────────────────────────────────

async fn get_block(
    State(db): State<ScanDb>,
    Path(height): Path<u64>,
) -> (StatusCode, Json<Value>) {
    let bc = db.lock().await;
    let difficulty = bc.difficulty;
    match bc.chain.iter().find(|b| b.index == height) {
        Some(block) => {
            let reward = crate::reward::RewardEngine::subsidy_at(block.index);
            let miner  = miner_from_block(block);
            (StatusCode::OK, Json(json!({
                "index":        block.index,
                "hash":         block.hash,
                "prev_hash":    block.prev_hash,
                "timestamp":    block.timestamp,
                "nonce":        block.nonce,
                "witness_root": block.witness_root,
                "tx_count":     block.transactions.len(),
                "transactions": block.transactions.iter().map(tx_summary).collect::<Vec<_>>(),
                "reward":       reward,
                "miner":        miner,
                "difficulty":   difficulty,
            })))
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("block {} not found", height) })),
        ),
    }
}

// ─── /api/txs ─────────────────────────────────────────────────────────────────

async fn get_txs(
    State(db): State<ScanDb>,
    Query(f): Query<TxFilterParams>,
) -> Json<Value> {
    let bc    = db.lock().await;
    let limit = f.limit.min(100);
    let total_unfiltered: usize = bc.chain.iter().map(|b| b.transactions.len()).sum();

    // Collect all candidates (cursor or offset)
    let candidates: Vec<Value> = if let Some(from) = f.from {
        crate::pagination::paginate_txs(&bc.chain, Some(from), usize::MAX)
            .iter()
            .map(|r| {
                let mut v = tx_summary(r.tx);
                v["block_height"]    = json!(r.block_height);
                v["block_timestamp"] = json!(r.block_timestamp);
                v
            })
            .collect()
    } else {
        bc.chain.iter().rev()
            .flat_map(|b| b.transactions.iter().map(move |tx| {
                let mut v = tx_summary(tx);
                v["block_height"]    = json!(b.index);
                v["block_timestamp"] = json!(b.timestamp);
                v
            }))
            .collect()
    };

    // Apply filters
    let filtered: Vec<Value> = candidates.into_iter()
        .filter(|v| {
            let amount = v["output_total"].as_u64().unwrap_or(0);
            let ts     = v["block_timestamp"].as_i64().unwrap_or(0);
            if let Some(min) = f.min_amount { if amount < min { return false; } }
            if let Some(max) = f.max_amount { if amount > max { return false; } }
            if let Some(since) = f.since    { if ts < since   { return false; } }
            if let Some(until) = f.until    { if ts > until   { return false; } }
            true
        })
        .collect();

    let total_filtered = filtered.len();
    let txs: Vec<Value> = filtered.into_iter()
        .skip(f.offset)
        .take(limit)
        .collect();

    Json(json!({
        "txs":            txs,
        "total":          total_unfiltered,
        "total_filtered": total_filtered,
        "limit":          limit,
        "offset":         f.offset,
        "filter": {
            "min_amount": f.min_amount,
            "max_amount": f.max_amount,
            "since":      f.since,
            "until":      f.until,
        },
    }))
}

// ─── /api/tx/:txid ────────────────────────────────────────────────────────────

async fn get_tx(
    State(db): State<ScanDb>,
    Path(txid): Path<String>,
) -> (StatusCode, Json<Value>) {
    let bc = db.lock().await;
    let tip_height = bc.chain.len().saturating_sub(1) as u64;

    // Search confirmed blocks (newest first for speed)
    for block in bc.chain.iter().rev() {
        if let Some(tx) = block.transactions.iter().find(|t| t.tx_id == txid) {
            let output_total: u64 = tx.outputs.iter().map(|o| o.amount).sum();
            let confirmations = tip_height.saturating_sub(block.index) + 1;
            let from = if tx.is_coinbase { "coinbase".to_string() }
                       else { tx.inputs.first().map(|i| i.tx_id.clone()).unwrap_or_default() };
            let to = tx.outputs.first().map(addr_from_output).unwrap_or_default();
            return (StatusCode::OK, Json(json!({
                "tx_id":          tx.tx_id,
                "wtx_id":         tx.wtx_id,
                "is_coinbase":    tx.is_coinbase,
                "fee":            tx.fee,
                "output_total":   output_total,
                "from":           from,
                "to":             to,
                "inputs":         tx.inputs,
                "outputs":        tx.outputs,
                "block_height":   block.index,
                "block_hash":     block.hash,
                "timestamp":      block.timestamp,
                "status":         "confirmed",
                "confirmations":  confirmations,
            })));
        }
    }

    // Search mempool (pending)
    if let Some(entry) = bc.mempool.entries.get(&txid) {
        let output_total: u64 = entry.tx.outputs.iter().map(|o| o.amount).sum();
        return (StatusCode::OK, Json(json!({
            "tx_id":         entry.tx.tx_id,
            "wtx_id":        entry.tx.wtx_id,
            "is_coinbase":   entry.tx.is_coinbase,
            "fee":           entry.fee,
            "output_total":  output_total,
            "inputs":        entry.tx.inputs,
            "outputs":       entry.tx.outputs,
            "block_height":  null,
            "block_hash":    null,
            "timestamp":     null,
            "status":        "pending",
            "confirmations": 0,
        })));
    }

    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": format!("tx {} not found", txid) })),
    )
}

// ─── /api/address/:addr ───────────────────────────────────────────────────────

async fn get_address(
    State(db): State<ScanDb>,
    Path(addr): Path<String>,
) -> Json<Value> {
    let bc = db.lock().await;
    let balance = bc.utxo_set.balance_of(&addr);

    // Unspent UTXOs
    let utxos: Vec<Value> = bc.utxo_set.utxos_of(&addr).iter()
        .map(|u| json!({
            "tx_id":        u.tx_id,
            "output_index": u.output_index,
            "amount":       u.output.amount,
        }))
        .collect();

    // Full tx history (received outputs, spent + unspent), newest first
    let tx_history: Vec<Value> = crate::address_index::history_for_addr(
        &addr, &bc.chain, &bc.utxo_set,
    )
    .iter()
    .map(|r| json!({
        "tx_id":           r.tx_id,
        "block_height":    r.block_height,
        "block_timestamp": r.block_timestamp,
        "output_index":    r.output_index,
        "amount":          r.amount,
        "spent":           r.spent,
    }))
    .collect();

    Json(json!({
        "address":    addr,
        "balance":    balance,
        "utxo_count": utxos.len(),
        "utxos":      utxos,
        "tx_count":   tx_history.len(),
        "tx_history": tx_history,
    }))
}

// ─── /api/mempool ─────────────────────────────────────────────────────────────

async fn get_mempool(State(db): State<ScanDb>) -> Json<Value> {
    let bc    = db.lock().await;
    let stats = crate::mempool_stats::MempoolStats::compute(&bc.mempool);

    // Entries sorted by fee_rate descending
    let mut sorted: Vec<_> = bc.mempool.entries.values().collect();
    sorted.sort_by(|a, b| b.fee_rate.partial_cmp(&a.fee_rate).unwrap());
    let txs: Vec<Value> = sorted.iter()
        .map(|e| json!({
            "tx_id":       e.tx.tx_id,
            "fee":         e.fee,
            "fee_rate":    e.fee_rate,
            "size_bytes":  e.size_bytes,
            "is_coinbase": e.tx.is_coinbase,
        }))
        .collect();

    let buckets: Vec<Value> = stats.fee_buckets.iter()
        .map(|b| json!({
            "label":       b.label,
            "count":       b.count,
            "total_fees":  b.total_fees,
        }))
        .collect();

    Json(json!({
        "count":             stats.count,
        "total_fees":        stats.total_fees,
        "total_size_bytes":  stats.total_size_bytes,
        "min_fee_rate":      stats.min_fee_rate,
        "max_fee_rate":      stats.max_fee_rate,
        "avg_fee_rate":      stats.avg_fee_rate,
        "fee_percentiles": {
            "p25": stats.percentiles.p25,
            "p50": stats.percentiles.p50,
            "p75": stats.percentiles.p75,
            "p90": stats.percentiles.p90,
        },
        "suggested_fast_fee":    stats.suggested_fast_fee(),
        "suggested_economy_fee": stats.suggested_economy_fee(),
        "fee_distribution":  buckets,
        "txs":               txs,
    }))
}

// ─── /api/search ──────────────────────────────────────────────────────────────

async fn get_search(
    State(db):    State<ScanDb>,
    Query(params): Query<SearchParams>,
) -> Json<Value> {
    let bc    = db.lock().await;
    let limit = params.limit.min(50);
    let idx   = crate::search_index::SearchIndex::build(&bc.chain);
    let hits  = idx.search(&params.q, &bc.utxo_set, limit);

    let results: Vec<Value> = hits.iter().map(|r| {
        use crate::search_index::SearchResult;
        match r {
            SearchResult::Block(b) => json!({
                "kind":      "block",
                "height":    b.height,
                "hash":      b.hash,
                "tx_count":  b.tx_count,
                "timestamp": b.timestamp,
            }),
            SearchResult::Tx(t) => json!({
                "kind":         "tx",
                "tx_id":        t.tx_id,
                "block_height": t.block_height,
                "is_coinbase":  t.is_coinbase,
                "fee":          t.fee,
            }),
            SearchResult::Address(a) => json!({
                "kind":       "address",
                "addr":       a.addr,
                "balance":    a.balance,
                "utxo_count": a.utxo_count,
            }),
        }
    }).collect();

    Json(json!({
        "query":   params.q,
        "count":   results.len(),
        "results": results,
    }))
}

// ─── /api/analytics/:metric ───────────────────────────────────────────────────

async fn get_analytics(
    State(db):       State<ScanDb>,
    Path(metric):    Path<String>,
    Query(params):   Query<AnalyticsParams>,
) -> (StatusCode, Json<Value>) {
    let bc     = db.lock().await;
    let window = params.window;

    match crate::chain_analytics::analytics(&metric, &bc.chain, bc.difficulty, window) {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": format!("unknown metric '{}'. valid: block_time, hashrate, fee_market, difficulty, tx_throughput", metric)
            })),
        ),
        Some(series) => {
            let points: Vec<Value> = series.points.iter().map(|p| {
                let mut v = json!({
                    "height":    p.height,
                    "timestamp": p.timestamp,
                    "value":     p.value,
                });
                if let Some(v2) = p.value2 {
                    v["value2"] = json!(v2);
                }
                v
            }).collect();

            (StatusCode::OK, Json(json!({
                "metric":  series.metric,
                "label":   series.label,
                "unit":    series.unit,
                "window":  series.window,
                "count":   points.len(),
                "points":  points,
            })))
        }
    }
}

// ─── /api/blocks.csv ──────────────────────────────────────────────────────────

async fn get_blocks_csv(
    State(db):    State<ScanDb>,
    Query(page):  Query<PageParams>,
) -> impl IntoResponse {
    let bc     = db.lock().await;
    let limit  = page.limit.min(500);
    let slice  = crate::pagination::paginate_blocks(&bc.chain, page.from, limit);
    let csv    = crate::pagination::blocks_to_csv(slice);
    (
        [
            ("Content-Type",        "text/csv; charset=utf-8"),
            ("Content-Disposition", "attachment; filename=\"blocks.csv\""),
        ],
        csv,
    )
}

// ─── /api/txs.csv ─────────────────────────────────────────────────────────────

async fn get_txs_csv(
    State(db):    State<ScanDb>,
    Query(page):  Query<PageParams>,
) -> impl IntoResponse {
    let bc    = db.lock().await;
    let limit = page.limit.min(500);
    let rows  = crate::pagination::paginate_txs(&bc.chain, page.from, limit);
    let csv   = crate::pagination::tx_rows_to_csv(&rows);
    (
        [
            ("Content-Type",        "text/csv; charset=utf-8"),
            ("Content-Disposition", "attachment; filename=\"txs.csv\""),
        ],
        csv,
    )
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Convert 20-byte pubkey hash → Base58Check address (version byte 0x00).
/// Cùng logic với Wallet::pubkey_to_address nhưng nhận sẵn hash bytes.
fn pubkey_hash_to_address(hash20: &[u8]) -> String {
    let mut payload = vec![0x00u8];
    payload.extend_from_slice(hash20);
    let checksum = blake3::hash(blake3::hash(&payload).as_bytes());
    payload.extend_from_slice(&checksum.as_bytes()[..4]);
    bs58::encode(payload).into_string()
}

/// Extract pubkey hash bytes from a P2PKH/P2WPKH script_pubkey.
/// P2PKH ops = [OpDup, OpHash160, OpPushData(hash20), OpEqualVerify, OpCheckSig]
/// P2WPKH ops = [Op0, OpPushData(hash20)]
fn pubkey_hash_from_output(out: &crate::transaction::TxOutput) -> Option<Vec<u8>> {
    use crate::script::Opcode;
    for op in &out.script_pubkey.ops {
        if let Opcode::OpPushData(bytes) = op {
            if bytes.len() == 20 {
                return Some(bytes.clone());
            }
        }
    }
    None
}

/// Returns Base58Check address of the miner (from coinbase output).
fn miner_from_block(block: &crate::block::Block) -> String {
    block.transactions.iter()
        .find(|t| t.is_coinbase)
        .and_then(|cb| cb.outputs.first())
        .and_then(pubkey_hash_from_output)
        .map(|h| pubkey_hash_to_address(&h))
        .unwrap_or_default()
}

/// Returns Base58Check address from first output of a tx (for "to" field).
fn addr_from_output(out: &crate::transaction::TxOutput) -> String {
    pubkey_hash_from_output(out)
        .map(|h| pubkey_hash_to_address(&h))
        .unwrap_or_default()
}

fn block_summary(block: &crate::block::Block, difficulty: usize) -> Value {
    let miner  = miner_from_block(block);
    let reward = crate::reward::RewardEngine::subsidy_at(block.index);
    json!({
        "index":      block.index,
        "hash":       block.hash,
        "prev_hash":  block.prev_hash,
        "timestamp":  block.timestamp,
        "tx_count":   block.transactions.len(),
        "nonce":      block.nonce,
        "miner":      miner,
        "difficulty": difficulty,
        "reward":     reward,
    })
}

fn tx_summary(tx: &crate::transaction::Transaction) -> Value {
    let output_total: u64 = tx.outputs.iter().map(|o| o.amount).sum();
    let from = if tx.is_coinbase {
        "coinbase".to_string()
    } else {
        tx.inputs.first().map(|i| i.tx_id.clone()).unwrap_or_default()
    };
    let to = tx.outputs.first().map(addr_from_output).unwrap_or_default();
    json!({
        "tx_id":        tx.tx_id,
        "is_coinbase":  tx.is_coinbase,
        "fee":          tx.fee,
        "output_total": output_total,
        "input_count":  tx.inputs.len(),
        "output_count": tx.outputs.len(),
        "from":         from,
        "to":           to,
    })
}

/// Ước tính hashrate từ difficulty và block time.
/// hashrate ≈ 16^difficulty / avg_block_time_s
pub fn estimate_hashrate(difficulty: usize, avg_block_time_s: f64) -> u64 {
    if avg_block_time_s <= 0.0 { return 0; }
    let hashes_per_block = 16_u64.pow(difficulty as u32) as f64;
    (hashes_per_block / avg_block_time_s) as u64
}

/// Trung bình thời gian giữa các blocks (giây).
pub fn avg_block_time_secs(chain: &[crate::block::Block]) -> f64 {
    if chain.len() < 2 { return 60.0; }
    let n = chain.len().min(20); // dùng 20 blocks gần nhất
    let tail = &chain[chain.len() - n..];
    let elapsed = tail.last().unwrap().timestamp - tail.first().unwrap().timestamp;
    if elapsed <= 0 { return 60.0; }
    elapsed as f64 / (n - 1) as f64
}

// ─── CLI ──────────────────────────────────────────────────────────────────────

pub async fn serve(state: ScanDb, port: u16) {
    use crate::pktscan_ws;
    use crate::pool_api;
    use crate::token_api;
    use crate::contract_api;
    use crate::staking_api;
    use crate::defi_api;
    use crate::address_labels;
    use crate::mining_pool::PoolServer;
    use crate::token::TokenRegistry;
    use crate::smart_contract::ContractRegistry;
    use crate::staking::StakingPool;
    use crate::oracle::{OracleRegistry, LendingProtocol};
    use std::sync::Arc as StdArc;

    // Sync chain + utxo_set + difficulty từ RocksDB mỗi 5s.
    // Chỉ overwrite data fields — giữ nguyên mempool, staking, token_registry in-memory.
    {
        let db_reload = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                let fresh = crate::storage::load_or_new();
                let mut bc = db_reload.lock().await;
                if fresh.chain.len() > bc.chain.len() {
                    bc.chain      = fresh.chain;
                    bc.utxo_set   = fresh.utxo_set;
                    bc.difficulty = fresh.difficulty;
                }
            }
        });
    }

    let hub      = StdArc::new(pktscan_ws::WsHub::new());
    let ws_state = pktscan_ws::WsState {
        hub:    StdArc::clone(&hub),
        config: StdArc::new(pktscan_ws::WsConfig::default()),
    };
    pktscan_ws::spawn_poller(StdArc::clone(&hub), Arc::clone(&state), 5);

    let pool_db = StdArc::new(tokio::sync::Mutex::new(
        PoolServer::new(state.lock().await.difficulty),
    ));

    // v9.1 — Token registry (pre-seeded với PKT token)
    let token_db: token_api::TokenDb = StdArc::new(tokio::sync::Mutex::new({
        let mut reg = TokenRegistry::new();
        let _ = reg.create_token(
            "PKT", "PacketCrypt Token", "PKT", 9,
            21_000_000_000_000_000u128, "genesis",
        );
        reg
    }));

    // v9.2 — Contract registry (empty; contracts deployed via chain events in future)
    let contract_db: contract_api::ContractDb = StdArc::new(tokio::sync::Mutex::new(
        ContractRegistry::new(),
    ));

    // v9.3 — Staking pool (empty; populated via chain events in future)
    let staking_db: staking_api::StakingDb = StdArc::new(tokio::sync::Mutex::new(
        StakingPool::new(),
    ));

    // v9.5 — Address labels (pre-seeded với known addresses)
    let label_db: address_labels::LabelDb = StdArc::new(tokio::sync::Mutex::new({
        let mut reg = address_labels::LabelRegistry::new();
        reg.insert(address_labels::AddressLabel::new(
            "genesis", "PKT Genesis", "foundation", "Genesis block creator",
        ));
        reg
    }));

    // v9.4 — DeFi: oracle registry + lending protocol
    let defi_db: defi_api::DefiDb = StdArc::new(tokio::sync::Mutex::new(
        defi_api::DefiState {
            oracle:  OracleRegistry::new(),
            lending: LendingProtocol::new("BTC/USD", 1.5),
        },
    ));

    // v8.8 — Response cache (TTL = 5 s)
    let cache_db: CacheDb = Arc::new(Mutex::new(
        crate::response_cache::ResponseCache::new(5),
    ));
    let cache_clone = Arc::clone(&cache_db);

    // v10.0 — API auth store
    let auth_db = crate::api_auth::ApiKeyStore::load_default();
    let auth_db_keys = Arc::clone(&auth_db); // v19.9: dùng cho key_api router

    // v10.1 — Audit log (daily rotation)
    let audit_db = crate::audit_log::AuditLogger::open_default();

    // v9.0 — Zero-Trust middleware (rate limit + input guard + audit log)
    let zt = crate::zt_middleware::ZtState::new(
        crate::zt_middleware::ZtConfig::default(),
    );

    // v9.6 — CORS allowlist (không dùng wildcard *)
    let cors_cfg: CorsState = Arc::new(CorsConfig::default());

    let app = router(Arc::clone(&state))
        .merge(pktscan_ws::ws_router(ws_state))
        .merge(pool_api::pool_router(pool_db))
        .merge(token_api::token_router(token_db))
        .merge(contract_api::contract_router(Arc::clone(&contract_db)))
        .merge(staking_api::staking_router(staking_db))
        .merge(defi_api::defi_router(defi_db))
        .merge(address_labels::label_router(label_db))
        .merge(crate::openapi::openapi_router())
        .merge(crate::sdk_gen::sdk_router())
        .merge(crate::graphql::graphql_router(Arc::clone(&state)))
        .merge(crate::webhook::webhook_router(crate::webhook::open_default()))
        .merge(crate::write_api::write_router(Arc::clone(&state), Arc::clone(&contract_db)))
        .merge(crate::scam_registry::risk_router(crate::scam_registry::open_default()))
        .merge(crate::address_watch::watch_router(
            crate::address_watch::open_default(), Arc::clone(&state)))
        .merge(crate::multi_chain::multi_chain_router(crate::multi_chain::open_default()))
        .merge(crate::audit_log::admin_router(Arc::clone(&audit_db)))
        .merge(crate::web_frontend::static_router())   // v14.2: embedded /static/app.js + /static/style.css
        .merge(crate::web_charts::charts_router())    // v14.5: embedded /static/charts.js
        .merge(crate::block_detail::detail_router())   // v14.6: embedded /static/detail.js
        .merge(crate::address_detail::address_router()) // v14.7: embedded /static/address.js
        .merge(crate::ws_live::live_router())          // v14.8: embedded /static/live.js
        .merge(crate::pkt_testnet_web::testnet_web_router()) // v15.6: /api/testnet/* + /static/testnet.js
        .merge(crate::pkt_rpc::rpc_router())               // v19.2: POST /rpc JSON-RPC 2.0
        .merge(crate::key_api::key_router(auth_db_keys))  // v19.9: GET/POST/DELETE /api/keys
        .merge(crate::web_serve::web_router())             // web/: ServeDir /web/** + /address/:a + /block/:h + /rx/:id
        .layer(middleware::from_fn_with_state(
            Arc::clone(&audit_db),
            crate::audit_log::audit_middleware,
        ))
        .layer(middleware::from_fn(move |req, next| {
            let cors = Arc::clone(&cors_cfg);
            cors_layer(cors, req, next)
        }))
        .layer(middleware::from_fn(move |req, next| {
            let c = Arc::clone(&cache_clone);
            api_cache_middleware(c, req, next)
        }))
        .layer(middleware::from_fn_with_state(
            auth_db,
            crate::api_auth::auth_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            zt,
            crate::zt_middleware::zt_middleware,
        ));

    let addr = format!("0.0.0.0:{}", port);
    println!();
    println!("  PKTScan  →  http://localhost:{}", port);
    println!("  GET  /                              (index.html — embedded binary)");
    println!("  GET  /static/app.js                (embedded binary)");
    println!("  GET  /static/style.css             (embedded binary)");
    println!("  GET  /api/stats");
    println!("  GET  /api/blocks?limit=20&from=<height>");
    println!("  GET  /api/block/:height");
    println!("  GET  /api/txs?limit=20&from=<height>&min_amount=&max_amount=&since=&until=");
    println!("  GET  /api/tx/:txid");
    println!("  GET  /api/address/:addr");
    println!("  GET  /api/mempool");
    println!("  GET  /api/search?q=");
    println!("  GET  /api/analytics/:metric?window=100");
    println!("  GET  /api/blocks.csv");
    println!("  GET  /api/txs.csv");
    println!("  GET  /api/pool/stats");
    println!("  GET  /api/pool/miners");
    println!("  GET  /api/tokens");
    println!("  GET  /api/token/:id");
    println!("  GET  /api/token/:id/holders?limit=20");
    println!("  GET  /api/token/:id/balance/:addr");
    println!("  GET  /api/contracts");
    println!("  GET  /api/contract/:addr");
    println!("  GET  /api/contract/:addr/state");
    println!("  GET  /api/contract/:addr/state/:key");
    println!("  GET  /api/staking/stats");
    println!("  GET  /api/staking/validators");
    println!("  GET  /api/staking/validator/:addr");
    println!("  GET  /api/staking/delegator/:addr");
    println!("  GET  /api/defi/feeds");
    println!("  GET  /api/defi/feed/:id");
    println!("  GET  /api/defi/feed/:id/history");
    println!("  GET  /api/defi/loans");
    println!("  GET  /api/defi/loans/liquidatable");
    println!("  GET  /api/labels");
    println!("  GET  /api/label/:addr");
    println!("  GET  /api/labels/category/:cat");
    println!("  WS   /ws   (live feed)");
    println!("  Cache TTL: 5 s  (ETag / 304 support)");
    println!("  GET  /api/openapi.json");
    println!("  GET  /api/sdk/js");
    println!("  GET  /api/sdk/ts");
    println!("  CORS: allowlist [localhost:3000/8080, pktscan.io]  (v9.6)");
    println!("  Auth: X-API-Key header  (optional for GET, required for write — v10.0)");
    println!("  GET  /api/admin/logs?date=YYYY-MM-DD&limit=100  (admin role only — v10.1)");
    println!("  ZT: rate limit 100 req/60s per IP  |  audit log ~/.pkt/audit.log");
    println!();
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Block;
    use crate::chain::Blockchain;
    use crate::transaction::Transaction;

    fn make_bc() -> Blockchain {
        let mut bc = Blockchain::new();
        // Mine 3 blocks với coinbase txs
        for i in 1..=3u64 {
            let coinbase = Transaction::coinbase_at(
                "aabbccdd00112233445566778899aabb", 1000, i,
            );
            let mut block = Block::new(i, vec![coinbase], bc.chain.last().unwrap().hash.clone());
            block.mine(2);
            bc.chain.push(block);
        }
        bc
    }

    #[test]
    fn test_avg_block_time_single_block() {
        let bc = Blockchain::new();
        let t = avg_block_time_secs(&bc.chain);
        assert_eq!(t, 60.0); // fallback
    }

    #[test]
    fn test_avg_block_time_multi() {
        let bc = make_bc();
        let t = avg_block_time_secs(&bc.chain);
        assert!(t >= 0.0);
    }

    #[test]
    fn test_estimate_hashrate_zero_time() {
        let h = estimate_hashrate(3, 0.0);
        assert_eq!(h, 0);
    }

    #[test]
    fn test_estimate_hashrate_positive() {
        let h = estimate_hashrate(2, 60.0);
        // 16^2 / 60 = 256 / 60 ≈ 4
        assert!(h > 0);
    }

    #[test]
    fn test_block_summary_fields() {
        let bc = make_bc();
        let b  = bc.chain.last().unwrap();
        let v  = block_summary(b, bc.difficulty);
        assert!(v["index"].is_number());
        assert!(v["hash"].is_string());
        assert!(v["tx_count"].is_number());
    }

    #[test]
    fn test_tx_summary_fields() {
        let tx = Transaction::coinbase_at(
            "aabbccdd00112233445566778899aabb", 0, 0,
        );
        let v = tx_summary(&tx);
        assert_eq!(v["is_coinbase"], true);
        assert!(v["output_total"].is_number());
    }

    #[test]
    fn test_router_builds() {
        // Chỉ verify router có thể khởi tạo mà không panic
        let bc  = Blockchain::new();
        let db  = Arc::new(Mutex::new(bc));
        let _r  = router(db);
    }

    #[test]
    fn test_page_params_defaults() {
        let p: PageParams = serde_json::from_str("{}").unwrap();
        assert_eq!(p.limit, 20);
        assert_eq!(p.offset, 0);
    }

    #[test]
    fn test_page_params_custom() {
        let p: PageParams = serde_json::from_str(r#"{"limit":50,"offset":100}"#).unwrap();
        assert_eq!(p.limit, 50);
        assert_eq!(p.offset, 100);
    }

    #[test]
    fn test_stats_helper_values() {
        // estimate_hashrate + avg_block_time đúng
        let h1 = estimate_hashrate(4, 60.0);
        let h2 = estimate_hashrate(4, 30.0);
        // difficulty=4 với block time ngắn hơn → hashrate cao hơn
        assert!(h2 > h1);
    }

    // ── v9.6 — TxFilterParams ─────────────────────────────────────────────

    #[test]
    fn test_tx_filter_defaults() {
        let f: TxFilterParams = serde_json::from_str("{}").unwrap();
        assert_eq!(f.limit, 20);
        assert_eq!(f.offset, 0);
        assert!(f.min_amount.is_none());
        assert!(f.max_amount.is_none());
        assert!(f.since.is_none());
        assert!(f.until.is_none());
    }

    #[test]
    fn test_tx_filter_all_fields() {
        let json = r#"{"limit":10,"offset":5,"min_amount":100,"max_amount":999,"since":1000,"until":2000}"#;
        let f: TxFilterParams = serde_json::from_str(json).unwrap();
        assert_eq!(f.limit, 10);
        assert_eq!(f.offset, 5);
        assert_eq!(f.min_amount, Some(100));
        assert_eq!(f.max_amount, Some(999));
        assert_eq!(f.since, Some(1000));
        assert_eq!(f.until, Some(2000));
    }

    #[test]
    fn test_tx_filter_limit_cap() {
        // limit được cap ở 100 trong get_txs logic
        let f: TxFilterParams = serde_json::from_str(r#"{"limit":999}"#).unwrap();
        let capped = f.limit.min(100);
        assert_eq!(capped, 100);
    }

    // ── v9.6 — CorsConfig ────────────────────────────────────────────────

    #[test]
    fn test_cors_default_allows_localhost_3000() {
        let cfg = CorsConfig::default();
        assert!(cfg.is_allowed("http://localhost:3000"));
    }

    #[test]
    fn test_cors_default_allows_localhost_8080() {
        let cfg = CorsConfig::default();
        assert!(cfg.is_allowed("http://localhost:8080"));
    }

    #[test]
    fn test_cors_default_allows_pktscan() {
        let cfg = CorsConfig::default();
        assert!(cfg.is_allowed("https://pktscan.io"));
    }

    #[test]
    fn test_cors_default_rejects_unknown() {
        let cfg = CorsConfig::default();
        assert!(!cfg.is_allowed("https://evil.example.com"));
    }

    #[test]
    fn test_cors_wildcard_allows_any() {
        let cfg = CorsConfig::new(vec!["*"]);
        assert!(cfg.is_allowed("https://random.site"));
        assert!(cfg.is_allowed("http://localhost:9999"));
    }

    #[test]
    fn test_cors_custom_allowlist() {
        let cfg = CorsConfig::new(vec!["https://myapp.com", "http://staging.myapp.com"]);
        assert!(cfg.is_allowed("https://myapp.com"));
        assert!(cfg.is_allowed("http://staging.myapp.com"));
        assert!(!cfg.is_allowed("https://other.com"));
    }

    #[test]
    fn test_cors_empty_list_rejects_all() {
        let cfg = CorsConfig::new(Vec::<String>::new());
        assert!(!cfg.is_allowed("http://localhost:3000"));
    }
}
