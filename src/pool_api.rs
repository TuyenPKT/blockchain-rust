#![allow(dead_code)]
//! v8.5 — Mining Pool Dashboard API
//!
//! REST endpoints exposing `PoolServer` state to PKTScan.
//!
//! Endpoints:
//!   GET /api/pool/stats   → pool summary (blocks_found, hashrate, shares, miners)
//!   GET /api/pool/miners  → per-miner breakdown (id, address, shares, hashrate, payout_est)
//!
//! Usage:
//!   let pool_db = Arc::new(Mutex::new(PoolServer::new(difficulty)));
//!   let app = rest_router.merge(pool_api::pool_router(pool_db));

use axum::{
    extract::State,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::mining_pool::PoolServer;

pub type PoolDb = Arc<Mutex<PoolServer>>;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn pool_router(state: PoolDb) -> Router {
    Router::new()
        .route("/api/pool/stats",  get(get_pool_stats))
        .route("/api/pool/miners", get(get_pool_miners))
        .with_state(state)
}

// ─── /api/pool/stats ──────────────────────────────────────────────────────────

async fn get_pool_stats(State(db): State<PoolDb>) -> Json<Value> {
    let pool = db.lock().await;

    let active_miners = pool.miners.values()
        .filter(|m| m.shares > 0)
        .count();

    let total_hashrate: f64 = pool.miners.values()
        .map(|m| m.estimated_hashrate())
        .sum();

    let has_job = pool.current_job.is_some();
    let job_id = pool.current_job.as_ref()
        .map(|j| j.job_id.clone())
        .unwrap_or_default();

    Json(json!({
        "block_difficulty":      pool.block_difficulty,
        "default_share_diff":    pool.default_share_diff,
        "blocks_found":          pool.blocks_found,
        "total_shares_in_round": pool.total_shares_in_round,
        "total_miners":          pool.miners.len(),
        "active_miners":         active_miners,
        "total_hashrate":        total_hashrate,
        "has_active_job":        has_job,
        "current_job_id":        job_id,
        "retarget_interval_s":   pool.retarget_interval,
    }))
}

// ─── /api/pool/miners ─────────────────────────────────────────────────────────

async fn get_pool_miners(State(db): State<PoolDb>) -> Json<Value> {
    let pool = db.lock().await;

    // Payout estimate based on current block reward (OCEIF initial = 20 PKT, total supply = 21M)
    let block_reward = crate::pkt_genesis::INITIAL_BLOCK_REWARD;
    let payouts = pool.payout(block_reward);

    let mut miners: Vec<Value> = pool.miners.values()
        .map(|m| {
            let payout_est = payouts.get(&m.id).copied().unwrap_or(0);
            json!({
                "id":               m.id,
                "address":          m.address,
                "shares":           m.shares,
                "total_shares":     m.total_shares,
                "share_difficulty": m.share_difficulty,
                "hashrate":         m.estimated_hashrate(),
                "last_share_ts":    m.last_share_ts,
                "payout_est":       payout_est,
            })
        })
        .collect();

    // Sort by shares descending
    miners.sort_by(|a, b| {
        let sa = a["shares"].as_u64().unwrap_or(0);
        let sb = b["shares"].as_u64().unwrap_or(0);
        sb.cmp(&sa)
    });

    Json(json!({
        "count":  miners.len(),
        "miners": miners,
    }))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mining_pool::{PoolServer, Share};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn make_pool() -> PoolDb {
        Arc::new(Mutex::new(PoolServer::new(4)))
    }

    fn populated_pool() -> PoolServer {
        let mut pool = PoolServer::new(4);
        pool.register_miner("alice", "addr_alice");
        pool.register_miner("bob",   "addr_bob");

        let job = pool.new_job(1, "prevhash000", "txroot", "witroot");

        // Submit shares for alice
        for nonce in 0..5u64 {
            let hash = job.hash_nonce(nonce);
            if job.meets_share(&hash) {
                let share = Share {
                    job_id:    job.job_id.clone(),
                    miner_id:  "alice".to_string(),
                    nonce,
                    hash:      hash.clone(),
                    is_block_solution: job.meets_block(&hash),
                };
                pool.submit_share(share);
            }
        }
        pool
    }

    // ── pool_router ───────────────────────────────────────────────────────

    #[test]
    fn test_pool_router_builds() {
        let db = make_pool();
        let _r = pool_router(db);
    }

    // ── PoolDb type ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_pool_db_lock() {
        let db = make_pool();
        let pool = db.lock().await;
        assert_eq!(pool.block_difficulty, 4);
    }

    // ── stats via direct PoolServer ───────────────────────────────────────

    #[test]
    fn test_stats_empty_pool() {
        let pool = PoolServer::new(3);
        assert_eq!(pool.miners.len(), 0);
        assert_eq!(pool.blocks_found, 0);
        assert_eq!(pool.total_shares_in_round, 0);
    }

    #[test]
    fn test_stats_after_register() {
        let mut pool = PoolServer::new(4);
        pool.register_miner("m1", "addr1");
        pool.register_miner("m2", "addr2");
        assert_eq!(pool.miners.len(), 2);
    }

    #[test]
    fn test_stats_blocks_found_starts_zero() {
        let pool = PoolServer::new(4);
        assert_eq!(pool.blocks_found, 0);
    }

    #[test]
    fn test_stats_default_share_diff() {
        let pool = PoolServer::new(4);
        assert_eq!(pool.default_share_diff, 2); // 4 - 2 = 2
    }

    #[test]
    fn test_stats_default_share_diff_low() {
        let pool = PoolServer::new(2);
        assert_eq!(pool.default_share_diff, 1); // clamped to 1
    }

    // ── miners via direct PoolServer ──────────────────────────────────────

    #[test]
    fn test_miners_empty() {
        let pool = PoolServer::new(4);
        assert!(pool.miners.is_empty());
    }

    #[test]
    fn test_miners_hashrate_zero_before_shares() {
        let mut pool = PoolServer::new(4);
        pool.register_miner("m1", "addr1");
        let m = pool.miners.get("m1").unwrap();
        assert_eq!(m.estimated_hashrate(), 0.0);
    }

    #[test]
    fn test_miners_share_difficulty_set() {
        let mut pool = PoolServer::new(4);
        pool.register_miner("m1", "addr1");
        let m = pool.miners.get("m1").unwrap();
        assert_eq!(m.share_difficulty, pool.default_share_diff);
    }

    // ── payout ────────────────────────────────────────────────────────────

    #[test]
    fn test_payout_empty_pool() {
        let pool = PoolServer::new(4);
        let p = pool.payout(1000);
        assert!(p.is_empty());
    }

    #[test]
    fn test_payout_no_shares() {
        let mut pool = PoolServer::new(4);
        pool.register_miner("m1", "addr1");
        let p = pool.payout(1000);
        assert!(p.is_empty()); // no shares → no payout
    }

    // ── total_hashrate ────────────────────────────────────────────────────

    #[test]
    fn test_total_hashrate_empty() {
        let pool = PoolServer::new(4);
        let hr: f64 = pool.miners.values().map(|m| m.estimated_hashrate()).sum();
        assert_eq!(hr, 0.0);
    }

    // ── miner_count ───────────────────────────────────────────────────────

    #[test]
    fn test_miner_count() {
        let mut pool = PoolServer::new(4);
        assert_eq!(pool.miner_count(), 0);
        pool.register_miner("a", "addr_a");
        assert_eq!(pool.miner_count(), 1);
    }

    // ── reset_round ───────────────────────────────────────────────────────

    #[test]
    fn test_reset_round_clears_shares() {
        let mut pool = PoolServer::new(4);
        pool.register_miner("m1", "addr1");
        // manually add shares
        if let Some(m) = pool.miners.get_mut("m1") {
            m.shares = 5;
        }
        pool.total_shares_in_round = 5;
        pool.reset_round();
        assert_eq!(pool.total_shares_in_round, 0);
        assert_eq!(pool.miners["m1"].shares, 0);
    }
}
