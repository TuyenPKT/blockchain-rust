#![allow(dead_code)]

/// v5.4 — Fee Market: Dynamic fee estimation + RBF (Replace-By-Fee)
///
/// FeeEstimator:
///   - Theo dõi fee_rate của các TX trong block gần nhất (sliding window 20 blocks)
///   - Trả về ước tính fee cho 3 mức xác nhận:
///       fast   = 1 block  (~90th percentile)
///       medium = 3 blocks (~50th percentile)
///       slow   = 6 blocks (~10th percentile)
///
/// RBF (Replace-By-Fee):
///   - Cho phép thay thế TX trong mempool bằng TX mới cùng inputs nhưng fee cao hơn
///   - Yêu cầu fee mới >= fee cũ * 110% (RBF_MIN_BUMP = 1.10)

use std::collections::VecDeque;
use crate::transaction::Transaction;
use crate::mempool::{Mempool, MempoolEntry};

/// Minimum fee bump ratio để RBF được chấp nhận (10% tăng)
pub const RBF_MIN_BUMP: f64 = 1.10;

/// Số block gần nhất dùng để ước tính fee
const HISTORY_BLOCKS: usize = 20;

// ─── Fee Estimate ─────────────────────────────────────────────────────────────

/// Kết quả ước tính fee cho các mục tiêu xác nhận khác nhau
#[derive(Debug, Clone)]
pub struct FeeEstimate {
    /// ~1 block (90th percentile) — dành cho TX cần xác nhận nhanh
    pub fast_sat_per_byte:   f64,
    /// ~3 blocks (50th percentile) — mức trung bình
    pub medium_sat_per_byte: f64,
    /// ~6 blocks (10th percentile) — giá rẻ, chờ lâu
    pub slow_sat_per_byte:   f64,
    /// Fee tối thiểu tuyệt đối (1 sat/byte)
    pub min_sat_per_byte:    f64,
}

impl Default for FeeEstimate {
    fn default() -> Self {
        // Giá trị mặc định khi chưa có lịch sử block
        FeeEstimate {
            fast_sat_per_byte:   10.0,
            medium_sat_per_byte: 5.0,
            slow_sat_per_byte:   1.0,
            min_sat_per_byte:    1.0,
        }
    }
}

impl FeeEstimate {
    /// Ước tính fee (sat) cho một TX có kích thước bytes
    pub fn fee_for_size(&self, size_bytes: usize, target: ConfTarget) -> u64 {
        let rate = match target {
            ConfTarget::Fast   => self.fast_sat_per_byte,
            ConfTarget::Medium => self.medium_sat_per_byte,
            ConfTarget::Slow   => self.slow_sat_per_byte,
        };
        (rate * size_bytes as f64).ceil() as u64
    }

    pub fn print(&self) {
        println!("  Fee estimate (sat/byte):");
        println!("    Fast   (~1 block) : {:.1}", self.fast_sat_per_byte);
        println!("    Medium (~3 blocks): {:.1}", self.medium_sat_per_byte);
        println!("    Slow   (~6 blocks): {:.1}", self.slow_sat_per_byte);
        println!("    Min               : {:.1}", self.min_sat_per_byte);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ConfTarget { Fast, Medium, Slow }

// ─── Fee Estimator ────────────────────────────────────────────────────────────

/// Theo dõi lịch sử fee_rate từ các block đã confirm để ước tính fee động
pub struct FeeEstimator {
    /// Mỗi phần tử = sorted fee rates (sat/byte) của tất cả TX trong 1 block
    history: VecDeque<Vec<f64>>,
}

impl Default for FeeEstimator {
    fn default() -> Self { Self::new() }
}

impl FeeEstimator {
    pub fn new() -> Self {
        FeeEstimator { history: VecDeque::with_capacity(HISTORY_BLOCKS) }
    }

    /// Ghi nhận fee data từ 1 block mới được confirm
    pub fn record_block(&mut self, txs: &[Transaction]) {
        let mut rates: Vec<f64> = txs.iter()
            .filter(|t| !t.is_coinbase && t.fee > 0)
            .map(|t| {
                let size = Mempool::estimate_size(t).max(1);
                t.fee as f64 / size as f64
            })
            .collect();

        if rates.is_empty() { return; }

        rates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        if self.history.len() >= HISTORY_BLOCKS {
            self.history.pop_front();
        }
        self.history.push_back(rates);
    }

    /// Tính fee estimate dựa trên lịch sử gần nhất
    pub fn estimate(&self) -> FeeEstimate {
        if self.history.is_empty() {
            return FeeEstimate::default();
        }

        let mut all_rates: Vec<f64> = self.history.iter().flatten().cloned().collect();
        if all_rates.is_empty() {
            return FeeEstimate::default();
        }
        all_rates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        FeeEstimate {
            fast_sat_per_byte:   percentile(&all_rates, 0.90).max(1.0),
            medium_sat_per_byte: percentile(&all_rates, 0.50).max(1.0),
            slow_sat_per_byte:   percentile(&all_rates, 0.10).max(1.0),
            min_sat_per_byte:    all_rates.first().copied().unwrap_or(1.0).max(1.0),
        }
    }

    /// Xây dựng lại estimator từ chain đã load (gọi sau khi load từ DB)
    pub fn rebuild_from_blocks(blocks: &[crate::block::Block]) -> Self {
        let mut est = Self::new();
        let start = blocks.len().saturating_sub(HISTORY_BLOCKS);
        for block in &blocks[start..] {
            est.record_block(&block.transactions);
        }
        est
    }

    pub fn history_depth(&self) -> usize { self.history.len() }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx = (p * (sorted.len().saturating_sub(1)) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ─── RBF (Replace-By-Fee) ─────────────────────────────────────────────────────

/// Kết quả kiểm tra RBF conflict
#[derive(Debug)]
pub enum RbfConflict<'a> {
    /// Không có TX nào trong mempool dùng cùng inputs
    None,
    /// Tìm thấy TX conflict, có thể replace nếu fee đủ
    Found { old_tx_id: &'a str, old_fee: u64 },
    /// TX conflict nhưng fee mới không đủ để replace
    InsufficientBump { old_tx_id: &'a str, old_fee: u64, min_required: u64 },
}

/// Tìm TX trong mempool có input trùng với new_tx (potential RBF)
pub fn find_rbf_conflict<'a>(mempool: &'a Mempool, new_tx: &Transaction) -> Option<&'a MempoolEntry> {
    let new_inputs: std::collections::HashSet<String> = new_tx.inputs.iter()
        .map(|i| format!("{}:{}", i.tx_id, i.output_index))
        .collect();

    mempool.entries.values().find(|entry| {
        entry.tx.inputs.iter().any(|inp| {
            new_inputs.contains(&format!("{}:{}", inp.tx_id, inp.output_index))
        })
    })
}

/// Validate fee bump đủ để RBF (yêu cầu tăng ít nhất RBF_MIN_BUMP = 10%)
pub fn is_valid_rbf_bump(old_fee: u64, new_fee: u64) -> bool {
    new_fee as f64 >= old_fee as f64 * RBF_MIN_BUMP
}

/// Kiểm tra và thực hiện RBF trong mempool nếu hợp lệ
/// Trả về Some(old_tx_id) nếu đã replace, None nếu không có conflict
/// Trả về Err nếu conflict nhưng fee không đủ
pub fn try_rbf_replace(mempool: &mut Mempool, new_tx: &Transaction, new_fee: u64) -> Result<Option<String>, String> {
    // Tìm TX conflict
    let conflict = {
        let new_inputs: std::collections::HashSet<String> = new_tx.inputs.iter()
            .map(|i| format!("{}:{}", i.tx_id, i.output_index))
            .collect();

        mempool.entries.values()
            .find(|entry| {
                entry.tx.inputs.iter().any(|inp|
                    new_inputs.contains(&format!("{}:{}", inp.tx_id, inp.output_index))
                )
            })
            .map(|e| (e.tx.tx_id.clone(), e.fee))
    };

    match conflict {
        None => Ok(None),
        Some((old_id, old_fee)) => {
            if !is_valid_rbf_bump(old_fee, new_fee) {
                let min_required = (old_fee as f64 * RBF_MIN_BUMP).ceil() as u64;
                return Err(format!(
                    "RBF từ chối: fee mới {} sat < min {} sat (old {} sat × {:.0}%)",
                    new_fee, min_required, old_fee, RBF_MIN_BUMP * 100.0
                ));
            }
            // Xóa TX cũ, cho phép add TX mới
            mempool.entries.remove(&old_id);
            println!("  🔄 RBF: thay thế TX {}...  fee {} → {} sat (+{:.0}%)",
                &old_id[..12.min(old_id.len())], old_fee, new_fee,
                (new_fee as f64 / old_fee as f64 - 1.0) * 100.0);
            Ok(Some(old_id))
        }
    }
}
