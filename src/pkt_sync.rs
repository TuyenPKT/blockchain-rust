#![allow(dead_code)]
//! v15.2 — Block Download
//!
//! Pipeline: GetHeaders → Headers → GetData → Block
//!
//! Tách thành hai pha:
//!   1. Header sync: GetHeaders → nhận Headers batch (2000/msg) → validate chain → lưu raw 80-byte headers
//!   2. Block sync: GetData → nhận Block messages → validate → lưu vào RocksDB
//!
//! Wire header storage (tách biệt với local chain):
//!   wireheader:{height:016x}  → 80 bytes raw header
//!   meta:sync_height          → u64 (số lớn nhất đã lưu)
//!   meta:sync_tip_hash        → [u8;32] hash của header ở sync_height
//!
//! Dùng pkt_wire (v15.0) + pkt_peer (v15.1) cho I/O

use std::path::{Path, PathBuf};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use rocksdb::{DB, Options};

use crate::pkt_wire::{
    self, PktMsg, WireBlockHeader, InvItem, TESTNET_MAGIC, MAINNET_MAGIC,
};
use crate::pkt_peer::{send_msg, recv_msg, PeerError};

// ── Constants ─────────────────────────────────────────────────────────────────

pub const HEADERS_PER_MSG:   usize = 2000;  // Bitcoin protocol max
pub const BLOCKS_PER_BATCH:  usize = 16;    // GetData batch size
pub const MAX_SYNC_HEADERS:  u64   = 10_000; // safety cap per session
pub const HEADER_SYNC_TIMEOUT_SECS: u64 = 30;

// ── SyncConfig ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub magic:           [u8; 4],
    pub network:         String,
    pub db_path:         PathBuf,
    pub max_headers:     u64,        // 0 = unlimited
    pub skip_pow_check:  bool,       // for tests/regtest
    pub recv_timeout_secs: u64,
    pub batch_size:      usize,      // headers per GetHeaders request
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            magic:             TESTNET_MAGIC,
            network:           "testnet".to_string(),
            db_path:           default_sync_db_path(),
            max_headers:       0,
            skip_pow_check:    false,
            recv_timeout_secs: HEADER_SYNC_TIMEOUT_SECS,
            batch_size:        HEADERS_PER_MSG,
        }
    }
}

impl SyncConfig {
    pub fn testnet() -> Self { Self::default() }

    pub fn mainnet() -> Self {
        Self {
            magic:   MAINNET_MAGIC,
            network: "mainnet".to_string(),
            ..Self::default()
        }
    }

    /// Config cho tests (regtest): bỏ qua PoW, dùng temp dir.
    pub fn regtest(db_path: PathBuf) -> Self {
        Self {
            magic:            TESTNET_MAGIC,
            network:          "regtest".to_string(),
            db_path,
            skip_pow_check:   true,
            recv_timeout_secs: 5,
            ..Self::default()
        }
    }
}

fn default_sync_db_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".pkt").join("syncdb")
}

// ── SyncState ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncPhase {
    Idle,
    SyncingHeaders,
    SyncingBlocks,
    UpToDate,
    Failed(String),
}

impl SyncPhase {
    pub fn is_done(&self) -> bool {
        matches!(self, Self::UpToDate | Self::Failed(_))
    }
}

#[derive(Debug, Clone)]
pub struct SyncState {
    pub phase:            SyncPhase,
    pub headers_downloaded: u64,
    pub blocks_downloaded:  u64,
    pub local_height:     u64,
    pub peer_height:      i32,
    pub last_hash:        [u8; 32], // hash of highest known header
}

impl SyncState {
    pub fn new(local_height: u64, peer_height: i32) -> Self {
        Self {
            phase:              SyncPhase::Idle,
            headers_downloaded: 0,
            blocks_downloaded:  0,
            local_height,
            peer_height,
            last_hash:          [0u8; 32],
        }
    }

    pub fn progress_pct(&self) -> f64 {
        if self.peer_height <= 0 { return 100.0; }
        let downloaded = self.headers_downloaded + self.local_height;
        (downloaded as f64 / self.peer_height as f64 * 100.0).min(100.0)
    }
}

// ── SyncError ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum SyncError {
    Peer(PeerError),
    Db(String),
    InvalidHeader(String),
    InvalidChain(String),
    PoWFailed { height: u64, hash: [u8; 32] },
    Timeout,
    UnexpectedMsg(String),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Peer(e)               => write!(f, "peer: {}", e),
            Self::Db(s)                 => write!(f, "db: {}", s),
            Self::InvalidHeader(s)      => write!(f, "invalid header: {}", s),
            Self::InvalidChain(s)       => write!(f, "chain broken: {}", s),
            Self::PoWFailed { height, .. } => write!(f, "PoW failed at height {}", height),
            Self::Timeout               => write!(f, "timeout"),
            Self::UnexpectedMsg(s)      => write!(f, "unexpected message: {}", s),
        }
    }
}

impl From<PeerError> for SyncError {
    fn from(e: PeerError) -> Self { Self::Peer(e) }
}

// ── Compact target (Bitcoin nBits) ────────────────────────────────────────────

/// Decode Bitcoin compact target (nBits) to 32-byte big-endian target.
///
/// nBits layout: [exponent: 1 byte][mantissa: 3 bytes] (big-endian u32)
/// target = mantissa * 256^(exponent - 3)
pub fn compact_target_to_bytes(bits: u32) -> [u8; 32] {
    let mut target = [0u8; 32];
    let exponent   = (bits >> 24) as usize;
    let mantissa   = (bits & 0x00ff_ffff) as u64;

    if exponent == 0 || exponent > 34 { return target; } // out of range

    // Place mantissa bytes at position (32 - exponent)..
    // mantissa is 3 bytes, so we write bytes at [32-exponent .. 32-exponent+3]
    let pos = 32usize.saturating_sub(exponent);
    let m_bytes = [(mantissa >> 16) as u8, (mantissa >> 8) as u8, mantissa as u8];

    for (i, &b) in m_bytes.iter().enumerate() {
        let idx = pos + i;
        if idx < 32 {
            target[idx] = b;
        }
    }
    target
}

/// Compare hash against compact target. Hash must be ≤ target.
/// Both are 32-byte big-endian values (as returned from block_hash()).
/// Bitcoin block hashes are stored little-endian on disk but compared big-endian here.
pub fn hash_meets_target(hash: &[u8; 32], target: &[u8; 32]) -> bool {
    // Compare big-endian: reverse hash (Bitcoin displays hash reversed)
    let mut h = *hash;
    h.reverse();
    for (h_byte, t_byte) in h.iter().zip(target.iter()) {
        match h_byte.cmp(t_byte) {
            std::cmp::Ordering::Less    => return true,
            std::cmp::Ordering::Greater => return false,
            std::cmp::Ordering::Equal   => {}
        }
    }
    true // equal → also meets target
}

/// Convenience: validate a single header's PoW.
pub fn validate_header_pow(header: &WireBlockHeader) -> bool {
    let hash   = header.block_hash();
    let target = compact_target_to_bytes(header.bits);
    hash_meets_target(&hash, &target)
}

// ── Header chain validation ────────────────────────────────────────────────────

/// Validate that a batch of headers forms a valid chain:
///   headers[i].prev_block == block_hash(headers[i-1])
///
/// `prev_hash` is the hash of the block BEFORE this batch (tip of what we already know).
pub fn validate_chain_links(
    headers:   &[WireBlockHeader],
    prev_hash: &[u8; 32],
) -> Result<(), SyncError> {
    if headers.is_empty() { return Ok(()); }
    let mut expected_prev = *prev_hash;
    for (i, h) in headers.iter().enumerate() {
        if h.prev_block != expected_prev {
            return Err(SyncError::InvalidChain(format!(
                "header[{}] prev_block mismatch: expected {}, got {}",
                i,
                hex::encode(expected_prev),
                hex::encode(h.prev_block),
            )));
        }
        expected_prev = h.block_hash();
    }
    Ok(())
}

/// Full validation: chain links + PoW (unless skip_pow_check).
pub fn validate_header_batch(
    headers:        &[WireBlockHeader],
    prev_hash:      &[u8; 32],
    skip_pow_check: bool,
    start_height:   u64,
) -> Result<(), SyncError> {
    validate_chain_links(headers, prev_hash)?;
    if !skip_pow_check {
        for (i, h) in headers.iter().enumerate() {
            if !validate_header_pow(h) {
                let hash = h.block_hash();
                return Err(SyncError::PoWFailed {
                    height: start_height + i as u64,
                    hash,
                });
            }
        }
    }
    Ok(())
}

// ── Block locator ─────────────────────────────────────────────────────────────

/// Build a Bitcoin-style block locator: exponentially spaced hashes from tip backwards.
///
/// Spacing: last 10 hashes (step=1), then step doubles each time.
/// `known_hashes` is ordered from genesis to tip (index 0 = genesis).
pub fn build_locator(known_hashes: &[[u8; 32]]) -> Vec<[u8; 32]> {
    if known_hashes.is_empty() { return vec![[0u8; 32]]; } // genesis hash

    let n      = known_hashes.len();
    let mut locs = Vec::new();
    let mut idx  = n as i64 - 1;
    let mut step = 1i64;

    while idx >= 0 {
        locs.push(known_hashes[idx as usize]);
        if locs.len() >= 10 {
            step *= 2;
        }
        idx -= step;
    }

    // Always include genesis
    if locs.last() != Some(&known_hashes[0]) {
        locs.push(known_hashes[0]);
    }
    locs
}

// ── SyncDb ────────────────────────────────────────────────────────────────────

/// RocksDB wrapper for downloaded wire headers and sync state.
pub struct SyncDb {
    db:   DB,
    path: PathBuf,
}

const KEY_SYNC_HEIGHT:   &[u8] = b"meta:sync_height";
const KEY_SYNC_TIP_HASH: &[u8] = b"meta:sync_tip_hash";

fn wireheader_key(height: u64) -> String {
    format!("wireheader:{:016x}", height)
}

impl SyncDb {
    /// Open (or create) at the given path.
    pub fn open(path: &Path) -> Result<Self, SyncError> {
        std::fs::create_dir_all(path)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        Ok(SyncDb { db, path: path.to_path_buf() })
    }

    /// Open read-only — không giữ write lock, dùng cho pktscan khi sync đang chạy.
    pub fn open_read_only(path: &Path) -> Result<Self, SyncError> {
        std::fs::create_dir_all(path)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        let opts = Options::default();
        let db = DB::open_for_read_only(&opts, path, false)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        Ok(SyncDb { db, path: path.to_path_buf() })
    }

    /// Open in a temp dir (test helper).
    pub fn open_temp() -> Result<Self, SyncError> {
        let path = std::env::temp_dir()
            .join(format!("pkt_syncdb_test_{}", rand_u64()));
        Self::open(&path)
    }

    /// Save one raw 80-byte header at `height`.
    pub fn save_header(&self, height: u64, raw: &[u8; 80]) -> Result<(), SyncError> {
        let key = wireheader_key(height);
        self.db.put(key.as_bytes(), raw.as_ref())
            .map_err(|e| SyncError::Db(e.to_string()))
    }

    /// Load raw 80-byte header at `height`.
    pub fn load_header(&self, height: u64) -> Result<Option<[u8; 80]>, SyncError> {
        let key = wireheader_key(height);
        match self.db.get(key.as_bytes()).map_err(|e| SyncError::Db(e.to_string()))? {
            None => Ok(None),
            Some(v) if v.len() == 80 => {
                let mut raw = [0u8; 80];
                raw.copy_from_slice(&v);
                Ok(Some(raw))
            }
            Some(v) => Err(SyncError::Db(format!("bad header length {} at {}", v.len(), height))),
        }
    }

    /// Get the last synced height (None if no headers saved yet).
    pub fn get_sync_height(&self) -> Result<Option<u64>, SyncError> {
        match self.db.get(KEY_SYNC_HEIGHT).map_err(|e| SyncError::Db(e.to_string()))? {
            None    => Ok(None),
            Some(v) => {
                let s = std::str::from_utf8(&v).map_err(|e| SyncError::Db(e.to_string()))?;
                let h = s.parse::<u64>().map_err(|e| SyncError::Db(e.to_string()))?;
                Ok(Some(h))
            }
        }
    }

    /// Persist the last synced height.
    pub fn set_sync_height(&self, height: u64) -> Result<(), SyncError> {
        self.db.put(KEY_SYNC_HEIGHT, height.to_string().as_bytes())
            .map_err(|e| SyncError::Db(e.to_string()))
    }

    /// Save the hash of the highest known header (used as prev for next batch).
    pub fn get_tip_hash(&self) -> Result<Option<[u8; 32]>, SyncError> {
        match self.db.get(KEY_SYNC_TIP_HASH).map_err(|e| SyncError::Db(e.to_string()))? {
            None => Ok(None),
            Some(v) if v.len() == 32 => {
                let mut h = [0u8; 32];
                h.copy_from_slice(&v);
                Ok(Some(h))
            }
            _ => Ok(None),
        }
    }

    pub fn set_tip_hash(&self, hash: &[u8; 32]) -> Result<(), SyncError> {
        self.db.put(KEY_SYNC_TIP_HASH, hash.as_ref())
            .map_err(|e| SyncError::Db(e.to_string()))
    }

    /// Return the SHA256d wire hash of the header at `height` (= block_hash peer uses).
    pub fn get_header_hash(&self, height: u64) -> Result<Option<[u8; 32]>, SyncError> {
        match self.load_header(height)? {
            None => Ok(None),
            Some(raw) => {
                let h = crate::pkt_wire::WireBlockHeader::block_hash_of_bytes(&raw);
                Ok(Some(h))
            }
        }
    }

    /// Count headers stored (iterates wireheader: keys).
    pub fn count_headers(&self) -> Result<u64, SyncError> {
        use rocksdb::IteratorMode;
        let mut count = 0u64;
        for item in self.db.iterator(IteratorMode::Start) {
            let (k, _) = item.map_err(|e| SyncError::Db(e.to_string()))?;
            if k.starts_with(b"wireheader:") { count += 1; }
        }
        Ok(count)
    }

    pub fn path(&self) -> &Path { &self.path }
}

impl Drop for SyncDb {
    fn drop(&mut self) {
        // Auto-cleanup temp dirs in tests
        let path_str = self.path.to_string_lossy();
        if path_str.contains("pkt_syncdb_test_") {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn rand_u64() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    h.finish()
}

// ── Wire protocol helpers ─────────────────────────────────────────────────────

/// Send GetHeaders to ask for headers after `locator_hashes`.
pub fn send_getheaders(
    stream:         &mut TcpStream,
    magic:          [u8; 4],
    locator_hashes: Vec<[u8; 32]>,
) -> Result<(), SyncError> {
    let msg = PktMsg::GetHeaders {
        version:        pkt_wire::PROTOCOL_VERSION,
        locator_hashes,
        hash_stop:      [0u8; 32],
    };
    send_msg(stream, msg, magic).map_err(SyncError::from)
}

/// Send GetData requesting specific blocks by hash.
pub fn send_getdata_blocks(
    stream: &mut TcpStream,
    magic:  [u8; 4],
    hashes: &[[u8; 32]],
) -> Result<(), SyncError> {
    let items: Vec<InvItem> = hashes.iter().map(|h| InvItem::block(*h)).collect();
    let msg = PktMsg::GetData { items };
    send_msg(stream, msg, magic).map_err(SyncError::from)
}

/// Read messages until we get a Headers message (skip pings/pongs/inv).
/// Returns empty Vec if peer responds with 0 headers (up-to-date).
pub fn recv_headers(
    stream:       &mut TcpStream,
    magic:        [u8; 4],
    timeout_secs: u64,
) -> Result<Vec<WireBlockHeader>, SyncError> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if Instant::now() > deadline {
            return Err(SyncError::Timeout);
        }
        let msg = recv_msg(stream, magic)?;
        match msg {
            PktMsg::Headers { headers } => return Ok(headers),
            PktMsg::Ping { nonce } => {
                send_msg(stream, PktMsg::Pong { nonce }, magic)?;
            }
            PktMsg::Inv { .. } => {} // ignore unsolicited invs during header sync
            other => return Err(SyncError::UnexpectedMsg(other.command_str().to_string())),
        }
    }
}

// ── Header sync session ───────────────────────────────────────────────────────

/// Result of a header sync session.
#[derive(Debug)]
pub struct HeaderSyncResult {
    pub headers_saved: u64,
    pub final_height:  u64,
    pub tip_hash:      [u8; 32],
    pub elapsed_ms:    u128,
}

/// Download headers from a connected peer and store them.
///
/// Starts from `start_height` (exclusive — we already have this header).
/// Loops sending GetHeaders → receiving Headers → validating → saving until
/// peer returns 0 headers (up-to-date) or max_headers reached.
pub fn sync_headers(
    stream:       &mut TcpStream,
    db:           &SyncDb,
    cfg:          &SyncConfig,
    start_height: u64,
    prev_hash:    [u8; 32],
) -> Result<HeaderSyncResult, SyncError> {
    let t0             = Instant::now();
    let mut height     = start_height;
    let mut tip_hash   = prev_hash;
    let mut total      = 0u64;

    loop {
        // Build locator from tip hash
        let locators = vec![tip_hash, [0u8; 32]]; // simple: just our tip
        send_getheaders(stream, cfg.magic, locators)?;

        let headers = recv_headers(stream, cfg.magic, cfg.recv_timeout_secs)?;
        if headers.is_empty() {
            break; // peer says we're up to date
        }

        // Validate batch
        validate_header_batch(&headers, &tip_hash, cfg.skip_pow_check, height + 1)?;

        // Save each header
        for h in &headers {
            height += 1;
            let raw = h.to_bytes();
            db.save_header(height, &raw)?;
            tip_hash = h.block_hash();
            total   += 1;

            if cfg.max_headers > 0 && total >= cfg.max_headers {
                db.set_sync_height(height)?;
                db.set_tip_hash(&tip_hash)?;
                return Ok(HeaderSyncResult {
                    headers_saved: total,
                    final_height:  height,
                    tip_hash,
                    elapsed_ms:    t0.elapsed().as_millis(),
                });
            }
        }

        // Persist progress after each batch
        db.set_sync_height(height)?;
        db.set_tip_hash(&tip_hash)?;

        // If peer sent fewer than max per message, we're caught up
        if headers.len() < cfg.batch_size {
            break;
        }
    }

    Ok(HeaderSyncResult {
        headers_saved: total,
        final_height:  height,
        tip_hash,
        elapsed_ms:    t0.elapsed().as_millis(),
    })
}

// ── Status formatting ─────────────────────────────────────────────────────────

pub fn format_sync_status(state: &SyncState) -> String {
    format!(
        "phase={:?} headers={} blocks={} local_h={} peer_h={} progress={:.1}%",
        state.phase,
        state.headers_downloaded,
        state.blocks_downloaded,
        state.local_height,
        state.peer_height,
        state.progress_pct(),
    )
}

pub fn format_header_result(r: &HeaderSyncResult) -> String {
    format!(
        "saved {} headers up to height {} in {} ms (tip={})",
        r.headers_saved,
        r.final_height,
        r.elapsed_ms,
        hex::encode(&r.tip_hash[..8]),
    )
}

// ── CLI ───────────────────────────────────────────────────────────────────────

pub fn parse_sync_args(args: &[String]) -> SyncConfig {
    let mut cfg = SyncConfig::default();
    let mut i   = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mainnet" => { cfg = SyncConfig::mainnet(); }
            "--max" | "-n" if i + 1 < args.len() => {
                i += 1;
                if let Ok(n) = args[i].parse() { cfg.max_headers = n; }
            }
            "--timeout" | "-t" if i + 1 < args.len() => {
                i += 1;
                if let Ok(n) = args[i].parse() { cfg.recv_timeout_secs = n; }
            }
            "--skip-pow" => { cfg.skip_pow_check = true; }
            _ => {}
        }
        i += 1;
    }
    cfg
}

pub fn cmd_sync(args: &[String]) {
    if args.first().map(|s| s.as_str()) == Some("--help") {
        println!();
        println!("  cargo run -- sync [host:port] [options]");
        println!();
        println!("  Options:");
        println!("    --mainnet          sync mainnet (mặc định: testnet)");
        println!("    --max N            tối đa N headers (0 = không giới hạn)");
        println!("    --timeout S        recv timeout (giây, mặc định: 30)");
        println!("    --skip-pow         bỏ qua kiểm tra PoW (regtest/debug)");
        println!();
        return;
    }

    let mut cfg = parse_sync_args(args);

    // Tìm host:port từ args (bare arg không bắt đầu bằng --)
    let peer_addr = args.iter()
        .find(|a| !a.starts_with('-') && a.contains(':'))
        .cloned()
        .unwrap_or_else(|| "seed.testnet.oceif.com:8333".to_string());

    // Chain mình dùng BLAKE3 PoW — tự động skip PoW khi kết nối node mình.
    // User có thể override bằng cách KHÔNG pass --skip-pow (hiện không có flag ngược lại),
    // nhưng kết nối seed.* của mình thì luôn skip để tránh PoWFailed.
    if peer_addr.contains("oceif.com") || peer_addr.starts_with("127.") || peer_addr.starts_with("localhost") {
        cfg.skip_pow_check = true;
    }

    println!("[sync] kết nối tới {} ({}) …", peer_addr, cfg.network);

    // TCP connect + handshake
    let peer_cfg = crate::pkt_peer::PeerConfig {
        host:  peer_addr.split(':').next().unwrap_or("seed.testnet.oceif.com").to_string(),
        port:  peer_addr.split(':').nth(1).and_then(|p| p.parse().ok()).unwrap_or(8333),
        magic: cfg.magic,
        ..crate::pkt_peer::PeerConfig::default()
    };

    let mut stream = match std::net::TcpStream::connect_timeout(
        &format!("{}:{}", peer_cfg.host, peer_cfg.port)
            .parse::<std::net::SocketAddr>()
            .or_else(|_| {
                use std::net::ToSocketAddrs;
                format!("{}:{}", peer_cfg.host, peer_cfg.port)
                    .to_socket_addrs()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                    .and_then(|mut i| i.next().ok_or_else(||
                        std::io::Error::new(std::io::ErrorKind::Other, "no addr")))
            })
            .unwrap_or_else(|_| {
                eprintln!("[sync] không resolve được {}", peer_addr);
                std::process::exit(1);
            }),
        std::time::Duration::from_secs(10),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[sync] connect thất bại: {}", e);
            std::process::exit(1);
        }
    };

    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(cfg.recv_timeout_secs)));

    let info = match crate::pkt_peer::do_handshake(&mut stream, &peer_cfg) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[sync] handshake thất bại: {}", e);
            std::process::exit(1);
        }
    };

    println!("[sync] ✅  connected: agent=\"{}\" height={}", info.user_agent, info.start_height);

    // Gửi GetAddr để discover thêm peers, lưu vào ~/.pkt/peers.txt
    {
        let peers_path = pkt_wire::default_peers_path();
        if send_msg(&mut stream, PktMsg::GetAddr, cfg.magic).is_ok() {
            let prev_timeout = stream.read_timeout().ok().flatten();
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            if let Ok(PktMsg::Addr { peers }) = recv_msg(&mut stream, cfg.magic) {
                match pkt_wire::save_peers(&peers_path, &peers) {
                    Ok(()) => println!("[sync] 📡 discovered {} peers → {}", peers.len(), peers_path.display()),
                    Err(e) => eprintln!("[sync] save_peers thất bại: {}", e),
                }
            }
            if let Some(t) = prev_timeout {
                let _ = stream.set_read_timeout(Some(t));
            } else {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(cfg.recv_timeout_secs)));
            }
        }
    }

    // Mở SyncDb, lấy start point
    let db = match SyncDb::open(&cfg.db_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[sync] mở DB thất bại: {}", e);
            std::process::exit(1);
        }
    };

    // Mở UtxoDb, BlockDb, AddrIndexDb và MempoolDb để apply transactions
    let utxo_path    = crate::pkt_testnet_web::default_utxo_db_path();
    let block_path   = crate::pkt_testnet_web::home_path(".pkt/blockdb");
    let addr_path    = crate::pkt_addr_index::default_addr_db_path();
    let mempool_path = crate::pkt_mempool_sync::default_mempool_db_path();

    let utxo_db = match crate::pkt_utxo_sync::UtxoSyncDb::open(&utxo_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[sync] mở utxodb thất bại: {}", e);
            std::process::exit(1);
        }
    };
    let block_db = match crate::pkt_block_sync::BlockSyncDb::open(&block_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[sync] mở blockdb thất bại: {}", e);
            std::process::exit(1);
        }
    };
    let addr_db = match crate::pkt_addr_index::AddrIndexDb::open(&addr_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[sync] mở addrdb thất bại: {}", e);
            std::process::exit(1);
        }
    };
    let mempool_db = match crate::pkt_mempool_sync::MempoolDb::open(&mempool_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[sync] mở mempooldb thất bại: {}", e);
            std::process::exit(1);
        }
    };
    let reorg_path = crate::pkt_reorg::default_reorg_db_path();
    let reorg_db = match crate::pkt_reorg::ReorgDb::open(&reorg_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[sync] mở reorgdb thất bại: {}", e);
            std::process::exit(1);
        }
    };

    let start_height: u64  = db.get_sync_height().ok().flatten().unwrap_or(0);
    let tip_hash: [u8; 32] = db.get_tip_hash().ok().flatten().unwrap_or([0u8; 32]);

    println!("[sync] resume từ height={} tip={}", start_height, hex::encode(&tip_hash[..8]));

    // Poll loop: sync headers → apply blocks → đợi block mới.
    let poll_secs = 60u64;
    let mut chain_reset = false;
    loop {
        let cur_height = db.get_sync_height().ok().flatten().unwrap_or(0);
        let cur_tip    = db.get_tip_hash().ok().flatten().unwrap_or([0u8; 32]);

        // Phase 1: download new headers
        match sync_headers(&mut stream, &db, &cfg, cur_height, cur_tip) {
            Ok(r) => {
                if r.headers_saved > 0 {
                    println!("[sync] ✅  {}", format_header_result(&r));
                }
            }
            Err(SyncError::InvalidChain(ref msg)) if msg.contains("prev_block mismatch") => {
                // Node restarted with a fresh chain → our stored tip is stale.
                eprintln!("[sync] node chain changed — sẽ reset DBs và sync lại từ genesis");
                chain_reset = true;
                break;
            }
            Err(e) => {
                eprintln!("[sync] header lỗi: {:?} — reconnect sau {}s", e, poll_secs);
                break;
            }
        }

        // Phase 2: detect reorg trước khi apply blocks
        let utxo_h = utxo_db.get_utxo_height().ok().flatten().unwrap_or(0);
        if utxo_h > 0 {
            match reorg_db.detect_reorg(&db, utxo_h) {
                Ok(true) => {
                    eprintln!("[reorg] phát hiện reorg tại height={}, tìm common ancestor...", utxo_h);
                    match reorg_db.find_common_ancestor(&db, utxo_h) {
                        Ok(Some(ancestor)) => {
                            match reorg_db.rollback_to(ancestor, utxo_h, &utxo_db, &addr_db) {
                                Ok(n) => eprintln!("[reorg] ✅  rollback {} blocks → ancestor={}", n, ancestor),
                                Err(e) => {
                                    eprintln!("[reorg] rollback thất bại: {:?} → full reset", e);
                                    chain_reset = true;
                                    break;
                                }
                            }
                        }
                        Ok(None) | Err(_) => {
                            eprintln!("[reorg] không tìm được common ancestor (reorg sâu > {} blocks) → full reset",
                                crate::pkt_reorg::MAX_LOOKBACK);
                            chain_reset = true;
                            break;
                        }
                    }
                }
                Ok(false) => {} // no reorg
                Err(e) => eprintln!("[reorg] detect lỗi: {:?} — bỏ qua", e),
            }
        }

        // Phase 3: apply blocks → UTXOs + address index (streaming, RAM ≈ size(tx))
        match crate::pkt_block_sync::sync_blocks(
            &mut stream, &db, &utxo_db, &block_db, Some(&addr_db), Some(&reorg_db),
            Some(&mempool_db), &cfg.magic, cfg.skip_pow_check,
        ) {
            Ok(r) if r.blocks_applied > 0 => {
                println!(
                    "[block-sync] ✅  applied {} blocks, utxo_height={}  ({} ms)",
                    r.blocks_applied, r.final_height, r.elapsed_ms,
                );
            }
            Ok(_) => {} // đã đủ, không có block mới
            Err(e) => {
                eprintln!("[block-sync] lỗi: {:?} — tiếp tục poll", e);
            }
        }

        // Phase 4: sync mempool (best-effort — errors are non-fatal)
        match crate::pkt_mempool_sync::sync_mempool(
            &mut stream, cfg.magic, &utxo_db, &mempool_db,
        ) {
            Ok(n) if n > 0 => {
                let count = mempool_db.count().unwrap_or(0);
                println!("[mempool] +{} txs  (total pending: {})", n, count);
            }
            Ok(_)  => {} // no new txs
            Err(e) => eprintln!("[mempool] sync lỗi: {:?}", e),
        }
        // Restore normal recv timeout after mempool ops
        let _ = stream.set_read_timeout(Some(Duration::from_secs(cfg.recv_timeout_secs)));

        std::thread::sleep(Duration::from_secs(poll_secs));
    }

    // Reset DBs khi phát hiện node chain thay đổi.
    // Drop DB handles trước khi xóa directory (giải phóng RocksDB LOCK file).
    if chain_reset {
        let _ = db.set_sync_height(0);
        let _ = db.set_tip_hash(&[0u8; 32]);
        drop(utxo_db);
        drop(block_db);
        drop(addr_db);
        drop(reorg_db);
        drop(mempool_db);
        if utxo_path.exists()    { let _ = std::fs::remove_dir_all(&utxo_path); }
        if block_path.exists()   { let _ = std::fs::remove_dir_all(&block_path); }
        if addr_path.exists()    { let _ = std::fs::remove_dir_all(&addr_path); }
        if reorg_path.exists()   { let _ = std::fs::remove_dir_all(&reorg_path); }
        if mempool_path.exists() { let _ = std::fs::remove_dir_all(&mempool_path); }
        eprintln!("[sync] DBs đã reset — systemd sẽ restart để sync từ genesis");
        std::process::exit(0); // systemd restarts → fresh start
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    // ── Test helpers ─────────────────────────────────────────────────────────

    /// Build a chain of N synthetic headers (no real PoW — skip_pow_check=true).
    /// Each header's prev_block = hash of previous.
    fn make_header_chain(n: usize, genesis_hash: [u8; 32]) -> Vec<WireBlockHeader> {
        let mut headers = Vec::with_capacity(n);
        let mut prev    = genesis_hash;
        for i in 0..n {
            let h = WireBlockHeader {
                version:     1,
                prev_block:  prev,
                merkle_root: [i as u8; 32],
                timestamp:   1_600_000_000 + i as u32,
                bits:        0x207f_ffff, // very easy target (skip_pow=true anyway)
                nonce:       i as u32,
            };
            prev = h.block_hash();
            headers.push(h);
        }
        headers
    }

    // ── SyncConfig tests ──────────────────────────────────────────────────────

    #[test]
    fn test_sync_config_default_network() {
        assert_eq!(SyncConfig::default().network, "testnet");
    }

    #[test]
    fn test_sync_config_default_magic() {
        assert_eq!(SyncConfig::default().magic, TESTNET_MAGIC);
    }

    #[test]
    fn test_sync_config_mainnet_magic() {
        assert_eq!(SyncConfig::mainnet().magic, MAINNET_MAGIC);
    }

    #[test]
    fn test_sync_config_mainnet_network() {
        assert_eq!(SyncConfig::mainnet().network, "mainnet");
    }

    #[test]
    fn test_sync_config_regtest_skip_pow() {
        let cfg = SyncConfig::regtest(std::env::temp_dir());
        assert!(cfg.skip_pow_check);
    }

    // ── SyncState / SyncPhase tests ───────────────────────────────────────────

    #[test]
    fn test_sync_phase_idle_not_done() {
        assert!(!SyncPhase::Idle.is_done());
    }

    #[test]
    fn test_sync_phase_up_to_date_done() {
        assert!(SyncPhase::UpToDate.is_done());
    }

    #[test]
    fn test_sync_phase_failed_done() {
        assert!(SyncPhase::Failed("err".to_string()).is_done());
    }

    #[test]
    fn test_sync_phase_syncing_headers_not_done() {
        assert!(!SyncPhase::SyncingHeaders.is_done());
    }

    #[test]
    fn test_sync_state_progress_zero_peer() {
        let s = SyncState::new(0, 0);
        assert_eq!(s.progress_pct(), 100.0);
    }

    #[test]
    fn test_sync_state_progress_half() {
        let mut s = SyncState::new(0, 100);
        s.headers_downloaded = 50;
        assert!((s.progress_pct() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_sync_state_progress_capped_100() {
        let mut s = SyncState::new(0, 10);
        s.headers_downloaded = 999;
        assert_eq!(s.progress_pct(), 100.0);
    }

    // ── compact_target_to_bytes tests ─────────────────────────────────────────

    #[test]
    fn test_compact_target_genesis_bitcoin() {
        // Bitcoin genesis bits = 0x1d00ffff
        // target = 0x00000000FFFF0000...0000 (29 bytes of zeros then ffff...)
        let target = compact_target_to_bytes(0x1d00ffff);
        // byte at index 4 (32-29=3, then +0,+1,+2 → 3,4,5)
        // exponent=29, pos = 32-29 = 3
        // m_bytes = [0x00, 0xff, 0xff]
        assert_eq!(target[3], 0x00);
        assert_eq!(target[4], 0xff);
        assert_eq!(target[5], 0xff);
        // rest after byte 5 should be 0
        for &b in &target[6..] { assert_eq!(b, 0); }
    }

    #[test]
    fn test_compact_target_easy() {
        // bits = 0x207fffff → very easy (used in regtest)
        // exponent=32, pos=0, m_bytes=[0x7f,0xff,0xff]
        let target = compact_target_to_bytes(0x207fffff);
        assert_eq!(target[0], 0x7f);
        assert_eq!(target[1], 0xff);
        assert_eq!(target[2], 0xff);
    }

    #[test]
    fn test_compact_target_zero_exponent() {
        let target = compact_target_to_bytes(0x0000ffff);
        assert_eq!(target, [0u8; 32]); // exponent=0 → zero target
    }

    #[test]
    fn test_compact_target_all_zeros_for_exponent_35() {
        // exponent > 34 → zero
        let target = compact_target_to_bytes(0x23000001);
        assert_eq!(target, [0u8; 32]);
    }

    // ── hash_meets_target tests ───────────────────────────────────────────────

    #[test]
    fn test_hash_meets_target_all_zeros_hash() {
        // hash=all-zeros always meets any target
        let hash   = [0u8; 32];
        let target = [0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00,
                      0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                      0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                      0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00u8];
        assert!(hash_meets_target(&hash, &target));
    }

    #[test]
    fn test_hash_meets_target_all_ff_hash_fails() {
        let hash   = [0xffu8; 32];
        let target = [0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
                      0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                      0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                      0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00u8];
        // reversed hash = all ff, target starts with 0x00,0x00 → ff > 00 → fail
        assert!(!hash_meets_target(&hash, &target));
    }

    #[test]
    fn test_hash_meets_target_equal() {
        let val    = [0x12u8; 32];
        // hash equal to target → should pass
        assert!(hash_meets_target(&val, &val));
    }

    #[test]
    fn test_hash_meets_target_easy_regtest() {
        // 0x207fffff target → almost any hash passes
        let target = compact_target_to_bytes(0x207fffff);
        let hash   = [0u8; 32]; // all-zeros trivially meets any target
        assert!(hash_meets_target(&hash, &target));
    }

    // ── validate_chain_links tests ────────────────────────────────────────────

    #[test]
    fn test_validate_chain_links_empty() {
        let result = validate_chain_links(&[], &[0u8; 32]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_chain_links_valid_one() {
        let genesis_hash = [0u8; 32];
        let headers      = make_header_chain(1, genesis_hash);
        assert!(validate_chain_links(&headers, &genesis_hash).is_ok());
    }

    #[test]
    fn test_validate_chain_links_valid_ten() {
        let genesis_hash = [0xabu8; 32];
        let headers      = make_header_chain(10, genesis_hash);
        assert!(validate_chain_links(&headers, &genesis_hash).is_ok());
    }

    #[test]
    fn test_validate_chain_links_broken_first() {
        let genesis_hash = [0u8; 32];
        let wrong_hash   = [0xffu8; 32]; // wrong prev
        let headers      = make_header_chain(3, genesis_hash);
        let result       = validate_chain_links(&headers, &wrong_hash);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("chain broken") || err_str.contains("mismatch"));
    }

    #[test]
    fn test_validate_chain_links_broken_in_middle() {
        let genesis_hash = [0u8; 32];
        let mut headers  = make_header_chain(5, genesis_hash);
        // Corrupt header[2].prev_block
        headers[2].prev_block = [0xddu8; 32];
        let result = validate_chain_links(&headers, &genesis_hash);
        assert!(result.is_err());
    }

    // ── build_locator tests ───────────────────────────────────────────────────

    #[test]
    fn test_build_locator_empty() {
        let locator = build_locator(&[]);
        assert_eq!(locator.len(), 1);
        assert_eq!(locator[0], [0u8; 32]); // genesis placeholder
    }

    #[test]
    fn test_build_locator_single() {
        let hash    = [0x01u8; 32];
        let locator = build_locator(&[hash]);
        assert!(locator.contains(&hash));
    }

    #[test]
    fn test_build_locator_includes_tip() {
        let hashes: Vec<[u8; 32]> = (0..20).map(|i| [i as u8; 32]).collect();
        let locator = build_locator(&hashes);
        // Tip is always first
        assert_eq!(locator[0], hashes[19]);
    }

    #[test]
    fn test_build_locator_includes_genesis() {
        let hashes: Vec<[u8; 32]> = (0..20).map(|i| [i as u8; 32]).collect();
        let locator = build_locator(&hashes);
        assert!(locator.contains(&hashes[0]));
    }

    #[test]
    fn test_build_locator_no_duplicates_small() {
        let hashes: Vec<[u8; 32]> = (0..5).map(|i| [i as u8; 32]).collect();
        let locator = build_locator(&hashes);
        // For small chains, all entries should be distinct
        let unique: std::collections::HashSet<_> = locator.iter().collect();
        assert_eq!(unique.len(), locator.len());
    }

    #[test]
    fn test_build_locator_density_near_tip() {
        let hashes: Vec<[u8; 32]> = (0..20u8).map(|i| [i; 32]).collect();
        let locator = build_locator(&hashes);
        // Should contain at least the last few hashes (dense near tip)
        assert!(locator.contains(&hashes[19]));
        assert!(locator.contains(&hashes[18]));
        assert!(locator.contains(&hashes[17]));
    }

    // ── SyncDb tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_syncdb_open_temp() {
        let db = SyncDb::open_temp();
        assert!(db.is_ok());
    }

    #[test]
    fn test_syncdb_save_and_load_header() {
        let db  = SyncDb::open_temp().unwrap();
        let raw = [0x42u8; 80];
        db.save_header(100, &raw).unwrap();
        let loaded = db.load_header(100).unwrap();
        assert_eq!(loaded, Some(raw));
    }

    #[test]
    fn test_syncdb_load_missing_header() {
        let db = SyncDb::open_temp().unwrap();
        assert_eq!(db.load_header(999).unwrap(), None);
    }

    #[test]
    fn test_syncdb_set_get_sync_height() {
        let db = SyncDb::open_temp().unwrap();
        assert_eq!(db.get_sync_height().unwrap(), None);
        db.set_sync_height(500).unwrap();
        assert_eq!(db.get_sync_height().unwrap(), Some(500));
    }

    #[test]
    fn test_syncdb_sync_height_overwrite() {
        let db = SyncDb::open_temp().unwrap();
        db.set_sync_height(100).unwrap();
        db.set_sync_height(200).unwrap();
        assert_eq!(db.get_sync_height().unwrap(), Some(200));
    }

    #[test]
    fn test_syncdb_set_get_tip_hash() {
        let db   = SyncDb::open_temp().unwrap();
        let hash = [0xabu8; 32];
        db.set_tip_hash(&hash).unwrap();
        assert_eq!(db.get_tip_hash().unwrap(), Some(hash));
    }

    #[test]
    fn test_syncdb_count_headers_zero() {
        let db = SyncDb::open_temp().unwrap();
        assert_eq!(db.count_headers().unwrap(), 0);
    }

    #[test]
    fn test_syncdb_count_headers_after_saves() {
        let db = SyncDb::open_temp().unwrap();
        for i in 0..5u64 {
            db.save_header(i + 1, &[i as u8; 80]).unwrap();
        }
        assert_eq!(db.count_headers().unwrap(), 5);
    }

    #[test]
    fn test_syncdb_save_multiple_heights() {
        let db = SyncDb::open_temp().unwrap();
        for h in [1u64, 100, 500, 999] {
            let raw: [u8; 80] = [h as u8; 80];
            db.save_header(h, &raw).unwrap();
        }
        assert_eq!(db.load_header(100).unwrap(), Some([100u8; 80]));
        assert_eq!(db.load_header(999).unwrap(), Some([231u8; 80])); // 999 % 256 = 231
    }

    // ── WireBlockHeader roundtrip via SyncDb ─────────────────────────────────

    #[test]
    fn test_wireheader_roundtrip_via_db() {
        let db  = SyncDb::open_temp().unwrap();
        let hdr = WireBlockHeader {
            version:     2,
            prev_block:  [0x11u8; 32],
            merkle_root: [0x22u8; 32],
            timestamp:   1_700_000_000,
            bits:        0x1d00ffff,
            nonce:       42,
        };
        let raw = hdr.to_bytes();
        db.save_header(10, &raw).unwrap();

        let loaded_raw = db.load_header(10).unwrap().unwrap();
        let loaded_hdr = WireBlockHeader::from_bytes(&loaded_raw).unwrap();

        assert_eq!(loaded_hdr.version,     hdr.version);
        assert_eq!(loaded_hdr.prev_block,  hdr.prev_block);
        assert_eq!(loaded_hdr.merkle_root, hdr.merkle_root);
        assert_eq!(loaded_hdr.timestamp,   hdr.timestamp);
        assert_eq!(loaded_hdr.bits,        hdr.bits);
        assert_eq!(loaded_hdr.nonce,       hdr.nonce);
    }

    // ── Loopback TCP: GetHeaders / Headers ────────────────────────────────────

    /// Spawn a server that:
    ///   1. Receives GetHeaders
    ///   2. Responds with `headers` (as Headers message)
    ///   3. On second GetHeaders → responds with empty Headers (caught up)
    fn spawn_headers_server(
        listener: TcpListener,
        headers:  Vec<WireBlockHeader>,
        magic:    [u8; 4],
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                s.set_read_timeout(Some(Duration::from_secs(5))).ok();
                // First GetHeaders → send headers
                if let Ok(PktMsg::GetHeaders { .. }) = recv_msg(&mut s, magic) {
                    let reply = PktMsg::Headers { headers };
                    let _ = send_msg(&mut s, reply, magic);
                }
                // Second GetHeaders (if any) → send empty Headers
                if let Ok(PktMsg::GetHeaders { .. }) = recv_msg(&mut s, magic) {
                    let _ = send_msg(&mut s, PktMsg::Headers { headers: vec![] }, magic);
                }
            }
        })
    }

    #[test]
    fn test_send_getheaders_loopback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let magic    = TESTNET_MAGIC;

        // Server just verifies it receives a GetHeaders
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).ok();
            recv_msg(&mut s, magic).unwrap()
        });

        let mut client = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();
        let result = send_getheaders(&mut client, magic, vec![[0u8; 32]]);

        let received = server.join().unwrap();
        assert!(result.is_ok());
        assert!(matches!(received, PktMsg::GetHeaders { .. }));
    }

    #[test]
    fn test_recv_headers_loopback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let magic    = TESTNET_MAGIC;
        let expected = make_header_chain(5, [0u8; 32]);
        let to_send  = expected.clone();

        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).ok();
            // Consume whatever client sends, then send headers
            let _ = recv_msg(&mut s, magic);
            let _ = send_msg(&mut s, PktMsg::Headers { headers: to_send }, magic);
        });

        let mut client = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();
        // Send any message to trigger server response
        let _ = send_msg(&mut client, PktMsg::Ping { nonce: 0 }, magic);
        let headers = recv_headers(&mut client, magic, 5).unwrap();

        server.join().ok();
        assert_eq!(headers.len(), 5);
        for (got, exp) in headers.iter().zip(expected.iter()) {
            assert_eq!(got.prev_block,  exp.prev_block);
            assert_eq!(got.merkle_root, exp.merkle_root);
            assert_eq!(got.nonce,       exp.nonce);
        }
    }

    #[test]
    fn test_sync_headers_loopback_saves_to_db() {
        let genesis_hash = [0u8; 32];
        let chain        = make_header_chain(5, genesis_hash);
        let to_send      = chain.clone();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_headers_server(listener, to_send, TESTNET_MAGIC);

        // Connect client
        let mut client = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let db  = SyncDb::open_temp().unwrap();
        let cfg = SyncConfig::regtest(db.path().to_path_buf());

        let result = sync_headers(&mut client, &db, &cfg, 0, genesis_hash);
        server.join().ok();

        assert!(result.is_ok(), "sync_headers failed: {:?}", result.err().map(|e| e.to_string()));
        let r = result.unwrap();
        assert_eq!(r.headers_saved, 5);
        assert_eq!(r.final_height, 5);
        assert_eq!(db.count_headers().unwrap(), 5);
    }

    #[test]
    fn test_sync_headers_persists_height() {
        let genesis_hash = [0u8; 32];
        let chain        = make_header_chain(3, genesis_hash);

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_headers_server(listener, chain, TESTNET_MAGIC);

        let mut client = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let db  = SyncDb::open_temp().unwrap();
        let cfg = SyncConfig::regtest(db.path().to_path_buf());
        let _   = sync_headers(&mut client, &db, &cfg, 0, genesis_hash);
        server.join().ok();

        assert_eq!(db.get_sync_height().unwrap(), Some(3));
    }

    #[test]
    fn test_sync_headers_tip_hash_correct() {
        let genesis_hash = [0u8; 32];
        let chain        = make_header_chain(3, genesis_hash);
        let expected_tip = chain.last().unwrap().block_hash();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_headers_server(listener, chain, TESTNET_MAGIC);

        let mut client = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let db  = SyncDb::open_temp().unwrap();
        let cfg = SyncConfig::regtest(db.path().to_path_buf());
        let r   = sync_headers(&mut client, &db, &cfg, 0, genesis_hash).unwrap();
        server.join().ok();

        assert_eq!(r.tip_hash, expected_tip);
    }

    #[test]
    fn test_sync_headers_chain_linkage_in_db() {
        let genesis_hash = [0u8; 32];
        let chain        = make_header_chain(4, genesis_hash);

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_headers_server(listener, chain.clone(), TESTNET_MAGIC);

        let mut client = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let db  = SyncDb::open_temp().unwrap();
        let cfg = SyncConfig::regtest(db.path().to_path_buf());
        sync_headers(&mut client, &db, &cfg, 0, genesis_hash).unwrap();
        server.join().ok();

        // Verify every stored header matches what was sent
        for (i, expected) in chain.iter().enumerate() {
            let raw    = db.load_header(i as u64 + 1).unwrap().unwrap();
            let stored = WireBlockHeader::from_bytes(&raw).unwrap();
            assert_eq!(stored.block_hash(), expected.block_hash(),
                "header {} hash mismatch", i + 1);
        }
    }

    // ── parse_sync_args tests ─────────────────────────────────────────────────

    #[test]
    fn test_parse_sync_args_empty() {
        let cfg = parse_sync_args(&[]);
        assert_eq!(cfg.network, "testnet");
        assert_eq!(cfg.max_headers, 0);
    }

    #[test]
    fn test_parse_sync_args_mainnet() {
        let cfg = parse_sync_args(&["--mainnet".to_string()]);
        assert_eq!(cfg.network, "mainnet");
    }

    #[test]
    fn test_parse_sync_args_max() {
        let cfg = parse_sync_args(&["--max".to_string(), "500".to_string()]);
        assert_eq!(cfg.max_headers, 500);
    }

    #[test]
    fn test_parse_sync_args_skip_pow() {
        let cfg = parse_sync_args(&["--skip-pow".to_string()]);
        assert!(cfg.skip_pow_check);
    }

    #[test]
    fn test_parse_sync_args_timeout() {
        let cfg = parse_sync_args(&["--timeout".to_string(), "60".to_string()]);
        assert_eq!(cfg.recv_timeout_secs, 60);
    }

    // ── format helpers tests ──────────────────────────────────────────────────

    #[test]
    fn test_format_sync_status_contains_phase() {
        let s = SyncState::new(10, 100);
        let f = format_sync_status(&s);
        assert!(f.contains("Idle"));
    }

    #[test]
    fn test_format_sync_status_contains_heights() {
        let s = SyncState::new(42, 1000);
        let f = format_sync_status(&s);
        assert!(f.contains("42"));
        assert!(f.contains("1000"));
    }

    #[test]
    fn test_format_header_result() {
        let r = HeaderSyncResult {
            headers_saved: 100,
            final_height:  100,
            tip_hash:      [0xabu8; 32],
            elapsed_ms:    500,
        };
        let f = format_header_result(&r);
        assert!(f.contains("100"));
        assert!(f.contains("500"));
    }

    // ── SyncError display ─────────────────────────────────────────────────────

    #[test]
    fn test_sync_error_display_db() {
        let e = SyncError::Db("write failed".to_string());
        assert!(e.to_string().contains("db"));
        assert!(e.to_string().contains("write failed"));
    }

    #[test]
    fn test_sync_error_display_invalid_chain() {
        let e = SyncError::InvalidChain("mismatch".to_string());
        assert!(e.to_string().contains("chain broken"));
    }

    #[test]
    fn test_sync_error_display_pow_failed() {
        let e = SyncError::PoWFailed { height: 42, hash: [0u8; 32] };
        assert!(e.to_string().contains("42"));
    }

    #[test]
    fn test_sync_error_display_timeout() {
        assert_eq!(SyncError::Timeout.to_string(), "timeout");
    }
}
