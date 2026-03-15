use std::collections::HashMap;
use crate::transaction::Transaction;
use crate::fee_market;

/// Một entry trong mempool — TX + fee tính được
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MempoolEntry {
    pub tx:         Transaction,
    pub fee:        u64,   // satoshi
    pub fee_rate:   f64,   // satoshi / byte (dùng để sort)
    pub size_bytes: usize, // kích thước TX ước tính
}

/// Mempool = nơi chứa TX đang chờ được đưa vào block
/// Miner chọn TX có fee cao nhất để tối đa hóa lợi nhuận
pub struct Mempool {
    pub entries: HashMap<String, MempoolEntry>, // key = tx_id
    pub max_size: usize, // giới hạn số TX trong mempool
}

impl Default for Mempool {
    fn default() -> Self { Self::new() }
}

impl Mempool {
    pub fn new() -> Self {
        Mempool {
            entries:  HashMap::new(),
            max_size: 5000, // Bitcoin giới hạn 300MB, ở đây dùng 5000 TX
        }
    }

    /// Thêm TX vào mempool với fee đã biết (input_total - output_total)
    pub fn add(&mut self, tx: Transaction, input_total: u64) -> Result<(), String> {
        if self.entries.contains_key(&tx.tx_id) {
            return Err("TX đã có trong mempool".to_string());
        }
        let fee        = input_total.saturating_sub(tx.total_output());
        let size_bytes = Self::estimate_size(&tx);
        if self.entries.len() >= self.max_size {
            // Nếu đầy, xóa TX có fee thấp nhất nếu TX mới có fee cao hơn
            let min_fee = self.entries.values().map(|e| e.fee).min().unwrap_or(0);
            if fee <= min_fee {
                return Err("Mempool đầy, fee quá thấp".to_string());
            }
            self.evict_lowest_fee();
        }
        let fee_rate   = if size_bytes > 0 { fee as f64 / size_bytes as f64 } else { 0.0 };

        println!(
            "  📥 Mempool: nhận TX {} | fee={} sat | fee_rate={:.1} sat/byte",
            &tx.tx_id[..12], fee, fee_rate
        );

        self.entries.insert(tx.tx_id.clone(), MempoolEntry {
            tx, fee, fee_rate, size_bytes,
        });
        Ok(())
    }

    /// Miner chọn tối đa `max_count` TX có fee_rate cao nhất
    /// Đây là "greedy selection" — Bitcoin Core dùng thuật toán phức tạp hơn
    pub fn select_transactions(&self, max_count: usize) -> Vec<Transaction> {
        let mut entries: Vec<&MempoolEntry> = self.entries.values().collect();

        // Sort giảm dần theo fee_rate
        entries.sort_by(|a, b| b.fee_rate.partial_cmp(&a.fee_rate).unwrap_or(std::cmp::Ordering::Equal));

        entries.iter()
            .take(max_count)
            .map(|e| e.tx.clone())
            .collect()
    }

    /// Sau khi block được mine, xóa các TX đã được confirm
    pub fn remove_confirmed(&mut self, confirmed_tx_ids: &[String]) {
        for tx_id in confirmed_tx_ids {
            if self.entries.remove(tx_id).is_some() {
                println!("  ✅ Mempool: TX {} đã confirm, xóa khỏi mempool", &tx_id[..12]);
            }
        }
    }

    /// Thêm TX với hỗ trợ RBF: tự động thay thế TX cũ nếu cùng inputs và fee đủ cao
    /// Trả về Ok(Some(old_tx_id)) nếu replaced, Ok(None) nếu TX mới bình thường
    #[allow(dead_code)]
    pub fn add_or_replace(&mut self, tx: Transaction, input_total: u64) -> Result<Option<String>, String> {
        let fee = input_total.saturating_sub(tx.total_output());
        match fee_market::try_rbf_replace(self, &tx, fee)? {
            Some(old_id) => {
                self.add(tx, input_total)?;
                Ok(Some(old_id))
            }
            None => {
                self.add(tx, input_total)?;
                Ok(None)
            }
        }
    }

    /// Xóa TX có fee thấp nhất khi mempool đầy
    fn evict_lowest_fee(&mut self) {
        if let Some(tx_id) = self.entries.values()
            .min_by(|a, b| a.fee.cmp(&b.fee))
            .map(|e| e.tx.tx_id.clone())
        {
            self.entries.remove(&tx_id);
            println!("  🗑️  Mempool: evict TX {} (fee thấp nhất)", &tx_id[..12]);
        }
    }

    /// Ước tính kích thước TX theo bytes
    /// Bitcoin thật: mỗi input ~148 bytes, mỗi output ~34 bytes, overhead ~10 bytes
    pub fn estimate_size(tx: &Transaction) -> usize {
        10 + tx.inputs.len() * 148 + tx.outputs.len() * 34
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize { self.entries.len() }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// Tổng fee đang chờ trong mempool
    #[allow(dead_code)]
    pub fn total_pending_fees(&self) -> u64 {
        self.entries.values().map(|e| e.fee).sum()
    }

    /// In trạng thái mempool
    #[allow(dead_code)]
    pub fn print_status(&self) {
        println!("  📊 Mempool: {} TX | tổng fee chờ: {} sat", self.len(), self.total_pending_fees());
        let mut entries: Vec<&MempoolEntry> = self.entries.values().collect();
        entries.sort_by(|a, b| b.fee_rate.partial_cmp(&a.fee_rate).unwrap_or(std::cmp::Ordering::Equal));
        for e in &entries {
            println!(
                "    TX {}... | {} sat | {:.1} sat/byte",
                &e.tx.tx_id[..12], e.fee, e.fee_rate
            );
        }
    }
}
