#![allow(dead_code)]
//! v23.7 — UTXO Snapshot
//!
//! Dump/load toàn bộ UTXO set thành file NDJSON để bootstrap nhanh không cần IBD.
//!
//! ## File format (NDJSON — newline-delimited JSON)
//! ```text
//! {"version":1,"height":12345,"tip_hash":"aabb..","utxo_count":50000,"created_at_unix":1700000000}
//! {"txid":"aa..","vout":0,"value":1000000,"script_pubkey":[118,169,...],"height":1000}
//! {"txid":"bb..","vout":1,"value":500000,"script_pubkey":[118,169,...],"height":1001}
//! ...
//! ```
//!
//! ## CLI
//! ```bash
//! blockchain-rust snapshot dump [output.ndjson]   # dump từ ~/.pkt/utxodb
//! blockchain-rust snapshot load <input.ndjson>    # load vào ~/.pkt/utxodb
//! blockchain-rust snapshot info <file.ndjson>     # xem header của snapshot
//! ```

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::pkt_sync::SyncError;
use crate::pkt_utxo_sync::{UtxoEntry, UtxoSyncDb, WireTxOut};

// ── Snapshot header ────────────────────────────────────────────────────────────

/// Dòng đầu tiên của snapshot file — chứa metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotHeader {
    pub version:       u8,
    pub height:        u64,
    pub tip_hash:      String,   // hex display (reversed SHA256d)
    pub utxo_count:    u64,
    pub created_at_unix: u64,
}

impl SnapshotHeader {
    fn new(height: u64, tip_hash: String, utxo_count: u64) -> Self {
        let created_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        SnapshotHeader { version: 1, height, tip_hash, utxo_count, created_at_unix }
    }
}

// ── Default paths ──────────────────────────────────────────────────────────────

pub fn default_utxo_db_path() -> PathBuf {
    crate::pkt_testnet_web::default_utxo_db_path()
}

fn default_snapshot_path(height: u64) -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".pkt")
        .join(format!("utxo_snapshot_{}.ndjson", height))
}

// ── Dump ──────────────────────────────────────────────────────────────────────

/// Dump toàn bộ UTXO set từ `utxo_db` ra file NDJSON tại `output_path`.
/// Dòng đầu = `SnapshotHeader`, mỗi dòng sau = `UtxoEntry` JSON.
pub fn dump_snapshot(utxo_db: &UtxoSyncDb, output_path: &Path) -> Result<SnapshotHeader, SyncError> {
    use rocksdb::{Direction, IteratorMode};

    let height   = utxo_db.get_utxo_height()?.unwrap_or(0);
    let tip_raw  = utxo_db.get_tip_hash()?;
    let tip_hash = match tip_raw {
        Some(h) => { let mut b = h; b.reverse(); hex::encode(b) }
        None    => "0".repeat(64),
    };
    let utxo_count = utxo_db.count_utxos()?;

    let header = SnapshotHeader::new(height, tip_hash, utxo_count);

    let file   = File::create(output_path).map_err(|e| SyncError::Db(e.to_string()))?;
    let mut w  = BufWriter::new(file);

    // Dòng 1: header
    let header_json = serde_json::to_string(&header)
        .map_err(|e| SyncError::Db(e.to_string()))?;
    writeln!(w, "{}", header_json).map_err(|e| SyncError::Db(e.to_string()))?;

    // Dòng 2+: UTXOs, scan prefix "utxo:"
    let db   = utxo_db.raw_db();
    let iter = db.iterator(IteratorMode::From(b"utxo:", Direction::Forward));
    let mut written = 0u64;

    for item in iter {
        let (k, v) = item.map_err(|e| SyncError::Db(e.to_string()))?;
        if !k.starts_with(b"utxo:") { break; }
        let entry: UtxoEntry = serde_json::from_slice(&v)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        let line = serde_json::to_string(&entry)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        writeln!(w, "{}", line).map_err(|e| SyncError::Db(e.to_string()))?;
        written += 1;
    }

    w.flush().map_err(|e| SyncError::Db(e.to_string()))?;

    // Trả về header với utxo_count thực tế (count_utxos có thể sai nếu meta keys lọt vào)
    Ok(SnapshotHeader { utxo_count: written, ..header })
}

// ── Load ──────────────────────────────────────────────────────────────────────

/// Load snapshot từ `input_path` vào `utxo_db`.
/// Ghi đè dữ liệu cũ: xoá tất cả key "utxo:*" trước, sau đó insert từ file.
pub fn load_snapshot(input_path: &Path, utxo_db: &UtxoSyncDb) -> Result<SnapshotHeader, SyncError> {
    use rocksdb::{Direction, IteratorMode, WriteBatch};

    let file   = File::open(input_path).map_err(|e| SyncError::Db(e.to_string()))?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    // Dòng đầu = header
    let header_line = lines.next()
        .ok_or_else(|| SyncError::Db("snapshot file is empty".into()))?
        .map_err(|e| SyncError::Db(e.to_string()))?;
    let header: SnapshotHeader = serde_json::from_str(&header_line)
        .map_err(|e| SyncError::Db(format!("header parse: {}", e)))?;
    if header.version != 1 {
        return Err(SyncError::Db(format!("unsupported snapshot version {}", header.version)));
    }

    // Xoá tất cả key "utxo:*" hiện tại (batch delete)
    let db = utxo_db.raw_db();
    {
        let iter = db.iterator(IteratorMode::From(b"utxo:", Direction::Forward));
        let mut batch = WriteBatch::default();
        for item in iter {
            let (k, _) = item.map_err(|e| SyncError::Db(e.to_string()))?;
            if !k.starts_with(b"utxo:") { break; }
            batch.delete(&k);
        }
        db.write(batch).map_err(|e| SyncError::Db(e.to_string()))?;
    }

    // Insert từng dòng = UtxoEntry
    let mut loaded = 0u64;
    for line_result in lines {
        let line = line_result.map_err(|e| SyncError::Db(e.to_string()))?;
        let line = line.trim();
        if line.is_empty() { continue; }

        let entry: UtxoEntry = serde_json::from_str(line)
            .map_err(|e| SyncError::Db(format!("entry parse: {}", e)))?;

        // Reconstruct WireTxOut để dùng insert_utxo API
        let txid_bytes: [u8; 32] = hex::decode(&entry.txid)
            .map_err(|e| SyncError::Db(e.to_string()))?
            .try_into()
            .map_err(|_| SyncError::Db(format!("txid not 32 bytes: {}", &entry.txid)))?;
        let wire_out = WireTxOut {
            value:         entry.value,
            script_pubkey: entry.script_pubkey.clone(),
        };
        utxo_db.insert_utxo(&txid_bytes, entry.vout, &wire_out, entry.height)?;
        loaded += 1;
    }

    // Khôi phục height + tip_hash từ header
    utxo_db.set_utxo_height(header.height)?;
    if let Ok(tip_bytes) = hex::decode(&header.tip_hash) {
        if tip_bytes.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&tip_bytes);
            arr.reverse(); // từ display format về wire format
            utxo_db.set_tip_hash(&arr)?;
        }
    }

    Ok(SnapshotHeader { utxo_count: loaded, ..header })
}

// ── Info ──────────────────────────────────────────────────────────────────────

/// Đọc chỉ header dòng đầu của snapshot file, không parse toàn bộ.
pub fn snapshot_info(path: &Path) -> Result<SnapshotHeader, SyncError> {
    let file   = File::open(path).map_err(|e| SyncError::Db(e.to_string()))?;
    let reader = BufReader::new(file);
    let first  = reader.lines().next()
        .ok_or_else(|| SyncError::Db("snapshot file is empty".into()))?
        .map_err(|e| SyncError::Db(e.to_string()))?;
    serde_json::from_str(&first)
        .map_err(|e| SyncError::Db(format!("header parse: {}", e)))
}

// ── CLI ───────────────────────────────────────────────────────────────────────

pub fn cmd_snapshot(args: &[String]) {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");
    match subcmd {
        "dump" => {
            let utxo_path = default_utxo_db_path();
            let db = match UtxoSyncDb::open_read_only(&utxo_path) {
                Ok(d)  => d,
                Err(e) => { eprintln!("[snapshot] cannot open utxo db: {}", e); return; }
            };
            let height = db.get_utxo_height().ok().flatten().unwrap_or(0);
            let out_path = args.get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| default_snapshot_path(height));
            println!("[snapshot] dumping UTXO set at height={} → {}", height, out_path.display());
            match dump_snapshot(&db, &out_path) {
                Ok(hdr) => println!("[snapshot] ✅ {} UTXOs saved  ({} bytes)",
                    hdr.utxo_count,
                    std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0)),
                Err(e)  => eprintln!("[snapshot] dump failed: {}", e),
            }
        }
        "load" => {
            let input = match args.get(1) {
                Some(p) => PathBuf::from(p),
                None    => { eprintln!("Usage: snapshot load <file.ndjson>"); return; }
            };
            // Hiển thị info trước khi load
            match snapshot_info(&input) {
                Ok(hdr) => println!("[snapshot] loading  height={} utxos={} tip={}…",
                    hdr.height, hdr.utxo_count, &hdr.tip_hash[..16]),
                Err(e)  => { eprintln!("[snapshot] cannot read header: {}", e); return; }
            }
            let utxo_path = default_utxo_db_path();
            let db = match UtxoSyncDb::open(&utxo_path) {
                Ok(d)  => d,
                Err(e) => { eprintln!("[snapshot] cannot open utxo db: {}", e); return; }
            };
            match load_snapshot(&input, &db) {
                Ok(hdr) => println!("[snapshot] ✅ {} UTXOs loaded at height={}",
                    hdr.utxo_count, hdr.height),
                Err(e)  => eprintln!("[snapshot] load failed: {}", e),
            }
        }
        "info" => {
            let path = match args.get(1) {
                Some(p) => PathBuf::from(p),
                None    => { eprintln!("Usage: snapshot info <file.ndjson>"); return; }
            };
            match snapshot_info(&path) {
                Ok(hdr) => {
                    println!("version:     {}", hdr.version);
                    println!("height:      {}", hdr.height);
                    println!("tip_hash:    {}", hdr.tip_hash);
                    println!("utxo_count:  {}", hdr.utxo_count);
                    println!("created_at:  {}", hdr.created_at_unix);
                    if let Some(size) = std::fs::metadata(&path).ok().map(|m| m.len()) {
                        println!("file_size:   {} bytes ({:.1} MB)", size, size as f64 / 1_048_576.0);
                    }
                }
                Err(e) => eprintln!("[snapshot] info failed: {}", e),
            }
        }
        _ => {
            println!("Usage:");
            println!("  snapshot dump [output.ndjson]   — dump UTXO set từ ~/.pkt/utxodb");
            println!("  snapshot load <file.ndjson>     — load snapshot vào ~/.pkt/utxodb");
            println!("  snapshot info <file.ndjson>     — xem metadata của snapshot file");
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkt_utxo_sync::{UtxoSyncDb, WireTxOut};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn tmp_path(label: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("pkt_snapshot_{}_{}", label, n))
    }

    fn p2pkh_script(hash: u8) -> Vec<u8> {
        let mut s = vec![0x76u8, 0xa9, 0x14];
        s.extend_from_slice(&[hash; 20]);
        s.push(0x88);
        s.push(0xac);
        s
    }

    fn make_db_with_utxos(n: u32) -> (UtxoSyncDb, PathBuf) {
        let path = tmp_path("db");
        let db   = UtxoSyncDb::open(&path).unwrap();
        for i in 0..n {
            let mut txid = [0u8; 32];
            txid[0] = (i & 0xff) as u8;
            txid[1] = ((i >> 8) & 0xff) as u8;
            let out  = WireTxOut { value: 1_000_000 * (i as u64 + 1), script_pubkey: p2pkh_script(i as u8) };
            db.insert_utxo(&txid, i % 4, &out, 100 + i as u64).unwrap();
        }
        db.set_utxo_height(1000 + n as u64).unwrap();
        let mut tip = [0xabu8; 32];
        tip[0] = n as u8;
        db.set_tip_hash(&tip).unwrap();
        (db, path)
    }

    // ── SnapshotHeader ────────────────────────────────────────────────────────

    #[test]
    fn test_header_roundtrip_json() {
        let h = SnapshotHeader { version: 1, height: 42, tip_hash: "aa".repeat(32), utxo_count: 5, created_at_unix: 9999 };
        let json = serde_json::to_string(&h).unwrap();
        let h2: SnapshotHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn test_header_version_1() {
        let h = SnapshotHeader::new(100, "aa".repeat(32), 10);
        assert_eq!(h.version, 1);
    }

    #[test]
    fn test_header_created_at_unix_nonzero() {
        let h = SnapshotHeader::new(1, "bb".repeat(32), 0);
        assert!(h.created_at_unix > 0);
    }

    // ── dump_snapshot ─────────────────────────────────────────────────────────

    #[test]
    fn test_dump_creates_file() {
        let (db, _db_path) = make_db_with_utxos(3);
        let out = tmp_path("dump.ndjson");
        dump_snapshot(&db, &out).unwrap();
        assert!(out.exists());
    }

    #[test]
    fn test_dump_first_line_is_valid_header() {
        let (db, _) = make_db_with_utxos(2);
        let out = tmp_path("dump.ndjson");
        dump_snapshot(&db, &out).unwrap();
        let hdr = snapshot_info(&out).unwrap();
        assert_eq!(hdr.version, 1);
        assert_eq!(hdr.height, 1002); // 1000 + 2
    }

    #[test]
    fn test_dump_utxo_count_matches() {
        let n = 5u32;
        let (db, _) = make_db_with_utxos(n);
        let out = tmp_path("dump.ndjson");
        let hdr = dump_snapshot(&db, &out).unwrap();
        assert_eq!(hdr.utxo_count, n as u64);
    }

    #[test]
    fn test_dump_empty_db_produces_only_header() {
        let path = tmp_path("empty_db");
        let db   = UtxoSyncDb::open(&path).unwrap();
        let out  = tmp_path("empty_dump.ndjson");
        let hdr  = dump_snapshot(&db, &out).unwrap();
        assert_eq!(hdr.utxo_count, 0);
        // File should have exactly 1 line (header)
        let content = std::fs::read_to_string(&out).unwrap();
        assert_eq!(content.lines().count(), 1);
    }

    #[test]
    fn test_dump_line_count_equals_header_plus_utxos() {
        let n = 4u32;
        let (db, _) = make_db_with_utxos(n);
        let out     = tmp_path("lines.ndjson");
        dump_snapshot(&db, &out).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        // 1 header + n utxo lines
        assert_eq!(content.lines().count(), 1 + n as usize);
    }

    #[test]
    fn test_dump_tip_hash_is_hex_64_chars() {
        let (db, _) = make_db_with_utxos(1);
        let out = tmp_path("tip.ndjson");
        let hdr = dump_snapshot(&db, &out).unwrap();
        assert_eq!(hdr.tip_hash.len(), 64);
        assert!(hdr.tip_hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ── snapshot_info ─────────────────────────────────────────────────────────

    #[test]
    fn test_info_reads_header_only() {
        let (db, _) = make_db_with_utxos(10);
        let out = tmp_path("info.ndjson");
        let dumped = dump_snapshot(&db, &out).unwrap();
        let info   = snapshot_info(&out).unwrap();
        assert_eq!(info.height,     dumped.height);
        assert_eq!(info.tip_hash,   dumped.tip_hash);
        assert_eq!(info.utxo_count, dumped.utxo_count);
    }

    #[test]
    fn test_info_empty_file_returns_error() {
        let path = tmp_path("empty.ndjson");
        std::fs::write(&path, b"").unwrap();
        assert!(snapshot_info(&path).is_err());
    }

    #[test]
    fn test_info_nonexistent_file_returns_error() {
        let path = tmp_path("nonexistent.ndjson");
        assert!(snapshot_info(&path).is_err());
    }

    #[test]
    fn test_info_corrupt_header_returns_error() {
        let path = tmp_path("corrupt.ndjson");
        std::fs::write(&path, b"not json\n").unwrap();
        assert!(snapshot_info(&path).is_err());
    }

    // ── load_snapshot ─────────────────────────────────────────────────────────

    #[test]
    fn test_roundtrip_utxo_count() {
        let n = 6u32;
        let (src_db, _) = make_db_with_utxos(n);
        let snap        = tmp_path("roundtrip.ndjson");
        dump_snapshot(&src_db, &snap).unwrap();

        let dst_path = tmp_path("dst_db");
        let dst_db   = UtxoSyncDb::open(&dst_path).unwrap();
        let hdr      = load_snapshot(&snap, &dst_db).unwrap();
        assert_eq!(hdr.utxo_count, n as u64);
        assert_eq!(dst_db.count_utxos().unwrap(), n as u64);
    }

    #[test]
    fn test_roundtrip_height_restored() {
        let (src_db, _) = make_db_with_utxos(3);
        let snap        = tmp_path("height.ndjson");
        dump_snapshot(&src_db, &snap).unwrap();

        let dst_path = tmp_path("dst_height");
        let dst_db   = UtxoSyncDb::open(&dst_path).unwrap();
        load_snapshot(&snap, &dst_db).unwrap();
        assert_eq!(dst_db.get_utxo_height().unwrap(), Some(1003));
    }

    #[test]
    fn test_roundtrip_tip_hash_restored() {
        let (src_db, _) = make_db_with_utxos(2);
        let src_tip     = src_db.get_tip_hash().unwrap().unwrap();
        let snap        = tmp_path("tiphash.ndjson");
        dump_snapshot(&src_db, &snap).unwrap();

        let dst_path = tmp_path("dst_tip");
        let dst_db   = UtxoSyncDb::open(&dst_path).unwrap();
        load_snapshot(&snap, &dst_db).unwrap();
        assert_eq!(dst_db.get_tip_hash().unwrap().unwrap(), src_tip);
    }

    #[test]
    fn test_roundtrip_utxo_values_preserved() {
        let (src_db, _) = make_db_with_utxos(4);
        let src_total   = src_db.total_value().unwrap();
        let snap        = tmp_path("values.ndjson");
        dump_snapshot(&src_db, &snap).unwrap();

        let dst_path = tmp_path("dst_values");
        let dst_db   = UtxoSyncDb::open(&dst_path).unwrap();
        load_snapshot(&snap, &dst_db).unwrap();
        assert_eq!(dst_db.total_value().unwrap(), src_total);
    }

    #[test]
    fn test_load_clears_old_utxos() {
        // dst_db đã có UTXOs cũ → load snapshot → chỉ còn UTXOs từ snapshot
        let (old_db, old_path) = make_db_with_utxos(10);
        let old_snap = tmp_path("old.ndjson");
        dump_snapshot(&old_db, &old_snap).unwrap();
        drop(old_db);

        let (new_db, _) = make_db_with_utxos(3);
        let new_snap = tmp_path("new.ndjson");
        dump_snapshot(&new_db, &new_snap).unwrap();

        // Mở dst_db với dữ liệu cũ (10 UTXOs)
        let dst_db = UtxoSyncDb::open(&old_path).unwrap();
        load_snapshot(&new_snap, &dst_db).unwrap();
        // Sau load phải còn đúng 3 UTXOs
        assert_eq!(dst_db.count_utxos().unwrap(), 3);
    }

    #[test]
    fn test_load_version_mismatch_returns_error() {
        let path = tmp_path("badver.ndjson");
        let hdr  = SnapshotHeader { version: 99, height: 1, tip_hash: "aa".repeat(32), utxo_count: 0, created_at_unix: 0 };
        std::fs::write(&path, format!("{}\n", serde_json::to_string(&hdr).unwrap())).unwrap();
        let dst_path = tmp_path("dst_badver");
        let dst_db   = UtxoSyncDb::open(&dst_path).unwrap();
        assert!(load_snapshot(&path, &dst_db).is_err());
    }

    #[test]
    fn test_load_empty_file_returns_error() {
        let path    = tmp_path("emptyload.ndjson");
        std::fs::write(&path, b"").unwrap();
        let dst_path = tmp_path("dst_emptyload");
        let dst_db   = UtxoSyncDb::open(&dst_path).unwrap();
        assert!(load_snapshot(&path, &dst_db).is_err());
    }

    #[test]
    fn test_load_nonexistent_file_returns_error() {
        let path     = tmp_path("nofile.ndjson");
        let dst_path = tmp_path("dst_nofile");
        let dst_db   = UtxoSyncDb::open(&dst_path).unwrap();
        assert!(load_snapshot(&path, &dst_db).is_err());
    }
}
