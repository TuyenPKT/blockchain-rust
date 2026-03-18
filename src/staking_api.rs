#![allow(dead_code)]
//! v9.3 — Staking API (GET only, Zero-Trust)
//!
//! REST endpoints exposing `StakingPool` to PKTScan.
//! Tất cả endpoints đều read-only (GET).
//! ZT middleware (rate limit + audit log) áp dụng ở router level.
//!
//! Endpoints:
//!   GET /api/staking/stats                      → tổng quan: total_staked, validator_count
//!   GET /api/staking/validators                 → list tất cả validators
//!   GET /api/staking/validator/:addr            → validator detail + delegator list
//!   GET /api/staking/delegator/:addr            → tất cả stakes + pending rewards của một delegator
//!
//! Usage:
//!   let staking_db = StakingDb::new(pool);
//!   let app = router.merge(staking_api::staking_router(staking_db));

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::staking::StakingPool;

pub type StakingDb = Arc<Mutex<StakingPool>>;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn staking_router(state: StakingDb) -> Router {
    Router::new()
        .route("/api/staking/stats",              get(get_staking_stats))
        .route("/api/staking/validators",         get(get_validators))
        .route("/api/staking/validator/:addr",    get(get_validator))
        .route("/api/staking/delegator/:addr",    get(get_delegator))
        .with_state(state)
}

// ─── GET /api/staking/stats ───────────────────────────────────────────────────

/// Tổng quan staking: total staked, validator count, active count.
async fn get_staking_stats(State(db): State<StakingDb>) -> Json<Value> {
    let pool = db.lock().await;

    let validator_count = pool.validators.len();
    let active_count    = pool.validators.values().filter(|v| v.active).count();
    let delegator_count = {
        let mut set = std::collections::HashSet::new();
        for s in &pool.stakes {
            set.insert(s.delegator.as_str());
        }
        set.len()
    };

    Json(json!({
        "total_staked":      pool.total_staked,
        "reward_per_token":  pool.reward_per_token,
        "validator_count":   validator_count,
        "active_validators": active_count,
        "delegator_count":   delegator_count,
        "stake_count":       pool.stakes.len(),
    }))
}

// ─── GET /api/staking/validators ─────────────────────────────────────────────

/// List tất cả validators, sorted by total_stake desc.
async fn get_validators(State(db): State<StakingDb>) -> Json<Value> {
    let pool = db.lock().await;

    let mut validators: Vec<Value> = pool.validators.values()
        .map(|v| {
            let delegator_count = pool.stakes.iter()
                .filter(|s| s.validator == v.address)
                .count();
            let share_pct = if pool.total_staked > 0 {
                v.total_stake as f64 / pool.total_staked as f64 * 100.0
            } else {
                0.0
            };
            json!({
                "address":          v.address,
                "commission_bps":   v.commission_bps,
                "commission_pct":   format!("{:.2}", v.commission_bps as f64 / 100.0),
                "total_stake":      v.total_stake,
                "share_pct":        format!("{:.4}", share_pct),
                "active":           v.active,
                "delegator_count":  delegator_count,
            })
        })
        .collect();

    validators.sort_by(|a, b| {
        let sa = a["total_stake"].as_u64().unwrap_or(0);
        let sb = b["total_stake"].as_u64().unwrap_or(0);
        sb.cmp(&sa)
    });

    Json(json!({
        "count":      validators.len(),
        "validators": validators,
    }))
}

// ─── GET /api/staking/validator/:addr ────────────────────────────────────────

/// Validator detail + list delegators.
async fn get_validator(
    State(db):   State<StakingDb>,
    Path(addr):  Path<String>,
) -> (StatusCode, Json<Value>) {
    let pool = db.lock().await;

    match pool.validators.get(&addr) {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("validator '{}' not found", addr) })),
        ),
        Some(v) => {
            let delegators: Vec<Value> = pool.stakes.iter()
                .filter(|s| s.validator == addr)
                .map(|s| {
                    let pending = pool.reward_per_token
                        .saturating_sub(s.reward_debt)
                        .saturating_mul(s.amount)
                        / 1_000_000_000;
                    json!({
                        "delegator":    s.delegator,
                        "amount":       s.amount,
                        "locked_until": s.locked_until,
                        "pending_reward": pending,
                    })
                })
                .collect();

            let share_pct = if pool.total_staked > 0 {
                v.total_stake as f64 / pool.total_staked as f64 * 100.0
            } else {
                0.0
            };

            (StatusCode::OK, Json(json!({
                "address":          v.address,
                "commission_bps":   v.commission_bps,
                "commission_pct":   format!("{:.2}", v.commission_bps as f64 / 100.0),
                "total_stake":      v.total_stake,
                "share_pct":        format!("{:.4}", share_pct),
                "active":           v.active,
                "delegator_count":  delegators.len(),
                "delegators":       delegators,
            })))
        }
    }
}

// ─── GET /api/staking/delegator/:addr ────────────────────────────────────────

/// Tất cả stakes + pending rewards của một delegator.
async fn get_delegator(
    State(db):   State<StakingDb>,
    Path(addr):  Path<String>,
) -> Json<Value> {
    let pool = db.lock().await;

    let stakes: Vec<Value> = pool.stakes.iter()
        .filter(|s| s.delegator == addr)
        .map(|s| {
            let pending = pool.reward_per_token
                .saturating_sub(s.reward_debt)
                .saturating_mul(s.amount)
                / 1_000_000_000;
            json!({
                "validator":      s.validator,
                "amount":         s.amount,
                "locked_until":   s.locked_until,
                "pending_reward": pending,
            })
        })
        .collect();

    let total_staked: u64 = pool.stakes.iter()
        .filter(|s| s.delegator == addr)
        .map(|s| s.amount)
        .sum();

    let total_pending: u64 = pool.pending_rewards(&addr);

    Json(json!({
        "delegator":     addr,
        "stake_count":   stakes.len(),
        "total_staked":  total_staked,
        "total_pending": total_pending,
        "stakes":        stakes,
    }))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::staking::StakingPool;

    fn make_db() -> StakingDb {
        Arc::new(Mutex::new(StakingPool::new()))
    }

    fn populated_db() -> StakingDb {
        let mut pool = StakingPool::new();
        pool.register_validator("val1", 500).unwrap();   // 5% commission
        pool.register_validator("val2", 1000).unwrap();  // 10% commission
        pool.delegate("alice", "val1", 10_000, 100, 0).unwrap();
        pool.delegate("bob",   "val1", 5_000,  50,  0).unwrap();
        pool.delegate("carol", "val2", 20_000, 200, 0).unwrap();
        Arc::new(Mutex::new(pool))
    }

    // ── StakingDb type ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_staking_db_empty() {
        let db   = make_db();
        let pool = db.lock().await;
        assert!(pool.validators.is_empty());
        assert!(pool.stakes.is_empty());
    }

    #[test]
    fn test_staking_router_builds() {
        let db = make_db();
        let _r = staking_router(db);
    }

    // ── GET /api/staking/stats ────────────────────────────────────────────

    #[tokio::test]
    async fn test_stats_total_staked() {
        let db   = populated_db();
        let pool = db.lock().await;
        assert_eq!(pool.total_staked, 35_000); // 10_000 + 5_000 + 20_000
    }

    #[tokio::test]
    async fn test_stats_validator_count() {
        let db   = populated_db();
        let pool = db.lock().await;
        assert_eq!(pool.validators.len(), 2);
    }

    #[tokio::test]
    async fn test_stats_active_validators() {
        let db   = populated_db();
        let pool = db.lock().await;
        let active = pool.validators.values().filter(|v| v.active).count();
        assert_eq!(active, 2);
    }

    #[tokio::test]
    async fn test_stats_stake_count() {
        let db   = populated_db();
        let pool = db.lock().await;
        assert_eq!(pool.stakes.len(), 3);
    }

    #[tokio::test]
    async fn test_stats_delegator_count() {
        let db   = populated_db();
        let pool = db.lock().await;
        let mut set = std::collections::HashSet::new();
        for s in &pool.stakes { set.insert(s.delegator.as_str()); }
        assert_eq!(set.len(), 3); // alice, bob, carol
    }

    // ── GET /api/staking/validators ───────────────────────────────────────

    #[tokio::test]
    async fn test_validators_list_count() {
        let db   = populated_db();
        let pool = db.lock().await;
        assert_eq!(pool.validators.len(), 2);
    }

    #[tokio::test]
    async fn test_validators_has_val1() {
        let db   = populated_db();
        let pool = db.lock().await;
        assert!(pool.validators.contains_key("val1"));
    }

    #[tokio::test]
    async fn test_validators_commission_bps() {
        let db   = populated_db();
        let pool = db.lock().await;
        assert_eq!(pool.validators["val1"].commission_bps, 500);
        assert_eq!(pool.validators["val2"].commission_bps, 1000);
    }

    #[tokio::test]
    async fn test_validator_total_stake() {
        let db   = populated_db();
        let pool = db.lock().await;
        assert_eq!(pool.total_staked_for("val1"), 15_000); // alice + bob
        assert_eq!(pool.total_staked_for("val2"), 20_000); // carol
    }

    #[tokio::test]
    async fn test_validators_share_sums_to_100() {
        let db   = populated_db();
        let pool = db.lock().await;
        let share1 = pool.total_staked_for("val1") as f64 / pool.total_staked as f64 * 100.0;
        let share2 = pool.total_staked_for("val2") as f64 / pool.total_staked as f64 * 100.0;
        assert!((share1 + share2 - 100.0).abs() < 0.01);
    }

    // ── GET /api/staking/validator/:addr ──────────────────────────────────

    #[tokio::test]
    async fn test_validator_found() {
        let db   = populated_db();
        let pool = db.lock().await;
        assert!(pool.validators.get("val1").is_some());
    }

    #[tokio::test]
    async fn test_validator_not_found() {
        let db   = make_db();
        let pool = db.lock().await;
        assert!(pool.validators.get("ghost").is_none());
    }

    #[tokio::test]
    async fn test_validator_delegators_count() {
        let db   = populated_db();
        let pool = db.lock().await;
        let count = pool.stakes.iter().filter(|s| s.validator == "val1").count();
        assert_eq!(count, 2); // alice + bob
    }

    #[tokio::test]
    async fn test_validator_active_flag() {
        let db   = populated_db();
        let pool = db.lock().await;
        assert!(pool.validators["val1"].active);
    }

    // ── GET /api/staking/delegator/:addr ──────────────────────────────────

    #[tokio::test]
    async fn test_delegator_stake_count() {
        let db   = populated_db();
        let pool = db.lock().await;
        let count = pool.stakes.iter().filter(|s| s.delegator == "alice").count();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_delegator_total_staked() {
        let db   = populated_db();
        let pool = db.lock().await;
        let total: u64 = pool.stakes.iter()
            .filter(|s| s.delegator == "alice")
            .map(|s| s.amount)
            .sum();
        assert_eq!(total, 10_000);
    }

    #[tokio::test]
    async fn test_delegator_unknown_has_no_stakes() {
        let db   = populated_db();
        let pool = db.lock().await;
        let count = pool.stakes.iter().filter(|s| s.delegator == "nobody").count();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_delegator_pending_rewards_zero_initially() {
        let db   = populated_db();
        let pool = db.lock().await;
        // No rewards distributed yet
        assert_eq!(pool.pending_rewards("alice"), 0);
        assert_eq!(pool.pending_rewards("carol"), 0);
    }

    // ── Rewards distribution ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_pending_rewards_after_distribution() {
        let mut pool = StakingPool::new();
        pool.register_validator("val1", 0).unwrap();
        pool.delegate("alice", "val1", 10_000, 0, 0).unwrap();
        pool.distribute_rewards(10_000);
        let pending = pool.pending_rewards("alice");
        assert!(pending > 0);
    }

    #[tokio::test]
    async fn test_rewards_proportional_to_stake() {
        let mut pool = StakingPool::new();
        pool.register_validator("val1", 0).unwrap();
        pool.delegate("alice", "val1", 10_000, 0, 0).unwrap();
        pool.delegate("bob",   "val1", 10_000, 0, 0).unwrap();
        pool.distribute_rewards(20_000);
        let alice = pool.pending_rewards("alice");
        let bob   = pool.pending_rewards("bob");
        // Equal stake → equal rewards (within rounding)
        assert!((alice as i64 - bob as i64).abs() <= 1);
    }

    // ── Lock mechanics ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_locked_until_correct() {
        let db   = populated_db();
        let pool = db.lock().await;
        let alice_stake = pool.stakes.iter()
            .find(|s| s.delegator == "alice")
            .unwrap();
        // lock_blocks=100, current_height=0 → locked_until=100
        assert_eq!(alice_stake.locked_until, 100);
    }

    #[tokio::test]
    async fn test_undelegate_fails_while_locked() {
        let mut pool = StakingPool::new();
        pool.register_validator("val1", 0).unwrap();
        pool.delegate("alice", "val1", 1_000, 100, 0).unwrap();
        let result = pool.undelegate("alice", "val1", 50); // block 50 < locked_until 100
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_undelegate_succeeds_after_lock() {
        let mut pool = StakingPool::new();
        pool.register_validator("val1", 0).unwrap();
        pool.delegate("alice", "val1", 1_000, 100, 0).unwrap();
        let result = pool.undelegate("alice", "val1", 100);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1_000);
    }
}
