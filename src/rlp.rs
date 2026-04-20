#![allow(dead_code)]
//! v26.1 — RLP (Recursive Length Prefix) encoder/decoder
//!
//! Dùng cho: EIP-155 tx signing, eth_wire geth compat, receipt encoding.
//! Spec: Ethereum Yellow Paper Appendix B.

// ─── RLP value ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rlp {
    Bytes(Vec<u8>),
    List(Vec<Rlp>),
}

impl Rlp {
    pub fn bytes(b: impl Into<Vec<u8>>) -> Self { Rlp::Bytes(b.into()) }
    pub fn uint(v: u64) -> Self { Rlp::Bytes(encode_uint(v)) }
    pub fn empty() -> Self { Rlp::Bytes(vec![]) }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        if let Rlp::Bytes(b) = self { Some(b) } else { None }
    }
    pub fn as_list(&self) -> Option<&[Rlp]> {
        if let Rlp::List(l) = self { Some(l) } else { None }
    }
    pub fn as_u64(&self) -> Option<u64> {
        let b = self.as_bytes()?;
        if b.is_empty() { return Some(0); }
        if b.len() > 8  { return None; }
        let mut arr = [0u8; 8];
        arr[8 - b.len()..].copy_from_slice(b);
        Some(u64::from_be_bytes(arr))
    }
}

// ─── Encode ───────────────────────────────────────────────────────────────────

fn encode_uint(v: u64) -> Vec<u8> {
    if v == 0 { return vec![]; }
    let bytes = v.to_be_bytes();
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(7);
    bytes[start..].to_vec()
}

fn encode_len(prefix_short: u8, prefix_long: u8, len: usize) -> Vec<u8> {
    if len < 56 {
        vec![prefix_short + len as u8]
    } else {
        let len_bytes = encode_uint(len as u64);
        let mut out = vec![prefix_long + len_bytes.len() as u8];
        out.extend_from_slice(&len_bytes);
        out
    }
}

pub fn encode(val: &Rlp) -> Vec<u8> {
    match val {
        Rlp::Bytes(b) => {
            if b.len() == 1 && b[0] < 0x80 {
                b.clone()
            } else {
                let mut out = encode_len(0x80, 0xB7, b.len());
                out.extend_from_slice(b);
                out
            }
        }
        Rlp::List(items) => {
            let encoded: Vec<u8> = items.iter().flat_map(|i| encode(i)).collect();
            let mut out = encode_len(0xC0, 0xF7, encoded.len());
            out.extend_from_slice(&encoded);
            out
        }
    }
}

pub fn encode_list(items: &[Rlp]) -> Vec<u8> {
    encode(&Rlp::List(items.to_vec()))
}

// ─── Decode ───────────────────────────────────────────────────────────────────

pub fn decode(data: &[u8]) -> Result<(Rlp, usize), String> {
    if data.is_empty() { return Err("empty input".into()); }
    let first = data[0];
    if first < 0x80 {
        Ok((Rlp::Bytes(vec![first]), 1))
    } else if first <= 0xB7 {
        let len = (first - 0x80) as usize;
        if data.len() < 1 + len { return Err("short bytes".into()); }
        Ok((Rlp::Bytes(data[1..1 + len].to_vec()), 1 + len))
    } else if first <= 0xBF {
        let len_len = (first - 0xB7) as usize;
        if data.len() < 1 + len_len { return Err("short len".into()); }
        let len = be_bytes_to_usize(&data[1..1 + len_len]);
        let start = 1 + len_len;
        if data.len() < start + len { return Err("short bytes (long)".into()); }
        Ok((Rlp::Bytes(data[start..start + len].to_vec()), start + len))
    } else if first <= 0xF7 {
        let len = (first - 0xC0) as usize;
        if data.len() < 1 + len { return Err("short list".into()); }
        let items = decode_list(&data[1..1 + len])?;
        Ok((Rlp::List(items), 1 + len))
    } else {
        let len_len = (first - 0xF7) as usize;
        if data.len() < 1 + len_len { return Err("short list len".into()); }
        let len = be_bytes_to_usize(&data[1..1 + len_len]);
        let start = 1 + len_len;
        if data.len() < start + len { return Err("short list (long)".into()); }
        let items = decode_list(&data[start..start + len])?;
        Ok((Rlp::List(items), start + len))
    }
}

fn decode_list(data: &[u8]) -> Result<Vec<Rlp>, String> {
    let mut items = vec![];
    let mut pos = 0;
    while pos < data.len() {
        let (item, consumed) = decode(&data[pos..])?;
        items.push(item);
        pos += consumed;
    }
    Ok(items)
}

fn be_bytes_to_usize(b: &[u8]) -> usize {
    let mut v = 0usize;
    for &byte in b { v = (v << 8) | byte as usize; }
    v
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(v: Rlp) -> Rlp { decode(&encode(&v)).unwrap().0 }

    #[test]
    fn test_single_byte_below_0x80() {
        let v = Rlp::bytes(vec![0x7F]);
        assert_eq!(encode(&v), vec![0x7F]);
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn test_empty_bytes() {
        assert_eq!(encode(&Rlp::empty()), vec![0x80]);
        assert_eq!(rt(Rlp::empty()), Rlp::empty());
    }

    #[test]
    fn test_short_bytes() {
        let v = Rlp::bytes(vec![1, 2, 3]);
        let enc = encode(&v);
        assert_eq!(enc[0], 0x80 + 3);
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn test_long_bytes() {
        let data = vec![0u8; 56];
        let v = Rlp::bytes(data);
        let enc = encode(&v);
        assert_eq!(enc[0], 0xB7 + 1); // len_len=1
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn test_empty_list() {
        let v = Rlp::List(vec![]);
        assert_eq!(encode(&v), vec![0xC0]);
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn test_list_of_bytes() {
        let v = Rlp::List(vec![Rlp::uint(1), Rlp::uint(2)]);
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn test_nested_list() {
        let inner = Rlp::List(vec![Rlp::uint(42)]);
        let outer = Rlp::List(vec![inner, Rlp::empty()]);
        assert_eq!(rt(outer.clone()), outer);
    }

    #[test]
    fn test_uint_zero() {
        assert_eq!(Rlp::uint(0).as_bytes(), Some([].as_slice()));
    }

    #[test]
    fn test_uint_roundtrip() {
        for v in [1u64, 127, 128, 255, 256, 65535, u64::MAX / 2] {
            let r = Rlp::uint(v);
            let decoded = rt(r);
            assert_eq!(decoded.as_u64().unwrap(), v, "failed for {v}");
        }
    }

    #[test]
    fn test_decode_error_empty() {
        assert!(decode(&[]).is_err());
    }

    #[test]
    fn test_decode_error_truncated() {
        // 0x83 means 3 bytes follow, but we only provide 1
        assert!(decode(&[0x83, 0x01]).is_err());
    }

    #[test]
    fn test_encode_list_helper() {
        let items = vec![Rlp::uint(1), Rlp::uint(2), Rlp::uint(3)];
        let a = encode_list(&items);
        let b = encode(&Rlp::List(items));
        assert_eq!(a, b);
    }
}
