#![allow(dead_code)]
//! v18.8 — Health & Uptime
//!
//! Kiểm tra trạng thái sức khoẻ của node:
//!   - last_block_age_secs : số giây kể từ block cuối cùng được sync
//!   - sync_lag            : sync_height - utxo_height (blocks chưa index UTXO)
//!   - mempool_count       : số TX đang pending
//!   - db_size_bytes       : dung lượng từng DB trên disk
//!   - alert               : true nếu block_age > 10 phút
//!
//! `GET /api/health/detailed` → JSON HealthStatus.
//! `web/health/index.html`    → status page, auto-refresh 10s.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::pkt_mempool_sync::MempoolDb;
use crate::pkt_sync::SyncDb;
use crate::pkt_utxo_sync::UtxoSyncDb;
use crate::pkt_wire::WireBlockHeader;

// ── Constants ──────────────────────────────────────────────────────────────────

/// Alert nếu block cuối cũ hơn 10 phút.
pub const BLOCK_AGE_ALERT_SECS: u64 = 600;

// ── HealthStatus ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct HealthStatus {
    /// Tổng quan: true khi synced và không có alert.
    pub ok:                   bool,
    /// true khi block_age > BLOCK_AGE_ALERT_SECS.
    pub alert:                bool,
    /// Mô tả vấn đề (nếu có).
    pub alert_message:        Option<String>,
    /// Chiều cao header đã sync.
    pub sync_height:          u64,
    /// Chiều cao UTXO đã index.
    pub utxo_height:          u64,
    /// sync_height - utxo_height (số block UTXO chưa kịp index).
    pub sync_lag:             u64,
    /// Unix timestamp của block cuối cùng.
    pub last_block_ts:        u64,
    /// Số giây kể từ block cuối cùng (0 khi chưa sync).
    pub last_block_age_secs:  u64,
    /// Số TX đang chờ trong mempool.
    pub mempool_count:        u64,
    /// Dung lượng syncdb (bytes).
    pub syncdb_size_bytes:    u64,
    /// Dung lượng utxodb (bytes).
    pub utxodb_size_bytes:    u64,
    /// Dung lượng addrdb (bytes).
    pub addrdb_size_bytes:    u64,
    /// Dung lượng mempooldb (bytes).
    pub mempooldb_size_bytes: u64,
    /// Tổng dung lượng tất cả DBs.
    pub total_db_size_bytes:  u64,
    /// Unix timestamp khi chạy check này.
    pub checked_at:           u64,
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Tính tổng kích thước các file trong một thư mục (không đệ quy).
pub fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

// ── Core logic ─────────────────────────────────────────────────────────────────

/// Nội bộ: tính HealthStatus từ các DB handle đã mở (hoặc None).
/// Testable vì tests có thể truyền vào DB handle trực tiếp.
pub fn collect_health(
    sdb:          Option<&SyncDb>,
    udb:          Option<&UtxoSyncDb>,
    mdb:          Option<&MempoolDb>,
    sync_path:    &Path,
    utxo_path:    &Path,
    addr_path:    &Path,
    mempool_path: &Path,
) -> HealthStatus {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // ── Heights ────────────────────────────────────────────────────────────────
    let (sync_height, last_block_ts) = sdb
        .and_then(|db| {
            let h   = db.get_sync_height().ok().flatten()?;
            let raw = db.load_header(h).ok().flatten()?;
            let hdr = WireBlockHeader::from_bytes(&raw).ok()?;
            Some((h, hdr.timestamp as u64))
        })
        .unwrap_or((0, 0));

    let utxo_height = udb
        .and_then(|db| db.get_utxo_height().ok().flatten())
        .unwrap_or(0);

    let last_block_age_secs = if last_block_ts > 0 {
        now_secs.saturating_sub(last_block_ts)
    } else {
        0
    };

    let sync_lag = sync_height.saturating_sub(utxo_height);

    // ── Mempool ────────────────────────────────────────────────────────────────
    let mempool_count = mdb
        .and_then(|db| db.count().ok())
        .unwrap_or(0) as u64;

    // ── DB sizes ───────────────────────────────────────────────────────────────
    let syncdb_size_bytes    = dir_size(sync_path);
    let utxodb_size_bytes    = dir_size(utxo_path);
    let addrdb_size_bytes    = dir_size(addr_path);
    let mempooldb_size_bytes = dir_size(mempool_path);
    let total_db_size_bytes  =
        syncdb_size_bytes + utxodb_size_bytes + addrdb_size_bytes + mempooldb_size_bytes;

    // ── Alerts ─────────────────────────────────────────────────────────────────
    let block_age_alert = last_block_ts > 0 && last_block_age_secs > BLOCK_AGE_ALERT_SECS;
    let not_synced      = sync_height == 0;

    let alert = block_age_alert;
    let alert_message = if block_age_alert {
        Some(format!(
            "No new block for {} min (last block {}s ago)",
            last_block_age_secs / 60,
            last_block_age_secs,
        ))
    } else if not_synced {
        Some("Not synced — run: cargo run -- sync".to_string())
    } else {
        None
    };

    let ok = sync_height > 0 && !alert;

    HealthStatus {
        ok,
        alert,
        alert_message,
        sync_height,
        utxo_height,
        sync_lag,
        last_block_ts,
        last_block_age_secs,
        mempool_count,
        syncdb_size_bytes,
        utxodb_size_bytes,
        addrdb_size_bytes,
        mempooldb_size_bytes,
        total_db_size_bytes,
        checked_at: now_secs,
    }
}

/// Truy vấn trạng thái health từ paths (production path).
/// Graceful: mọi DB unavailable đều trả về giá trị mặc định an toàn.
pub fn query_health(
    sync_path:    &Path,
    utxo_path:    &Path,
    addr_path:    &Path,
    mempool_path: &Path,
) -> HealthStatus {
    let sdb = SyncDb::open_read_only(sync_path).ok();
    let udb = UtxoSyncDb::open_read_only(utxo_path).ok();
    let mdb = MempoolDb::open_read_only(mempool_path).ok();
    collect_health(
        sdb.as_ref(),
        udb.as_ref(),
        mdb.as_ref(),
        sync_path,
        utxo_path,
        addr_path,
        mempool_path,
    )
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const MISSING: &str = "/nonexistent/pkt_health_test_path";
    fn missing() -> &'static Path { Path::new(MISSING) }

    /// Unique temp SyncDb per call — avoids rand_u64() nanosecond collisions in parallel tests.
    fn make_sync_db() -> SyncDb {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("pkt_health_test_sync_{}", n));
        SyncDb::open(&path).unwrap()
    }

    /// Unique temp UtxoSyncDb per call.
    fn make_utxo_db() -> crate::pkt_utxo_sync::UtxoSyncDb {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("pkt_health_test_utxo_{}", n));
        crate::pkt_utxo_sync::UtxoSyncDb::open(&path).unwrap()
    }

    /// Helper: collect_health with only sdb set, everything else None/missing
    fn health_sdb(sdb: &SyncDb) -> HealthStatus {
        collect_health(
            Some(sdb), None, None,
            sdb.path(), missing(), missing(), missing(),
        )
    }

    /// Helper: collect_health with no DBs
    fn health_none() -> HealthStatus {
        collect_health(None, None, None, missing(), missing(), missing(), missing())
    }

    // ── dir_size ───────────────────────────────────────────────────────────────

    #[test]
    fn test_dir_size_nonexistent_returns_zero() {
        assert_eq!(dir_size(missing()), 0);
    }

    #[test]
    fn test_dir_size_empty_dir_returns_zero() {
        let tmp = std::env::temp_dir().join("pkt_health_dir_size_test");
        let _ = std::fs::create_dir_all(&tmp);
        assert_eq!(dir_size(&tmp), 0);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── collect_health with no DBs ─────────────────────────────────────────────

    #[test]
    fn test_health_no_dbs_sync_height_zero() {
        assert_eq!(health_none().sync_height, 0);
    }

    #[test]
    fn test_health_no_dbs_utxo_height_zero() {
        assert_eq!(health_none().utxo_height, 0);
    }

    #[test]
    fn test_health_no_dbs_not_ok() {
        assert!(!health_none().ok);
    }

    #[test]
    fn test_health_no_dbs_no_alert() {
        assert!(!health_none().alert);
    }

    #[test]
    fn test_health_no_dbs_has_not_synced_message() {
        let msg = health_none().alert_message.unwrap_or_default();
        assert!(msg.contains("Not synced"));
    }

    #[test]
    fn test_health_checked_at_is_positive() {
        assert!(health_none().checked_at > 0);
    }

    #[test]
    fn test_health_total_size_equals_sum() {
        let s = health_none();
        assert_eq!(
            s.total_db_size_bytes,
            s.syncdb_size_bytes + s.utxodb_size_bytes
                + s.addrdb_size_bytes + s.mempooldb_size_bytes
        );
    }

    #[test]
    fn test_health_sync_lag_zero_when_no_data() {
        assert_eq!(health_none().sync_lag, 0);
    }

    // ── collect_health with live SyncDb ────────────────────────────────────────

    #[test]
    fn test_health_synced_db_has_height() {
        let sdb = make_sync_db();
        let hdr = crate::pkt_wire::WireBlockHeader {
            version: 1, prev_block: [0; 32], merkle_root: [1; 32],
            timestamp: 1_700_000_000, bits: 0x207fffff, nonce: 0,
        };
        sdb.save_header(5, &hdr.to_bytes()).unwrap();
        sdb.set_sync_height(5).unwrap();
        assert_eq!(health_sdb(&sdb).sync_height, 5);
    }

    #[test]
    fn test_health_old_block_triggers_alert() {
        let sdb = make_sync_db();
        let hdr = crate::pkt_wire::WireBlockHeader {
            version: 1, prev_block: [0; 32], merkle_root: [2; 32],
            timestamp: 1, bits: 0x207fffff, nonce: 0,
        };
        sdb.save_header(1, &hdr.to_bytes()).unwrap();
        sdb.set_sync_height(1).unwrap();
        let s = health_sdb(&sdb);
        assert!(s.alert);
        assert!(!s.ok);
        assert!(s.alert_message.unwrap().contains("No new block"));
    }

    #[test]
    fn test_health_recent_block_no_alert() {
        let sdb = make_sync_db();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
        let hdr = crate::pkt_wire::WireBlockHeader {
            version: 1, prev_block: [0; 32], merkle_root: [3; 32],
            timestamp: now, bits: 0x207fffff, nonce: 0,
        };
        sdb.save_header(1, &hdr.to_bytes()).unwrap();
        sdb.set_sync_height(1).unwrap();
        let s = health_sdb(&sdb);
        assert!(!s.alert);
        assert!(s.ok);
    }

    #[test]
    fn test_health_sync_lag_computed() {
        let sdb = make_sync_db();
        let udb = make_utxo_db();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
        let hdr = crate::pkt_wire::WireBlockHeader {
            version: 1, prev_block: [0; 32], merkle_root: [4; 32],
            timestamp: now, bits: 0x207fffff, nonce: 0,
        };
        sdb.save_header(10, &hdr.to_bytes()).unwrap();
        sdb.set_sync_height(10).unwrap();
        udb.set_utxo_height(7).unwrap();

        let s = collect_health(
            Some(&sdb), Some(&udb), None,
            sdb.path(), udb.path(), missing(), missing(),
        );
        assert_eq!(s.sync_height, 10);
        assert_eq!(s.utxo_height, 7);
        assert_eq!(s.sync_lag, 3);
    }
}
