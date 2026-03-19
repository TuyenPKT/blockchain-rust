#![allow(dead_code)]
//! v11.8 — CLI Staking
//!
//! Quản lý staking và delegation từ command line.
//! Dữ liệu lưu trong RocksDB (`~/.pkt/db/`, key: `staking:pool`).
//!
//! Cách dùng:
//!   cargo run -- staking validators               liệt kê validators
//!   cargo run -- staking register  <addr> <commission_bps>
//!   cargo run -- staking delegate  <delegator> <validator> <amount>
//!   cargo run -- staking undelegate <delegator> <validator> <amount>
//!   cargo run -- staking rewards   <delegator>   xem pending rewards
//!   cargo run -- staking claim     <delegator>   claim rewards
//!   cargo run -- staking info      <validator>   chi tiết validator
//!   cargo run -- staking slash     <validator> <bps>  slash validator (admin)
//!
//! commission_bps: basis points (1 bps = 0.01%, 1000 bps = 10%)
//! locked_until: 0 (no lock) — có thể truyền block height nếu cần

use crate::staking::StakingPool;
use crate::storage::{load_staking_pool, save_staking_pool};

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn cmd_staking(args: &[String]) {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("help");
    match sub {
        "validators" => cmd_validators(),
        "register"   => cmd_register(args),
        "delegate"   => cmd_delegate(args),
        "undelegate" => cmd_undelegate(args),
        "rewards"    => cmd_rewards(args),
        "claim"      => cmd_claim(args),
        "info"       => cmd_info(args),
        "slash"      => cmd_slash(args),
        _            => print_help(),
    }
}

// ─── Sub-commands ─────────────────────────────────────────────────────────────

/// cargo run -- staking validators
fn cmd_validators() {
    let pool = load_staking_pool();
    if pool.validators.is_empty() {
        println!("  No validators registered.");
        println!("  Register one: cargo run -- staking register <addr> <commission_bps>");
        return;
    }
    println!();
    println!("  Validators ({}):", pool.validators.len());
    println!("  {:<44} {:<8} {:<16} {}", "Address", "Comm%", "Total Staked", "Active");
    println!("  {}", "-".repeat(80));
    let mut addrs: Vec<_> = pool.validators.keys().collect();
    addrs.sort();
    for addr in addrs {
        let v    = &pool.validators[addr];
        let comm = v.commission_bps as f64 / 100.0;
        println!("  {:<44} {:<8.2} {:<16} {}",
            v.address, comm, v.total_stake, if v.active { "yes" } else { "no" });
    }
    println!();
    println!("  Total staked: {}", pool.total_staked);
    println!();
}

/// cargo run -- staking register <addr> <commission_bps>
fn cmd_register(args: &[String]) {
    let addr   = require_arg(args, 3, "address");
    let comm   = args.get(4)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or_else(|| die("commission_bps must be a number"));

    let mut pool = load_staking_pool();
    match pool.register_validator(&addr, comm) {
        Ok(()) => {
            save_or_die(&pool);
            println!();
            println!("  Validator registered:");
            println!("  Address    : {}", addr);
            println!("  Commission : {} bps ({:.2}%)", comm, comm as f64 / 100.0);
            println!();
        }
        Err(e) => die(&format!("register failed: {}", e)),
    }
}

/// cargo run -- staking delegate <delegator> <validator> <amount>
fn cmd_delegate(args: &[String]) {
    let delegator = require_arg(args, 3, "delegator");
    let validator = require_arg(args, 4, "validator");
    let amount    = parse_u64(args, 5, "amount");

    let mut pool = load_staking_pool();
    match pool.delegate(&delegator, &validator, amount, 0, 0) {
        Ok(()) => {
            save_or_die(&pool);
            let total = pool.total_staked_for(&validator);
            println!();
            println!("  Delegated {} → {}", delegator, validator);
            println!("  Amount       : {}", amount);
            println!("  Total staked : {}", total);
            println!("  Pool total   : {}", pool.total_staked);
            println!();
        }
        Err(e) => die(&format!("delegate failed: {}", e)),
    }
}

/// cargo run -- staking undelegate <delegator> <validator>
fn cmd_undelegate(args: &[String]) {
    let delegator = require_arg(args, 3, "delegator");
    let validator = require_arg(args, 4, "validator");

    let mut pool = load_staking_pool();
    // current_height = 0 → only works if locked_until == 0 (no lock)
    match pool.undelegate(&delegator, &validator, 0) {
        Ok(amount) => {
            save_or_die(&pool);
            println!();
            println!("  Undelegated {} from {}", delegator, validator);
            println!("  Amount     : {}", amount);
            println!("  Pool total : {}", pool.total_staked);
            println!();
        }
        Err(e) => die(&format!("undelegate failed: {}", e)),
    }
}

/// cargo run -- staking rewards <delegator>
fn cmd_rewards(args: &[String]) {
    let delegator = require_arg(args, 3, "delegator");
    let pool      = load_staking_pool();
    let pending   = pool.pending_rewards(&delegator);

    // Collect stake info for this delegator
    let stakes: Vec<_> = pool.stakes.iter()
        .filter(|s| s.delegator == delegator)
        .collect();

    println!();
    println!("  Staking info: {}", delegator);
    if stakes.is_empty() {
        println!("  No active stakes.");
    } else {
        println!("  {:<44} {:<16} {}", "Validator", "Staked", "Locked Until");
        println!("  {}", "-".repeat(70));
        for s in &stakes {
            println!("  {:<44} {:<16} {}", s.validator, s.amount, s.locked_until);
        }
    }
    println!();
    println!("  Pending rewards: {}", pending);
    if pending == 0 {
        println!("  (Distribute rewards first with a block event, or use REST API)");
    }
    println!();
}

/// cargo run -- staking claim <delegator>
fn cmd_claim(args: &[String]) {
    let delegator = require_arg(args, 3, "delegator");
    let mut pool  = load_staking_pool();
    let before    = pool.pending_rewards(&delegator);
    let claimed   = pool.claim_rewards(&delegator);

    if claimed == 0 && before == 0 {
        println!("  No rewards to claim for {}.", delegator);
        println!("  (Rewards accumulate when blocks are mined with stakers registered)");
        return;
    }

    save_or_die(&pool);
    println!();
    println!("  Claimed rewards for: {}", delegator);
    println!("  Amount claimed: {}", claimed);
    println!();
}

/// cargo run -- staking info <validator>
fn cmd_info(args: &[String]) {
    let addr = require_arg(args, 3, "validator");
    let pool = load_staking_pool();

    match pool.validators.get(&addr) {
        None    => die(&format!("Validator '{}' not found", addr)),
        Some(v) => {
            let delegators: Vec<_> = pool.stakes.iter()
                .filter(|s| s.validator == addr)
                .collect();
            let apy = pool.apy(&addr, 6_250_000_000); // ~6.25 PKT/block * 1M blocks/year est

            println!();
            println!("  Validator: {}", addr);
            println!("  Commission  : {} bps ({:.2}%)", v.commission_bps, v.commission_bps as f64 / 100.0);
            println!("  Total staked: {}", v.total_stake);
            println!("  Active      : {}", v.active);
            println!("  Delegators  : {}", delegators.len());
            println!("  Est. APY    : {:.2}%", apy);
            if !delegators.is_empty() {
                println!();
                println!("  Delegators:");
                for s in &delegators {
                    println!("    {} — {} staked", s.delegator, s.amount);
                }
            }
            println!();
        }
    }
}

/// cargo run -- staking slash <validator> <slash_bps>
fn cmd_slash(args: &[String]) {
    let addr      = require_arg(args, 3, "validator");
    let slash_bps = args.get(4)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or_else(|| die("slash_bps must be a number"));

    let mut pool = load_staking_pool();
    if !pool.validators.contains_key(&addr) {
        die(&format!("Validator '{}' not found", addr));
    }

    let before = pool.total_staked_for(&addr);
    pool.slash(&addr, slash_bps);
    let after = pool.total_staked_for(&addr);
    save_or_die(&pool);

    println!();
    println!("  Slashed validator: {}", addr);
    println!("  Slash bps  : {} ({:.2}%)", slash_bps, slash_bps as f64 / 100.0);
    println!("  Staked before: {}", before);
    println!("  Staked after : {}", after);
    println!("  Slashed amt  : {}", before.saturating_sub(after));
    println!();
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn require_arg(args: &[String], idx: usize, name: &str) -> String {
    args.get(idx).cloned()
        .unwrap_or_else(|| die(&format!("missing argument: <{}>", name)))
}

fn parse_u64(args: &[String], idx: usize, name: &str) -> u64 {
    args.get(idx)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| die(&format!("{} must be a non-negative integer", name)))
}

fn save_or_die(pool: &StakingPool) {
    if let Err(e) = save_staking_pool(pool) {
        die(&format!("failed to save staking pool: {}", e));
    }
}

fn die(msg: &str) -> ! {
    eprintln!("  Error: {}", msg);
    std::process::exit(1);
}

// ─── Help ─────────────────────────────────────────────────────────────────────

fn print_help() {
    println!();
    println!("  staking commands:");
    println!("    cargo run -- staking validators");
    println!("    cargo run -- staking register    <addr> <commission_bps>");
    println!("    cargo run -- staking delegate    <delegator> <validator> <amount>");
    println!("    cargo run -- staking undelegate  <delegator> <validator> <amount>");
    println!("    cargo run -- staking rewards     <delegator>");
    println!("    cargo run -- staking claim       <delegator>");
    println!("    cargo run -- staking info        <validator>");
    println!("    cargo run -- staking slash       <validator> <slash_bps>");
    println!();
    println!("  commission_bps: 1 = 0.01%, 100 = 1%, 1000 = 10%, 10000 = 100%");
    println!("  slash_bps     : portion of stake to slash (basis points)");
    println!();
    println!("  REST API: GET /api/staking/* (read) via pktscan server");
    println!("  Data stored at: ~/.pkt/db/ (RocksDB, key: staking:pool)");
    println!();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::staking::StakingPool;

    fn make_pool() -> StakingPool {
        let mut p = StakingPool::new();
        p.register_validator("val1", 500).unwrap();   // 5% commission
        p.register_validator("val2", 1000).unwrap();  // 10% commission
        p
    }

    // ── StakingPool serde ─────────────────────────────────────────────────────

    #[test]
    fn test_pool_serializes() {
        let pool = make_pool();
        let json = serde_json::to_string(&pool);
        assert!(json.is_ok());
        assert!(json.unwrap().contains("val1"));
    }

    #[test]
    fn test_pool_roundtrip() {
        let mut pool = make_pool();
        pool.delegate("del1", "val1", 1000, 0, 0).unwrap();
        let json   = serde_json::to_vec(&pool).unwrap();
        let pool2: StakingPool = serde_json::from_slice(&json).unwrap();
        assert_eq!(pool2.total_staked, pool.total_staked);
        assert_eq!(pool2.stakes.len(), 1);
        assert_eq!(pool2.stakes[0].delegator, "del1");
    }

    #[test]
    fn test_pool_roundtrip_validators() {
        let pool   = make_pool();
        let json   = serde_json::to_vec(&pool).unwrap();
        let pool2: StakingPool = serde_json::from_slice(&json).unwrap();
        assert!(pool2.validators.contains_key("val1"));
        assert!(pool2.validators.contains_key("val2"));
        assert_eq!(pool2.validators["val1"].commission_bps, 500);
    }

    // ── register_validator ────────────────────────────────────────────────────

    #[test]
    fn test_register_validator_ok() {
        let mut p = StakingPool::new();
        assert!(p.register_validator("val1", 100).is_ok());
        assert!(p.validators.contains_key("val1"));
    }

    #[test]
    fn test_register_validator_duplicate() {
        let mut p = StakingPool::new();
        p.register_validator("val1", 100).unwrap();
        let err = p.register_validator("val1", 200).unwrap_err();
        assert!(err.contains("already registered"));
    }

    // ── delegate / undelegate ─────────────────────────────────────────────────

    #[test]
    fn test_delegate_ok() {
        let mut p = make_pool();
        assert!(p.delegate("del1", "val1", 500, 0, 0).is_ok());
        assert_eq!(p.total_staked, 500);
        assert_eq!(p.total_staked_for("val1"), 500);
    }

    #[test]
    fn test_delegate_unknown_validator() {
        let mut p = make_pool();
        let err = p.delegate("del1", "unknown", 100, 0, 0).unwrap_err();
        assert!(err.contains("not found") || err.contains("Validator"));
    }

    #[test]
    fn test_undelegate_ok() {
        let mut p = make_pool();
        p.delegate("del1", "val1", 1000, 0, 0).unwrap();
        let returned = p.undelegate("del1", "val1", 0).unwrap();
        assert_eq!(returned, 1000);
        assert_eq!(p.total_staked, 0);
    }

    #[test]
    fn test_undelegate_not_found() {
        let mut p = make_pool();
        let err = p.undelegate("nobody", "val1", 0).unwrap_err();
        assert!(err.contains("not found") || err.contains("Stake"));
    }

    // ── rewards ───────────────────────────────────────────────────────────────

    #[test]
    fn test_pending_rewards_zero_before_distribute() {
        let mut p = make_pool();
        p.delegate("del1", "val1", 1000, 0, 0).unwrap();
        assert_eq!(p.pending_rewards("del1"), 0);
    }

    #[test]
    fn test_distribute_then_pending() {
        let mut p = make_pool();
        p.delegate("del1", "val1", 1000, 0, 0).unwrap();
        p.distribute_rewards(10_000);
        assert!(p.pending_rewards("del1") > 0);
    }

    #[test]
    fn test_claim_rewards_clears_pending() {
        let mut p = make_pool();
        p.delegate("del1", "val1", 1000, 0, 0).unwrap();
        p.distribute_rewards(10_000);
        let claimed = p.claim_rewards("del1");
        assert!(claimed > 0);
        assert_eq!(p.pending_rewards("del1"), 0);
    }

    #[test]
    fn test_claim_no_rewards_returns_zero() {
        let mut p = make_pool();
        assert_eq!(p.claim_rewards("nobody"), 0);
    }

    // ── slash ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_slash_reduces_stake() {
        let mut p = make_pool();
        p.delegate("del1", "val1", 10_000, 0, 0).unwrap();
        let before = p.total_staked_for("val1");
        p.slash("val1", 1000); // 10%
        let after = p.total_staked_for("val1");
        assert!(after < before);
    }

    #[test]
    fn test_slash_unknown_validator_noop() {
        let mut p = make_pool();
        p.slash("nobody", 500); // should not panic
    }

    // ── apy ───────────────────────────────────────────────────────────────────

    #[test]
    fn test_apy_zero_when_no_stake() {
        let p = make_pool();
        assert_eq!(p.apy("val1", 1_000_000), 0.0);
    }

    #[test]
    fn test_apy_positive_after_staking() {
        let mut p = make_pool();
        p.delegate("del1", "val1", 10_000, 0, 0).unwrap();
        let apy = p.apy("val1", 1_000_000_000);
        assert!(apy > 0.0);
    }

    // ── collect_block_rewards ─────────────────────────────────────────────────

    #[test]
    fn test_collect_block_rewards_returns_payouts() {
        let mut p = make_pool();
        p.delegate("del1", "val1", 1000, 0, 0).unwrap();
        let payouts = p.collect_block_rewards(100_000);
        // del1 should have received something
        assert!(!payouts.is_empty());
        assert_eq!(payouts[0].0, "del1");
        assert!(payouts[0].1 > 0);
    }
}
