#![allow(dead_code)]
//! v13.3 — PKT Address Format
//!
//! Bech32/Bech32m encoding cho PKT Cash addresses (không cần crate ngoài):
//!   - HRP "pkt"  → PKT mainnet
//!   - HRP "tpkt" → PKT testnet
//!   - HRP "rpkt" → PKT regtest
//!   - Witness v0, 20-byte (hash160)  → P2WPKH → bech32
//!   - Witness v0, 32-byte            → P2WSH  → bech32
//!   - Witness v1, 32-byte (x-only)   → P2TR   → bech32m
//!
//! Tham chiếu: BIP-0173 (bech32), BIP-0350 (bech32m), PKT address spec

// ── Constants ──────────────────────────────────────────────────────────────

pub const PKT_MAINNET_HRP: &str = "pkt";
pub const PKT_TESTNET_HRP: &str = "tpkt";
pub const PKT_REGTEST_HRP:  &str = "rpkt";

const CHARSET: &[u8]      = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const BECH32_CONST: u32   = 1;
const BECH32M_CONST: u32  = 0x2bc830a3;

// ── Polymod / checksum ──────────────────────────────────────────────────────

fn polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let mut chk: u32 = 1;
    for v in values {
        let b = (chk >> 25) as u8;
        chk = (chk & 0x1ff_ffff) << 5 ^ (*v as u32);
        for (i, &g) in GEN.iter().enumerate() {
            if (b >> i) & 1 == 1 { chk ^= g; }
        }
    }
    chk
}

fn hrp_expand(hrp: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(hrp.len() * 2 + 1);
    for c in hrp.bytes() { out.push(c >> 5); }
    out.push(0);
    for c in hrp.bytes() { out.push(c & 0x1f); }
    out
}

fn create_checksum(hrp: &str, data: &[u8], constant: u32) -> [u8; 6] {
    let mut values = hrp_expand(hrp);
    values.extend_from_slice(data);
    values.extend_from_slice(&[0u8; 6]);
    let pmv = polymod(&values) ^ constant;
    let mut ret = [0u8; 6];
    for i in 0..6 {
        ret[i] = ((pmv >> (5 * (5 - i))) & 0x1f) as u8;
    }
    ret
}

fn verify_checksum(hrp: &str, data: &[u8], constant: u32) -> bool {
    let mut values = hrp_expand(hrp);
    values.extend_from_slice(data);
    polymod(&values) == constant
}

// ── Bit conversion ──────────────────────────────────────────────────────────

/// Chuyển đổi giữa các nhóm bit (8→5 khi encode, 5→8 khi decode)
fn convertbits(data: &[u8], from: u32, to: u32, pad: bool) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut ret = Vec::new();
    let maxv = (1u32 << to) - 1;
    for &byte in data {
        let value = byte as u32;
        if value >> from != 0 { return None; }
        acc = (acc << from) | value;
        bits += from;
        while bits >= to {
            bits -= to;
            ret.push(((acc >> bits) & maxv) as u8);
        }
    }
    if pad {
        if bits > 0 { ret.push(((acc << (to - bits)) & maxv) as u8); }
    } else if bits >= from || ((acc << (to - bits)) & maxv) != 0 {
        return None;
    }
    Some(ret)
}

// ── Core encode / decode ────────────────────────────────────────────────────

fn encode_raw(hrp: &str, witver: u8, witprog: &[u8]) -> Result<String, String> {
    if witver > 16 {
        return Err(format!("witness version {} > 16", witver));
    }
    let conv = convertbits(witprog, 8, 5, true)
        .ok_or("convertbits(8→5) failed")?;
    let constant = if witver == 0 { BECH32_CONST } else { BECH32M_CONST };

    let mut data = Vec::with_capacity(1 + conv.len() + 6);
    data.push(witver);
    data.extend_from_slice(&conv);
    let checksum = create_checksum(hrp, &data, constant);
    data.extend_from_slice(&checksum);

    let mut result = format!("{}1", hrp);
    for d in &data {
        result.push(CHARSET[*d as usize] as char);
    }
    Ok(result)
}

fn decode_raw(addr: &str) -> Result<(String, u8, Vec<u8>), String> {
    let lower = addr.to_ascii_lowercase();
    let sep = lower.rfind('1').ok_or("no '1' separator")?;
    if sep == 0 { return Err("empty HRP".into()); }
    let data_str = &lower[sep + 1..];
    if data_str.len() < 7 { return Err("address too short".into()); }  // ver + ≥1 + 6 checksum

    let hrp = &lower[..sep];

    // Charset decode
    let mut data = Vec::with_capacity(data_str.len());
    for c in data_str.chars() {
        let idx = CHARSET.iter().position(|&x| x == c as u8)
            .ok_or_else(|| format!("invalid character '{}'", c))?;
        data.push(idx as u8);
    }

    let witver = data[0];
    if witver > 16 { return Err(format!("invalid witness version {}", witver)); }

    let constant = if witver == 0 { BECH32_CONST } else { BECH32M_CONST };
    if !verify_checksum(hrp, &data, constant) {
        return Err("invalid checksum".into());
    }

    // Strip checksum (last 6) and version byte
    let payload = &data[1..data.len() - 6];
    let witprog = convertbits(payload, 5, 8, false)
        .ok_or("convertbits(5→8) failed")?;

    if witprog.len() < 2 || witprog.len() > 40 {
        return Err(format!("witness program length {} invalid", witprog.len()));
    }
    if witver == 0 && witprog.len() != 20 && witprog.len() != 32 {
        return Err(format!("v0 program must be 20 or 32 bytes, got {}", witprog.len()));
    }
    if witver == 1 && witprog.len() != 32 {
        return Err(format!("v1 program must be 32 bytes, got {}", witprog.len()));
    }

    Ok((hrp.to_string(), witver, witprog))
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Encode P2WPKH address (witness v0, 20-byte hash160) — bech32
pub fn encode_p2wpkh(hrp: &str, hash160: &[u8; 20]) -> Result<String, String> {
    encode_raw(hrp, 0, hash160)
}

/// Encode P2WSH address (witness v0, 32-byte script hash) — bech32
pub fn encode_p2wsh(hrp: &str, script_hash: &[u8; 32]) -> Result<String, String> {
    encode_raw(hrp, 0, script_hash)
}

/// Encode P2TR address (witness v1, 32-byte x-only pubkey) — bech32m
pub fn encode_p2tr(hrp: &str, xonly_pubkey: &[u8; 32]) -> Result<String, String> {
    encode_raw(hrp, 1, xonly_pubkey)
}

/// Decode bất kỳ PKT bech32/bech32m address → `PktAddress`
pub fn decode_address(addr: &str) -> Result<PktAddress, String> {
    let (hrp, witver, witprog) = decode_raw(addr)?;
    let network = match hrp.as_str() {
        PKT_MAINNET_HRP => PktNetwork::Mainnet,
        PKT_TESTNET_HRP => PktNetwork::Testnet,
        PKT_REGTEST_HRP  => PktNetwork::Regtest,
        other => return Err(format!("unknown HRP '{}'", other)),
    };
    let addr_type = match (witver, witprog.len()) {
        (0, 20) => PktAddrType::P2WPKH,
        (0, 32) => PktAddrType::P2WSH,
        (1, 32) => PktAddrType::P2TR,
        _       => PktAddrType::Unknown,
    };
    Ok(PktAddress { network, addr_type, witness_version: witver, witness_program: witprog })
}

/// Derive PKT P2WPKH address từ compressed secp256k1 pubkey (33 bytes)
/// hash160 = RIPEMD160(SHA256(pubkey))
pub fn pubkey_to_pkt_address(hrp: &str, compressed_pubkey: &[u8]) -> Result<String, String> {
    use sha2::{Sha256, Digest as _};
    use ripemd::Ripemd160;
    if compressed_pubkey.len() != 33 {
        return Err(format!("expected 33-byte compressed pubkey, got {}", compressed_pubkey.len()));
    }
    let sha = Sha256::digest(compressed_pubkey);
    let hash160: [u8; 20] = Ripemd160::digest(sha).into();
    encode_p2wpkh(hrp, &hash160)
}

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PktNetwork {
    Mainnet,
    Testnet,
    Regtest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PktAddrType {
    /// P2WPKH — pay to witness pubkey hash (hash160, 20 bytes, v0)
    P2WPKH,
    /// P2WSH  — pay to witness script hash (sha256, 32 bytes, v0)
    P2WSH,
    /// P2TR   — pay to taproot (x-only pubkey, 32 bytes, v1)
    P2TR,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct PktAddress {
    pub network:         PktNetwork,
    pub addr_type:       PktAddrType,
    pub witness_version: u8,
    pub witness_program: Vec<u8>,
}

impl PktAddress {
    pub fn is_mainnet(&self) -> bool { self.network == PktNetwork::Mainnet }
    pub fn is_testnet(&self) -> bool { self.network == PktNetwork::Testnet }

    /// Trả về hash160 nếu là P2WPKH
    pub fn hash160(&self) -> Option<[u8; 20]> {
        if self.addr_type == PktAddrType::P2WPKH {
            let mut arr = [0u8; 20];
            arr.copy_from_slice(&self.witness_program);
            Some(arr)
        } else { None }
    }

    /// Trả về x-only pubkey nếu là P2TR
    pub fn taproot_key(&self) -> Option<[u8; 32]> {
        if self.addr_type == PktAddrType::P2TR {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&self.witness_program);
            Some(arr)
        } else { None }
    }
}

impl std::fmt::Display for PktAddrType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PktAddrType::P2WPKH  => write!(f, "P2WPKH"),
            PktAddrType::P2WSH   => write!(f, "P2WSH"),
            PktAddrType::P2TR    => write!(f, "P2TR"),
            PktAddrType::Unknown => write!(f, "Unknown"),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Encode / decode roundtrip ─────────────────────────────────────────

    #[test]
    fn test_p2wpkh_encode_decode_roundtrip() {
        let hash160 = [0x1au8; 20];
        let addr = encode_p2wpkh(PKT_MAINNET_HRP, &hash160).unwrap();
        assert!(addr.starts_with("pkt1"));
        let parsed = decode_address(&addr).unwrap();
        assert_eq!(parsed.addr_type, PktAddrType::P2WPKH);
        assert_eq!(parsed.witness_program, hash160);
    }

    #[test]
    fn test_p2wsh_encode_decode_roundtrip() {
        let sh = [0x2bu8; 32];
        let addr = encode_p2wsh(PKT_MAINNET_HRP, &sh).unwrap();
        assert!(addr.starts_with("pkt1"));
        let parsed = decode_address(&addr).unwrap();
        assert_eq!(parsed.addr_type, PktAddrType::P2WSH);
        assert_eq!(parsed.witness_program, sh);
    }

    #[test]
    fn test_p2tr_encode_decode_roundtrip() {
        let xonly = [0x3cu8; 32];
        let addr = encode_p2tr(PKT_MAINNET_HRP, &xonly).unwrap();
        assert!(addr.starts_with("pkt1"));
        let parsed = decode_address(&addr).unwrap();
        assert_eq!(parsed.addr_type, PktAddrType::P2TR);
        assert_eq!(parsed.witness_program, xonly);
    }

    #[test]
    fn test_testnet_hrp() {
        let hash160 = [0x01u8; 20];
        let addr = encode_p2wpkh(PKT_TESTNET_HRP, &hash160).unwrap();
        assert!(addr.starts_with("tpkt1"));
        let parsed = decode_address(&addr).unwrap();
        assert!(parsed.is_testnet());
    }

    #[test]
    fn test_regtest_hrp() {
        let hash160 = [0x02u8; 20];
        let addr = encode_p2wpkh(PKT_REGTEST_HRP, &hash160).unwrap();
        assert!(addr.starts_with("rpkt1"));
        let parsed = decode_address(&addr).unwrap();
        assert_eq!(parsed.network, PktNetwork::Regtest);
    }

    // ── Address properties ────────────────────────────────────────────────

    #[test]
    fn test_p2wpkh_witness_version_is_0() {
        let h = [0xaau8; 20];
        let addr = encode_p2wpkh(PKT_MAINNET_HRP, &h).unwrap();
        let p = decode_address(&addr).unwrap();
        assert_eq!(p.witness_version, 0);
    }

    #[test]
    fn test_p2tr_witness_version_is_1() {
        let k = [0xbbu8; 32];
        let addr = encode_p2tr(PKT_MAINNET_HRP, &k).unwrap();
        let p = decode_address(&addr).unwrap();
        assert_eq!(p.witness_version, 1);
    }

    #[test]
    fn test_hash160_accessor() {
        let h = [0x77u8; 20];
        let addr = encode_p2wpkh(PKT_MAINNET_HRP, &h).unwrap();
        let p = decode_address(&addr).unwrap();
        assert_eq!(p.hash160(), Some(h));
    }

    #[test]
    fn test_taproot_key_accessor() {
        let k = [0x99u8; 32];
        let addr = encode_p2tr(PKT_MAINNET_HRP, &k).unwrap();
        let p = decode_address(&addr).unwrap();
        assert_eq!(p.taproot_key(), Some(k));
    }

    #[test]
    fn test_is_mainnet_flag() {
        let h = [0x11u8; 20];
        let addr = encode_p2wpkh(PKT_MAINNET_HRP, &h).unwrap();
        let p = decode_address(&addr).unwrap();
        assert!(p.is_mainnet());
        assert!(!p.is_testnet());
    }

    // ── Different hash values produce different addresses ─────────────────

    #[test]
    fn test_different_hashes_different_addresses() {
        let a = encode_p2wpkh(PKT_MAINNET_HRP, &[0x00u8; 20]).unwrap();
        let b = encode_p2wpkh(PKT_MAINNET_HRP, &[0xffu8; 20]).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn test_all_zero_hash160_roundtrip() {
        let h = [0u8; 20];
        let addr = encode_p2wpkh(PKT_MAINNET_HRP, &h).unwrap();
        let p = decode_address(&addr).unwrap();
        assert_eq!(p.hash160(), Some(h));
    }

    #[test]
    fn test_all_ff_hash160_roundtrip() {
        let h = [0xffu8; 20];
        let addr = encode_p2wpkh(PKT_MAINNET_HRP, &h).unwrap();
        let p = decode_address(&addr).unwrap();
        assert_eq!(p.hash160(), Some(h));
    }

    // ── Checksum validation ───────────────────────────────────────────────

    #[test]
    fn test_tampered_address_fails_decode() {
        let h = [0x55u8; 20];
        let addr = encode_p2wpkh(PKT_MAINNET_HRP, &h).unwrap();
        // Flip last char
        let mut chars: Vec<char> = addr.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == 'q' { 'p' } else { 'q' };
        let bad: String = chars.into_iter().collect();
        assert!(decode_address(&bad).is_err());
    }

    #[test]
    fn test_wrong_hrp_fails_decode() {
        let h = [0x11u8; 20];
        let addr = encode_p2wpkh(PKT_MAINNET_HRP, &h).unwrap();
        // Replace "pkt1" with "btc1" — will fail unknown HRP or checksum
        let bad = addr.replacen("pkt1", "btc1", 1);
        assert!(decode_address(&bad).is_err());
    }

    #[test]
    fn test_p2wpkh_and_p2tr_differ_for_same_bytes() {
        let bytes = [0x42u8; 32];
        let p2tr = encode_p2tr(PKT_MAINNET_HRP, &bytes).unwrap();
        let h20 = [0x42u8; 20];
        let p2wpkh = encode_p2wpkh(PKT_MAINNET_HRP, &h20).unwrap();
        assert_ne!(p2tr, p2wpkh);
    }

    // ── Address length / charset ──────────────────────────────────────────

    #[test]
    fn test_p2wpkh_mainnet_address_length() {
        // "pkt1" + 39 data chars (1 witver + 32 conv + 6 checksum) = 4 + 39 = 43
        let addr = encode_p2wpkh(PKT_MAINNET_HRP, &[0u8; 20]).unwrap();
        assert_eq!(addr.len(), 43, "P2WPKH pkt1 address should be 43 chars");
    }

    #[test]
    fn test_p2tr_mainnet_address_length() {
        // "pkt1" + 59 data chars (1 witver + 52 conv + 6 checksum) = 4 + 59 = 63
        let addr = encode_p2tr(PKT_MAINNET_HRP, &[0u8; 32]).unwrap();
        assert_eq!(addr.len(), 63, "P2TR pkt1 address should be 63 chars");
    }

    #[test]
    fn test_address_only_lowercase_bech32_charset() {
        let addr = encode_p2wpkh(PKT_MAINNET_HRP, &[0x33u8; 20]).unwrap();
        let valid = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l1";
        for c in addr.bytes() {
            assert!(valid.contains(&c), "unexpected char '{}' in address", c as char);
        }
    }

    // ── pubkey_to_pkt_address ─────────────────────────────────────────────

    #[test]
    fn test_pubkey_to_pkt_address_33_bytes_ok() {
        // Compressed pubkey: 0x02 prefix + 32 bytes
        let mut pk = [0u8; 33];
        pk[0] = 0x02;
        let addr = pubkey_to_pkt_address(PKT_MAINNET_HRP, &pk).unwrap();
        assert!(addr.starts_with("pkt1"));
        let p = decode_address(&addr).unwrap();
        assert_eq!(p.addr_type, PktAddrType::P2WPKH);
    }

    #[test]
    fn test_pubkey_to_pkt_address_wrong_length_fails() {
        let result = pubkey_to_pkt_address(PKT_MAINNET_HRP, &[0u8; 32]);
        assert!(result.is_err());
    }

    #[test]
    fn test_two_different_pubkeys_different_addresses() {
        let mut pk1 = [0u8; 33]; pk1[0] = 0x02;
        let mut pk2 = [0u8; 33]; pk2[0] = 0x03;
        let a1 = pubkey_to_pkt_address(PKT_MAINNET_HRP, &pk1).unwrap();
        let a2 = pubkey_to_pkt_address(PKT_MAINNET_HRP, &pk2).unwrap();
        assert_ne!(a1, a2);
    }

    // ── addr_type Display ─────────────────────────────────────────────────

    #[test]
    fn test_addr_type_display() {
        assert_eq!(format!("{}", PktAddrType::P2WPKH),  "P2WPKH");
        assert_eq!(format!("{}", PktAddrType::P2WSH),   "P2WSH");
        assert_eq!(format!("{}", PktAddrType::P2TR),    "P2TR");
        assert_eq!(format!("{}", PktAddrType::Unknown), "Unknown");
    }
}
