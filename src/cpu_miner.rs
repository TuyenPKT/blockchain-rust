#![allow(dead_code)]
//! v6.1 — CPU Multi-thread Miner
//!
//! rayon work-stealing, nonce space split across N threads.
//! AtomicBool stop flag — winning thread sets it, others exit immediately.
//! Default threads = max(1, logical_cores / 3)
//!
//! API:
//!   default_threads() -> usize
//!   mine_parallel(block, threads, difficulty) -> MineResult
//!   CpuMinerConfig::new(addr).with_threads(n).with_difficulty(d)
//!   CpuMiner::new(config) → mine_block(&block) -> MineResult
//!
//! CLI: cargo run -- cpumine [addr] [diff] [blocks]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use rayon::prelude::*;

use crate::blake3_hash::{Blake3Block, HASH_VERSION_BLAKE3, pow_hash, to_hex};

// ── Thread count ──────────────────────────────────────────────────────────────

/// Recommended thread count = max(1, logical_cores / 3).
/// Leaves ~2/3 of CPU free for node + OS.
pub fn default_threads() -> usize {
    (num_cpus::get() / 3).max(1)
}

// ── Config ────────────────────────────────────────────────────────────────────

pub struct CpuMinerConfig {
    pub threads:    usize,
    pub difficulty: usize,
    pub max_blocks: Option<u32>,
    pub address:    String,
}

impl CpuMinerConfig {
    pub fn new(address: &str) -> Self {
        CpuMinerConfig {
            threads:    default_threads(),
            difficulty: 3,
            max_blocks: None,
            address:    address.to_string(),
        }
    }

    pub fn with_threads(mut self, n: usize) -> Self {
        self.threads = n.max(1);
        self
    }

    pub fn with_difficulty(mut self, d: usize) -> Self {
        self.difficulty = d;
        self
    }

    pub fn with_max_blocks(mut self, n: u32) -> Self {
        self.max_blocks = Some(n);
        self
    }
}

// ── Mine result ───────────────────────────────────────────────────────────────

pub struct MineResult {
    pub nonce:        u64,
    pub hash:         String,
    pub hashes_tried: u64,
    pub elapsed_ms:   u64,
    pub thread_id:    usize,
}

// ── Core: parallel mining ─────────────────────────────────────────────────────

/// Mine `block` using N threads.
///
/// Nonce space [0, u64::MAX) is divided into N equal chunks.
/// Uses rayon `find_map_any` — returns as soon as any thread finds a valid hash.
/// `AtomicBool` stop flag lets losing threads exit their inner loops immediately.
pub fn mine_parallel(
    block:      &Blake3Block,
    threads:    usize,
    difficulty: usize,
) -> MineResult {
    let t0           = Instant::now();
    let stop         = Arc::new(AtomicBool::new(false));
    let total_hashes = Arc::new(AtomicU64::new(0));
    let target       = "0".repeat(difficulty);

    let n       = threads.max(1);
    let chunk   = u64::MAX / n as u64;
    let hv      = block.hash_version;

    // Precompute the constant prefix — only nonce changes per iteration.
    // Must match Blake3Block::header_bytes() format:
    //   "{index}|{timestamp}|{txid_root}|{witness_root}|{prev_hash}|{nonce}|{hash_version}"
    let prefix = format!(
        "{}|{}|{}|{}|{}|",
        block.index, block.timestamp,
        block.txid_root, block.witness_root,
        block.prev_hash
    );

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .build()
        .expect("cpu_miner: build thread pool");

    let found = pool.install(|| {
        (0..n).into_par_iter().find_map_any(|tid| {
            let start_nonce = (tid as u64).saturating_mul(chunk);
            let end_nonce   = if tid == n - 1 {
                u64::MAX
            } else {
                start_nonce.saturating_add(chunk)
            };

            let mut local = 0u64;

            for nonce in start_nonce..end_nonce {
                if stop.load(Ordering::Relaxed) {
                    total_hashes.fetch_add(local, Ordering::Relaxed);
                    return None;
                }

                let header = format!("{}{}|{}", prefix, nonce, hv);
                let raw    = pow_hash(header.as_bytes(), hv);
                local     += 1;

                if to_hex(&raw).starts_with(&target) {
                    stop.store(true, Ordering::Relaxed);
                    total_hashes.fetch_add(local, Ordering::Relaxed);
                    return Some((tid, nonce, to_hex(&raw)));
                }
            }

            total_hashes.fetch_add(local, Ordering::Relaxed);
            None
        })
    });

    let elapsed_ms   = t0.elapsed().as_millis() as u64;
    let hashes_tried = total_hashes.load(Ordering::Relaxed);

    match found {
        Some((tid, nonce, hash)) => {
            MineResult { nonce, hash, hashes_tried, elapsed_ms, thread_id: tid }
        }
        None => {
            MineResult { nonce: u64::MAX, hash: String::new(), hashes_tried, elapsed_ms, thread_id: 0 }
        }
    }
}

// ── Stats ─────────────────────────────────────────────────────────────────────

pub struct CpuMinerStats {
    pub blocks_mined: u32,
    pub total_hashes: u64,
    start:            Instant,
}

impl CpuMinerStats {
    pub fn new() -> Self {
        CpuMinerStats { blocks_mined: 0, total_hashes: 0, start: Instant::now() }
    }

    pub fn record(&mut self, r: &MineResult) {
        self.blocks_mined += 1;
        self.total_hashes += r.hashes_tried;
    }

    pub fn avg_hashrate(&self) -> f64 {
        let t = self.start.elapsed().as_secs_f64();
        if t < 0.001 { 0.0 } else { self.total_hashes as f64 / t }
    }
}

// ── CpuMiner ─────────────────────────────────────────────────────────────────

pub struct CpuMiner {
    pub config: CpuMinerConfig,
    pub stats:  CpuMinerStats,
}

impl CpuMiner {
    pub fn new(config: CpuMinerConfig) -> Self {
        CpuMiner { config, stats: CpuMinerStats::new() }
    }

    /// Mine a single block template; returns nonce + hash once solved.
    pub fn mine_block(&mut self, block: &Blake3Block) -> MineResult {
        let r = mine_parallel(block, self.config.threads, self.config.difficulty);
        self.stats.record(&r);
        r
    }
}

// ── CLI ───────────────────────────────────────────────────────────────────────

pub fn cmd_cpu_mine(address: &str, difficulty: usize, max_blocks: u32) {
    let threads = default_threads();
    println!();
    println!("  CPU Multi-thread Miner  v6.1");
    println!("  ─────────────────────────────────────────────");
    println!("  Address    : {}", address);
    println!("  Threads    : {} (logical cores / 3)", threads);
    println!("  Difficulty : {}", difficulty);
    println!("  Blocks     : {}", max_blocks);
    println!();

    let config = CpuMinerConfig::new(address)
        .with_threads(threads)
        .with_difficulty(difficulty)
        .with_max_blocks(max_blocks);

    let mut miner = CpuMiner::new(config);

    for i in 0..max_blocks {
        let block = Blake3Block::new(
            i as u64 + 1,
            chrono::Utc::now().timestamp(),
            format!("txroot_{}", i),
            format!("witroot_{}", i),
            "0".repeat(64),
            HASH_VERSION_BLAKE3,
        );
        let r = miner.mine_block(&block);
        let khs = r.hashes_tried as f64 / (r.elapsed_ms.max(1) as f64 / 1000.0) / 1000.0;
        println!(
            "  block #{:<4}  nonce={:<12}  hash={}...  {:.1} KH/s  {}ms  thread={}",
            i + 1, r.nonce, &r.hash[..8], khs, r.elapsed_ms, r.thread_id
        );
    }

    println!();
    println!("  Total blocks : {}", miner.stats.blocks_mined);
    println!("  Total hashes : {}", miner.stats.total_hashes);
    println!("  Avg hashrate : {:.1} KH/s", miner.stats.avg_hashrate() / 1000.0);
    println!();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blake3_hash::meets_difficulty;

    fn test_block(index: u64) -> Blake3Block {
        Blake3Block::new(
            index, 1_700_000_000,
            format!("txroot_{}", index),
            format!("witroot_{}", index),
            "0".repeat(64),
            HASH_VERSION_BLAKE3,
        )
    }

    #[test]
    fn test_default_threads_at_least_one() {
        assert!(default_threads() >= 1);
    }

    #[test]
    fn test_mine_parallel_single_thread_diff1() {
        let block  = test_block(1);
        let result = mine_parallel(&block, 1, 1);
        assert!(result.hash.starts_with('0'), "hash must start with '0'");
        assert!(result.hashes_tried >= 1);
    }

    #[test]
    fn test_mine_parallel_two_threads_diff1() {
        let block  = test_block(2);
        let result = mine_parallel(&block, 2, 1);
        assert!(result.hash.starts_with('0'));
        assert!(result.thread_id < 2);
    }

    #[test]
    fn test_mine_parallel_hash_verifiable() {
        let block  = test_block(3);
        let result = mine_parallel(&block, 2, 1);
        // Recompute expected hash for the returned nonce
        let header = format!("{}|{}|{}|{}|{}|{}|{}",
            block.index, block.timestamp,
            block.txid_root, block.witness_root,
            block.prev_hash, result.nonce, block.hash_version);
        let raw = pow_hash(header.as_bytes(), block.hash_version);
        assert_eq!(to_hex(&raw), result.hash, "returned hash must match nonce");
    }

    #[test]
    fn test_mine_parallel_meets_difficulty() {
        let block  = test_block(4);
        let result = mine_parallel(&block, 2, 1);
        let raw: [u8; 32] = hex::decode(&result.hash).unwrap().try_into().unwrap();
        assert!(meets_difficulty(&raw, 1));
    }

    #[test]
    fn test_cpu_miner_config_builder() {
        let cfg = CpuMinerConfig::new("aabbccddee112233445566778899aabbccddee11")
            .with_threads(4)
            .with_difficulty(2)
            .with_max_blocks(5);
        assert_eq!(cfg.threads, 4);
        assert_eq!(cfg.difficulty, 2);
        assert_eq!(cfg.max_blocks, Some(5));
    }

    #[test]
    fn test_with_threads_min_one() {
        let cfg = CpuMinerConfig::new("addr").with_threads(0);
        assert_eq!(cfg.threads, 1, "threads must be >= 1 even if 0 passed");
    }

    #[test]
    fn test_cpu_miner_mine_block_valid() {
        let cfg   = CpuMinerConfig::new("aabb").with_threads(2).with_difficulty(1);
        let mut m = CpuMiner::new(cfg);
        let r     = m.mine_block(&test_block(10));
        assert!(r.hash.starts_with('0'));
        assert_eq!(m.stats.blocks_mined, 1);
        assert_eq!(m.stats.total_hashes, r.hashes_tried);
    }

    #[test]
    fn test_cpu_miner_stats_accumulate() {
        let cfg   = CpuMinerConfig::new("aabb").with_threads(1).with_difficulty(1);
        let mut m = CpuMiner::new(cfg);
        m.mine_block(&test_block(20));
        m.mine_block(&test_block(21));
        assert_eq!(m.stats.blocks_mined, 2);
        assert!(m.stats.total_hashes >= 2);
    }

    #[test]
    fn test_avg_hashrate_non_negative() {
        let cfg   = CpuMinerConfig::new("aabb").with_threads(1).with_difficulty(1);
        let mut m = CpuMiner::new(cfg);
        m.mine_block(&test_block(30));
        assert!(m.stats.avg_hashrate() >= 0.0);
    }
}
