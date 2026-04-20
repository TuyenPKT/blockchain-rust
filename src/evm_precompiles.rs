#![allow(dead_code)]
//! v26.1 — EVM Precompiled Contracts
//!
//! Addresses 0x01..0x09 (Byzantium set):
//!   0x01 ecRecover     — secp256k1 signature recovery
//!   0x02 SHA256        — SHA-2 256-bit hash
//!   0x03 RIPEMD160     — RIPEMD-160 hash (padded to 32 bytes)
//!   0x04 Identity      — copy input to output
//!   0x05 ModExp        — big-integer modular exponentiation
//!   0x06 BN128Add      — elliptic curve point addition (stub)
//!   0x07 BN128Mul      — elliptic curve scalar multiplication (stub)
//!   0x08 BN128Pairing  — pairing check (stub)
//!   0x09 Blake2F       — Blake2b compression (stub)

use sha2::Digest;

// ─── Gas costs ────────────────────────────────────────────────────────────────

pub const GAS_ECRECOVER:    u64 = 3_000;
pub const GAS_SHA256_BASE:  u64 = 60;
pub const GAS_SHA256_WORD:  u64 = 12;
pub const GAS_RIPEMD_BASE:  u64 = 600;
pub const GAS_RIPEMD_WORD:  u64 = 120;
pub const GAS_IDENTITY_BASE: u64 = 15;
pub const GAS_IDENTITY_WORD: u64 = 3;
pub const GAS_BN128_ADD:    u64 = 150;
pub const GAS_BN128_MUL:    u64 = 6_000;
pub const GAS_BN128_PAIR_BASE: u64 = 45_000;
pub const GAS_BN128_PAIR_PER:  u64 = 34_000;
pub const GAS_BLAKE2F_PER_ROUND: u64 = 1;

// ─── Result type ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PrecompileResult {
    pub gas_used:    u64,
    pub output:      Vec<u8>,
    pub success:     bool,
}

impl PrecompileResult {
    fn ok(gas_used: u64, output: Vec<u8>) -> Self {
        PrecompileResult { gas_used, output, success: true }
    }
    fn fail(gas_used: u64) -> Self {
        PrecompileResult { gas_used, output: vec![], success: false }
    }
}

// ─── Dispatcher ───────────────────────────────────────────────────────────────

/// Returns Some if address is a precompile (0x01..0x09), None otherwise.
pub fn call_precompile(addr: &[u8; 20], input: &[u8], gas_limit: u64) -> Option<PrecompileResult> {
    // Only addresses 0x00..0x00..0x01 through 0x09 are precompiles
    for i in 0..19 { if addr[i] != 0 { return None; } }
    match addr[19] {
        0x01 => Some(precompile_ecrecover(input, gas_limit)),
        0x02 => Some(precompile_sha256(input, gas_limit)),
        0x03 => Some(precompile_ripemd160(input, gas_limit)),
        0x04 => Some(precompile_identity(input, gas_limit)),
        0x05 => Some(precompile_modexp(input, gas_limit)),
        0x06 => Some(precompile_bn128_add(input, gas_limit)),
        0x07 => Some(precompile_bn128_mul(input, gas_limit)),
        0x08 => Some(precompile_bn128_pairing(input, gas_limit)),
        0x09 => Some(precompile_blake2f(input, gas_limit)),
        _    => None,
    }
}

pub fn is_precompile(addr: &[u8; 20]) -> bool {
    call_precompile(addr, &[], 0).is_some()
}

// ─── 0x01 ecRecover ───────────────────────────────────────────────────────────

fn precompile_ecrecover(input: &[u8], gas_limit: u64) -> PrecompileResult {
    if gas_limit < GAS_ECRECOVER { return PrecompileResult::fail(gas_limit); }

    // Input: hash(32) + v(32) + r(32) + s(32) = 128 bytes
    if input.len() < 128 { return PrecompileResult::ok(GAS_ECRECOVER, vec![0u8; 32]); }

    let hash = &input[0..32];
    let v    = input[63]; // low byte of v word
    let r    = &input[64..96];
    let s    = &input[96..128];

    let rec_id = match v {
        27 => 0i32,
        28 => 1i32,
        _  => return PrecompileResult::ok(GAS_ECRECOVER, vec![0u8; 32]),
    };

    let result = recover_secp256k1(hash, r, s, rec_id);
    match result {
        Some(addr) => {
            let mut out = vec![0u8; 32];
            out[12..32].copy_from_slice(&addr);
            PrecompileResult::ok(GAS_ECRECOVER, out)
        }
        None => PrecompileResult::ok(GAS_ECRECOVER, vec![0u8; 32]),
    }
}

fn recover_secp256k1(hash: &[u8], r: &[u8], s: &[u8], rec_id: i32) -> Option<[u8; 20]> {
    use secp256k1::{ecdsa::{RecoverableSignature, RecoveryId}, Message, SECP256K1};

    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(r);
    sig_bytes[32..].copy_from_slice(s);

    let rid  = RecoveryId::from_i32(rec_id).ok()?;
    let sig  = RecoverableSignature::from_compact(&sig_bytes, rid).ok()?;
    let msg  = Message::from_slice(hash).ok()?;
    let pubkey = SECP256K1.recover_ecdsa(&msg, &sig).ok()?;

    let pub_bytes = pubkey.serialize_uncompressed(); // 65 bytes: 04 + x + y
    let addr_hash = keccak160(&pub_bytes[1..]); // keccak256 of x||y, take last 20 bytes
    Some(addr_hash)
}

fn keccak160(data: &[u8]) -> [u8; 20] {
    // Simplified: SHA256 then take last 20 bytes (full impl uses keccak256)
    let hash = sha2::Sha256::digest(data);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&hash[12..]);
    addr
}

// ─── 0x02 SHA256 ─────────────────────────────────────────────────────────────

fn precompile_sha256(input: &[u8], gas_limit: u64) -> PrecompileResult {
    let words = (input.len() + 31) / 32;
    let cost  = GAS_SHA256_BASE + GAS_SHA256_WORD * words as u64;
    if gas_limit < cost { return PrecompileResult::fail(gas_limit); }
    let hash = sha2::Sha256::digest(input);
    PrecompileResult::ok(cost, hash.to_vec())
}

// ─── 0x03 RIPEMD160 ──────────────────────────────────────────────────────────

fn precompile_ripemd160(input: &[u8], gas_limit: u64) -> PrecompileResult {
    let words = (input.len() + 31) / 32;
    let cost  = GAS_RIPEMD_BASE + GAS_RIPEMD_WORD * words as u64;
    if gas_limit < cost { return PrecompileResult::fail(gas_limit); }
    use ripemd::Ripemd160;
    let hash = Ripemd160::digest(input);
    let mut out = vec![0u8; 32];
    out[12..32].copy_from_slice(&hash); // right-aligned in 32 bytes
    PrecompileResult::ok(cost, out)
}

// ─── 0x04 Identity ───────────────────────────────────────────────────────────

fn precompile_identity(input: &[u8], gas_limit: u64) -> PrecompileResult {
    let words = (input.len() + 31) / 32;
    let cost  = GAS_IDENTITY_BASE + GAS_IDENTITY_WORD * words as u64;
    if gas_limit < cost { return PrecompileResult::fail(gas_limit); }
    PrecompileResult::ok(cost, input.to_vec())
}

// ─── 0x05 ModExp ─────────────────────────────────────────────────────────────
//
// Input: base_len(32) + exp_len(32) + mod_len(32) + base + exp + mod

fn precompile_modexp(input: &[u8], gas_limit: u64) -> PrecompileResult {
    fn read_len(b: &[u8], off: usize) -> usize {
        if b.len() < off + 32 { return 0; }
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&b[off + 24..off + 32]);
        u64::from_be_bytes(arr).min(1024) as usize // cap at 1K for safety
    }

    let base_len = read_len(input, 0);
    let exp_len  = read_len(input, 32);
    let mod_len  = read_len(input, 64);

    // EIP-2565 gas (simplified)
    let max_len = base_len.max(mod_len) as u64;
    let exp_bytes = exp_len.min(32) as u64;
    let gas = (max_len * max_len / 64).max(1) * exp_bytes.max(1);
    let gas = gas.max(200);

    if gas_limit < gas { return PrecompileResult::fail(gas_limit); }

    let start = 96;
    let base_start = start;
    let exp_start  = start + base_len;
    let mod_start  = exp_start + exp_len;

    fn read_bytes(b: &[u8], off: usize, len: usize) -> Vec<u8> {
        if len == 0 { return vec![]; }
        let end = (off + len).min(b.len());
        let mut out = vec![0u8; len];
        if off < b.len() {
            let copy = end - off;
            out[..copy].copy_from_slice(&b[off..end]);
        }
        out
    }

    let base_bytes = read_bytes(input, base_start, base_len);
    let exp_bytes_v = read_bytes(input, exp_start, exp_len);
    let mod_bytes  = read_bytes(input, mod_start, mod_len);

    if mod_len == 0 { return PrecompileResult::ok(gas, vec![]); }

    // Simple big-int modexp using u128 for small values, fallback to 0 for large
    let result = big_modexp(&base_bytes, &exp_bytes_v, &mod_bytes, mod_len);
    PrecompileResult::ok(gas, result)
}

fn big_modexp(base: &[u8], exp: &[u8], modulus: &[u8], mod_len: usize) -> Vec<u8> {
    // For mod_len <= 8: use u64 arithmetic
    if mod_len <= 8 {
        let b = bytes_to_u64(base);
        let e = bytes_to_u64(exp);
        let m = bytes_to_u64(modulus);
        if m == 0 { return vec![0u8; mod_len]; }
        let r = modpow_u64(b, e, m);
        let mut out = vec![0u8; mod_len];
        let rb = r.to_be_bytes();
        let copy_len = mod_len.min(8);
        out[mod_len - copy_len..].copy_from_slice(&rb[8 - copy_len..]);
        out
    } else {
        vec![0u8; mod_len] // stub for large integers
    }
}

fn bytes_to_u64(b: &[u8]) -> u64 {
    let relevant = if b.len() > 8 { &b[b.len() - 8..] } else { b };
    let mut arr = [0u8; 8];
    arr[8 - relevant.len()..].copy_from_slice(relevant);
    u64::from_be_bytes(arr)
}

fn modpow_u64(mut base: u64, mut exp: u64, modulus: u64) -> u64 {
    if modulus == 1 { return 0; }
    let mut result = 1u64;
    base %= modulus;
    while exp > 0 {
        if exp & 1 == 1 {
            result = (result as u128 * base as u128 % modulus as u128) as u64;
        }
        exp >>= 1;
        base = (base as u128 * base as u128 % modulus as u128) as u64;
    }
    result
}

// ─── 0x06-0x09 Stubs ─────────────────────────────────────────────────────────

fn precompile_bn128_add(_input: &[u8], gas_limit: u64) -> PrecompileResult {
    if gas_limit < GAS_BN128_ADD { return PrecompileResult::fail(gas_limit); }
    PrecompileResult::ok(GAS_BN128_ADD, vec![0u8; 64]) // stub: point at infinity
}

fn precompile_bn128_mul(_input: &[u8], gas_limit: u64) -> PrecompileResult {
    if gas_limit < GAS_BN128_MUL { return PrecompileResult::fail(gas_limit); }
    PrecompileResult::ok(GAS_BN128_MUL, vec![0u8; 64])
}

fn precompile_bn128_pairing(input: &[u8], gas_limit: u64) -> PrecompileResult {
    let pairs = input.len() / 192;
    let gas = GAS_BN128_PAIR_BASE + GAS_BN128_PAIR_PER * pairs as u64;
    if gas_limit < gas { return PrecompileResult::fail(gas_limit); }
    let mut out = vec![0u8; 32];
    out[31] = 1; // stub: pairing result = 1 (valid)
    PrecompileResult::ok(gas, out)
}

fn precompile_blake2f(input: &[u8], gas_limit: u64) -> PrecompileResult {
    if input.len() != 213 { return PrecompileResult::fail(0); }
    let rounds = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let gas = GAS_BLAKE2F_PER_ROUND * rounds as u64;
    if gas_limit < gas { return PrecompileResult::fail(gas_limit); }
    PrecompileResult::ok(gas, vec![0u8; 64]) // stub output
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(n: u8) -> [u8; 20] { let mut a = [0u8; 20]; a[19] = n; a }

    #[test]
    fn test_not_precompile() {
        let mut a = [0u8; 20]; a[0] = 1; // not a precompile
        assert!(call_precompile(&a, &[], 1_000_000).is_none());
    }

    #[test]
    fn test_sha256_empty() {
        let r = call_precompile(&addr(2), &[], 1_000_000).unwrap();
        assert!(r.success);
        assert_eq!(r.output.len(), 32);
        // SHA256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(r.output[0], 0xe3);
    }

    #[test]
    fn test_sha256_known_value() {
        // SHA256("abc")
        let r = call_precompile(&addr(2), b"abc", 1_000_000).unwrap();
        assert!(r.success);
        assert_eq!(r.output[0], 0xba); // ba7816bf...
    }

    #[test]
    fn test_sha256_out_of_gas() {
        let r = call_precompile(&addr(2), b"hello", 1).unwrap(); // need 60+
        assert!(!r.success);
    }

    #[test]
    fn test_ripemd160_empty() {
        let r = call_precompile(&addr(3), &[], 1_000_000).unwrap();
        assert!(r.success);
        assert_eq!(r.output.len(), 32);
        assert_eq!(&r.output[0..12], &[0u8; 12]); // left-padded
    }

    #[test]
    fn test_identity() {
        let r = call_precompile(&addr(4), b"hello world", 1_000_000).unwrap();
        assert!(r.success);
        assert_eq!(r.output, b"hello world");
    }

    #[test]
    fn test_identity_out_of_gas() {
        let r = call_precompile(&addr(4), b"x", 1).unwrap(); // need 15
        assert!(!r.success);
    }

    #[test]
    fn test_modexp_2_pow_3_mod_5() {
        // base=2, exp=3, mod=5 → result=3
        let mut input = vec![0u8; 96];
        input[31] = 1; // base_len = 1
        input[63] = 1; // exp_len  = 1
        input[95] = 1; // mod_len  = 1
        input.push(2); // base = 2
        input.push(3); // exp  = 3
        input.push(5); // mod  = 5
        let r = call_precompile(&addr(5), &input, 1_000_000).unwrap();
        assert!(r.success);
        assert_eq!(r.output, vec![3]);
    }

    #[test]
    fn test_modexp_zero_mod() {
        let input = vec![0u8; 96]; // all lens = 0
        let r = call_precompile(&addr(5), &input, 1_000_000).unwrap();
        assert!(r.success);
        assert_eq!(r.output, Vec::<u8>::new());
    }

    #[test]
    fn test_ecrecover_wrong_v() {
        let input = vec![0u8; 128]; // v=0 is invalid
        let r = call_precompile(&addr(1), &input, 1_000_000).unwrap();
        assert!(r.success); // returns zeroes, doesn't error
        assert_eq!(r.output.len(), 32);
    }

    #[test]
    fn test_ecrecover_gas_cost() {
        let input = vec![0u8; 128];
        let r = call_precompile(&addr(1), &input, 1_000_000).unwrap();
        assert_eq!(r.gas_used, GAS_ECRECOVER);
    }

    #[test]
    fn test_bn128_add_stub() {
        let r = call_precompile(&addr(6), &[0u8; 128], 1_000_000).unwrap();
        assert!(r.success);
        assert_eq!(r.output.len(), 64);
    }

    #[test]
    fn test_bn128_mul_stub() {
        let r = call_precompile(&addr(7), &[0u8; 96], 1_000_000).unwrap();
        assert!(r.success);
        assert_eq!(r.output.len(), 64);
    }

    #[test]
    fn test_bn128_pairing_stub() {
        let r = call_precompile(&addr(8), &[], 1_000_000).unwrap();
        assert!(r.success);
        assert_eq!(r.output[31], 1); // valid pairing result
    }

    #[test]
    fn test_blake2f_wrong_length() {
        let r = call_precompile(&addr(9), &[0u8; 100], 1_000_000).unwrap();
        assert!(!r.success); // must be exactly 213 bytes
    }

    #[test]
    fn test_is_precompile_true() {
        for n in 1u8..=9 { assert!(is_precompile(&addr(n)), "0x{n:02X} should be precompile"); }
    }

    #[test]
    fn test_is_precompile_false() {
        assert!(!is_precompile(&addr(0)));
        assert!(!is_precompile(&addr(10)));
        assert!(!is_precompile(&[0xFF; 20]));
    }
}
