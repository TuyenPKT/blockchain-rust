#![allow(dead_code)]
//! v8.3 — PKTScan Search Engine
//!
//! Builds an in-memory index from the chain and answers prefix queries for
//! blocks, transactions, and addresses.
//!
//! Search rules (applied in order, results merged):
//!   1. Pure numeric query  → exact block height lookup
//!   2. Hex string ≥ 4 chars → prefix-match block hashes
//!   3. Hex string ≥ 4 chars → prefix-match tx IDs
//!   4. Hex string == 40 chars (20 B) or 64 chars (32 B) → address lookup
//!
//! API:
//!   SearchIndex::build(chain, utxo_set)        → index
//!   SearchIndex::search(query, utxo_set, limit) → Vec<SearchResult>
//!   search_one(query, chain, utxo_set)          → Vec<SearchResult>  (one-shot)

use std::collections::HashMap;

use crate::block::Block;
use crate::utxo::UtxoSet;

// ─── Result types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct BlockRef {
    pub height:    u64,
    pub hash:      String,
    pub tx_count:  usize,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TxRef {
    pub tx_id:        String,
    pub block_height: u64,
    pub is_coinbase:  bool,
    pub fee:          u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AddrRef {
    pub addr:       String,
    pub balance:    u64,
    pub utxo_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchResult {
    Block(BlockRef),
    Tx(TxRef),
    Address(AddrRef),
}

impl SearchResult {
    pub fn kind(&self) -> &'static str {
        match self {
            SearchResult::Block(_)   => "block",
            SearchResult::Tx(_)      => "tx",
            SearchResult::Address(_) => "address",
        }
    }
}

// ─── SearchIndex ──────────────────────────────────────────────────────────────

pub struct SearchIndex {
    /// full_hash → BlockRef
    blocks_by_hash:   HashMap<String, BlockRef>,
    /// height → BlockRef
    blocks_by_height: HashMap<u64, BlockRef>,
    /// full_txid → TxRef
    txs_by_id:        HashMap<String, TxRef>,
    /// known address hex strings (20-byte or 32-byte hex)
    known_addrs:      Vec<String>,
}

impl SearchIndex {
    /// Build the index by scanning `chain` once.
    pub fn build(chain: &[Block]) -> Self {
        let mut blocks_by_hash   = HashMap::new();
        let mut blocks_by_height = HashMap::new();
        let mut txs_by_id        = HashMap::new();
        let mut addr_set: std::collections::HashSet<String> = std::collections::HashSet::new();

        for block in chain {
            let bref = BlockRef {
                height:    block.index,
                hash:      block.hash.clone(),
                tx_count:  block.transactions.len(),
                timestamp: block.timestamp,
            };
            blocks_by_hash.insert(block.hash.clone(), bref.clone());
            blocks_by_height.insert(block.index, bref);

            for tx in &block.transactions {
                txs_by_id.insert(tx.tx_id.clone(), TxRef {
                    tx_id:        tx.tx_id.clone(),
                    block_height: block.index,
                    is_coinbase:  tx.is_coinbase,
                    fee:          tx.fee,
                });
                // Collect output addresses
                for output in &tx.outputs {
                    if let Some(addr) = UtxoSet::output_owner_hex(output) {
                        addr_set.insert(addr);
                    }
                }
            }
        }

        SearchIndex {
            blocks_by_hash,
            blocks_by_height,
            txs_by_id,
            known_addrs: addr_set.into_iter().collect(),
        }
    }

    /// Search for `query` and return up to `limit` results.
    /// `utxo_set` is used to resolve address balance/UTXO count.
    pub fn search(&self, query: &str, utxo_set: &UtxoSet, limit: usize) -> Vec<SearchResult> {
        let q = query.trim();
        if q.is_empty() { return vec![]; }

        let mut results: Vec<SearchResult> = Vec::new();

        // 1. Numeric → exact height lookup
        if let Ok(height) = q.parse::<u64>() {
            if let Some(bref) = self.blocks_by_height.get(&height) {
                results.push(SearchResult::Block(bref.clone()));
            }
        }

        // 2. Hex prefix → block hash prefix match (q must be ≥ 4 hex chars)
        let is_hex = q.chars().all(|c| c.is_ascii_hexdigit());
        if is_hex && q.len() >= 4 {
            let q_lower = q.to_lowercase();
            for (hash, bref) in &self.blocks_by_hash {
                if hash.starts_with(&q_lower) {
                    if !results.iter().any(|r| matches!(r, SearchResult::Block(b) if b.height == bref.height)) {
                        results.push(SearchResult::Block(bref.clone()));
                    }
                    if results.len() >= limit { break; }
                }
            }
        }

        // 3. Hex prefix → txid prefix match
        if is_hex && q.len() >= 4 {
            let q_lower = q.to_lowercase();
            for (txid, tref) in &self.txs_by_id {
                if txid.starts_with(&q_lower) {
                    results.push(SearchResult::Tx(tref.clone()));
                    if results.len() >= limit { break; }
                }
            }
        }

        // 4. Exact address match (20-byte = 40 hex, 32-byte = 64 hex)
        if is_hex && (q.len() == 40 || q.len() == 64) {
            let addr = q.to_lowercase();
            if self.known_addrs.iter().any(|a| a == &addr) {
                let balance    = utxo_set.balance_of(&addr);
                let utxo_count = utxo_set.utxos_of(&addr).len();
                results.push(SearchResult::Address(AddrRef {
                    addr,
                    balance,
                    utxo_count,
                }));
            }
        }

        results.truncate(limit);
        results
    }

    pub fn block_count(&self) -> usize { self.blocks_by_height.len() }
    pub fn tx_count(&self)    -> usize { self.txs_by_id.len() }
    pub fn addr_count(&self)  -> usize { self.known_addrs.len() }
}

// ─── One-shot helper ──────────────────────────────────────────────────────────

/// Build a temporary index and search.  Use `SearchIndex::build` + `search`
/// when handling multiple queries on the same chain snapshot.
pub fn search_one(query: &str, chain: &[Block], utxo_set: &UtxoSet, limit: usize) -> Vec<SearchResult> {
    SearchIndex::build(chain).search(query, utxo_set, limit)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Block;
    use crate::chain::Blockchain;
    use crate::transaction::Transaction;

    // 40 hex chars = 20 bytes — valid P2PKH pubkey hash
    const ADDR: &str = "aabbccdd00112233445566778899aabbccddeeff";

    fn make_chain() -> Blockchain {
        let mut bc = Blockchain::new();
        for i in 1..=3u64 {
            let cb = Transaction::coinbase_at(ADDR, 0, i);
            let mut blk = Block::new(i, vec![cb], bc.chain.last().unwrap().hash.clone());
            blk.mine(2);
            bc.chain.push(blk.clone());
            bc.utxo_set.apply_block(&blk.transactions);
        }
        bc
    }

    // ── build ─────────────────────────────────────────────────────────────

    #[test]
    fn test_build_counts() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        // genesis + 3 blocks
        assert_eq!(idx.block_count(), 4);
        // 3 coinbase txs + genesis coinbase
        assert!(idx.tx_count() >= 3);
        assert!(idx.addr_count() >= 1);
    }

    // ── height search ─────────────────────────────────────────────────────

    #[test]
    fn test_search_by_height() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let res = idx.search("1", &bc.utxo_set, 10);
        assert!(res.iter().any(|r| matches!(r, SearchResult::Block(b) if b.height == 1)));
    }

    #[test]
    fn test_search_by_height_genesis() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let res = idx.search("0", &bc.utxo_set, 10);
        assert!(res.iter().any(|r| matches!(r, SearchResult::Block(b) if b.height == 0)));
    }

    #[test]
    fn test_search_height_not_found() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let res = idx.search("9999", &bc.utxo_set, 10);
        // 9999 is also a valid hex prefix but no block at that height
        // result may contain tx prefix matches but not height block
        let has_height_9999 = res.iter().any(|r| matches!(r, SearchResult::Block(b) if b.height == 9999));
        assert!(!has_height_9999);
    }

    // ── hash prefix search ────────────────────────────────────────────────

    #[test]
    fn test_search_by_hash_prefix() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let hash = &bc.chain[1].hash;
        let prefix = &hash[..8];
        let res = idx.search(prefix, &bc.utxo_set, 10);
        assert!(res.iter().any(|r| matches!(r, SearchResult::Block(b) if b.hash == *hash)));
    }

    #[test]
    fn test_search_hash_full() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let hash = bc.chain.last().unwrap().hash.clone();
        let res = idx.search(&hash, &bc.utxo_set, 10);
        assert!(res.iter().any(|r| matches!(r, SearchResult::Block(b) if b.hash == hash)));
    }

    // ── txid prefix search ────────────────────────────────────────────────

    #[test]
    fn test_search_by_txid_prefix() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let txid = &bc.chain[1].transactions[0].tx_id;
        let prefix = &txid[..8];
        let res = idx.search(prefix, &bc.utxo_set, 10);
        assert!(res.iter().any(|r| matches!(r, SearchResult::Tx(t) if t.tx_id == *txid)));
    }

    #[test]
    fn test_search_txid_full() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let txid = bc.chain[1].transactions[0].tx_id.clone();
        let res = idx.search(&txid, &bc.utxo_set, 10);
        assert!(res.iter().any(|r| matches!(r, SearchResult::Tx(t) if t.tx_id == txid)));
    }

    // ── address search ────────────────────────────────────────────────────

    #[test]
    fn test_search_by_address() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let res = idx.search(ADDR, &bc.utxo_set, 10);
        assert!(res.iter().any(|r| matches!(r, SearchResult::Address(a) if a.addr == ADDR)));
    }

    #[test]
    fn test_search_address_has_balance() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let res = idx.search(ADDR, &bc.utxo_set, 10);
        if let Some(SearchResult::Address(a)) = res.iter().find(|r| matches!(r, SearchResult::Address(_))) {
            assert!(a.balance > 0);
            assert!(a.utxo_count > 0);
        } else {
            panic!("address result not found");
        }
    }

    #[test]
    fn test_search_unknown_address_empty() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let res = idx.search("0000000000000000000000000000000000000000", &bc.utxo_set, 10);
        assert!(!res.iter().any(|r| matches!(r, SearchResult::Address(_))));
    }

    // ── edge cases ────────────────────────────────────────────────────────

    #[test]
    fn test_search_empty_query() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let res = idx.search("", &bc.utxo_set, 10);
        assert!(res.is_empty());
    }

    #[test]
    fn test_search_limit_respected() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        // "0" matches height 0 and is a prefix of many hashes/txids
        let res = idx.search("0", &bc.utxo_set, 2);
        assert!(res.len() <= 2);
    }

    #[test]
    fn test_search_result_kind() {
        let bc = make_chain();
        let idx = SearchIndex::build(&bc.chain);
        let res = idx.search("1", &bc.utxo_set, 5);
        for r in &res {
            assert!(["block", "tx", "address"].contains(&r.kind()));
        }
    }

    // ── search_one ────────────────────────────────────────────────────────

    #[test]
    fn test_search_one_helper() {
        let bc = make_chain();
        let res = search_one("1", &bc.chain, &bc.utxo_set, 10);
        assert!(res.iter().any(|r| matches!(r, SearchResult::Block(b) if b.height == 1)));
    }
}
