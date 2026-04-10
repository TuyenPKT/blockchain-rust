#![allow(dead_code)]

/// v5.5 — Storage v2: WAL + Atomic Writes + Crash Recovery
///
/// Vấn đề của storage v1 (v4.2):
///   save_blockchain = save_chain() + save_utxo() — hai DB write riêng biệt
///   Nếu crash giữa save_chain và save_utxo → chain tiến nhưng UTXO cũ → inconsistent
///
/// Giải pháp v5.5:
///   1. WriteBatch  — blocks + UTXOs + difficulty ghi trong 1 atomic batch
///   2. WriteEpoch  — meta:write_epoch chẵn = committed, lẻ = đang write (crash flag)
///   3. ChainHeight — meta:wal_chain_height = height đã confirmed, khác chain.len-1 → repair
///   4. Repair      — nếu detect inconsistency khi startup, rebuild UTXO từ chain trong DB

use std::collections::HashMap;
use std::path::PathBuf;

use rocksdb::{DB, Options, WriteBatch, IteratorMode};

use crate::block::Block;
use crate::transaction::TxOutput;
use crate::utxo::UtxoSet;

// ─── DB path (cùng schema với storage.rs) ─────────────────────────────────────

fn pkt_dir() -> PathBuf {
    // Unix: $HOME  |  Windows: %USERPROFILE%  |  fallback: current dir
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".pkt")
}
fn db_path() -> PathBuf { crate::pkt_paths::data_dir().join("db") }

fn open_db() -> Result<DB, String> {
    let path = db_path();
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let mut opts = Options::default();
    opts.create_if_missing(true);
    DB::open(&opts, &path).map_err(|e| e.to_string())
}

// ─── Key schema (phải khớp với storage.rs) ────────────────────────────────────

fn block_key(height: u64) -> String { format!("block:{:016x}", height) }
fn utxo_key(k: &str)       -> String { format!("utxo:{}", k) }

const META_HEIGHT:        &[u8] = b"meta:height";
const META_DIFFICULTY:    &[u8] = b"meta:difficulty";
const META_WRITE_EPOCH:   &[u8] = b"meta:write_epoch";   // v5.5: crash detection
const META_WAL_HEIGHT:    &[u8] = b"meta:wal_height";    // v5.5: last confirmed height

// ─── Recovery Status ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryStatus {
    /// DB nhất quán, không cần repair
    Ok,
    /// Phát hiện write chưa hoàn thành — đã repair UTXO từ chain
    Repaired { from_height: u64, to_height: u64 },
    /// DB mới, chưa có data
    Fresh,
}

// ─── Write Epoch helpers ──────────────────────────────────────────────────────

fn read_epoch(db: &DB) -> u64 {
    db.get(META_WRITE_EPOCH).ok().flatten()
        .and_then(|v| std::str::from_utf8(&v).ok().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

fn read_wal_height(db: &DB) -> Option<u64> {
    db.get(META_WAL_HEIGHT).ok().flatten()
        .and_then(|v| std::str::from_utf8(&v).ok().and_then(|s| s.parse().ok()))
}

fn is_write_in_progress(db: &DB) -> bool {
    read_epoch(db) % 2 != 0
}

// ─── Atomic Save ──────────────────────────────────────────────────────────────

/// Lưu Blockchain vào DB trong 1 atomic WriteBatch.
/// Thay thế save_blockchain trong storage.rs: không bao giờ bị split giữa chừng.
pub fn atomic_save(bc: &crate::chain::Blockchain) -> Result<(), String> {
    let db = open_db()?;

    let epoch = read_epoch(&db);

    // ── Phase 1: Đánh dấu đang write (epoch lẻ) ──────────────────────────────
    db.put(META_WRITE_EPOCH, (epoch + 1).to_string().as_bytes())
        .map_err(|e| e.to_string())?;

    // ── Phase 2: Build WriteBatch — tất cả keys trong 1 batch ────────────────
    let mut batch = WriteBatch::default();

    // Blocks
    for block in &bc.chain {
        let key = block_key(block.index);
        let val = serde_json::to_vec(block)
            .map_err(|e| format!("serialize block {}: {e}", block.index))?;
        batch.put(key.as_bytes(), &val);
    }

    // Xóa UTXOs cũ + ghi mới trong cùng batch
    let old_utxo_keys: Vec<Vec<u8>> = db.iterator(IteratorMode::Start)
        .filter_map(|item| item.ok().and_then(|(k, _)| {
            if std::str::from_utf8(&k).unwrap_or("").starts_with("utxo:") {
                Some(k.to_vec())
            } else { None }
        }))
        .collect();
    for k in old_utxo_keys {
        batch.delete(&k);
    }
    for (k, output) in &bc.utxo_set.utxos {
        let key = utxo_key(k);
        let val = serde_json::to_vec(output)
            .map_err(|e| format!("serialize utxo {k}: {e}"))?;
        batch.put(key.as_bytes(), &val);
    }

    // Metadata
    if let Some(last) = bc.chain.last() {
        batch.put(META_HEIGHT,    last.index.to_string().as_bytes());
        batch.put(META_WAL_HEIGHT, last.index.to_string().as_bytes());
    }
    batch.put(META_DIFFICULTY, bc.difficulty.to_string().as_bytes());
    // Epoch chẵn = committed
    batch.put(META_WRITE_EPOCH, (epoch + 2).to_string().as_bytes());

    // ── Phase 3: Atomic commit ────────────────────────────────────────────────
    db.write(batch).map_err(|e| format!("atomic write failed: {e}"))
}

// ─── Crash Recovery ───────────────────────────────────────────────────────────

/// Kiểm tra DB sau khi load. Nếu phát hiện write chưa hoàn thành hoặc
/// UTXO không khớp với chain, rebuild UTXO từ chain đã lưu.
pub fn check_and_recover(bc: &mut crate::chain::Blockchain) -> RecoveryStatus {
    if bc.chain.is_empty() {
        return RecoveryStatus::Fresh;
    }

    let db = match open_db() {
        Ok(d)  => d,
        Err(_) => return RecoveryStatus::Fresh,
    };

    let chain_tip = bc.chain.last().unwrap().index;

    // Kiểm tra 1: Write epoch lẻ = crash giữa chừng
    let crash_detected = is_write_in_progress(&db);

    // Kiểm tra 2: wal_height < chain_tip = blocks ghi xong nhưng UTXO chưa
    let wal_height = read_wal_height(&db).unwrap_or(0);
    let utxo_stale = wal_height < chain_tip;

    if !crash_detected && !utxo_stale {
        return RecoveryStatus::Ok;
    }

    let repair_from = wal_height;
    println!("  🔧 WAL: detect inconsistency (epoch_odd={}, wal_height={}, chain_tip={})",
        crash_detected, wal_height, chain_tip);
    println!("  🔧 WAL: rebuilding UTXO from block #{} → #{}", repair_from + 1, chain_tip);

    // Rebuild UTXO bằng cách replay từ đầu (đơn giản nhất, đúng nhất)
    bc.utxo_set = rebuild_utxo_from_chain(&bc.chain);

    // Reset epoch về chẵn sau khi repair
    let _ = db.put(META_WRITE_EPOCH, b"0");
    let _ = db.put(META_WAL_HEIGHT, chain_tip.to_string().as_bytes());

    println!("  ✅ WAL: repair complete — {} UTXOs rebuilt", bc.utxo_set.utxos.len());
    RecoveryStatus::Repaired { from_height: repair_from, to_height: chain_tip }
}

/// Rebuild UTXO set bằng cách replay toàn bộ chain (bỏ qua genesis)
fn rebuild_utxo_from_chain(chain: &[Block]) -> UtxoSet {
    let mut utxo_set = UtxoSet::new();
    for block in chain.iter().skip(1) { // skip genesis (empty block)
        utxo_set.apply_block(&block.transactions);
    }
    utxo_set
}

// ─── Diagnostics ──────────────────────────────────────────────────────────────

/// Thông tin trạng thái WAL (dùng cho metrics/debug)
pub struct WalStatus {
    pub write_epoch:     u64,
    pub is_clean:        bool,  // epoch chẵn = no crash
    pub wal_height:      Option<u64>,
    pub db_exists:       bool,
}

pub fn wal_status() -> WalStatus {
    if !db_path().exists() {
        return WalStatus { write_epoch: 0, is_clean: true, wal_height: None, db_exists: false };
    }
    match open_db() {
        Ok(db) => {
            let epoch = read_epoch(&db);
            WalStatus {
                write_epoch: epoch,
                is_clean:    epoch % 2 == 0,
                wal_height:  read_wal_height(&db),
                db_exists:   true,
            }
        }
        Err(_) => WalStatus { write_epoch: 0, is_clean: true, wal_height: None, db_exists: true },
    }
}

/// Load tất cả blocks từ DB (dùng nội bộ cho recovery, không duplicate với storage.rs)
pub fn load_blocks_for_recovery() -> Result<Option<Vec<Block>>, String> {
    if !db_path().exists() { return Ok(None); }
    let db = open_db()?;
    let mut blocks: Vec<Block> = Vec::new();
    for item in db.iterator(IteratorMode::Start) {
        let (key, val) = item.map_err(|e| e.to_string())?;
        if std::str::from_utf8(&key).unwrap_or("").starts_with("block:") {
            let block: Block = serde_json::from_slice(&val)
                .map_err(|e| format!("deserialize: {e}"))?;
            blocks.push(block);
        }
    }
    if blocks.is_empty() { return Ok(None); }
    blocks.sort_by_key(|b| b.index);
    Ok(Some(blocks))
}

/// Load UTXOs từ DB (dùng nội bộ)
pub fn load_utxos_for_recovery() -> HashMap<String, TxOutput> {
    if !db_path().exists() { return HashMap::new(); }
    let db = match open_db() { Ok(d) => d, Err(_) => return HashMap::new() };
    let mut utxos = HashMap::new();
    for item in db.iterator(IteratorMode::Start) {
        let Ok((key, val)) = item else { continue };
        if let Some(utxo_k) = std::str::from_utf8(&key).ok().and_then(|s| s.strip_prefix("utxo:")) {
            if let Ok(output) = serde_json::from_slice::<TxOutput>(&val) {
                utxos.insert(utxo_k.to_string(), output);
            }
        }
    }
    utxos
}
