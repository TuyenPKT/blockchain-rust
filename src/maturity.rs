#![allow(dead_code)]

//! v5.3 — Coinbase maturity, replay protection, locktime/sequence validation
//!
//! Three independent components (chain.rs / transaction.rs unchanged):
//!   1. `CoinbaseGuard`      — enforces 100-block coinbase maturity rule
//!   2. `TxReplayGuard`      — bounded confirmed-txid set for replay detection
//!   3. `LockTimeValidator`  — BIP-style locktime and sequence validation
//!
//! All components are stateless validators or lightweight trackers that can
//! be plugged into `chain.rs` validation logic without rewriting it.

use std::collections::{HashMap, VecDeque, HashSet};
use crate::transaction::Transaction;

// ─── 1. Coinbase Maturity ─────────────────────────────────────────────────────

/// Enforces the coinbase maturity rule: coinbase outputs may not be spent
/// until `MATURITY_DEPTH` blocks have been mined on top of the block that
/// contains the coinbase transaction.
///
/// Bitcoin mainnet uses 100; we default to the same.
pub struct CoinbaseGuard {
    /// tx_id → block height at which the coinbase was mined
    coinbase_heights: HashMap<String, u64>,
}

impl CoinbaseGuard {
    /// Number of confirmation blocks required before a coinbase UTXO is spendable.
    pub const MATURITY_DEPTH: u64 = 100;

    pub fn new() -> Self {
        CoinbaseGuard { coinbase_heights: HashMap::new() }
    }

    /// Register a coinbase transaction mined at `height`.
    /// Call this whenever a block containing a coinbase is appended to the chain.
    pub fn register(&mut self, tx_id: &str, height: u64) {
        self.coinbase_heights.insert(tx_id.to_string(), height);
    }

    /// Register all coinbase transactions in a block.
    pub fn register_block(&mut self, transactions: &[Transaction], height: u64) {
        for tx in transactions {
            if tx.is_coinbase {
                self.register(&tx.tx_id, height);
            }
        }
    }

    /// Returns `true` if the coinbase output is mature (old enough to spend).
    ///
    /// - Unknown tx_id (not a coinbase): always mature (not restricted).
    /// - Known coinbase: mature iff `current_height >= mined_height + MATURITY_DEPTH`.
    pub fn is_mature(&self, tx_id: &str, current_height: u64) -> bool {
        match self.coinbase_heights.get(tx_id) {
            None         => true, // not a tracked coinbase → no restriction
            Some(&mined) => current_height >= mined + Self::MATURITY_DEPTH,
        }
    }

    /// Returns `true` if the tx_id is a tracked coinbase (regardless of maturity).
    pub fn is_coinbase(&self, tx_id: &str) -> bool {
        self.coinbase_heights.contains_key(tx_id)
    }

    /// Blocks remaining until the coinbase at `tx_id` becomes spendable.
    /// Returns 0 if already mature or unknown.
    pub fn blocks_until_mature(&self, tx_id: &str, current_height: u64) -> u64 {
        match self.coinbase_heights.get(tx_id) {
            None         => 0,
            Some(&mined) => {
                let ready_at = mined + Self::MATURITY_DEPTH;
                if current_height >= ready_at { 0 } else { ready_at - current_height }
            }
        }
    }
}

impl Default for CoinbaseGuard {
    fn default() -> Self { Self::new() }
}

// ─── 2. Replay Protection ─────────────────────────────────────────────────────

/// Bounded confirmed-transaction tracker for replay detection.
///
/// In a pure UTXO model, replaying a transaction is naturally impossible
/// because the inputs are already spent. However, edge cases exist:
///   - Chain reorgs that resurrect spent UTXOs
///   - Cross-fork replay when a hard fork shares history
///
/// `TxReplayGuard` maintains a bounded FIFO set of recently confirmed tx_ids.
/// Once a tx_id is confirmed, any attempt to confirm it again is rejected.
pub struct TxReplayGuard {
    confirmed:   HashSet<String>,
    order:       VecDeque<String>,
    max_size:    usize,
}

impl TxReplayGuard {
    /// Default window: last 10 000 confirmed transactions.
    pub const DEFAULT_WINDOW: usize = 10_000;

    pub fn new(max_size: usize) -> Self {
        assert!(max_size > 0);
        TxReplayGuard {
            confirmed: HashSet::with_capacity(max_size),
            order:     VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    /// Attempt to confirm a transaction.
    ///
    /// Returns `Ok(())` if the tx_id is new and has been recorded.
    /// Returns `Err(tx_id)` if the tx_id was already confirmed (replay detected).
    pub fn confirm(&mut self, tx_id: &str) -> Result<(), String> {
        if self.confirmed.contains(tx_id) {
            return Err(tx_id.to_string());
        }
        // Evict oldest if at capacity
        if self.confirmed.len() >= self.max_size {
            if let Some(oldest) = self.order.pop_front() {
                self.confirmed.remove(&oldest);
            }
        }
        self.confirmed.insert(tx_id.to_string());
        self.order.push_back(tx_id.to_string());
        Ok(())
    }

    /// Returns `true` if the tx_id is in the confirmed window (potential replay).
    pub fn is_replay(&self, tx_id: &str) -> bool {
        self.confirmed.contains(tx_id)
    }

    /// Confirm all transactions in a block (skip coinbase — no replay risk).
    pub fn confirm_block(&mut self, transactions: &[Transaction]) {
        for tx in transactions {
            if !tx.is_coinbase {
                self.confirm(&tx.tx_id).ok(); // ignore replays in batch replay
            }
        }
    }

    pub fn len(&self)     -> usize { self.confirmed.len() }
    pub fn is_empty(&self) -> bool { self.confirmed.is_empty() }
}

// ─── 3. Locktime / Sequence Validation ───────────────────────────────────────

/// BIP-style locktime and sequence number validation.
///
/// Bitcoin locktime rules (simplified):
///   - locktime == 0               : no locktime, always valid
///   - 0 < locktime < 500_000_000  : block-height locktime
///   - locktime >= 500_000_000     : unix-timestamp locktime (treated as valid here)
///
/// BIP68 sequence rules (simplified):
///   - sequence == 0xFFFF_FFFF     : SEQUENCE_FINAL — opt out of locktime
///   - lower values                : relative locktime encoded in bits 0-15
pub struct LockTimeValidator;

impl LockTimeValidator {
    /// Standard BIP68 "sequence final" value — disables relative locktime.
    pub const SEQUENCE_FINAL: u32 = 0xFFFF_FFFF;

    /// Values >= this threshold are treated as unix timestamps in locktime fields.
    pub const LOCKTIME_THRESHOLD: u64 = 500_000_000;

    /// Returns `true` if the transaction-level locktime is satisfied at `current_height`.
    ///
    /// A TX with a future block-height locktime must not be included in a block.
    pub fn check_locktime(locktime: u64, current_height: u64) -> bool {
        if locktime == 0 {
            return true; // no locktime
        }
        if locktime < Self::LOCKTIME_THRESHOLD {
            // Block-height locktime: must be <= current height
            locktime <= current_height
        } else {
            // Timestamp locktime: not enforced here (no live clock in core logic)
            true
        }
    }

    /// Returns `true` if all inputs have SEQUENCE_FINAL (standard non-time-locked TX).
    pub fn all_inputs_final(tx: &Transaction) -> bool {
        tx.inputs.iter().all(|i| i.sequence == Self::SEQUENCE_FINAL)
    }

    /// Returns `true` if a transaction is ready to be included in a block.
    ///
    /// Conditions:
    ///   1. `locktime == 0`  OR  locktime is satisfied at `current_height`
    ///   2. If locktime > 0, at least one input must have a non-FINAL sequence
    ///      to signal that locktime should be enforced (BIP65/BIP68 opt-in).
    pub fn is_final(tx: &Transaction, current_height: u64) -> bool {
        // Coinbase sequences are not constrained
        if tx.is_coinbase { return true; }

        // Shortcut: no inputs → always final
        if tx.inputs.is_empty() { return true; }

        // If all inputs are final, locktime is ignored (opt-out)
        if Self::all_inputs_final(tx) { return true; }

        // Otherwise check locktime
        // We embed locktime as u64 in the first input's sequence field (simplified)
        // Real Bitcoin stores it at the TX level; here we check max sequence as locktime proxy
        let effective_locktime = tx.inputs.iter()
            .filter(|i| i.sequence != Self::SEQUENCE_FINAL)
            .map(|i| i.sequence as u64)
            .max()
            .unwrap_or(0);

        Self::check_locktime(effective_locktime, current_height)
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::Transaction;

    #[test]
    fn test_coinbase_maturity_basic() {
        let mut guard = CoinbaseGuard::new();

        let tx_id = "coinbase_tx_aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabb";
        guard.register(tx_id, 10); // mined at height 10

        // Not mature before height 110
        assert!(!guard.is_mature(tx_id, 50));
        assert!(!guard.is_mature(tx_id, 109));

        // Mature at height 110 (= 10 + 100)
        assert!(guard.is_mature(tx_id, 110));
        assert!(guard.is_mature(tx_id, 200));
    }

    #[test]
    fn test_coinbase_maturity_unknown_tx() {
        let guard = CoinbaseGuard::new();
        // Unknown tx_id (regular TX) is always mature
        assert!(guard.is_mature("some_regular_tx_id", 0));
        assert!(guard.is_mature("some_regular_tx_id", 50));
    }

    #[test]
    fn test_coinbase_blocks_until_mature() {
        let mut guard = CoinbaseGuard::new();
        guard.register("cb1", 0); // mined at genesis

        assert_eq!(guard.blocks_until_mature("cb1", 0),  100);
        assert_eq!(guard.blocks_until_mature("cb1", 50),  50);
        assert_eq!(guard.blocks_until_mature("cb1", 99),   1);
        assert_eq!(guard.blocks_until_mature("cb1", 100),  0); // mature
        assert_eq!(guard.blocks_until_mature("cb1", 200),  0);
    }

    #[test]
    fn test_coinbase_register_block() {
        use crate::chain::Blockchain;

        let mut bc = Blockchain::new();
        bc.difficulty = 1;
        let addr = "aabbccddaabbccddaabbccddaabbccddaabbccdd";
        bc.add_block(vec![], addr);

        let mut guard = CoinbaseGuard::new();
        let block = bc.chain.last().unwrap();
        guard.register_block(&block.transactions, block.index);

        let coinbase_id = &block.transactions[0].tx_id;
        assert!(guard.is_coinbase(coinbase_id));
        // Not yet mature (needs 100 more blocks)
        assert!(!guard.is_mature(coinbase_id, block.index));
        assert_eq!(guard.blocks_until_mature(coinbase_id, block.index), 100);
    }

    #[test]
    fn test_replay_guard_basic() {
        let mut guard = TxReplayGuard::new(100);

        assert!(guard.confirm("tx1").is_ok());
        assert!(guard.confirm("tx2").is_ok());
        assert!(guard.confirm("tx1").is_err(), "tx1 is a replay");
        assert!(guard.is_replay("tx1"));
        assert!(!guard.is_replay("tx3"));
    }

    #[test]
    fn test_replay_guard_eviction() {
        let mut guard = TxReplayGuard::new(2);

        guard.confirm("a").unwrap();
        guard.confirm("b").unwrap();
        // "c" evicts "a"
        guard.confirm("c").unwrap();

        assert!(!guard.is_replay("a"), "evicted entry should not block re-confirm");
        assert!(guard.is_replay("b"));
        assert!(guard.is_replay("c"));
        assert_eq!(guard.len(), 2);
    }

    #[test]
    fn test_locktime_check_locktime() {
        // locktime 0 → always valid
        assert!(LockTimeValidator::check_locktime(0, 0));
        assert!(LockTimeValidator::check_locktime(0, 1000));

        // Block-height locktime
        assert!(!LockTimeValidator::check_locktime(100, 50));  // future
        assert!(LockTimeValidator::check_locktime(100, 100));  // exactly met
        assert!(LockTimeValidator::check_locktime(100, 200));  // past

        // Timestamp locktime (>= 500_000_000) always valid in our simplified impl
        assert!(LockTimeValidator::check_locktime(500_000_001, 0));
    }

    #[test]
    fn test_locktime_all_inputs_final() {
        let mut tx = Transaction::coinbase("aabbccddaabbccddaabbccddaabbccddaabbccdd", 0);
        tx.is_coinbase = false;
        // Coinbase has no inputs, so all_inputs_final = true (vacuous)
        assert!(LockTimeValidator::all_inputs_final(&tx));

        // is_final: coinbase always final
        let cb = Transaction::coinbase("aabb", 0);
        assert!(LockTimeValidator::is_final(&cb, 0));
    }
}
