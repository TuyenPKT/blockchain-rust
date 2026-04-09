#![allow(dead_code)]
//! v17.0 — Address Index
//!
//! RocksDB namespace (`addrdb/`) với 3 key family:
//!
//!   atx:{script_hex}:{height:016x}:{txid_hex}  → ""
//!       Tx history per address, sorted ascending by height.
//!       Prefix scan O(log n + results).
//!
//!   bal:{script_hex}  → u64 LE (satoshis)
//!       Balance snapshot, updated on every received/spent output.
//!
//!   rich:{(u64::MAX - balance):020}:{script_hex}  → ""
//!       Rich list index: lexicographic scan = highest balance first.
//!
//! Integration:
//!   `index_tx_inputs()` called BEFORE `apply_wire_tx` (reads UTXOs before
//!   they're removed from utxo_db).
//!   `index_tx_outputs()` called AFTER `apply_wire_tx` (reads directly from WireTx).

use std::path::{Path, PathBuf};

use rocksdb::{Direction, IteratorMode, Options, DB};

use crate::pkt_sync::SyncError;
use crate::pkt_utxo_sync::{UtxoSyncDb, WireTx};

// ── AddrIndexDb ───────────────────────────────────────────────────────────────

pub struct AddrIndexDb {
    db:   DB,
    path: PathBuf,
}

/// One entry in a per-address tx history list.
pub struct AddrTxEntry {
    pub height: u64,
    pub txid:   String, // lowercase hex
}

impl AddrIndexDb {
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
        let path = std::env::temp_dir().join(format!("pkt_addrdb_{}", ts));
        Self::open(&path)
    }

    pub fn path(&self) -> &Path { &self.path }

    // ── Key constructors ───────────────────────────────────────────────────────

    fn bal_key(script_hex: &str) -> String {
        format!("bal:{}", script_hex)
    }

    fn atx_key(script_hex: &str, height: u64, txid_hex: &str) -> String {
        format!("atx:{}:{:016x}:{}", script_hex, height, txid_hex)
    }

    /// Secondary index: height → txid (one key per tx per block, deduped by RocksDB).
    /// Key: "htx:{height:016x}:{txid_hex}" → ""
    fn htx_key(height: u64, txid_hex: &str) -> String {
        format!("htx:{:016x}:{}", height, txid_hex)
    }

    /// Rich list key: lower `u64::MAX - balance` sorts before higher values,
    /// so scanning prefix "rich:" from start gives highest-balance entries first.
    fn rich_key(balance: u64, script_hex: &str) -> String {
        format!("rich:{:020}:{}", u64::MAX - balance, script_hex)
    }

    // ── Balance helpers ────────────────────────────────────────────────────────

    fn read_balance(&self, script_hex: &str) -> Result<u64, SyncError> {
        match self.db.get(Self::bal_key(script_hex).as_bytes())
            .map_err(|e| SyncError::Db(e.to_string()))?
        {
            None => Ok(0),
            Some(v) if v.len() == 8 => {
                Ok(u64::from_le_bytes(v[..8].try_into().unwrap()))
            }
            Some(_) => Ok(0),
        }
    }

    fn write_balance(&self, script_hex: &str, new_bal: u64, old_bal: u64) -> Result<(), SyncError> {
        // Remove stale rich entry
        if old_bal > 0 {
            self.db.delete(Self::rich_key(old_bal, script_hex).as_bytes())
                .map_err(|e| SyncError::Db(e.to_string()))?;
        }
        // Write new balance
        self.db.put(Self::bal_key(script_hex).as_bytes(), &new_bal.to_le_bytes())
            .map_err(|e| SyncError::Db(e.to_string()))?;
        // Write new rich entry (skip if zero — not a holder)
        if new_bal > 0 {
            self.db.put(Self::rich_key(new_bal, script_hex).as_bytes(), b"")
                .map_err(|e| SyncError::Db(e.to_string()))?;
        }
        Ok(())
    }

    pub fn add_to_balance(&self, script_hex: &str, amount: u64) -> Result<(), SyncError> {
        let old = self.read_balance(script_hex)?;
        self.write_balance(script_hex, old.saturating_add(amount), old)
    }

    pub fn sub_from_balance(&self, script_hex: &str, amount: u64) -> Result<(), SyncError> {
        let old = self.read_balance(script_hex)?;
        self.write_balance(script_hex, old.saturating_sub(amount), old)
    }

    /// Delete a specific key (used by rollback to remove atx entries).
    pub fn delete_key(&self, key: &str) -> Result<(), SyncError> {
        self.db.delete(key.as_bytes()).map_err(|e| SyncError::Db(e.to_string()))
    }

    /// Clear all bal: and rich: entries (before rebuilding from UTXO set).
    pub fn clear_balance_index(&self) -> Result<(), SyncError> {
        for prefix in &["bal:", "rich:"] {
            let mut keys: Vec<Vec<u8>> = Vec::new();
            let mode = IteratorMode::From(prefix.as_bytes(), Direction::Forward);
            for item in self.db.iterator(mode) {
                let (k, _) = item.map_err(|e| SyncError::Db(e.to_string()))?;
                if !k.starts_with(prefix.as_bytes()) { break; }
                keys.push(k.to_vec());
            }
            for key in keys {
                self.db.delete(&key).map_err(|e| SyncError::Db(e.to_string()))?;
            }
        }
        Ok(())
    }

    // ── Indexing ───────────────────────────────────────────────────────────────

    /// Index inputs being spent.  Must be called BEFORE `apply_wire_tx` so the
    /// UTXOs still exist in `utxo_db`.
    pub fn index_tx_inputs(
        &self,
        utxo_db: &UtxoSyncDb,
        tx:      &WireTx,
        txid:    &[u8; 32],
        height:  u64,
    ) -> Result<(), SyncError> {
        let txid_hex = hex::encode(txid);
        // htx: secondary index — one write per tx (idempotent)
        self.db.put(Self::htx_key(height, &txid_hex).as_bytes(), b"")
            .map_err(|e| SyncError::Db(e.to_string()))?;
        for inp in &tx.inputs {
            if inp.is_coinbase() { continue; }
            if let Ok(Some(entry)) = utxo_db.get_utxo(&inp.prev_txid, inp.prev_vout) {
                if entry.script_pubkey.is_empty() { continue; }
                let script = hex::encode(&entry.script_pubkey);
                let atx_key = Self::atx_key(&script, height, &txid_hex);
                self.db.put(atx_key.as_bytes(), b"")
                    .map_err(|e| SyncError::Db(e.to_string()))?;
                self.sub_from_balance(&script, entry.value)?;
            }
            // UTXO not found → silently skip (intra-block spend edge case)
        }
        Ok(())
    }

    /// Index outputs being created.  Values come directly from the WireTx.
    pub fn index_tx_outputs(
        &self,
        tx:     &WireTx,
        txid:   &[u8; 32],
        height: u64,
    ) -> Result<(), SyncError> {
        let txid_hex = hex::encode(txid);
        // htx: secondary index — one write per tx (idempotent)
        self.db.put(Self::htx_key(height, &txid_hex).as_bytes(), b"")
            .map_err(|e| SyncError::Db(e.to_string()))?;
        for out in &tx.outputs {
            if out.script_pubkey.is_empty() { continue; }
            let script = hex::encode(&out.script_pubkey);
            let atx_key = Self::atx_key(&script, height, &txid_hex);
            self.db.put(atx_key.as_bytes(), b"")
                .map_err(|e| SyncError::Db(e.to_string()))?;
            self.add_to_balance(&script, out.value)?;
        }
        Ok(())
    }

    /// TxIDs ở block height cụ thể, từ index htx: (O(log n + results)).
    /// Trả empty Vec nếu htx: chưa được index (data cũ trước v18.4).
    pub fn get_txids_at_height(&self, height: u64, limit: usize) -> Vec<String> {
        let prefix = format!("htx:{:016x}:", height);
        let mode   = IteratorMode::From(prefix.as_bytes(), Direction::Forward);
        let mut out = Vec::new();
        for item in self.db.iterator(mode) {
            if out.len() >= limit { break; }
            let Ok((k, _)) = item else { continue };
            let key = std::str::from_utf8(&k).unwrap_or("");
            if !key.starts_with(&prefix) { break; }
            out.push(key[prefix.len()..].to_string());
        }
        out
    }

    /// TxIDs gần nhất, newest-first. Dùng cho list API cursor-based (v18.5).
    /// `before_height`: None = từ tip, Some(h) = chỉ lấy blocks < h (cursor exclusive).
    pub fn get_recent_txids(&self, before_height: Option<u64>, limit: usize) -> Vec<(u64, String)> {
        let seek_buf = match before_height {
            None    => b"htx:\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff".to_vec(),
            Some(h) => format!("htx:{:016x}:", h).into_bytes(),
        };
        let iter = self.db.iterator(IteratorMode::From(&seek_buf, Direction::Reverse));
        let mut out = Vec::new();
        for item in iter {
            if out.len() >= limit { break; }
            let Ok((k, _)) = item else { continue };
            let key = std::str::from_utf8(&k).unwrap_or("");
            if !key.starts_with("htx:") { break; }
            let mut parts = key.splitn(3, ':');
            let _ = parts.next();
            let h = match parts.next().and_then(|s| u64::from_str_radix(s, 16).ok()) {
                Some(h) => h, None => continue,
            };
            let txid = match parts.next() {
                Some(t) => t.to_string(), None => continue,
            };
            out.push((h, txid));
        }
        out
    }

    // ── Queries ────────────────────────────────────────────────────────────────

    /// Tx history for one address (oldest-first).
    /// `cursor_height`: if `Some(h)`, start from height `h` (inclusive).
    pub fn get_tx_history(
        &self,
        script_hex:    &str,
        cursor_height: Option<u64>,
        limit:         usize,
    ) -> Result<Vec<AddrTxEntry>, SyncError> {
        let prefix = format!("atx:{}:", script_hex);
        let start = match cursor_height {
            Some(h) => format!("atx:{}:{:016x}:", script_hex, h),
            None    => prefix.clone(),
        };
        let mode = IteratorMode::From(start.as_bytes(), Direction::Forward);
        let mut out = Vec::new();
        for item in self.db.iterator(mode) {
            if out.len() >= limit { break; }
            let (k, _) = item.map_err(|e| SyncError::Db(e.to_string()))?;
            let key = std::str::from_utf8(&k).unwrap_or("");
            if !key.starts_with(&prefix) { break; }
            let rest = &key[prefix.len()..]; // "{height:016x}:{txid}"
            if let Some((h_str, txid)) = rest.split_once(':') {
                let height = u64::from_str_radix(h_str, 16).unwrap_or(0);
                out.push(AddrTxEntry { height, txid: txid.to_string() });
            }
        }
        Ok(out)
    }

    /// Balance snapshot for one address (satoshis).
    pub fn get_balance(&self, script_hex: &str) -> Result<u64, SyncError> {
        self.read_balance(script_hex)
    }

    /// Top-N holders by balance (descending).
    pub fn get_rich_list(&self, limit: usize) -> Result<Vec<(String, u64)>, SyncError> {
        let prefix = "rich:";
        let mode = IteratorMode::From(prefix.as_bytes(), Direction::Forward);
        let mut out = Vec::new();
        for item in self.db.iterator(mode) {
            if out.len() >= limit { break; }
            let (k, _) = item.map_err(|e| SyncError::Db(e.to_string()))?;
            let key = std::str::from_utf8(&k).unwrap_or("");
            if !key.starts_with(prefix) { break; }
            let rest = &key[prefix.len()..]; // "{inv:020}:{script}"
            if let Some((inv_str, script)) = rest.split_once(':') {
                let inv: u64 = inv_str.parse().unwrap_or(0);
                let balance = u64::MAX - inv;
                out.push((script.to_string(), balance));
            }
        }
        Ok(out)
    }

    // ── Height tracking ────────────────────────────────────────────────────────

    pub fn get_addr_height(&self) -> Result<Option<u64>, SyncError> {
        match self.db.get(b"meta:addr_height")
            .map_err(|e| SyncError::Db(e.to_string()))?
        {
            None => Ok(None),
            Some(v) if v.len() == 8 => {
                Ok(Some(u64::from_le_bytes(v[..8].try_into().unwrap())))
            }
            Some(_) => Ok(None),
        }
    }

    pub fn set_addr_height(&self, height: u64) -> Result<(), SyncError> {
        self.db.put(b"meta:addr_height", &height.to_le_bytes())
            .map_err(|e| SyncError::Db(e.to_string()))
    }
}

// ── Path helper ───────────────────────────────────────────────────────────────

fn home_path(rel: &str) -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(rel)
}

pub fn default_addr_db_path() -> PathBuf {
    crate::pkt_paths::addr_index()
}

/// Rebuild all balance snapshots (bal: / rich:) by scanning utxo_db.
/// Called after rollback to restore correct balances without storing deltas.
/// Returns number of UTXOs processed.
pub fn rebuild_balances_from_utxo(
    addr_db:  &AddrIndexDb,
    utxo_db:  &crate::pkt_utxo_sync::UtxoSyncDb,
) -> Result<u64, crate::pkt_sync::SyncError> {
    use rocksdb::Direction;
    use crate::pkt_sync::SyncError;
    use crate::pkt_utxo_sync::UtxoEntry;

    addr_db.clear_balance_index()?;

    let prefix = "utxo:";
    let raw    = utxo_db.raw_db();
    let mode   = rocksdb::IteratorMode::From(prefix.as_bytes(), Direction::Forward);
    let mut count = 0u64;

    for item in raw.iterator(mode) {
        let (k, v) = item.map_err(|e| SyncError::Db(e.to_string()))?;
        let key = std::str::from_utf8(&k).unwrap_or("");
        if !key.starts_with(prefix) { break; }
        let entry: UtxoEntry = serde_json::from_slice(&v)
            .map_err(|e| SyncError::Db(e.to_string()))?;
        if !entry.script_pubkey.is_empty() {
            let script_hex = hex::encode(&entry.script_pubkey);
            addr_db.add_to_balance(&script_hex, entry.value)?;
        }
        count += 1;
    }
    Ok(count)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkt_utxo_sync::{UtxoSyncDb, WireTxIn, WireTxOut};

    // open_temp() uses SystemTime — serialize to avoid lock collision.
    static DB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn make_tx(inputs: Vec<WireTxIn>, outputs: Vec<WireTxOut>) -> WireTx {
        WireTx { version: 1, inputs, outputs, locktime: 0 }
    }

    fn coinbase_in() -> WireTxIn {
        WireTxIn {
            prev_txid:  [0u8; 32],
            prev_vout:  0xffff_ffff,
            script_sig: vec![],
            sequence:   0xffff_ffff,
        }
    }

    fn spend_in(txid: [u8; 32], vout: u32) -> WireTxIn {
        WireTxIn { prev_txid: txid, prev_vout: vout, script_sig: vec![], sequence: 0xffff_ffff }
    }

    fn out(value: u64, script: &[u8]) -> WireTxOut {
        WireTxOut { value, script_pubkey: script.to_vec() }
    }

    #[test]
    fn test_open_temp_path_exists() {
        let _g = DB_LOCK.lock().unwrap();
        let db = AddrIndexDb::open_temp().unwrap();
        assert!(db.path().exists());
    }

    #[test]
    fn test_balance_starts_zero() {
        let _g = DB_LOCK.lock().unwrap();
        let db = AddrIndexDb::open_temp().unwrap();
        assert_eq!(db.get_balance("deadbeef").unwrap(), 0);
    }

    #[test]
    fn test_index_outputs_updates_balance() {
        let _g  = DB_LOCK.lock().unwrap();
        let db   = AddrIndexDb::open_temp().unwrap();
        let txid = [0x01u8; 32];
        let tx   = make_tx(vec![coinbase_in()], vec![out(5000, b"\x76\xa9\x14")]);
        db.index_tx_outputs(&tx, &txid, 1).unwrap();
        let script = hex::encode(b"\x76\xa9\x14");
        assert_eq!(db.get_balance(&script).unwrap(), 5000);
    }

    #[test]
    fn test_index_outputs_records_history() {
        let _g   = DB_LOCK.lock().unwrap();
        let db   = AddrIndexDb::open_temp().unwrap();
        let txid = [0x02u8; 32];
        let tx   = make_tx(vec![coinbase_in()], vec![out(1000, b"\x51")]);
        db.index_tx_outputs(&tx, &txid, 5).unwrap();
        let script = hex::encode(b"\x51");
        let hist = db.get_tx_history(&script, None, 10).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].height, 5);
        assert_eq!(hist[0].txid, hex::encode(txid));
    }

    #[test]
    fn test_index_inputs_decreases_balance() {
        let _g  = DB_LOCK.lock().unwrap();
        let adb = AddrIndexDb::open_temp().unwrap();
        let udb = UtxoSyncDb::open_temp().unwrap();
        let prev_txid = [0x10u8; 32];
        let script = b"\x76\xa9";
        let utxo_out = WireTxOut { value: 3000, script_pubkey: script.to_vec() };
        udb.insert_utxo(&prev_txid, 0, &utxo_out, 0).unwrap();
        adb.add_to_balance(&hex::encode(script), 3000).unwrap();

        let spend_tx   = make_tx(vec![spend_in(prev_txid, 0)], vec![]);
        let spend_txid = [0x11u8; 32];
        adb.index_tx_inputs(&udb, &spend_tx, &spend_txid, 10).unwrap();

        assert_eq!(adb.get_balance(&hex::encode(script)).unwrap(), 0);
    }

    #[test]
    fn test_coinbase_input_skipped() {
        let _g  = DB_LOCK.lock().unwrap();
        let adb = AddrIndexDb::open_temp().unwrap();
        let udb = UtxoSyncDb::open_temp().unwrap();
        let tx   = make_tx(vec![coinbase_in()], vec![]);
        let txid = [0x20u8; 32];
        adb.index_tx_inputs(&udb, &tx, &txid, 1).unwrap();
        // No history written for any address
        assert_eq!(adb.get_tx_history("00", None, 10).unwrap().len(), 0);
    }

    #[test]
    fn test_empty_script_skipped() {
        let _g = DB_LOCK.lock().unwrap();
        let db = AddrIndexDb::open_temp().unwrap();
        let tx   = make_tx(vec![coinbase_in()], vec![out(1000, b"")]);
        let txid = [0x30u8; 32];
        db.index_tx_outputs(&tx, &txid, 1).unwrap();
        assert_eq!(db.get_balance("").unwrap(), 0);
        assert_eq!(db.get_tx_history("", None, 10).unwrap().len(), 0);
    }

    #[test]
    fn test_rich_list_sorted_descending() {
        let _g = DB_LOCK.lock().unwrap();
        let db = AddrIndexDb::open_temp().unwrap();
        db.add_to_balance("aa", 100).unwrap();
        db.add_to_balance("bb", 9999).unwrap();
        db.add_to_balance("cc", 500).unwrap();

        let list = db.get_rich_list(10).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0], ("bb".to_string(), 9999));
        assert_eq!(list[1], ("cc".to_string(), 500));
        assert_eq!(list[2], ("aa".to_string(), 100));
    }

    #[test]
    fn test_rich_list_limit() {
        let _g = DB_LOCK.lock().unwrap();
        let db = AddrIndexDb::open_temp().unwrap();
        for i in 0u64..10 {
            db.add_to_balance(&format!("addr{:02}", i), (i + 1) * 100).unwrap();
        }
        assert_eq!(db.get_rich_list(3).unwrap().len(), 3);
    }

    #[test]
    fn test_rich_list_updates_on_balance_change() {
        let _g = DB_LOCK.lock().unwrap();
        let db = AddrIndexDb::open_temp().unwrap();
        db.add_to_balance("xx", 500).unwrap();
        db.add_to_balance("xx", 500).unwrap(); // now 1000
        let list = db.get_rich_list(10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], ("xx".to_string(), 1000));
    }

    #[test]
    fn test_tx_history_multiple_blocks() {
        let _g = DB_LOCK.lock().unwrap();
        let db = AddrIndexDb::open_temp().unwrap();
        let script = hex::encode(b"\x76");
        for h in [1u64, 5, 10, 20] {
            let txid = [h as u8; 32];
            let tx = make_tx(vec![coinbase_in()], vec![out(100, b"\x76")]);
            db.index_tx_outputs(&tx, &txid, h).unwrap();
        }
        let hist = db.get_tx_history(&script, None, 100).unwrap();
        assert_eq!(hist.len(), 4);
        assert_eq!(hist[0].height, 1);
        assert_eq!(hist[3].height, 20);
    }

    #[test]
    fn test_tx_history_cursor_pagination() {
        let _g = DB_LOCK.lock().unwrap();
        let db = AddrIndexDb::open_temp().unwrap();
        let script = hex::encode(b"\x77");
        for h in [1u64, 2, 3, 4, 5] {
            let txid = [h as u8; 32];
            let tx = make_tx(vec![coinbase_in()], vec![out(100, b"\x77")]);
            db.index_tx_outputs(&tx, &txid, h).unwrap();
        }
        // cursor=3 → heights 3, 4, 5
        let hist = db.get_tx_history(&script, Some(3), 100).unwrap();
        assert_eq!(hist.len(), 3);
        assert_eq!(hist[0].height, 3);
    }

    #[test]
    fn test_addr_height_set_get() {
        let _g = DB_LOCK.lock().unwrap();
        let db = AddrIndexDb::open_temp().unwrap();
        assert_eq!(db.get_addr_height().unwrap(), None);
        db.set_addr_height(42).unwrap();
        assert_eq!(db.get_addr_height().unwrap(), Some(42));
    }

    #[test]
    fn test_cumulative_balance_across_blocks() {
        let _g = DB_LOCK.lock().unwrap();
        let db = AddrIndexDb::open_temp().unwrap();
        let script = hex::encode(b"\x52");
        for (h, val) in [(1u64, 1000u64), (2, 2000), (3, 3000)] {
            let txid = [h as u8; 32];
            let tx = make_tx(vec![coinbase_in()], vec![out(val, b"\x52")]);
            db.index_tx_outputs(&tx, &txid, h).unwrap();
        }
        assert_eq!(db.get_balance(&script).unwrap(), 6000);
    }
}
