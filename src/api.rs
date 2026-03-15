#![allow(dead_code)]

/// v4.4 — REST API
///
/// Endpoints:
///   GET  /chain              → { height, difficulty, block_count }
///   GET  /chain/:height      → Block tại height
///   GET  /balance/:addr      → { address, balance }
///   GET  /mempool            → { count, total_fees, txs: [...] }
///   POST /tx                 → thêm transaction vào mempool
///   GET  /status             → { height, utxo_count, mempool_count, difficulty }

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::chain::Blockchain;
use crate::transaction::Transaction;

pub type Db = Arc<Mutex<Blockchain>>;

/// Tạo router với tất cả các endpoints
pub fn router(state: Db) -> Router {
    Router::new()
        .route("/chain",          get(get_chain))
        .route("/chain/:height",  get(get_block))
        .route("/balance/:addr",  get(get_balance))
        .route("/mempool",        get(get_mempool))
        .route("/tx",             post(post_tx))
        .route("/status",         get(get_status))
        .route("/metrics",        get(get_metrics))
        .with_state(state)
}

/// GET /chain — toàn bộ chain (height + danh sách blocks)
async fn get_chain(State(db): State<Db>) -> Json<Value> {
    let bc = db.lock().await;
    Json(json!({
        "height":      bc.chain.len().saturating_sub(1),
        "difficulty":  bc.difficulty,
        "block_count": bc.chain.len(),
        "blocks":      bc.chain,
    }))
}

/// GET /chain/:height — block tại height cụ thể
async fn get_block(
    State(db): State<Db>,
    Path(height): Path<u64>,
) -> (StatusCode, Json<Value>) {
    let bc = db.lock().await;
    match bc.chain.iter().find(|b| b.index == height) {
        Some(block) => (StatusCode::OK, Json(json!(block))),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("block at height {} not found", height) })),
        ),
    }
}

/// GET /balance/:addr — số dư theo pubkey_hash_hex hoặc xonly_hex
async fn get_balance(
    State(db): State<Db>,
    Path(addr): Path<String>,
) -> Json<Value> {
    let bc = db.lock().await;
    let balance = bc.utxo_set.balance_of(&addr);
    Json(json!({ "address": addr, "balance": balance }))
}

/// GET /mempool — trạng thái mempool
async fn get_mempool(State(db): State<Db>) -> Json<Value> {
    let bc = db.lock().await;
    let txs: Vec<Value> = bc.mempool.entries.values()
        .map(|e| json!({
            "tx_id":      e.tx.tx_id,
            "fee":        e.fee,
            "fee_rate":   e.fee_rate,
            "size_bytes": e.size_bytes,
        }))
        .collect();
    Json(json!({
        "count":       txs.len(),
        "total_fees":  bc.mempool.total_pending_fees(),
        "txs":         txs,
    }))
}

/// POST /tx — nhận JSON Transaction, thêm vào mempool
async fn post_tx(
    State(db): State<Db>,
    Json(tx): Json<Transaction>,
) -> (StatusCode, Json<Value>) {
    let mut bc = db.lock().await;
    let tx_id  = tx.tx_id.clone();
    let fee    = tx.fee;
    let output_total: u64 = tx.outputs.iter().map(|o| o.amount).sum();
    let input_total  = output_total + fee;
    match bc.mempool.add(tx, input_total) {
        Ok(_) => (StatusCode::OK,         Json(json!({ "status": "ok",    "tx_id": tx_id }))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "status": "error", "message": e  }))),
    }
}

/// GET /metrics — v4.8: hashrate, peer count, mempool, block time
async fn get_metrics(State(db): State<Db>) -> Json<Value> {
    let bc = db.lock().await;
    let m  = crate::metrics::collect(&bc, None);
    Json(serde_json::to_value(m).unwrap_or(serde_json::json!({})))
}

/// GET /status — tóm tắt trạng thái node
async fn get_status(State(db): State<Db>) -> Json<Value> {
    let bc = db.lock().await;
    Json(json!({
        "height":        bc.chain.len().saturating_sub(1),
        "difficulty":    bc.difficulty,
        "utxo_count":    bc.utxo_set.utxos.len(),
        "mempool_count": bc.mempool.entries.len(),
        "total_supply":  bc.utxo_set.total_supply(),
    }))
}

/// Khởi động API server tại port cho trước
pub async fn serve(state: Db, port: u16) {
    let app  = router(state);
    let addr = format!("0.0.0.0:{}", port);
    println!("🌐 REST API listening on http://{}", addr);
    println!("   GET  /chain");
    println!("   GET  /chain/:height");
    println!("   GET  /balance/:addr");
    println!("   GET  /mempool");
    println!("   POST /tx");
    println!("   GET  /status");
    println!("   GET  /metrics");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
