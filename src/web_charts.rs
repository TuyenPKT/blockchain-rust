#![allow(dead_code)]
//! v14.5 — Web Charts
//!
//! ASCII sparkline trong terminal + Chart.js line charts trên web.
//! Dữ liệu từ `GET /api/analytics/:metric` (chain_analytics.rs).
//!
//! CLI:  `cargo run -- charts [hashrate|block_time|tx_volume|mempool]`
//! Web:  `GET /static/charts.js` → inject vào trang, tự fetch analytics API

use axum::{http::header, response::IntoResponse, routing::get, Router};

/// Nhúng charts.js compile-time
pub static CHARTS_JS: &[u8] = include_bytes!("../frontend/charts.js");

// ── Unicode sparkline ─────────────────────────────────────────────────────────

/// 8 block elements cho sparkline ▁▂▃▄▅▆▇█
const SPARKS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Xu hướng của chuỗi dữ liệu
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    Up,
    Down,
    Flat,
}

impl Trend {
    pub fn symbol(self) -> &'static str {
        match self {
            Trend::Up   => "↑",
            Trend::Down => "↓",
            Trend::Flat => "→",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Trend::Up   => "tăng",
            Trend::Down => "giảm",
            Trend::Flat => "ổn định",
        }
    }
}

/// Kết quả render sparkline
#[derive(Debug, Clone)]
pub struct SparklineResult {
    /// Chuỗi Unicode ▁▂▃▄▅▆▇█
    pub line:   String,
    pub min:    f64,
    pub max:    f64,
    pub avg:    f64,
    pub latest: f64,
    pub trend:  Trend,
}

// ── Sparkline engine ──────────────────────────────────────────────────────────

/// Render sparkline từ slice f64.
///
/// - `data`:  dãy giá trị (bất kỳ độ dài)
/// - `width`: số ký tự Unicode muốn xuất ra
///
/// Trả `None` nếu `data` rỗng hoặc `width == 0`.
pub fn sparkline(data: &[f64], width: usize) -> Option<SparklineResult> {
    if data.is_empty() || width == 0 {
        return None;
    }

    let min    = data.iter().cloned().fold(f64::INFINITY,     f64::min);
    let max    = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let avg    = data.iter().sum::<f64>() / data.len() as f64;
    let latest = *data.last().unwrap();

    let sampled = resample(data, width);
    let range   = max - min;

    let line: String = sampled.iter().map(|&v| {
        if range < 1e-10 {
            SPARKS[3]
        } else {
            let idx = ((v - min) / range * 7.0).round() as usize;
            SPARKS[idx.min(7)]
        }
    }).collect();

    let trend = detect_trend(&sampled);
    Some(SparklineResult { line, min, max, avg, latest, trend })
}

/// Average-pool hoặc interpolate data về đúng `n` điểm.
fn resample(data: &[f64], n: usize) -> Vec<f64> {
    let len = data.len();
    if len == n {
        return data.to_vec();
    }
    (0..n)
        .map(|i| {
            let start = i * len / n;
            let end   = ((i + 1) * len / n).max(start + 1).min(len);
            let slice = &data[start..end];
            slice.iter().sum::<f64>() / slice.len() as f64
        })
        .collect()
}

/// So sánh avg của 25% đầu vs 25% cuối để xác định xu hướng.
fn detect_trend(data: &[f64]) -> Trend {
    let n = data.len();
    if n < 4 {
        return Trend::Flat;
    }
    let q = (n / 4).max(1);
    let first = data[..q].iter().sum::<f64>() / q as f64;
    let last  = data[n - q..].iter().sum::<f64>() / q as f64;

    if first.abs() < 1e-10 {
        return Trend::Flat;
    }
    let pct = (last - first) / first.abs();
    if pct > 0.05      { Trend::Up }
    else if pct < -0.05 { Trend::Down }
    else                { Trend::Flat }
}

// ── MetricChart ───────────────────────────────────────────────────────────────

/// Chart cho một metric với sparkline đã render
#[derive(Debug)]
pub struct MetricChart {
    pub metric:   &'static str,
    pub title:    &'static str,
    pub unit:     &'static str,
    pub spark:    SparklineResult,
    pub data_len: usize,
}

impl MetricChart {
    /// Trả `None` nếu `data` rỗng.
    pub fn build(
        metric: &'static str,
        title:  &'static str,
        unit:   &'static str,
        data:   &[f64],
    ) -> Option<Self> {
        let w     = data.len().max(1).min(60);
        let spark = sparkline(data, w)?;
        Some(MetricChart { metric, title, unit, spark, data_len: data.len() })
    }

    /// In một dòng ra terminal.
    pub fn print(&self) {
        let s = &self.spark;
        println!(
            "  {:12} │{}│  latest={:.1}{} avg={:.1} min={:.1} max={:.1}  {} ({} pts)",
            self.title, s.line,
            s.latest, self.unit, s.avg, s.min, s.max,
            s.trend.symbol(), self.data_len,
        );
    }
}

// ── ChartDashboard ────────────────────────────────────────────────────────────

/// Dashboard nhiều MetricChart in theo thứ tự
#[derive(Default)]
pub struct ChartDashboard {
    pub charts: Vec<MetricChart>,
}

impl ChartDashboard {
    pub fn new() -> Self { Self::default() }

    pub fn add(&mut self, chart: MetricChart) {
        self.charts.push(chart);
    }

    pub fn print(&self) {
        println!();
        println!("  ── PKT Chain Sparklines ─────────────────────────────────────────────────────");
        if self.charts.is_empty() {
            println!("  (Chưa có data — mine vài block trước: cargo run -- mine)");
        } else {
            for c in &self.charts {
                c.print();
            }
        }
        println!();
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Data giả lập (sin wave + harmonic) cho CLI demo / tests
pub fn mock_data(n: usize, base: f64, amplitude: f64, phase: f64) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let t = i as f64 / n.max(1) as f64;
            base
                + amplitude        * (t * 6.283 + phase).sin()
                + amplitude * 0.25 * (t * 31.4  + phase).sin()
        })
        .collect()
}

/// Format hashrate: kH/s → MH/s → GH/s
pub fn format_hashrate(khs: f64) -> String {
    if khs >= 1_000_000.0 {
        format!("{:.2} GH/s", khs / 1_000_000.0)
    } else if khs >= 1_000.0 {
        format!("{:.2} MH/s", khs / 1_000.0)
    } else {
        format!("{:.1} kH/s", khs)
    }
}

// ── Axum router ───────────────────────────────────────────────────────────────

/// Phục vụ charts.js nhúng qua compile-time
async fn charts_js_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        CHARTS_JS,
    )
}

/// Router chỉ mount `/static/charts.js`.
/// Merge vào `pktscan_api::serve()` giống `static_router()`.
pub fn charts_router() -> Router {
    Router::new().route("/static/charts.js", get(charts_js_handler))
}

// ── CLI ───────────────────────────────────────────────────────────────────────

/// `cargo run -- charts [metric]`
///
/// Hiển thị ASCII sparkline cho metric (hoặc tất cả nếu không chỉ định).
pub fn cmd_charts(args: &[String]) {
    let metric = args.first().map(|s| s.as_str()).unwrap_or("all");

    // Bảng cấu hình: (id, title, unit, base, amplitude, phase)
    let configs: &[(&str, &str, &str, f64, f64, f64)] = &[
        ("hashrate",   "Hashrate",   " kH/s",    500.0, 160.0, 0.00),
        ("block_time", "Block Time", " s",         30.0,  10.0, 1.05),
        ("tx_volume",  "TX Volume",  " tx/blk",    12.0,   6.0, 2.10),
        ("mempool",    "Mempool",    " tx",         80.0,  35.0, 3.15),
    ];

    let valid_ids: Vec<&str> = configs.iter().map(|c| c.0).collect();

    if metric != "all" && !valid_ids.contains(&metric) {
        eprintln!("Metric không hợp lệ: '{}'", metric);
        eprintln!("Hợp lệ: all | {}", valid_ids.join(" | "));
        return;
    }

    let mut dash = ChartDashboard::new();
    for &(id, title, unit, base, amp, phase) in configs {
        if metric != "all" && metric != id {
            continue;
        }
        let data = mock_data(80, base, amp, phase);
        if let Some(chart) = MetricChart::build(id, title, unit, &data) {
            dash.add(chart);
        }
    }

    println!();
    println!("  PKT Chain — Sparkline Charts");
    println!("  (Web charts: http://localhost:3000/static/charts.js)");
    dash.print();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── sparkline ─────────────────────────────────────────────────────────

    #[test]
    fn test_sparkline_empty_none() {
        assert!(sparkline(&[], 20).is_none());
    }

    #[test]
    fn test_sparkline_zero_width_none() {
        assert!(sparkline(&[1.0, 2.0], 0).is_none());
    }

    #[test]
    fn test_sparkline_single_point() {
        let r = sparkline(&[7.0], 1).unwrap();
        assert_eq!(r.line.chars().count(), 1);
        assert_eq!(r.min, 7.0);
        assert_eq!(r.max, 7.0);
        assert_eq!(r.latest, 7.0);
    }

    #[test]
    fn test_sparkline_correct_width() {
        let data: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let r = sparkline(&data, 40).unwrap();
        assert_eq!(r.line.chars().count(), 40);
    }

    #[test]
    fn test_sparkline_width_larger_than_data() {
        let r = sparkline(&[1.0, 2.0, 3.0], 20).unwrap();
        assert_eq!(r.line.chars().count(), 20);
    }

    #[test]
    fn test_sparkline_flat_uses_middle_block() {
        let data = vec![42.0; 10];
        let r = sparkline(&data, 10).unwrap();
        assert!(r.line.chars().all(|c| c == '▄'));
    }

    #[test]
    fn test_sparkline_ascending_last_is_max() {
        let data: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let r = sparkline(&data, 10).unwrap();
        assert_eq!(r.line.chars().last().unwrap(), '█');
    }

    #[test]
    fn test_sparkline_descending_first_is_max() {
        let data: Vec<f64> = (0..10).map(|i| (9 - i) as f64).collect();
        let r = sparkline(&data, 10).unwrap();
        assert_eq!(r.line.chars().next().unwrap(), '█');
    }

    #[test]
    fn test_sparkline_only_block_chars() {
        let data: Vec<f64> = (0..50).map(|i| (i as f64 * 0.3).sin() * 100.0).collect();
        let r = sparkline(&data, 30).unwrap();
        for c in r.line.chars() {
            assert!(SPARKS.contains(&c), "ký tự lạ: {}", c);
        }
    }

    #[test]
    fn test_sparkline_min_max() {
        let r = sparkline(&[3.0, 1.0, 5.0, 2.0, 4.0], 5).unwrap();
        assert!((r.min - 1.0).abs() < 1e-9);
        assert!((r.max - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_sparkline_avg() {
        let r = sparkline(&[1.0, 2.0, 3.0, 4.0, 5.0], 5).unwrap();
        assert!((r.avg - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_sparkline_latest() {
        let r = sparkline(&[1.0, 5.0, 99.0], 3).unwrap();
        assert!((r.latest - 99.0).abs() < 1e-9);
    }

    // ── Trend ─────────────────────────────────────────────────────────────

    #[test]
    fn test_trend_up() {
        let data: Vec<f64> = (0..20).map(|i| i as f64).collect();
        assert_eq!(sparkline(&data, 20).unwrap().trend, Trend::Up);
    }

    #[test]
    fn test_trend_down() {
        let data: Vec<f64> = (0..20).map(|i| (19 - i) as f64).collect();
        assert_eq!(sparkline(&data, 20).unwrap().trend, Trend::Down);
    }

    #[test]
    fn test_trend_flat() {
        let data = vec![50.0; 20];
        assert_eq!(sparkline(&data, 20).unwrap().trend, Trend::Flat);
    }

    #[test]
    fn test_trend_symbol_up()   { assert_eq!(Trend::Up.symbol(),   "↑"); }
    #[test]
    fn test_trend_symbol_down() { assert_eq!(Trend::Down.symbol(), "↓"); }
    #[test]
    fn test_trend_symbol_flat() { assert_eq!(Trend::Flat.symbol(), "→"); }

    #[test]
    fn test_trend_label() {
        assert_eq!(Trend::Up.label(),   "tăng");
        assert_eq!(Trend::Down.label(), "giảm");
        assert_eq!(Trend::Flat.label(), "ổn định");
    }

    // ── resample ──────────────────────────────────────────────────────────

    #[test]
    fn test_resample_same_len() {
        let d = vec![1.0, 2.0, 3.0];
        assert_eq!(resample(&d, 3), d);
    }

    #[test]
    fn test_resample_downsample_len() {
        let d: Vec<f64> = (0..100).map(|i| i as f64).collect();
        assert_eq!(resample(&d, 10).len(), 10);
    }

    #[test]
    fn test_resample_upsample_len() {
        assert_eq!(resample(&[0.0, 10.0], 20).len(), 20);
    }

    #[test]
    fn test_resample_single_to_many() {
        let r = resample(&[5.0], 4);
        assert_eq!(r.len(), 4);
        assert!(r.iter().all(|&v| (v - 5.0).abs() < 1e-9));
    }

    // ── MetricChart ───────────────────────────────────────────────────────

    #[test]
    fn test_metric_chart_empty_none() {
        assert!(MetricChart::build("h", "H", "u", &[]).is_none());
    }

    #[test]
    fn test_metric_chart_ok() {
        let data: Vec<f64> = (0..50).map(|i| i as f64).collect();
        let c = MetricChart::build("hashrate", "Hashrate", "kH/s", &data).unwrap();
        assert_eq!(c.metric, "hashrate");
        assert_eq!(c.data_len, 50);
    }

    #[test]
    fn test_metric_chart_print_no_panic() {
        let data: Vec<f64> = (0..30).map(|i| i as f64).collect();
        MetricChart::build("block_time", "Block Time", "s", &data)
            .unwrap()
            .print();
    }

    // ── ChartDashboard ────────────────────────────────────────────────────

    #[test]
    fn test_dashboard_empty_print_no_panic() {
        ChartDashboard::new().print();
    }

    #[test]
    fn test_dashboard_add() {
        let mut d = ChartDashboard::new();
        let data: Vec<f64> = (0..20).map(|i| i as f64).collect();
        d.add(MetricChart::build("h", "H", "u", &data).unwrap());
        assert_eq!(d.charts.len(), 1);
        d.print();
    }

    // ── mock_data ─────────────────────────────────────────────────────────

    #[test]
    fn test_mock_data_len() {
        assert_eq!(mock_data(80, 100.0, 20.0, 0.0).len(), 80);
    }

    #[test]
    fn test_mock_data_zero_amp() {
        let d = mock_data(10, 50.0, 0.0, 0.0);
        assert!(d.iter().all(|&v| (v - 50.0).abs() < 1e-9));
    }

    #[test]
    fn test_mock_data_within_reasonable_range() {
        let d = mock_data(100, 100.0, 30.0, 0.0);
        assert!(d.iter().all(|&v| v > 50.0 && v < 150.0));
    }

    // ── format_hashrate ───────────────────────────────────────────────────

    #[test]
    fn test_format_khs() { assert!(format_hashrate(500.0).contains("kH/s")); }
    #[test]
    fn test_format_mhs() { assert!(format_hashrate(5_000.0).contains("MH/s")); }
    #[test]
    fn test_format_ghs() { assert!(format_hashrate(2_000_000.0).contains("GH/s")); }

    // ── CHARTS_JS embedded ────────────────────────────────────────────────

    #[test]
    fn test_charts_js_not_empty() {
        assert!(!CHARTS_JS.is_empty());
    }

    #[test]
    fn test_charts_js_valid_utf8() {
        assert!(std::str::from_utf8(CHARTS_JS).is_ok());
    }

    #[test]
    fn test_charts_js_has_fetch() {
        let s = std::str::from_utf8(CHARTS_JS).unwrap();
        assert!(s.contains("fetch"), "charts.js phải có fetch()");
    }

    #[test]
    fn test_charts_js_has_sparkline_logic() {
        let s = std::str::from_utf8(CHARTS_JS).unwrap();
        assert!(s.contains("SPARKS") || s.contains("▁"), "charts.js phải có sparkline");
    }

    // ── cmd_charts smoke tests ────────────────────────────────────────────

    #[test]
    fn test_cmd_charts_all()        { cmd_charts(&[]); }
    #[test]
    fn test_cmd_charts_hashrate()   { cmd_charts(&["hashrate".to_string()]); }
    #[test]
    fn test_cmd_charts_block_time() { cmd_charts(&["block_time".to_string()]); }
    #[test]
    fn test_cmd_charts_tx_volume()  { cmd_charts(&["tx_volume".to_string()]); }
    #[test]
    fn test_cmd_charts_mempool()    { cmd_charts(&["mempool".to_string()]); }
    #[test]
    fn test_cmd_charts_unknown()    { cmd_charts(&["foo".to_string()]); }
}
