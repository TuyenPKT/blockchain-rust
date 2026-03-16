#![allow(dead_code)]
//! v6.3 — Parallel Block Validation
//!
//! Validates N blocks simultaneously using `rayon::par_iter()`.
//!
//! Two validation passes:
//!   1. Individual (parallel) — hash, difficulty, witness_root, coinbase, tx validity
//!   2. Chain links (parallel) — prev_hash[i] == hash[i-1] for each consecutive pair
//!
//! Each pass returns a `Vec<ValidationResult>`.
//! `ChainValidationReport` aggregates both passes.
//!
//! Structs:
//!   ValidationError — typed error enum
//!   ValidationResult { block_index, is_valid, errors }
//!   ChainValidationReport { results, all_valid, invalid_count, elapsed_ms }
//!   ParallelValidator { difficulty } — reusable validator
//!
//! Free functions:
//!   validate_block(block, difficulty) -> ValidationResult
//!   validate_blocks_parallel(blocks, difficulty) -> ChainValidationReport

use std::time::Instant;
use rayon::prelude::*;

use crate::block::Block;

// ── Error types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// Stored hash doesn't match recomputed hash.
    HashMismatch { expected: String, got: String },

    /// Hash doesn't satisfy required leading zeros.
    DifficultyNotMet { required: usize, hash: String },

    /// Stored witness_root doesn't match recomputed value.
    WitnessRootMismatch { expected: String, got: String },

    /// Non-genesis block is missing a coinbase transaction.
    MissingCoinbase,

    /// A transaction within the block failed internal validation.
    InvalidTransaction { tx_index: usize },

    /// Block's prev_hash doesn't match hash of the preceding block.
    BrokenChainLink { block_index: u64, expected: String, got: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HashMismatch { expected, got } =>
                write!(f, "hash mismatch: expected {}…, got {}…", &expected[..8], &got[..8]),
            Self::DifficultyNotMet { required, hash } =>
                write!(f, "difficulty {} not met by {}", required, &hash[..8]),
            Self::WitnessRootMismatch { .. } =>
                write!(f, "witness_root mismatch"),
            Self::MissingCoinbase =>
                write!(f, "missing coinbase tx"),
            Self::InvalidTransaction { tx_index } =>
                write!(f, "invalid tx at index {}", tx_index),
            Self::BrokenChainLink { block_index, expected, got } =>
                write!(f, "block {} chain break: prev_hash expected {}…, got {}…",
                    block_index, &expected[..8], &got[..8]),
        }
    }
}

// ── ValidationResult ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub block_index: u64,
    pub is_valid:    bool,
    pub errors:      Vec<ValidationError>,
}

impl ValidationResult {
    fn ok(block_index: u64) -> Self {
        ValidationResult { block_index, is_valid: true, errors: vec![] }
    }
    fn fail(block_index: u64, errors: Vec<ValidationError>) -> Self {
        ValidationResult { block_index, is_valid: false, errors }
    }
}

// ── ChainValidationReport ─────────────────────────────────────────────────────

pub struct ChainValidationReport {
    /// One result per block (individual + link errors merged).
    pub results:       Vec<ValidationResult>,
    pub all_valid:     bool,
    pub invalid_count: usize,
    pub elapsed_ms:    u64,
}

impl ChainValidationReport {
    pub fn print_summary(&self) {
        println!("  Validation: {} blocks  {} invalid  {}ms",
            self.results.len(), self.invalid_count, self.elapsed_ms);
        for r in &self.results {
            if !r.is_valid {
                for e in &r.errors {
                    println!("  ❌ block #{}: {}", r.block_index, e);
                }
            }
        }
        if self.all_valid {
            println!("  ✅ All blocks valid");
        }
    }
}

// ── Individual block validation (parallelizable) ──────────────────────────────

/// Validate a single block's internal consistency.
/// Does NOT check chain linkage (prev_hash of neighbours).
pub fn validate_block(block: &Block, difficulty: usize) -> ValidationResult {
    let mut errors = Vec::new();

    // 1. Hash correctness
    let expected = Block::calculate_hash(
        block.index, block.timestamp,
        &block.transactions, &block.prev_hash, block.nonce,
    );
    if block.hash != expected {
        errors.push(ValidationError::HashMismatch {
            expected: expected.clone(),
            got:      block.hash.clone(),
        });
    }

    // 2. Difficulty (genesis block index=0 exempt)
    if block.index > 0 && !block.hash.starts_with(&"0".repeat(difficulty)) {
        errors.push(ValidationError::DifficultyNotMet {
            required: difficulty,
            hash:     block.hash.clone(),
        });
    }

    // 3. Witness root
    let expected_wr = Block::merkle_root_wtxid(&block.transactions);
    if block.witness_root != expected_wr {
        errors.push(ValidationError::WitnessRootMismatch {
            expected: expected_wr,
            got:      block.witness_root.clone(),
        });
    }

    // 4. Coinbase (non-genesis blocks)
    if block.index > 0 && !block.has_coinbase() {
        errors.push(ValidationError::MissingCoinbase);
    }

    // 5. Transaction validity
    for (i, tx) in block.transactions.iter().enumerate() {
        if !tx.is_valid() {
            errors.push(ValidationError::InvalidTransaction { tx_index: i });
        }
    }

    if errors.is_empty() {
        ValidationResult::ok(block.index)
    } else {
        ValidationResult::fail(block.index, errors)
    }
}

// ── Parallel validation ───────────────────────────────────────────────────────

/// Validate N blocks in parallel (individual checks only, no chain link).
/// Returns one `ValidationResult` per block, in original order.
pub fn validate_blocks_individual_parallel(
    blocks:     &[Block],
    difficulty: usize,
) -> Vec<ValidationResult> {
    blocks.par_iter()
        .map(|b| validate_block(b, difficulty))
        .collect()
}

/// Validate chain linkage in parallel: blocks[i].prev_hash == blocks[i-1].hash.
/// Returns one result per block (block[0] is always valid — no previous).
pub fn validate_chain_links_parallel(blocks: &[Block]) -> Vec<ValidationResult> {
    if blocks.is_empty() { return vec![]; }

    (0..blocks.len()).into_par_iter().map(|i| {
        if i == 0 {
            return ValidationResult::ok(blocks[i].index);
        }
        let prev = &blocks[i - 1];
        let curr = &blocks[i];
        if curr.prev_hash != prev.hash {
            ValidationResult::fail(curr.index, vec![
                ValidationError::BrokenChainLink {
                    block_index: curr.index,
                    expected:    prev.hash.clone(),
                    got:         curr.prev_hash.clone(),
                }
            ])
        } else {
            ValidationResult::ok(curr.index)
        }
    }).collect()
}

/// Full parallel validation: individual checks + chain links.
/// Merges results from both passes into a single `ChainValidationReport`.
pub fn validate_blocks_parallel(
    blocks:     &[Block],
    difficulty: usize,
) -> ChainValidationReport {
    let t0 = Instant::now();

    // Both passes run independently (no shared mutable state)
    let (individual, links): (Vec<ValidationResult>, Vec<ValidationResult>) = rayon::join(
        || validate_blocks_individual_parallel(blocks, difficulty),
        || validate_chain_links_parallel(blocks),
    );

    // Merge: group by block index
    let mut merged: Vec<ValidationResult> = individual;
    for link_result in links {
        if !link_result.is_valid {
            // Find the matching block result and append errors
            if let Some(r) = merged.iter_mut().find(|r| r.block_index == link_result.block_index) {
                r.errors.extend(link_result.errors);
                r.is_valid = false;
            }
        }
    }

    let invalid_count = merged.iter().filter(|r| !r.is_valid).count();
    let elapsed_ms    = t0.elapsed().as_millis() as u64;

    ChainValidationReport {
        all_valid: invalid_count == 0,
        invalid_count,
        elapsed_ms,
        results: merged,
    }
}

// ── ParallelValidator ─────────────────────────────────────────────────────────

/// Reusable validator with configured difficulty.
pub struct ParallelValidator {
    pub difficulty: usize,
}

impl ParallelValidator {
    pub fn new(difficulty: usize) -> Self {
        ParallelValidator { difficulty }
    }

    pub fn validate_one(&self, block: &Block) -> ValidationResult {
        validate_block(block, self.difficulty)
    }

    pub fn validate_many(&self, blocks: &[Block]) -> ChainValidationReport {
        validate_blocks_parallel(blocks, self.difficulty)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Block;
    use crate::transaction::Transaction;

    const ADDR: &str = "aabbccddee112233445566778899aabbccddee11";

    /// Build and mine a valid block at difficulty 1.
    /// Non-genesis blocks include a coinbase tx (required by validator).
    fn mined_block(index: u64, prev_hash: &str) -> Block {
        let txs = if index > 0 {
            vec![Transaction::coinbase_at(ADDR, 0, index)]
        } else {
            vec![]
        };
        let mut b = Block::new(index, txs, prev_hash.to_string());
        b.mine(1);
        b
    }

    #[test]
    fn test_validate_block_valid_genesis() {
        let genesis = mined_block(0, "0".repeat(64).as_str());
        let r = validate_block(&genesis, 1);
        assert!(r.is_valid, "{:?}", r.errors);
    }

    #[test]
    fn test_validate_block_valid_non_genesis() {
        let genesis = mined_block(0, "0".repeat(64).as_str());
        let block1  = mined_block(1, &genesis.hash);
        let r = validate_block(&block1, 1);
        assert!(r.is_valid, "{:?}", r.errors);
    }

    #[test]
    fn test_validate_block_hash_mismatch() {
        let mut b = mined_block(1, "0".repeat(64).as_str());
        b.hash = "ff".repeat(32);  // tamper with hash
        let r = validate_block(&b, 1);
        assert!(!r.is_valid);
        assert!(r.errors.iter().any(|e| matches!(e, ValidationError::HashMismatch { .. })));
    }

    #[test]
    fn test_validate_block_difficulty_not_met() {
        // Build block with a hash that doesn't meet difficulty 4
        let b = mined_block(1, "0".repeat(64).as_str());
        // Force a recalculation: use the real hash but check at higher difficulty
        let r = validate_block(&b, 4);
        if b.hash.starts_with("0000") {
            assert!(r.is_valid);
        } else {
            assert!(!r.is_valid);
            assert!(r.errors.iter().any(|e| matches!(e, ValidationError::DifficultyNotMet { .. })));
        }
    }

    #[test]
    fn test_validate_blocks_parallel_all_valid() {
        let g  = mined_block(0, "0".repeat(64).as_str());
        let b1 = mined_block(1, &g.hash);
        let b2 = mined_block(2, &b1.hash);
        let blocks = vec![g, b1, b2];

        let report = validate_blocks_parallel(&blocks, 1);
        assert!(report.all_valid, "invalid: {:?}",
            report.results.iter().filter(|r| !r.is_valid).collect::<Vec<_>>());
        assert_eq!(report.invalid_count, 0);
    }

    #[test]
    fn test_validate_chain_links_valid() {
        let g  = mined_block(0, "0".repeat(64).as_str());
        let b1 = mined_block(1, &g.hash);
        let b2 = mined_block(2, &b1.hash);
        let blocks = vec![g, b1, b2];

        let results = validate_chain_links_parallel(&blocks);
        assert!(results.iter().all(|r| r.is_valid));
    }

    #[test]
    fn test_validate_chain_links_broken() {
        let g  = mined_block(0, "0".repeat(64).as_str());
        let mut b1 = mined_block(1, &g.hash);
        b1.prev_hash = "b".repeat(64);  // break the link
        let blocks = vec![g, b1];

        let results = validate_chain_links_parallel(&blocks);
        assert!(!results[1].is_valid);
        assert!(results[1].errors.iter().any(|e| matches!(e, ValidationError::BrokenChainLink { .. })));
    }

    #[test]
    fn test_parallel_validator_struct() {
        let v  = ParallelValidator::new(1);
        let g  = mined_block(0, "0".repeat(64).as_str());
        let b1 = mined_block(1, &g.hash);
        let r  = v.validate_one(&b1);
        assert!(r.is_valid, "{:?}", r.errors);
    }

    #[test]
    fn test_parallel_validator_many() {
        let g  = mined_block(0, "0".repeat(64).as_str());
        let b1 = mined_block(1, &g.hash);
        let b2 = mined_block(2, &b1.hash);
        let v  = ParallelValidator::new(1);
        let report = v.validate_many(&[g, b1, b2]);
        assert!(report.all_valid);
    }

    #[test]
    fn test_validate_individual_parallel_10_blocks() {
        // Build a chain of 10 blocks and validate individual in parallel
        let mut blocks = Vec::new();
        let mut prev = "0".repeat(64);
        for i in 0..10u64 {
            let b = mined_block(i, &prev);
            prev = b.hash.clone();
            blocks.push(b);
        }
        let results = validate_blocks_individual_parallel(&blocks, 1);
        assert_eq!(results.len(), 10);
        assert!(results.iter().all(|r| r.is_valid));
    }

    #[test]
    fn test_empty_blocks_slice() {
        let report = validate_blocks_parallel(&[], 1);
        assert!(report.all_valid);
        assert_eq!(report.results.len(), 0);
        assert_eq!(report.invalid_count, 0);
    }
}
