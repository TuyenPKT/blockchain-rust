#![allow(dead_code)]
//! v8.6 — Chain Analytics
//!
//! Computes time-series data points from the chain for PKTScan charts.
//!
//! Supported metrics (via `/api/analytics/:metric?window=N`):
//!   block_time    → seconds between consecutive blocks
//!   hashrate      → estimated network hashrate per block
//!   fee_market    → total fees + avg fee per block
//!   difficulty    → difficulty value per block (constant in demo chain)
//!   tx_throughput → transaction count per block
//!
//! API:
//!   analytics(metric, chain, difficulty, window) → Option<AnalyticsSeries>
//!   Metric enum variants + DataPoint struct

use crate::block::Block;
use crate::pktscan_api::{avg_block_time_secs, estimate_hashrate};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DataPoint {
    pub height:    u64,
    pub timestamp: i64,
    pub value:     f64,
    /// Optional secondary value (e.g. avg_fee alongside total_fees)
    pub value2:    Option<f64>,
}

#[derive(Debug, Clone)]
pub struct AnalyticsSeries {
    pub metric:  String,
    pub label:   String,
    pub unit:    String,
    pub points:  Vec<DataPoint>,
    pub window:  usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Metric {
    BlockTime,
    Hashrate,
    FeeMarket,
    Difficulty,
    TxThroughput,
}

impl Metric {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "block_time"    => Some(Metric::BlockTime),
            "hashrate"      => Some(Metric::Hashrate),
            "fee_market"    => Some(Metric::FeeMarket),
            "difficulty"    => Some(Metric::Difficulty),
            "tx_throughput" => Some(Metric::TxThroughput),
            _               => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Metric::BlockTime    => "block_time",
            Metric::Hashrate     => "hashrate",
            Metric::FeeMarket    => "fee_market",
            Metric::Difficulty   => "difficulty",
            Metric::TxThroughput => "tx_throughput",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Metric::BlockTime    => "Block Time",
            Metric::Hashrate     => "Network Hashrate",
            Metric::FeeMarket    => "Fee Market",
            Metric::Difficulty   => "Mining Difficulty",
            Metric::TxThroughput => "Transactions per Block",
        }
    }

    pub fn unit(&self) -> &'static str {
        match self {
            Metric::BlockTime    => "seconds",
            Metric::Hashrate     => "H/s",
            Metric::FeeMarket    => "sat",
            Metric::Difficulty   => "",
            Metric::TxThroughput => "txs",
        }
    }
}

// ─── Series builders ──────────────────────────────────────────────────────────

/// Block time in seconds between consecutive blocks.
pub fn block_time_series(chain: &[Block], window: usize) -> AnalyticsSeries {
    let tail = tail_blocks(chain, window + 1);
    let points: Vec<DataPoint> = tail.windows(2)
        .map(|w| {
            let elapsed = (w[1].timestamp - w[0].timestamp).max(0) as f64;
            DataPoint {
                height:    w[1].index,
                timestamp: w[1].timestamp,
                value:     elapsed,
                value2:    None,
            }
        })
        .collect();

    AnalyticsSeries {
        metric:  Metric::BlockTime.name().into(),
        label:   Metric::BlockTime.label().into(),
        unit:    Metric::BlockTime.unit().into(),
        points,
        window,
    }
}

/// Estimated network hashrate (H/s) computed at each block using a 5-block rolling window.
pub fn hashrate_series(chain: &[Block], difficulty: usize, window: usize) -> AnalyticsSeries {
    let tail = tail_blocks(chain, window);
    let roll  = 5_usize;

    let points: Vec<DataPoint> = tail.iter().enumerate().map(|(i, block)| {
        let start = i.saturating_sub(roll - 1);
        let slice = &tail[start..=i];
        let avg   = avg_block_time_secs(slice);
        let hr    = estimate_hashrate(difficulty, avg);
        DataPoint {
            height:    block.index,
            timestamp: block.timestamp,
            value:     hr as f64,
            value2:    None,
        }
    }).collect();

    AnalyticsSeries {
        metric:  Metric::Hashrate.name().into(),
        label:   Metric::Hashrate.label().into(),
        unit:    Metric::Hashrate.unit().into(),
        points,
        window,
    }
}

/// Total fees and avg fee per block.
pub fn fee_market_series(chain: &[Block], window: usize) -> AnalyticsSeries {
    let tail = tail_blocks(chain, window);
    let points: Vec<DataPoint> = tail.iter().map(|block| {
        let total_fees: u64 = block.transactions.iter().map(|tx| tx.fee).sum();
        let tx_count         = block.transactions.len().max(1);
        let avg_fee          = total_fees as f64 / tx_count as f64;
        DataPoint {
            height:    block.index,
            timestamp: block.timestamp,
            value:     total_fees as f64,
            value2:    Some(avg_fee),
        }
    }).collect();

    AnalyticsSeries {
        metric:  Metric::FeeMarket.name().into(),
        label:   Metric::FeeMarket.label().into(),
        unit:    Metric::FeeMarket.unit().into(),
        points,
        window,
    }
}

/// Difficulty value at each block (constant in demo chain).
pub fn difficulty_series(chain: &[Block], difficulty: usize, window: usize) -> AnalyticsSeries {
    let tail = tail_blocks(chain, window);
    let points: Vec<DataPoint> = tail.iter().map(|block| {
        DataPoint {
            height:    block.index,
            timestamp: block.timestamp,
            value:     difficulty as f64,
            value2:    None,
        }
    }).collect();

    AnalyticsSeries {
        metric:  Metric::Difficulty.name().into(),
        label:   Metric::Difficulty.label().into(),
        unit:    Metric::Difficulty.unit().into(),
        points,
        window,
    }
}

/// Transaction count per block.
pub fn tx_throughput_series(chain: &[Block], window: usize) -> AnalyticsSeries {
    let tail = tail_blocks(chain, window);
    let points: Vec<DataPoint> = tail.iter().map(|block| {
        DataPoint {
            height:    block.index,
            timestamp: block.timestamp,
            value:     block.transactions.len() as f64,
            value2:    None,
        }
    }).collect();

    AnalyticsSeries {
        metric:  Metric::TxThroughput.name().into(),
        label:   Metric::TxThroughput.label().into(),
        unit:    Metric::TxThroughput.unit().into(),
        points,
        window,
    }
}

/// Dispatch to the right series builder.
pub fn analytics(
    metric:     &str,
    chain:      &[Block],
    difficulty: usize,
    window:     usize,
) -> Option<AnalyticsSeries> {
    let window = window.clamp(2, 500);
    match Metric::from_str(metric)? {
        Metric::BlockTime    => Some(block_time_series(chain, window)),
        Metric::Hashrate     => Some(hashrate_series(chain, difficulty, window)),
        Metric::FeeMarket    => Some(fee_market_series(chain, window)),
        Metric::Difficulty   => Some(difficulty_series(chain, difficulty, window)),
        Metric::TxThroughput => Some(tx_throughput_series(chain, window)),
    }
}

// ─── Helper ───────────────────────────────────────────────────────────────────

fn tail_blocks(chain: &[Block], n: usize) -> &[Block] {
    let len = chain.len();
    if n >= len { chain } else { &chain[len - n..] }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Block;
    use crate::chain::Blockchain;
    use crate::transaction::Transaction;

    const ADDR: &str = "aabbccdd00112233445566778899aabbccddeeff";

    fn make_chain(n: u64) -> Blockchain {
        let mut bc = Blockchain::new();
        for i in 1..=n {
            let cb = Transaction::coinbase_at(ADDR, 0, i);
            let mut blk = Block::new(i, vec![cb], bc.chain.last().unwrap().hash.clone());
            blk.mine(2);
            bc.chain.push(blk);
        }
        bc
    }

    // ── Metric::from_str ──────────────────────────────────────────────────

    #[test]
    fn test_metric_from_str_all() {
        assert_eq!(Metric::from_str("block_time"),    Some(Metric::BlockTime));
        assert_eq!(Metric::from_str("hashrate"),      Some(Metric::Hashrate));
        assert_eq!(Metric::from_str("fee_market"),    Some(Metric::FeeMarket));
        assert_eq!(Metric::from_str("difficulty"),    Some(Metric::Difficulty));
        assert_eq!(Metric::from_str("tx_throughput"), Some(Metric::TxThroughput));
    }

    #[test]
    fn test_metric_from_str_invalid() {
        assert_eq!(Metric::from_str("unknown"), None);
        assert_eq!(Metric::from_str(""),        None);
    }

    // ── block_time_series ─────────────────────────────────────────────────

    #[test]
    fn test_block_time_series_len() {
        let bc = make_chain(5);
        let s = block_time_series(&bc.chain, 50);
        // window+1 blocks → window pairs (but capped to chain len - 1)
        assert!(s.points.len() >= 1);
    }

    #[test]
    fn test_block_time_series_non_negative() {
        let bc = make_chain(5);
        let s = block_time_series(&bc.chain, 50);
        for p in &s.points { assert!(p.value >= 0.0); }
    }

    #[test]
    fn test_block_time_series_metric_name() {
        let bc = make_chain(3);
        let s = block_time_series(&bc.chain, 10);
        assert_eq!(s.metric, "block_time");
    }

    // ── hashrate_series ───────────────────────────────────────────────────

    #[test]
    fn test_hashrate_series_len() {
        let bc = make_chain(5);
        let s = hashrate_series(&bc.chain, 2, 10);
        assert!(s.points.len() >= 1);
    }

    #[test]
    fn test_hashrate_series_non_negative() {
        let bc = make_chain(5);
        let s = hashrate_series(&bc.chain, 2, 10);
        for p in &s.points { assert!(p.value >= 0.0); }
    }

    // ── fee_market_series ─────────────────────────────────────────────────

    #[test]
    fn test_fee_market_series_len() {
        let bc = make_chain(4);
        let s = fee_market_series(&bc.chain, 10);
        assert_eq!(s.points.len(), bc.chain.len());
    }

    #[test]
    fn test_fee_market_series_value2_present() {
        let bc = make_chain(3);
        let s = fee_market_series(&bc.chain, 10);
        for p in &s.points { assert!(p.value2.is_some()); }
    }

    #[test]
    fn test_fee_market_coinbase_zero_fee() {
        let bc = make_chain(3);
        let s = fee_market_series(&bc.chain, 10);
        // coinbase txs have fee=0
        for p in &s.points { assert_eq!(p.value, 0.0); }
    }

    // ── difficulty_series ─────────────────────────────────────────────────

    #[test]
    fn test_difficulty_series_constant() {
        let bc = make_chain(4);
        let diff = 3;
        let s = difficulty_series(&bc.chain, diff, 10);
        for p in &s.points { assert_eq!(p.value, diff as f64); }
    }

    // ── tx_throughput_series ──────────────────────────────────────────────

    #[test]
    fn test_tx_throughput_at_least_one() {
        let bc = make_chain(3);
        let s = tx_throughput_series(&bc.chain, 10);
        // skip genesis (height=0, may have 0 txs); blocks 1+ each have 1 coinbase
        for p in s.points.iter().filter(|p| p.height > 0) {
            assert!(p.value >= 1.0);
        }
    }

    // ── analytics dispatch ────────────────────────────────────────────────

    #[test]
    fn test_analytics_valid_metrics() {
        let bc = make_chain(5);
        for m in &["block_time", "hashrate", "fee_market", "difficulty", "tx_throughput"] {
            let r = analytics(m, &bc.chain, 2, 10);
            assert!(r.is_some(), "metric {} should return Some", m);
        }
    }

    #[test]
    fn test_analytics_invalid_metric() {
        let bc = make_chain(3);
        assert!(analytics("unknown", &bc.chain, 2, 10).is_none());
    }

    #[test]
    fn test_analytics_window_clamped_min() {
        let bc = make_chain(5);
        // window=0 should be clamped to 2
        let s = analytics("block_time", &bc.chain, 2, 0).unwrap();
        assert_eq!(s.window, 2);
    }

    #[test]
    fn test_analytics_window_clamped_max() {
        let bc = make_chain(5);
        let s = analytics("difficulty", &bc.chain, 2, 9999).unwrap();
        assert_eq!(s.window, 500);
    }

    // ── tail_blocks ───────────────────────────────────────────────────────

    #[test]
    fn test_tail_blocks_less_than_chain() {
        let bc = make_chain(10);
        let t = tail_blocks(&bc.chain, 3);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].index, bc.chain[bc.chain.len() - 3].index);
    }

    #[test]
    fn test_tail_blocks_more_than_chain() {
        let bc = make_chain(3);
        let t = tail_blocks(&bc.chain, 100);
        assert_eq!(t.len(), bc.chain.len());
    }
}
