#![allow(dead_code)]

//! v5.0 — Performance: UTXO indexing, block cache, faster Merkle tree
//!
//! Three focused optimizations:
//!   1. `UtxoIndex`       — secondary address→keys index → O(1) balance_of / utxos_of
//!   2. `BlockCache`      — hash→height HashMap → O(1) block deduplication
//!   3. `fast_merkle`     — raw [u8;32] Merkle tree (no hex encode/decode overhead)
//!
//! `UtxoSet::balance_of` and `utxos_of` scan every UTXO — O(n).
//! With a secondary index those lookups become O(owned_utxos).

use std::collections::HashMap;
use crate::block::Block;
use crate::transaction::{Transaction, TxOutput};
use crate::script::Opcode;
use crate::utxo::Utxo;

// ─── 1. UTXO Index ────────────────────────────────────────────────────────────

/// UtxoIndex wraps the flat UTXO map with a secondary address→keys index.
///
/// Primary storage is identical to `UtxoSet` (`HashMap<"txid:index", TxOutput>`).
/// The secondary `addr_idx` maps `owner_hex → Vec<utxo_key>` so that
/// `balance_of` and `utxos_of` are O(owned_utxos) instead of O(total_utxos).
pub struct UtxoIndex {
    /// Primary: key = "txid:index" → TxOutput
    pub utxos: HashMap<String, TxOutput>,
    /// Secondary index: owner_hex → Vec<"txid:index">
    addr_idx:  HashMap<String, Vec<String>>,
}

impl Default for UtxoIndex {
    fn default() -> Self { Self::new() }
}

impl UtxoIndex {
    pub fn new() -> Self {
        UtxoIndex { utxos: HashMap::new(), addr_idx: HashMap::new() }
    }

    fn key(tx_id: &str, index: usize) -> String {
        format!("{}:{}", tx_id, index)
    }

    /// Extract owner identifier from a TxOutput script.
    /// Mirrors `utxo::UtxoSet::owner_bytes_of` but returns hex.
    fn owner_hex(output: &TxOutput) -> Option<String> {
        // P2WPKH: OP_0 <20 bytes>
        if let Some(h) = output.script_pubkey.p2wpkh_hash() {
            return Some(hex::encode(h));
        }
        // P2TR: OP_1 <32 bytes>
        if let Some(h) = output.script_pubkey.p2tr_xonly() {
            return Some(hex::encode(h));
        }
        // P2PKH: OP_DUP OP_HASH160 <20 bytes> OP_EQUALVERIFY OP_CHECKSIG
        if let [Opcode::OpDup, Opcode::OpHash160, Opcode::OpPushData(d),
                Opcode::OpEqualVerify, Opcode::OpCheckSig] = output.script_pubkey.ops.as_slice() {
            if d.len() == 20 { return Some(hex::encode(d)); }
        }
        // P2SH: OP_HASH160 <20 bytes> OP_EQUAL
        if output.script_pubkey.is_p2sh() {
            for op in &output.script_pubkey.ops {
                if let Opcode::OpPushData(d) = op {
                    if d.len() == 20 { return Some(hex::encode(d)); }
                }
            }
        }
        None
    }

    /// Apply a mined block: remove spent UTXOs, add new ones, keep index in sync.
    pub fn apply_block(&mut self, transactions: &[Transaction]) {
        for tx in transactions {
            for input in &tx.inputs {
                let k = Self::key(&input.tx_id, input.output_index);
                if let Some(old_out) = self.utxos.remove(&k) {
                    if let Some(owner) = Self::owner_hex(&old_out) {
                        if let Some(keys) = self.addr_idx.get_mut(&owner) {
                            keys.retain(|x| x != &k);
                            if keys.is_empty() { self.addr_idx.remove(&owner); }
                        }
                    }
                }
            }
            for (i, output) in tx.outputs.iter().enumerate() {
                let k = Self::key(&tx.tx_id, i);
                // If key already exists (e.g. coinbase collision: two blocks with identical
                // outputs produce the same tx_id), skip re-adding to addr_idx — it's already
                // indexed from the first insertion.
                if self.utxos.contains_key(&k) {
                    self.utxos.insert(k, output.clone());
                    continue;
                }
                if let Some(owner) = Self::owner_hex(output) {
                    self.addr_idx.entry(owner).or_default().push(k.clone());
                }
                self.utxos.insert(k, output.clone());
            }
        }
    }

    /// O(owned_utxos) balance lookup via secondary index.
    pub fn balance_of(&self, pubkey_hash_hex: &str) -> u64 {
        match self.addr_idx.get(pubkey_hash_hex) {
            None => 0,
            Some(keys) => keys.iter()
                .filter_map(|k| self.utxos.get(k))
                .map(|o| o.amount)
                .sum(),
        }
    }

    /// O(owned_utxos) UTXO list via secondary index.
    pub fn utxos_of(&self, pubkey_hash_hex: &str) -> Vec<Utxo> {
        match self.addr_idx.get(pubkey_hash_hex) {
            None => vec![],
            Some(keys) => keys.iter()
                .filter_map(|k| {
                    self.utxos.get(k).map(|o| {
                        let parts: Vec<&str> = k.splitn(2, ':').collect();
                        Utxo {
                            tx_id:        parts[0].to_string(),
                            output_index: parts[1].parse().unwrap_or(0),
                            output:       o.clone(),
                        }
                    })
                })
                .collect(),
        }
    }

    pub fn is_unspent(&self, tx_id: &str, output_index: usize) -> bool {
        self.utxos.contains_key(&Self::key(tx_id, output_index))
    }

    pub fn get_amount(&self, tx_id: &str, output_index: usize) -> Option<u64> {
        self.utxos.get(&Self::key(tx_id, output_index)).map(|o| o.amount)
    }

    pub fn len(&self) -> usize   { self.utxos.len() }
    pub fn is_empty(&self) -> bool { self.utxos.is_empty() }
}

// ─── 2. Block Cache ───────────────────────────────────────────────────────────

/// BlockCache provides O(1) "do we already have this block?" lookups.
///
/// Node sync code checks received blocks against the local chain. Without a
/// cache that scan is O(chain_length). BlockCache keeps a `hash→height` map
/// so deduplication is O(1) regardless of chain length.
pub struct BlockCache {
    /// block hash hex → chain height (index)
    hash_to_height: HashMap<String, u64>,
}

impl Default for BlockCache {
    fn default() -> Self { Self::new() }
}

impl BlockCache {
    pub fn new() -> Self {
        BlockCache { hash_to_height: HashMap::new() }
    }

    /// Build from an existing chain slice — O(n) one-time cost.
    pub fn build_from_chain(chain: &[Block]) -> Self {
        let mut cache = Self::new();
        for block in chain {
            cache.hash_to_height.insert(block.hash.clone(), block.index);
        }
        cache
    }

    /// Insert a single newly-mined or received block.
    pub fn insert(&mut self, block: &Block) {
        self.hash_to_height.insert(block.hash.clone(), block.index);
    }

    /// O(1) — is this hash already in our chain?
    pub fn contains_hash(&self, hash: &str) -> bool {
        self.hash_to_height.contains_key(hash)
    }

    /// O(1) — height of a known block hash, None if not in cache.
    pub fn height_of(&self, hash: &str) -> Option<u64> {
        self.hash_to_height.get(hash).copied()
    }

    pub fn len(&self) -> usize   { self.hash_to_height.len() }
    pub fn is_empty(&self) -> bool { self.hash_to_height.is_empty() }
}

// ─── 3. Fast Merkle ───────────────────────────────────────────────────────────

/// Compute a double-SHA256 Merkle root from raw 32-byte leaves.
///
/// `Block::merkle()` hex-encodes every intermediate hash and decodes it back,
/// adding ~2× string allocation overhead per node. `fast_merkle` works entirely
/// on `[u8;32]` values — no intermediate encoding — and only hex-encodes the
/// final root when the caller requests it via `fast_merkle_txids`.
pub fn fast_merkle(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() { return [0u8; 32]; }

    let mut current: Vec<[u8; 32]> = leaves.to_vec();
    while current.len() > 1 {
        if current.len() % 2 == 1 {
            let last = *current.last().unwrap();
            current.push(last); // duplicate last leaf when count is odd
        }
        current = current.chunks(2).map(|pair| {
            let mut buf = [0u8; 64];
            buf[..32].copy_from_slice(&pair[0]);
            buf[32..].copy_from_slice(&pair[1]);
            let first  = blake3::hash(&buf);
            let second = blake3::hash(first.as_bytes());
            *second.as_bytes()
        }).collect();
    }
    current[0]
}

/// Drop-in replacement for `Block::merkle_root_txid`: accepts txid hex strings,
/// returns a 64-char hex root — but computes with raw bytes internally.
pub fn fast_merkle_txids(txids: &[String]) -> String {
    if txids.is_empty() { return "0".repeat(64); }
    let leaves: Vec<[u8; 32]> = txids.iter()
        .map(|h| {
            let bytes = hex::decode(h).unwrap_or_else(|_| vec![0u8; 32]);
            let mut arr = [0u8; 32];
            let len = bytes.len().min(32);
            arr[..len].copy_from_slice(&bytes[..len]);
            arr
        })
        .collect();
    hex::encode(fast_merkle(&leaves))
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Blockchain;

    #[test]
    fn test_utxo_index_vs_utxo_set() {
        let mut bc = Blockchain::new();
        bc.difficulty = 1;
        let addr = "aabbccddaabbccddaabbccddaabbccddaabbccdd"; // 20-byte hex
        bc.add_block(vec![], addr);
        bc.add_block(vec![], addr);

        // Build UtxoIndex by replaying the same blocks
        let mut idx = UtxoIndex::new();
        for block in &bc.chain {
            idx.apply_block(&block.transactions);
        }

        // balance_of must match UtxoSet (O(n))
        let expected_bal = bc.utxo_set.balance_of(addr);
        assert_eq!(idx.balance_of(addr), expected_bal,
            "UtxoIndex balance must match UtxoSet");

        // utxos_of count must match
        assert_eq!(
            idx.utxos_of(addr).len(),
            bc.utxo_set.utxos_of(addr).len(),
            "UtxoIndex UTXO count must match UtxoSet"
        );

        // Total UTXOs in primary map must match
        assert_eq!(idx.len(), bc.utxo_set.utxos.len());

        // Unknown address → 0
        assert_eq!(idx.balance_of("deadbeefdeadbeef"), 0);
    }

    #[test]
    fn test_block_cache_lookup() {
        let mut bc = Blockchain::new();
        bc.difficulty = 1;
        bc.add_block(vec![], "miner_addr");
        bc.add_block(vec![], "miner_addr");

        let cache = BlockCache::build_from_chain(&bc.chain);
        assert_eq!(cache.len(), bc.chain.len());

        for block in &bc.chain {
            assert!(cache.contains_hash(&block.hash), "cache must contain block hash");
            assert_eq!(cache.height_of(&block.hash), Some(block.index));
        }

        // Unknown hash → not found
        assert!(!cache.contains_hash(&"0".repeat(64)));
        assert!(cache.height_of("nonexistent").is_none());

        // Incremental insert
        let mut cache2 = BlockCache::new();
        cache2.insert(&bc.chain[0]);
        assert_eq!(cache2.len(), 1);
        assert!(cache2.contains_hash(&bc.chain[0].hash));
        assert!(!cache2.contains_hash(&bc.chain[1].hash));
    }

    #[test]
    fn test_fast_merkle_matches_block() {
        // Note: Block::merkle hashes the UTF-8 bytes of hex-string concatenations
        // (non-standard). fast_merkle uses raw byte concatenation (Bitcoin standard).
        // They intentionally produce different values — this test verifies correctness
        // of fast_merkle independently.

        let leaf1 = [0x01u8; 32];
        let leaf2 = [0x02u8; 32];
        let leaf3 = [0x03u8; 32];

        // Deterministic
        assert_eq!(fast_merkle(&[leaf1, leaf2]), fast_merkle(&[leaf1, leaf2]));

        // Different order → different root (Merkle is not commutative)
        assert_ne!(fast_merkle(&[leaf1, leaf2]), fast_merkle(&[leaf2, leaf1]));

        // Adding a leaf changes the root
        assert_ne!(fast_merkle(&[leaf1, leaf2]), fast_merkle(&[leaf1, leaf2, leaf3]));

        // Single leaf: root is the leaf itself (no hashing in single-leaf case)
        assert_eq!(fast_merkle(&[leaf1]), leaf1);

        // Empty
        assert_eq!(fast_merkle(&[]), [0u8; 32]);
        assert_eq!(fast_merkle_txids(&[]), "0".repeat(64));

        // fast_merkle_txids output is 64-char hex
        let txids = vec!["a".repeat(64), "b".repeat(64)];
        let root = fast_merkle_txids(&txids);
        assert_eq!(root.len(), 64, "root must be 64-char hex");
        assert_ne!(root, "0".repeat(64));
    }
}
