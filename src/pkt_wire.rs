#![allow(dead_code)]
//! v15.0 — PKT Wire Protocol
//!
//! Implement pktd P2P message format (dựa trên Bitcoin wire protocol).
//!
//! Message frame:
//!   [4 bytes magic] [12 bytes command] [4 bytes length LE] [4 bytes checksum] [payload]
//!
//! Checksum = SHA256(SHA256(payload))[0..4]
//! Integers = little-endian
//! VarInt   = Bitcoin compact integer encoding
//!
//! Tài liệu: https://en.bitcoin.it/wiki/Protocol_documentation
//! PKT fork: pktd (github.com/pkt-cash/pktd) — magic bytes PKT-specific

use sha2::{Digest, Sha256};

// ── Network magic bytes ───────────────────────────────────────────────────────

/// PKT testnet magic — PktTestNet = 0x070911fc (little-endian on wire)
/// Source: pkt-cash/pktd wire/protocol/protocol.go
pub const TESTNET_MAGIC: [u8; 4] = [0xfc, 0x11, 0x09, 0x07];

/// PKT mainnet magic — PktMainNet = 0x082f00fc (little-endian on wire)
/// Source: pkt-cash/pktd wire/protocol/protocol.go
pub const MAINNET_MAGIC: [u8; 4] = [0xfc, 0x00, 0x2f, 0x08];

// ── Protocol constants ────────────────────────────────────────────────────────

/// PKT protocol version — Source: pkt-cash/pktd wire/protocol/protocol.go
pub const PROTOCOL_VERSION: u32 = 70013;
pub const SERVICES_NODE:    u64 = 1;      // NODE_NETWORK
pub const COMMAND_LEN:      usize = 12;
pub const HEADER_LEN:       usize = 24;   // magic(4) + cmd(12) + len(4) + checksum(4)
pub const MAX_PAYLOAD:      usize = 32 * 1024 * 1024;  // 32 MiB limit

/// User-agent string gửi trong Version message.
pub const USER_AGENT: &str = "/blockchain-rust:16.3/";

// ── Inventory types ───────────────────────────────────────────────────────────

pub const INV_ERROR:           u32 = 0;
pub const INV_MSG_TX:          u32 = 1;
pub const INV_MSG_BLOCK:       u32 = 2;
pub const INV_MSG_FILTERED:    u32 = 3;
pub const INV_WITNESS_TX:      u32 = 0x4000_0001;
pub const INV_WITNESS_BLOCK:   u32 = 0x4000_0002;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    NotEnoughData { need: usize, have: usize },
    BadMagic([u8; 4]),
    PayloadTooLarge(usize),
    ChecksumMismatch { expected: [u8; 4], got: [u8; 4] },
    InvalidUtf8,
    UnexpectedEof,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotEnoughData { need, have } =>
                write!(f, "not enough data: need {} have {}", need, have),
            Self::BadMagic(m) =>
                write!(f, "bad magic: {:02x}{:02x}{:02x}{:02x}", m[0], m[1], m[2], m[3]),
            Self::PayloadTooLarge(n) =>
                write!(f, "payload too large: {} bytes", n),
            Self::ChecksumMismatch { expected, got } =>
                write!(f, "checksum mismatch: expected {:?} got {:?}", expected, got),
            Self::InvalidUtf8 => write!(f, "invalid utf-8"),
            Self::UnexpectedEof => write!(f, "unexpected EOF"),
        }
    }
}

// ── VarInt ────────────────────────────────────────────────────────────────────

/// Encode Bitcoin VarInt (compact integer).
pub fn encode_varint(n: u64) -> Vec<u8> {
    if n < 0xfd {
        vec![n as u8]
    } else if n <= 0xffff {
        let mut v = vec![0xfd];
        v.extend_from_slice(&(n as u16).to_le_bytes());
        v
    } else if n <= 0xffff_ffff {
        let mut v = vec![0xfe];
        v.extend_from_slice(&(n as u32).to_le_bytes());
        v
    } else {
        let mut v = vec![0xff];
        v.extend_from_slice(&n.to_le_bytes());
        v
    }
}

/// Decode Bitcoin VarInt; trả về (value, bytes_consumed).
pub fn decode_varint(data: &[u8]) -> Result<(u64, usize), WireError> {
    let b = *data.first().ok_or(WireError::UnexpectedEof)?;
    match b {
        0x00..=0xfc => Ok((b as u64, 1)),
        0xfd => {
            if data.len() < 3 { return Err(WireError::NotEnoughData { need: 3, have: data.len() }); }
            Ok((u16::from_le_bytes([data[1], data[2]]) as u64, 3))
        }
        0xfe => {
            if data.len() < 5 { return Err(WireError::NotEnoughData { need: 5, have: data.len() }); }
            Ok((u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as u64, 5))
        }
        0xff => {
            if data.len() < 9 { return Err(WireError::NotEnoughData { need: 9, have: data.len() }); }
            Ok((u64::from_le_bytes(data[1..9].try_into().unwrap()), 9))
        }
    }
}

// ── VarStr ────────────────────────────────────────────────────────────────────

/// Encode length-prefixed UTF-8 string (VarInt length + bytes).
pub fn encode_varstr(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut v = encode_varint(bytes.len() as u64);
    v.extend_from_slice(bytes);
    v
}

/// Decode VarStr; trả về (string, bytes_consumed).
pub fn decode_varstr(data: &[u8]) -> Result<(String, usize), WireError> {
    let (len, consumed) = decode_varint(data)?;
    let end = consumed + len as usize;
    if data.len() < end {
        return Err(WireError::NotEnoughData { need: end, have: data.len() });
    }
    let s = std::str::from_utf8(&data[consumed..end])
        .map_err(|_| WireError::InvalidUtf8)?
        .to_string();
    Ok((s, end))
}

// ── Checksum ──────────────────────────────────────────────────────────────────

/// SHA256(SHA256(data))[0..4] — Bitcoin/PKT message checksum.
pub fn checksum(payload: &[u8]) -> [u8; 4] {
    let first  = Sha256::digest(payload);
    let second = Sha256::digest(&first);
    [second[0], second[1], second[2], second[3]]
}

/// Checksum của empty payload (Verack).
pub const EMPTY_CHECKSUM: [u8; 4] = [0x5d, 0xf6, 0xe0, 0xe2];

// ── Command ───────────────────────────────────────────────────────────────────

/// Convert command string → 12-byte zero-padded array.
pub fn command_bytes(name: &str) -> [u8; COMMAND_LEN] {
    let mut out = [0u8; COMMAND_LEN];
    let b = name.as_bytes();
    let n = b.len().min(COMMAND_LEN);
    out[..n].copy_from_slice(&b[..n]);
    out
}

/// Parse 12-byte command → &str (trim trailing nulls).
pub fn command_name(cmd: &[u8; COMMAND_LEN]) -> &str {
    let end = cmd.iter().position(|&b| b == 0).unwrap_or(COMMAND_LEN);
    std::str::from_utf8(&cmd[..end]).unwrap_or("?")
}

// ── Message header ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MsgHeader {
    pub magic:    [u8; 4],
    pub command:  [u8; COMMAND_LEN],
    pub length:   u32,
    pub checksum: [u8; 4],
}

impl MsgHeader {
    pub fn command_str(&self) -> &str { command_name(&self.command) }
}

pub fn encode_header(magic: &[u8; 4], cmd: &str, payload: &[u8]) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[0..4].copy_from_slice(magic);
    let cb = command_bytes(cmd);
    h[4..16].copy_from_slice(&cb);
    let len = payload.len() as u32;
    h[16..20].copy_from_slice(&len.to_le_bytes());
    let cs = checksum(payload);
    h[20..24].copy_from_slice(&cs);
    h
}

pub fn decode_header(data: &[u8]) -> Result<MsgHeader, WireError> {
    if data.len() < HEADER_LEN {
        return Err(WireError::NotEnoughData { need: HEADER_LEN, have: data.len() });
    }
    let magic:    [u8; 4]          = data[0..4].try_into().unwrap();
    let command:  [u8; COMMAND_LEN] = data[4..16].try_into().unwrap();
    let length  = u32::from_le_bytes(data[16..20].try_into().unwrap());
    let checksum: [u8; 4]          = data[20..24].try_into().unwrap();
    Ok(MsgHeader { magic, command, length, checksum })
}

// ── Message types ─────────────────────────────────────────────────────────────

/// Version message (handshake step 1).
#[derive(Debug, Clone)]
pub struct VersionMsg {
    pub version:      u32,
    pub services:     u64,
    pub timestamp:    i64,
    pub nonce:        u64,
    pub user_agent:   String,
    pub start_height: i32,
    pub relay:        bool,
}

impl VersionMsg {
    pub fn new(start_height: i32) -> Self {
        VersionMsg {
            version:      PROTOCOL_VERSION,
            services:     SERVICES_NODE,
            timestamp:    chrono::Utc::now().timestamp(),
            nonce:        rand_nonce(),
            user_agent:   USER_AGENT.to_string(),
            start_height,
            relay:        true,
        }
    }
}

/// Inventory item (type + 32-byte hash).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvItem {
    pub inv_type: u32,
    pub hash:     [u8; 32],
}

impl InvItem {
    pub fn block(hash: [u8; 32]) -> Self { InvItem { inv_type: INV_MSG_BLOCK, hash } }
    pub fn tx(hash: [u8; 32])    -> Self { InvItem { inv_type: INV_MSG_TX,    hash } }

    pub fn type_name(&self) -> &'static str {
        match self.inv_type {
            INV_MSG_TX      => "tx",
            INV_MSG_BLOCK   => "block",
            INV_MSG_FILTERED => "filtered_block",
            _ => "unknown",
        }
    }
}

/// Block header (80 bytes on the wire).
#[derive(Debug, Clone)]
pub struct WireBlockHeader {
    pub version:     i32,
    pub prev_block:  [u8; 32],
    pub merkle_root: [u8; 32],
    pub timestamp:   u32,
    pub bits:        u32,
    pub nonce:       u32,
}

impl WireBlockHeader {
    /// Serialize to 80 bytes (Bitcoin wire format).
    pub fn to_bytes(&self) -> [u8; 80] {
        let mut buf = [0u8; 80];
        buf[0..4].copy_from_slice(&self.version.to_le_bytes());
        buf[4..36].copy_from_slice(&self.prev_block);
        buf[36..68].copy_from_slice(&self.merkle_root);
        buf[68..72].copy_from_slice(&self.timestamp.to_le_bytes());
        buf[72..76].copy_from_slice(&self.bits.to_le_bytes());
        buf[76..80].copy_from_slice(&self.nonce.to_le_bytes());
        buf
    }

    /// Parse from 80 bytes.
    pub fn from_bytes(b: &[u8]) -> Result<Self, WireError> {
        if b.len() < 80 {
            return Err(WireError::NotEnoughData { need: 80, have: b.len() });
        }
        Ok(WireBlockHeader {
            version:     i32::from_le_bytes(b[0..4].try_into().unwrap()),
            prev_block:  b[4..36].try_into().unwrap(),
            merkle_root: b[36..68].try_into().unwrap(),
            timestamp:   u32::from_le_bytes(b[68..72].try_into().unwrap()),
            bits:        u32::from_le_bytes(b[72..76].try_into().unwrap()),
            nonce:       u32::from_le_bytes(b[76..80].try_into().unwrap()),
        })
    }

    /// Double-SHA256 hash of this header (block hash).
    pub fn block_hash(&self) -> [u8; 32] {
        let bytes = self.to_bytes();
        let first  = Sha256::digest(&bytes);
        let second = Sha256::digest(&first);
        second.into()
    }
}

// ── PktMsg enum ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum PktMsg {
    Version(VersionMsg),
    Verack,
    Ping  { nonce: u64 },
    Pong  { nonce: u64 },
    Inv   { items: Vec<InvItem> },
    GetData { items: Vec<InvItem> },
    GetHeaders {
        version:        u32,
        locator_hashes: Vec<[u8; 32]>,
        hash_stop:      [u8; 32],
    },
    Headers { headers: Vec<WireBlockHeader> },
    Unknown { command: [u8; COMMAND_LEN], payload: Vec<u8> },
}

impl PktMsg {
    pub fn command_str(&self) -> &'static str {
        match self {
            Self::Version(_)    => "version",
            Self::Verack        => "verack",
            Self::Ping  { .. }  => "ping",
            Self::Pong  { .. }  => "pong",
            Self::Inv   { .. }  => "inv",
            Self::GetData { .. }=> "getdata",
            Self::GetHeaders { .. } => "getheaders",
            Self::Headers { .. }=> "headers",
            Self::Unknown { .. }=> "unknown",
        }
    }
}

// ── Encode ────────────────────────────────────────────────────────────────────

/// Encode PktMsg thành wire bytes (header + payload).
pub fn encode_message(msg: &PktMsg, magic: &[u8; 4]) -> Vec<u8> {
    let (cmd, payload) = encode_payload(msg);
    let header = encode_header(magic, cmd, &payload);
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(&payload);
    out
}

fn encode_payload(msg: &PktMsg) -> (&'static str, Vec<u8>) {
    match msg {
        PktMsg::Version(v) => ("version", encode_version_payload(v)),
        PktMsg::Verack      => ("verack",  vec![]),
        PktMsg::Ping { nonce } => ("ping", nonce.to_le_bytes().to_vec()),
        PktMsg::Pong { nonce } => ("pong", nonce.to_le_bytes().to_vec()),
        PktMsg::Inv { items }     => ("inv",     encode_inv_payload(items)),
        PktMsg::GetData { items } => ("getdata", encode_inv_payload(items)),
        PktMsg::GetHeaders { version, locator_hashes, hash_stop } =>
            ("getheaders", encode_getheaders_payload(*version, locator_hashes, hash_stop)),
        PktMsg::Headers { headers } => ("headers", encode_headers_payload(headers)),
        PktMsg::Unknown { command, payload } => {
            let name = command_name(command);
            // SAFETY: returning a &'static str is a lie here, but name from command bytes is fine
            // We leak the string to get 'static — acceptable for Unknown messages
            let s: &'static str = Box::leak(name.to_string().into_boxed_str());
            (s, payload.clone())
        }
    }
}

fn encode_version_payload(v: &VersionMsg) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&v.version.to_le_bytes());
    buf.extend_from_slice(&v.services.to_le_bytes());
    buf.extend_from_slice(&v.timestamp.to_le_bytes());
    // addr_recv (26 bytes: services 8 + ip 16 + port 2) — zeroed for now
    buf.extend_from_slice(&[0u8; 26]);
    // addr_from (26 bytes) — zeroed
    buf.extend_from_slice(&[0u8; 26]);
    buf.extend_from_slice(&v.nonce.to_le_bytes());
    buf.extend_from_slice(&encode_varstr(&v.user_agent));
    buf.extend_from_slice(&v.start_height.to_le_bytes());
    buf.push(if v.relay { 1 } else { 0 });
    buf
}

fn encode_inv_payload(items: &[InvItem]) -> Vec<u8> {
    let mut buf = encode_varint(items.len() as u64);
    for item in items {
        buf.extend_from_slice(&item.inv_type.to_le_bytes());
        buf.extend_from_slice(&item.hash);
    }
    buf
}

fn encode_getheaders_payload(
    version: u32,
    locators: &[[u8; 32]],
    hash_stop: &[u8; 32],
) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&version.to_le_bytes());
    buf.extend_from_slice(&encode_varint(locators.len() as u64));
    for h in locators { buf.extend_from_slice(h); }
    buf.extend_from_slice(hash_stop);
    buf
}

fn encode_headers_payload(headers: &[WireBlockHeader]) -> Vec<u8> {
    let mut buf = encode_varint(headers.len() as u64);
    for hdr in headers {
        buf.extend_from_slice(&hdr.to_bytes());
        buf.push(0x00);   // txn_count VarInt = 0
    }
    buf
}

// ── Decode ────────────────────────────────────────────────────────────────────

/// Decode một message từ raw bytes.
/// Trả về (PktMsg, bytes_consumed).
/// Không validate magic — caller phải tự kiểm tra nếu cần.
pub fn decode_message(data: &[u8]) -> Result<(PktMsg, usize), WireError> {
    let header = decode_header(data)?;

    let total = HEADER_LEN + header.length as usize;
    if data.len() < total {
        return Err(WireError::NotEnoughData { need: total, have: data.len() });
    }
    if header.length as usize > MAX_PAYLOAD {
        return Err(WireError::PayloadTooLarge(header.length as usize));
    }

    let payload = &data[HEADER_LEN..total];

    // Verify checksum
    let cs = checksum(payload);
    if cs != header.checksum {
        return Err(WireError::ChecksumMismatch { expected: header.checksum, got: cs });
    }

    let cmd = header.command_str();
    let msg = match cmd {
        "version"    => decode_version(payload)?,
        "verack"     => PktMsg::Verack,
        "ping"       => decode_ping_pong(payload, true)?,
        "pong"       => decode_ping_pong(payload, false)?,
        "inv"        => decode_inv(payload, false)?,
        "getdata"    => decode_inv(payload, true)?,
        "getheaders" => decode_getheaders(payload)?,
        "headers"    => decode_headers(payload)?,
        _            => PktMsg::Unknown {
            command: header.command,
            payload: payload.to_vec(),
        },
    };

    Ok((msg, total))
}

fn decode_version(data: &[u8]) -> Result<PktMsg, WireError> {
    if data.len() < 4 { return Err(WireError::NotEnoughData { need: 4, have: data.len() }); }
    let version  = u32::from_le_bytes(data[0..4].try_into().unwrap());
    let services = if data.len() >= 12 { u64::from_le_bytes(data[4..12].try_into().unwrap()) } else { 0 };
    let timestamp = if data.len() >= 20 { i64::from_le_bytes(data[12..20].try_into().unwrap()) } else { 0 };
    // Skip addr_recv (26) + addr_from (26) = 52 bytes
    let mut off = 20 + 52;
    let nonce = if data.len() >= off + 8 {
        let n = u64::from_le_bytes(data[off..off+8].try_into().unwrap());
        off += 8;
        n
    } else { 0 };
    let (user_agent, ua_len) = if data.len() > off {
        decode_varstr(&data[off..])
            .unwrap_or_else(|_| (String::new(), 0))
    } else { (String::new(), 0) };
    off += ua_len;
    let start_height = if data.len() >= off + 4 {
        i32::from_le_bytes(data[off..off+4].try_into().unwrap())
    } else { 0 };
    off += 4;
    let relay = data.get(off).copied().unwrap_or(1) != 0;

    Ok(PktMsg::Version(VersionMsg { version, services, timestamp, nonce, user_agent, start_height, relay }))
}

fn decode_ping_pong(data: &[u8], is_ping: bool) -> Result<PktMsg, WireError> {
    if data.len() < 8 { return Err(WireError::NotEnoughData { need: 8, have: data.len() }); }
    let nonce = u64::from_le_bytes(data[0..8].try_into().unwrap());
    Ok(if is_ping { PktMsg::Ping { nonce } } else { PktMsg::Pong { nonce } })
}

fn decode_inv(data: &[u8], is_getdata: bool) -> Result<PktMsg, WireError> {
    let (count, mut off) = decode_varint(data)?;
    let mut items = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if data.len() < off + 36 {
            return Err(WireError::NotEnoughData { need: off + 36, have: data.len() });
        }
        let inv_type = u32::from_le_bytes(data[off..off+4].try_into().unwrap());
        let hash: [u8; 32] = data[off+4..off+36].try_into().unwrap();
        items.push(InvItem { inv_type, hash });
        off += 36;
    }
    Ok(if is_getdata { PktMsg::GetData { items } } else { PktMsg::Inv { items } })
}

fn decode_getheaders(data: &[u8]) -> Result<PktMsg, WireError> {
    if data.len() < 4 { return Err(WireError::NotEnoughData { need: 4, have: data.len() }); }
    let version = u32::from_le_bytes(data[0..4].try_into().unwrap());
    let (count, mut off) = decode_varint(&data[4..])?;
    off += 4;
    let mut locator_hashes = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if data.len() < off + 32 {
            return Err(WireError::NotEnoughData { need: off + 32, have: data.len() });
        }
        locator_hashes.push(data[off..off+32].try_into().unwrap());
        off += 32;
    }
    let hash_stop = if data.len() >= off + 32 {
        data[off..off+32].try_into().unwrap()
    } else { [0u8; 32] };
    Ok(PktMsg::GetHeaders { version, locator_hashes, hash_stop })
}

fn decode_headers(data: &[u8]) -> Result<PktMsg, WireError> {
    let (count, mut off) = decode_varint(data)?;
    let mut headers = Vec::with_capacity(count as usize);
    for _ in 0..count {
        // 80 bytes header + 1 byte txn_count (always 0)
        if data.len() < off + 81 {
            return Err(WireError::NotEnoughData { need: off + 81, have: data.len() });
        }
        headers.push(WireBlockHeader::from_bytes(&data[off..off+80])?);
        off += 81; // skip txn_count byte
    }
    Ok(PktMsg::Headers { headers })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn rand_nonce() -> u64 {
    // Simple pseudo-random nonce from system time + stack address
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0) as u64;
    t ^ ((&t as *const u64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

/// Build GetHeaders message để request blocks từ `locator` trở đi.
pub fn get_headers_msg(locator_hashes: Vec<[u8; 32]>) -> PktMsg {
    PktMsg::GetHeaders {
        version: PROTOCOL_VERSION,
        locator_hashes,
        hash_stop: [0u8; 32],   // all zeros = get as many as possible
    }
}

/// Build GetData message để request 1 block.
pub fn get_block_msg(hash: [u8; 32]) -> PktMsg {
    PktMsg::GetData { items: vec![InvItem::block(hash)] }
}

/// Kiểm tra magic bytes có khớp testnet không.
pub fn is_testnet(magic: &[u8; 4]) -> bool { *magic == TESTNET_MAGIC }

/// Kiểm tra magic bytes có khớp mainnet không.
pub fn is_mainnet(magic: &[u8; 4]) -> bool { *magic == MAINNET_MAGIC }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Magic bytes ───────────────────────────────────────────────────────────

    #[test]
    fn testnet_magic_correct() {
        assert_eq!(TESTNET_MAGIC, [0xfc, 0x11, 0x09, 0x07]); // PktTestNet LE
    }

    #[test]
    fn mainnet_magic_correct() {
        assert_eq!(MAINNET_MAGIC, [0xfc, 0x00, 0x2f, 0x08]); // PktMainNet LE
    }

    #[test]
    fn is_testnet_true() {
        assert!(is_testnet(&TESTNET_MAGIC));
        assert!(!is_testnet(&MAINNET_MAGIC));
    }

    #[test]
    fn is_mainnet_true() {
        assert!(is_mainnet(&MAINNET_MAGIC));
        assert!(!is_mainnet(&TESTNET_MAGIC));
    }

    // ── VarInt ────────────────────────────────────────────────────────────────

    #[test]
    fn varint_encode_small() {
        assert_eq!(encode_varint(0),    vec![0x00]);
        assert_eq!(encode_varint(1),    vec![0x01]);
        assert_eq!(encode_varint(0xfc), vec![0xfc]);
    }

    #[test]
    fn varint_encode_fd() {
        let v = encode_varint(0x100);
        assert_eq!(v[0], 0xfd);
        assert_eq!(v.len(), 3);
        assert_eq!(u16::from_le_bytes([v[1], v[2]]), 0x100);
    }

    #[test]
    fn varint_encode_fe() {
        let v = encode_varint(0x1_0000);
        assert_eq!(v[0], 0xfe);
        assert_eq!(v.len(), 5);
    }

    #[test]
    fn varint_encode_ff() {
        let v = encode_varint(0x1_0000_0000);
        assert_eq!(v[0], 0xff);
        assert_eq!(v.len(), 9);
    }

    #[test]
    fn varint_roundtrip_small() {
        for n in [0u64, 1, 100, 0xfc] {
            let enc = encode_varint(n);
            let (dec, _) = decode_varint(&enc).unwrap();
            assert_eq!(dec, n);
        }
    }

    #[test]
    fn varint_roundtrip_fd() {
        let n = 300u64;
        let enc = encode_varint(n);
        let (dec, consumed) = decode_varint(&enc).unwrap();
        assert_eq!(dec, n);
        assert_eq!(consumed, 3);
    }

    #[test]
    fn varint_roundtrip_fe() {
        let n = 70_000u64;
        let enc = encode_varint(n);
        let (dec, consumed) = decode_varint(&enc).unwrap();
        assert_eq!(dec, n);
        assert_eq!(consumed, 5);
    }

    #[test]
    fn varint_roundtrip_ff() {
        let n = 5_000_000_000u64;
        let enc = encode_varint(n);
        let (dec, consumed) = decode_varint(&enc).unwrap();
        assert_eq!(dec, n);
        assert_eq!(consumed, 9);
    }

    #[test]
    fn varint_decode_empty_err() {
        assert!(decode_varint(&[]).is_err());
    }

    // ── VarStr ────────────────────────────────────────────────────────────────

    #[test]
    fn varstr_roundtrip_empty() {
        let enc = encode_varstr("");
        let (s, _) = decode_varstr(&enc).unwrap();
        assert_eq!(s, "");
    }

    #[test]
    fn varstr_roundtrip_ascii() {
        let enc = encode_varstr("/pkt:1.0/");
        let (s, consumed) = decode_varstr(&enc).unwrap();
        assert_eq!(s, "/pkt:1.0/");
        assert_eq!(consumed, 1 + 9); // varint(9) + 9 bytes
    }

    #[test]
    fn varstr_roundtrip_user_agent() {
        let enc = encode_varstr(USER_AGENT);
        let (s, _) = decode_varstr(&enc).unwrap();
        assert_eq!(s, USER_AGENT);
    }

    // ── Checksum ──────────────────────────────────────────────────────────────

    #[test]
    fn checksum_empty_matches_constant() {
        // SHA256(SHA256(""))[0..4] = well-known value
        assert_eq!(checksum(&[]), EMPTY_CHECKSUM);
    }

    #[test]
    fn checksum_different_payloads_differ() {
        let a = checksum(b"hello");
        let b = checksum(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn checksum_deterministic() {
        let a = checksum(b"pkt test");
        let b = checksum(b"pkt test");
        assert_eq!(a, b);
    }

    #[test]
    fn checksum_four_bytes() {
        let cs = checksum(b"test");
        assert_eq!(cs.len(), 4);
    }

    // ── command_bytes / command_name ──────────────────────────────────────────

    #[test]
    fn command_bytes_short_name() {
        let b = command_bytes("ping");
        assert_eq!(&b[..4], b"ping");
        assert!(b[4..].iter().all(|&x| x == 0), "remaining bytes should be null");
    }

    #[test]
    fn command_bytes_exactly_12() {
        let b = command_bytes("getheaders");
        assert_eq!(b.len(), 12);
    }

    #[test]
    fn command_name_roundtrip() {
        for name in ["version", "verack", "ping", "pong", "inv", "getdata", "getheaders", "headers"] {
            let b = command_bytes(name);
            assert_eq!(command_name(&b), name);
        }
    }

    #[test]
    fn command_name_null_padded() {
        let mut b = [0u8; 12];
        b[..4].copy_from_slice(b"ping");
        assert_eq!(command_name(&b), "ping");
    }

    // ── Header encode/decode ──────────────────────────────────────────────────

    #[test]
    fn header_len_is_24() {
        assert_eq!(HEADER_LEN, 24);
    }

    #[test]
    fn encode_decode_header_verack() {
        let h = encode_header(&TESTNET_MAGIC, "verack", &[]);
        assert_eq!(h.len(), HEADER_LEN);
        let hdr = decode_header(&h).unwrap();
        assert_eq!(hdr.magic,    TESTNET_MAGIC);
        assert_eq!(hdr.command_str(), "verack");
        assert_eq!(hdr.length,   0);
        assert_eq!(hdr.checksum, EMPTY_CHECKSUM);
    }

    #[test]
    fn decode_header_too_short_err() {
        assert!(decode_header(&[0u8; 10]).is_err());
    }

    // ── encode_message / decode_message ───────────────────────────────────────

    #[test]
    fn roundtrip_verack() {
        let bytes = encode_message(&PktMsg::Verack, &TESTNET_MAGIC);
        let (msg, consumed) = decode_message(&bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert!(matches!(msg, PktMsg::Verack));
    }

    #[test]
    fn roundtrip_ping() {
        let nonce = 0xdeadbeef_cafebabe_u64;
        let bytes = encode_message(&PktMsg::Ping { nonce }, &TESTNET_MAGIC);
        let (msg, _) = decode_message(&bytes).unwrap();
        assert!(matches!(msg, PktMsg::Ping { nonce: n } if n == nonce));
    }

    #[test]
    fn roundtrip_pong() {
        let nonce = 12345678u64;
        let bytes = encode_message(&PktMsg::Pong { nonce }, &TESTNET_MAGIC);
        let (msg, _) = decode_message(&bytes).unwrap();
        assert!(matches!(msg, PktMsg::Pong { nonce: n } if n == nonce));
    }

    #[test]
    fn roundtrip_inv_single_block() {
        let hash = [0xab; 32];
        let items = vec![InvItem::block(hash)];
        let bytes = encode_message(&PktMsg::Inv { items: items.clone() }, &TESTNET_MAGIC);
        let (msg, _) = decode_message(&bytes).unwrap();
        if let PktMsg::Inv { items: decoded } = msg {
            assert_eq!(decoded.len(), 1);
            assert_eq!(decoded[0].inv_type, INV_MSG_BLOCK);
            assert_eq!(decoded[0].hash, hash);
        } else {
            panic!("expected Inv");
        }
    }

    #[test]
    fn roundtrip_inv_empty() {
        let bytes = encode_message(&PktMsg::Inv { items: vec![] }, &TESTNET_MAGIC);
        let (msg, _) = decode_message(&bytes).unwrap();
        if let PktMsg::Inv { items } = msg {
            assert!(items.is_empty());
        } else { panic!(); }
    }

    #[test]
    fn roundtrip_getdata() {
        let hash = [0xcd; 32];
        let items = vec![InvItem::tx(hash)];
        let bytes = encode_message(&PktMsg::GetData { items }, &TESTNET_MAGIC);
        let (msg, _) = decode_message(&bytes).unwrap();
        assert!(matches!(msg, PktMsg::GetData { .. }));
    }

    #[test]
    fn roundtrip_getheaders() {
        let locators = vec![[0x01u8; 32], [0x02u8; 32]];
        let msg = get_headers_msg(locators.clone());
        let bytes = encode_message(&msg, &TESTNET_MAGIC);
        let (decoded, _) = decode_message(&bytes).unwrap();
        if let PktMsg::GetHeaders { version, locator_hashes, hash_stop } = decoded {
            assert_eq!(version, PROTOCOL_VERSION);
            assert_eq!(locator_hashes, locators);
            assert_eq!(hash_stop, [0u8; 32]);
        } else { panic!(); }
    }

    #[test]
    fn roundtrip_headers_single() {
        let hdr = WireBlockHeader {
            version:     1,
            prev_block:  [0u8; 32],
            merkle_root: [1u8; 32],
            timestamp:   1_700_000_000,
            bits:        0x1d00ffff,
            nonce:       12345,
        };
        let bytes = encode_message(&PktMsg::Headers { headers: vec![hdr.clone()] }, &TESTNET_MAGIC);
        let (msg, _) = decode_message(&bytes).unwrap();
        if let PktMsg::Headers { headers } = msg {
            assert_eq!(headers.len(), 1);
            assert_eq!(headers[0].nonce,     hdr.nonce);
            assert_eq!(headers[0].timestamp, hdr.timestamp);
            assert_eq!(headers[0].bits,      hdr.bits);
        } else { panic!(); }
    }

    #[test]
    fn roundtrip_version() {
        let v   = VersionMsg::new(100);
        let bytes = encode_message(&PktMsg::Version(v.clone()), &TESTNET_MAGIC);
        let (msg, _) = decode_message(&bytes).unwrap();
        if let PktMsg::Version(decoded) = msg {
            assert_eq!(decoded.version,      v.version);
            assert_eq!(decoded.start_height, v.start_height);
            assert_eq!(decoded.user_agent,   v.user_agent);
        } else { panic!(); }
    }

    // ── Bad magic ─────────────────────────────────────────────────────────────

    #[test]
    fn decode_message_bad_checksum() {
        let mut bytes = encode_message(&PktMsg::Verack, &TESTNET_MAGIC);
        bytes[22] ^= 0xff;   // corrupt checksum byte
        let err = decode_message(&bytes);
        assert!(matches!(err, Err(WireError::ChecksumMismatch { .. })));
    }

    #[test]
    fn decode_message_too_short() {
        let err = decode_message(&[0u8; 10]);
        assert!(matches!(err, Err(WireError::NotEnoughData { .. })));
    }

    // ── WireBlockHeader ───────────────────────────────────────────────────────

    #[test]
    fn block_header_to_from_bytes_roundtrip() {
        let hdr = WireBlockHeader {
            version:     2,
            prev_block:  [0xaau8; 32],
            merkle_root: [0xbbu8; 32],
            timestamp:   1_704_067_200,
            bits:        0x1a00ffff,
            nonce:       99999,
        };
        let bytes = hdr.to_bytes();
        assert_eq!(bytes.len(), 80);
        let decoded = WireBlockHeader::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.version,     hdr.version);
        assert_eq!(decoded.prev_block,  hdr.prev_block);
        assert_eq!(decoded.merkle_root, hdr.merkle_root);
        assert_eq!(decoded.timestamp,   hdr.timestamp);
        assert_eq!(decoded.bits,        hdr.bits);
        assert_eq!(decoded.nonce,       hdr.nonce);
    }

    #[test]
    fn block_header_hash_is_32_bytes() {
        let hdr = WireBlockHeader {
            version: 1, prev_block: [0u8; 32], merkle_root: [0u8; 32],
            timestamp: 0, bits: 0, nonce: 0,
        };
        let h = hdr.block_hash();
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn block_header_from_bytes_too_short() {
        assert!(WireBlockHeader::from_bytes(&[0u8; 20]).is_err());
    }

    // ── InvItem ───────────────────────────────────────────────────────────────

    #[test]
    fn inv_item_block_type() {
        let item = InvItem::block([0u8; 32]);
        assert_eq!(item.inv_type, INV_MSG_BLOCK);
        assert_eq!(item.type_name(), "block");
    }

    #[test]
    fn inv_item_tx_type() {
        let item = InvItem::tx([0u8; 32]);
        assert_eq!(item.inv_type, INV_MSG_TX);
        assert_eq!(item.type_name(), "tx");
    }

    // ── PktMsg ────────────────────────────────────────────────────────────────

    #[test]
    fn pkt_msg_command_str() {
        assert_eq!(PktMsg::Verack.command_str(), "verack");
        assert_eq!(PktMsg::Ping { nonce: 0 }.command_str(), "ping");
        assert_eq!(PktMsg::Pong { nonce: 0 }.command_str(), "pong");
        assert_eq!((PktMsg::Inv { items: vec![] }).command_str(), "inv");
        assert_eq!((PktMsg::GetData { items: vec![] }).command_str(), "getdata");
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    #[test]
    fn get_headers_msg_has_zero_hash_stop() {
        let msg = get_headers_msg(vec![[1u8; 32]]);
        if let PktMsg::GetHeaders { hash_stop, .. } = msg {
            assert_eq!(hash_stop, [0u8; 32]);
        } else { panic!(); }
    }

    #[test]
    fn get_block_msg_is_getdata() {
        let hash = [0xddu8; 32];
        let msg = get_block_msg(hash);
        if let PktMsg::GetData { items } = msg {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].inv_type, INV_MSG_BLOCK);
            assert_eq!(items[0].hash, hash);
        } else { panic!(); }
    }

    #[test]
    fn version_msg_new_uses_protocol_version() {
        let v = VersionMsg::new(0);
        assert_eq!(v.version, PROTOCOL_VERSION);
        assert_eq!(v.user_agent, USER_AGENT);
    }
}
