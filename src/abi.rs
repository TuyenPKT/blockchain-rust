#![allow(dead_code)]
//! v26.1 — Solidity ABI encoder / decoder
//!
//! Implements EIP-712 / Solidity ABI spec:
//!   - Function selector: keccak256(sig)[0..4] — simplified as SHA256[0..4]
//!   - Head/tail encoding for dynamic types (bytes, string, arrays)
//!   - Static types: uint256, int256, address, bool, bytes32, bytes1..32
//!   - Encode: AbiValue → calldata bytes
//!   - Decode: calldata → Vec<AbiValue>

use sha2::Digest;

// ─── ABI value types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiValue {
    Uint256([u8; 32]),
    Int256([u8; 32]),
    Address([u8; 20]),
    Bool(bool),
    Bytes32([u8; 32]),
    Bytes(Vec<u8>),      // dynamic
    String(String),       // dynamic
    Array(Vec<AbiValue>), // dynamic, same type elements
    Tuple(Vec<AbiValue>), // static if all elements static
}

impl AbiValue {
    pub fn uint(v: u64) -> Self {
        let mut b = [0u8; 32];
        b[24..].copy_from_slice(&v.to_be_bytes());
        AbiValue::Uint256(b)
    }

    pub fn address(a: [u8; 20]) -> Self { AbiValue::Address(a) }
    pub fn bool_(v: bool) -> Self { AbiValue::Bool(v) }
    pub fn bytes32(b: [u8; 32]) -> Self { AbiValue::Bytes32(b) }
    pub fn bytes(b: Vec<u8>) -> Self { AbiValue::Bytes(b) }
    pub fn string(s: impl Into<String>) -> Self { AbiValue::String(s.into()) }

    fn is_dynamic(&self) -> bool {
        matches!(self, AbiValue::Bytes(_) | AbiValue::String(_) | AbiValue::Array(_))
    }
}

// ─── Function selector ────────────────────────────────────────────────────────

/// Compute 4-byte function selector from signature string.
/// e.g. "transfer(address,uint256)" → first 4 bytes of SHA256(sig)
/// Note: Ethereum uses keccak256; we use SHA256 as internal substitute.
pub fn function_selector(sig: &str) -> [u8; 4] {
    let hash = sha2::Sha256::digest(sig.as_bytes());
    [hash[0], hash[1], hash[2], hash[3]]
}

/// Build full calldata: selector(4) + encoded args
pub fn encode_call(sig: &str, args: &[AbiValue]) -> Vec<u8> {
    let mut out = function_selector(sig).to_vec();
    out.extend_from_slice(&encode(args));
    out
}

// ─── Encoder ─────────────────────────────────────────────────────────────────

/// Encode a sequence of ABI values (no selector prefix).
pub fn encode(values: &[AbiValue]) -> Vec<u8> {
    let mut head = Vec::new();
    let mut tail = Vec::new();
    let base_offset = values.len() * 32; // head is always N×32 bytes

    for v in values {
        if v.is_dynamic() {
            let offset = base_offset + tail.len();
            head.extend_from_slice(&pad_u256(offset as u64));
            tail.extend_from_slice(&encode_dynamic(v));
        } else {
            head.extend_from_slice(&encode_static(v));
        }
    }

    head.extend_from_slice(&tail);
    head
}

fn encode_static(v: &AbiValue) -> Vec<u8> {
    match v {
        AbiValue::Uint256(b) | AbiValue::Int256(b) | AbiValue::Bytes32(b) => b.to_vec(),
        AbiValue::Address(a) => {
            let mut out = vec![0u8; 32];
            out[12..].copy_from_slice(a);
            out
        }
        AbiValue::Bool(b) => pad_u256(*b as u64),
        AbiValue::Tuple(items) => encode(items),
        _ => vec![0u8; 32],
    }
}

fn encode_bytes_payload(b: &[u8]) -> Vec<u8> {
    let mut out = pad_u256(b.len() as u64);
    out.extend_from_slice(b);
    let pad = (32 - b.len() % 32) % 32;
    out.extend_from_slice(&vec![0u8; pad]);
    out
}

fn encode_dynamic(v: &AbiValue) -> Vec<u8> {
    match v {
        AbiValue::Bytes(b) => encode_bytes_payload(b),
        AbiValue::String(s) => encode_bytes_payload(s.as_bytes()),
        AbiValue::Array(items) => {
            let mut out = pad_u256(items.len() as u64);
            out.extend_from_slice(&encode(items));
            out
        }
        _ => encode_static(v),
    }
}

fn pad_u256(v: u64) -> Vec<u8> {
    let mut b = vec![0u8; 32];
    b[24..].copy_from_slice(&v.to_be_bytes());
    b
}

// ─── Decoder ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiType {
    Uint256,
    Int256,
    Address,
    Bool,
    Bytes32,
    Bytes,
    String,
    Array(Box<AbiType>),
}

pub fn decode(data: &[u8], types: &[AbiType]) -> Result<Vec<AbiValue>, String> {
    let mut values = Vec::with_capacity(types.len());
    let mut head_pos = 0usize;

    for t in types {
        if head_pos + 32 > data.len() {
            return Err(format!("not enough data at offset {head_pos}"));
        }
        match t {
            AbiType::Uint256 => {
                let mut b = [0u8; 32];
                b.copy_from_slice(&data[head_pos..head_pos + 32]);
                values.push(AbiValue::Uint256(b));
                head_pos += 32;
            }
            AbiType::Int256 => {
                let mut b = [0u8; 32];
                b.copy_from_slice(&data[head_pos..head_pos + 32]);
                values.push(AbiValue::Int256(b));
                head_pos += 32;
            }
            AbiType::Address => {
                let mut a = [0u8; 20];
                a.copy_from_slice(&data[head_pos + 12..head_pos + 32]);
                values.push(AbiValue::Address(a));
                head_pos += 32;
            }
            AbiType::Bool => {
                let v = data[head_pos + 31] != 0;
                values.push(AbiValue::Bool(v));
                head_pos += 32;
            }
            AbiType::Bytes32 => {
                let mut b = [0u8; 32];
                b.copy_from_slice(&data[head_pos..head_pos + 32]);
                values.push(AbiValue::Bytes32(b));
                head_pos += 32;
            }
            AbiType::Bytes | AbiType::String => {
                let offset = read_u64(&data[head_pos..head_pos + 32]) as usize;
                head_pos += 32;
                if offset + 32 > data.len() {
                    return Err(format!("dynamic offset {offset} out of bounds"));
                }
                let len = read_u64(&data[offset..offset + 32]) as usize;
                let start = offset + 32;
                if start + len > data.len() {
                    return Err(format!("dynamic data out of bounds: {start}+{len}>{}", data.len()));
                }
                let bytes = data[start..start + len].to_vec();
                if matches!(t, AbiType::String) {
                    let s = String::from_utf8(bytes).map_err(|e| e.to_string())?;
                    values.push(AbiValue::String(s));
                } else {
                    values.push(AbiValue::Bytes(bytes));
                }
            }
            AbiType::Array(elem_type) => {
                let offset = read_u64(&data[head_pos..head_pos + 32]) as usize;
                head_pos += 32;
                if offset + 32 > data.len() {
                    return Err("array offset out of bounds".into());
                }
                let count = read_u64(&data[offset..offset + 32]) as usize;
                let elem_data = &data[offset + 32..];
                let elem_types: Vec<AbiType> = (0..count).map(|_| *elem_type.clone()).collect();
                let elems = decode(elem_data, &elem_types)?;
                values.push(AbiValue::Array(elems));
            }
        }
    }

    Ok(values)
}

fn read_u64(b: &[u8]) -> u64 {
    if b.len() < 8 { return 0; }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&b[b.len() - 8..b.len().min(b.len())]);
    // Last 8 bytes of 32-byte word
    let start = if b.len() >= 8 { b.len() - 8 } else { 0 };
    arr.copy_from_slice(&b[start..start + 8]);
    u64::from_be_bytes(arr)
}

// ─── ERC-20 helper selectors ──────────────────────────────────────────────────

pub fn selector_transfer()      -> [u8; 4] { function_selector("transfer(address,uint256)") }
pub fn selector_balance_of()    -> [u8; 4] { function_selector("balanceOf(address)") }
pub fn selector_total_supply()  -> [u8; 4] { function_selector("totalSupply()") }
pub fn selector_approve()       -> [u8; 4] { function_selector("approve(address,uint256)") }
pub fn selector_allowance()     -> [u8; 4] { function_selector("allowance(address,address)") }
pub fn selector_transfer_from() -> [u8; 4] { function_selector("transferFrom(address,address,uint256)") }

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selector_is_4_bytes() {
        let s = function_selector("transfer(address,uint256)");
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn test_selector_deterministic() {
        assert_eq!(
            function_selector("transfer(address,uint256)"),
            function_selector("transfer(address,uint256)")
        );
    }

    #[test]
    fn test_selector_differs_by_sig() {
        assert_ne!(
            function_selector("transfer(address,uint256)"),
            function_selector("transferFrom(address,address,uint256)")
        );
    }

    #[test]
    fn test_encode_uint256() {
        let v = AbiValue::uint(255);
        let enc = encode(&[v]);
        assert_eq!(enc.len(), 32);
        assert_eq!(enc[31], 255);
    }

    #[test]
    fn test_encode_address() {
        let addr = [0xABu8; 20];
        let v = AbiValue::address(addr);
        let enc = encode(&[v]);
        assert_eq!(enc.len(), 32);
        assert_eq!(&enc[12..], &addr);
    }

    #[test]
    fn test_encode_bool_true() {
        let enc = encode(&[AbiValue::bool_(true)]);
        assert_eq!(enc[31], 1);
    }

    #[test]
    fn test_encode_bool_false() {
        let enc = encode(&[AbiValue::bool_(false)]);
        assert_eq!(enc[31], 0);
    }

    #[test]
    fn test_encode_bytes_dynamic() {
        let b = vec![1u8, 2, 3];
        let enc = encode(&[AbiValue::bytes(b.clone())]);
        // offset(32) + len(32) + padded data(32) = 96
        assert_eq!(enc.len(), 96);
        // first word = offset = 32
        assert_eq!(enc[31], 32);
        // second word = len = 3
        assert_eq!(enc[63], 3);
        assert_eq!(&enc[64..67], &[1, 2, 3]);
    }

    #[test]
    fn test_encode_string_dynamic() {
        let s = "hello";
        let enc = encode(&[AbiValue::string(s)]);
        assert!(enc.len() >= 96);
        // offset
        assert_eq!(enc[31], 32);
        // length
        assert_eq!(enc[63], 5);
        assert_eq!(&enc[64..69], b"hello");
    }

    #[test]
    fn test_encode_two_statics() {
        let enc = encode(&[AbiValue::uint(1), AbiValue::uint(2)]);
        assert_eq!(enc.len(), 64);
        assert_eq!(enc[31], 1);
        assert_eq!(enc[63], 2);
    }

    #[test]
    fn test_decode_uint256() {
        let enc = encode(&[AbiValue::uint(42)]);
        let vals = decode(&enc, &[AbiType::Uint256]).unwrap();
        assert_eq!(vals[0], AbiValue::uint(42));
    }

    #[test]
    fn test_decode_address() {
        let addr = [0x11u8; 20];
        let enc = encode(&[AbiValue::address(addr)]);
        let vals = decode(&enc, &[AbiType::Address]).unwrap();
        assert_eq!(vals[0], AbiValue::address(addr));
    }

    #[test]
    fn test_decode_bool() {
        let enc = encode(&[AbiValue::bool_(true)]);
        let vals = decode(&enc, &[AbiType::Bool]).unwrap();
        assert_eq!(vals[0], AbiValue::Bool(true));
    }

    #[test]
    fn test_decode_bytes_roundtrip() {
        let b = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let enc = encode(&[AbiValue::bytes(b.clone())]);
        let vals = decode(&enc, &[AbiType::Bytes]).unwrap();
        assert_eq!(vals[0], AbiValue::Bytes(b));
    }

    #[test]
    fn test_decode_string_roundtrip() {
        let s = "PKT network";
        let enc = encode(&[AbiValue::string(s)]);
        let vals = decode(&enc, &[AbiType::String]).unwrap();
        assert_eq!(vals[0], AbiValue::String(s.to_string()));
    }

    #[test]
    fn test_encode_call_prefix() {
        let sig = "transfer(address,uint256)";
        let args = [AbiValue::address([0u8; 20]), AbiValue::uint(100)];
        let data = encode_call(sig, &args);
        assert_eq!(&data[..4], &function_selector(sig));
        assert_eq!(data.len(), 4 + 64);
    }

    #[test]
    fn test_erc20_selectors_unique() {
        let sels = [
            selector_transfer(),
            selector_balance_of(),
            selector_total_supply(),
            selector_approve(),
            selector_allowance(),
            selector_transfer_from(),
        ];
        for i in 0..sels.len() {
            for j in i + 1..sels.len() {
                assert_ne!(sels[i], sels[j], "selectors {i} and {j} collide");
            }
        }
    }

    #[test]
    fn test_decode_error_insufficient_data() {
        let result = decode(&[0u8; 10], &[AbiType::Uint256]);
        assert!(result.is_err());
    }
}
