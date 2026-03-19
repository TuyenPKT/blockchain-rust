#![allow(dead_code)]
//! v7.8 — Staking & Delegation

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validator {
    pub address: String,
    pub commission_bps: u32,
    pub total_stake: u64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stake {
    pub delegator: String,
    pub validator: String,
    pub amount: u64,
    pub locked_until: u64,
    pub reward_debt: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingPool {
    pub validators: HashMap<String, Validator>,
    pub stakes: Vec<Stake>,
    pub reward_per_token: u64,
    pub total_staked: u64,
}

impl StakingPool {
    pub fn new() -> Self {
        StakingPool {
            validators: HashMap::new(),
            stakes: Vec::new(),
            reward_per_token: 0,
            total_staked: 0,
        }
    }

    pub fn register_validator(&mut self, address: &str, commission_bps: u32) -> Result<(), String> {
        if self.validators.contains_key(address) {
            return Err(format!("Validator {} already registered", address));
        }
        self.validators.insert(address.to_string(), Validator {
            address: address.to_string(),
            commission_bps,
            total_stake: 0,
            active: true,
        });
        Ok(())
    }

    pub fn delegate(
        &mut self,
        delegator: &str,
        validator: &str,
        amount: u64,
        lock_blocks: u64,
        current_height: u64,
    ) -> Result<(), String> {
        if !self.validators.contains_key(validator) {
            return Err(format!("Validator {} not found", validator));
        }
        if amount == 0 {
            return Err("Amount must be > 0".to_string());
        }
        let locked_until = current_height + lock_blocks;
        self.stakes.push(Stake {
            delegator: delegator.to_string(),
            validator: validator.to_string(),
            amount,
            locked_until,
            reward_debt: self.reward_per_token,
        });
        self.validators.get_mut(validator).unwrap().total_stake += amount;
        self.total_staked += amount;
        Ok(())
    }

    pub fn undelegate(
        &mut self,
        delegator: &str,
        validator: &str,
        current_height: u64,
    ) -> Result<u64, String> {
        let pos = self.stakes.iter().position(|s| {
            s.delegator == delegator && s.validator == validator
        }).ok_or_else(|| format!("Stake not found for {} -> {}", delegator, validator))?;

        let stake = &self.stakes[pos];
        if current_height < stake.locked_until {
            return Err(format!(
                "Still locked until block {}, current {}",
                stake.locked_until, current_height
            ));
        }
        let amount = stake.amount;
        let val = stake.validator.clone();
        self.stakes.remove(pos);
        if let Some(v) = self.validators.get_mut(&val) {
            v.total_stake = v.total_stake.saturating_sub(amount);
        }
        self.total_staked = self.total_staked.saturating_sub(amount);
        Ok(amount)
    }

    pub fn distribute_rewards(&mut self, block_reward: u64) {
        if self.total_staked == 0 {
            return;
        }
        // reward_per_token uses scaled integer (multiply by 1e9 for precision)
        let reward_increment = block_reward.saturating_mul(1_000_000_000) / self.total_staked;
        self.reward_per_token = self.reward_per_token.saturating_add(reward_increment);
    }

    pub fn claim_rewards(&mut self, delegator: &str) -> u64 {
        let mut total = 0u64;
        let reward_per_token = self.reward_per_token;
        for stake in self.stakes.iter_mut() {
            if stake.delegator == delegator {
                let pending = stake.amount
                    .saturating_mul(reward_per_token.saturating_sub(stake.reward_debt))
                    / 1_000_000_000;
                total += pending;
                stake.reward_debt = reward_per_token;
            }
        }
        total
    }

    pub fn pending_rewards(&self, delegator: &str) -> u64 {
        let mut total = 0u64;
        for stake in &self.stakes {
            if stake.delegator == delegator {
                let pending = stake.amount
                    .saturating_mul(self.reward_per_token.saturating_sub(stake.reward_debt))
                    / 1_000_000_000;
                total += pending;
            }
        }
        total
    }

    pub fn slash(&mut self, validator: &str, slash_bps: u32) {
        if slash_bps == 0 || slash_bps > 10000 {
            return;
        }
        let mut total_slashed = 0u64;
        for stake in self.stakes.iter_mut() {
            if stake.validator == validator {
                let slash_amount = stake.amount.saturating_mul(slash_bps as u64) / 10000;
                stake.amount = stake.amount.saturating_sub(slash_amount);
                total_slashed += slash_amount;
            }
        }
        if let Some(v) = self.validators.get_mut(validator) {
            v.total_stake = v.total_stake.saturating_sub(total_slashed);
        }
        self.total_staked = self.total_staked.saturating_sub(total_slashed);
    }

    pub fn total_staked_for(&self, validator: &str) -> u64 {
        self.validators.get(validator).map(|v| v.total_stake).unwrap_or(0)
    }

    pub fn apy(&self, validator: &str, annual_reward: u64) -> f64 {
        let total = self.total_staked_for(validator);
        if total == 0 {
            return 0.0;
        }
        annual_reward as f64 / total as f64 * 100.0
    }

    /// v10.5 — Distribute `block_reward` and immediately collect claimable amounts.
    /// Returns `(delegator_address, amount)` pairs with amount > 0.
    /// Called once per block to auto-pay stakers in the coinbase TX.
    pub fn collect_block_rewards(&mut self, block_reward: u64) -> Vec<(String, u64)> {
        self.distribute_rewards(block_reward);
        // Collect unique delegators
        let mut delegators: Vec<String> = self.stakes.iter()
            .map(|s| s.delegator.clone())
            .collect();
        delegators.sort();
        delegators.dedup();
        // Claim and return non-zero rewards
        delegators.into_iter()
            .filter_map(|addr| {
                let amount = self.claim_rewards(&addr);
                if amount > 0 { Some((addr, amount)) } else { None }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pool() -> StakingPool {
        let mut pool = StakingPool::new();
        pool.register_validator("val1", 500).unwrap(); // 5% commission
        pool
    }

    #[test]
    fn test_register_validator() {
        let pool = make_pool();
        assert!(pool.validators.contains_key("val1"));
        assert_eq!(pool.validators["val1"].commission_bps, 500);
    }

    #[test]
    fn test_register_duplicate_fails() {
        let mut pool = make_pool();
        let r = pool.register_validator("val1", 100);
        assert!(r.is_err());
    }

    #[test]
    fn test_delegate() {
        let mut pool = make_pool();
        pool.delegate("alice", "val1", 1000, 100, 0).unwrap();
        assert_eq!(pool.total_staked, 1000);
        assert_eq!(pool.total_staked_for("val1"), 1000);
    }

    #[test]
    fn test_undelegate_unlocked() {
        let mut pool = make_pool();
        pool.delegate("alice", "val1", 1000, 10, 0).unwrap();
        let amount = pool.undelegate("alice", "val1", 11).unwrap();
        assert_eq!(amount, 1000);
        assert_eq!(pool.total_staked, 0);
    }

    #[test]
    fn test_undelegate_locked() {
        let mut pool = make_pool();
        pool.delegate("alice", "val1", 1000, 100, 0).unwrap();
        let r = pool.undelegate("alice", "val1", 50);
        assert!(r.is_err());
    }

    #[test]
    fn test_distribute_and_claim() {
        let mut pool = make_pool();
        pool.delegate("alice", "val1", 1000, 0, 0).unwrap();
        pool.distribute_rewards(100);
        let rewards = pool.claim_rewards("alice");
        assert!(rewards > 0);
    }

    #[test]
    fn test_slash() {
        let mut pool = make_pool();
        pool.delegate("alice", "val1", 1000, 0, 0).unwrap();
        pool.slash("val1", 1000); // 10% slash
        let stake = &pool.stakes[0];
        assert_eq!(stake.amount, 900);
    }

    #[test]
    fn test_apy() {
        let mut pool = make_pool();
        pool.delegate("alice", "val1", 10_000, 0, 0).unwrap();
        let apy = pool.apy("val1", 1000);
        assert!((apy - 10.0).abs() < 0.001); // 1000/10000 * 100 = 10%
    }

    #[test]
    fn test_pending_rewards() {
        let mut pool = make_pool();
        pool.delegate("alice", "val1", 1000, 0, 0).unwrap();
        pool.distribute_rewards(100);
        let pending = pool.pending_rewards("alice");
        assert!(pending > 0);
    }
}
