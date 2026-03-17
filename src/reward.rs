#![allow(dead_code)]
//! v7.0 — Block Reward Engine

pub const INITIAL_SUBSIDY: u64 = 50_000_000_000; // 50 PKT in satoshi
pub const HALVING_INTERVAL: u64 = 210_000;
pub const MAX_SUPPLY: u64 = 21_000_000 * 1_000_000_000; // 21M PKT

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
        supply.min(MAX_SUPPLY)
    }
}

pub fn cmd_reward_info() {
    println!("=== PKT Block Reward Schedule ===");
    println!("{:<6} {:<15} {:<20} {:<20}", "Era", "Start Block", "Subsidy (sat)", "Subsidy (PKT)");
    for era in 0..6u32 {
        let start_block = era as u64 * HALVING_INTERVAL;
        let subsidy = RewardEngine::subsidy_at(start_block);
        let pkt = subsidy as f64 / 1_000_000_000.0;
        println!("{:<6} {:<15} {:<20} {:<20.4}", era, start_block, subsidy, pkt);
    }
    let current_height = 840_000u64;
    let reward = RewardEngine::calculate(current_height, 50_000);
    println!("\n--- Current Block (height={}) ---", current_height);
    println!("Subsidy:     {} sat", reward.subsidy);
    println!("Fees:        {} sat", reward.fees);
    println!("Total:       {} sat", reward.total);
    println!("Halving Era: {}", reward.halving_era);
    println!("Blocks until next halving: {}", RewardEngine::blocks_until_next_halving(current_height));
    println!("Estimated supply: {} sat", RewardEngine::estimated_supply(current_height));
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
        assert_eq!(RewardEngine::subsidy_at(210_000), INITIAL_SUBSIDY / 2);
    }

    #[test]
    fn test_subsidy_at_second_halving() {
        assert_eq!(RewardEngine::subsidy_at(420_000), INITIAL_SUBSIDY / 4);
    }

    #[test]
    fn test_subsidy_at_third_halving() {
        assert_eq!(RewardEngine::subsidy_at(630_000), INITIAL_SUBSIDY / 8);
    }

    #[test]
    fn test_total_supply_never_exceeds_max() {
        let supply = RewardEngine::estimated_supply(21_000_000);
        assert!(supply <= MAX_SUPPLY, "supply {} > MAX_SUPPLY {}", supply, MAX_SUPPLY);
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
        assert_eq!(RewardEngine::halving_era(209_999), 0);
        assert_eq!(RewardEngine::halving_era(210_000), 1);
        assert_eq!(RewardEngine::halving_era(420_000), 2);
    }

    #[test]
    fn test_blocks_until_next_halving() {
        assert_eq!(RewardEngine::blocks_until_next_halving(0), 210_000);
        assert_eq!(RewardEngine::blocks_until_next_halving(100_000), 110_000);
        assert_eq!(RewardEngine::blocks_until_next_halving(210_000), 210_000);
    }

    #[test]
    fn test_subsidy_eventually_zero() {
        // After 64 halvings, subsidy should be 0
        assert_eq!(RewardEngine::subsidy_at(64 * HALVING_INTERVAL), 0);
    }
}
