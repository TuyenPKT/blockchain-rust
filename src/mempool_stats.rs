#![allow(dead_code)]
//! v8.4 — Mempool Explorer Stats
//!
//! Computes fee distribution, size summary, and percentile fee estimates
//! from a `Mempool` snapshot.  Used by the PKTScan `/api/mempool` endpoint.
//!
//! API:
//!   MempoolStats::compute(mempool)  → full stats snapshot
//!   FeeBucket                       → count + total_fees per fee-rate band
//!   FeePercentiles                  → p25 / p50 / p75 / p90 fee rates

use crate::mempool::Mempool;

// ─── Fee bucket bands (sat/byte) ──────────────────────────────────────────────

/// (label, min_rate_inclusive, max_rate_exclusive)
const BANDS: &[(&str, f64, f64)] = &[
    ("0-1",   0.0,  1.0),
    ("1-5",   1.0,  5.0),
    ("5-10",  5.0, 10.0),
    ("10-50", 10.0, 50.0),
    ("50+",   50.0, f64::MAX),
];

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FeeBucket {
    pub label:       &'static str,
    pub min_rate:    f64,
    pub max_rate:    f64,
    pub count:       usize,
    pub total_fees:  u64,
}

#[derive(Debug, Clone)]
pub struct FeePercentiles {
    pub p25: f64,
    pub p50: f64,
    pub p75: f64,
    pub p90: f64,
}

#[derive(Debug, Clone)]
pub struct MempoolStats {
    pub count:            usize,
    pub total_fees:       u64,
    pub total_size_bytes: usize,
    pub min_fee_rate:     f64,
    pub max_fee_rate:     f64,
    pub avg_fee_rate:     f64,
    pub percentiles:      FeePercentiles,
    pub fee_buckets:      Vec<FeeBucket>,
}

impl MempoolStats {
    /// Compute a full stats snapshot from a `Mempool`.
    pub fn compute(mp: &Mempool) -> Self {
        let count            = mp.entries.len();
        let total_fees: u64  = mp.entries.values().map(|e| e.fee).sum();
        let total_size_bytes = mp.entries.values().map(|e| e.size_bytes).sum();

        if count == 0 {
            return MempoolStats {
                count, total_fees, total_size_bytes,
                min_fee_rate: 0.0,
                max_fee_rate: 0.0,
                avg_fee_rate: 0.0,
                percentiles: FeePercentiles { p25: 0.0, p50: 0.0, p75: 0.0, p90: 0.0 },
                fee_buckets: Self::empty_buckets(),
            };
        }

        let mut rates: Vec<f64> = mp.entries.values().map(|e| e.fee_rate).collect();
        rates.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let min_fee_rate = rates.first().copied().unwrap_or(0.0);
        let max_fee_rate = rates.last().copied().unwrap_or(0.0);
        let avg_fee_rate = rates.iter().sum::<f64>() / count as f64;

        let percentiles = FeePercentiles {
            p25: percentile(&rates, 25),
            p50: percentile(&rates, 50),
            p75: percentile(&rates, 75),
            p90: percentile(&rates, 90),
        };

        // Build fee buckets
        let mut buckets: Vec<FeeBucket> = BANDS.iter().map(|(label, min, max)| FeeBucket {
            label, min_rate: *min, max_rate: *max, count: 0, total_fees: 0,
        }).collect();

        for entry in mp.entries.values() {
            for bucket in &mut buckets {
                if entry.fee_rate >= bucket.min_rate && entry.fee_rate < bucket.max_rate {
                    bucket.count      += 1;
                    bucket.total_fees += entry.fee;
                    break;
                }
            }
        }

        MempoolStats {
            count,
            total_fees,
            total_size_bytes,
            min_fee_rate,
            max_fee_rate,
            avg_fee_rate,
            percentiles,
            fee_buckets: buckets,
        }
    }

    fn empty_buckets() -> Vec<FeeBucket> {
        BANDS.iter().map(|(label, min, max)| FeeBucket {
            label, min_rate: *min, max_rate: *max, count: 0, total_fees: 0,
        }).collect()
    }

    /// Suggested "fast" fee rate: p90 or 1.0 sat/byte minimum.
    pub fn suggested_fast_fee(&self) -> f64 {
        self.percentiles.p90.max(1.0)
    }

    /// Suggested "economy" fee rate: p25 or 1.0 sat/byte minimum.
    pub fn suggested_economy_fee(&self) -> f64 {
        self.percentiles.p25.max(1.0)
    }
}

/// Returns the `pct`-th percentile from a sorted slice.
fn percentile(sorted: &[f64], pct: usize) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx = ((pct as f64 / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mempool::Mempool;
    use crate::transaction::{Transaction, TxInput, TxOutput};
    use crate::script::Script;

    // 40 hex chars = valid P2PKH pubkey hash
    const ADDR: &str = "aabbccdd00112233445566778899aabbccddeeff";

    fn make_tx(id_suffix: u8, output_amount: u64) -> Transaction {
        Transaction {
            tx_id:       format!("{:064x}", id_suffix),
            wtx_id:      format!("{:064x}", id_suffix),
            inputs:      vec![TxInput {
                tx_id:        format!("{:064x}", id_suffix + 100),
                output_index: 0,
                script_sig:   Script::empty(),
                sequence:     0xFFFFFFFF,
                witness:      vec![],
            }],
            outputs:     vec![TxOutput::p2pkh(output_amount, ADDR)],
            is_coinbase: false,
            fee:         0,
        }
    }

    fn populated_mempool() -> Mempool {
        let mut mp = Mempool::new();
        // fees: 10, 50, 100, 200, 500  → rates vary by tx size
        for (i, fee) in [(1u8, 10u64), (2, 50), (3, 100), (4, 200), (5, 500)] {
            let tx = make_tx(i, 1000);
            let input_total = 1000 + fee;
            mp.add(tx, input_total).ok();
        }
        mp
    }

    // ── empty mempool ──────────────────────────────────────────────────────

    #[test]
    fn test_empty_mempool_stats() {
        let mp = Mempool::new();
        let s  = MempoolStats::compute(&mp);
        assert_eq!(s.count, 0);
        assert_eq!(s.total_fees, 0);
        assert_eq!(s.total_size_bytes, 0);
        assert_eq!(s.min_fee_rate, 0.0);
        assert_eq!(s.max_fee_rate, 0.0);
        assert_eq!(s.avg_fee_rate, 0.0);
    }

    #[test]
    fn test_empty_buckets_all_zero() {
        let mp = Mempool::new();
        let s  = MempoolStats::compute(&mp);
        assert_eq!(s.fee_buckets.len(), 5);
        for b in &s.fee_buckets {
            assert_eq!(b.count, 0);
        }
    }

    // ── populated mempool ─────────────────────────────────────────────────

    #[test]
    fn test_count() {
        let mp = populated_mempool();
        let s  = MempoolStats::compute(&mp);
        assert_eq!(s.count, 5);
    }

    #[test]
    fn test_total_fees() {
        let mp = populated_mempool();
        let s  = MempoolStats::compute(&mp);
        assert_eq!(s.total_fees, 10 + 50 + 100 + 200 + 500);
    }

    #[test]
    fn test_total_size_bytes_positive() {
        let mp = populated_mempool();
        let s  = MempoolStats::compute(&mp);
        assert!(s.total_size_bytes > 0);
    }

    #[test]
    fn test_min_max_fee_rate_ordered() {
        let mp = populated_mempool();
        let s  = MempoolStats::compute(&mp);
        assert!(s.min_fee_rate <= s.max_fee_rate);
        assert!(s.min_fee_rate >= 0.0);
    }

    #[test]
    fn test_avg_fee_rate_in_range() {
        let mp = populated_mempool();
        let s  = MempoolStats::compute(&mp);
        assert!(s.avg_fee_rate >= s.min_fee_rate);
        assert!(s.avg_fee_rate <= s.max_fee_rate);
    }

    // ── percentiles ───────────────────────────────────────────────────────

    #[test]
    fn test_percentiles_ordered() {
        let mp = populated_mempool();
        let s  = MempoolStats::compute(&mp);
        assert!(s.percentiles.p25 <= s.percentiles.p50);
        assert!(s.percentiles.p50 <= s.percentiles.p75);
        assert!(s.percentiles.p75 <= s.percentiles.p90);
    }

    #[test]
    fn test_percentile_single_element() {
        let v = vec![5.0_f64];
        assert_eq!(percentile(&v, 50), 5.0);
        assert_eq!(percentile(&v, 0),  5.0);
        assert_eq!(percentile(&v, 100),5.0);
    }

    #[test]
    fn test_percentile_empty() {
        let v: Vec<f64> = vec![];
        assert_eq!(percentile(&v, 50), 0.0);
    }

    // ── fee buckets ───────────────────────────────────────────────────────

    #[test]
    fn test_fee_buckets_count() {
        let mp = populated_mempool();
        let s  = MempoolStats::compute(&mp);
        let total: usize = s.fee_buckets.iter().map(|b| b.count).sum();
        assert_eq!(total, s.count);
    }

    #[test]
    fn test_fee_buckets_labels() {
        let mp = populated_mempool();
        let s  = MempoolStats::compute(&mp);
        let labels: Vec<&str> = s.fee_buckets.iter().map(|b| b.label).collect();
        assert_eq!(labels, vec!["0-1", "1-5", "5-10", "10-50", "50+"]);
    }

    #[test]
    fn test_fee_buckets_fees_sum() {
        let mp = populated_mempool();
        let s  = MempoolStats::compute(&mp);
        let bucket_total: u64 = s.fee_buckets.iter().map(|b| b.total_fees).sum();
        assert_eq!(bucket_total, s.total_fees);
    }

    // ── suggestions ───────────────────────────────────────────────────────

    #[test]
    fn test_suggested_fast_fee_min_1() {
        let mp = Mempool::new();
        let s  = MempoolStats::compute(&mp);
        assert_eq!(s.suggested_fast_fee(), 1.0);
    }

    #[test]
    fn test_suggested_economy_fee_min_1() {
        let mp = Mempool::new();
        let s  = MempoolStats::compute(&mp);
        assert_eq!(s.suggested_economy_fee(), 1.0);
    }

    #[test]
    fn test_suggested_fast_fee_populated() {
        let mp = populated_mempool();
        let s  = MempoolStats::compute(&mp);
        assert!(s.suggested_fast_fee() >= s.suggested_economy_fee());
    }
}
