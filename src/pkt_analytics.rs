#![allow(dead_code)]
//! v18.0 — PKT Testnet Analytics
//!
//! Tổng hợp time-series từ SyncDb (block headers) cho charts:
//!   hashrate    → estimated network hashrate (H/s) mỗi block
//!   block_time  → seconds giữa 2 block liên tiếp
//!   difficulty  → compact difficulty mỗi block
//!
//! API: GET /api/testnet/analytics?metric=hashrate|block_time|difficulty&window=N
//!
//! Giới hạn: window tối đa 1000 blocks.

use crate::pkt_sync::{SyncDb, SyncError};
use crate::pkt_wire::WireBlockHeader;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalyticsPoint {
    pub height:    u64,
    pub timestamp: u32,
    pub value:     f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalyticsSeries {
    pub metric: String,
    pub unit:   String,
    pub window: usize,
    pub points: Vec<AnalyticsPoint>,
}

// ── Core helpers ──────────────────────────────────────────────────────────────

/// Tải `window` headers gần nhất từ SyncDb, trả về Vec<(height, header)>.
pub fn load_recent_headers(
    db: &SyncDb,
    window: usize,
) -> Result<Vec<(u64, WireBlockHeader)>, SyncError> {
    let tip = match db.get_sync_height()? {
        Some(h) => h,
        None    => return Ok(vec![]),
    };
    let from = tip.saturating_sub(window as u64);
    let mut out = Vec::with_capacity(window + 1);
    for h in from..=tip {
        if let Some(raw) = db.load_header(h)? {
            if let Ok(hdr) = WireBlockHeader::from_bytes(&raw) {
                out.push((h, hdr));
            }
        }
    }
    Ok(out)
}

/// Chuyển `bits` (compact target) → difficulty (float).
/// difficulty = genesis_target / current_target
/// Genesis bits PKT testnet = 0x207fffff (easiest target)
pub fn bits_to_difficulty(bits: u32) -> f64 {
    // Decode compact target thành 256-bit big-endian float
    let exponent = (bits >> 24) as usize;
    let mantissa = (bits & 0x007f_ffff) as f64; // bỏ bit sign
    if exponent == 0 || mantissa == 0.0 {
        return 1.0;
    }
    // current_target ≈ mantissa * 256^(exponent-3)
    // genesis_target: bits=0x207fffff → exponent=32, mantissa=0x7fffff
    let genesis_mantissa = 0x7f_ffff_u32 as f64;
    let genesis_exp = 32usize;
    // ratio = genesis / current (same base, compare exponents)
    let exp_diff = genesis_exp as i64 - exponent as i64;
    let ratio = (genesis_mantissa / mantissa) * (256f64).powi(exp_diff as i32);
    ratio.max(1.0)
}

/// Ước tính hashrate (H/s) từ difficulty và block time.
/// hashrate = difficulty * 2^32 / block_time_secs
pub fn estimate_hashrate_from(difficulty: f64, block_time_secs: f64) -> f64 {
    if block_time_secs <= 0.0 {
        return 0.0;
    }
    difficulty * 4_294_967_296.0 / block_time_secs
}

// ── Series builders ───────────────────────────────────────────────────────────

/// Tính block_time series: seconds giữa 2 block liên tiếp.
pub fn block_time_series(
    db: &SyncDb,
    window: usize,
) -> Result<AnalyticsSeries, SyncError> {
    // Cần thêm 1 header để tính diff
    let headers = load_recent_headers(db, window + 1)?;
    let mut points = Vec::with_capacity(window);
    for i in 1..headers.len() {
        let (h, ref cur) = headers[i];
        let (_, ref prev) = headers[i - 1];
        let dt = (cur.timestamp as i64 - prev.timestamp as i64).max(0) as f64;
        points.push(AnalyticsPoint { height: h, timestamp: cur.timestamp, value: dt });
    }
    Ok(AnalyticsSeries {
        metric: "block_time".into(),
        unit:   "seconds".into(),
        window: points.len(),
        points,
    })
}

/// Tính difficulty series từ bits.
pub fn difficulty_series(
    db: &SyncDb,
    window: usize,
) -> Result<AnalyticsSeries, SyncError> {
    let headers = load_recent_headers(db, window)?;
    let points = headers.iter().map(|(h, hdr)| AnalyticsPoint {
        height:    *h,
        timestamp: hdr.timestamp,
        value:     bits_to_difficulty(hdr.bits),
    }).collect();
    Ok(AnalyticsSeries {
        metric: "difficulty".into(),
        unit:   "ratio".into(),
        window,
        points,
    })
}

/// Tính hashrate series (H/s).
pub fn hashrate_series(
    db: &SyncDb,
    window: usize,
) -> Result<AnalyticsSeries, SyncError> {
    let headers = load_recent_headers(db, window + 1)?;
    let mut points = Vec::with_capacity(window);
    for i in 1..headers.len() {
        let (h, ref cur) = headers[i];
        let (_, ref prev) = headers[i - 1];
        let dt = (cur.timestamp as i64 - prev.timestamp as i64).max(1) as f64;
        let diff = bits_to_difficulty(cur.bits);
        let hr = estimate_hashrate_from(diff, dt);
        points.push(AnalyticsPoint { height: h, timestamp: cur.timestamp, value: hr });
    }
    Ok(AnalyticsSeries {
        metric: "hashrate".into(),
        unit:   "H/s".into(),
        window: points.len(),
        points,
    })
}

/// Dispatch: chọn series theo metric string.
pub fn analytics(
    metric: &str,
    db: &SyncDb,
    window: usize,
) -> Result<AnalyticsSeries, SyncError> {
    let w = window.clamp(2, 1000);
    match metric {
        "hashrate"   => hashrate_series(db, w),
        "block_time" => block_time_series(db, w),
        "difficulty" => difficulty_series(db, w),
        _            => Err(SyncError::InvalidChain(
            format!("unknown metric: {}", metric)
        )),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── bits_to_difficulty ─────────────────────────────────────────────────

    #[test]
    fn test_bits_to_difficulty_genesis() {
        // genesis bits = 0x207fffff → difficulty = 1.0
        let d = bits_to_difficulty(0x207f_ffff);
        assert!((d - 1.0).abs() < 0.01, "genesis diff={}", d);
    }

    #[test]
    fn test_bits_to_difficulty_harder() {
        // smaller target (larger exponent diff) → higher difficulty
        let easy = bits_to_difficulty(0x207f_ffff);
        let hard = bits_to_difficulty(0x1d00_ffff); // Bitcoin genesis
        assert!(hard > easy, "harder target must have higher difficulty");
    }

    #[test]
    fn test_bits_to_difficulty_zero_mantissa() {
        assert_eq!(bits_to_difficulty(0), 1.0);
    }

    #[test]
    fn test_bits_to_difficulty_min_one() {
        let d = bits_to_difficulty(0x207f_ffff);
        assert!(d >= 1.0);
    }

    // ── estimate_hashrate_from ─────────────────────────────────────────────

    #[test]
    fn test_hashrate_positive() {
        let hr = estimate_hashrate_from(1.0, 60.0);
        assert!(hr > 0.0);
    }

    #[test]
    fn test_hashrate_zero_block_time() {
        assert_eq!(estimate_hashrate_from(1.0, 0.0), 0.0);
    }

    #[test]
    fn test_hashrate_negative_block_time() {
        assert_eq!(estimate_hashrate_from(1.0, -1.0), 0.0);
    }

    #[test]
    fn test_hashrate_scales_with_difficulty() {
        let hr1 = estimate_hashrate_from(1.0, 60.0);
        let hr2 = estimate_hashrate_from(2.0, 60.0);
        assert!((hr2 - hr1 * 2.0).abs() < 1.0);
    }

    #[test]
    fn test_hashrate_scales_with_block_time() {
        let hr1 = estimate_hashrate_from(1.0, 60.0);
        let hr2 = estimate_hashrate_from(1.0, 120.0);
        assert!((hr1 - hr2 * 2.0).abs() < 1.0);
    }

    // ── load_recent_headers + series (DB) ─────────────────────────────────

    use crate::pkt_wire::WireBlockHeader;
    use std::sync::Mutex;

    static DB_LOCK: Mutex<()> = Mutex::new(());

    fn make_header(ts: u32, bits: u32) -> [u8; crate::pkt_wire::WIRE_HEADER_LEN] {
        WireBlockHeader {
            version:     1,
            prev_block:  [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp:   ts,
            bits,
            nonce:       0,
        }.to_bytes()
    }

    #[test]
    fn test_load_recent_headers_empty() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        let headers = load_recent_headers(&db, 10).unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn test_load_recent_headers_window() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=20u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let headers = load_recent_headers(&db, 5).unwrap();
        assert_eq!(headers.len(), 6); // height 15..=20 (6 headers, window+1 for diffs)
        // Wait, load_recent_headers with window=5 loads from tip-5..=tip = 15..=20 = 6 items
        assert!(headers.last().unwrap().0 == 20);
    }

    #[test]
    fn test_block_time_series_len() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=10u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32 * 60, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let s = block_time_series(&db, 5).unwrap();
        assert_eq!(s.metric, "block_time");
        assert_eq!(s.unit, "seconds");
        assert!(!s.points.is_empty());
    }

    #[test]
    fn test_block_time_values() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=5u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32 * 60, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let s = block_time_series(&db, 4).unwrap();
        for p in &s.points {
            assert!((p.value - 60.0).abs() < 1.0, "block_time should be 60s, got {}", p.value);
        }
    }

    #[test]
    fn test_difficulty_series_len() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=8u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let s = difficulty_series(&db, 5).unwrap();
        assert_eq!(s.metric, "difficulty");
        assert!(!s.points.is_empty());
    }

    #[test]
    fn test_difficulty_genesis_bits() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        db.save_header(1, &make_header(1_600_000_000, 0x207fffff)).unwrap();
        db.set_sync_height(1).unwrap();
        let s = difficulty_series(&db, 1).unwrap();
        assert_eq!(s.points.len(), 1);
        assert!((s.points[0].value - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_hashrate_series_len() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=10u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32 * 60, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let s = hashrate_series(&db, 5).unwrap();
        assert_eq!(s.metric, "hashrate");
        assert_eq!(s.unit, "H/s");
        assert!(!s.points.is_empty());
    }

    #[test]
    fn test_hashrate_positive_values() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=5u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32 * 60, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let s = hashrate_series(&db, 4).unwrap();
        for p in &s.points {
            assert!(p.value > 0.0, "hashrate must be positive");
        }
    }

    // ── analytics dispatch ─────────────────────────────────────────────────

    #[test]
    fn test_analytics_hashrate() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=5u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32 * 60, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let s = analytics("hashrate", &db, 4).unwrap();
        assert_eq!(s.metric, "hashrate");
    }

    #[test]
    fn test_analytics_block_time() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=5u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32 * 60, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let s = analytics("block_time", &db, 4).unwrap();
        assert_eq!(s.metric, "block_time");
    }

    #[test]
    fn test_analytics_difficulty() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=5u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let s = analytics("difficulty", &db, 4).unwrap();
        assert_eq!(s.metric, "difficulty");
    }

    #[test]
    fn test_analytics_unknown_metric() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        db.set_sync_height(1).unwrap();
        assert!(analytics("unknown_metric", &db, 10).is_err());
    }

    #[test]
    fn test_analytics_window_clamped() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=5u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32 * 60, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        // window=0 should be clamped to 2, không panic
        let s = analytics("hashrate", &db, 0).unwrap();
        assert_eq!(s.metric, "hashrate");
    }

    #[test]
    fn test_analytics_window_max_clamped() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=5u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32 * 60, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        // window=99999 clamped to 1000, không panic
        let s = analytics("block_time", &db, 99999).unwrap();
        assert_eq!(s.metric, "block_time");
    }

    #[test]
    fn test_heights_ascending() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=10u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32 * 60, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let s = hashrate_series(&db, 8).unwrap();
        let heights: Vec<u64> = s.points.iter().map(|p| p.height).collect();
        let mut sorted = heights.clone();
        sorted.sort();
        assert_eq!(heights, sorted, "heights phải tăng dần");
    }

    #[test]
    fn test_analytics_point_has_timestamp() {
        let _g = DB_LOCK.lock().unwrap();
        let db = SyncDb::open_temp().unwrap();
        for h in 1..=5u64 {
            db.save_header(h, &make_header(1_600_000_000 + h as u32 * 60, 0x207fffff)).unwrap();
            db.set_sync_height(h).unwrap();
        }
        let s = block_time_series(&db, 4).unwrap();
        for p in &s.points {
            assert!(p.timestamp > 0, "timestamp phải > 0");
        }
    }
}
