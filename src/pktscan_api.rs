#![allow(dead_code)]
//! v8.0 — PKTScan REST Backend
//!
//! Axum server phục vụ `index.html` (PKTScan frontend) với đầy đủ
//! dữ liệu blockchain qua JSON API.
//!
//! Endpoints:
//!   GET /api/stats                → network stats
//!   GET /api/blocks               → latest blocks (limit/offset)
//!   GET /api/block/:height        → block detail
//!   GET /api/txs                  → latest transactions (limit/offset)
//!   GET /api/tx/:txid             → transaction detail
//!   GET /api/address/:addr        → balance + UTXOs
//!   GET /api/mempool              → pending transactions
//!
//! CORS: `Access-Control-Allow-Origin: *` cho mọi response
//! CLI: `cargo run -- pktscan [port]` (default 8080)

use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::chain::Blockchain;

pub type ScanDb = Arc<Mutex<Blockchain>>;

// ─── CORS Middleware ──────────────────────────────────────────────────────────

async fn cors_layer(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        "Access-Control-Allow-Origin",
        HeaderValue::from_static("*"),
    );
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
}
fn default_limit() -> usize { 20 }

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}
fn default_search_limit() -> usize { 10 }

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn router(state: ScanDb) -> Router {
    Router::new()
        .route("/api/stats",           get(get_stats))
        .route("/api/blocks",          get(get_blocks))
        .route("/api/block/:height",   get(get_block))
        .route("/api/txs",             get(get_txs))
        .route("/api/tx/:txid",        get(get_tx))
        .route("/api/address/:addr",   get(get_address))
        .route("/api/mempool",         get(get_mempool))
        .route("/api/search",          get(get_search))
        .layer(middleware::from_fn(cors_layer))
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
    let bc = db.lock().await;
    let limit  = page.limit.min(100);
    let offset = page.offset;
    let total  = bc.chain.len();

    // Trả về blocks mới nhất trước (reverse order)
    let blocks: Vec<Value> = bc.chain.iter().rev()
        .skip(offset)
        .take(limit)
        .map(block_summary)
        .collect();

    Json(json!({
        "blocks": blocks,
        "total":  total,
        "limit":  limit,
        "offset": offset,
    }))
}

// ─── /api/block/:height ───────────────────────────────────────────────────────

async fn get_block(
    State(db): State<ScanDb>,
    Path(height): Path<u64>,
) -> (StatusCode, Json<Value>) {
    let bc = db.lock().await;
    match bc.chain.iter().find(|b| b.index == height) {
        Some(block) => {
            let reward = crate::reward::RewardEngine::subsidy_at(block.index);
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
    Query(page): Query<PageParams>,
) -> Json<Value> {
    let bc = db.lock().await;
    let limit  = page.limit.min(100);
    let offset = page.offset;

    // Collect all txs newest block first
    let all_txs: Vec<Value> = bc.chain.iter().rev()
        .flat_map(|b| b.transactions.iter().map(move |tx| {
            let mut v = tx_summary(tx);
            v["block_height"] = json!(b.index);
            v["block_timestamp"] = json!(b.timestamp);
            v
        }))
        .skip(offset)
        .take(limit)
        .collect();

    let total: usize = bc.chain.iter().map(|b| b.transactions.len()).sum();

    Json(json!({
        "txs":    all_txs,
        "total":  total,
        "limit":  limit,
        "offset": offset,
    }))
}

// ─── /api/tx/:txid ────────────────────────────────────────────────────────────

async fn get_tx(
    State(db): State<ScanDb>,
    Path(txid): Path<String>,
) -> (StatusCode, Json<Value>) {
    let bc = db.lock().await;
    for block in bc.chain.iter().rev() {
        if let Some(tx) = block.transactions.iter().find(|t| t.tx_id == txid) {
            let output_total: u64 = tx.outputs.iter()
                .map(|o| o.amount)
                .sum();
            return (StatusCode::OK, Json(json!({
                "tx_id":        tx.tx_id,
                "wtx_id":       tx.wtx_id,
                "is_coinbase":  tx.is_coinbase,
                "fee":          tx.fee,
                "output_total": output_total,
                "inputs":       tx.inputs,
                "outputs":      tx.outputs,
                "block_height": block.index,
                "block_hash":   block.hash,
                "timestamp":    block.timestamp,
            })));
        }
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

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn block_summary(block: &crate::block::Block) -> Value {
    json!({
        "index":     block.index,
        "hash":      block.hash,
        "prev_hash": block.prev_hash,
        "timestamp": block.timestamp,
        "tx_count":  block.transactions.len(),
        "nonce":     block.nonce,
    })
}

fn tx_summary(tx: &crate::transaction::Transaction) -> Value {
    let output_total: u64 = tx.outputs.iter().map(|o| o.amount).sum();
    json!({
        "tx_id":       tx.tx_id,
        "is_coinbase": tx.is_coinbase,
        "fee":         tx.fee,
        "output_total": output_total,
        "input_count": tx.inputs.len(),
        "output_count": tx.outputs.len(),
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
    use crate::mining_pool::PoolServer;
    use std::sync::Arc as StdArc;

    let hub = StdArc::new(pktscan_ws::WsHub::new());
    pktscan_ws::spawn_poller(StdArc::clone(&hub), Arc::clone(&state), 5);

    let pool_db = StdArc::new(tokio::sync::Mutex::new(
        PoolServer::new(state.lock().await.difficulty),
    ));

    let app  = router(Arc::clone(&state))
        .merge(pktscan_ws::ws_router(hub))
        .merge(pool_api::pool_router(pool_db));
    let addr = format!("0.0.0.0:{}", port);
    println!();
    println!("  PKTScan API  →  http://localhost:{}", port);
    println!("  GET  /api/stats");
    println!("  GET  /api/blocks?limit=20&offset=0");
    println!("  GET  /api/block/:height");
    println!("  GET  /api/txs?limit=20&offset=0");
    println!("  GET  /api/tx/:txid");
    println!("  GET  /api/address/:addr");
    println!("  GET  /api/mempool");
    println!("  GET  /api/search?q=");
    println!("  GET  /api/pool/stats");
    println!("  GET  /api/pool/miners");
    println!("  WS   /ws   (live feed)");
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
        let v  = block_summary(b);
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
}
