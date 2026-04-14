#![allow(dead_code)]
//! v7.0 — Block Reward Engine

pub const INITIAL_SUBSIDY: u64 = crate::pkt_genesis::INITIAL_BLOCK_REWARD; // 20 PKT
pub const HALVING_INTERVAL: u64 = crate::pkt_genesis::HALVING_INTERVAL;   // 525,000 blocks (~365 ngày)

#[derive(Debug, Clone)]
pub struct BlockReward {
    pub subsidy: u64,
    pub fees: u64,
    pub total: u64,
    pub halving_era: u32,
}

#[derive(Debug, Clone)]
pub struct RewardEngine {
    pub halving_interval: u64,
    pub initial_subsidy: u64,
}

impl RewardEngine {
    pub fn new() -> Self {
        RewardEngine {
            halving_interval: HALVING_INTERVAL,
            initial_subsidy: INITIAL_SUBSIDY,
        }
    }

    pub fn subsidy_at(height: u64) -> u64 {
        let era = height / HALVING_INTERVAL;
        if era >= 64 {
            return 0;
        }
        let shifted = INITIAL_SUBSIDY >> era;
        shifted
    }

    pub fn calculate(height: u64, total_fees: u64) -> BlockReward {
        let subsidy = Self::subsidy_at(height);
        let era = Self::halving_era(height);
        BlockReward {
            subsidy,
            fees: total_fees,
            total: subsidy + total_fees,
            halving_era: era,
        }
    }

    pub fn halving_era(height: u64) -> u32 {
        (height / HALVING_INTERVAL) as u32
    }

    pub fn blocks_until_next_halving(height: u64) -> u64 {
        let next_halving = (height / HALVING_INTERVAL + 1) * HALVING_INTERVAL;
        next_halving - height
    }

    pub fn estimated_supply(height: u64) -> u64 {
        let mut supply: u64 = 0;
        let mut h = 0u64;
        while h < height {
            let era = h / HALVING_INTERVAL;
            let era_end = ((era + 1) * HALVING_INTERVAL).min(height);
            let blocks_in_era = era_end - h;
            let subsidy = Self::subsidy_at(h);
            let era_total = subsidy.saturating_mul(blocks_in_era);
            supply = supply.saturating_add(era_total);
            h = era_end;
            if h >= height {
                break;
            }
        }
        supply
    }
}

pub fn cmd_reward_info() {
    const PKT: u64 = 1_000_000_000; // paklets per PKT

    println!();
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                  PKT Block Reward — Halving Schedule                ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  Max supply    : TBD (chưa quyết định)");
    println!("  Halving every : {:>14} blocks  (~{:.1} years at 10min/block)",
        HALVING_INTERVAL,
        HALVING_INTERVAL as f64 * 600.0 / 86400.0 / 365.25);
    println!("  Genesis reward: {:>14} PKT / block", INITIAL_SUBSIDY / PKT);
    println!();
    println!("  {:<5} {:<12} {:<16} {:<14} {:<14}",
        "Era", "Start block", "Subsidy (PKT)", "Era total PKT", "Cumul. PKT");
    println!("  {}", "─".repeat(65));

    let mut cumulative = 0u64;
    for era in 0u64.. {
        let start = era * HALVING_INTERVAL;
        let subsidy = RewardEngine::subsidy_at(start);
        if subsidy == 0 { break; }

        let era_total = subsidy.saturating_mul(HALVING_INTERVAL);
        cumulative = cumulative.saturating_add(era_total);
        println!("  {:<5} {:<12} {:<16} {:<14} {:<14}",
            era,
            start,
            format!("{:.9}", subsidy as f64 / PKT as f64),
            format!("{:.3}", era_total as f64 / PKT as f64),
            format!("{:.3}", cumulative as f64 / PKT as f64),
        );
    }

    println!("  {}", "─".repeat(65));
    println!("  Total mined   : {:.3} PKT  (nếu mine đến era cuối)",
        cumulative as f64 / PKT as f64);
    println!();

    // Stats tại height hiện tại (ví dụ block đầu tiên)
    for h in [0u64, HALVING_INTERVAL, HALVING_INTERVAL*2, HALVING_INTERVAL*4, HALVING_INTERVAL*10] {
        let s = RewardEngine::subsidy_at(h);
        if s > 0 {
            println!("  Block #{:<10}: reward = {:.9} PKT  (era {})",
                h, s as f64 / PKT as f64, RewardEngine::halving_era(h));
        }
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subsidy_at_genesis() {
        assert_eq!(RewardEngine::subsidy_at(0), INITIAL_SUBSIDY);
    }

    #[test]
    fn test_subsidy_at_first_halving() {
        assert_eq!(RewardEngine::subsidy_at(HALVING_INTERVAL), INITIAL_SUBSIDY / 2);
    }

    #[test]
    fn test_subsidy_at_second_halving() {
        assert_eq!(RewardEngine::subsidy_at(HALVING_INTERVAL * 2), INITIAL_SUBSIDY / 4);
    }

    #[test]
    fn test_subsidy_at_third_halving() {
        assert_eq!(RewardEngine::subsidy_at(HALVING_INTERVAL * 3), INITIAL_SUBSIDY / 8);
    }

    #[test]
    fn test_estimated_supply_increases_with_height() {
        let s1 = RewardEngine::estimated_supply(HALVING_INTERVAL);
        let s2 = RewardEngine::estimated_supply(HALVING_INTERVAL * 2);
        assert!(s2 > s1, "supply at 2nd halving should exceed supply at 1st halving");
    }

    #[test]
    fn test_fees_included_in_total() {
        let reward = RewardEngine::calculate(0, 12345);
        assert_eq!(reward.total, reward.subsidy + reward.fees);
        assert_eq!(reward.fees, 12345);
    }

    #[test]
    fn test_halving_era_calculation() {
        assert_eq!(RewardEngine::halving_era(0), 0);
        assert_eq!(RewardEngine::halving_era(HALVING_INTERVAL - 1), 0);
        assert_eq!(RewardEngine::halving_era(HALVING_INTERVAL), 1);
        assert_eq!(RewardEngine::halving_era(HALVING_INTERVAL * 2), 2);
    }

    #[test]
    fn test_blocks_until_next_halving() {
        assert_eq!(RewardEngine::blocks_until_next_halving(0), HALVING_INTERVAL);
        assert_eq!(RewardEngine::blocks_until_next_halving(HALVING_INTERVAL / 2), HALVING_INTERVAL / 2);
        assert_eq!(RewardEngine::blocks_until_next_halving(HALVING_INTERVAL), HALVING_INTERVAL);
    }

    #[test]
    fn test_subsidy_eventually_zero() {
        // After 64 halvings, subsidy should be 0
        assert_eq!(RewardEngine::subsidy_at(64 * HALVING_INTERVAL), 0);
    }
}
