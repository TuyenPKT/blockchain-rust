#![allow(dead_code)]
//! v19.1 — Flat File Block Storage
//!
//! Lưu trữ `Block` vào các file `blk00000.dat`, `blk00001.dat`... (append-only),
//! tương tự Bitcoin Core. Thay thế `Vec<Block>` in-memory trong chain storage.
//!
//! ## File format
//!
//! Mỗi record trong `.dat` file:
//!   `[magic: 4 bytes][block_size: 4 bytes LE][block_json: block_size bytes]`
//!
//! - `magic` = `b"PKT!"` (0x50 0x4B 0x54 0x21) — phân biệt đầu record, phát hiện corrupt
//! - `block_size` = số bytes của JSON serialized block (u32 LE)
//! - `block_json` = serde_json bytes của `Block`
//!
//! File mới được tạo khi file hiện tại đạt `MAX_FILE_SIZE` (mặc định 128 MB).
//!
//! ## Index (RocksDB)
//!
//! Key: `blk:{height:016x}` → Value: `[file_num: 4 LE][offset: 8 LE][size: 4 LE]` (16 bytes)
//! Key: `meta:tip`          → Value: `[height: 8 LE]`
//! Key: `meta:cur_file`     → Value: `[file_num: 4 LE]`
//! Key: `meta:cur_offset`   → Value: `[offset: 8 LE]`
//!
//! ## CLI
//!
//! ```
//! cargo run -- block-storage --help
//! ```

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rocksdb::{DB, Options};
use serde_json;

use crate::block::Block;

// ── Constants ──────────────────────────────────────────────────────────────────

/// Magic bytes — "PKT!" in ASCII.
pub const MAGIC: [u8; 4] = [0x50, 0x4B, 0x54, 0x21];
/// Record header size: magic(4) + block_size(4).
pub const RECORD_HEADER: u64 = 8;
/// Default max file size: 128 MB.
pub const MAX_FILE_SIZE: u64 = 128 * 1024 * 1024;
/// Naming pattern: `blk{:05}.dat`
pub const FILE_PREFIX: &str = "blk";
pub const FILE_SUFFIX: &str = ".dat";

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum BlockStorageError {
    Io(std::io::Error),
    Db(String),
    Json(serde_json::Error),
    Corrupt { file_num: u32, offset: u64, reason: String },
    NotFound(u64),
}

impl std::fmt::Display for BlockStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e)                    => write!(f, "IO: {}", e),
            Self::Db(s)                    => write!(f, "DB: {}", s),
            Self::Json(e)                  => write!(f, "JSON: {}", e),
            Self::Corrupt { file_num, offset, reason } =>
                write!(f, "corrupt blk{:05}.dat @ offset {}: {}", file_num, offset, reason),
            Self::NotFound(h)              => write!(f, "block {} not found", h),
        }
    }
}

impl From<std::io::Error>   for BlockStorageError { fn from(e: std::io::Error)   -> Self { Self::Io(e)   } }
impl From<serde_json::Error> for BlockStorageError { fn from(e: serde_json::Error) -> Self { Self::Json(e) } }

pub type StorageResult<T> = Result<T, BlockStorageError>;

// ── BlockLocation ─────────────────────────────────────────────────────────────

/// Vị trí của một block trong flat files.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockLocation {
    /// Số thứ tự file (0 = blk00000.dat).
    pub file_num: u32,
    /// Byte offset của record header (magic) trong file.
    pub offset:   u64,
    /// Kích thước JSON data (bytes).
    pub size:     u32,
}

impl BlockLocation {
    fn to_bytes(&self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..4].copy_from_slice(&self.file_num.to_le_bytes());
        b[4..12].copy_from_slice(&self.offset.to_le_bytes());
        b[12..16].copy_from_slice(&self.size.to_le_bytes());
        b
    }

    fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 16 { return None; }
        Some(Self {
            file_num: u32::from_le_bytes(b[0..4].try_into().ok()?),
            offset:   u64::from_le_bytes(b[4..12].try_into().ok()?),
            size:     u32::from_le_bytes(b[12..16].try_into().ok()?),
        })
    }
}

// ── BlockStorage ──────────────────────────────────────────────────────────────

/// Flat file block storage với RocksDB index.
pub struct BlockStorage {
    data_dir:      PathBuf,
    index:         DB,
    max_file_size: u64,
    write_lock:    Mutex<()>,
}

impl BlockStorage {
    /// Mở (hoặc tạo mới) storage tại `data_dir`.
    pub fn open(data_dir: &Path) -> StorageResult<Self> {
        Self::open_with_max(data_dir, MAX_FILE_SIZE)
    }

    /// Mở với `max_file_size` tùy chỉnh — dùng trong tests.
    pub fn open_with_max(data_dir: &Path, max_file_size: u64) -> StorageResult<Self> {
        std::fs::create_dir_all(data_dir)?;
        let index_path = data_dir.join("index");
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let index = DB::open(&opts, &index_path)
            .map_err(|e| BlockStorageError::Db(e.to_string()))?;
        Ok(Self {
            data_dir: data_dir.to_owned(),
            index,
            max_file_size,
            write_lock: Mutex::new(()),
        })
    }

    // ── Paths ─────────────────────────────────────────────────────────────────

    fn dat_path(&self, file_num: u32) -> PathBuf {
        self.data_dir.join(format!("{}{:05}{}", FILE_PREFIX, file_num, FILE_SUFFIX))
    }

    // ── Index helpers ─────────────────────────────────────────────────────────

    fn index_key(height: u64) -> String {
        format!("blk:{:016x}", height)
    }

    fn get_u32(&self, key: &[u8]) -> StorageResult<Option<u32>> {
        match self.index.get(key).map_err(|e| BlockStorageError::Db(e.to_string()))? {
            None    => Ok(None),
            Some(v) if v.len() >= 4 =>
                Ok(Some(u32::from_le_bytes(v[..4].try_into().unwrap()))),
            Some(_) => Ok(None),
        }
    }

    fn get_u64(&self, key: &[u8]) -> StorageResult<Option<u64>> {
        match self.index.get(key).map_err(|e| BlockStorageError::Db(e.to_string()))? {
            None    => Ok(None),
            Some(v) if v.len() >= 8 =>
                Ok(Some(u64::from_le_bytes(v[..8].try_into().unwrap()))),
            Some(_) => Ok(None),
        }
    }

    fn set_u32(&self, key: &[u8], val: u32) -> StorageResult<()> {
        self.index.put(key, &val.to_le_bytes())
            .map_err(|e| BlockStorageError::Db(e.to_string()))
    }

    fn set_u64(&self, key: &[u8], val: u64) -> StorageResult<()> {
        self.index.put(key, &val.to_le_bytes())
            .map_err(|e| BlockStorageError::Db(e.to_string()))
    }

    // ── Write state ───────────────────────────────────────────────────────────

    fn current_file_num(&self) -> StorageResult<u32> {
        Ok(self.get_u32(b"meta:cur_file")?.unwrap_or(0))
    }

    fn current_offset(&self) -> StorageResult<u64> {
        Ok(self.get_u64(b"meta:cur_offset")?.unwrap_or(0))
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Chiều cao block cao nhất đã lưu.
    pub fn get_tip_height(&self) -> StorageResult<Option<u64>> {
        self.get_u64(b"meta:tip")
    }

    /// Số block đã lưu (đếm từ index).
    pub fn count(&self) -> u64 {
        let mut n = 0u64;
        let mode = rocksdb::IteratorMode::From(b"blk:", rocksdb::Direction::Forward);
        for item in self.index.iterator(mode) {
            let Ok((k, _)) = item else { break };
            if !k.starts_with(b"blk:") { break; }
            n += 1;
        }
        n
    }

    /// Lấy `BlockLocation` của block có height cho trước.
    pub fn get_location(&self, height: u64) -> StorageResult<Option<BlockLocation>> {
        let key = Self::index_key(height);
        match self.index.get(key.as_bytes())
            .map_err(|e| BlockStorageError::Db(e.to_string()))?
        {
            None    => Ok(None),
            Some(v) => Ok(BlockLocation::from_bytes(&v)),
        }
    }

    /// Đọc block tại vị trí cụ thể (không qua index).
    pub fn read_at(&self, loc: &BlockLocation) -> StorageResult<Block> {
        let path = self.dat_path(loc.file_num);
        let mut f = File::open(&path)?;
        f.seek(SeekFrom::Start(loc.offset))?;

        // Đọc và validate magic
        let mut hdr = [0u8; 8];
        f.read_exact(&mut hdr)?;
        if &hdr[0..4] != MAGIC {
            return Err(BlockStorageError::Corrupt {
                file_num: loc.file_num,
                offset:   loc.offset,
                reason:   format!("bad magic: {:02x}{:02x}{:02x}{:02x}",
                    hdr[0], hdr[1], hdr[2], hdr[3]),
            });
        }
        let stored_size = u32::from_le_bytes(hdr[4..8].try_into().unwrap());
        if stored_size != loc.size {
            return Err(BlockStorageError::Corrupt {
                file_num: loc.file_num,
                offset:   loc.offset,
                reason:   format!("size mismatch: index={} file={}", loc.size, stored_size),
            });
        }

        let mut buf = vec![0u8; loc.size as usize];
        f.read_exact(&mut buf)?;
        Ok(serde_json::from_slice(&buf)?)
    }

    /// Đọc block theo height.
    pub fn get(&self, height: u64) -> StorageResult<Option<Block>> {
        match self.get_location(height)? {
            None      => Ok(None),
            Some(loc) => Ok(Some(self.read_at(&loc)?)),
        }
    }

    /// Append một block vào storage.
    /// Tự động tạo file mới khi file hiện tại đạt `max_file_size`.
    pub fn append(&self, block: &Block) -> StorageResult<BlockLocation> {
        let _guard = self.write_lock.lock().unwrap();

        let json   = serde_json::to_vec(block)?;
        let size   = json.len() as u32;
        let needed = RECORD_HEADER + size as u64;

        // Xác định file và offset để ghi
        let mut file_num = self.current_file_num()?;
        let mut offset   = self.current_offset()?;

        // Tạo file mới nếu cần
        if offset > 0 && offset + needed > self.max_file_size {
            file_num += 1;
            offset    = 0;
            self.set_u32(b"meta:cur_file", file_num)?;
            self.set_u64(b"meta:cur_offset", 0)?;
        }

        // Mở file (tạo nếu chưa có)
        let path = self.dat_path(file_num);
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        // Ghi record: [magic][size LE][json]
        let mut header = [0u8; 8];
        header[0..4].copy_from_slice(&MAGIC);
        header[4..8].copy_from_slice(&size.to_le_bytes());
        f.write_all(&header)?;
        f.write_all(&json)?;
        f.flush()?;

        let loc = BlockLocation { file_num, offset, size };

        // Cập nhật index
        let ikey = Self::index_key(block.index);
        self.index.put(ikey.as_bytes(), &loc.to_bytes())
            .map_err(|e| BlockStorageError::Db(e.to_string()))?;

        // Cập nhật metadata
        let new_offset = offset + needed;
        self.set_u64(b"meta:cur_offset", new_offset)?;

        // Cập nhật tip
        let tip = self.get_u64(b"meta:tip")?.unwrap_or(0);
        if block.index >= tip {
            self.set_u64(b"meta:tip", block.index)?;
        }

        Ok(loc)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn make_storage() -> BlockStorage {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n   = SEQ.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let dir = std::env::temp_dir()
            .join(format!("pkt_block_storage_{}_{}", pid, n));
        BlockStorage::open(&dir).unwrap()
    }

    fn make_storage_max(max: u64) -> BlockStorage {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n   = SEQ.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let dir = std::env::temp_dir()
            .join(format!("pkt_block_storage_max_{}_{}", pid, n));
        BlockStorage::open_with_max(&dir, max).unwrap()
    }

    fn sample_block(height: u64) -> Block {
        Block {
            index:        height,
            timestamp:    1_700_000_000 + height as i64,
            transactions: vec![],
            prev_hash:    format!("{:064x}", height.saturating_sub(1)),
            nonce:        height * 7,
            hash:         format!("{:064x}", height),
            witness_root: "0".repeat(64),
        }
    }

    // ── BlockLocation ────────────────────────────────────────────────────────

    #[test]
    fn test_location_roundtrip() {
        let loc = BlockLocation { file_num: 3, offset: 12345, size: 512 };
        let bytes = loc.to_bytes();
        let decoded = BlockLocation::from_bytes(&bytes).unwrap();
        assert_eq!(loc, decoded);
    }

    #[test]
    fn test_location_from_bytes_too_short() {
        assert!(BlockLocation::from_bytes(&[0u8; 10]).is_none());
    }

    // ── Empty storage ────────────────────────────────────────────────────────

    #[test]
    fn test_empty_tip_none() {
        let s = make_storage();
        assert_eq!(s.get_tip_height().unwrap(), None);
    }

    #[test]
    fn test_empty_count_zero() {
        let s = make_storage();
        assert_eq!(s.count(), 0);
    }

    #[test]
    fn test_empty_get_none() {
        let s = make_storage();
        assert!(s.get(0).unwrap().is_none());
    }

    #[test]
    fn test_empty_get_location_none() {
        let s = make_storage();
        assert!(s.get_location(42).unwrap().is_none());
    }

    // ── Append & read ────────────────────────────────────────────────────────

    #[test]
    fn test_append_read_roundtrip() {
        let s   = make_storage();
        let blk = sample_block(0);
        s.append(&blk).unwrap();
        let got = s.get(0).unwrap().unwrap();
        assert_eq!(got.index, 0);
        assert_eq!(got.nonce, blk.nonce);
    }

    #[test]
    fn test_append_multiple() {
        let s = make_storage();
        for h in 0..5 {
            s.append(&sample_block(h)).unwrap();
        }
        assert_eq!(s.count(), 5);
        for h in 0..5 {
            let b = s.get(h).unwrap().unwrap();
            assert_eq!(b.index, h);
        }
    }

    #[test]
    fn test_tip_height_updated() {
        let s = make_storage();
        s.append(&sample_block(0)).unwrap();
        s.append(&sample_block(1)).unwrap();
        s.append(&sample_block(2)).unwrap();
        assert_eq!(s.get_tip_height().unwrap(), Some(2));
    }

    #[test]
    fn test_location_correct_file_num() {
        let s   = make_storage();
        let blk = sample_block(10);
        let loc = s.append(&blk).unwrap();
        assert_eq!(loc.file_num, 0);
        assert_eq!(loc.offset, 0);
    }

    #[test]
    fn test_second_block_offset_nonzero() {
        let s  = make_storage();
        let l0 = s.append(&sample_block(0)).unwrap();
        let l1 = s.append(&sample_block(1)).unwrap();
        assert_eq!(l0.file_num, 0);
        assert_eq!(l1.file_num, 0);
        assert!(l1.offset > l0.offset);
    }

    // ── File split ───────────────────────────────────────────────────────────

    #[test]
    fn test_file_split_on_size_limit() {
        // max = 50 bytes: кожен block ~120 bytes JSON → після першого блока
        // наступний вже переходить до файлу 1
        let s  = make_storage_max(50);
        let l0 = s.append(&sample_block(0)).unwrap();
        let l1 = s.append(&sample_block(1)).unwrap();
        assert_eq!(l0.file_num, 0);
        assert_eq!(l1.file_num, 1); // переключилось на новий файл
        assert_eq!(l1.offset, 0);   // offset скидається до 0
    }

    #[test]
    fn test_both_files_readable_after_split() {
        let s = make_storage_max(50);
        s.append(&sample_block(0)).unwrap();
        s.append(&sample_block(1)).unwrap();
        let b0 = s.get(0).unwrap().unwrap();
        let b1 = s.get(1).unwrap().unwrap();
        assert_eq!(b0.index, 0);
        assert_eq!(b1.index, 1);
    }

    // ── Magic validation ─────────────────────────────────────────────────────

    #[test]
    fn test_magic_bytes_constant() {
        assert_eq!(&MAGIC, b"PKT!");
    }

    #[test]
    fn test_read_at_bad_magic_returns_corrupt_error() {
        let s = make_storage();
        s.append(&sample_block(0)).unwrap();

        // Corrupt the magic bytes in the file
        let path = s.dat_path(0);
        let mut data = std::fs::read(&path).unwrap();
        data[0] = 0xFF; // corrupt first magic byte
        std::fs::write(&path, &data).unwrap();

        let loc = s.get_location(0).unwrap().unwrap();
        let err = s.read_at(&loc).unwrap_err();
        assert!(matches!(err, BlockStorageError::Corrupt { .. }));
    }

    // ── Error display ────────────────────────────────────────────────────────

    #[test]
    fn test_error_display_not_found() {
        let e = BlockStorageError::NotFound(42);
        assert!(e.to_string().contains("42"));
    }

    #[test]
    fn test_error_display_corrupt() {
        let e = BlockStorageError::Corrupt {
            file_num: 2, offset: 1024, reason: "bad magic".to_string(),
        };
        assert!(e.to_string().contains("blk00002.dat"));
        assert!(e.to_string().contains("1024"));
    }
}
