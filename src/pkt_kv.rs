#![allow(dead_code)]
//! v25.5 — KV backend: redb (pure Rust, no C++ toolchain needed)
//!
//! ## API
//! - `open_rw` / `open_ro` — mở DB
//! - `get` / `put` / `delete` — single-key ops
//! - `write_batch` — atomic multi-op
//! - `scan_all` / `scan_from` / `scan_rev` / `scan_prefix` — iterators
//!
//! `Kv` = `RedbKv` (unconditional — RocksDB đã bị xóa ở v25.5)

use std::path::Path;

// ── BatchOp ───────────────────────────────────────────────────────────────────

/// Một thao tác trong batch write.
pub enum BatchOp<'a> {
    Put(&'a [u8], &'a [u8]),
    Delete(&'a [u8]),
}

// ── RedbKv ────────────────────────────────────────────────────────────────────

mod redb_impl {
    use super::{BatchOp, Path};
    use redb::{Database, TableDefinition};
    use std::collections::HashMap;
    use std::sync::{Arc, LazyLock, Mutex, Weak};

    /// Single table cho toàn bộ KV data.
    const TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("kv");

    /// Registry: path → Weak<Database> để share cùng instance trong 1 process.
    /// redb lock file exclusive — mở 2 lần cùng path sẽ fail nếu không share Arc.
    static DB_REGISTRY: LazyLock<Mutex<HashMap<std::path::PathBuf, Weak<Database>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    /// Pure-Rust redb backend — không cần C++ toolchain, cross-compile trivial.
    ///
    /// ## Khác biệt so với RocksDB
    /// - redb lock file exclusive → cùng path phải dùng chung Arc<Database>
    ///   (DB_REGISTRY xử lý tự động)
    /// - Không hỗ trợ multi-process access — phù hợp pkt-fullnode (sync + API 1 process)
    /// - Single-op `put`/`delete` = 1 transaction → dùng `write_batch` cho bulk
    pub struct RedbKv {
        db: Arc<Database>,
    }

    impl RedbKv {
        /// redb dùng single FILE, RocksDB dùng DIRECTORY.
        /// Convention: file `data.redb` bên trong directory để giữ cùng path API.
        fn db_file(path: &Path) -> std::path::PathBuf {
            path.join("data.redb")
        }

        /// Lấy hoặc tạo Arc<Database> cho `db_file`, update registry.
        fn acquire(db_file: std::path::PathBuf, create: bool) -> Result<Arc<Database>, String> {
            let mut reg = DB_REGISTRY.lock().map_err(|e| e.to_string())?;
            // dọn stale entries
            reg.retain(|_, w| w.strong_count() > 0);
            // trả về instance đang mở nếu có
            if let Some(weak) = reg.get(&db_file) {
                if let Some(arc) = weak.upgrade() {
                    return Ok(arc);
                }
            }
            // mở/tạo mới
            let db = if create {
                Database::create(&db_file).map_err(|e| e.to_string())?
            } else {
                Database::open(&db_file).map_err(|e| e.to_string())?
            };
            let arc = Arc::new(db);
            reg.insert(db_file, Arc::downgrade(&arc));
            Ok(arc)
        }

        /// Tạo hoặc mở DB tại `path/data.redb` (read-write).
        pub fn open_rw(path: &Path) -> Result<Self, String> {
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
            let arc = Self::acquire(Self::db_file(path), true)?;
            // Đảm bảo table tồn tại
            let txn = arc.begin_write().map_err(|e| e.to_string())?;
            txn.open_table(TABLE).map_err(|e| e.to_string())?;
            txn.commit().map_err(|e| e.to_string())?;
            Ok(Self { db: arc })
        }

        /// Mở DB đã có tại `path/data.redb`.
        /// Nếu cùng path đang được mở rw trong process → chia sẻ Arc (không lock lại).
        pub fn open_ro(path: &Path) -> Result<Self, String> {
            let arc = Self::acquire(Self::db_file(path), false)?;
            Ok(Self { db: arc })
        }

        // ── Single-key ops ────────────────────────────────────────────────────

        pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
            let txn = self.db.begin_read().map_err(|e| e.to_string())?;
            let table = txn.open_table(TABLE).map_err(|e| e.to_string())?;
            match table.get(key).map_err(|e| e.to_string())? {
                None    => Ok(None),
                Some(v) => Ok(Some(v.value().to_vec())),
            }
        }

        pub fn put(&self, key: &[u8], val: &[u8]) -> Result<(), String> {
            let txn = self.db.begin_write().map_err(|e| e.to_string())?;
            {
                let mut table = txn.open_table(TABLE).map_err(|e| e.to_string())?;
                table.insert(key, val).map_err(|e| e.to_string())?;
            }
            txn.commit().map_err(|e| e.to_string())
        }

        pub fn delete(&self, key: &[u8]) -> Result<(), String> {
            let txn = self.db.begin_write().map_err(|e| e.to_string())?;
            {
                let mut table = txn.open_table(TABLE).map_err(|e| e.to_string())?;
                table.remove(key).map_err(|e| e.to_string())?;
            }
            txn.commit().map_err(|e| e.to_string())
        }

        /// Xóa tất cả keys có prefix trong 1 write transaction thay vì N.
        /// Scan keys trong read txn trước (redb không cho read+write cùng lúc),
        /// sau đó commit 1 lần duy nhất.
        pub fn delete_prefix(&self, prefix: &[u8]) -> Result<(), String> {
            let keys: Vec<Vec<u8>> = {
                let txn   = self.db.begin_read().map_err(|e| e.to_string())?;
                let table = txn.open_table(TABLE).map_err(|e| e.to_string())?;
                table.range(prefix..).map_err(|e| e.to_string())?
                    .filter_map(|r| r.ok())
                    .map(|(k, _)| k.value().to_vec())
                    .take_while(|k| k.starts_with(prefix))
                    .collect()
            };
            if keys.is_empty() { return Ok(()); }
            let txn = self.db.begin_write().map_err(|e| e.to_string())?;
            {
                let mut table = txn.open_table(TABLE).map_err(|e| e.to_string())?;
                for key in &keys {
                    table.remove(key.as_slice()).map_err(|e| e.to_string())?;
                }
            }
            txn.commit().map_err(|e| e.to_string())
        }

        // ── Batch write ───────────────────────────────────────────────────────

        /// Áp dụng nhiều put/delete trong 1 transaction — dùng cho bulk ops.
        pub fn write_batch(&self, ops: &[BatchOp<'_>]) -> Result<(), String> {
            let txn = self.db.begin_write().map_err(|e| e.to_string())?;
            {
                let mut table = txn.open_table(TABLE).map_err(|e| e.to_string())?;
                for op in ops {
                    match op {
                        BatchOp::Put(k, v)  => { table.insert(*k, *v).map_err(|e| e.to_string())?; }
                        BatchOp::Delete(k)  => { table.remove(*k).map_err(|e| e.to_string())?; }
                    }
                }
            }
            txn.commit().map_err(|e| e.to_string())
        }

        // ── Scan ops ──────────────────────────────────────────────────────────

        pub fn scan_all(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
            let Ok(txn)   = self.db.begin_read()       else { return vec![] };
            let Ok(table) = txn.open_table(TABLE)      else { return vec![] };
            let Ok(range) = table.range::<&[u8]>(..)   else { return vec![] };
            range
                .filter_map(|r| r.ok())
                .map(|(k, v)| (k.value().to_vec(), v.value().to_vec()))
                .collect()
        }

        pub fn scan_from(&self, start: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
            let Ok(txn)   = self.db.begin_read()    else { return vec![] };
            let Ok(table) = txn.open_table(TABLE)   else { return vec![] };
            let Ok(range) = table.range(start..)    else { return vec![] };
            range
                .filter_map(|r| r.ok())
                .map(|(k, v)| (k.value().to_vec(), v.value().to_vec()))
                .collect()
        }

        /// Scan backward từ `start` (inclusive).
        /// redb Range implement DoubleEndedIterator nên .rev() hoạt động.
        pub fn scan_rev(&self, start: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
            let Ok(txn)   = self.db.begin_read()    else { return vec![] };
            let Ok(table) = txn.open_table(TABLE)   else { return vec![] };
            let Ok(range) = table.range(..=start)   else { return vec![] };
            range.rev()
                .filter_map(|r| r.ok())
                .map(|(k, v)| (k.value().to_vec(), v.value().to_vec()))
                .collect()
        }

        pub fn scan_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
            self.scan_from(prefix)
                .into_iter()
                .take_while(|(k, _)| k.starts_with(prefix))
                .collect()
        }
    }
}

pub use redb_impl::RedbKv;

/// KV backend duy nhất: redb (pure Rust).
pub type Kv = RedbKv;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());

    fn temp_kv() -> Kv {
        let _g = LOCK.lock().unwrap();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("pkt_kv_test_{}", ts));
        Kv::open_rw(&path).unwrap()
    }

    #[test]
    fn test_delete_prefix_removes_matching_keys() {
        let kv = temp_kv();
        kv.put(b"bal:abc", b"1").unwrap();
        kv.put(b"bal:def", b"2").unwrap();
        kv.put(b"rich:xyz", b"3").unwrap();
        kv.delete_prefix(b"bal:").unwrap();
        assert!(kv.get(b"bal:abc").unwrap().is_none());
        assert!(kv.get(b"bal:def").unwrap().is_none());
        assert!(kv.get(b"rich:xyz").unwrap().is_some(), "other prefix untouched");
    }

    #[test]
    fn test_delete_prefix_empty_is_noop() {
        let kv = temp_kv();
        kv.put(b"other:key", b"v").unwrap();
        kv.delete_prefix(b"bal:").unwrap();
        assert!(kv.get(b"other:key").unwrap().is_some());
    }

    #[test]
    fn test_delete_prefix_all_keys_removed() {
        let kv = temp_kv();
        for i in 0u8..10 {
            kv.put(&[b'p', b':', i], b"v").unwrap();
        }
        kv.delete_prefix(b"p:").unwrap();
        assert!(kv.scan_prefix(b"p:").is_empty());
    }
}
