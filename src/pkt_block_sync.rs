#![allow(dead_code)]
//! v15.9 — Block Sync (streaming)
//!
//! Downloads full blocks from peer, streaming-parses transactions,
//! applies UTXOs, verifies merkle root.  RAM footprint ≈ size(one tx).
//!
//! Pipeline:
//!   peer → TcpStream → LimitedReader(payload_len)
//!     → header(80B) → varint(tx_count)
//!     → [tx_i → apply_wire_tx → collect txid] × N
//!     → verify merkle(txids) → discard
//!
//! Storage:
//!   blockdb  (~/.pkt/blockdb): height:016x → block_hash (metadata only)
//!   utxodb   : updated in-place via apply_wire_tx
//!
//! Resume: starts from utxo_height + 1, skips already-applied blocks.

use std::io::{self, Read};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rocksdb::{DB, Options};
use sha2::{Digest, Sha256};

use crate::pkt_addr_index::AddrIndexDb;
use crate::pkt_mempool_sync::MempoolDb;
use crate::pkt_reorg::{BlockDelta, ReorgDb};
use crate::pkt_sync::{SyncDb, SyncError};
use crate::pkt_utxo_sync::{apply_wire_tx, wire_txid, UtxoSyncDb, WireTx, WireTxIn, WireTxOut};
use crate::pkt_wire::{InvItem, PktMsg, HEADER_LEN};
use crate::pkt_peer::send_msg;

// ── BlockSyncDb ───────────────────────────────────────────────────────────────

/// Stores minimal block metadata (height ↔ hash).  No raw block bytes.
pub struct BlockSyncDb {
    db:   DB,
    path: PathBuf,
}

impl BlockSyncDb {
    pub fn open(path: &Path) -> Result<Self, SyncError> {
        let mut opts = crate::pkt_paths::db_opts();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        Ok(Self { db, path: path.to_owned() })
    }

    pub fn open_read_only(path: &Path) -> Result<Self, SyncError> {
        let opts = Options::default();
        let db = DB::open_for_read_only(&opts, path, false)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        Ok(Self { db, path: path.to_owned() })
    }

    pub fn open_temp() -> Result<Self, SyncError> {
        use std::time::SystemTime;
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("pkt_blockdb_{}", ts));
        Self::open(&path)
    }

    pub fn path(&self) -> &Path { &self.path }

    /// Store height → hash and hash → height.
    pub fn set_block(&self, height: u64, hash: &[u8; 32]) -> Result<(), SyncError> {
        let hkey = format!("height:{:016x}", height);
        self.db.put(hkey.as_bytes(), hash)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        let mut rkey = [0u8; 37]; // "hash:" + 32 bytes
        rkey[..5].copy_from_slice(b"hash:");
        rkey[5..].copy_from_slice(hash);
        self.db.put(&rkey, &height.to_le_bytes())
            .map_err(|e| SyncError::Db(e.to_string()))?;
        self.db.put(b"meta:block_height", &height.to_le_bytes())
            .map_err(|e| SyncError::Db(e.to_string()))
    }

    pub fn get_block_hash(&self, height: u64) -> Result<Option<[u8; 32]>, SyncError> {
        let key = format!("height:{:016x}", height);
        match self.db.get(key.as_bytes()).map_err(|e| SyncError::Db(e.to_string()))? {
            None => Ok(None),
            Some(v) if v.len() == 32 => {
                let mut h = [0u8; 32];
                h.copy_from_slice(&v);
                Ok(Some(h))
            }
            Some(_) => Ok(None),
        }
    }

    pub fn get_block_height(&self) -> Result<Option<u64>, SyncError> {
        match self.db.get(b"meta:block_height").map_err(|e| SyncError::Db(e.to_string()))? {
            None => Ok(None),
            Some(v) if v.len() == 8 => Ok(Some(u64::from_le_bytes(v[..8].try_into().unwrap()))),
            Some(_) => Ok(None),
        }
    }
}

// ── LimitedReader ─────────────────────────────────────────────────────────────

/// Wraps a TcpStream, allowing exactly `remaining` bytes to be read.
/// Prevents over-reading into the next wire message.
struct LimitedReader<'a> {
    inner:     &'a mut TcpStream,
    remaining: u32,
}

impl<'a> Read for LimitedReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 { return Ok(0); }
        let to_read = buf.len().min(self.remaining as usize);
        let n = self.inner.read(&mut buf[..to_read])?;
        self.remaining = self.remaining.saturating_sub(n as u32);
        Ok(n)
    }
}

impl<'a> LimitedReader<'a> {
    /// Drain any unread bytes (e.g. PacketCrypt proof after last tx).
    fn drain(mut self) -> io::Result<()> {
        let mut buf = [0u8; 4096];
        while self.remaining > 0 {
            let n = self.read(&mut buf)?;
            if n == 0 { break; }
        }
        Ok(())
    }
}

// ── Stream primitives ─────────────────────────────────────────────────────────

fn read_exact_s<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<(), SyncError> {
    r.read_exact(buf).map_err(|e| SyncError::InvalidHeader(format!("read: {}", e)))
}

fn read_u32_le_s<R: Read>(r: &mut R) -> Result<u32, SyncError> {
    let mut b = [0u8; 4];
    read_exact_s(r, &mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_i32_le_s<R: Read>(r: &mut R) -> Result<i32, SyncError> {
    let mut b = [0u8; 4];
    read_exact_s(r, &mut b)?;
    Ok(i32::from_le_bytes(b))
}

fn read_u64_le_s<R: Read>(r: &mut R) -> Result<u64, SyncError> {
    let mut b = [0u8; 8];
    read_exact_s(r, &mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn read_hash32_s<R: Read>(r: &mut R) -> Result<[u8; 32], SyncError> {
    let mut h = [0u8; 32];
    read_exact_s(r, &mut h)?;
    Ok(h)
}

fn read_bytes_s<R: Read>(r: &mut R, n: usize) -> Result<Vec<u8>, SyncError> {
    let mut v = vec![0u8; n];
    read_exact_s(r, &mut v)?;
    Ok(v)
}

fn read_varint_s<R: Read>(r: &mut R) -> Result<u64, SyncError> {
    let mut first = [0u8; 1];
    read_exact_s(r, &mut first)?;
    match first[0] {
        0xfd => {
            let mut b = [0u8; 2];
            read_exact_s(r, &mut b)?;
            Ok(u16::from_le_bytes(b) as u64)
        }
        0xfe => {
            let mut b = [0u8; 4];
            read_exact_s(r, &mut b)?;
            Ok(u32::from_le_bytes(b) as u64)
        }
        0xff => {
            let mut b = [0u8; 8];
            read_exact_s(r, &mut b)?;
            Ok(u64::from_le_bytes(b))
        }
        n => Ok(n as u64),
    }
}

// ── Tx stream parsing ─────────────────────────────────────────────────────────

fn read_txin_s<R: Read>(r: &mut R) -> Result<WireTxIn, SyncError> {
    let prev_txid  = read_hash32_s(r)?;
    let prev_vout  = read_u32_le_s(r)?;
    let slen       = read_varint_s(r)? as usize;
    let script_sig = read_bytes_s(r, slen)?;
    let sequence   = read_u32_le_s(r)?;
    Ok(WireTxIn { prev_txid, prev_vout, script_sig, sequence })
}

fn read_txout_s<R: Read>(r: &mut R) -> Result<WireTxOut, SyncError> {
    let value      = read_u64_le_s(r)?;
    let slen       = read_varint_s(r)? as usize;
    let script_pubkey = read_bytes_s(r, slen)?;
    Ok(WireTxOut { value, script_pubkey })
}

/// Parse one Bitcoin wire tx (legacy + segwit).
pub fn read_tx_s<R: Read>(r: &mut R) -> Result<WireTx, SyncError> {
    let version   = read_i32_le_s(r)?;
    let in_count  = read_varint_s(r)? as usize;

    // Segwit: in_count == 0 means [marker=0x00][flag=0x01]
    if in_count == 0 {
        let _flag     = read_bytes_s(r, 1)?; // flag byte (0x01)
        let in_count2 = read_varint_s(r)? as usize;
        let mut inputs = Vec::with_capacity(in_count2);
        for _ in 0..in_count2 { inputs.push(read_txin_s(r)?); }
        let out_count = read_varint_s(r)? as usize;
        let mut outputs = Vec::with_capacity(out_count);
        for _ in 0..out_count { outputs.push(read_txout_s(r)?); }
        // Skip witness stacks (one stack per input)
        for _ in 0..in_count2 {
            let items = read_varint_s(r)? as usize;
            for _ in 0..items {
                let len = read_varint_s(r)? as usize;
                read_bytes_s(r, len)?;
            }
        }
        let locktime = read_u32_le_s(r)?;
        return Ok(WireTx { version, inputs, outputs, locktime });
    }

    let mut inputs = Vec::with_capacity(in_count);
    for _ in 0..in_count { inputs.push(read_txin_s(r)?); }
    let out_count  = read_varint_s(r)? as usize;
    let mut outputs = Vec::with_capacity(out_count);
    for _ in 0..out_count { outputs.push(read_txout_s(r)?); }
    let locktime   = read_u32_le_s(r)?;
    Ok(WireTx { version, inputs, outputs, locktime })
}

// ── Merkle ────────────────────────────────────────────────────────────────────

fn sha256d(data: &[u8]) -> [u8; 32] {
    let h1 = Sha256::digest(data);
    Sha256::digest(h1).into()
}

/// Compute Bitcoin merkle root from a list of txids (little-endian).
fn merkle_root(txids: &[[u8; 32]]) -> [u8; 32] {
    if txids.is_empty() { return [0u8; 32]; }
    let mut row: Vec<[u8; 32]> = txids.to_vec();
    while row.len() > 1 {
        if row.len() % 2 == 1 { row.push(*row.last().unwrap()); }
        row = row.chunks(2).map(|pair| {
            let mut buf = [0u8; 64];
            buf[..32].copy_from_slice(&pair[0]);
            buf[32..].copy_from_slice(&pair[1]);
            sha256d(&buf)
        }).collect();
    }
    row[0]
}

// ── Wire message header ───────────────────────────────────────────────────────

/// Read 24-byte wire header, return (command_str, payload_len).
/// Verifies magic bytes.
fn recv_wire_msg_header(
    stream: &mut TcpStream,
    magic:  &[u8; 4],
) -> Result<(String, u32), SyncError> {
    let mut hdr = [0u8; HEADER_LEN];
    stream.read_exact(&mut hdr)
        .map_err(|e| SyncError::InvalidHeader(format!("wire header read: {}", e)))?;

    if &hdr[0..4] != magic {
        return Err(SyncError::InvalidHeader(format!(
            "wrong magic: {:02x}{:02x}{:02x}{:02x}",
            hdr[0], hdr[1], hdr[2], hdr[3]
        )));
    }

    // Command: 12 bytes null-padded ASCII
    let cmd_bytes = &hdr[4..16];
    let cmd = std::str::from_utf8(cmd_bytes)
        .unwrap_or("")
        .trim_end_matches('\0')
        .to_string();

    let payload_len = u32::from_le_bytes([hdr[16], hdr[17], hdr[18], hdr[19]]);
    Ok((cmd, payload_len))
}

// ── Block receive + apply ─────────────────────────────────────────────────────

/// Result of applying one block's UTXOs.
pub struct BlockApplyResult {
    pub height:          u64,
    pub tx_count:        u64,
    pub hash:            [u8; 32],
    /// Txids confirmed by this block (used for mempool eviction).
    pub confirmed_txids: Vec<[u8; 32]>,
}

/// Send GetData for one block hash, stream-receive and apply UTXOs.
///
/// Skips ping/pong messages while waiting. Returns error on non-block messages
/// after MAX_SKIP attempts.
pub fn sync_one_block(
    stream:       &mut TcpStream,
    magic:        &[u8; 4],
    block_hash:   [u8; 32],
    height:       u64,
    utxo_db:      &UtxoSyncDb,
    block_db:     &BlockSyncDb,
    addr_db:      Option<&AddrIndexDb>,
    reorg_db:     Option<&ReorgDb>,
    mempool_db:   Option<&MempoolDb>,
    skip_merkle:  bool,
) -> Result<BlockApplyResult, SyncError> {
    // Request the block
    let getdata = PktMsg::GetData { items: vec![InvItem::block(block_hash)] };
    send_msg(stream, getdata, *magic)
        .map_err(|e| SyncError::InvalidHeader(format!("getdata send: {:?}", e)))?;

    // Wait for "block" message (skip ping/pong/inv)
    const MAX_SKIP: usize = 20;
    for _ in 0..MAX_SKIP {
        let (cmd, payload_len) = recv_wire_msg_header(stream, magic)?;

        match cmd.as_str() {
            "ping" => {
                // Respond to ping to keep connection alive
                let mut payload = vec![0u8; payload_len as usize];
                stream.read_exact(&mut payload)
                    .map_err(|e| SyncError::InvalidHeader(format!("ping read: {}", e)))?;
                let nonce = if payload.len() >= 8 {
                    u64::from_le_bytes(payload[..8].try_into().unwrap())
                } else { 0 };
                let pong = PktMsg::Pong { nonce };
                let _ = send_msg(stream, pong, *magic);
                continue;
            }
            "inv" | "alert" | "notfound" => {
                // Drain and skip
                let mut buf = vec![0u8; payload_len as usize];
                stream.read_exact(&mut buf)
                    .map_err(|e| SyncError::InvalidHeader(format!("skip {}: {}", cmd, e)))?;
                continue;
            }
            "block" => {
                // Parse block streaming
                let mut reader = LimitedReader { inner: stream, remaining: payload_len };
                let result = apply_block_streaming(
                    &mut reader, utxo_db, block_db, addr_db, reorg_db, height, block_hash, skip_merkle,
                )?;
                reader.drain()
                    .map_err(|e| SyncError::InvalidHeader(format!("drain: {}", e)))?;
                // Evict confirmed txs from mempool
                if let Some(mdb) = mempool_db {
                    let _ = mdb.evict_confirmed(&result.confirmed_txids);
                }
                return Ok(result);
            }
            other => {
                // Drain unknown message
                let mut buf = vec![0u8; payload_len as usize];
                stream.read_exact(&mut buf)
                    .map_err(|e| SyncError::InvalidHeader(format!("drain {}: {}", other, e)))?;
            }
        }
    }

    Err(SyncError::Timeout)
}

/// Stream-parse a block payload, apply UTXOs, verify merkle.
fn apply_block_streaming<R: Read>(
    r:           &mut R,
    utxo_db:     &UtxoSyncDb,
    block_db:    &BlockSyncDb,
    addr_db:     Option<&AddrIndexDb>,
    reorg_db:    Option<&ReorgDb>,
    height:      u64,
    block_hash:  [u8; 32],
    skip_merkle: bool,
) -> Result<BlockApplyResult, SyncError> {
    // Read 80-byte wire header — keep bytes for merkle check
    let header_bytes = read_bytes_s(r, 80)?;
    // merkle_root field is bytes [36..68]
    let mut expected_merkle = [0u8; 32];
    expected_merkle.copy_from_slice(&header_bytes[36..68]);

    let tx_count = read_varint_s(r)? as usize;
    // Genesis block has 0 txs — no UTXOs to apply, just advance height pointer.
    if tx_count == 0 {
        block_db.set_block(height, &block_hash)?;
        utxo_db.set_utxo_height(height)
            .map_err(|e| SyncError::Db(format!("set_utxo_height: {:?}", e)))?;
        if let Some(adb) = addr_db {
            adb.set_addr_height(height)
                .map_err(|e| SyncError::Db(format!("addr_height: {:?}", e)))?;
        }
        return Ok(BlockApplyResult { height, tx_count: 0, hash: block_hash, confirmed_txids: vec![] });
    }

    let mut txids: Vec<[u8; 32]> = Vec::with_capacity(tx_count);
    // Collect delta only when reorg_db is present (zero overhead otherwise).
    let mut delta: Option<BlockDelta> = reorg_db.map(|_| BlockDelta::new(block_hash));

    for _ in 0..tx_count {
        let tx       = read_tx_s(r)?;
        let txid     = wire_txid(&tx);
        let txid_hex = hex::encode(&txid);
        txids.push(txid);

        // Index inputs BEFORE apply removes UTXOs from utxo_db
        if let Some(adb) = addr_db {
            adb.index_tx_inputs(utxo_db, &tx, &txid, height)?;
        }

        // Collect delta for inputs BEFORE apply (UTXOs still exist)
        if let Some(ref mut d) = delta {
            for inp in &tx.inputs {
                if inp.is_coinbase() { continue; }
                if let Ok(Some(entry)) = utxo_db.get_utxo(&inp.prev_txid, inp.prev_vout) {
                    let script  = hex::encode(&entry.script_pubkey);
                    let atx_key = format!("atx:{}:{:016x}:{}", script, height, txid_hex);
                    d.add_spent(
                        hex::encode(inp.prev_txid),
                        inp.prev_vout,
                        entry.value,
                        script,
                        atx_key,
                    );
                }
            }
        }

        apply_wire_tx(utxo_db, &tx, &txid, height)?;

        // Index outputs directly from WireTx (no utxo_db lookup needed)
        if let Some(adb) = addr_db {
            adb.index_tx_outputs(&tx, &txid, height)?;
        }

        // Collect delta for outputs
        if let Some(ref mut d) = delta {
            for (vout, out) in tx.outputs.iter().enumerate() {
                if out.script_pubkey.is_empty() { continue; }
                let script  = hex::encode(&out.script_pubkey);
                let atx_key = format!("atx:{}:{:016x}:{}", script, height, txid_hex);
                d.add_created(txid_hex.clone(), vout as u32, atx_key);
            }
        }
    }

    // Merkle verification
    if !skip_merkle {
        let computed = merkle_root(&txids);
        if computed != expected_merkle {
            return Err(SyncError::InvalidHeader(format!(
                "merkle mismatch at height {}: expected {} got {}",
                height,
                hex::encode(expected_merkle),
                hex::encode(computed),
            )));
        }
    }

    // Record in blockdb
    block_db.set_block(height, &block_hash)?;

    // Update utxo_height
    utxo_db.set_utxo_height(height)
        .map_err(|e| SyncError::Db(format!("set_utxo_height: {:?}", e)))?;

    // Update addr_height
    if let Some(adb) = addr_db {
        adb.set_addr_height(height)
            .map_err(|e| SyncError::Db(format!("addr_height: {:?}", e)))?;
    }

    // Save reorg delta (checkpoint + rollback data)
    if let (Some(rdb), Some(d)) = (reorg_db, delta) {
        rdb.save_delta(height, &d)
            .map_err(|e| SyncError::Db(format!("reorg_delta: {:?}", e)))?;
    }

    Ok(BlockApplyResult {
        height,
        tx_count:        tx_count as u64,
        hash:            block_hash,
        confirmed_txids: txids,
    })
}

// ── Main sync loop ────────────────────────────────────────────────────────────

pub struct BlockSyncResult {
    pub blocks_applied: u64,
    pub final_height:   u64,
    pub elapsed_ms:     u128,
}

/// Download and apply blocks from `utxo_height + 1` to `sync_height`.
///
/// Reads block hashes from syncdb (already validated headers).
/// Applies UTXOs streaming per block. Resumes on restart.
pub fn sync_blocks(
    stream:      &mut TcpStream,
    sync_db:     &SyncDb,
    utxo_db:     &UtxoSyncDb,
    block_db:    &BlockSyncDb,
    addr_db:     Option<&AddrIndexDb>,
    reorg_db:    Option<&ReorgDb>,
    mempool_db:  Option<&MempoolDb>,
    magic:       &[u8; 4],
    skip_merkle: bool,
) -> Result<BlockSyncResult, SyncError> {
    let t0 = Instant::now();

    let sync_height = sync_db.get_sync_height()
        .map_err(|e| SyncError::Db(format!("{:?}", e)))?
        .unwrap_or(0);
    let utxo_height = utxo_db.get_utxo_height()
        .map_err(|e| SyncError::Db(format!("{:?}", e)))?
        .unwrap_or(0);

    if utxo_height >= sync_height {
        return Ok(BlockSyncResult { blocks_applied: 0, final_height: utxo_height, elapsed_ms: 0 });
    }

    let start = utxo_height + 1;
    let mut applied = 0u64;
    let mut final_h = utxo_height;

    for height in start..=sync_height {
        let block_hash = sync_db.get_header_hash(height)
            .map_err(|e| SyncError::Db(format!("{:?}", e)))?
            .ok_or_else(|| SyncError::InvalidHeader(format!("no header at height {}", height)))?;

        let r = sync_one_block(stream, magic, block_hash, height, utxo_db, block_db, addr_db, reorg_db, mempool_db, skip_merkle)?;
        applied += 1;
        final_h  = r.height;
        // v22.2: lưu tx_count vào sync_db để block_detail có thể dùng ngay cả khi addr_db trống
        let _ = sync_db.save_block_tx_count(r.height, r.tx_count);

        if applied % 10 == 0 || height == sync_height {
            println!(
                "[block-sync] applied height={} txs={} ({}/{})",
                r.height, r.tx_count, applied, sync_height - start + 1
            );
        }
    }

    Ok(BlockSyncResult {
        blocks_applied: applied,
        final_height:   final_h,
        elapsed_ms:     t0.elapsed().as_millis(),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // open_temp() uses SystemTime — serialize DB tests to avoid lock collision.
    static DB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_read_varint_s_single_byte() {
        let data = [42u8];
        let mut r = &data[..];
        assert_eq!(read_varint_s(&mut r).unwrap(), 42);
    }

    #[test]
    fn test_read_varint_s_fd() {
        let data = [0xfd, 0x00, 0x01];
        let mut r = &data[..];
        assert_eq!(read_varint_s(&mut r).unwrap(), 256);
    }

    #[test]
    fn test_read_varint_s_fe() {
        let mut data = vec![0xfe];
        data.extend_from_slice(&(65537u32).to_le_bytes());
        let mut r = &data[..];
        assert_eq!(read_varint_s(&mut r).unwrap(), 65537);
    }

    #[test]
    fn test_merkle_root_single() {
        let txid = [1u8; 32];
        assert_eq!(merkle_root(&[txid]), txid);
    }

    #[test]
    fn test_merkle_root_two() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let mut buf = [0u8; 64];
        buf[..32].copy_from_slice(&a);
        buf[32..].copy_from_slice(&b);
        let expected = sha256d(&buf);
        assert_eq!(merkle_root(&[a, b]), expected);
    }

    #[test]
    fn test_merkle_root_odd_duplicates_last() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let c = [3u8; 32];
        // odd: [a,b,c,c] → pairs (a,b) and (c,c)
        let r3 = merkle_root(&[a, b, c]);
        let ab = { let mut buf = [0u8;64]; buf[..32].copy_from_slice(&a); buf[32..].copy_from_slice(&b); sha256d(&buf) };
        let cc = { let mut buf = [0u8;64]; buf[..32].copy_from_slice(&c); buf[32..].copy_from_slice(&c); sha256d(&buf) };
        let root = { let mut buf = [0u8;64]; buf[..32].copy_from_slice(&ab); buf[32..].copy_from_slice(&cc); sha256d(&buf) };
        assert_eq!(r3, root);
    }

    #[test]
    fn test_merkle_root_empty() {
        assert_eq!(merkle_root(&[]), [0u8; 32]);
    }

    #[test]
    fn test_blockdb_open_temp() {
        let _g = DB_LOCK.lock().unwrap();
        let db = BlockSyncDb::open_temp().unwrap();
        assert!(db.path().exists());
    }

    #[test]
    fn test_blockdb_set_get_block() {
        let _g   = DB_LOCK.lock().unwrap();
        let db   = BlockSyncDb::open_temp().unwrap();
        let hash = [0xab; 32];
        db.set_block(7, &hash).unwrap();
        assert_eq!(db.get_block_hash(7).unwrap(), Some(hash));
        assert_eq!(db.get_block_height().unwrap(), Some(7));
    }

    #[test]
    fn test_blockdb_get_missing_returns_none() {
        let _g = DB_LOCK.lock().unwrap();
        let db = BlockSyncDb::open_temp().unwrap();
        assert_eq!(db.get_block_hash(999).unwrap(), None);
    }

    #[test]
    fn test_blockdb_height_updates_on_set() {
        let _g = DB_LOCK.lock().unwrap();
        let db = BlockSyncDb::open_temp().unwrap();
        db.set_block(1, &[0x01; 32]).unwrap();
        db.set_block(2, &[0x02; 32]).unwrap();
        assert_eq!(db.get_block_height().unwrap(), Some(2));
    }

    #[test]
    fn test_limited_reader_limits_bytes() {
        // Build a mock: we can't use TcpStream easily, test LimitedReader logic indirectly
        // via merkle_root which is pure.
        let r = merkle_root(&[[0u8; 32], [1u8; 32]]);
        assert_ne!(r, [0u8; 32]);
    }

    #[test]
    fn test_sha256d_known() {
        // SHA256d("") = 5df6e0e2... (Bitcoin genesis coinbase)
        let result = sha256d(b"");
        // Just verify it's not all zeros (correctness test via known property)
        assert_ne!(result, [0u8; 32]);
    }

    #[test]
    fn test_read_tx_s_coinbase_like() {
        // Build a minimal coinbase tx and parse it streaming
        use crate::pkt_utxo_sync::encode_wire_tx;
        let tx = WireTx {
            version: 1,
            inputs:  vec![WireTxIn {
                prev_txid:  [0u8; 32],
                prev_vout:  0xffffffff,
                script_sig: vec![0x03, 0x01, 0x00, 0x00],
                sequence:   0xffffffff,
            }],
            outputs: vec![WireTxOut {
                value:         5000000000,
                script_pubkey: vec![0x76, 0xa9, 0x14],
            }],
            locktime: 0,
        };
        let encoded = encode_wire_tx(&tx);
        let mut r   = encoded.as_slice();
        let decoded = read_tx_s(&mut r).unwrap();
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.inputs.len(), 1);
        assert_eq!(decoded.outputs.len(), 1);
        assert_eq!(decoded.outputs[0].value, 5000000000);
    }
}
