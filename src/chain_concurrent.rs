#![allow(dead_code)]
//! v6.2 — Thread-safe Chain
//!
//! `ConcurrentChain` wraps `Arc<RwLock<Blockchain>>`:
//!   - Multiple readers run simultaneously (RwLock read guards)
//!   - Single writer holds exclusive access (RwLock write guard)
//!   - `clone_handle()` cheaply shares the same underlying chain
//!
//! Read API:
//!   height()         → u64
//!   tip_hash()       → String
//!   difficulty()     → usize
//!   block_hash(h)    → Option<String>
//!   chain_len()      → usize
//!   is_valid()       → bool
//!   balance_of(addr) → u64
//!
//! Write API:
//!   add_block(txs, miner_addr) → u64   (returns new height)
//!   mine_and_add(miner_addr)   → u64
//!
//! Clone/share:
//!   clone_handle() → ConcurrentChain   (Arc clone, same chain)

use std::sync::{Arc, RwLock};

use crate::chain::Blockchain;
use crate::transaction::Transaction;

// ── ConcurrentChain ───────────────────────────────────────────────────────────

/// Thread-safe wrapper around `Blockchain`.
///
/// Internally holds `Arc<RwLock<Blockchain>>`.
/// Cheap to clone — each clone shares the same underlying data.
#[derive(Clone)]
pub struct ConcurrentChain {
    inner: Arc<RwLock<Blockchain>>,
}

impl ConcurrentChain {
    /// Create a new chain with a fresh genesis block.
    pub fn new() -> Self {
        ConcurrentChain {
            inner: Arc::new(RwLock::new(Blockchain::new())),
        }
    }

    /// Wrap an existing `Blockchain`.
    pub fn from_blockchain(bc: Blockchain) -> Self {
        ConcurrentChain {
            inner: Arc::new(RwLock::new(bc)),
        }
    }

    /// Clone the handle — both handles point to the same chain.
    /// This is identical to `.clone()` but more explicit at call sites.
    pub fn clone_handle(&self) -> Self {
        self.clone()
    }

    /// Number of active references to this chain.
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }

    // ── Read operations ───────────────────────────────────────────────────────

    /// Current block height (number of blocks − 1).
    pub fn height(&self) -> u64 {
        let bc = self.inner.read().unwrap();
        bc.chain.len() as u64 - 1
    }

    /// Total number of blocks (including genesis).
    pub fn chain_len(&self) -> usize {
        let bc = self.inner.read().unwrap();
        bc.chain.len()
    }

    /// Hash of the most recent block.
    pub fn tip_hash(&self) -> String {
        let bc = self.inner.read().unwrap();
        bc.last_block().hash.clone()
    }

    /// Current mining difficulty.
    pub fn difficulty(&self) -> usize {
        let bc = self.inner.read().unwrap();
        bc.difficulty
    }

    /// Hash of block at a given height, or `None` if out of range.
    pub fn block_hash(&self, height: usize) -> Option<String> {
        let bc = self.inner.read().unwrap();
        bc.chain.get(height).map(|b| b.hash.clone())
    }

    /// Validate the entire chain (hash links + difficulty + coinbase).
    pub fn is_valid(&self) -> bool {
        let bc = self.inner.read().unwrap();
        bc.is_valid()
    }

    /// UTXO balance for an address (pubkey_hash hex).
    pub fn balance_of(&self, addr: &str) -> u64 {
        let bc = self.inner.read().unwrap();
        bc.utxo_set.balance_of(addr)
    }

    /// Mempool size.
    pub fn mempool_size(&self) -> usize {
        let bc = self.inner.read().unwrap();
        bc.mempool.entries.len()
    }

    // ── Write operations ──────────────────────────────────────────────────────

    /// Mine and append a block with given transactions.
    /// Returns the new block height.
    pub fn add_block(&self, transactions: Vec<Transaction>, miner_addr: &str) -> u64 {
        let mut bc = self.inner.write().unwrap();
        bc.add_block(transactions, miner_addr);
        bc.chain.len() as u64 - 1
    }

    /// Mine a new block to `miner_addr` (uses difficulty from chain).
    /// Returns the new block height.
    pub fn mine_and_add(&self, miner_addr: &str) -> u64 {
        let mut bc = self.inner.write().unwrap();
        bc.mine_block_to_hash(miner_addr);
        bc.chain.len() as u64 - 1
    }

    /// Apply raw write access — for operations not covered by the API above.
    pub fn with_write<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Blockchain) -> R,
    {
        let mut bc = self.inner.write().unwrap();
        f(&mut bc)
    }

    /// Apply raw read access.
    pub fn with_read<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Blockchain) -> R,
    {
        let bc = self.inner.read().unwrap();
        f(&bc)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;

    #[test]
    fn test_new_chain_height_zero() {
        let cc = ConcurrentChain::new();
        assert_eq!(cc.height(), 0);
        assert_eq!(cc.chain_len(), 1); // genesis block
    }

    #[test]
    fn test_tip_hash_non_empty() {
        let cc = ConcurrentChain::new();
        assert_eq!(cc.tip_hash().len(), 64);
    }

    #[test]
    fn test_difficulty_default() {
        let cc = ConcurrentChain::new();
        assert!(cc.difficulty() >= 1);
    }

    #[test]
    fn test_add_block_increments_height() {
        let cc = ConcurrentChain::new();
        let h = cc.add_block(vec![], "aabbccddee112233445566778899aabbccddee11");
        assert_eq!(h, 1);
        assert_eq!(cc.height(), 1);
    }

    #[test]
    fn test_block_hash_genesis() {
        let cc = ConcurrentChain::new();
        let h0 = cc.block_hash(0);
        assert!(h0.is_some());
        assert_eq!(h0.unwrap().len(), 64);
    }

    #[test]
    fn test_block_hash_out_of_range() {
        let cc = ConcurrentChain::new();
        assert!(cc.block_hash(999).is_none());
    }

    #[test]
    fn test_clone_handle_shares_state() {
        let cc1 = ConcurrentChain::new();
        let cc2 = cc1.clone_handle();

        cc1.add_block(vec![], "aabbccddee112233445566778899aabbccddee11");
        // cc2 must see the change since it shares the same Arc
        assert_eq!(cc2.height(), 1);
        assert_eq!(cc1.tip_hash(), cc2.tip_hash());
    }

    #[test]
    fn test_ref_count_with_clone() {
        let cc1 = ConcurrentChain::new();
        assert_eq!(cc1.ref_count(), 1);
        let cc2 = cc1.clone_handle();
        assert_eq!(cc1.ref_count(), 2);
        drop(cc2);
        assert_eq!(cc1.ref_count(), 1);
    }

    #[test]
    fn test_concurrent_readers_do_not_block_each_other() {
        let cc  = ConcurrentChain::new();
        let sum = Arc::new(AtomicU64::new(0));

        // Spawn 8 reader threads — all should run without blocking each other
        let handles: Vec<_> = (0..8).map(|_| {
            let chain_ref = cc.clone_handle();
            let sum_ref   = Arc::clone(&sum);
            thread::spawn(move || {
                let h = chain_ref.height();
                sum_ref.fetch_add(h, Ordering::Relaxed);
            })
        }).collect();

        for h in handles { h.join().unwrap(); }
        // All 8 threads saw height=0 → sum = 0
        assert_eq!(sum.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_write_then_concurrent_read() {
        let cc   = ConcurrentChain::new();
        let addr = "aabbccddee112233445566778899aabbccddee11";
        cc.add_block(vec![], addr);

        let handles: Vec<_> = (0..4).map(|_| {
            let c = cc.clone_handle();
            thread::spawn(move || c.height())
        }).collect();

        for h in handles {
            assert_eq!(h.join().unwrap(), 1);
        }
    }

    #[test]
    fn test_is_valid_after_blocks() {
        let cc   = ConcurrentChain::new();
        let addr = "aabbccddee112233445566778899aabbccddee11";
        cc.add_block(vec![], addr);
        cc.add_block(vec![], addr);
        assert!(cc.is_valid());
    }

    #[test]
    fn test_with_read_closure() {
        let cc = ConcurrentChain::new();
        let len = cc.with_read(|bc| bc.chain.len());
        assert_eq!(len, 1);
    }
}
