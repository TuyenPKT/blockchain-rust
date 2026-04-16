#![allow(dead_code)]
//! v15.3 — UTXO Sync
//!
//! Apply downloaded blocks (headers + transactions) vào UTXO state.
//! Track sync height, resume sau restart từ last known height.
//!
//! Pipeline:
//!   SyncDb (headers) → pkt_utxo_sync → UtxoSyncDb (UTXOs)
//!
//! Wire transaction format (Bitcoin standard, non-segwit):
//!   version(4) inputs[varint × input] outputs[varint × output] locktime(4)
//!   input:  prev_txid(32) prev_vout(4) script_sig[varint+N] sequence(4)
//!   output: value(8) script_pubkey[varint+N]
//!
//! txid = SHA256(SHA256(serialized_tx))  (without witness)
//!
//! UTXO DB key schema (separate RocksDB instance from SyncDb):
//!   utxo:{txid_hex}:{vout}  → JSON(UtxoEntry)
//!   txmeta:{txid_hex}       → JSON(TxMeta)   ← v24.1: TX index
//!   meta:utxo_height        → decimal u64 string
//!   meta:utxo_tip_hash      → raw [u8;32]

use std::path::{Path, PathBuf};

use crate::pkt_kv::Kv;
use sha2::{Digest, Sha256};
use serde::{Serialize, Deserialize};

use crate::pkt_wire::decode_varint;
use crate::pkt_sync::SyncError;

// ── Wire transaction types ────────────────────────────────────────────────────

/// Minimal Bitcoin wire transaction input (no witness).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireTxIn {
    pub prev_txid:  [u8; 32],
    pub prev_vout:  u32,
    pub script_sig: Vec<u8>,
    pub sequence:   u32,
}

impl WireTxIn {
    /// Coinbase input has null txid (all zeros) and vout=0xffffffff.
    pub fn is_coinbase(&self) -> bool {
        self.prev_txid == [0u8; 32] && self.prev_vout == 0xffff_ffff
    }
}

/// Minimal Bitcoin wire transaction output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireTxOut {
    pub value:        u64,
    pub script_pubkey: Vec<u8>,
}

/// A decoded Bitcoin wire transaction.
#[derive(Debug, Clone)]
pub struct WireTx {
    pub version:  i32,
    pub inputs:   Vec<WireTxIn>,
    pub outputs:  Vec<WireTxOut>,
    pub locktime: u32,
}

impl WireTx {
    pub fn is_coinbase(&self) -> bool {
        self.inputs.len() == 1 && self.inputs[0].is_coinbase()
    }
}

// ── UTXO entry (persisted in DB) ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoEntry {
    pub txid:         String,   // hex
    pub vout:         u32,
    pub value:        u64,      // satoshis
    pub script_pubkey: Vec<u8>,
    #[serde(default)]
    pub height:       u64,      // block height where UTXO was created (0 = unknown/pre-v22.1)
}

/// TX metadata stored in index (v24.1).
/// Key: `txmeta:{txid_hex}` in UtxoSyncDb.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxMeta {
    pub height:           u64,
    pub size:             u32,   // serialized bytes (non-witness)
    pub fee_rate_msat_vb: u64,   // 0 for coinbase hoặc khi không đủ UTXO data
    pub is_coinbase:      bool,
}

// ── Wire tx encoding (for tests) ─────────────────────────────────────────────

fn write_varint(buf: &mut Vec<u8>, n: u64) {
    let encoded = crate::pkt_wire::encode_varint(n);
    buf.extend_from_slice(&encoded);
}

/// Serialize a WireTxIn to bytes (for txid computation / tests).
fn encode_txin(inp: &WireTxIn) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&inp.prev_txid);
    buf.extend_from_slice(&inp.prev_vout.to_le_bytes());
    write_varint(&mut buf, inp.script_sig.len() as u64);
    buf.extend_from_slice(&inp.script_sig);
    buf.extend_from_slice(&inp.sequence.to_le_bytes());
    buf
}

/// Serialize a WireTxOut to bytes.
fn encode_txout(out: &WireTxOut) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&out.value.to_le_bytes());
    write_varint(&mut buf, out.script_pubkey.len() as u64);
    buf.extend_from_slice(&out.script_pubkey);
    buf
}

/// Serialize a WireTx to bytes (standard non-segwit format).
pub fn encode_wire_tx(tx: &WireTx) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&tx.version.to_le_bytes());
    write_varint(&mut buf, tx.inputs.len() as u64);
    for inp in &tx.inputs {
        buf.extend_from_slice(&encode_txin(inp));
    }
    write_varint(&mut buf, tx.outputs.len() as u64);
    for out in &tx.outputs {
        buf.extend_from_slice(&encode_txout(out));
    }
    buf.extend_from_slice(&tx.locktime.to_le_bytes());
    buf
}

// ── Wire tx decoding ──────────────────────────────────────────────────────────

fn need(data: &[u8], pos: usize, n: usize) -> Result<(), SyncError> {
    if pos + n > data.len() {
        Err(SyncError::InvalidHeader(format!(
            "tx decode: need {} bytes at pos {}, have {}",
            n, pos, data.len()
        )))
    } else {
        Ok(())
    }
}

fn read_u32_le(data: &[u8], pos: &mut usize) -> Result<u32, SyncError> {
    need(data, *pos, 4)?;
    let v = u32::from_le_bytes(data[*pos..*pos + 4].try_into().unwrap());
    *pos += 4;
    Ok(v)
}

fn read_i32_le(data: &[u8], pos: &mut usize) -> Result<i32, SyncError> {
    need(data, *pos, 4)?;
    let v = i32::from_le_bytes(data[*pos..*pos + 4].try_into().unwrap());
    *pos += 4;
    Ok(v)
}

fn read_u64_le(data: &[u8], pos: &mut usize) -> Result<u64, SyncError> {
    need(data, *pos, 8)?;
    let v = u64::from_le_bytes(data[*pos..*pos + 8].try_into().unwrap());
    *pos += 8;
    Ok(v)
}

fn read_hash32(data: &[u8], pos: &mut usize) -> Result<[u8; 32], SyncError> {
    need(data, *pos, 32)?;
    let mut h = [0u8; 32];
    h.copy_from_slice(&data[*pos..*pos + 32]);
    *pos += 32;
    Ok(h)
}

fn read_varint(data: &[u8], pos: &mut usize) -> Result<u64, SyncError> {
    let (v, consumed) = decode_varint(&data[*pos..])
        .map_err(|e| SyncError::InvalidHeader(format!("varint: {:?}", e)))?;
    *pos += consumed;
    Ok(v)
}

fn read_bytes(data: &[u8], pos: &mut usize, n: usize) -> Result<Vec<u8>, SyncError> {
    need(data, *pos, n)?;
    let v = data[*pos..*pos + n].to_vec();
    *pos += n;
    Ok(v)
}

fn decode_txin(data: &[u8], pos: &mut usize) -> Result<WireTxIn, SyncError> {
    let prev_txid  = read_hash32(data, pos)?;
    let prev_vout  = read_u32_le(data, pos)?;
    let script_len = read_varint(data, pos)? as usize;
    let script_sig = read_bytes(data, pos, script_len)?;
    let sequence   = read_u32_le(data, pos)?;
    Ok(WireTxIn { prev_txid, prev_vout, script_sig, sequence })
}

fn decode_txout(data: &[u8], pos: &mut usize) -> Result<WireTxOut, SyncError> {
    let value       = read_u64_le(data, pos)?;
    let script_len  = read_varint(data, pos)? as usize;
    let script_pubkey = read_bytes(data, pos, script_len)?;
    Ok(WireTxOut { value, script_pubkey })
}

/// Decode one Bitcoin wire transaction starting at `pos`, advance `pos`.
pub fn decode_wire_tx(data: &[u8], pos: &mut usize) -> Result<WireTx, SyncError> {
    let version    = read_i32_le(data, pos)?;
    let in_count   = read_varint(data, pos)? as usize;

    // Segwit marker detection: if in_count == 0, this is a segwit tx
    // segwit: [version(4)] [marker=0x00] [flag=0x01] [inputs] [outputs] [witness] [locktime(4)]
    if in_count == 0 {
        // Skip flag byte
        let _flag = read_bytes(data, pos, 1)?;
        return decode_wire_tx_segwit(data, pos, version);
    }

    let mut inputs = Vec::with_capacity(in_count);
    for _ in 0..in_count {
        inputs.push(decode_txin(data, pos)?);
    }

    let out_count  = read_varint(data, pos)? as usize;
    let mut outputs = Vec::with_capacity(out_count);
    for _ in 0..out_count {
        outputs.push(decode_txout(data, pos)?);
    }

    let locktime   = read_u32_le(data, pos)?;
    Ok(WireTx { version, inputs, outputs, locktime })
}

/// Decode segwit transaction (skip witness data).
fn decode_wire_tx_segwit(data: &[u8], pos: &mut usize, version: i32) -> Result<WireTx, SyncError> {
    let in_count  = read_varint(data, pos)? as usize;
    let mut inputs = Vec::with_capacity(in_count);
    for _ in 0..in_count {
        inputs.push(decode_txin(data, pos)?);
    }
    let out_count  = read_varint(data, pos)? as usize;
    let mut outputs = Vec::with_capacity(out_count);
    for _ in 0..out_count {
        outputs.push(decode_txout(data, pos)?);
    }
    // Skip witness stacks (one per input)
    for _ in 0..in_count {
        let stack_items = read_varint(data, pos)? as usize;
        for _ in 0..stack_items {
            let item_len = read_varint(data, pos)? as usize;
            read_bytes(data, pos, item_len)?;
        }
    }
    let locktime   = read_u32_le(data, pos)?;
    Ok(WireTx { version, inputs, outputs, locktime })
}

/// Decode all transactions from block payload (bytes after the 80-byte header).
pub fn decode_block_txns(block_payload: &[u8]) -> Result<Vec<WireTx>, SyncError> {
    if block_payload.len() < 80 {
        return Err(SyncError::InvalidHeader(
            format!("block payload too short: {} bytes", block_payload.len())
        ));
    }
    let mut pos = 80; // skip the 80-byte header
    let tx_count = read_varint(block_payload, &mut pos)? as usize;
    if tx_count == 0 {
        return Ok(vec![]);
    }
    let mut txns = Vec::with_capacity(tx_count);
    for _ in 0..tx_count {
        txns.push(decode_wire_tx(block_payload, &mut pos)?);
    }
    Ok(txns)
}

// ── txid computation ──────────────────────────────────────────────────────────

/// Compute txid = SHA256(SHA256(encoded_tx)).
pub fn wire_txid(tx: &WireTx) -> [u8; 32] {
    let bytes  = encode_wire_tx(tx);
    let first  = Sha256::digest(&bytes);
    let second = Sha256::digest(&first);
    second.into()
}

// ── UtxoSyncDb ────────────────────────────────────────────────────────────────

/// RocksDB for downloaded UTXO state (separate from local chain UTXOs).
pub struct UtxoSyncDb {
    kv:   Kv,
    path: PathBuf,
}

const KEY_UTXO_HEIGHT:   &[u8] = b"meta:utxo_height";
const KEY_UTXO_TIP_HASH: &[u8] = b"meta:utxo_tip_hash";

fn utxo_key(txid: &[u8; 32], vout: u32) -> String {
    format!("utxo:{}:{}", hex::encode(txid), vout)
}

impl UtxoSyncDb {
    pub fn open(path: &Path) -> Result<Self, SyncError> {
        std::fs::create_dir_all(path)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        let kv = Kv::open_rw(path).map_err(SyncError::Db)?;
        Ok(Self { kv, path: path.to_path_buf() })
    }

    /// Open read-only — không giữ write lock, dùng cho pktscan khi sync đang chạy.
    pub fn open_read_only(path: &Path) -> Result<Self, SyncError> {
        std::fs::create_dir_all(path)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        let kv = Kv::open_ro(path).map_err(SyncError::Db)?;
        Ok(Self { kv, path: path.to_path_buf() })
    }

    pub fn open_temp() -> Result<Self, SyncError> {
        let path = std::env::temp_dir()
            .join(format!("pkt_utxodb_test_{}", rand_u64()));
        Self::open(&path)
    }

    // ── UTXO CRUD ────────────────────────────────────────────────────────────

    pub fn insert_utxo(&self, txid: &[u8; 32], vout: u32, out: &WireTxOut, height: u64) -> Result<(), SyncError> {
        let key   = utxo_key(txid, vout);
        let entry = UtxoEntry {
            txid:         hex::encode(txid),
            vout,
            value:        out.value,
            script_pubkey: out.script_pubkey.clone(),
            height,
        };
        let val = serde_json::to_vec(&entry)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        self.kv.put(key.as_bytes(), &val).map_err(SyncError::Db)
    }

    pub fn remove_utxo(&self, txid: &[u8; 32], vout: u32) -> Result<(), SyncError> {
        let key = utxo_key(txid, vout);
        self.kv.delete(key.as_bytes()).map_err(SyncError::Db)
    }

    pub fn get_utxo(&self, txid: &[u8; 32], vout: u32) -> Result<Option<UtxoEntry>, SyncError> {
        let key = utxo_key(txid, vout);
        match self.kv.get(key.as_bytes()).map_err(SyncError::Db)? {
            None    => Ok(None),
            Some(v) => {
                let entry: UtxoEntry = serde_json::from_slice(&v)
                    .map_err(|e| SyncError::Db(e.to_string()))?;
                Ok(Some(entry))
            }
        }
    }

    // ── Height / tip tracking ─────────────────────────────────────────────────

    pub fn get_utxo_height(&self) -> Result<Option<u64>, SyncError> {
        match self.kv.get(KEY_UTXO_HEIGHT).map_err(SyncError::Db)? {
            None => Ok(None),
            Some(v) => {
                let s = std::str::from_utf8(&v).map_err(|e| SyncError::Db(e.to_string()))?;
                let h = s.parse::<u64>().map_err(|e| SyncError::Db(e.to_string()))?;
                Ok(Some(h))
            }
        }
    }

    pub fn set_utxo_height(&self, height: u64) -> Result<(), SyncError> {
        self.kv.put(KEY_UTXO_HEIGHT, height.to_string().as_bytes())
            .map_err(SyncError::Db)
    }

    pub fn get_tip_hash(&self) -> Result<Option<[u8; 32]>, SyncError> {
        match self.kv.get(KEY_UTXO_TIP_HASH).map_err(SyncError::Db)? {
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
        self.kv.put(KEY_UTXO_TIP_HASH, hash.as_ref())
            .map_err(SyncError::Db)
    }

    // ── Stats ─────────────────────────────────────────────────────────────────

    pub fn count_utxos(&self) -> Result<u64, SyncError> {
        let count = self.kv.scan_prefix(b"utxo:").len();
        Ok(count as u64)
    }

    /// Sum of all UTXO values in this DB.
    pub fn total_value(&self) -> Result<u64, SyncError> {
        let total = self.kv.scan_prefix(b"utxo:")
            .into_iter()
            .filter_map(|(_, v)| serde_json::from_slice::<UtxoEntry>(&v).ok())
            .fold(0u64, |acc, e| acc.saturating_add(e.value));
        Ok(total)
    }

    pub fn path(&self) -> &Path { &self.path }

    /// Scan tất cả unspent outputs của một txid.
    /// Key prefix: "utxo:{txid_hex}:"
    pub fn scan_tx_outputs(&self, txid_hex: &str) -> Vec<UtxoEntry> {
        let prefix = format!("utxo:{}:", txid_hex);
        self.kv.scan_prefix(prefix.as_bytes())
            .into_iter()
            .filter_map(|(_, v)| serde_json::from_slice::<UtxoEntry>(&v).ok())
            .collect()
    }

    /// Raw KV access for iteration (used by explorer queries in pkt_explorer_api).
    pub fn raw_kv(&self) -> &Kv { &self.kv }

    // ── TX meta index (v24.1) ─────────────────────────────────────────────────

    pub fn put_tx_meta(&self, txid_hex: &str, meta: &TxMeta) -> Result<(), SyncError> {
        let key = format!("txmeta:{}", txid_hex);
        let val = serde_json::to_vec(meta).map_err(|e| SyncError::Db(e.to_string()))?;
        self.kv.put(key.as_bytes(), &val).map_err(SyncError::Db)
    }

    pub fn get_tx_meta(&self, txid_hex: &str) -> Result<Option<TxMeta>, SyncError> {
        let key = format!("txmeta:{}", txid_hex);
        match self.kv.get(key.as_bytes()).map_err(SyncError::Db)? {
            None    => Ok(None),
            Some(v) => serde_json::from_slice(&v).map(Some)
                           .map_err(|e| SyncError::Db(e.to_string())),
        }
    }
}

impl Drop for UtxoSyncDb {
    fn drop(&mut self) {
        if self.path.to_string_lossy().contains("pkt_utxodb_test_") {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

// ── UTXO application ──────────────────────────────────────────────────────────

/// Apply one transaction: spend inputs, create outputs, write TxMeta index.
/// Coinbase inputs (null prev_txid) are not spent from the UTXO set.
pub fn apply_wire_tx(
    db:     &UtxoSyncDb,
    tx:     &WireTx,
    txid:   &[u8; 32],
    height: u64,
) -> Result<(), SyncError> {
    let is_coinbase = tx.is_coinbase();
    let size        = encode_wire_tx(tx).len() as u32;

    // Spend inputs — đọc value trước khi xóa để tính fee
    let mut input_sum: u64 = 0;
    for inp in &tx.inputs {
        if !inp.is_coinbase() {
            if let Ok(Some(utxo)) = db.get_utxo(&inp.prev_txid, inp.prev_vout) {
                input_sum = input_sum.saturating_add(utxo.value);
            }
            db.remove_utxo(&inp.prev_txid, inp.prev_vout)?;
        }
    }

    // Create outputs
    let mut output_sum: u64 = 0;
    for (vout, out) in tx.outputs.iter().enumerate() {
        output_sum = output_sum.saturating_add(out.value);
        db.insert_utxo(txid, vout as u32, out, height)?;
    }

    // Tính fee_rate (msat/vByte); 0 cho coinbase hoặc không đủ data
    let fee_rate_msat_vb = if is_coinbase || input_sum == 0 || size == 0 {
        0
    } else {
        let fee = input_sum.saturating_sub(output_sum);
        fee.saturating_mul(1000) / size as u64
    };

    // Ghi TX index
    let txid_hex = hex::encode(txid);
    db.put_tx_meta(&txid_hex, &TxMeta { height, size, fee_rate_msat_vb, is_coinbase })?;

    Ok(())
}

/// Apply all transactions in a block to the UTXO set.
/// Persists `height` and `tip_hash` after success.
pub fn apply_block_txns(
    db:       &UtxoSyncDb,
    txns:     &[WireTx],
    height:   u64,
    tip_hash: &[u8; 32],
) -> Result<(), SyncError> {
    for tx in txns {
        let txid = wire_txid(tx);
        apply_wire_tx(db, tx, &txid, height)?;
    }
    db.set_utxo_height(height)?;
    db.set_tip_hash(tip_hash)?;
    Ok(())
}

// ── Resume sync ───────────────────────────────────────────────────────────────

/// Result of a UTXO sync (applied blocks count).
#[derive(Debug)]
pub struct UtxoSyncResult {
    pub blocks_applied: u64,
    pub final_height:   u64,
    pub total_utxos:    u64,
    pub total_value:    u64,
}

/// Apply a list of (height, txns, header_hash) to the UTXO DB, skipping already-applied.
///
/// `resume_from` is the last height already applied (from `db.get_utxo_height()`).
/// Blocks at height ≤ resume_from are skipped.
pub fn sync_utxos(
    db:          &UtxoSyncDb,
    blocks:      &[(u64, Vec<WireTx>, [u8; 32])],  // (height, txns, header_hash)
    resume_from: Option<u64>,
) -> Result<UtxoSyncResult, SyncError> {
    let skip_until = resume_from.unwrap_or(0);
    let mut applied = 0u64;
    let mut final_h = skip_until;

    for (height, txns, tip_hash) in blocks {
        if *height <= skip_until { continue; }
        apply_block_txns(db, txns, *height, tip_hash)?;
        applied += 1;
        final_h  = *height;
    }

    Ok(UtxoSyncResult {
        blocks_applied: applied,
        final_height:   final_h,
        total_utxos:    db.count_utxos()?,
        total_value:    db.total_value()?,
    })
}

// ── Stats formatting ──────────────────────────────────────────────────────────

pub fn format_utxo_stats(r: &UtxoSyncResult) -> String {
    format!(
        "applied={} height={} utxos={} value={} sat",
        r.blocks_applied, r.final_height, r.total_utxos, r.total_value
    )
}

// ── CLI ───────────────────────────────────────────────────────────────────────

pub fn cmd_utxo_sync(args: &[String]) {
    if args.first().map(|s| s.as_str()) == Some("--help") {
        println!();
        println!("  cargo run -- utxosync [--height]");
        println!();
        println!("  Hiển thị trạng thái UTXO sync từ testnet headers.");
        println!("  Chạy sau `cargo run -- sync` để download headers trước.");
        println!();
        return;
    }

    let db_path = std::env::temp_dir().join("pkt_utxo_status");
    match UtxoSyncDb::open(&db_path) {
        Ok(db) => {
            let height = db.get_utxo_height().ok().flatten();
            let count  = db.count_utxos().unwrap_or(0);
            let total  = db.total_value().unwrap_or(0);
            match height {
                Some(h) => println!("[utxosync] height={} utxos={} value={} sat", h, count, total),
                None    => println!("[utxosync] không có dữ liệu — chạy sync trước"),
            }
        }
        Err(e) => eprintln!("[utxosync] lỗi: {}", e),
    }
}

fn rand_u64() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    // Mix with thread id to avoid collision in parallel tests
    std::thread::current().id().hash(&mut h);
    h.finish()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test helpers ─────────────────────────────────────────────────────────

    fn coinbase_tx(value: u64) -> WireTx {
        WireTx {
            version:  1,
            inputs:   vec![WireTxIn {
                prev_txid:  [0u8; 32],
                prev_vout:  0xffff_ffff,
                script_sig: vec![0x03, 0x01, 0x00, 0x00], // height=1 push
                sequence:   0xffff_ffff,
            }],
            outputs:  vec![WireTxOut {
                value,
                script_pubkey: vec![0x76, 0xa9, 0x14], // P2PKH prefix (test)
            }],
            locktime: 0,
        }
    }

    fn spend_tx(prev_txid: [u8; 32], prev_vout: u32, value: u64) -> WireTx {
        WireTx {
            version:  1,
            inputs:   vec![WireTxIn {
                prev_txid,
                prev_vout,
                script_sig: vec![],
                sequence:   0xffff_ffff,
            }],
            outputs:  vec![WireTxOut {
                value,
                script_pubkey: vec![0x51], // OP_1 (anyone can spend, test only)
            }],
            locktime: 0,
        }
    }

    // ── WireTxIn tests ────────────────────────────────────────────────────────

    #[test]
    fn test_wiretxin_coinbase_detection() {
        let inp = WireTxIn {
            prev_txid:  [0u8; 32],
            prev_vout:  0xffff_ffff,
            script_sig: vec![],
            sequence:   0,
        };
        assert!(inp.is_coinbase());
    }

    #[test]
    fn test_wiretxin_not_coinbase_nonzero_txid() {
        let inp = WireTxIn {
            prev_txid:  [1u8; 32],
            prev_vout:  0,
            script_sig: vec![],
            sequence:   0,
        };
        assert!(!inp.is_coinbase());
    }

    #[test]
    fn test_wiretxin_not_coinbase_wrong_vout() {
        let inp = WireTxIn {
            prev_txid:  [0u8; 32],
            prev_vout:  0,    // vout=0, not 0xffffffff
            script_sig: vec![],
            sequence:   0,
        };
        assert!(!inp.is_coinbase());
    }

    #[test]
    fn test_wiretx_is_coinbase() {
        assert!(coinbase_tx(1000).is_coinbase());
    }

    #[test]
    fn test_wiretx_not_coinbase() {
        assert!(!spend_tx([1u8; 32], 0, 500).is_coinbase());
    }

    // ── Encode/decode roundtrip tests ─────────────────────────────────────────

    #[test]
    fn test_encode_decode_coinbase_tx() {
        let tx  = coinbase_tx(50_000_000);
        let raw = encode_wire_tx(&tx);
        let mut pos = 0;
        let decoded = decode_wire_tx(&raw, &mut pos).unwrap();

        assert_eq!(decoded.version,               tx.version);
        assert_eq!(decoded.locktime,              tx.locktime);
        assert_eq!(decoded.inputs.len(),          1);
        assert_eq!(decoded.inputs[0].prev_txid,   [0u8; 32]);
        assert_eq!(decoded.inputs[0].prev_vout,   0xffff_ffff);
        assert_eq!(decoded.outputs.len(),         1);
        assert_eq!(decoded.outputs[0].value,      50_000_000);
    }

    #[test]
    fn test_encode_decode_spend_tx() {
        let prev = [0xabu8; 32];
        let tx   = spend_tx(prev, 2, 999_000);
        let raw  = encode_wire_tx(&tx);
        let mut pos = 0;
        let decoded = decode_wire_tx(&raw, &mut pos).unwrap();

        assert_eq!(decoded.inputs[0].prev_txid, prev);
        assert_eq!(decoded.inputs[0].prev_vout, 2);
        assert_eq!(decoded.outputs[0].value,    999_000);
    }

    #[test]
    fn test_encode_decode_multi_output() {
        let tx = WireTx {
            version:  1,
            inputs:   vec![WireTxIn {
                prev_txid:  [0u8; 32],
                prev_vout:  0xffff_ffff,
                script_sig: vec![],
                sequence:   0xffff_ffff,
            }],
            outputs:  vec![
                WireTxOut { value: 1000, script_pubkey: vec![0x51] },
                WireTxOut { value: 2000, script_pubkey: vec![0x52] },
                WireTxOut { value: 3000, script_pubkey: vec![0x53] },
            ],
            locktime: 0,
        };
        let raw  = encode_wire_tx(&tx);
        let mut pos = 0;
        let dec = decode_wire_tx(&raw, &mut pos).unwrap();
        assert_eq!(dec.outputs.len(), 3);
        assert_eq!(dec.outputs[0].value, 1000);
        assert_eq!(dec.outputs[2].value, 3000);
    }

    #[test]
    fn test_encode_decode_script_preserved() {
        let script = vec![0x76, 0xa9, 0x14, 0xab, 0xcd, 0x88, 0xac];
        let tx = WireTx {
            version:  1,
            inputs:   vec![WireTxIn {
                prev_txid: [0u8; 32], prev_vout: 0xffff_ffff,
                script_sig: script.clone(), sequence: 0xffff_ffff,
            }],
            outputs:  vec![WireTxOut { value: 1, script_pubkey: script.clone() }],
            locktime: 0,
        };
        let raw  = encode_wire_tx(&tx);
        let mut pos = 0;
        let dec  = decode_wire_tx(&raw, &mut pos).unwrap();
        assert_eq!(dec.inputs[0].script_sig,    script);
        assert_eq!(dec.outputs[0].script_pubkey, script);
    }

    #[test]
    fn test_decode_advances_pos() {
        let tx1 = coinbase_tx(100);
        let tx2 = spend_tx([1u8; 32], 0, 50);
        let mut raw = encode_wire_tx(&tx1);
        raw.extend(encode_wire_tx(&tx2));

        let mut pos = 0;
        let d1 = decode_wire_tx(&raw, &mut pos).unwrap();
        let d2 = decode_wire_tx(&raw, &mut pos).unwrap();

        assert!(d1.is_coinbase());
        assert!(!d2.is_coinbase());
        assert_eq!(pos, raw.len());
    }

    // ── wire_txid tests ───────────────────────────────────────────────────────

    #[test]
    fn test_wire_txid_deterministic() {
        let tx  = coinbase_tx(5000);
        let id1 = wire_txid(&tx);
        let id2 = wire_txid(&tx);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_wire_txid_different_txs_differ() {
        let id1 = wire_txid(&coinbase_tx(1000));
        let id2 = wire_txid(&coinbase_tx(2000));
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_wire_txid_not_all_zeros() {
        let id = wire_txid(&coinbase_tx(5000));
        assert_ne!(id, [0u8; 32]);
    }

    // ── UtxoSyncDb tests ──────────────────────────────────────────────────────

    #[test]
    fn test_utxodb_open_temp() {
        assert!(UtxoSyncDb::open_temp().is_ok());
    }

    #[test]
    fn test_utxodb_insert_get() {
        let db  = UtxoSyncDb::open_temp().unwrap();
        let txid = [0x11u8; 32];
        let out  = WireTxOut { value: 5000, script_pubkey: vec![0x51] };
        db.insert_utxo(&txid, 0, &out, 0).unwrap();

        let got = db.get_utxo(&txid, 0).unwrap().unwrap();
        assert_eq!(got.value, 5000);
        assert_eq!(got.vout,  0);
    }

    #[test]
    fn test_utxodb_get_missing() {
        let db = UtxoSyncDb::open_temp().unwrap();
        assert!(db.get_utxo(&[0u8; 32], 0).unwrap().is_none());
    }

    #[test]
    fn test_utxodb_remove() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let txid = [0x22u8; 32];
        let out  = WireTxOut { value: 1000, script_pubkey: vec![] };
        db.insert_utxo(&txid, 1, &out, 0).unwrap();
        db.remove_utxo(&txid, 1).unwrap();
        assert!(db.get_utxo(&txid, 1).unwrap().is_none());
    }

    #[test]
    fn test_utxodb_count_zero() {
        let db = UtxoSyncDb::open_temp().unwrap();
        assert_eq!(db.count_utxos().unwrap(), 0);
    }

    #[test]
    fn test_utxodb_count_after_inserts() {
        let db  = UtxoSyncDb::open_temp().unwrap();
        let out = WireTxOut { value: 100, script_pubkey: vec![] };
        for i in 0..5u8 {
            db.insert_utxo(&[i; 32], 0, &out, 0).unwrap();
        }
        assert_eq!(db.count_utxos().unwrap(), 5);
    }

    #[test]
    fn test_utxodb_total_value() {
        let db  = UtxoSyncDb::open_temp().unwrap();
        for i in 0..3u8 {
            let out = WireTxOut { value: 1000 * (i as u64 + 1), script_pubkey: vec![] };
            db.insert_utxo(&[i; 32], 0, &out, 0).unwrap();
        }
        // 1000 + 2000 + 3000 = 6000
        assert_eq!(db.total_value().unwrap(), 6000);
    }

    #[test]
    fn test_utxodb_height_initially_none() {
        let db = UtxoSyncDb::open_temp().unwrap();
        assert_eq!(db.get_utxo_height().unwrap(), None);
    }

    #[test]
    fn test_utxodb_set_get_height() {
        let db = UtxoSyncDb::open_temp().unwrap();
        db.set_utxo_height(42).unwrap();
        assert_eq!(db.get_utxo_height().unwrap(), Some(42));
    }

    #[test]
    fn test_utxodb_height_overwrites() {
        let db = UtxoSyncDb::open_temp().unwrap();
        db.set_utxo_height(10).unwrap();
        db.set_utxo_height(20).unwrap();
        assert_eq!(db.get_utxo_height().unwrap(), Some(20));
    }

    #[test]
    fn test_utxodb_tip_hash() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let hash = [0xbbu8; 32];
        db.set_tip_hash(&hash).unwrap();
        assert_eq!(db.get_tip_hash().unwrap(), Some(hash));
    }

    // ── apply_wire_tx tests ───────────────────────────────────────────────────

    #[test]
    fn test_apply_coinbase_creates_utxo() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let tx   = coinbase_tx(50_000_000);
        let txid = wire_txid(&tx);
        apply_wire_tx(&db, &tx, &txid, 0).unwrap();

        let got = db.get_utxo(&txid, 0).unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().value, 50_000_000);
    }

    #[test]
    fn test_apply_coinbase_does_not_remove_anything() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let tx   = coinbase_tx(1000);
        let txid = wire_txid(&tx);
        // Insert a dummy UTXO that should NOT be removed (coinbase doesn't spend)
        let dummy = WireTxOut { value: 999, script_pubkey: vec![] };
        db.insert_utxo(&[0u8; 32], 0xffff_ffff, &dummy, 0).unwrap();
        apply_wire_tx(&db, &tx, &txid, 0).unwrap();
        // The dummy UTXO should still be there (coinbase input is skipped)
        assert!(db.get_utxo(&[0u8; 32], 0xffff_ffff).unwrap().is_some());
    }

    #[test]
    fn test_apply_spend_tx_removes_input() {
        let db   = UtxoSyncDb::open_temp().unwrap();

        // First, create a UTXO to spend
        let cb      = coinbase_tx(5000);
        let cb_txid = wire_txid(&cb);
        apply_wire_tx(&db, &cb, &cb_txid, 1).unwrap();
        assert!(db.get_utxo(&cb_txid, 0).unwrap().is_some());

        // Now spend it
        let spend      = spend_tx(cb_txid, 0, 4900);
        let spend_txid = wire_txid(&spend);
        apply_wire_tx(&db, &spend, &spend_txid, 1).unwrap();

        // Input UTXO should be gone
        assert!(db.get_utxo(&cb_txid, 0).unwrap().is_none());
        // New output UTXO should exist
        assert!(db.get_utxo(&spend_txid, 0).unwrap().is_some());
    }

    #[test]
    fn test_apply_spend_creates_new_utxo() {
        let db       = UtxoSyncDb::open_temp().unwrap();
        let cb       = coinbase_tx(10_000);
        let cb_txid  = wire_txid(&cb);
        apply_wire_tx(&db, &cb, &cb_txid, 1).unwrap();

        let sp      = spend_tx(cb_txid, 0, 9000);
        let sp_txid = wire_txid(&sp);
        apply_wire_tx(&db, &sp, &sp_txid, 2).unwrap();

        let got = db.get_utxo(&sp_txid, 0).unwrap().unwrap();
        assert_eq!(got.value, 9000);
    }

    // ── apply_block_txns tests ────────────────────────────────────────────────

    #[test]
    fn test_apply_block_txns_sets_height() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let txns = vec![coinbase_tx(5000)];
        let hash = [0xaau8; 32];
        apply_block_txns(&db, &txns, 10, &hash).unwrap();
        assert_eq!(db.get_utxo_height().unwrap(), Some(10));
    }

    #[test]
    fn test_apply_block_txns_sets_tip_hash() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let hash = [0xddu8; 32];
        apply_block_txns(&db, &[coinbase_tx(1000)], 1, &hash).unwrap();
        assert_eq!(db.get_tip_hash().unwrap(), Some(hash));
    }

    #[test]
    fn test_apply_block_txns_multiple() {
        let db       = UtxoSyncDb::open_temp().unwrap();
        let cb       = coinbase_tx(5000);
        let cb_txid  = wire_txid(&cb);
        let sp       = spend_tx(cb_txid, 0, 4500);
        let txns     = vec![cb, sp];
        apply_block_txns(&db, &txns, 1, &[0u8; 32]).unwrap();

        // Coinbase UTXO spent → gone; spend output → exists
        assert!(db.get_utxo(&cb_txid, 0).unwrap().is_none());
        assert_eq!(db.count_utxos().unwrap(), 1);
    }

    // ── sync_utxos (resume) tests ─────────────────────────────────────────────

    fn make_blocks(n: u64) -> Vec<(u64, Vec<WireTx>, [u8; 32])> {
        (1..=n).map(|h| {
            let txns = vec![coinbase_tx(h * 5000)];
            let hash = [h as u8; 32];
            (h, txns, hash)
        }).collect()
    }

    #[test]
    fn test_sync_utxos_applies_all() {
        let db     = UtxoSyncDb::open_temp().unwrap();
        let blocks = make_blocks(3);
        let r      = sync_utxos(&db, &blocks, None).unwrap();
        assert_eq!(r.blocks_applied, 3);
        assert_eq!(r.final_height, 3);
    }

    #[test]
    fn test_sync_utxos_height_persisted() {
        let db     = UtxoSyncDb::open_temp().unwrap();
        let blocks = make_blocks(5);
        sync_utxos(&db, &blocks, None).unwrap();
        assert_eq!(db.get_utxo_height().unwrap(), Some(5));
    }

    #[test]
    fn test_sync_utxos_resume_skips_applied() {
        let db     = UtxoSyncDb::open_temp().unwrap();
        let blocks = make_blocks(5);

        // First pass: apply blocks 1-3
        sync_utxos(&db, &blocks[..3], None).unwrap();
        assert_eq!(db.get_utxo_height().unwrap(), Some(3));
        let count_after_3 = db.count_utxos().unwrap();

        // Second pass: resume from height=3, apply blocks 4-5
        sync_utxos(&db, &blocks, Some(3)).unwrap();
        assert_eq!(db.get_utxo_height().unwrap(), Some(5));

        // Should have more UTXOs now
        assert!(db.count_utxos().unwrap() >= count_after_3);
    }

    #[test]
    fn test_sync_utxos_resume_zero_blocks_if_uptodate() {
        let db     = UtxoSyncDb::open_temp().unwrap();
        let blocks = make_blocks(3);
        sync_utxos(&db, &blocks, None).unwrap();

        // "Restart" and try to sync same blocks → should skip all
        let r = sync_utxos(&db, &blocks, Some(3)).unwrap();
        assert_eq!(r.blocks_applied, 0);
        assert_eq!(r.final_height, 3);
    }

    #[test]
    fn test_sync_utxos_empty_blocks() {
        let db = UtxoSyncDb::open_temp().unwrap();
        let r  = sync_utxos(&db, &[], None).unwrap();
        assert_eq!(r.blocks_applied, 0);
        assert_eq!(r.final_height, 0);
    }

    #[test]
    fn test_sync_utxos_total_value_grows() {
        let db     = UtxoSyncDb::open_temp().unwrap();
        let blocks = make_blocks(4);
        let r      = sync_utxos(&db, &blocks, None).unwrap();
        // Each block adds 1*5000 + 2*5000 + 3*5000 + 4*5000 = 50000 (coinbase outputs)
        assert!(r.total_value > 0);
    }

    // ── format_utxo_stats tests ───────────────────────────────────────────────

    #[test]
    fn test_format_utxo_stats_contains_height() {
        let r = UtxoSyncResult { blocks_applied: 5, final_height: 10, total_utxos: 8, total_value: 50000 };
        let s = format_utxo_stats(&r);
        assert!(s.contains("10"));
    }

    #[test]
    fn test_format_utxo_stats_contains_value() {
        let r = UtxoSyncResult { blocks_applied: 1, final_height: 1, total_utxos: 1, total_value: 99999 };
        let s = format_utxo_stats(&r);
        assert!(s.contains("99999"));
    }
}
