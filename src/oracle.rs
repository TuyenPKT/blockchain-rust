#![allow(dead_code)]

/// v2.7 — Oracle: Off-chain Data Feed On-chain
///
/// Kiến trúc:
///
///   Off-chain (Oracle Nodes)         On-chain (Aggregator Contract)
///   ──────────────────────           ──────────────────────────────
///   Fetch real-world data            Collect reports from N nodes
///   Sign report                      Filter outliers (deviation check)
///   Submit report ──────────────────► Compute median / TWAP
///                                    Store latest answer
///                                    Emit NewRound event
///
///   Consumer contracts read: aggregator.latest_answer(feed_id)
///
/// Các loại aggregation:
///   Median:  loại bỏ outliers, robust với manipulation
///   TWAP:    time-weighted average price — kháng flash loan
///   Mean:    đơn giản nhất, dễ bị outlier ảnh hưởng
///
/// Trust model:
///   - Cần f+1 honest oracles trong tổng N (giống BFT)
///   - Staking + slashing để incentivize honest reporting
///   - Deviation threshold: loại bỏ report lệch quá xa median
///
/// Tham khảo: Chainlink Data Feeds, Band Protocol, Pyth Network

use sha2::{Sha256, Digest};
use std::collections::HashMap;

// ─── Price (fixed-point) ──────────────────────────────────────────────────────

/// Giá được lưu dưới dạng u64 với 8 chữ số thập phân
/// Ví dụ: BTC/USD = 65432.50000000 → stored as 6_543_250_000_000
pub const PRICE_DECIMALS: u64 = 100_000_000; // 1e8

pub fn price_from_f64(p: f64) -> u64 {
    (p * PRICE_DECIMALS as f64) as u64
}

pub fn price_to_f64(p: u64) -> f64 {
    p as f64 / PRICE_DECIMALS as f64
}

// ─── OracleReport ─────────────────────────────────────────────────────────────

/// 1 báo cáo giá từ 1 oracle node
#[derive(Debug, Clone)]
pub struct OracleReport {
    pub feed_id:    String,    // e.g. "BTC/USD"
    pub price:      u64,       // fixed-point price
    pub timestamp:  u64,       // unix timestamp (seconds)
    pub reporter:   String,    // oracle node address
    pub round:      u64,       // round number
    pub signature:  String,    // H(feed_id || price || timestamp || reporter)
}

impl OracleReport {
    pub fn new(
        feed_id: impl Into<String>,
        price: u64,
        timestamp: u64,
        reporter: impl Into<String>,
        round: u64,
    ) -> Self {
        let feed_id  = feed_id.into();
        let reporter = reporter.into();

        let mut h = Sha256::new();
        h.update(b"oracle_report_v27");
        h.update(feed_id.as_bytes());
        h.update(price.to_le_bytes());
        h.update(timestamp.to_le_bytes());
        h.update(reporter.as_bytes());
        h.update(round.to_le_bytes());
        let signature = hex::encode(h.finalize());

        OracleReport { feed_id, price, timestamp, reporter, round, signature }
    }

    pub fn verify(&self) -> bool {
        let expected = Self::new(
            self.feed_id.clone(),
            self.price,
            self.timestamp,
            self.reporter.clone(),
            self.round,
        );
        self.signature == expected.signature
    }
}

// ─── RoundData ────────────────────────────────────────────────────────────────

/// Kết quả 1 round sau aggregation
#[derive(Debug, Clone)]
pub struct RoundData {
    pub round:           u64,
    pub answer:          u64,       // median price
    pub answer_f64:      f64,
    pub timestamp:       u64,
    pub report_count:    usize,     // số reports hợp lệ
    pub deviation_pct:   f64,       // max deviation so với median (%)
    pub twap_answer:     u64,       // TWAP across recent rounds
}

// ─── OracleFeed ───────────────────────────────────────────────────────────────

/// 1 price feed (e.g. "BTC/USD") — aggregates reports từ nhiều oracles
pub struct OracleFeed {
    pub feed_id:           String,
    pub description:       String,
    pub decimals:          u8,
    pub min_submissions:   usize,    // cần ít nhất n reports để settle round
    pub deviation_threshold: f64,   // % — loại bỏ nếu lệch quá xa
    pub heartbeat:         u64,      // giây — staleness threshold

    pub current_round:     u64,
    pub round_reports:     Vec<OracleReport>,   // reports chờ trong round hiện tại
    pub history:           Vec<RoundData>,      // lịch sử các rounds đã settle
    pub authorized_nodes:  Vec<String>,
}

impl OracleFeed {
    pub fn new(
        feed_id: impl Into<String>,
        description: impl Into<String>,
        min_submissions: usize,
        deviation_threshold: f64,
        heartbeat: u64,
    ) -> Self {
        OracleFeed {
            feed_id:              feed_id.into(),
            description:          description.into(),
            decimals:             8,
            min_submissions,
            deviation_threshold,
            heartbeat,
            current_round:        1,
            round_reports:        vec![],
            history:              vec![],
            authorized_nodes:     vec![],
        }
    }

    pub fn authorize(&mut self, node: impl Into<String>) {
        self.authorized_nodes.push(node.into());
    }

    /// Oracle node submit 1 report
    pub fn submit(&mut self, report: OracleReport) -> Result<Option<RoundData>, String> {
        // Verify signature
        if !report.verify() {
            return Err("Invalid report signature".to_string());
        }
        // Check authorization
        if !self.authorized_nodes.contains(&report.reporter) {
            return Err(format!("Unauthorized reporter: {}", report.reporter));
        }
        // Check round
        if report.round != self.current_round {
            return Err(format!("Wrong round: expected {}, got {}", self.current_round, report.round));
        }
        // No duplicate from same reporter
        if self.round_reports.iter().any(|r| r.reporter == report.reporter) {
            return Err(format!("Duplicate report from {}", report.reporter));
        }

        self.round_reports.push(report);

        // Try to settle round if enough submissions
        if self.round_reports.len() >= self.min_submissions {
            Ok(Some(self.settle_round()))
        } else {
            Ok(None)
        }
    }

    /// Settle round: compute median, filter outliers, store result
    fn settle_round(&mut self) -> RoundData {
        let timestamp = self.round_reports.iter().map(|r| r.timestamp).max().unwrap_or(0);

        // Sort prices to compute median
        let mut prices: Vec<u64> = self.round_reports.iter().map(|r| r.price).collect();
        prices.sort();

        let median = if prices.is_empty() {
            0
        } else if prices.len() % 2 == 0 {
            (prices[prices.len()/2 - 1] + prices[prices.len()/2]) / 2
        } else {
            prices[prices.len()/2]
        };

        // Filter outliers: remove reports deviating > threshold from median
        let valid_reports: Vec<&OracleReport> = self.round_reports.iter()
            .filter(|r| {
                if median == 0 { return true; }
                let dev = if r.price > median { r.price - median } else { median - r.price };
                let pct = dev as f64 / median as f64 * 100.0;
                pct <= self.deviation_threshold
            })
            .collect();

        // Recompute median from valid reports only
        let mut valid_prices: Vec<u64> = valid_reports.iter().map(|r| r.price).collect();
        valid_prices.sort();
        let answer = if valid_prices.is_empty() { median } else {
            if valid_prices.len() % 2 == 0 {
                (valid_prices[valid_prices.len()/2 - 1] + valid_prices[valid_prices.len()/2]) / 2
            } else {
                valid_prices[valid_prices.len()/2]
            }
        };

        // Max deviation among valid reports
        let deviation_pct = valid_prices.iter().map(|&p| {
            let dev = if p > answer { p - answer } else { answer - p };
            dev as f64 / answer as f64 * 100.0
        }).fold(0.0f64, f64::max);

        // TWAP: average of last 3 rounds + current
        let recent: Vec<u64> = self.history.iter()
            .rev()
            .take(2)
            .map(|r| r.answer)
            .collect();
        let twap_sum: u64 = recent.iter().sum::<u64>() + answer;
        let twap_answer = twap_sum / (recent.len() as u64 + 1);

        let round_data = RoundData {
            round:        self.current_round,
            answer,
            answer_f64:   price_to_f64(answer),
            timestamp,
            report_count: valid_reports.len(),
            deviation_pct,
            twap_answer,
        };

        // Advance state
        self.history.push(round_data.clone());
        self.current_round += 1;
        self.round_reports.clear();

        round_data
    }

    /// Lấy giá mới nhất
    pub fn latest_answer(&self) -> Option<&RoundData> {
        self.history.last()
    }

    /// Kiểm tra staleness: giá có còn mới không?
    pub fn is_fresh(&self, current_time: u64) -> bool {
        match self.latest_answer() {
            Some(r) => current_time.saturating_sub(r.timestamp) <= self.heartbeat,
            None    => false,
        }
    }

    /// Price circuit breaker: giá mới có nhảy quá lớn so với round trước không?
    pub fn check_circuit_breaker(&self, new_price: u64, max_change_pct: f64) -> bool {
        match self.latest_answer() {
            None => true,
            Some(last) => {
                if last.answer == 0 { return true; }
                let delta = if new_price > last.answer { new_price - last.answer } else { last.answer - new_price };
                let pct = delta as f64 / last.answer as f64 * 100.0;
                pct <= max_change_pct
            }
        }
    }
}

// ─── OracleNode ───────────────────────────────────────────────────────────────

/// Off-chain oracle node — fetch + sign + submit price
pub struct OracleNode {
    pub address: String,
    pub stake:   u64,       // bond để disincentivize manipulation
    pub reports: u64,
    pub slashed: u64,
}

impl OracleNode {
    pub fn new(address: impl Into<String>, stake: u64) -> Self {
        OracleNode { address: address.into(), stake, reports: 0, slashed: 0 }
    }

    /// Tạo report (trong thực tế: fetch từ CEX/DEX API, aggregate, sign)
    pub fn report(
        &mut self,
        feed_id: &str,
        price: f64,
        timestamp: u64,
        round: u64,
    ) -> OracleReport {
        self.reports += 1;
        OracleReport::new(
            feed_id,
            price_from_f64(price),
            timestamp,
            self.address.clone(),
            round,
        )
    }

    /// Slash node nếu report gian lận
    pub fn slash(&mut self, amount: u64) {
        self.slashed  += amount.min(self.stake);
        self.stake     = self.stake.saturating_sub(amount);
    }
}

// ─── OracleRegistry ───────────────────────────────────────────────────────────

/// On-chain registry — quản lý nhiều feeds
pub struct OracleRegistry {
    pub feeds: HashMap<String, OracleFeed>,
}

impl OracleRegistry {
    pub fn new() -> Self {
        OracleRegistry { feeds: HashMap::new() }
    }

    pub fn add_feed(&mut self, feed: OracleFeed) {
        self.feeds.insert(feed.feed_id.clone(), feed);
    }

    pub fn submit(&mut self, report: OracleReport) -> Result<Option<RoundData>, String> {
        let feed = self.feeds.get_mut(&report.feed_id)
            .ok_or_else(|| format!("Feed not found: {}", report.feed_id))?;
        feed.submit(report)
    }

    pub fn latest_price(&self, feed_id: &str) -> Option<f64> {
        self.feeds.get(feed_id)
            .and_then(|f| f.latest_answer())
            .map(|r| r.answer_f64)
    }

    pub fn is_fresh(&self, feed_id: &str, current_time: u64) -> bool {
        self.feeds.get(feed_id).map_or(false, |f| f.is_fresh(current_time))
    }
}

// ─── DeFi Consumer ────────────────────────────────────────────────────────────

/// Ví dụ consumer contract dùng oracle price
/// Loan protocol: kiểm tra collateral ratio trước khi cho vay
pub struct LendingProtocol {
    pub oracle:         String,  // oracle feed để dùng
    pub min_collateral: f64,     // collateral ratio tối thiểu (e.g. 1.5 = 150%)
    pub loans:          Vec<Loan>,
}

#[derive(Debug, Clone)]
pub struct Loan {
    pub borrower:         String,
    pub collateral_btc:   f64,    // số BTC collateral
    pub borrowed_usd:     f64,    // số USD đã vay
    pub collateral_ratio: f64,    // current ratio
    pub liquidatable:     bool,
}

impl LendingProtocol {
    pub fn new(oracle: impl Into<String>, min_collateral: f64) -> Self {
        LendingProtocol { oracle: oracle.into(), min_collateral, loans: vec![] }
    }

    /// Tạo loan mới — kiểm tra collateral đủ không
    pub fn create_loan(
        &mut self,
        borrower: &str,
        collateral_btc: f64,
        borrow_usd: f64,
        btc_price: f64,
    ) -> Result<(), String> {
        let collateral_value = collateral_btc * btc_price;
        let ratio = collateral_value / borrow_usd;

        if ratio < self.min_collateral {
            return Err(format!(
                "Collateral ratio {:.2} < minimum {:.2}",
                ratio, self.min_collateral
            ));
        }

        self.loans.push(Loan {
            borrower:         borrower.to_string(),
            collateral_btc,
            borrowed_usd:     borrow_usd,
            collateral_ratio: ratio,
            liquidatable:     false,
        });
        Ok(())
    }

    /// Update tất cả loans với giá mới từ oracle
    pub fn update_prices(&mut self, btc_price: f64) {
        for loan in &mut self.loans {
            let collateral_value = loan.collateral_btc * btc_price;
            loan.collateral_ratio = collateral_value / loan.borrowed_usd;
            loan.liquidatable     = loan.collateral_ratio < self.min_collateral;
        }
    }

    pub fn liquidatable_loans(&self) -> Vec<&Loan> {
        self.loans.iter().filter(|l| l.liquidatable).collect()
    }
}
