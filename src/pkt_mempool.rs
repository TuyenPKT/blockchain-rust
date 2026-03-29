#![allow(dead_code)]
//! v23.4 — Mempool Full
//!
//! `PktMempool` thay thế `Mempool` với:
//! - **Fee priority queue**: `BTreeMap<FeeKey, tx_id>` — O(log n) insert/evict/select
//! - **RBF** (Replace-By-Fee): fee mới >= fee cũ × 110%; detect conflict qua spender index
//! - **Max-size eviction**: khi đầy, evict TX có fee_rate thấp nhất
//! - **72h age expiry**: `evict_expired(now)` loại bỏ TX cũ > 72 giờ
//!
//! Indices được giữ đồng bộ qua mọi thao tác thêm/xoá.

use std::collections::{BTreeMap, HashMap};
use crate::transaction::Transaction;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Số TX tối đa trong mempool.
pub const DEFAULT_MAX_SIZE: usize = 5_000;

/// TX cũ hơn 72 giờ bị evict.
pub const MAX_AGE_SECS: u64 = 72 * 3600;

/// RBF: fee mới phải >= fee cũ × 110%.
pub const RBF_MIN_BUMP: f64 = 1.10;

// ── FeeKey ────────────────────────────────────────────────────────────────────

/// Ordering key cho BTreeMap: sort ascending → first = thấp nhất, last = cao nhất.
/// Dùng fee_rate * 1_000_000 làm integer để tránh float comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct FeeKey {
    fee_rate_micro: u64, // fee_rate × 1_000_000, truncated
    seq:            u64, // tie-break: TX đến trước có seq nhỏ hơn
}

impl FeeKey {
    fn new(fee_rate: f64, seq: u64) -> Self {
        FeeKey {
            fee_rate_micro: (fee_rate * 1_000_000.0) as u64,
            seq,
        }
    }
}

// ── PktMempoolEntry ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PktMempoolEntry {
    pub tx:         Transaction,
    pub fee:        u64,   // satoshi
    pub fee_rate:   f64,   // sat/byte
    pub size_bytes: usize,
    pub added_at:   u64,   // unix timestamp (giây)
    key:            FeeKey, // BTreeMap key — lưu lại để remove đúng
}

// ── AddResult ─────────────────────────────────────────────────────────────────

/// Kết quả của `PktMempool::add()`.
#[derive(Debug, PartialEq)]
pub enum AddResult {
    /// TX mới được thêm thành công.
    Added,
    /// TX thay thế TX cũ qua RBF (trả về tx_id của TX bị thay thế).
    Replaced(String),
    /// TX bị từ chối — lý do đính kèm.
    Rejected(String),
}

// ── PktMempool ────────────────────────────────────────────────────────────────

pub struct PktMempool {
    /// tx_id → entry
    entries:      HashMap<String, PktMempoolEntry>,
    /// FeeKey → tx_id; BTreeMap cho phép O(log n) min/max
    by_fee:       BTreeMap<FeeKey, String>,
    /// (prev_txid, output_index) → mempool tx_id — dùng để detect RBF conflict
    spenders:     HashMap<(String, usize), String>,
    /// Số TX tối đa
    pub max_size: usize,
    /// TX cũ hơn `max_age_secs` bị evict
    pub max_age_secs: u64,
    /// Sequence number tăng dần, dùng để tie-break FeeKey
    seq:          u64,
}

impl Default for PktMempool {
    fn default() -> Self { Self::new() }
}

impl PktMempool {
    pub fn new() -> Self {
        PktMempool {
            entries:      HashMap::new(),
            by_fee:       BTreeMap::new(),
            spenders:     HashMap::new(),
            max_size:     DEFAULT_MAX_SIZE,
            max_age_secs: MAX_AGE_SECS,
            seq:          0,
        }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Thêm TX vào mempool.
    ///
    /// - `input_total`: tổng satoshi của tất cả inputs (từ UTXO lookup).
    /// - `now`: unix timestamp hiện tại (để set `added_at`).
    ///
    /// Logic:
    /// 1. Coinbase TX luôn bị từ chối.
    /// 2. TX trùng tx_id → reject.
    /// 3. Input conflict → thử RBF; reject nếu fee bump không đủ.
    /// 4. Mempool đầy → evict TX có fee_rate thấp nhất; reject nếu TX mới có fee_rate thấp hơn.
    /// 5. Insert vào cả ba indices.
    pub fn add(&mut self, tx: Transaction, input_total: u64, now: u64) -> AddResult {
        // Coinbase không vào mempool
        if tx.is_coinbase {
            return AddResult::Rejected("coinbase TX không vào mempool".to_string());
        }

        // Duplicate
        if self.entries.contains_key(&tx.tx_id) {
            return AddResult::Rejected("TX đã tồn tại trong mempool".to_string());
        }

        let fee        = input_total.saturating_sub(tx.total_output());
        let size_bytes = estimate_size(&tx);
        let fee_rate   = if size_bytes > 0 { fee as f64 / size_bytes as f64 } else { 0.0 };

        // Kiểm tra conflict (RBF)
        let conflict = self.find_conflict(&tx);
        if let Some(ref conflict_id) = conflict {
            let old_fee = self.entries[conflict_id].fee;
            // So sánh integer để tránh floating-point rounding: fee*100 >= old_fee*110
            let bump_pct = ((RBF_MIN_BUMP - 1.0) * 100.0).round() as u64; // 10
            if fee * 100 < old_fee * (100 + bump_pct) {
                return AddResult::Rejected(format!(
                    "RBF từ chối: fee {} sat < min {} sat (cần tăng {}%)",
                    fee,
                    (old_fee * (100 + bump_pct)).div_ceil(100),
                    bump_pct,
                ));
            }
            // Evict TX cũ để nhường chỗ
            let old_id = conflict_id.clone();
            self.remove_entry(&old_id);
            self.insert_entry(tx, fee, fee_rate, size_bytes, now);
            return AddResult::Replaced(old_id);
        }

        // Mempool đầy — cần evict lowest trước khi insert
        if self.entries.len() >= self.max_size {
            let min_rate = self.min_fee_rate().unwrap_or(0.0);
            if fee_rate <= min_rate {
                return AddResult::Rejected(format!(
                    "Mempool đầy ({} TX): fee_rate {:.1} <= min {:.1} sat/byte",
                    self.max_size, fee_rate, min_rate
                ));
            }
            self.evict_lowest();
        }

        self.insert_entry(tx, fee, fee_rate, size_bytes, now);
        AddResult::Added
    }

    /// Chọn tối đa `max_count` TX có fee_rate cao nhất (cho miner).
    pub fn select_transactions(&self, max_count: usize) -> Vec<Transaction> {
        self.by_fee.values()
            .rev() // BTreeMap ascending → rev để lấy cao nhất trước
            .take(max_count)
            .filter_map(|tx_id| self.entries.get(tx_id))
            .map(|e| e.tx.clone())
            .collect()
    }

    /// Xoá các TX đã được confirm khỏi mempool.
    pub fn remove_confirmed(&mut self, confirmed_txids: &[String]) {
        for tx_id in confirmed_txids {
            self.remove_entry(tx_id);
        }
    }

    /// Evict các TX cũ hơn `max_age_secs` (gọi định kỳ, ví dụ mỗi phút).
    ///
    /// Trả về số TX đã evict.
    pub fn evict_expired(&mut self, now: u64) -> usize {
        let cutoff = now.saturating_sub(self.max_age_secs);
        let expired: Vec<String> = self.entries.values()
            .filter(|e| e.added_at < cutoff)
            .map(|e| e.tx.tx_id.clone())
            .collect();
        let count = expired.len();
        for tx_id in &expired { self.remove_entry(tx_id); }
        count
    }

    /// Evict TX có fee_rate thấp nhất.
    pub fn evict_lowest(&mut self) {
        if let Some((_, tx_id)) = self.by_fee.iter().next().map(|(k, v)| (*k, v.clone())) {
            self.remove_entry(&tx_id);
        }
    }

    // ── Query ─────────────────────────────────────────────────────────────────

    pub fn len(&self) -> usize { self.entries.len() }

    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    pub fn total_fees(&self) -> u64 {
        self.entries.values().map(|e| e.fee).sum()
    }

    pub fn min_fee_rate(&self) -> Option<f64> {
        self.by_fee.keys().next().map(|k| k.fee_rate_micro as f64 / 1_000_000.0)
    }

    pub fn max_fee_rate(&self) -> Option<f64> {
        self.by_fee.keys().next_back().map(|k| k.fee_rate_micro as f64 / 1_000_000.0)
    }

    pub fn get(&self, tx_id: &str) -> Option<&PktMempoolEntry> {
        self.entries.get(tx_id)
    }

    pub fn contains(&self, tx_id: &str) -> bool {
        self.entries.contains_key(tx_id)
    }

    pub fn entries(&self) -> impl Iterator<Item = &PktMempoolEntry> {
        self.entries.values()
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn insert_entry(&mut self, tx: Transaction, fee: u64, fee_rate: f64, size_bytes: usize, now: u64) {
        self.seq += 1;
        let key = FeeKey::new(fee_rate, self.seq);

        // Cập nhật spender index
        for input in &tx.inputs {
            if !tx.is_coinbase {
                self.spenders.insert((input.tx_id.clone(), input.output_index), tx.tx_id.clone());
            }
        }

        self.by_fee.insert(key, tx.tx_id.clone());
        self.entries.insert(tx.tx_id.clone(), PktMempoolEntry {
            tx, fee, fee_rate, size_bytes, added_at: now, key,
        });
    }

    fn remove_entry(&mut self, tx_id: &str) {
        if let Some(entry) = self.entries.remove(tx_id) {
            self.by_fee.remove(&entry.key);
            for input in &entry.tx.inputs {
                self.spenders.remove(&(input.tx_id.clone(), input.output_index));
            }
        }
    }

    /// Tìm TX trong mempool đang spend cùng input với `tx` (conflict / RBF).
    /// Trả về tx_id của TX đầu tiên bị conflict.
    fn find_conflict(&self, tx: &Transaction) -> Option<String> {
        for input in &tx.inputs {
            let key = (input.tx_id.clone(), input.output_index);
            if let Some(conflict_id) = self.spenders.get(&key) {
                return Some(conflict_id.clone());
            }
        }
        None
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Ước tính kích thước TX (bytes): 10 overhead + 148/input + 34/output.
pub fn estimate_size(tx: &Transaction) -> usize {
    10 + tx.inputs.len() * 148 + tx.outputs.len() * 34
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::{Transaction, TxInput, TxOutput};
    use crate::script::Script;

    const NOW: u64 = 1_000_000;

    fn make_tx(id: &str, inputs: &[(&str, usize)], output_amount: u64) -> Transaction {
        Transaction {
            tx_id:       id.to_string(),
            wtx_id:      id.to_string(),
            is_coinbase: false,
            fee:         0,
            inputs:  inputs.iter().map(|(prev, idx)| TxInput {
                tx_id:        prev.to_string(),
                output_index: *idx,
                script_sig:   Script::empty(),
                sequence:     0xFFFFFFFF,
                witness:      vec![],
            }).collect(),
            outputs: vec![TxOutput {
                amount:        output_amount,
                script_pubkey: Script::empty(),
            }],
        }
    }

    fn coinbase_tx() -> Transaction {
        Transaction::coinbase("miner_addr", 0)
    }

    // ── add basic ─────────────────────────────────────────────────────────────

    #[test]
    fn add_basic_tx() {
        let mut mp = PktMempool::new();
        let tx = make_tx("tx1", &[("prev1", 0)], 900);
        assert_eq!(mp.add(tx, 1000, NOW), AddResult::Added);
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn coinbase_rejected() {
        let mut mp = PktMempool::new();
        let result = mp.add(coinbase_tx(), 50_000_000, NOW);
        assert!(matches!(result, AddResult::Rejected(_)));
        assert_eq!(mp.len(), 0);
    }

    #[test]
    fn duplicate_rejected() {
        let mut mp = PktMempool::new();
        let tx = make_tx("tx1", &[("prev1", 0)], 900);
        mp.add(tx.clone(), 1000, NOW);
        let result = mp.add(tx, 1000, NOW);
        assert!(matches!(result, AddResult::Rejected(_)));
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn zero_fee_accepted() {
        let mut mp = PktMempool::new();
        let tx = make_tx("tx1", &[("prev1", 0)], 1000);
        assert_eq!(mp.add(tx, 1000, NOW), AddResult::Added);
    }

    // ── select_transactions ───────────────────────────────────────────────────

    #[test]
    fn select_ordered_by_fee_rate_desc() {
        let mut mp = PktMempool::new();
        // tx_low: fee=100, size≈192 → fee_rate≈0.52
        let tx_low  = make_tx("low",  &[("p1", 0)], 900);
        // tx_high: fee=1000, size≈192 → fee_rate≈5.2
        let tx_high = make_tx("high", &[("p2", 0)], 0);
        // tx_mid: fee=300
        let tx_mid  = make_tx("mid",  &[("p3", 0)], 700);

        mp.add(tx_low,  1000, NOW);
        mp.add(tx_high, 1000, NOW);
        mp.add(tx_mid,  1000, NOW);

        let selected = mp.select_transactions(3);
        assert_eq!(selected.len(), 3);
        // Đầu tiên phải là tx có fee cao nhất
        assert_eq!(selected[0].tx_id, "high");
    }

    #[test]
    fn select_respects_max_count() {
        let mut mp = PktMempool::new();
        for i in 0..10u64 {
            let tx = make_tx(&format!("tx{}", i), &[(&format!("p{}", i), 0)], i * 10);
            mp.add(tx, 1000, NOW);
        }
        assert_eq!(mp.select_transactions(3).len(), 3);
        assert_eq!(mp.select_transactions(100).len(), 10);
    }

    // ── eviction ──────────────────────────────────────────────────────────────

    #[test]
    fn evict_lowest_when_full() {
        let mut mp = PktMempool::new();
        mp.max_size = 2;

        let tx_low  = make_tx("low",  &[("p1", 0)], 950); // fee=50
        let tx_high = make_tx("high", &[("p2", 0)], 500); // fee=500
        mp.add(tx_low,  1000, NOW);
        mp.add(tx_high, 1000, NOW);

        // Thêm TX có fee_rate cao hơn min → evict low
        let tx_new = make_tx("new", &[("p3", 0)], 0); // fee=1000
        assert_eq!(mp.add(tx_new, 1000, NOW), AddResult::Added);
        assert!(!mp.contains("low"));
        assert_eq!(mp.len(), 2);
    }

    #[test]
    fn reject_when_full_and_fee_too_low() {
        let mut mp = PktMempool::new();
        mp.max_size = 1;
        let tx1 = make_tx("tx1", &[("p1", 0)], 0); // fee=1000, high
        mp.add(tx1, 1000, NOW);

        // TX mới có fee thấp hơn min → reject
        let tx2 = make_tx("tx2", &[("p2", 0)], 999); // fee=1
        assert!(matches!(mp.add(tx2, 1000, NOW), AddResult::Rejected(_)));
        assert_eq!(mp.len(), 1);
    }

    // ── RBF ───────────────────────────────────────────────────────────────────

    #[test]
    fn rbf_accept_sufficient_bump() {
        let mut mp = PktMempool::new();
        // Original TX spending prev1:0, fee=100
        let orig = make_tx("orig", &[("prev1", 0)], 900);
        mp.add(orig, 1000, NOW);

        // Replacement spending same input, fee=150 (150 >= 100 * 1.10 = 110)
        let replacement = make_tx("repl", &[("prev1", 0)], 850);
        let result = mp.add(replacement, 1000, NOW);
        assert_eq!(result, AddResult::Replaced("orig".to_string()));
        assert!(!mp.contains("orig"));
        assert!(mp.contains("repl"));
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn rbf_reject_insufficient_bump() {
        let mut mp = PktMempool::new();
        let orig = make_tx("orig", &[("prev1", 0)], 900); // fee=100
        mp.add(orig, 1000, NOW);

        // fee=105 < 100 * 1.10 = 110 → reject
        let replacement = make_tx("repl", &[("prev1", 0)], 895);
        assert!(matches!(mp.add(replacement, 1000, NOW), AddResult::Rejected(_)));
        assert!(mp.contains("orig"));
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn rbf_exact_minimum_accepted() {
        let mut mp = PktMempool::new();
        let orig = make_tx("orig", &[("prev1", 0)], 900); // fee=100
        mp.add(orig, 1000, NOW);

        // fee=110 == ceil(100 * 1.10) = 110 → accept
        let replacement = make_tx("repl", &[("prev1", 0)], 890);
        assert_eq!(mp.add(replacement, 1000, NOW), AddResult::Replaced("orig".to_string()));
    }

    #[test]
    fn rbf_updates_spender_index() {
        let mut mp = PktMempool::new();
        let orig = make_tx("orig", &[("prev1", 0)], 900);
        mp.add(orig, 1000, NOW);
        assert_eq!(mp.spenders.get(&("prev1".to_string(), 0)), Some(&"orig".to_string()));

        let replacement = make_tx("repl", &[("prev1", 0)], 850);
        mp.add(replacement, 1000, NOW);
        // spender index phải trỏ sang tx mới
        assert_eq!(mp.spenders.get(&("prev1".to_string(), 0)), Some(&"repl".to_string()));
    }

    // ── remove_confirmed ──────────────────────────────────────────────────────

    #[test]
    fn remove_confirmed_cleans_all_indices() {
        let mut mp = PktMempool::new();
        let tx = make_tx("tx1", &[("prev1", 0)], 900);
        mp.add(tx, 1000, NOW);

        mp.remove_confirmed(&["tx1".to_string()]);
        assert_eq!(mp.len(), 0);
        assert!(mp.by_fee.is_empty());
        assert!(!mp.spenders.contains_key(&("prev1".to_string(), 0)));
    }

    // ── age expiry ────────────────────────────────────────────────────────────

    #[test]
    fn evict_expired_removes_old_txs() {
        let mut mp = PktMempool::new();
        mp.max_age_secs = 3600;

        let tx_old = make_tx("old", &[("p1", 0)], 900);
        let tx_new = make_tx("new", &[("p2", 0)], 900);

        mp.add(tx_old, 1000, 1000);          // added_at=1000
        mp.add(tx_new, 1000, 1000 + 3600 + 1); // added_at sau giới hạn

        // now = 1000 + 3600 + 60: tx_old cũ hơn 3600s, tx_new vừa đúng giới hạn
        let evicted = mp.evict_expired(1000 + 3600 + 60);
        assert_eq!(evicted, 1);
        assert!(!mp.contains("old"));
        assert!(mp.contains("new"));
    }

    #[test]
    fn evict_expired_none_when_fresh() {
        let mut mp = PktMempool::new();
        let tx = make_tx("tx1", &[("p1", 0)], 900);
        mp.add(tx, 1000, NOW);
        assert_eq!(mp.evict_expired(NOW + 100), 0);
    }

    // ── fee stats ─────────────────────────────────────────────────────────────

    #[test]
    fn min_max_fee_rate() {
        let mut mp = PktMempool::new();
        let tx_low  = make_tx("low",  &[("p1", 0)], 950); // fee=50
        let tx_high = make_tx("high", &[("p2", 0)], 0);   // fee=1000
        mp.add(tx_low,  1000, NOW);
        mp.add(tx_high, 1000, NOW);

        assert!(mp.min_fee_rate().unwrap() < mp.max_fee_rate().unwrap());
    }

    #[test]
    fn total_fees_sum() {
        let mut mp = PktMempool::new();
        mp.add(make_tx("t1", &[("p1", 0)], 700), 1000, NOW);
        mp.add(make_tx("t2", &[("p2", 0)], 600), 1000, NOW);
        assert_eq!(mp.total_fees(), 300 + 400);
    }

    #[test]
    fn fee_stats_none_when_empty() {
        let mp = PktMempool::new();
        assert!(mp.min_fee_rate().is_none());
        assert!(mp.max_fee_rate().is_none());
    }
}
