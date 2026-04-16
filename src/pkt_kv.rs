#![allow(dead_code)]
//! v25.1 — KV abstraction: RocksKv (default) + RedbKv (--features use-redb)
//!
//! ## Chọn backend
//!
//! ```bash
//! cargo build                        # dùng RocksDB (default)
//! cargo build --features use-redb    # dùng redb (pure Rust)
//! ```
//!
//! ## API surface (giống nhau ở cả hai backend)
//! - `open_rw` / `open_ro` — mở DB
//! - `get` / `put` / `delete` — single-key ops
//! - `write_batch` — atomic multi-op
//! - `scan_all` / `scan_from` / `scan_rev` / `scan_prefix` — iterators
//!
//! ## Type alias
//! `Kv` = `RedbKv` khi `use-redb` feature bật, ngược lại = `RocksKv`.
//! DB structs dùng `Kv` — không cần sửa khi switch backend.

use std::path::Path;

// ── BatchOp ───────────────────────────────────────────────────────────────────

/// Một thao tác trong batch write.
pub enum BatchOp<'a> {
    Put(&'a [u8], &'a [u8]),
    Delete(&'a [u8]),
}

// ── RocksKv ───────────────────────────────────────────────────────────────────

use rocksdb::{Direction, IteratorMode, Options, WriteBatch, DB};

/// Wrapper mỏng quanh một RocksDB instance (single column family).
pub struct RocksKv {
    db: DB,
}

impl RocksKv {
    /// Mở read-write với LZ4 compression, tạo nếu chưa có.
    pub fn open_rw(path: &Path) -> Result<Self, String> {
        let mut opts = crate::pkt_paths::db_opts();
        opts.create_if_missing(true);
        DB::open(&opts, path)
            .map(|db| Self { db })
            .map_err(|e| e.to_string())
    }

    /// Mở read-only — nhiều reader cùng lúc với writer.
    pub fn open_ro(path: &Path) -> Result<Self, String> {
        let opts = Options::default();
        DB::open_for_read_only(&opts, path, false)
            .map(|db| Self { db })
            .map_err(|e| e.to_string())
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
        self.db.get(key).map_err(|e| e.to_string())
    }

    pub fn put(&self, key: &[u8], val: &[u8]) -> Result<(), String> {
        self.db.put(key, val).map_err(|e| e.to_string())
    }

    pub fn delete(&self, key: &[u8]) -> Result<(), String> {
        self.db.delete(key).map_err(|e| e.to_string())
    }

    pub fn write_batch(&self, ops: &[BatchOp<'_>]) -> Result<(), String> {
        let mut batch = WriteBatch::default();
        for op in ops {
            match op {
                BatchOp::Put(k, v) => batch.put(k, v),
                BatchOp::Delete(k) => batch.delete(k),
            }
        }
        self.db.write(batch).map_err(|e| e.to_string())
    }

    pub fn scan_all(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.db
            .iterator(IteratorMode::Start)
            .filter_map(|r| r.ok())
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect()
    }

    pub fn scan_from(&self, start: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.db
            .iterator(IteratorMode::From(start, Direction::Forward))
            .filter_map(|r| r.ok())
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect()
    }

    pub fn scan_rev(&self, start: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.db
            .iterator(IteratorMode::From(start, Direction::Reverse))
            .filter_map(|r| r.ok())
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect()
    }

    pub fn scan_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.scan_from(prefix)
            .into_iter()
            .take_while(|(k, _)| k.starts_with(prefix))
            .collect()
    }
}

// ── RedbKv ────────────────────────────────────────────────────────────────────

#[cfg(feature = "use-redb")]
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

#[cfg(feature = "use-redb")]
pub use redb_impl::RedbKv;

// ── Kv type alias ─────────────────────────────────────────────────────────────
//
// DB structs dùng `Kv` thay vì `RocksKv` trực tiếp.
// Switch backend = thêm/bỏ --features use-redb, không cần sửa code.

/// Backend hiện tại được chọn tại compile time.
#[cfg(feature = "use-redb")]
pub type Kv = RedbKv;

/// Backend mặc định: RocksDB.
#[cfg(not(feature = "use-redb"))]
pub type Kv = RocksKv;
