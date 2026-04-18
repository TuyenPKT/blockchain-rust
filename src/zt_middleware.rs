#![allow(dead_code)]
//! v9.0 — Zero-Trust Middleware
//!
//! Axum middleware layer applied to ALL PKTScan endpoints.
//! Enforces four Zero-Trust controls on every HTTP request:
//!
//!   1. Request-ID   — `X-Request-ID` header on every response (blake3 of ts+counter)
//!   2. Rate Limiter — 100 req / 60 s per IP; HTTP 429 on exceed; `Retry-After: 60`
//!   3. Input Guard  — reject path > 256 chars, query > 512 chars, null bytes, `../`
//!   4. Audit Log    — append-only `~/.pkt/audit.log`:
//!                     `<unix_ts>|<req_id>|<ip>|<method>|<path>|<status>|<ms>`
//!
//! Usage (in pktscan_api::serve):
//!   let zt = ZtState::new(ZtConfig::default());
//!   let app = router(...)
//!       .layer(middleware::from_fn_with_state(zt, zt_middleware));

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

// ─── Config ───────────────────────────────────────────────────────────────────

/// Tunable parameters for the Zero-Trust middleware.
#[derive(Debug, Clone)]
pub struct ZtConfig {
    /// Max requests per IP per window. Default: 600.
    pub max_per_window: u32,
    /// Sliding-window length in seconds. Default: 60.
    pub window_secs: u64,
    /// Max URL query-string length in bytes. Default: 512.
    pub max_query_len: usize,
    /// Max URL path length in bytes. Default: 256.
    pub max_path_len: usize,
    /// Audit log file path. `None` disables file logging.
    pub log_path: Option<PathBuf>,
    /// Max number of unique IPs tracked in rate-limit map.
    /// Prevents unbounded memory growth when XFF headers are spoofed.
    /// Default: 10_000. When full, expired entries are purged first; if still
    /// full the new IP is rate-limited without being tracked (fail-closed).
    pub max_tracked_ips: usize,
    /// Trust X-Forwarded-For / X-Real-IP headers for the client IP.
    /// Set to `true` only when the server is behind a trusted reverse proxy.
    /// Default: reads `PKT_TRUSTED_PROXY=1` env var; otherwise `false`.
    pub trust_proxy: bool,
}

impl Default for ZtConfig {
    fn default() -> Self {
        ZtConfig {
            max_per_window:  600,
            window_secs:     60,
            max_query_len:   512,
            max_path_len:    256,
            log_path:        Some(default_log_path()),
            max_tracked_ips: 10_000,
            trust_proxy:     std::env::var("PKT_TRUSTED_PROXY").as_deref() == Ok("1"),
        }
    }
}

fn default_log_path() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".pkt")
        .join("audit.log")
}

// ─── Shared state ─────────────────────────────────────────────────────────────

/// ip → (request_count, window_start_unix_secs)
type RateCounts = Mutex<HashMap<String, (u32, u64)>>;

/// Shared, cheaply-cloneable Zero-Trust state (wrapped in Arc).
pub struct ZtState {
    pub config:  ZtConfig,
    rate_counts: RateCounts,
    counter:     AtomicU64,
    log_writer:  Mutex<Option<BufWriter<File>>>,
}

impl ZtState {
    /// Create a new `ZtState` wrapped in `Arc`.
    pub fn new(config: ZtConfig) -> Arc<Self> {
        let log_writer = config.log_path.as_ref().and_then(|p| {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .ok()
                .map(BufWriter::new)
        });

        Arc::new(ZtState {
            config,
            rate_counts: Mutex::new(HashMap::new()),
            counter:     AtomicU64::new(0),
            log_writer:  Mutex::new(log_writer),
        })
    }

    // ── Request-ID ────────────────────────────────────────────────────────

    /// Generate a 16-hex-char request ID: blake3(subsec_nanos XOR counter).
    pub fn make_request_id(&self) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        let hash  = blake3::hash(&(nanos ^ count).to_le_bytes());
        hash.to_hex()[..16].to_string()
    }

    // ── Rate Limiter ──────────────────────────────────────────────────────

    /// Returns `true` if the IP is still within its rate limit.
    pub async fn check_rate(&self, ip: &str) -> bool {
        let now = unix_now();
        let mut counts = self.rate_counts.lock().await;

        // Fast-path: already tracked
        if let Some(entry) = counts.get_mut(ip) {
            if now.saturating_sub(entry.1) >= self.config.window_secs {
                *entry = (1, now);
                return true;
            }
            entry.0 += 1;
            return entry.0 <= self.config.max_per_window;
        }

        // New IP — enforce map size cap
        if counts.len() >= self.config.max_tracked_ips {
            // Purge expired entries first
            counts.retain(|_, (_, ts)| now.saturating_sub(*ts) < self.config.window_secs);
            // If still full, reject without tracking (fail-closed)
            if counts.len() >= self.config.max_tracked_ips {
                return false;
            }
        }

        counts.insert(ip.to_string(), (1, now));
        true
    }

    /// Current request count for an IP in the active window.
    pub async fn count_for(&self, ip: &str) -> u32 {
        self.rate_counts.lock().await
            .get(ip)
            .map(|(c, _)| *c)
            .unwrap_or(0)
    }

    /// Reset the rate counter for an IP (e.g. after a ban is lifted).
    pub async fn reset_rate(&self, ip: &str) {
        self.rate_counts.lock().await.remove(ip);
    }

    // ── Input Guard ───────────────────────────────────────────────────────

    /// Validate path + query string.
    /// Returns `Err(&'static str)` with a short reason on rejection.
    pub fn validate_input(&self, path: &str, query: Option<&str>) -> Result<(), &'static str> {
        if path.len() > self.config.max_path_len {
            return Err("path too long");
        }
        if path.contains('\0') || path.contains("../") {
            return Err("invalid path");
        }
        if let Some(q) = query {
            if q.len() > self.config.max_query_len {
                return Err("query string too long");
            }
            if q.contains('\0') {
                return Err("null byte in query");
            }
        }
        Ok(())
    }

    // ── Audit Log ─────────────────────────────────────────────────────────

    /// Append one structured audit log line.
    /// Format: `<unix_ts>|<req_id>|<ip>|<method>|<path>|<status>|<ms>`
    pub async fn audit(
        &self,
        req_id: &str,
        ip:     &str,
        method: &str,
        path:   &str,
        status: u16,
        ms:     u128,
    ) {
        let line = format!("{}|{}|{}|{}|{}|{}|{}\n",
            unix_now(), req_id, ip, method, path, status, ms);
        let mut guard = self.log_writer.lock().await;
        if let Some(w) = guard.as_mut() {
            let _ = w.write_all(line.as_bytes());
            let _ = w.flush();
        }
    }
}

// ─── Middleware function ───────────────────────────────────────────────────────

/// Axum middleware — attach to router via `from_fn_with_state`.
pub async fn zt_middleware(
    State(zt): State<Arc<ZtState>>,
    request:   Request<Body>,
    next:      Next,
) -> Response {
    let start  = Instant::now();
    let ip     = extract_ip_with_trust(&request, zt.config.trust_proxy);
    let method = request.method().as_str().to_string();
    let path   = request.uri().path().to_string();
    let query  = request.uri().query().map(str::to_string);
    let req_id = zt.make_request_id();

    // ── 1. Input guard ────────────────────────────────────────────────────
    if let Err(reason) = zt.validate_input(&path, query.as_deref()) {
        let status = StatusCode::BAD_REQUEST;
        zt.audit(&req_id, &ip, &method, &path, status.as_u16(),
                 start.elapsed().as_millis()).await;
        return bad_request_response(reason, &req_id);
    }

    // ── 2. Rate limiter ───────────────────────────────────────────────────
    if !zt.check_rate(&ip).await {
        let status = StatusCode::TOO_MANY_REQUESTS;
        zt.audit(&req_id, &ip, &method, &path, status.as_u16(),
                 start.elapsed().as_millis()).await;
        let mut r = (status, "{\"error\":\"rate limit exceeded\"}").into_response();
        attach_req_id(&mut r, &req_id);
        r.headers_mut().insert("Retry-After", HeaderValue::from_static("60"));
        return r;
    }

    // ── 3. Run handler ────────────────────────────────────────────────────
    let mut response = next.run(request).await;
    let ms     = start.elapsed().as_millis();
    let status = response.status().as_u16();

    // ── 4. Attach Request-ID to response ──────────────────────────────────
    attach_req_id(&mut response, &req_id);

    // ── 5. Audit log ──────────────────────────────────────────────────────
    zt.audit(&req_id, &ip, &method, &path, status, ms).await;

    response
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Extract client IP: X-Forwarded-For → X-Real-IP → "unknown".
/// Extract client IP.  Only reads proxy headers if `trust_proxy = true`
/// (set via `PKT_TRUSTED_PROXY=1`).  Otherwise returns "unknown" to prevent
/// rate-limit bypass via spoofed X-Forwarded-For headers.
pub fn extract_ip_with_trust(req: &Request<Body>, trust_proxy: bool) -> String {
    if !trust_proxy {
        return "unknown".to_string();
    }
    if let Some(fwd) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = fwd.to_str() {
            let first = s.split(',').next().unwrap_or("").trim();
            if !first.is_empty() { return first.to_string(); }
        }
    }
    if let Some(real) = req.headers().get("x-real-ip") {
        if let Ok(s) = real.to_str() { return s.trim().to_string(); }
    }
    "unknown".to_string()
}

// Back-compat alias — always trust (used by audit_log where IP is forensics-only)
pub fn extract_ip(req: &Request<Body>) -> String {
    extract_ip_with_trust(req, true)
}

fn attach_req_id(response: &mut Response, req_id: &str) {
    if let Ok(hv) = HeaderValue::from_str(req_id) {
        response.headers_mut().insert("X-Request-ID", hv);
    }
}

fn bad_request_response(reason: &str, req_id: &str) -> Response {
    let body = format!("{{\"error\":\"{reason}\"}}");
    let mut r = (StatusCode::BAD_REQUEST, body).into_response();
    attach_req_id(&mut r, req_id);
    r
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn zt_no_log() -> Arc<ZtState> {
        let config = ZtConfig {
            log_path: None,
            ..ZtConfig::default()
        };
        ZtState::new(config)
    }

    // ── Request-ID ────────────────────────────────────────────────────────

    #[test]
    fn test_request_id_length() {
        let zt = zt_no_log();
        let id = zt.make_request_id();
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn test_request_id_is_hex() {
        let zt = zt_no_log();
        let id = zt.make_request_id();
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "not hex: {id}");
    }

    #[test]
    fn test_request_ids_unique() {
        let zt = zt_no_log();
        let a  = zt.make_request_id();
        let b  = zt.make_request_id();
        let c  = zt.make_request_id();
        assert_ne!(a, b);
        assert_ne!(b, c);
    }

    #[test]
    fn test_counter_increments() {
        let zt = zt_no_log();
        let before = zt.counter.load(Ordering::Relaxed);
        zt.make_request_id();
        zt.make_request_id();
        let after = zt.counter.load(Ordering::Relaxed);
        assert_eq!(after, before + 2);
    }

    // ── Rate Limiter ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_rate_allows_within_limit() {
        let config = ZtConfig { max_per_window: 5, window_secs: 60, log_path: None, ..ZtConfig::default() };
        let zt = ZtState::new(config);
        for _ in 0..5 {
            assert!(zt.check_rate("1.2.3.4").await);
        }
    }

    #[tokio::test]
    async fn test_rate_blocks_on_exceed() {
        let config = ZtConfig { max_per_window: 3, window_secs: 60, log_path: None, ..ZtConfig::default() };
        let zt = ZtState::new(config);
        for _ in 0..3 {
            zt.check_rate("1.2.3.4").await;
        }
        assert!(!zt.check_rate("1.2.3.4").await);
    }

    #[tokio::test]
    async fn test_rate_independent_per_ip() {
        let config = ZtConfig { max_per_window: 2, window_secs: 60, log_path: None, ..ZtConfig::default() };
        let zt = ZtState::new(config);
        zt.check_rate("1.1.1.1").await;
        zt.check_rate("1.1.1.1").await;
        // 1.1.1.1 is at limit, but 2.2.2.2 has its own counter
        assert!(!zt.check_rate("1.1.1.1").await);
        assert!(zt.check_rate("2.2.2.2").await);
    }

    #[tokio::test]
    async fn test_rate_reset_clears_counter() {
        let config = ZtConfig { max_per_window: 1, window_secs: 60, log_path: None, ..ZtConfig::default() };
        let zt = ZtState::new(config);
        zt.check_rate("ip").await;
        assert!(!zt.check_rate("ip").await);
        zt.reset_rate("ip").await;
        assert!(zt.check_rate("ip").await);
    }

    #[tokio::test]
    async fn test_count_for_unknown_ip_is_zero() {
        let zt = zt_no_log();
        assert_eq!(zt.count_for("9.9.9.9").await, 0);
    }

    #[tokio::test]
    async fn test_count_for_tracks_requests() {
        let config = ZtConfig { max_per_window: 100, window_secs: 60, log_path: None, ..ZtConfig::default() };
        let zt = ZtState::new(config);
        zt.check_rate("ip").await;
        zt.check_rate("ip").await;
        assert_eq!(zt.count_for("ip").await, 2);
    }

    // ── Input Guard ───────────────────────────────────────────────────────

    #[test]
    fn test_valid_path_passes() {
        let zt = zt_no_log();
        assert!(zt.validate_input("/api/blocks", None).is_ok());
    }

    #[test]
    fn test_valid_path_with_query_passes() {
        let zt = zt_no_log();
        assert!(zt.validate_input("/api/txs", Some("limit=20&from=100")).is_ok());
    }

    #[test]
    fn test_path_too_long_rejected() {
        let zt = zt_no_log();
        let long = "/".repeat(300);
        assert_eq!(zt.validate_input(&long, None), Err("path too long"));
    }

    #[test]
    fn test_path_null_byte_rejected() {
        let zt = zt_no_log();
        assert_eq!(zt.validate_input("/api/bl\0cks", None), Err("invalid path"));
    }

    #[test]
    fn test_path_traversal_rejected() {
        let zt = zt_no_log();
        assert_eq!(zt.validate_input("/api/../etc/passwd", None), Err("invalid path"));
    }

    #[test]
    fn test_query_too_long_rejected() {
        let zt = zt_no_log();
        let long_q = "q=".to_string() + &"x".repeat(520);
        assert_eq!(zt.validate_input("/api/search", Some(&long_q)), Err("query string too long"));
    }

    #[test]
    fn test_query_null_byte_rejected() {
        let zt = zt_no_log();
        assert_eq!(zt.validate_input("/api/search", Some("q=hello\0world")), Err("null byte in query"));
    }

    #[test]
    fn test_query_none_passes() {
        let zt = zt_no_log();
        assert!(zt.validate_input("/api/stats", None).is_ok());
    }

    #[test]
    fn test_path_exactly_max_len_passes() {
        let config = ZtConfig { max_path_len: 10, log_path: None, ..ZtConfig::default() };
        let zt = ZtState::new(config);
        let path = "/".repeat(10);
        assert!(zt.validate_input(&path, None).is_ok());
    }

    #[test]
    fn test_path_one_over_max_rejected() {
        let config = ZtConfig { max_path_len: 10, log_path: None, ..ZtConfig::default() };
        let zt = ZtState::new(config);
        let path = "/".repeat(11);
        assert_eq!(zt.validate_input(&path, None), Err("path too long"));
    }

    // ── IP extraction ─────────────────────────────────────────────────────

    #[test]
    fn test_extract_ip_x_forwarded_for() {
        use axum::http::Request;
        let req = Request::builder()
            .header("x-forwarded-for", "1.2.3.4, 5.6.7.8")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_ip(&req), "1.2.3.4");
    }

    #[test]
    fn test_extract_ip_x_real_ip() {
        use axum::http::Request;
        let req = Request::builder()
            .header("x-real-ip", "9.8.7.6")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_ip(&req), "9.8.7.6");
    }

    #[test]
    fn test_extract_ip_fallback_unknown() {
        use axum::http::Request;
        let req = Request::builder().body(Body::empty()).unwrap();
        assert_eq!(extract_ip(&req), "unknown");
    }

    #[test]
    fn test_extract_ip_prefers_forwarded_over_real() {
        use axum::http::Request;
        let req = Request::builder()
            .header("x-forwarded-for", "1.2.3.4")
            .header("x-real-ip", "9.9.9.9")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_ip(&req), "1.2.3.4");
    }

    // ── Audit log ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_audit_no_log_does_not_panic() {
        let zt = zt_no_log();
        // Should complete silently even with no log file
        zt.audit("req001", "1.1.1.1", "GET", "/api/stats", 200, 5).await;
    }

    #[tokio::test]
    async fn test_audit_writes_to_file() {
        use std::io::BufRead;
        let tmp = std::env::temp_dir().join("pkt_audit_test.log");
        let _ = std::fs::remove_file(&tmp);

        let config = ZtConfig { log_path: Some(tmp.clone()), ..ZtConfig::default() };
        let zt = ZtState::new(config);
        zt.audit("abc123", "1.2.3.4", "GET", "/api/stats", 200, 12).await;
        // Drop the zt to flush
        drop(zt);

        let file    = File::open(&tmp).expect("log file should exist");
        let mut rdr = std::io::BufReader::new(file);
        let mut line = String::new();
        rdr.read_line(&mut line).unwrap();

        assert!(line.contains("abc123"));
        assert!(line.contains("1.2.3.4"));
        assert!(line.contains("GET"));
        assert!(line.contains("/api/stats"));
        assert!(line.contains("|200|"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn test_audit_format_fields() {
        use std::io::Read;
        let tmp = std::env::temp_dir().join("pkt_audit_format_test.log");
        let _ = std::fs::remove_file(&tmp);

        let config = ZtConfig { log_path: Some(tmp.clone()), ..ZtConfig::default() };
        let zt = ZtState::new(config);
        zt.audit("req001", "5.5.5.5", "POST", "/api/tx", 400, 3).await;
        drop(zt);

        let mut content = String::new();
        File::open(&tmp).unwrap().read_to_string(&mut content).unwrap();
        let parts: Vec<&str> = content.trim().split('|').collect();
        // ts|req_id|ip|method|path|status|ms
        assert_eq!(parts.len(), 7);
        assert_eq!(parts[1], "req001");
        assert_eq!(parts[2], "5.5.5.5");
        assert_eq!(parts[3], "POST");
        assert_eq!(parts[4], "/api/tx");
        assert_eq!(parts[5], "400");
        assert_eq!(parts[6], "3");
        let _ = std::fs::remove_file(&tmp);
    }

    // ── Config defaults ───────────────────────────────────────────────────

    #[test]
    fn test_default_config_values() {
        let cfg = ZtConfig::default();
        assert_eq!(cfg.max_per_window, 600);
        assert_eq!(cfg.window_secs, 60);
        assert_eq!(cfg.max_query_len, 512);
        assert_eq!(cfg.max_path_len, 256);
        assert!(cfg.log_path.is_some());
    }

    #[test]
    fn test_default_log_path_contains_pkt() {
        let p = default_log_path();
        assert!(p.to_string_lossy().contains(".pkt"));
        assert!(p.to_string_lossy().contains("audit.log"));
    }
}
