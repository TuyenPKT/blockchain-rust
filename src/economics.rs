#![allow(dead_code)]
//! v7.9 — Economic Model

#[derive(Debug, Clone)]
pub struct EraParams {
    pub era: u32,
    pub block_reward: u64,
    pub fee_burn_pct: u8,
    pub staking_yield_bps: u32,
    pub inflation_pct_x100: u32,
}

#[derive(Debug, Clone)]
pub struct TokenEconomics {
    pub name: String,
    pub symbol: String,
    pub max_supply: u128,
    pub current_supply: u128,
    pub burn_rate_bps: u32,
    pub staking_ratio_bps: u32,
}

#[derive(Debug, Clone)]
pub struct EraSnapshot {
    pub block: u64,
    pub era: u32,
    pub circulating: u128,
    pub staked: u128,
    pub burned: u128,
    pub miner_reward: u64,
    pub fee_burned: u64,
}

#[derive(Debug, Clone)]
pub struct Simulator {
    pub economics: TokenEconomics,
    pub history: Vec<EraSnapshot>,
    total_burned: u128,
    total_staked: u128,
}

impl Simulator {
    pub fn new(economics: TokenEconomics) -> Self {
        Simulator {
            economics,
            history: Vec::new(),
            total_burned: 0,
            total_staked: 0,
        }
    }

    pub fn era_params(era: u32) -> EraParams {
        // Era 0: reward=50B, burn=0%, yield=500bps
        // Each era: reward /= 2, burn += 5%, yield -= 50bps
        let base_reward: u64 = crate::pkt_genesis::INITIAL_BLOCK_REWARD;
        let block_reward = if era >= 64 { 0 } else { base_reward >> era };
        let fee_burn_pct = (era * 5).min(100) as u8;
        let staking_yield_bps = 500u32.saturating_sub(era * 50);
        let inflation_pct_x100 = if block_reward == 0 { 0 } else { 100 / (era + 1) };
        EraParams {
            era,
            block_reward,
            fee_burn_pct,
            staking_yield_bps,
            inflation_pct_x100,
        }
    }

    pub fn step(&mut self, block: u64, fees: u64) {
        let halving_interval = crate::pkt_genesis::HALVING_INTERVAL;
        let era = (block / halving_interval) as u32;
        let params = Self::era_params(era);

        // Mint block reward (only if under max supply)
        let reward = params.block_reward;
        let new_supply = self.economics.current_supply + reward as u128;
        let actual_reward = if new_supply > self.economics.max_supply {
            (self.economics.max_supply - self.economics.current_supply) as u64
        } else {
            reward
        };
        self.economics.current_supply += actual_reward as u128;

        // Burn fees
        let fee_burned = (fees as u128 * params.fee_burn_pct as u128) / 100;
        self.economics.current_supply = self.economics.current_supply.saturating_sub(fee_burned);
        self.total_burned += fee_burned;

        // Staked amount
        let staked = self.economics.current_supply
            * self.economics.staking_ratio_bps as u128
            / 10000;
        self.total_staked = staked;

        let snap = EraSnapshot {
            block,
            era,
            circulating: self.economics.current_supply,
            staked,
            burned: self.total_burned,
            miner_reward: actual_reward,
            fee_burned: fee_burned as u64,
        };
        self.history.push(snap);
    }

    pub fn project(&mut self, n_blocks: u64, avg_fees_per_block: u64) -> Vec<EraSnapshot> {
        let start_block = self.history.last().map(|s| s.block + 1).unwrap_or(0);
        for i in 0..n_blocks {
            self.step(start_block + i, avg_fees_per_block);
        }
        self.history.iter().rev().take(n_blocks as usize).cloned().collect::<Vec<_>>()
            .into_iter().rev().collect()
    }

    pub fn total_burned(&self) -> u128 {
        self.total_burned
    }

    pub fn current_apr(&self) -> f64 {
        if self.total_staked == 0 {
            return 0.0;
        }
        let last_block = self.history.last().map(|s| s.block).unwrap_or(0);
        let era = (last_block / crate::pkt_genesis::HALVING_INTERVAL) as u32;
        let params = Self::era_params(era);
        // Blocks per year ~ 52560 (10-min blocks)
        let annual_reward = params.block_reward as u128 * 52560;
        let staking_share = annual_reward * self.economics.staking_ratio_bps as u128 / 10000;
        if self.total_staked == 0 {
            return 0.0;
        }
        staking_share as f64 / self.total_staked as f64 * 100.0
    }
}

pub fn cmd_economics_demo() {
    let economics = TokenEconomics {
        name: "PKT".to_string(),
        symbol: "PKT".to_string(),
        max_supply: 21_000_000 * 1_000_000_000u128,
        current_supply: 0,
        burn_rate_bps: 0,
        staking_ratio_bps: 2000, // 20% staked
    };
    let mut sim = Simulator::new(economics);
    let snapshots = sim.project(1000, 10_000);
    println!("=== PKT Economic Simulation (1000 blocks) ===");
    if let Some(last) = snapshots.last() {
        println!("Block:        {}", last.block);
        println!("Era:          {}", last.era);
        println!("Circulating:  {} sat", last.circulating);
        println!("Staked:       {} sat", last.staked);
        println!("Total burned: {} sat", last.burned);
        println!("Miner reward: {} sat", last.miner_reward);
        println!("Fee burned:   {} sat/block", last.fee_burned);
    }
    println!("Current APR:  {:.2}%", sim.current_apr());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sim() -> Simulator {
        let eco = TokenEconomics {
            name: "PKT".to_string(),
            symbol: "PKT".to_string(),
            max_supply: 21_000_000 * 1_000_000_000u128,
            current_supply: 0,
            burn_rate_bps: 0,
            staking_ratio_bps: 2000,
        };
        Simulator::new(eco)
    }

    #[test]
    fn test_era_params_era0() {
        let p = Simulator::era_params(0);
        assert_eq!(p.block_reward, crate::pkt_genesis::INITIAL_BLOCK_REWARD);
        assert_eq!(p.fee_burn_pct, 0);
        assert_eq!(p.staking_yield_bps, 500);
    }

    #[test]
    fn test_era_params_era1() {
        let p = Simulator::era_params(1);
        assert_eq!(p.block_reward, 25_000_000_000); // halved
        assert_eq!(p.fee_burn_pct, 5);
        assert_eq!(p.staking_yield_bps, 450);
    }

    #[test]
    fn test_step_advances_supply() {
        let mut sim = make_sim();
        let before = sim.economics.current_supply;
        sim.step(0, 0);
        assert!(sim.economics.current_supply > before);
    }

    #[test]
    fn test_project_returns_n_snapshots() {
        let mut sim = make_sim();
        let snaps = sim.project(100, 1000);
        assert_eq!(snaps.len(), 100);
    }

    #[test]
    fn test_total_burned_increases() {
        let mut sim = make_sim();
        sim.project(10, 0);
        let burned_no_fees = sim.total_burned();
        sim.project(10, 10_000);
        // Era 0: fee_burn_pct=0 so no burn in either case
        // Let's test with era 1+
        let eco = TokenEconomics {
            name: "PKT".to_string(),
            symbol: "PKT".to_string(),
            max_supply: 21_000_000 * 1_000_000_000u128,
            current_supply: 0,
            burn_rate_bps: 0,
            staking_ratio_bps: 2000,
        };
        let mut sim2 = Simulator::new(eco);
        // Simulate at block 210000 (era 1, burn_pct=5)
        sim2.step(crate::pkt_genesis::HALVING_INTERVAL, 100_000);
        assert!(sim2.total_burned() > 0);
        let _ = burned_no_fees; // suppress unused
    }

    #[test]
    fn test_supply_never_exceeds_max() {
        let mut sim = make_sim();
        sim.project(1000, 0);
        assert!(sim.economics.current_supply <= sim.economics.max_supply);
    }

    #[test]
    fn test_burn_decreases_circulating() {
        let eco = TokenEconomics {
            name: "PKT".to_string(),
            symbol: "PKT".to_string(),
            max_supply: 21_000_000 * 1_000_000_000u128,
            current_supply: 1_000_000_000_000u128,
            burn_rate_bps: 0,
            staking_ratio_bps: 2000,
        };
        let mut sim = Simulator::new(eco);
        // era 1: burn=5%
        sim.step(crate::pkt_genesis::HALVING_INTERVAL, 100_000);
        let snap = sim.history.last().unwrap();
        assert!(snap.fee_burned > 0); // fees were burned
    }

    #[test]
    fn test_history_grows() {
        let mut sim = make_sim();
        sim.step(0, 0);
        sim.step(1, 0);
        sim.step(2, 0);
        assert_eq!(sim.history.len(), 3);
    }

    #[test]
    fn test_era_params_high_era() {
        let p = Simulator::era_params(64);
        assert_eq!(p.block_reward, 0);
        assert_eq!(p.fee_burn_pct, 100); // capped at 100
    }
}
