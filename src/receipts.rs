#![allow(dead_code)]
//! v26.1 — Transaction Receipt Store (redb-backed)
//!
//! Persists EVM execution receipts per transaction:
//!   - tx_hash → TxReceipt (status, gas_used, logs, bloom)
//!   - block_hash → Vec<tx_hash> (receipts per block)
//!
//! Schema:
//!   TABLE receipts_by_tx:    tx_hash[32]  → JSON<TxReceipt>
//!   TABLE receipts_by_block: block_hash[32] → JSON<Vec<[u8;32]>>

use std::path::Path;
use redb::{Database, ReadableTable, TableDefinition};

use crate::eth_wire::TxReceipt;

// ─── Table definitions ────────────────────────────────────────────────────────

const TABLE_BY_TX:    TableDefinition<&[u8], &str> = TableDefinition::new("receipts_by_tx");
const TABLE_BY_BLOCK: TableDefinition<&[u8], &str> = TableDefinition::new("receipts_by_block");

// ─── Bloom filter (simplified 256-byte) ──────────────────────────────────────

pub fn bloom_add(bloom: &mut [u8; 256], data: &[u8]) {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(data);
    for i in 0..3 {
        let bit = u16::from_be_bytes([hash[i * 2], hash[i * 2 + 1]]) as usize % 2048;
        bloom[bit / 8] |= 1 << (bit % 8);
    }
}

pub fn bloom_test(bloom: &[u8; 256], data: &[u8]) -> bool {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(data);
    for i in 0..3 {
        let bit = u16::from_be_bytes([hash[i * 2], hash[i * 2 + 1]]) as usize % 2048;
        if bloom[bit / 8] & (1 << (bit % 8)) == 0 { return false; }
    }
    true
}

/// Build bloom filter from a list of receipts (addresses + topics).
pub fn build_block_bloom(receipts: &[TxReceipt]) -> [u8; 256] {
    let mut bloom = [0u8; 256];
    for r in receipts {
        for log in &r.logs {
            bloom_add(&mut bloom, &log.address);
            for topic in &log.topics {
                bloom_add(&mut bloom, topic);
            }
        }
    }
    bloom
}

// ─── Receipt DB ───────────────────────────────────────────────────────────────

pub struct ReceiptDb {
    db: Database,
}

impl ReceiptDb {
    pub fn open(path: &Path) -> Result<Self, String> {
        let db = Database::create(path).map_err(|e| e.to_string())?;
        // Ensure tables exist
        let wtx = db.begin_write().map_err(|e| e.to_string())?;
        { let _ = wtx.open_table(TABLE_BY_TX).map_err(|e| e.to_string())?; }
        { let _ = wtx.open_table(TABLE_BY_BLOCK).map_err(|e| e.to_string())?; }
        wtx.commit().map_err(|e| e.to_string())?;
        Ok(ReceiptDb { db })
    }

    /// Store receipts for a block.
    pub fn put_block_receipts(
        &self,
        block_hash: &[u8; 32],
        receipts: &[TxReceipt],
    ) -> Result<(), String> {
        let wtx = self.db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut tbl_tx    = wtx.open_table(TABLE_BY_TX).map_err(|e| e.to_string())?;
            let mut tbl_block = wtx.open_table(TABLE_BY_BLOCK).map_err(|e| e.to_string())?;

            let mut tx_hashes: Vec<[u8; 32]> = vec![];
            for r in receipts {
                let json = serde_json::to_string(r).map_err(|e| e.to_string())?;
                tbl_tx.insert(r.tx_hash.as_slice(), json.as_str()).map_err(|e| e.to_string())?;
                tx_hashes.push(r.tx_hash);
            }
            let hashes_json = serde_json::to_string(&tx_hashes).map_err(|e| e.to_string())?;
            tbl_block.insert(block_hash.as_slice(), hashes_json.as_str()).map_err(|e| e.to_string())?;
        }
        wtx.commit().map_err(|e| e.to_string())
    }

    /// Get receipt for a single transaction.
    pub fn get_tx_receipt(&self, tx_hash: &[u8; 32]) -> Result<Option<TxReceipt>, String> {
        let rtx = self.db.begin_read().map_err(|e| e.to_string())?;
        let tbl = rtx.open_table(TABLE_BY_TX).map_err(|e| e.to_string())?;
        match tbl.get(tx_hash.as_slice()).map_err(|e| e.to_string())? {
            None => Ok(None),
            Some(v) => {
                let r = serde_json::from_str(v.value()).map_err(|e| e.to_string())?;
                Ok(Some(r))
            }
        }
    }

    /// Get all receipts for a block.
    pub fn get_block_receipts(&self, block_hash: &[u8; 32]) -> Result<Vec<TxReceipt>, String> {
        let rtx = self.db.begin_read().map_err(|e| e.to_string())?;
        let tbl_block = rtx.open_table(TABLE_BY_BLOCK).map_err(|e| e.to_string())?;
        let tbl_tx    = rtx.open_table(TABLE_BY_TX).map_err(|e| e.to_string())?;

        let Some(hashes_raw) = tbl_block.get(block_hash.as_slice()).map_err(|e| e.to_string())? else {
            return Ok(vec![]);
        };
        let hashes: Vec<[u8; 32]> = serde_json::from_str(hashes_raw.value()).map_err(|e| e.to_string())?;

        let mut receipts = vec![];
        for h in &hashes {
            if let Some(v) = tbl_tx.get(h.as_slice()).map_err(|e| e.to_string())? {
                let r: TxReceipt = serde_json::from_str(v.value()).map_err(|e| e.to_string())?;
                receipts.push(r);
            }
        }
        Ok(receipts)
    }

    /// Count total receipts stored.
    pub fn count(&self) -> Result<u64, String> {
        let rtx = self.db.begin_read().map_err(|e| e.to_string())?;
        let tbl = rtx.open_table(TABLE_BY_TX).map_err(|e| e.to_string())?;
        let mut n = 0u64;
        for item in tbl.iter().map_err(|e| e.to_string())? {
            item.map_err(|e| e.to_string())?;
            n += 1;
        }
        Ok(n)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use crate::eth_wire::ReceiptLog;

    static LOCK: Mutex<()> = Mutex::new(());

    fn open_temp() -> ReceiptDb {
        let _guard = LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep();
        ReceiptDb::open(&path.join("receipts.redb")).unwrap()
    }

    fn receipt(tx_byte: u8, status: u8, gas: u64) -> TxReceipt {
        let mut h = [0u8; 32];
        h[0] = tx_byte;
        TxReceipt { tx_hash: h, gas_used: gas, status, logs: vec![] }
    }

    fn block_hash(b: u8) -> [u8; 32] { let mut h = [0u8; 32]; h[0] = b; h }

    #[test]
    fn test_open() {
        open_temp(); // should not panic
    }

    #[test]
    fn test_put_and_get_tx_receipt() {
        let db = open_temp();
        let r  = receipt(1, 1, 21_000);
        db.put_block_receipts(&block_hash(1), &[r.clone()]).unwrap();
        let got = db.get_tx_receipt(&r.tx_hash).unwrap().unwrap();
        assert_eq!(got.gas_used, 21_000);
        assert_eq!(got.status, 1);
    }

    #[test]
    fn test_get_missing_receipt_returns_none() {
        let db = open_temp();
        let h = [0xFFu8; 32];
        assert!(db.get_tx_receipt(&h).unwrap().is_none());
    }

    #[test]
    fn test_get_block_receipts() {
        let db = open_temp();
        let bh = block_hash(2);
        let r1 = receipt(1, 1, 21_000);
        let r2 = receipt(2, 0, 50_000);
        db.put_block_receipts(&bh, &[r1.clone(), r2.clone()]).unwrap();
        let all = db.get_block_receipts(&bh).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|r| r.tx_hash == r1.tx_hash));
        assert!(all.iter().any(|r| r.tx_hash == r2.tx_hash));
    }

    #[test]
    fn test_get_block_receipts_empty_block() {
        let db = open_temp();
        let all = db.get_block_receipts(&block_hash(99)).unwrap();
        assert!(all.is_empty());
    }

    #[test]
    fn test_count_increases() {
        let db = open_temp();
        assert_eq!(db.count().unwrap(), 0);
        db.put_block_receipts(&block_hash(1), &[receipt(1, 1, 100)]).unwrap();
        assert_eq!(db.count().unwrap(), 1);
        db.put_block_receipts(&block_hash(2), &[receipt(2, 1, 200), receipt(3, 0, 300)]).unwrap();
        assert_eq!(db.count().unwrap(), 3);
    }

    #[test]
    fn test_bloom_add_and_test() {
        let mut bloom = [0u8; 256];
        let addr = [0xABu8; 20];
        bloom_add(&mut bloom, &addr);
        assert!(bloom_test(&bloom, &addr));
    }

    #[test]
    fn test_bloom_absent() {
        let bloom = [0u8; 256];
        assert!(!bloom_test(&bloom, &[0x01u8; 20]));
    }

    #[test]
    fn test_build_block_bloom() {
        let log = ReceiptLog {
            address: [0x11u8; 20],
            topics:  vec![[0x22u8; 32]],
            data:    vec![],
        };
        let r = TxReceipt { tx_hash: [0u8; 32], gas_used: 0, status: 1, logs: vec![log] };
        let bloom = build_block_bloom(&[r]);
        assert!(bloom_test(&bloom, &[0x11u8; 20]));
        assert!(bloom_test(&bloom, &[0x22u8; 32]));
        assert!(!bloom_test(&bloom, &[0x33u8; 20]));
    }

    #[test]
    fn test_failed_receipt_stored() {
        let db = open_temp();
        let r = receipt(9, 0, 30_000); // status=0 = failed
        db.put_block_receipts(&block_hash(5), &[r.clone()]).unwrap();
        let got = db.get_tx_receipt(&r.tx_hash).unwrap().unwrap();
        assert_eq!(got.status, 0);
    }
}
