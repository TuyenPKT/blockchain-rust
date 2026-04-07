#![allow(dead_code)]
//! v18.1 — Address Labels
//!
//! Lưu và tra cứu nhãn cho script_pubkey / địa chỉ PKT.
//!
//! ## Nguồn nhãn (theo thứ tự ưu tiên)
//!   1. Presets tích hợp sẵn (miners, exchanges, burn, system) — không cần DB
//!   2. LabelDb (RocksDB tại `~/.pkt/labeldb`) — label tùy chỉnh
//!
//! ## Key schema (RocksDB)
//!   `lbl:{script_hex}` → JSON `LabelEntry`
//!
//! ## API
//!   GET /api/testnet/label/:script  → LabelEntry | 404

use std::path::{Path, PathBuf};

use rocksdb::{Direction, IteratorMode, Options, DB};
use serde::{Deserialize, Serialize};

use crate::pkt_sync::SyncError;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LabelEntry {
    pub label:    String,
    /// "miner" | "exchange" | "burn" | "system" | "other"
    pub category: String,
    pub verified: bool,
}

// ── Preset labels ─────────────────────────────────────────────────────────────
//
// Khớp theo tiền tố Base58Check address.
// Tuple: (address_prefix, label, category, verified)

pub static PRESETS: &[(&str, &str, &str, bool)] = &[
    // OCEIF treasury address (mainnet)
    ("pSEHPyBk", "OCEIF Treasury",      "system",   true),
    // OCEIF mining pool v1 (mainnet)
    ("p7LMkZBs", "OCEIF Pool v1",       "miner",    true),
    // Testnet genesis coinbase recipient (placeholder — replace when known)
    ("pGTESTNE", "Testnet Genesis",      "system",   true),
    // Common burn pattern: address starting with all-zero hash160
    // Represented as first character after version byte being "1" in base58
    ("p111111",  "Burn Address",         "burn",     true),
];

/// Look up preset label by Base58Check address (exact prefix match).
pub fn preset_by_address(addr: &str) -> Option<LabelEntry> {
    for (prefix, label, category, verified) in PRESETS {
        if addr.starts_with(prefix) {
            return Some(LabelEntry {
                label:    label.to_string(),
                category: category.to_string(),
                verified: *verified,
            });
        }
    }
    None
}

// ── LabelDb ───────────────────────────────────────────────────────────────────

pub struct LabelDb {
    db:   DB,
    path: PathBuf,
}

impl LabelDb {
    fn lbl_key(script_hex: &str) -> String {
        format!("lbl:{}", script_hex)
    }

    pub fn open(path: &Path) -> Result<Self, SyncError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path).map_err(|e| SyncError::Db(e.to_string()))?;
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
        let path = std::env::temp_dir().join(format!("pkt_labeldb_{}", ts));
        Self::open(&path)
    }

    pub fn path(&self) -> &Path { &self.path }

    // ── Read ──────────────────────────────────────────────────────────────────

    /// Tra cứu label theo script_hex. Chỉ tìm trong DB (không check preset).
    pub fn get_label(&self, script_hex: &str) -> Option<LabelEntry> {
        let key = Self::lbl_key(script_hex);
        let raw = self.db.get(key.as_bytes()).ok()??;
        serde_json::from_slice::<LabelEntry>(&raw).ok()
    }

    /// Tra cứu label theo address (Base58Check).
    /// Ưu tiên: preset → DB (với address key).
    pub fn get_label_by_address(&self, addr: &str) -> Option<LabelEntry> {
        if let Some(e) = preset_by_address(addr) {
            return Some(e);
        }
        // DB lookup bằng address trực tiếp làm key
        let key = Self::lbl_key(addr);
        let raw = self.db.get(key.as_bytes()).ok()??;
        serde_json::from_slice::<LabelEntry>(&raw).ok()
    }

    /// Tra cứu label theo script_hex, fallback sang address nếu được cung cấp.
    pub fn get_label_for(&self, script_hex: &str, address: Option<&str>) -> Option<LabelEntry> {
        // 1. Script-based preset (chưa có — script quá dài để preset)
        // 2. Address-based preset
        if let Some(addr) = address {
            if let Some(e) = preset_by_address(addr) {
                return Some(e);
            }
        }
        // 3. DB by script_hex
        if let Some(e) = self.get_label(script_hex) {
            return Some(e);
        }
        // 4. DB by address
        if let Some(addr) = address {
            let key = Self::lbl_key(addr);
            if let Ok(Some(raw)) = self.db.get(key.as_bytes()) {
                if let Ok(e) = serde_json::from_slice::<LabelEntry>(&raw) {
                    return Some(e);
                }
            }
        }
        None
    }

    /// Liệt kê tất cả labels trong DB.
    pub fn list_all(&self) -> Vec<(String, LabelEntry)> {
        let prefix = b"lbl:";
        let iter = self.db.iterator(IteratorMode::From(prefix, Direction::Forward));
        let mut out = Vec::new();
        for item in iter {
            let Ok((key, val)) = item else { continue };
            if !key.starts_with(prefix) { break; }
            let script = std::str::from_utf8(&key[4..]).unwrap_or("").to_string();
            if let Ok(entry) = serde_json::from_slice::<LabelEntry>(&val) {
                out.push((script, entry));
            }
        }
        out
    }

    // ── Write ─────────────────────────────────────────────────────────────────

    /// Lưu label vào DB theo script_hex (hoặc address string).
    pub fn set_label(
        &self,
        key_str:  &str,
        label:    &str,
        category: &str,
        verified: bool,
    ) -> Result<(), SyncError> {
        let key = Self::lbl_key(key_str);
        let entry = LabelEntry {
            label:    label.to_string(),
            category: category.to_string(),
            verified,
        };
        let raw = serde_json::to_vec(&entry).map_err(|e| SyncError::Db(e.to_string()))?;
        self.db.put(key.as_bytes(), &raw).map_err(|e| SyncError::Db(e.to_string()))
    }

    /// Xóa label khỏi DB.
    pub fn delete_label(&self, key_str: &str) -> Result<(), SyncError> {
        let key = Self::lbl_key(key_str);
        self.db.delete(key.as_bytes()).map_err(|e| SyncError::Db(e.to_string()))
    }
}

// ── Path helper ───────────────────────────────────────────────────────────────

fn home_path(rel: &str) -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(rel)
}

pub fn default_label_db_path() -> PathBuf {
    home_path(".pkt/labeldb")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static DB_LOCK: Mutex<()> = Mutex::new(());

    // ── LabelEntry serde ──────────────────────────────────────────────────────

    #[test]
    fn test_label_entry_serde_roundtrip() {
        let e = LabelEntry {
            label:    "Test Miner".into(),
            category: "miner".into(),
            verified: true,
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: LabelEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn test_label_entry_all_categories() {
        for cat in &["miner", "exchange", "burn", "system", "other"] {
            let e = LabelEntry { label: "X".into(), category: cat.to_string(), verified: false };
            let s = serde_json::to_string(&e).unwrap();
            let b: LabelEntry = serde_json::from_str(&s).unwrap();
            assert_eq!(b.category, *cat);
        }
    }

    // ── preset_by_address ─────────────────────────────────────────────────────

    #[test]
    fn test_preset_burn_address() {
        let e = preset_by_address("p111111xxxx");
        assert!(e.is_some());
        let e = e.unwrap();
        assert_eq!(e.category, "burn");
        assert!(e.verified);
    }

    #[test]
    fn test_preset_network_steward() {
        let e = preset_by_address("pSEHPyBkxxxxxx");
        assert!(e.is_some());
        assert_eq!(e.unwrap().category, "system");
    }

    #[test]
    fn test_preset_no_match() {
        assert!(preset_by_address("pZZZZZZZ").is_none());
    }

    #[test]
    fn test_preset_verified_flag() {
        let e = preset_by_address("p7LMkZBsxxx").unwrap();
        assert!(e.verified);
    }

    #[test]
    fn test_preset_miner_category() {
        let e = preset_by_address("p7LMkZBsxxx").unwrap();
        assert_eq!(e.category, "miner");
    }

    // ── LabelDb open / read / write ───────────────────────────────────────────

    #[test]
    fn test_open_temp() {
        let _g = DB_LOCK.lock().unwrap();
        assert!(LabelDb::open_temp().is_ok());
    }

    #[test]
    fn test_set_and_get_label() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        db.set_label("abc123script", "My Miner", "miner", false).unwrap();
        let e = db.get_label("abc123script").unwrap();
        assert_eq!(e.label, "My Miner");
        assert_eq!(e.category, "miner");
        assert!(!e.verified);
    }

    #[test]
    fn test_get_label_not_found() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        assert!(db.get_label("nonexistent_script").is_none());
    }

    #[test]
    fn test_delete_label() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        db.set_label("to_delete", "Temp", "other", false).unwrap();
        assert!(db.get_label("to_delete").is_some());
        db.delete_label("to_delete").unwrap();
        assert!(db.get_label("to_delete").is_none());
    }

    #[test]
    fn test_list_all_empty() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        assert!(db.list_all().is_empty());
    }

    #[test]
    fn test_list_all_multiple() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        db.set_label("script_a", "Label A", "miner",    true).unwrap();
        db.set_label("script_b", "Label B", "exchange", false).unwrap();
        db.set_label("script_c", "Label C", "burn",     true).unwrap();
        let all = db.list_all();
        assert_eq!(all.len(), 3);
        let keys: Vec<_> = all.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"script_a"));
        assert!(keys.contains(&"script_b"));
        assert!(keys.contains(&"script_c"));
    }

    #[test]
    fn test_overwrite_label() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        db.set_label("s1", "First", "other", false).unwrap();
        db.set_label("s1", "Updated", "miner", true).unwrap();
        let e = db.get_label("s1").unwrap();
        assert_eq!(e.label, "Updated");
        assert_eq!(e.category, "miner");
        assert!(e.verified);
    }

    // ── get_label_for ─────────────────────────────────────────────────────────

    #[test]
    fn test_get_label_for_preset_wins() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        // Even if DB has a label for the same address, preset wins
        db.set_label("p111111xxxx", "Custom", "other", false).unwrap();
        let e = db.get_label_for("somescript", Some("p111111xxxx")).unwrap();
        assert_eq!(e.category, "burn"); // preset
    }

    #[test]
    fn test_get_label_for_db_fallback() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        db.set_label("myscript", "DB Label", "exchange", false).unwrap();
        let e = db.get_label_for("myscript", Some("pZZZZunknown")).unwrap();
        assert_eq!(e.label, "DB Label");
    }

    #[test]
    fn test_get_label_for_none_when_no_match() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        assert!(db.get_label_for("unknownscript", Some("pZZZZunknown")).is_none());
    }

    // ── default_label_db_path ─────────────────────────────────────────────────

    #[test]
    fn test_default_label_db_path_ends_with_labeldb() {
        let p = default_label_db_path();
        assert!(p.to_str().unwrap().contains("labeldb"));
    }

    #[test]
    fn test_default_label_db_path_under_pkt() {
        let p = default_label_db_path();
        assert!(p.to_str().unwrap().contains(".pkt"));
    }

    // ── get_label_by_address ──────────────────────────────────────────────────

    #[test]
    fn test_get_label_by_address_preset() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        let e = db.get_label_by_address("pSEHPyBk_any_suffix");
        assert!(e.is_some());
        assert_eq!(e.unwrap().label, "PKT Network Steward");
    }

    #[test]
    fn test_get_label_by_address_db() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        db.set_label("pCustomAddr123", "My Pool", "miner", false).unwrap();
        let e = db.get_label_by_address("pCustomAddr123").unwrap();
        assert_eq!(e.label, "My Pool");
    }

    #[test]
    fn test_get_label_by_address_none() {
        let _g = DB_LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        assert!(db.get_label_by_address("pNOTHING").is_none());
    }
}
