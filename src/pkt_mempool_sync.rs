#![allow(dead_code)]
//! v17.2 — Mempool Realtime
//!
//! Syncs pending transactions from the peer's mempool via Bitcoin wire protocol:
//!   1. Send `mempool` message → peer responds with `inv` (tx hashes)
//!   2. Filter out txids we already have
//!   3. Send `getdata` for new txids (up to MAX_MEMPOOL_FETCH)
//!   4. Receive `tx` messages → parse → calc fee rate → store in RocksDB
//!
//! RocksDB key schema (`~/.pkt/mempooldb`):
//!   tx:{txid_hex}                       → raw tx bytes (wire format)
//!   fee:{u64::MAX-rate_msat_vb:020}:{txid_hex} → "" (scan = highest fee first)
//!   ts:{txid_hex}                       → 16 LE bytes [ts_ns u64][fee_rate u64]
//!
//! Fee rate = (sum_inputs − sum_outputs) × 1000 / tx_size  [msat/vB = sat*1000/byte]
//! For coinbase or unresolvable inputs, fee_rate = 0.

use std::io::Cursor;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rocksdb::{Direction, IteratorMode, Options, DB};
use serde::Serialize;

use crate::pkt_block_sync::read_tx_s;
use crate::pkt_peer::{recv_msg, send_msg, PeerError};
use crate::pkt_sync::SyncError;
use crate::pkt_utxo_sync::{wire_txid, UtxoSyncDb, WireTx};
use crate::pkt_wire::{command_bytes, command_name, InvItem, PktMsg, INV_MSG_TX};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Max txids to request per mempool sync session.
pub const MAX_MEMPOOL_FETCH: usize = 256;
/// Timeout waiting for inv response after sending `mempool` message.
pub const MEMPOOL_INV_TIMEOUT_SECS: u64 = 5;
/// Timeout waiting for all `tx` messages after sending getdata.
pub const MEMPOOL_TX_TIMEOUT_SECS: u64 = 15;

// ── MempoolTxInfo ─────────────────────────────────────────────────────────────

/// Returned by `get_pending()` — serializable for API responses.
#[derive(Debug, Clone, Serialize)]
pub struct MempoolTxInfo {
    pub txid:             String,
    pub size:             u64,
    pub fee_rate_msat_vb: u64,   // sat * 1000 / byte
    pub ts_secs:          u64,   // unix timestamp (seconds)
}

// ── MempoolDb ────────────────────────────────────────────────────────────────

pub struct MempoolDb {
    db:   DB,
    path: PathBuf,
}

impl MempoolDb {
    // ── Open ─────────────────────────────────────────────────────────────────

    pub fn open(path: &Path) -> Result<Self, SyncError> {
        std::fs::create_dir_all(path)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        let mut opts = crate::pkt_paths::db_opts();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        Ok(Self { db, path: path.to_owned() })
    }

    pub fn open_read_only(path: &Path) -> Result<Self, SyncError> {
        std::fs::create_dir_all(path)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        let opts = Options::default();
        let db = DB::open_for_read_only(&opts, path, false)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        Ok(Self { db, path: path.to_owned() })
    }

    /// Temp DB for unit tests.
    pub fn open_temp() -> Result<Self, SyncError> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("pkt_mempooldb_test_{}", ts));
        Self::open(&path)
    }

    pub fn path(&self) -> &Path { &self.path }

    // ── Core operations ───────────────────────────────────────────────────────

    /// Return true if the given txid (hex) is already in the mempool.
    pub fn has_tx(&self, txid_hex: &str) -> bool {
        let key = format!("tx:{}", txid_hex);
        self.db.get(key.as_bytes()).ok().flatten().is_some()
    }

    /// Store a transaction: writes `tx:`, `fee:`, `ts:` keys atomically.
    pub fn put_tx(
        &self,
        txid_hex:         &str,
        raw_bytes:        &[u8],
        fee_rate_msat_vb: u64,
        ts_ns:            u64,
    ) -> Result<(), SyncError> {
        // tx:{txid} → raw wire bytes
        let tx_key = format!("tx:{}", txid_hex);
        self.db.put(tx_key.as_bytes(), raw_bytes)
            .map_err(|e| SyncError::Db(e.to_string()))?;

        // fee:{MAX-rate:020}:{txid} → "" (lex scan = highest fee first)
        let inv_rate = u64::MAX - fee_rate_msat_vb;
        let fee_key  = format!("fee:{:020}:{}", inv_rate, txid_hex);
        self.db.put(fee_key.as_bytes(), b"")
            .map_err(|e| SyncError::Db(e.to_string()))?;

        // ts:{txid} → [ts_ns: 8 LE][fee_rate: 8 LE]
        let ts_key = format!("ts:{}", txid_hex);
        let mut meta = [0u8; 16];
        meta[..8].copy_from_slice(&ts_ns.to_le_bytes());
        meta[8..].copy_from_slice(&fee_rate_msat_vb.to_le_bytes());
        self.db.put(ts_key.as_bytes(), &meta)
            .map_err(|e| SyncError::Db(e.to_string()))?;

        Ok(())
    }

    /// Lấy raw bytes + fee_rate + ts_ns của một tx trong mempool.
    /// Returns None nếu txid không tồn tại.
    pub fn get_tx_raw(&self, txid_hex: &str) -> Option<(Vec<u8>, u64, u64)> {
        let tx_key = format!("tx:{}", txid_hex);
        let raw    = self.db.get(tx_key.as_bytes()).ok()??;
        let ts_key = format!("ts:{}", txid_hex);
        let (ts_ns, fee_rate) = match self.db.get(ts_key.as_bytes()).ok().flatten() {
            Some(v) if v.len() == 16 => (
                u64::from_le_bytes(v[..8].try_into().unwrap()),
                u64::from_le_bytes(v[8..].try_into().unwrap()),
            ),
            _ => (0, 0),
        };
        Some((raw.to_vec(), fee_rate, ts_ns))
    }

    /// Delete confirmed transactions (called when a block is applied).
    pub fn evict_confirmed(&self, txids: &[[u8; 32]]) -> Result<(), SyncError> {
        for txid in txids {
            let txid_hex = hex::encode(txid);

            // Read fee_rate from ts: key to reconstruct the fee: key
            let ts_key   = format!("ts:{}", txid_hex);
            let fee_rate = match self.db.get(ts_key.as_bytes())
                .map_err(|e| SyncError::Db(e.to_string()))?
            {
                Some(v) if v.len() == 16 => {
                    u64::from_le_bytes(v[8..16].try_into().unwrap())
                }
                _ => {
                    // Not in mempool — skip gracefully
                    continue;
                }
            };

            let inv_rate = u64::MAX - fee_rate;
            let fee_key  = format!("fee:{:020}:{}", inv_rate, txid_hex);
            let tx_key   = format!("tx:{}", txid_hex);

            self.db.delete(fee_key.as_bytes()).map_err(|e| SyncError::Db(e.to_string()))?;
            self.db.delete(tx_key.as_bytes()).map_err(|e| SyncError::Db(e.to_string()))?;
            self.db.delete(ts_key.as_bytes()).map_err(|e| SyncError::Db(e.to_string()))?;
        }
        Ok(())
    }

    /// Return up to `limit` pending transactions sorted by fee rate (highest first).
    pub fn get_pending(&self, limit: usize) -> Result<Vec<MempoolTxInfo>, SyncError> {
        let mut results = Vec::with_capacity(limit.min(256));
        let iter = self.db.iterator(IteratorMode::From(b"fee:", Direction::Forward));

        for item in iter {
            if results.len() >= limit { break; }
            let (k, _) = item.map_err(|e| SyncError::Db(e.to_string()))?;
            if !k.starts_with(b"fee:") { break; }

            // Key: "fee:{inv_rate:020}:{txid_hex}"
            let key_str = match std::str::from_utf8(&k) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // Split into at most 3 parts on ':'
            let mut parts = key_str.splitn(3, ':');
            let _prefix   = parts.next(); // "fee"
            let inv_str   = match parts.next() { Some(s) => s, None => continue };
            let txid_hex  = match parts.next() { Some(s) => s, None => continue };

            let inv_rate: u64 = inv_str.parse().unwrap_or(0);
            let fee_rate_msat_vb = u64::MAX - inv_rate;

            // Read raw bytes to get size
            let tx_key = format!("tx:{}", txid_hex);
            let raw = match self.db.get(tx_key.as_bytes())
                .map_err(|e| SyncError::Db(e.to_string()))?
            {
                Some(v) => v,
                None => continue, // stale fee key
            };

            // Read timestamp from ts: key
            let ts_key = format!("ts:{}", txid_hex);
            let ts_ns  = match self.db.get(ts_key.as_bytes())
                .map_err(|e| SyncError::Db(e.to_string()))?
            {
                Some(v) if v.len() == 16 => u64::from_le_bytes(v[..8].try_into().unwrap()),
                _ => 0,
            };

            results.push(MempoolTxInfo {
                txid:             txid_hex.to_string(),
                size:             raw.len() as u64,
                fee_rate_msat_vb,
                ts_secs:          ts_ns / 1_000_000_000,
            });
        }
        Ok(results)
    }

    /// Count pending transactions.
    pub fn count(&self) -> Result<usize, SyncError> {
        let mut n = 0usize;
        let iter = self.db.iterator(IteratorMode::From(b"tx:", Direction::Forward));
        for item in iter {
            let (k, _) = item.map_err(|e| SyncError::Db(e.to_string()))?;
            if !k.starts_with(b"tx:") { break; }
            n += 1;
        }
        Ok(n)
    }

    /// Fee rate histogram: returns Vec of (lower_bound_msat_vb, count).
    ///
    /// Buckets: [0, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000]
    /// The last bucket captures everything ≥ 1000 msat/vB.
    pub fn fee_rate_histogram(&self) -> Result<Vec<(u64, u64)>, SyncError> {
        const BUCKETS: &[u64] = &[0, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000];
        let mut counts = vec![0u64; BUCKETS.len()];

        let iter = self.db.iterator(IteratorMode::From(b"fee:", Direction::Forward));
        for item in iter {
            let (k, _) = item.map_err(|e| SyncError::Db(e.to_string()))?;
            if !k.starts_with(b"fee:") { break; }

            let key_str = match std::str::from_utf8(&k) { Ok(s) => s, Err(_) => continue };
            let mut parts = key_str.splitn(3, ':');
            let _  = parts.next();
            let inv_str = match parts.next() { Some(s) => s, None => continue };
            let inv_rate: u64 = inv_str.parse().unwrap_or(0);
            let fee_rate = u64::MAX - inv_rate;

            // Find highest bucket ≤ fee_rate
            let mut bucket_idx = 0;
            for (i, &b) in BUCKETS.iter().enumerate() {
                if fee_rate >= b { bucket_idx = i; }
            }
            counts[bucket_idx] += 1;
        }

        Ok(BUCKETS.iter().copied().zip(counts).collect())
    }
}

impl Drop for MempoolDb {
    fn drop(&mut self) {
        let p = self.path.to_string_lossy();
        if p.contains("pkt_mempooldb_test_") {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

// ── Default path ──────────────────────────────────────────────────────────────

pub fn default_mempool_db_path() -> PathBuf {
    crate::pkt_paths::mempool_db()
}

// ── Fee rate helper ───────────────────────────────────────────────────────────

/// Calculate fee rate in msat/vB (sat × 1000 / size_bytes).
///
/// Returns 0 for coinbase or when input UTXOs cannot be resolved.
pub fn calc_fee_rate_msat(tx: &WireTx, raw_size: usize, utxo_db: &UtxoSyncDb) -> u64 {
    if tx.is_coinbase() || raw_size == 0 { return 0; }

    let mut in_sum: u64 = 0;
    for inp in &tx.inputs {
        match utxo_db.get_utxo(&inp.prev_txid, inp.prev_vout) {
            Ok(Some(entry)) => { in_sum = in_sum.saturating_add(entry.value); }
            _ => { return 0; } // unresolvable input → treat as 0 fee
        }
    }
    let out_sum: u64 = tx.outputs.iter().map(|o| o.value).sum();
    if in_sum < out_sum { return 0; }
    let fee = in_sum - out_sum;
    fee.saturating_mul(1000) / raw_size as u64
}

// ── Mempool sync ─────────────────────────────────────────────────────────────

/// Sync pending transactions from the peer's mempool.
///
/// Flow:
///   1. Send `mempool` message (zero payload, custom wire command)
///   2. Wait up to MEMPOOL_INV_TIMEOUT_SECS for `inv` responses with tx hashes
///   3. Filter out txids already in `mempool_db`
///   4. Send `getdata` for up to MAX_MEMPOOL_FETCH new txids
///   5. Receive `tx` messages, parse, calculate fee, store in mempool_db
///
/// Returns the number of new transactions stored.
pub fn sync_mempool(
    stream:     &mut TcpStream,
    magic:      [u8; 4],
    utxo_db:    &UtxoSyncDb,
    mempool_db: &MempoolDb,
) -> Result<usize, SyncError> {
    // Set short recv timeout for mempool operations
    let _ = stream.set_read_timeout(Some(Duration::from_secs(MEMPOOL_INV_TIMEOUT_SECS)));

    // Send `mempool` message (empty payload, custom command)
    let cmd = command_bytes("mempool");
    let msg = PktMsg::Unknown { command: cmd, payload: vec![] };
    send_msg(stream, msg, magic).map_err(SyncError::from)?;

    // ── Phase 1: collect tx hashes from inv responses ─────────────────────────
    let mut pending: Vec<[u8; 32]> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(MEMPOOL_INV_TIMEOUT_SECS);

    loop {
        if Instant::now() > deadline { break; }
        let m = match recv_msg(stream, magic) {
            Ok(m)  => m,
            Err(PeerError::Timeout) | Err(PeerError::Io(_)) => break,
            Err(_) => break,
        };
        match m {
            PktMsg::Inv { items } => {
                for item in items {
                    if item.inv_type != INV_MSG_TX { continue; }
                    if mempool_db.has_tx(&hex::encode(item.hash)) { continue; }
                    pending.push(item.hash);
                    if pending.len() >= MAX_MEMPOOL_FETCH { break; }
                }
                // Got an inv — whether empty or full, move on
                break;
            }
            PktMsg::Ping { nonce } => {
                let _ = send_msg(stream, PktMsg::Pong { nonce }, magic);
            }
            _ => {} // ignore other unsolicited messages
        }
    }

    if pending.is_empty() { return Ok(0); }

    // ── Phase 2: request txs ──────────────────────────────────────────────────
    let items: Vec<InvItem> = pending.iter().map(|h| InvItem::tx(*h)).collect();
    send_msg(stream, PktMsg::GetData { items }, magic).map_err(SyncError::from)?;

    // ── Phase 3: receive and store tx messages ────────────────────────────────
    let _ = stream.set_read_timeout(Some(Duration::from_secs(MEMPOOL_TX_TIMEOUT_SECS)));
    let deadline2 = Instant::now() + Duration::from_secs(MEMPOOL_TX_TIMEOUT_SECS);
    let expected  = pending.len();
    let mut stored = 0usize;

    loop {
        if Instant::now() > deadline2 { break; }
        if stored >= expected { break; }

        let m = match recv_msg(stream, magic) {
            Ok(m)  => m,
            Err(PeerError::Timeout) | Err(PeerError::Io(_)) => break,
            Err(_) => break,
        };
        match m {
            PktMsg::Unknown { command, payload } if command_name(&command) == "tx" => {
                // Parse tx from raw payload bytes
                let mut cur = Cursor::new(&payload[..]);
                let tx = match read_tx_s(&mut cur) {
                    Ok(t)  => t,
                    Err(_) => { stored += 1; continue; } // malformed tx → skip
                };
                let txid_hex  = hex::encode(wire_txid(&tx));
                let fee_rate  = calc_fee_rate_msat(&tx, payload.len(), utxo_db);
                let ts_ns     = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0);

                if let Err(e) = mempool_db.put_tx(&txid_hex, &payload, fee_rate, ts_ns) {
                    eprintln!("[mempool] store error {}: {:?}", txid_hex, e);
                }
                stored += 1;
            }
            PktMsg::Ping { nonce } => {
                let _ = send_msg(stream, PktMsg::Pong { nonce }, magic);
            }
            _ => {}
        }
    }

    Ok(stored)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static DB_LOCK: Mutex<()> = Mutex::new(());

    fn make_db() -> MempoolDb {
        let _g = DB_LOCK.lock().unwrap();
        // Small sleep to ensure unique timestamp suffix
        std::thread::sleep(Duration::from_millis(2));
        MempoolDb::open_temp().expect("open temp mempool db")
    }

    // ── open ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_open_temp_succeeds() {
        let db = make_db();
        assert!(db.path().exists());
    }

    // ── has_tx / put_tx ───────────────────────────────────────────────────────

    #[test]
    fn test_has_tx_unknown_returns_false() {
        let db = make_db();
        assert!(!db.has_tx("deadbeef"));
    }

    #[test]
    fn test_put_then_has_tx() {
        let db = make_db();
        let txid = "aabbccdd00112233aabbccdd00112233aabbccdd00112233aabbccdd00112233";
        db.put_tx(txid, b"raw_bytes", 42, 1_000_000).unwrap();
        assert!(db.has_tx(txid));
    }

    #[test]
    fn test_put_tx_increments_count() {
        let db = make_db();
        assert_eq!(db.count().unwrap(), 0);
        db.put_tx("tx0000000000000000000000000000000000000000000000000000000000000001",
                  b"raw1", 10, 100).unwrap();
        db.put_tx("tx0000000000000000000000000000000000000000000000000000000000000002",
                  b"raw2", 20, 200).unwrap();
        assert_eq!(db.count().unwrap(), 2);
    }

    // ── evict_confirmed ───────────────────────────────────────────────────────

    #[test]
    fn test_evict_removes_tx() {
        let db = make_db();
        let txid_hex = "cc00000000000000000000000000000000000000000000000000000000000001";
        db.put_tx(txid_hex, b"raw", 50, 999).unwrap();
        assert!(db.has_tx(txid_hex));

        let mut txid_bytes = [0u8; 32];
        hex::decode_to_slice(txid_hex, &mut txid_bytes).unwrap();
        db.evict_confirmed(&[txid_bytes]).unwrap();
        assert!(!db.has_tx(txid_hex));
    }

    #[test]
    fn test_evict_unknown_txid_is_noop() {
        let db = make_db();
        let txid = [0xffu8; 32];
        // Should not return error even if txid not in mempool
        db.evict_confirmed(&[txid]).unwrap();
    }

    #[test]
    fn test_evict_reduces_count() {
        let db = make_db();
        let t1 = "dd00000000000000000000000000000000000000000000000000000000000001";
        let t2 = "dd00000000000000000000000000000000000000000000000000000000000002";
        db.put_tx(t1, b"r1", 1, 0).unwrap();
        db.put_tx(t2, b"r2", 2, 0).unwrap();
        assert_eq!(db.count().unwrap(), 2);

        let mut b1 = [0u8; 32]; hex::decode_to_slice(t1, &mut b1).unwrap();
        db.evict_confirmed(&[b1]).unwrap();
        assert_eq!(db.count().unwrap(), 1);
    }

    // ── get_pending ───────────────────────────────────────────────────────────

    #[test]
    fn test_get_pending_highest_fee_first() {
        let db = make_db();
        let low  = "ee00000000000000000000000000000000000000000000000000000000000001";
        let high = "ee00000000000000000000000000000000000000000000000000000000000002";
        db.put_tx(low,  b"raw_low",  10, 0).unwrap();
        db.put_tx(high, b"raw_high", 9000, 0).unwrap();

        let pending = db.get_pending(10).unwrap();
        assert_eq!(pending.len(), 2);
        assert!(pending[0].fee_rate_msat_vb >= pending[1].fee_rate_msat_vb,
            "first entry should have higher fee rate");
        assert_eq!(pending[0].txid, high);
    }

    #[test]
    fn test_get_pending_respects_limit() {
        let db = make_db();
        for i in 0u64..5 {
            let txid = format!("ff{:062x}", i);
            db.put_tx(&txid, b"r", i * 100, 0).unwrap();
        }
        let pending = db.get_pending(3).unwrap();
        assert_eq!(pending.len(), 3);
    }

    // ── fee_rate_histogram ────────────────────────────────────────────────────

    #[test]
    fn test_histogram_empty() {
        let db = make_db();
        let hist = db.fee_rate_histogram().unwrap();
        assert!(!hist.is_empty());
        let total: u64 = hist.iter().map(|(_, c)| c).sum();
        assert_eq!(total, 0);
    }

    #[test]
    fn test_histogram_buckets_correct() {
        let db = make_db();
        // fee_rate = 7 → bucket 5 (≥5, <10)
        db.put_tx("aa00000000000000000000000000000000000000000000000000000000000001",
                  b"r", 7, 0).unwrap();
        // fee_rate = 150 → bucket 100 (≥100, <200)
        db.put_tx("aa00000000000000000000000000000000000000000000000000000000000002",
                  b"r", 150, 0).unwrap();
        let hist = db.fee_rate_histogram().unwrap();
        // Bucket with lower=5 should have count=1
        let bucket_5  = hist.iter().find(|(lb, _)| *lb == 5).map(|(_, c)| *c).unwrap_or(0);
        let bucket_100 = hist.iter().find(|(lb, _)| *lb == 100).map(|(_, c)| *c).unwrap_or(0);
        assert_eq!(bucket_5,   1);
        assert_eq!(bucket_100, 1);
    }

    // ── default_path ──────────────────────────────────────────────────────────

    #[test]
    fn test_default_path_contains_pkt() {
        let p = default_mempool_db_path();
        assert!(p.to_string_lossy().contains(".pkt"));
    }
}
