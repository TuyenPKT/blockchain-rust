#![allow(dead_code)]

/// v5.6 — Fuzz testing + Property-based tests
///
/// Hai loại kiểm tra:
///
/// 1. Property-based tests (proptest):
///    - Dùng `proptest!` macro để generate random inputs
///    - Kiểm tra các invariants phải luôn đúng với mọi input
///    - Tests nằm trong mod tests của main.rs (dùng proptest! macro)
///
/// 2. Manual fuzz targets:
///    - FuzzTarget struct cho phép feed arbitrary bytes vào parsers
///    - Kiểm tra không panic (soundness), không infinite loop, không UB
///
/// Invariants được kiểm tra:
///   - Hash determinism: cùng input → cùng hash
///   - Message roundtrip: serialize → deserialize = identity
///   - Fee market bounds: fast ≥ medium ≥ slow ≥ min ≥ 1.0
///   - RBF bump ratio: is_valid_rbf_bump consistent với RBF_MIN_BUMP
///   - UTXO balance: total UTXOs = sum of coinbase rewards
///   - Block validity: is_valid() consistent với hash prefix

// ─── Proptest strategies ──────────────────────────────────────────────────────

/// Generate arbitrary hex string (lowercase, fixed length)
pub fn arb_hex_string(len: usize) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(len);
    // Deterministic based on len for non-proptest use
    for i in 0..len {
        let _ = write!(s, "{:x}", i % 16);
    }
    s
}

/// Generate a valid pubkey_hash hex string (40 chars = 20 bytes)
pub fn arb_pubkey_hash() -> &'static str {
    "aabbccddaabbccddaabbccddaabbccddaabbccdd"
}

// ─── Manual Fuzz Targets ──────────────────────────────────────────────────────

/// Kết quả của một fuzz run
#[derive(Debug, Clone)]
pub struct FuzzResult {
    pub input_len:  usize,
    pub panicked:   bool,
    pub parsed_ok:  bool,
    pub error_msg:  Option<String>,
}

/// Fuzz Message::deserialize với arbitrary bytes
pub fn fuzz_message_deserialize(data: &[u8]) -> FuzzResult {
    use crate::message::Message;
    let result = std::panic::catch_unwind(|| {
        Message::deserialize(data)
    });
    match result {
        Ok(Some(_)) => FuzzResult { input_len: data.len(), panicked: false, parsed_ok: true, error_msg: None },
        Ok(None)    => FuzzResult { input_len: data.len(), panicked: false, parsed_ok: false, error_msg: None },
        Err(_)      => FuzzResult { input_len: data.len(), panicked: true,  parsed_ok: false, error_msg: Some("panic".to_string()) },
    }
}

/// Fuzz Block::calculate_hash với arbitrary inputs
pub fn fuzz_block_hash(index: u64, timestamp: i64, nonce: u64, prev_hash: &str) -> FuzzResult {
    use crate::block::Block;
    let result = std::panic::catch_unwind(|| {
        Block::calculate_hash(index, timestamp, &[], prev_hash, nonce)
    });
    match result {
        Ok(hash) => FuzzResult {
            input_len: prev_hash.len(),
            panicked: false,
            parsed_ok: hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()),
            error_msg: None,
        },
        Err(_) => FuzzResult { input_len: prev_hash.len(), panicked: true, parsed_ok: false, error_msg: Some("panic".to_string()) },
    }
}

/// Fuzz storage: serialization roundtrip của Block
pub fn fuzz_block_serialization(index: u64, nonce: u64) -> bool {
    use crate::block::Block;
    let mut block = Block::new(
        index,
        vec![],
        "0".repeat(64),
    );
    block.nonce = nonce;
    block.hash = Block::calculate_hash(index, block.timestamp, &[], &block.prev_hash, nonce);
    // Serialize → Deserialize phải roundtrip
    if let Ok(json) = serde_json::to_vec(&block) {
        if let Ok(decoded) = serde_json::from_slice::<Block>(&json) {
            return decoded.hash == block.hash
                && decoded.index == block.index
                && decoded.nonce == block.nonce;
        }
    }
    false
}

/// Corpus mẫu cho fuzz testing Message::deserialize
pub fn message_fuzz_corpus() -> Vec<Vec<u8>> {
    vec![
        // Valid JSON messages
        br#"{"Ping":null}"#.to_vec(),
        br#"{"GetHeight":null}"#.to_vec(),
        br#"{"GetPeers":null}"#.to_vec(),
        br#"{"GetFeeEstimate":null}"#.to_vec(),
        br#"{"GetTemplate":null}"#.to_vec(),
        // Malformed
        b"{}".to_vec(),
        b"null".to_vec(),
        b"".to_vec(),
        b"\x00\x01\x02".to_vec(),
        // Partial JSON
        br#"{"NewBlock":"#.to_vec(),
        // Unicode garbage
        "💀🔥⛏".as_bytes().to_vec(),
        // Very long input
        b"A".repeat(100_000),
    ]
}

// ─── Property invariant checkers ─────────────────────────────────────────────
// Dùng trong proptest! macro ở main.rs

/// Invariant: hash always 64 hex chars
pub fn invariant_hash_is_64_hex(hash: &str) -> bool {
    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
}

/// Invariant: fee estimate ordering
pub fn invariant_fee_estimate_ordered(fast: f64, medium: f64, slow: f64, min: f64) -> bool {
    fast >= medium && medium >= slow && slow >= min && min >= 1.0
}

/// Invariant: RBF bump ratio consistent
pub fn invariant_rbf_bump(old_fee: u64, new_fee: u64, expected_valid: bool) -> bool {
    use crate::fee_market::{is_valid_rbf_bump, RBF_MIN_BUMP};
    let actual = is_valid_rbf_bump(old_fee, new_fee);
    let manual = new_fee as f64 >= old_fee as f64 * RBF_MIN_BUMP;
    actual == manual && actual == expected_valid
}

/// Invariant: block serialization roundtrip
pub fn invariant_block_roundtrip(block: &crate::block::Block) -> bool {
    if let Ok(json) = serde_json::to_vec(block) {
        if let Ok(decoded) = serde_json::from_slice::<crate::block::Block>(&json) {
            return decoded.hash == block.hash
                && decoded.index == block.index
                && decoded.nonce == block.nonce
                && decoded.prev_hash == block.prev_hash;
        }
    }
    false
}

// ─── Fuzz runner (batch mode) ─────────────────────────────────────────────────

pub struct FuzzSummary {
    pub total:   usize,
    pub panics:  usize,
    pub errors:  usize,
    pub ok:      usize,
}

/// Chạy fuzz corpus và trả về summary
pub fn run_message_fuzz_corpus() -> FuzzSummary {
    let corpus = message_fuzz_corpus();
    let mut summary = FuzzSummary { total: 0, panics: 0, errors: 0, ok: 0 };

    for input in &corpus {
        summary.total += 1;
        let r = fuzz_message_deserialize(input);
        if r.panicked    { summary.panics += 1; }
        else if r.parsed_ok { summary.ok    += 1; }
        else             { summary.errors += 1; }
    }
    summary
}
