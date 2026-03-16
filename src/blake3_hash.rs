//! v6.0 — BLAKE3 Hash Engine
//!
//! Thay SHA-256 bằng BLAKE3 cho PoW block hashing (3–4x nhanh hơn).
//! SHA-256 giữ nguyên cho ECDSA/address (backward-compatible).
//!
//! `hash_version: u8` trên Block:
//!   0 = SHA-256  (legacy, Era 1–11)
//!   1 = BLAKE3   (Era 12+)
//!
//! API:
//!   pow_hash(data: &[u8]) -> [u8; 32]   — chọn thuật toán theo version
//!   blake3_hash(data: &[u8]) -> [u8; 32]
//!   sha256d(data: &[u8]) -> [u8; 32]    — double-SHA256 (giữ cho compat)
//!   leading_zeros(hash: &[u8; 32]) -> u32
//!   meets_difficulty(hash: &[u8; 32], difficulty: usize) -> bool

#![allow(dead_code)]

use sha2::{Sha256, Digest};

// ── Hash version constants ────────────────────────────────────────────────────

pub const HASH_VERSION_SHA256: u8 = 0;
pub const HASH_VERSION_BLAKE3: u8 = 1;

// ── Core hash functions ───────────────────────────────────────────────────────

/// BLAKE3 hash — 32-byte output
pub fn blake3_hash(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

/// Double-SHA256 (Bitcoin-style) — giữ nguyên cho ECDSA / address
pub fn sha256d(data: &[u8]) -> [u8; 32] {
    let first  = Sha256::digest(data);
    let second = Sha256::digest(first);
    second.into()
}

/// Single SHA-256
pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// PoW hash theo `hash_version`
/// - version 0: double-SHA256 (legacy)
/// - version 1: BLAKE3
pub fn pow_hash(data: &[u8], hash_version: u8) -> [u8; 32] {
    match hash_version {
        HASH_VERSION_BLAKE3 => blake3_hash(data),
        _ => sha256d(data),
    }
}

/// Hex string từ 32-byte hash
pub fn to_hex(hash: &[u8; 32]) -> String {
    hex::encode(hash)
}

// ── Difficulty helpers ────────────────────────────────────────────────────────

/// Đếm số leading zero bits trong hash
pub fn leading_zeros(hash: &[u8; 32]) -> u32 {
    let mut count = 0u32;
    for &byte in hash.iter() {
        if byte == 0 {
            count += 8;
        } else {
            count += byte.leading_zeros();
            break;
        }
    }
    count
}

/// Kiểm tra hash có đủ leading-zero nibbles (hex chars) cho `difficulty`
/// Giữ nguyên semantics với SHA-256 PoW cũ: difficulty = số hex '0' đầu
pub fn meets_difficulty(hash: &[u8; 32], difficulty: usize) -> bool {
    if difficulty == 0 { return true; }
    let hex = to_hex(hash);
    hex.starts_with(&"0".repeat(difficulty))
}

// ── Blake3Block ───────────────────────────────────────────────────────────────

/// Block header dùng BLAKE3 PoW
/// Compatible với Block struct cũ nhưng thêm hash_version field.
#[derive(Debug, Clone)]
pub struct Blake3Block {
    pub index:        u64,
    pub timestamp:    i64,
    pub txid_root:    String,
    pub witness_root: String,
    pub prev_hash:    String,
    pub nonce:        u64,
    pub hash_version: u8,
    pub hash:         String,
}

impl Blake3Block {
    pub fn new(
        index: u64,
        timestamp: i64,
        txid_root: String,
        witness_root: String,
        prev_hash: String,
        hash_version: u8,
    ) -> Self {
        Blake3Block {
            index, timestamp, txid_root, witness_root,
            prev_hash, nonce: 0, hash_version, hash: String::new(),
        }
    }

    pub fn header_bytes(&self) -> Vec<u8> {
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.index, self.timestamp,
            self.txid_root, self.witness_root,
            self.prev_hash, self.nonce, self.hash_version
        ).into_bytes()
    }

    pub fn calculate_hash(&self) -> String {
        let raw = pow_hash(&self.header_bytes(), self.hash_version);
        to_hex(&raw)
    }

    pub fn mine(&mut self, difficulty: usize) {
        let target = "0".repeat(difficulty);
        loop {
            let h = self.calculate_hash();
            if h.starts_with(&target) {
                self.hash = h;
                return;
            }
            self.nonce += 1;
        }
    }

    pub fn is_valid(&self, difficulty: usize) -> bool {
        let expected = self.calculate_hash();
        if self.hash != expected { return false; }
        if self.index > 0 && !self.hash.starts_with(&"0".repeat(difficulty)) { return false; }
        true
    }
}

// ── Benchmark comparison ──────────────────────────────────────────────────────

/// So sánh throughput BLAKE3 vs SHA-256 trên `iters` lần hash
pub struct HashComparison {
    pub blake3_ns_per_op: u64,
    pub sha256d_ns_per_op: u64,
    pub speedup_x: f64,
}

pub fn benchmark_compare(iters: u64) -> HashComparison {
    use std::time::Instant;

    let data = b"blockchain-rust benchmark data v6.0 BLAKE3 vs SHA-256";

    let t0 = Instant::now();
    for i in 0..iters {
        let mut d = data.to_vec();
        d.extend_from_slice(&i.to_le_bytes());
        let _ = blake3_hash(&d);
    }
    let blake3_ns = t0.elapsed().as_nanos() as u64;

    let t1 = Instant::now();
    for i in 0..iters {
        let mut d = data.to_vec();
        d.extend_from_slice(&i.to_le_bytes());
        let _ = sha256d(&d);
    }
    let sha256_ns = t1.elapsed().as_nanos() as u64;

    let b3_per = blake3_ns / iters.max(1);
    let sh_per = sha256_ns / iters.max(1);
    let speedup = if b3_per == 0 { 0.0 } else { sh_per as f64 / b3_per as f64 };

    HashComparison {
        blake3_ns_per_op: b3_per,
        sha256d_ns_per_op: sh_per,
        speedup_x: speedup,
    }
}

pub fn cmd_blake3_bench() {
    println!();
    println!("  BLAKE3 vs SHA-256d Benchmark (10_000 iters)");
    println!("  ─────────────────────────────────────────────");
    let r = benchmark_compare(10_000);
    println!("  BLAKE3   : {} ns/op", r.blake3_ns_per_op);
    println!("  SHA-256d : {} ns/op", r.sha256d_ns_per_op);
    println!("  Speedup  : {:.2}x faster", r.speedup_x);
    println!();

    let block = Blake3Block::new(
        1,
        1_700_000_000,
        "a".repeat(64),
        "b".repeat(64),
        "0".repeat(64),
        HASH_VERSION_BLAKE3,
    );
    println!("  BLAKE3 block hash (diff=1): mining...");
    let mut b = block.clone();
    b.mine(1);
    println!("  hash  : {}", b.hash);
    println!("  nonce : {}", b.nonce);
    println!("  valid : {}", b.is_valid(1));
    println!();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blake3_deterministic() {
        let h1 = blake3_hash(b"hello world");
        let h2 = blake3_hash(b"hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_blake3_different_from_sha256() {
        let data = b"test data";
        let b3 = blake3_hash(data);
        let sh = sha256d(data);
        assert_ne!(b3, sh, "BLAKE3 và SHA-256d phải cho kết quả khác nhau");
    }

    #[test]
    fn test_sha256d_deterministic() {
        let h1 = sha256d(b"blockchain");
        let h2 = sha256d(b"blockchain");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_pow_hash_version_routing() {
        let data = b"pow test";
        let v0 = pow_hash(data, HASH_VERSION_SHA256);
        let v1 = pow_hash(data, HASH_VERSION_BLAKE3);
        assert_eq!(v0, sha256d(data));
        assert_eq!(v1, blake3_hash(data));
        assert_ne!(v0, v1);
    }

    #[test]
    fn test_meets_difficulty() {
        // hash phải bắt đầu bằng difficulty "0"s — thử với diff=0
        let hash = [0u8; 32];
        assert!(meets_difficulty(&hash, 0));
        assert!(meets_difficulty(&hash, 10)); // all zeros → hex "00000..." pass any diff

        let nonzero = [0xffu8; 32];
        assert!(meets_difficulty(&nonzero, 0));
        assert!(!meets_difficulty(&nonzero, 1));
    }

    #[test]
    fn test_leading_zeros() {
        let hash = [0u8; 32];
        assert_eq!(leading_zeros(&hash), 256);

        let hash2 = [0x00, 0x0f, 0xff, 0u8, 0u8, 0u8, 0u8, 0u8,
                     0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
                     0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
                     0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
        assert_eq!(leading_zeros(&hash2), 12); // 0x00 = 8 zeros, 0x0f = 4 zeros
    }

    #[test]
    fn test_blake3_block_mine_and_verify() {
        let mut block = Blake3Block::new(
            1,
            1_700_000_000,
            "txid_root".to_string(),
            "witness_root".to_string(),
            "0".repeat(64),
            HASH_VERSION_BLAKE3,
        );
        block.mine(1);
        assert!(block.hash.starts_with('0'));
        assert!(block.is_valid(1));
    }

    #[test]
    fn test_sha256_block_backward_compat() {
        let mut block = Blake3Block::new(
            1,
            1_700_000_000,
            "txid_root".to_string(),
            "witness_root".to_string(),
            "0".repeat(64),
            HASH_VERSION_SHA256,
        );
        block.mine(1);
        assert!(block.hash.starts_with('0'));
        assert!(block.is_valid(1));
        // hash phải khác với BLAKE3
        let mut b3_block = block.clone();
        b3_block.hash_version = HASH_VERSION_BLAKE3;
        b3_block.hash = String::new();
        b3_block.mine(1);
        // Nonces khác nhau (hoặc hash khác nhau nếu cùng nonce)
        assert_ne!(
            Blake3Block { hash_version: HASH_VERSION_SHA256, ..block.clone() }.calculate_hash(),
            Blake3Block { hash_version: HASH_VERSION_BLAKE3, ..block.clone() }.calculate_hash(),
        );
    }

    #[test]
    fn test_benchmark_compare_returns_results() {
        let r = benchmark_compare(100);
        assert!(r.blake3_ns_per_op > 0 || r.sha256d_ns_per_op > 0);
        // Speedup thường >= 1 nhưng không guarantee trong CI — chỉ check > 0
        assert!(r.speedup_x >= 0.0);
    }

    #[test]
    fn test_to_hex_length() {
        let hash = blake3_hash(b"test");
        let hex_str = to_hex(&hash);
        assert_eq!(hex_str.len(), 64);
    }
}
