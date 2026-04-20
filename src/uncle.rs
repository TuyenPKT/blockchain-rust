#![allow(dead_code)]
//! v26.1 — Uncle / Ommer tracking + reward
//!
//! Ethereum GHOST protocol uncle rules:
//!   - Uncle phải là block hợp lệ, không phải ancestor của block hiện tại
//!   - Uncle depth <= 7 (block hiện tại - uncle.number >= 1 && <= 7)
//!   - Mỗi block có tối đa 2 uncles
//!   - Uncle reward = block_reward * (uncle.number + 8 - block.number) / 8
//!   - Nephew reward (người include uncle) = block_reward / 32

use serde::{Deserialize, Serialize};

// ─── Uncle block header ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UncleHeader {
    pub number:      u64,
    pub hash:        [u8; 32],
    pub parent_hash: [u8; 32],
    pub miner:       [u8; 20],
    pub difficulty:  u64,
    pub timestamp:   u64,
}

// ─── Constants ────────────────────────────────────────────────────────────────

pub const MAX_UNCLES_PER_BLOCK: usize = 2;
pub const MAX_UNCLE_DEPTH:      u64   = 7;
pub const UNCLE_REWARD_DENOM:   u64   = 8;
pub const NEPHEW_REWARD_DENOM:  u64   = 32;

// ─── Uncle validation ─────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
pub enum UncleError {
    TooDeep,          // uncle.number <= block.number - 8
    SameAsParent,     // uncle.hash == block.parent_hash
    TooManyUncles,    // > 2 uncles in block
    Duplicate,        // uncle already included in this or recent block
    InvalidDepth,     // uncle.number >= block.number
}

pub fn validate_uncle(
    uncle: &UncleHeader,
    block_number: u64,
    block_parent_hash: &[u8; 32],
    already_included: &[[u8; 32]],
) -> Result<(), UncleError> {
    if uncle.number >= block_number {
        return Err(UncleError::InvalidDepth);
    }
    let depth = block_number - uncle.number;
    if depth > MAX_UNCLE_DEPTH {
        return Err(UncleError::TooDeep);
    }
    if &uncle.parent_hash == block_parent_hash && depth == 1 {
        return Err(UncleError::SameAsParent);
    }
    if already_included.iter().any(|h| h == &uncle.hash) {
        return Err(UncleError::Duplicate);
    }
    Ok(())
}

// ─── Reward calculation ───────────────────────────────────────────────────────

/// Uncle miner reward: block_reward * (uncle.number + 8 - block.number) / 8
/// Ethereum Yellow Paper eq. (163)
pub fn uncle_miner_reward(block_reward: u64, uncle_number: u64, block_number: u64) -> u64 {
    let numerator = uncle_number + UNCLE_REWARD_DENOM + 1 - block_number;
    block_reward * numerator / UNCLE_REWARD_DENOM
}

/// Nephew reward (block miner for including uncle): block_reward / 32
pub fn nephew_reward(block_reward: u64) -> u64 {
    block_reward / NEPHEW_REWARD_DENOM
}

/// Total extra reward for a block that includes `n` uncles.
pub fn total_nephew_reward(block_reward: u64, n_uncles: usize) -> u64 {
    nephew_reward(block_reward) * n_uncles as u64
}

// ─── Uncle pool (tracks recent uncles for inclusion) ──────────────────────────

pub struct UnclePool {
    /// uncle_hash → UncleHeader for headers seen but not yet included
    pending: std::collections::HashMap<[u8; 32], UncleHeader>,
    /// hashes already included in chain (prevent duplicate inclusion)
    included: std::collections::HashSet<[u8; 32]>,
}

impl UnclePool {
    pub fn new() -> Self {
        UnclePool {
            pending:  std::collections::HashMap::new(),
            included: std::collections::HashSet::new(),
        }
    }

    pub fn add_candidate(&mut self, uncle: UncleHeader) {
        self.pending.insert(uncle.hash, uncle);
    }

    pub fn mark_included(&mut self, hash: &[u8; 32]) {
        self.pending.remove(hash);
        self.included.insert(*hash);
    }

    pub fn is_included(&self, hash: &[u8; 32]) -> bool {
        self.included.contains(hash)
    }

    /// Pick up to MAX_UNCLES_PER_BLOCK valid uncles for a new block at `block_number`.
    pub fn pick_uncles(
        &self,
        block_number: u64,
        block_parent_hash: &[u8; 32],
    ) -> Vec<UncleHeader> {
        let included_vec: Vec<[u8; 32]> = self.included.iter().copied().collect();
        self.pending
            .values()
            .filter(|u| {
                validate_uncle(u, block_number, block_parent_hash, &included_vec).is_ok()
            })
            .take(MAX_UNCLES_PER_BLOCK)
            .cloned()
            .collect()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_hash() -> [u8; 32]  { [0u8; 32] }
    fn zero_addr() -> [u8; 20]  { [0u8; 20] }

    fn uncle(number: u64, hash_byte: u8, parent_hash: [u8; 32]) -> UncleHeader {
        let mut h = [0u8; 32];
        h[0] = hash_byte;
        UncleHeader { number, hash: h, parent_hash, miner: zero_addr(), difficulty: 1, timestamp: 0 }
    }

    #[test]
    fn test_uncle_reward_at_depth1() {
        // depth=1: uncle.number = block.number - 1
        // reward = block_reward * (uncle + 8 - block) / 8 = block_reward * 8/8 = block_reward
        // Wait: (uncle_number + 8 + 1 - block_number) / 8 = (block_number-1+8+1-block_number)/8 = 8/8 = 1
        let block_reward = 2_000_000_000u64;
        let r = uncle_miner_reward(block_reward, 9, 10);
        assert_eq!(r, block_reward); // 8/8
    }

    #[test]
    fn test_uncle_reward_at_depth7() {
        // depth=7: (uncle + 8 + 1 - block) = (3 + 8 + 1 - 10) = 2; reward = block_reward * 2/8
        let block_reward = 8_000;
        let r = uncle_miner_reward(block_reward, 3, 10);
        assert_eq!(r, block_reward * 2 / 8);
    }

    #[test]
    fn test_nephew_reward() {
        let r = nephew_reward(32_000);
        assert_eq!(r, 1_000);
    }

    #[test]
    fn test_total_nephew_reward_two_uncles() {
        let r = total_nephew_reward(32_000, 2);
        assert_eq!(r, 2_000);
    }

    #[test]
    fn test_validate_uncle_ok() {
        let u = uncle(9, 1, zero_hash());
        assert!(validate_uncle(&u, 10, &[1u8; 32], &[]).is_ok());
    }

    #[test]
    fn test_validate_uncle_too_deep() {
        let u = uncle(1, 1, zero_hash());
        assert_eq!(validate_uncle(&u, 10, &zero_hash(), &[]), Err(UncleError::TooDeep));
    }

    #[test]
    fn test_validate_uncle_invalid_depth_future() {
        let u = uncle(10, 1, zero_hash());
        assert_eq!(validate_uncle(&u, 10, &zero_hash(), &[]), Err(UncleError::InvalidDepth));
    }

    #[test]
    fn test_validate_uncle_duplicate() {
        let mut h = [0u8; 32]; h[0] = 0xAB;
        let mut different_parent = [0u8; 32]; different_parent[0] = 0xFF;
        let u = uncle(9, 0xAB, different_parent); // parent_hash != block_parent_hash → no SameAsParent
        assert_eq!(validate_uncle(&u, 10, &zero_hash(), &[h]), Err(UncleError::Duplicate));
    }

    #[test]
    fn test_uncle_pool_pick() {
        let mut pool = UnclePool::new();
        pool.add_candidate(uncle(9, 1, zero_hash()));
        pool.add_candidate(uncle(8, 2, zero_hash()));
        let picked = pool.pick_uncles(10, &[1u8; 32]);
        assert!(picked.len() <= MAX_UNCLES_PER_BLOCK);
        assert!(!picked.is_empty());
    }

    #[test]
    fn test_uncle_pool_mark_included() {
        let mut pool = UnclePool::new();
        let u = uncle(9, 5, zero_hash());
        pool.add_candidate(u.clone());
        pool.mark_included(&u.hash);
        assert!(pool.is_included(&u.hash));
        let picked = pool.pick_uncles(10, &[1u8; 32]);
        assert!(!picked.iter().any(|p| p.hash == u.hash));
    }

    #[test]
    fn test_max_uncles_per_block() {
        assert_eq!(MAX_UNCLES_PER_BLOCK, 2);
    }

    #[test]
    fn test_max_uncle_depth() {
        assert_eq!(MAX_UNCLE_DEPTH, 7);
    }
}
