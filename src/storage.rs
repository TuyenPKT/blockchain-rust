#![allow(dead_code)]

/// v4.2.1 — Persistent Storage (RocksDB backend)
///
/// Thay thế JSON files bằng RocksDB embedded key-value store.
/// RocksDB: production-grade, LSM-tree, atomic batch writes, compaction.
///
/// DB path: ~/.pkt/db/
///
/// Key schema:
///   block:{height:016x}  → serde_json bytes of Block   (zero-padded hex → lexicographic sort)
///   utxo:{txid}:{index}  → serde_json bytes of TxOutput
///   meta:height          → current tip height (decimal string)
///
/// Public API không thay đổi so với v4.2 (JSON) — caller không cần sửa.

use std::collections::HashMap;
use std::path::PathBuf;

use rocksdb::{DB, Options, IteratorMode};

use crate::block::Block;
use crate::transaction::TxOutput;

// ─── DB path ──────────────────────────────────────────────────────────────────

fn pkt_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".pkt")
}

fn db_path() -> PathBuf { pkt_dir().join("db") }

fn open_db() -> Result<DB, String> {
    let path = db_path();
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let mut opts = Options::default();
    opts.create_if_missing(true);
    DB::open(&opts, &path).map_err(|e| e.to_string())
}

// ─── Key helpers ──────────────────────────────────────────────────────────────

fn block_key(height: u64) -> String { format!("block:{:016x}", height) }
fn utxo_key(k: &str)       -> String { format!("utxo:{}", k) }
const META_HEIGHT:     &[u8] = b"meta:height";
const META_DIFFICULTY: &[u8] = b"meta:difficulty";

// ─── Chain storage ────────────────────────────────────────────────────────────

/// Lưu tất cả blocks vào RocksDB
pub fn save_chain(blocks: &[Block]) -> Result<(), String> {
    let db = open_db()?;
    for block in blocks {
        let key = block_key(block.index);
        let val = serde_json::to_vec(block)
            .map_err(|e| format!("serialize block {}: {e}", block.index))?;
        db.put(key.as_bytes(), &val).map_err(|e| e.to_string())?;
    }
    // Ghi tip height vào meta
    if let Some(last) = blocks.last() {
        db.put(META_HEIGHT, last.index.to_string().as_bytes())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Load tất cả blocks từ RocksDB, sắp xếp theo height
pub fn load_chain() -> Result<Option<Vec<Block>>, String> {
    if !db_path().exists() { return Ok(None); }
    let db = open_db()?;
    let mut blocks: Vec<Block> = Vec::new();

    for item in db.iterator(IteratorMode::Start) {
        let (key, val) = item.map_err(|e| e.to_string())?;
        let key_str = std::str::from_utf8(&key).unwrap_or("");
        if key_str.starts_with("block:") {
            let block: Block = serde_json::from_slice(&val)
                .map_err(|e| format!("deserialize block: {e}"))?;
            blocks.push(block);
        }
    }

    if blocks.is_empty() { return Ok(None); }
    blocks.sort_by_key(|b| b.index);
    Ok(Some(blocks))
}

// ─── UTXO storage ─────────────────────────────────────────────────────────────

/// Lưu toàn bộ UTXO set vào RocksDB
pub fn save_utxo(utxos: &HashMap<String, TxOutput>) -> Result<(), String> {
    let db = open_db()?;

    // Xóa toàn bộ utxo cũ trước khi ghi mới (clean write)
    let old_keys: Vec<Vec<u8>> = db.iterator(IteratorMode::Start)
        .filter_map(|item| {
            item.ok().and_then(|(k, _)| {
                if std::str::from_utf8(&k).unwrap_or("").starts_with("utxo:") {
                    Some(k.to_vec())
                } else { None }
            })
        })
        .collect();
    for k in old_keys {
        db.delete(&k).map_err(|e| e.to_string())?;
    }

    // Ghi UTXO mới
    for (k, output) in utxos {
        let key = utxo_key(k);
        let val = serde_json::to_vec(output)
            .map_err(|e| format!("serialize utxo {k}: {e}"))?;
        db.put(key.as_bytes(), &val).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Load toàn bộ UTXO set từ RocksDB
pub fn load_utxo() -> Result<Option<HashMap<String, TxOutput>>, String> {
    if !db_path().exists() { return Ok(None); }
    let db = open_db()?;
    let mut utxos: HashMap<String, TxOutput> = HashMap::new();

    for item in db.iterator(IteratorMode::Start) {
        let (key, val) = item.map_err(|e| e.to_string())?;
        let key_str = std::str::from_utf8(&key).unwrap_or("");
        if let Some(utxo_key) = key_str.strip_prefix("utxo:") {
            let output: TxOutput = serde_json::from_slice(&val)
                .map_err(|e| format!("deserialize utxo: {e}"))?;
            utxos.insert(utxo_key.to_string(), output);
        }
    }

    if utxos.is_empty() { return Ok(None); }
    Ok(Some(utxos))
}

// ─── Snapshot ────────────────────────────────────────────────────────────────

/// Lưu chain + UTXO vào DB (gọi sau mỗi block mới)
pub fn save_snapshot(
    blocks: &[Block],
    utxos:  &HashMap<String, TxOutput>,
) -> Result<(), String> {
    save_chain(blocks)?;
    save_utxo(utxos)?;
    Ok(())
}

/// Thông tin về DB hiện tại
pub struct SnapshotInfo {
    pub chain_height: usize,
    pub utxo_count:   usize,
    pub db_path:      PathBuf,
}

pub fn snapshot_info() -> Result<Option<SnapshotInfo>, String> {
    if !db_path().exists() { return Ok(None); }
    let blocks = load_chain()?.unwrap_or_default();
    let utxos  = load_utxo()?.unwrap_or_default();
    Ok(Some(SnapshotInfo {
        chain_height: blocks.len().saturating_sub(1),
        utxo_count:   utxos.len(),
        db_path:      db_path(),
    }))
}

// ─── Integration ─────────────────────────────────────────────────────────────

use crate::chain::Blockchain;
use crate::utxo::UtxoSet;

/// Load snapshot vào Blockchain struct. Nếu không có → genesis.
pub fn load_or_new() -> Blockchain {
    match try_load_blockchain() {
        Ok(Some(mut bc)) => {
            // v5.5: Kiểm tra và repair nếu phát hiện crash mid-write
            crate::wal::check_and_recover(&mut bc);
            println!(
                "  📦 Loaded from RocksDB: height={}, utxos={}",
                bc.chain.len() - 1,
                bc.utxo_set.utxos.len()
            );
            bc
        }
        Ok(None) => {
            println!("  🌱 No DB found — starting fresh (genesis)");
            Blockchain::new()
        }
        Err(e) => {
            eprintln!("  ⚠️  DB load failed: {} — starting fresh", e);
            Blockchain::new()
        }
    }
}

fn try_load_blockchain() -> Result<Option<Blockchain>, String> {
    let blocks = match load_chain()? {
        Some(b) => b,
        None    => return Ok(None),
    };
    let utxo_map = load_utxo()?.unwrap_or_default();
    let mut utxo_set = UtxoSet::new();
    utxo_set.utxos = utxo_map;

    // Load difficulty từ DB; nếu chưa có (DB cũ) → tính lại từ chain
    let difficulty = {
        let db = open_db()?;
        match db.get(META_DIFFICULTY).map_err(|e| e.to_string())? {
            Some(v) => std::str::from_utf8(&v).ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(3),
            None => recalculate_difficulty(&blocks),
        }
    };

    let fee_estimator = crate::fee_market::FeeEstimator::rebuild_from_blocks(&blocks);
    Ok(Some(Blockchain {
        chain: blocks,
        difficulty,
        utxo_set,
        mempool:       crate::mempool::Mempool::new(),
        fee_estimator,
    }))
}

/// Tính lại difficulty từ lịch sử chain (migration từ DB cũ không lưu difficulty)
fn recalculate_difficulty(blocks: &[crate::block::Block]) -> usize {
    // Đếm số leading zeros trong hash của các block gần nhất
    let recent = blocks.iter().rev().take(10);
    let avg_zeros = recent
        .map(|b| b.hash.chars().take_while(|&c| c == '0').count())
        .max()
        .unwrap_or(3);
    avg_zeros.max(3)
}

/// Lưu Blockchain snapshot — v5.5: dùng atomic WriteBatch qua wal::atomic_save
pub fn save_blockchain(bc: &Blockchain) -> Result<(), String> {
    crate::wal::atomic_save(bc)
}

// ─── Utility ─────────────────────────────────────────────────────────────────

/// Xóa toàn bộ DB (dùng cho tests và hard reset)
pub fn reset_storage() -> Result<(), String> {
    let path = db_path();
    if path.exists() {
        // DB::destroy xóa đúng cách (bao gồm manifest, WAL, SST files)
        DB::destroy(&Options::default(), &path)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Kích thước toàn bộ DB directory (bytes)
pub fn storage_size_bytes() -> u64 {
    dir_size(&db_path())
}

fn dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0; };
    entries.flatten().map(|e| {
        let p = e.path();
        if p.is_dir() { dir_size(&p) } else {
            std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0)
        }
    }).sum()
}
