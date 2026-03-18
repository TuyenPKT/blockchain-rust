#![allow(dead_code)]
//! v8.2 — Address Index (tx history)
//!
//! Scans the chain to build a per-address transaction history.
//! Used by the PKTScan `/api/address/:addr` endpoint to return
//! received outputs (both spent and unspent) sorted newest-first.
//!
//! API:
//!   history_for_addr(addr, chain, utxo_set) → Vec<TxRecord>
//!   AddressIndex::build(chain, utxo_set)    → index all addresses at once
//!   AddressIndex::history_of(addr)          → &[TxRecord]

use std::collections::HashMap;

use crate::block::Block;
use crate::utxo::UtxoSet;

// ─── TxRecord ─────────────────────────────────────────────────────────────────

/// A single received output record for an address.
#[derive(Debug, Clone)]
pub struct TxRecord {
    pub tx_id:           String,
    pub block_height:    u64,
    pub block_timestamp: i64,
    pub output_index:    usize,
    pub amount:          u64,
    /// `false` → still in UTXO set (unspent); `true` → already spent.
    pub spent:           bool,
}

// ─── Standalone helper ────────────────────────────────────────────────────────

/// Scan `chain` and return every output sent to `addr`, annotated with
/// spent/unspent status via `utxo_set`.  Results are newest-block-first.
pub fn history_for_addr(addr: &str, chain: &[Block], utxo_set: &UtxoSet) -> Vec<TxRecord> {
    let mut records: Vec<TxRecord> = Vec::new();

    for block in chain.iter().rev() {
        for tx in &block.transactions {
            for (idx, output) in tx.outputs.iter().enumerate() {
                if UtxoSet::output_owner_hex(output).as_deref() == Some(addr) {
                    let spent = !utxo_set.is_unspent(&tx.tx_id, idx);
                    records.push(TxRecord {
                        tx_id:           tx.tx_id.clone(),
                        block_height:    block.index,
                        block_timestamp: block.timestamp,
                        output_index:    idx,
                        amount:          output.amount,
                        spent,
                    });
                }
            }
        }
    }

    records
}

// ─── AddressIndex ─────────────────────────────────────────────────────────────

/// Pre-built index for all addresses found in `chain`.
/// Useful when multiple address lookups are needed in a single request.
pub struct AddressIndex {
    data: HashMap<String, Vec<TxRecord>>,
}

impl AddressIndex {
    /// Build the full index by scanning the entire chain once.
    pub fn build(chain: &[Block], utxo_set: &UtxoSet) -> Self {
        let mut data: HashMap<String, Vec<TxRecord>> = HashMap::new();

        for block in chain.iter() {
            for tx in &block.transactions {
                for (idx, output) in tx.outputs.iter().enumerate() {
                    if let Some(addr) = UtxoSet::output_owner_hex(output) {
                        let spent = !utxo_set.is_unspent(&tx.tx_id, idx);
                        data.entry(addr).or_default().push(TxRecord {
                            tx_id:           tx.tx_id.clone(),
                            block_height:    block.index,
                            block_timestamp: block.timestamp,
                            output_index:    idx,
                            amount:          output.amount,
                            spent,
                        });
                    }
                }
            }
        }

        // Sort each address's history newest-first
        for records in data.values_mut() {
            records.sort_by(|a, b| b.block_height.cmp(&a.block_height));
        }

        AddressIndex { data }
    }

    /// Return tx history for `addr`, newest-first.  Empty slice if unknown.
    pub fn history_of(&self, addr: &str) -> &[TxRecord] {
        self.data.get(addr).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Number of distinct addresses indexed.
    pub fn address_count(&self) -> usize {
        self.data.len()
    }

    /// Total confirmed output records across all addresses.
    pub fn total_records(&self) -> usize {
        self.data.values().map(Vec::len).sum()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Block;
    use crate::chain::Blockchain;
    use crate::transaction::Transaction;

    /// Build a tiny chain: genesis + 2 blocks, each with a coinbase to ADDR.
    // 40 hex chars = 20 bytes — valid P2PKH pubkey hash
    const ADDR: &str = "aabbccdd00112233445566778899aabbccddeeff";

    fn make_chain() -> Blockchain {
        let mut bc = Blockchain::new();
        for i in 1..=2u64 {
            let cb = Transaction::coinbase_at(ADDR, 1000, i);
            let mut blk = Block::new(i, vec![cb], bc.chain.last().unwrap().hash.clone());
            blk.mine(2);
            bc.chain.push(blk.clone());
            bc.utxo_set.apply_block(&blk.transactions);
        }
        bc
    }

    // ── history_for_addr ──────────────────────────────────────────────────

    #[test]
    fn test_history_for_addr_finds_outputs() {
        let bc = make_chain();
        let hist = history_for_addr(ADDR, &bc.chain, &bc.utxo_set);
        // 2 coinbase outputs to ADDR (genesis has no coinbase to ADDR)
        assert_eq!(hist.len(), 2);
    }

    #[test]
    fn test_history_for_addr_newest_first() {
        let bc = make_chain();
        let hist = history_for_addr(ADDR, &bc.chain, &bc.utxo_set);
        assert!(hist[0].block_height >= hist[1].block_height);
    }

    #[test]
    fn test_history_for_addr_unspent_flag() {
        let bc = make_chain();
        let hist = history_for_addr(ADDR, &bc.chain, &bc.utxo_set);
        // coinbase outputs not yet spent
        for r in &hist {
            assert!(!r.spent, "coinbase outputs should be unspent");
        }
    }

    #[test]
    fn test_history_for_addr_unknown_addr() {
        let bc = make_chain();
        let hist = history_for_addr("0000000000000000000000000000000000000000", &bc.chain, &bc.utxo_set);
        assert!(hist.is_empty());
    }

    #[test]
    fn test_history_for_addr_amounts() {
        let bc = make_chain();
        let hist = history_for_addr(ADDR, &bc.chain, &bc.utxo_set);
        for r in &hist {
            assert!(r.amount > 0);
        }
    }

    #[test]
    fn test_history_for_addr_tx_id_non_empty() {
        let bc = make_chain();
        let hist = history_for_addr(ADDR, &bc.chain, &bc.utxo_set);
        for r in &hist {
            assert!(!r.tx_id.is_empty());
        }
    }

    // ── AddressIndex ──────────────────────────────────────────────────────

    #[test]
    fn test_address_index_builds() {
        let bc = make_chain();
        let idx = AddressIndex::build(&bc.chain, &bc.utxo_set);
        assert!(idx.address_count() > 0);
    }

    #[test]
    fn test_address_index_history_of() {
        let bc = make_chain();
        let idx = AddressIndex::build(&bc.chain, &bc.utxo_set);
        let hist = idx.history_of(ADDR);
        assert_eq!(hist.len(), 2);
    }

    #[test]
    fn test_address_index_history_sorted() {
        let bc = make_chain();
        let idx = AddressIndex::build(&bc.chain, &bc.utxo_set);
        let hist = idx.history_of(ADDR);
        if hist.len() >= 2 {
            assert!(hist[0].block_height >= hist[1].block_height);
        }
    }

    #[test]
    fn test_address_index_unknown_addr_empty() {
        let bc = make_chain();
        let idx = AddressIndex::build(&bc.chain, &bc.utxo_set);
        assert!(idx.history_of("deadbeef").is_empty());
    }

    #[test]
    fn test_address_index_total_records() {
        let bc = make_chain();
        let idx = AddressIndex::build(&bc.chain, &bc.utxo_set);
        assert!(idx.total_records() >= 2);
    }
}
