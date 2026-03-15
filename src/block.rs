//! Block — v1.1 thêm witness_root
//!
//! Block header commit đến cả txid_root lẫn witness_root
//! → không thể thay đổi witness data mà không làm vô hiệu block hash

use sha2::{Sha256, Digest};
use chrono::Utc;
use serde::{Serialize, Deserialize};
use crate::transaction::Transaction;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub index:        u64,
    pub timestamp:    i64,
    pub transactions: Vec<Transaction>,
    pub prev_hash:    String,
    pub nonce:        u64,
    pub hash:         String,
    pub witness_root: String, // ← v1.1: Merkle root của wtxids
}

impl Block {
    pub fn new(index: u64, transactions: Vec<Transaction>, prev_hash: String) -> Self {
        let timestamp    = Utc::now().timestamp();
        let witness_root = Self::merkle_root_wtxid(&transactions);
        Block { index, timestamp, transactions, prev_hash, nonce: 0, hash: String::new(), witness_root }
    }

    /// Merkle root của txids (dùng trong block hash)
    pub fn merkle_root_txid(txs: &[Transaction]) -> String {
        if txs.is_empty() { return "0".repeat(64); }
        let leaves: Vec<String> = txs.iter().map(|t| t.tx_id.clone()).collect();
        Self::merkle(leaves)
    }

    /// Merkle root của wtxids ← v1.1
    pub fn merkle_root_wtxid(txs: &[Transaction]) -> String {
        if txs.is_empty() { return "0".repeat(64); }
        let leaves: Vec<String> = txs.iter().map(|t| t.wtx_id.clone()).collect();
        Self::merkle(leaves)
    }

    /// Binary Merkle tree: cặp hash → hash cha
    fn merkle(mut hashes: Vec<String>) -> String {
        while hashes.len() > 1 {
            if hashes.len() % 2 == 1 {
                let last = hashes.last().unwrap().clone();
                hashes.push(last); // duplicate last nếu lẻ
            }
            hashes = hashes.chunks(2).map(|pair| {
                let combined = format!("{}{}", pair[0], pair[1]);
                hex::encode(Sha256::digest(Sha256::digest(combined.as_bytes())))
            }).collect();
        }
        hashes.into_iter().next().unwrap_or_else(|| "0".repeat(64))
    }

    pub fn calculate_hash(
        index: u64, timestamp: i64,
        transactions: &[Transaction],
        prev_hash: &str, nonce: u64,
    ) -> String {
        let txid_root    = Self::merkle_root_txid(transactions);
        let witness_root = Self::merkle_root_wtxid(transactions);
        let input = format!("{}|{}|{}|{}|{}|{}", index, timestamp, txid_root, witness_root, prev_hash, nonce);
        hex::encode(Sha256::digest(Sha256::digest(input.as_bytes())))
    }

    pub fn mine(&mut self, difficulty: usize) {
        let target = "0".repeat(difficulty);
        loop {
            let hash = Self::calculate_hash(
                self.index, self.timestamp, &self.transactions, &self.prev_hash, self.nonce,
            );
            if hash.starts_with(&target) { self.hash = hash; return; }
            self.nonce += 1;
        }
    }

    pub fn is_valid(&self, difficulty: usize) -> bool {
        let expected = Self::calculate_hash(
            self.index, self.timestamp, &self.transactions, &self.prev_hash, self.nonce,
        );
        if self.hash != expected { return false; }
        if self.index > 0 && !self.hash.starts_with(&"0".repeat(difficulty)) { return false; }
        // Kiểm tra witness_root đúng
        if self.witness_root != Self::merkle_root_wtxid(&self.transactions) { return false; }
        self.transactions.iter().all(|tx| tx.is_valid())
    }

    #[allow(dead_code)]
    pub fn has_coinbase(&self) -> bool {
        self.transactions.first().map_or(false, |tx| tx.is_coinbase)
    }

    #[allow(dead_code)]
    pub fn segwit_tx_count(&self) -> usize {
        self.transactions.iter().filter(|tx| tx.is_segwit()).count()
    }

    #[allow(dead_code)]
    pub fn vsize(&self) -> usize {
        self.transactions.iter().map(|tx| tx.vsize()).sum()
    }
}
