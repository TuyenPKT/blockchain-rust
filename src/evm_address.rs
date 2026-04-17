#![allow(dead_code)]
//! v25.6 — PKT Address Format (0x-prefixed)
//!
//! Địa chỉ `0x...` — PKT dùng RIPEMD160(BLAKE3(compressed_pubkey)).
//! Không dùng Keccak256 (đó là Ethereum). Consistent với pkt_script::verify_p2pkh_input.
//!
//! ## Quy trình derive:
//!   1. compressed pubkey 33 bytes
//!   2. BLAKE3(33 bytes) → 32 bytes
//!   3. RIPEMD160(32 bytes) → 20 bytes (hash160)
//!   4. "0x" + hex(hash160) → 42 ký tự
//!
//! ## Public API
//!   `pubkey_to_evm_address(pubkey: &[u8]) -> String`   — từ 33/65-byte pubkey
//!   `raw_to_evm_address(bytes: &[u8; 20]) -> String`   — từ 20-byte hash
//!   `is_valid_evm_address(s: &str) -> bool`            — validate format


// ── Core: pubkey → PKT address ─────────────────────────────────────────────

/// Derive PKT address từ secp256k1 public key.
///
/// Chấp nhận:
///   - 33 bytes (compressed, 0x02/0x03 prefix)
///   - 65 bytes (uncompressed, 0x04 prefix) → tự động convert sang compressed
///
/// hash160 = RIPEMD160(BLAKE3(compressed)) — consistent với pkt_script::verify_p2pkh_input.
/// Returns `"0x" + hex(hash160)` — 42 ký tự.
pub fn pubkey_to_evm_address(pubkey_bytes: &[u8]) -> Result<String, String> {
    use ripemd::{Ripemd160, Digest as _};
    // Lấy compressed pubkey 33 bytes
    let compressed: Vec<u8> = match pubkey_bytes.len() {
        33 => pubkey_bytes.to_vec(),
        65 => {
            if pubkey_bytes[0] != 0x04 {
                return Err("uncompressed pubkey must start with 0x04".into());
            }
            // uncompressed → compressed qua secp256k1
            use secp256k1::PublicKey;
            let pk = PublicKey::from_slice(pubkey_bytes)
                .map_err(|e| format!("invalid uncompressed pubkey: {e}"))?;
            pk.serialize().to_vec() // 33 bytes compressed
        }
        n => return Err(format!("pubkey must be 33 or 65 bytes, got {n}")),
    };

    // RIPEMD160(BLAKE3(compressed)) → 20 bytes
    let b3  = blake3::hash(&compressed);
    let raw: [u8; 20] = Ripemd160::digest(b3.as_bytes()).into();
    Ok(raw_to_evm_address(&raw))
}

/// Chuyển 20-byte hash160 → `0x...` string (lowercase hex, không EIP-55).
pub fn raw_to_evm_address(raw: &[u8; 20]) -> String {
    format!("0x{}", hex::encode(raw))
}

/// Parse `0x...` hoặc `0X...` → [u8; 20]. Không validate EIP-55.
pub fn parse_evm_address(s: &str) -> Result<[u8; 20], String> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
        .ok_or_else(|| format!("địa chỉ EVM phải bắt đầu bằng '0x': {s}"))?;
    if hex.len() != 40 {
        return Err(format!("địa chỉ EVM phải có đúng 40 ký tự hex, got {}", hex.len()));
    }
    let bytes = hex::decode(hex).map_err(|e| format!("hex decode: {e}"))?;
    bytes.try_into().map_err(|_| "slice error".to_string())
}

/// Kiểm tra format cơ bản: `0x` + 40 hex chars (không validate EIP-55).
pub fn is_valid_evm_address(s: &str) -> bool {
    parse_evm_address(s).is_ok()
}

/// Normalize: lowercase hex `0x...` (bỏ EIP-55, dùng để so sánh / lưu DB).
pub fn normalize_evm_address(s: &str) -> Option<String> {
    let raw = parse_evm_address(s).ok()?;
    Some(format!("0x{}", hex::encode(raw)))
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Secp256k1, SecretKey};

    /// Tạo compressed pubkey từ u8 seed (deterministic).
    fn make_pubkey(seed: u8) -> Vec<u8> {
        let secp = Secp256k1::new();
        let mut sk_bytes = [1u8; 32]; // all-1 base, ensure non-zero
        sk_bytes[31] = seed;
        let sk = SecretKey::from_slice(&sk_bytes).unwrap();
        let pk = secp256k1::PublicKey::from_secret_key(&secp, &sk);
        pk.serialize().to_vec() // 33 bytes compressed
    }

    #[test]
    fn test_evm_address_starts_with_0x() {
        let pk = make_pubkey(1);
        let addr = pubkey_to_evm_address(&pk).unwrap();
        assert!(addr.starts_with("0x"), "địa chỉ phải bắt đầu bằng '0x'");
    }

    #[test]
    fn test_evm_address_length_42() {
        let pk = make_pubkey(2);
        let addr = pubkey_to_evm_address(&pk).unwrap();
        assert_eq!(addr.len(), 42, "0x + 40 hex chars = 42");
    }

    #[test]
    fn test_evm_address_deterministic() {
        let pk = make_pubkey(3);
        let a1 = pubkey_to_evm_address(&pk).unwrap();
        let a2 = pubkey_to_evm_address(&pk).unwrap();
        assert_eq!(a1, a2, "cùng pubkey → cùng địa chỉ");
    }

    #[test]
    fn test_different_pubkeys_different_addresses() {
        let addr1 = pubkey_to_evm_address(&make_pubkey(10)).unwrap();
        let addr2 = pubkey_to_evm_address(&make_pubkey(11)).unwrap();
        assert_ne!(addr1, addr2);
    }

    #[test]
    fn test_address_is_lowercase_hex() {
        // PKT address = 0x + lowercase hex (không EIP-55)
        let pk = make_pubkey(5);
        let addr = pubkey_to_evm_address(&pk).unwrap();
        assert!(addr[2..].chars().all(|c| !c.is_ascii_uppercase()),
            "PKT address phải lowercase hex");
        // roundtrip: parse → raw_to_evm_address → same
        let raw = parse_evm_address(&addr).unwrap();
        assert_eq!(addr, raw_to_evm_address(&raw));
    }

    #[test]
    fn test_raw_to_evm_address_known_vector() {
        let raw = [0x52u8, 0x90, 0x8f, 0x89, 0x8b, 0x73, 0x18, 0x6c, 0xc0,
                   0x8e, 0x13, 0xdb, 0xf1, 0x84, 0xf9, 0x2e, 0xef, 0x37, 0x59, 0x58];
        let addr = raw_to_evm_address(&raw);
        assert!(addr.starts_with("0x"));
        assert_eq!(addr.len(), 42);
        assert!(addr[2..].chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn test_parse_evm_address_valid() {
        let addr = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
        assert!(parse_evm_address(addr).is_ok());
    }

    #[test]
    fn test_parse_evm_address_no_prefix_fails() {
        assert!(parse_evm_address("5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed").is_err());
    }

    #[test]
    fn test_parse_evm_address_too_short_fails() {
        assert!(parse_evm_address("0x1234").is_err());
    }

    #[test]
    fn test_is_valid_evm_address_ok() {
        assert!(is_valid_evm_address("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"));
    }

    #[test]
    fn test_is_valid_evm_address_bad() {
        assert!(!is_valid_evm_address("pkt1qxxxxxx"));
        assert!(!is_valid_evm_address("0x123"));
        assert!(!is_valid_evm_address(""));
    }

    #[test]
    fn test_normalize_evm_address() {
        let addr = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
        let norm = normalize_evm_address(addr).unwrap();
        assert!(norm.starts_with("0x"));
        assert_eq!(norm.len(), 42);
        // normalized = lowercase hex
        assert!(norm[2..].chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn test_compressed_and_uncompressed_same_address() {
        let secp = Secp256k1::new();
        let mut sk_bytes = [1u8; 32];
        sk_bytes[31] = 7;
        let sk = SecretKey::from_slice(&sk_bytes).unwrap();
        let pk = secp256k1::PublicKey::from_secret_key(&secp, &sk);

        let compressed   = pk.serialize().to_vec();         // 33 bytes
        let uncompressed = pk.serialize_uncompressed().to_vec(); // 65 bytes

        let addr_c = pubkey_to_evm_address(&compressed).unwrap();
        let addr_u = pubkey_to_evm_address(&uncompressed).unwrap();
        assert_eq!(addr_c, addr_u, "compressed và uncompressed phải cho cùng địa chỉ");
    }

    #[test]
    fn test_invalid_pubkey_length_fails() {
        assert!(pubkey_to_evm_address(&[0u8; 10]).is_err());
        assert!(pubkey_to_evm_address(&[0u8; 64]).is_err());
    }
}
