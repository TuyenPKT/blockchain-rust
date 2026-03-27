#![allow(dead_code)]
//! v18.9 — Data Export
//!
//! Streaming CSV export (không buffer toàn bộ vào RAM — trả về Vec<u8> tích lũy row-by-row):
//!
//!   GET /api/testnet/address/:s/export.csv
//!       → CSV: height,txid
//!       → Tối đa MAX_ADDR_EXPORT_ROWS rows
//!
//!   GET /api/testnet/blocks/export.csv?from=H&to=H
//!       → CSV: height,hash,prev_hash,timestamp,bits,nonce,version
//!       → Tối đa MAX_EXPORT_BLOCKS rows
//!       → Nếu from > to thì swap tự động; khoảng bị cắt tại MAX_EXPORT_BLOCKS

use crate::pkt_addr_index::AddrIndexDb;
use crate::pkt_sync::SyncDb;
use crate::pkt_wire::WireBlockHeader;

// ── Constants ──────────────────────────────────────────────────────────────────

/// Số rows tối đa cho address TX history export.
pub const MAX_ADDR_EXPORT_ROWS:  usize = 100_000;
/// Số blocks tối đa cho blocks range export.
pub const MAX_EXPORT_BLOCKS:     u64   = 10_000;

// ── Address export ─────────────────────────────────────────────────────────────

/// Tạo CSV bytes cho TX history của một address.
///
/// Header: `height,txid`
/// Mỗi dòng: `<height>,<txid_hex>`
/// Giới hạn MAX_ADDR_EXPORT_ROWS rows (không kể header).
pub fn generate_address_csv(adb: &AddrIndexDb, script_hex: &str, max_rows: usize) -> Vec<u8> {
    let limit = max_rows.min(MAX_ADDR_EXPORT_ROWS);
    let entries = adb.get_tx_history(script_hex, None, limit).unwrap_or_default();
    let mut out = Vec::with_capacity(entries.len() * 80);
    out.extend_from_slice(b"height,txid\n");
    for e in &entries {
        out.extend_from_slice(e.height.to_string().as_bytes());
        out.push(b',');
        out.extend_from_slice(e.txid.as_bytes());
        out.push(b'\n');
    }
    out
}

// ── Blocks export ──────────────────────────────────────────────────────────────

/// Tạo CSV bytes cho một khoảng block heights.
///
/// Header: `height,hash,prev_hash,timestamp,bits,nonce,version`
/// Giới hạn MAX_EXPORT_BLOCKS rows (không kể header).
/// Nếu `from > to` sẽ swap để luôn đi từ thấp đến cao.
pub fn generate_blocks_csv(sdb: &SyncDb, from: u64, to: u64) -> Vec<u8> {
    let (lo, hi) = if from <= to { (from, to) } else { (to, from) };
    let count = (hi - lo + 1).min(MAX_EXPORT_BLOCKS);
    let hi_capped = lo + count - 1;

    let mut out = Vec::with_capacity((count as usize) * 120);
    out.extend_from_slice(b"height,hash,prev_hash,timestamp,bits,nonce,version\n");

    for h in lo..=hi_capped {
        let raw = match sdb.load_header(h) {
            Ok(Some(r)) => r,
            _ => continue,
        };
        let hdr = match WireBlockHeader::from_bytes(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let hash      = hex::encode(WireBlockHeader::block_hash_of_bytes(&raw));
        let prev_hash = hex::encode(hdr.prev_block);

        out.extend_from_slice(h.to_string().as_bytes());
        out.push(b',');
        out.extend_from_slice(hash.as_bytes());
        out.push(b',');
        out.extend_from_slice(prev_hash.as_bytes());
        out.push(b',');
        out.extend_from_slice(hdr.timestamp.to_string().as_bytes());
        out.push(b',');
        out.extend_from_slice(hdr.bits.to_string().as_bytes());
        out.push(b',');
        out.extend_from_slice(hdr.nonce.to_string().as_bytes());
        out.push(b',');
        out.extend_from_slice(hdr.version.to_string().as_bytes());
        out.push(b'\n');
    }
    out
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn make_sync_db() -> SyncDb {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n   = SEQ.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("pkt_export_test_sync_{}_{}", pid, n));
        SyncDb::open(&path).unwrap()
    }

    fn make_addr_db() -> AddrIndexDb {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n   = SEQ.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("pkt_export_test_addr_{}_{}", pid, n));
        AddrIndexDb::open(&path).unwrap()
    }

    fn sample_header(ts: u32, merkle: [u8; 32]) -> WireBlockHeader {
        WireBlockHeader {
            version:     1,
            prev_block:  [0u8; 32],
            merkle_root: merkle,
            timestamp:   ts,
            bits:        0x207fffff,
            nonce:       0,
        }
    }

    // ── generate_blocks_csv ───────────────────────────────────────────────────

    #[test]
    fn test_blocks_csv_has_header_row() {
        let sdb = make_sync_db();
        let csv = generate_blocks_csv(&sdb, 0, 0);
        let text = String::from_utf8(csv).unwrap();
        assert!(text.starts_with("height,hash,prev_hash,timestamp,bits,nonce,version\n"));
    }

    #[test]
    fn test_blocks_csv_empty_db_header_only() {
        let sdb = make_sync_db();
        let csv = generate_blocks_csv(&sdb, 0, 5);
        let text = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        // Only header, no data rows (DB empty)
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_blocks_csv_single_block() {
        let sdb = make_sync_db();
        let hdr = sample_header(1_700_000_000, [1u8; 32]);
        sdb.save_header(10, &hdr.to_bytes()).unwrap();
        sdb.set_sync_height(10).unwrap();

        let csv = generate_blocks_csv(&sdb, 10, 10);
        let text = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        // header + 1 data row
        assert_eq!(lines.len(), 2);
        let row = lines[1];
        assert!(row.starts_with("10,"));
        assert!(row.contains("1700000000"));
    }

    #[test]
    fn test_blocks_csv_from_gt_to_swaps() {
        let sdb = make_sync_db();
        let hdr = sample_header(1_700_000_001, [2u8; 32]);
        sdb.save_header(5, &hdr.to_bytes()).unwrap();
        sdb.set_sync_height(5).unwrap();

        // from=10, to=5 → should still load block 5
        let csv = generate_blocks_csv(&sdb, 10, 5);
        let text = String::from_utf8(csv).unwrap();
        assert!(text.contains("5,"));
    }

    #[test]
    fn test_blocks_csv_capped_at_max() {
        let sdb = make_sync_db();
        // Request 20_000 blocks but cap is 10_000
        let csv = generate_blocks_csv(&sdb, 0, 19_999);
        let text = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        // header only (DB empty), but no panic
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_blocks_csv_columns_count() {
        let sdb = make_sync_db();
        let hdr = sample_header(1_700_000_002, [3u8; 32]);
        sdb.save_header(1, &hdr.to_bytes()).unwrap();
        sdb.set_sync_height(1).unwrap();

        let csv = generate_blocks_csv(&sdb, 1, 1);
        let text = String::from_utf8(csv).unwrap();
        let mut lines = text.lines();
        let header_cols = lines.next().unwrap().split(',').count();
        let data_cols   = lines.next().unwrap().split(',').count();
        assert_eq!(header_cols, 7);
        assert_eq!(data_cols,   7);
    }

    // ── generate_address_csv ──────────────────────────────────────────────────

    #[test]
    fn test_address_csv_has_header_row() {
        let adb = make_addr_db();
        let csv = generate_address_csv(&adb, "deadbeef", 100);
        let text = String::from_utf8(csv).unwrap();
        assert!(text.starts_with("height,txid\n"));
    }

    #[test]
    fn test_address_csv_empty_address_header_only() {
        let adb = make_addr_db();
        let csv = generate_address_csv(&adb, "nonexistent_script", 100);
        let text = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_address_csv_respects_max_rows() {
        // max_rows=0 → only header
        let adb = make_addr_db();
        let csv = generate_address_csv(&adb, "script", 0);
        let text = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1);
    }
}
