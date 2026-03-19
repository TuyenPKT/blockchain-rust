#![allow(dead_code)]
//! v10.1 — Audit Log (structured, daily-rotating)
//!
//! Enhanced audit log với `api_key_id` field — mọi HTTP request đều được ghi.
//! Daily rotation: một file per ngày tại `~/.pkt/audit/audit-YYYY-MM-DD.log`.
//!
//! Format mỗi dòng (pipe-separated):
//!   `<unix_ts>|<ip>|<method>|<path>|<status>|<api_key_id>|<latency_ms>`
//!
//! Endpoint (admin role only):
//!   GET /api/admin/logs?date=YYYY-MM-DD&limit=N&offset=N
//!
//! Middleware:
//!   .layer(middleware::from_fn_with_state(audit_db, audit_log::audit_middleware))
//!
//! Quyết định thiết kế:
//!   - `api_key_id` = 8 ký tự đầu của raw X-API-Key header (nếu có), else "-"
//!   - zt_middleware audit log (~/.pkt/audit.log) vẫn tồn tại — log này tách biệt
//!   - Admin check: đọc `ApiRole` extension được gắn bởi auth_middleware (v10.0)

use axum::{
    body::Body,
    extract::{Query, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

// ─── Entry ────────────────────────────────────────────────────────────────────

/// One parsed audit log line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub unix_ts:    u64,
    pub ip:         String,
    pub method:     String,
    pub path:       String,
    pub status:     u16,
    pub api_key_id: String,
    pub latency_ms: u64,
}

impl AuditEntry {
    /// Serialize to pipe-separated log line (no trailing newline).
    pub fn to_log_line(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.unix_ts, self.ip, self.method,
            self.path, self.status, self.api_key_id, self.latency_ms,
        )
    }

    /// Parse a pipe-separated log line.  Returns `None` on malformed input.
    pub fn from_log_line(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.trim().splitn(7, '|').collect();
        if parts.len() != 7 {
            return None;
        }
        Some(AuditEntry {
            unix_ts:    parts[0].parse().ok()?,
            ip:         parts[1].to_string(),
            method:     parts[2].to_string(),
            path:       parts[3].to_string(),
            status:     parts[4].parse().ok()?,
            api_key_id: parts[5].to_string(),
            latency_ms: parts[6].parse().ok()?,
        })
    }
}

// ─── Logger ───────────────────────────────────────────────────────────────────

/// Append-only audit logger with daily file rotation.
pub struct AuditLogger {
    /// Directory containing daily log files.
    pub dir: PathBuf,
    /// Date of currently open file: "YYYY-MM-DD".
    current_date: String,
    writer: Option<BufWriter<File>>,
}

/// Shared handle used as axum State.
pub type AuditDb = Arc<Mutex<AuditLogger>>;

impl AuditLogger {
    /// Default log directory: `~/.pkt/audit/`.
    pub fn default_dir() -> PathBuf {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".pkt")
            .join("audit")
    }

    /// Open logger.  Creates the directory if it does not exist.
    pub fn open(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let today = today_str();
        let writer = open_log_file(&dir, &today);
        AuditLogger { dir, current_date: today, writer }
    }

    /// Open from default directory.
    pub fn open_default() -> AuditDb {
        Arc::new(Mutex::new(Self::open(Self::default_dir())))
    }

    /// Write one audit entry, rotating to a new file if the date has changed.
    pub fn write(&mut self, entry: &AuditEntry) {
        let today = today_str();
        if today != self.current_date {
            // Rotate: flush old file, open new one
            self.writer = None;
            self.current_date = today.clone();
            self.writer = open_log_file(&self.dir, &today);
        }
        if let Some(w) = self.writer.as_mut() {
            let line = entry.to_log_line() + "\n";
            let _ = w.write_all(line.as_bytes());
            let _ = w.flush();
        }
    }

    /// Path of the log file for a given date string ("YYYY-MM-DD").
    pub fn log_path_for(&self, date: &str) -> PathBuf {
        self.dir.join(format!("audit-{}.log", date))
    }

    /// Read up to `limit` entries from the log for a given date, starting at `offset`.
    pub fn read_entries(&self, date: &str, offset: usize, limit: usize) -> Vec<AuditEntry> {
        let path = self.log_path_for(date);
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        content
            .lines()
            .filter_map(AuditEntry::from_log_line)
            .skip(offset)
            .take(limit)
            .collect()
    }

    /// Total line count for a given date's log file.
    pub fn entry_count(&self, date: &str) -> usize {
        let path = self.log_path_for(date);
        std::fs::read_to_string(&path)
            .map(|c| c.lines().filter(|l| !l.is_empty()).count())
            .unwrap_or(0)
    }
}

fn open_log_file(dir: &PathBuf, date: &str) -> Option<BufWriter<File>> {
    let path = dir.join(format!("audit-{}.log", date));
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()
        .map(BufWriter::new)
}

/// Current date as "YYYY-MM-DD" (UTC approximation from unix timestamp).
pub fn today_str() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    unix_to_date(secs)
}

/// Convert unix timestamp to "YYYY-MM-DD" (simplified, no timezone).
pub fn unix_to_date(secs: u64) -> String {
    // Days since 1970-01-01
    let days = secs / 86400;
    let mut y = 1970u64;
    let mut rem = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if rem < days_in_year { break; }
        rem -= days_in_year;
        y += 1;
    }
    let months = if is_leap(y) {
        [31u64, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0usize;
    while m < 12 && rem >= months[m] {
        rem -= months[m];
        m += 1;
    }
    format!("{:04}-{:02}-{:02}", y, m + 1, rem + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Extract api_key_id from X-API-Key header: first 8 chars, else "-".
pub fn extract_key_id(req: &Request<Body>) -> String {
    req.headers()
        .get("x-api-key")
        .and_then(|h| h.to_str().ok())
        .map(|k| k.chars().take(8).collect())
        .unwrap_or_else(|| "-".to_string())
}

/// Extract client IP (mirrors zt_middleware logic).
fn extract_ip(req: &Request<Body>) -> String {
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

// ─── Axum middleware ──────────────────────────────────────────────────────────

/// Axum middleware — log every request to the daily audit file.
///
/// Captures: IP, method, path, status, X-API-Key first-8, latency_ms.
/// Must be layered AFTER auth_middleware so that the key_id is available.
pub async fn audit_middleware(
    State(db): State<AuditDb>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let start      = Instant::now();
    let ip         = extract_ip(&request);
    let method     = request.method().as_str().to_string();
    let path       = request.uri().path().to_string();
    let api_key_id = extract_key_id(&request);

    let response = next.run(request).await;

    let entry = AuditEntry {
        unix_ts:    unix_now(),
        ip,
        method,
        path,
        status:     response.status().as_u16(),
        api_key_id,
        latency_ms: start.elapsed().as_millis() as u64,
    };

    db.lock().await.write(&entry);
    response
}

// ─── Admin endpoint ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Date to query: "YYYY-MM-DD" (default: today).
    pub date:   Option<String>,
    /// Max entries to return (default 100, max 500).
    pub limit:  Option<usize>,
    /// Skip N entries (default 0).
    pub offset: Option<usize>,
}

pub fn admin_router(db: AuditDb) -> Router {
    Router::new()
        .route("/api/admin/logs", get(get_admin_logs))
        .with_state(db)
}

/// GET /api/admin/logs — admin role only.
async fn get_admin_logs(
    State(db):  State<AuditDb>,
    Query(q):   Query<LogsQuery>,
    request:    Request<Body>,
) -> Response {
    // Admin-only: check ApiRole extension set by auth_middleware
    let role = request.extensions().get::<crate::api_auth::ApiRole>().cloned();
    if role.as_ref() != Some(&crate::api_auth::ApiRole::Admin) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "admin role required" })),
        )
            .into_response();
    }

    let date   = q.date.clone().unwrap_or_else(today_str);
    let limit  = q.limit.unwrap_or(100).min(500);
    let offset = q.offset.unwrap_or(0);

    let logger  = db.lock().await;
    let total   = logger.entry_count(&date);
    let entries = logger.read_entries(&date, offset, limit);

    Json(json!({
        "date":    date,
        "total":   total,
        "offset":  offset,
        "limit":   limit,
        "entries": entries,
    }))
    .into_response()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── AuditEntry ────────────────────────────────────────────────────────

    #[test]
    fn test_entry_to_log_line_format() {
        let e = AuditEntry {
            unix_ts: 1700000000, ip: "1.2.3.4".into(), method: "GET".into(),
            path: "/api/stats".into(), status: 200, api_key_id: "ab12cd34".into(),
            latency_ms: 5,
        };
        let line = e.to_log_line();
        assert_eq!(line, "1700000000|1.2.3.4|GET|/api/stats|200|ab12cd34|5");
    }

    #[test]
    fn test_entry_roundtrip() {
        let e = AuditEntry {
            unix_ts: 9999, ip: "::1".into(), method: "POST".into(),
            path: "/api/tx".into(), status: 400, api_key_id: "-".into(),
            latency_ms: 12,
        };
        let parsed = AuditEntry::from_log_line(&e.to_log_line()).unwrap();
        assert_eq!(parsed.unix_ts, 9999);
        assert_eq!(parsed.ip, "::1");
        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.path, "/api/tx");
        assert_eq!(parsed.status, 400);
        assert_eq!(parsed.api_key_id, "-");
        assert_eq!(parsed.latency_ms, 12);
    }

    #[test]
    fn test_entry_from_log_line_bad_input() {
        assert!(AuditEntry::from_log_line("bad").is_none());
        assert!(AuditEntry::from_log_line("").is_none());
        assert!(AuditEntry::from_log_line("a|b|c").is_none());
    }

    #[test]
    fn test_entry_from_log_line_nonnumeric_ts() {
        let bad = "NOT_A_NUM|1.2.3.4|GET|/api/stats|200|key1|5";
        assert!(AuditEntry::from_log_line(bad).is_none());
    }

    #[test]
    fn test_entry_from_log_line_nonnumeric_status() {
        let bad = "1700000000|1.2.3.4|GET|/api/stats|OK|key1|5";
        assert!(AuditEntry::from_log_line(bad).is_none());
    }

    #[test]
    fn test_entry_to_log_line_no_trailing_newline() {
        let e = AuditEntry {
            unix_ts: 1, ip: "x".into(), method: "G".into(),
            path: "/".into(), status: 200, api_key_id: "-".into(), latency_ms: 0,
        };
        assert!(!e.to_log_line().ends_with('\n'));
    }

    // ── unix_to_date ──────────────────────────────────────────────────────

    #[test]
    fn test_unix_to_date_epoch() {
        // 1970-01-01
        assert_eq!(unix_to_date(0), "1970-01-01");
    }

    #[test]
    fn test_unix_to_date_known() {
        // 2024-01-01 = days 19723 from epoch
        // 19723 * 86400 = 1704067200
        assert_eq!(unix_to_date(1704067200), "2024-01-01");
    }

    #[test]
    fn test_unix_to_date_format_length() {
        let d = unix_to_date(1700000000);
        assert_eq!(d.len(), 10);
        assert_eq!(&d[4..5], "-");
        assert_eq!(&d[7..8], "-");
    }

    #[test]
    fn test_today_str_format() {
        let d = today_str();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20")); // year 2xxx
    }

    // ── AuditLogger ───────────────────────────────────────────────────────

    #[test]
    fn test_logger_log_path_format() {
        let logger = AuditLogger {
            dir: PathBuf::from("/tmp/pkt_audit_test"),
            current_date: "2024-01-01".into(),
            writer: None,
        };
        let p = logger.log_path_for("2024-03-15");
        assert!(p.to_string_lossy().contains("audit-2024-03-15.log"));
    }

    #[test]
    fn test_logger_write_and_read() {
        let dir = std::env::temp_dir().join("pkt_audit_log_test");
        let _ = std::fs::remove_dir_all(&dir);
        let mut logger = AuditLogger::open(dir.clone());

        let entry = AuditEntry {
            unix_ts: 1700000001, ip: "127.0.0.1".into(), method: "GET".into(),
            path: "/api/stats".into(), status: 200, api_key_id: "test1234".into(),
            latency_ms: 3,
        };
        logger.write(&entry);
        drop(logger.writer.take()); // flush

        let today = today_str();
        let logger2 = AuditLogger { dir: dir.clone(), current_date: today.clone(), writer: None };
        let entries = logger2.read_entries(&today, 0, 100);
        assert!(!entries.is_empty());
        assert_eq!(entries[0].ip, "127.0.0.1");
        assert_eq!(entries[0].api_key_id, "test1234");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_logger_entry_count() {
        let dir = std::env::temp_dir().join("pkt_audit_count_test");
        let _ = std::fs::remove_dir_all(&dir);
        let mut logger = AuditLogger::open(dir.clone());
        let today = today_str();

        for i in 0..5u64 {
            logger.write(&AuditEntry {
                unix_ts: i, ip: "x".into(), method: "GET".into(),
                path: "/".into(), status: 200, api_key_id: "-".into(), latency_ms: 1,
            });
        }
        drop(logger.writer.take());

        let logger2 = AuditLogger { dir: dir.clone(), current_date: today.clone(), writer: None };
        assert_eq!(logger2.entry_count(&today), 5);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_logger_read_missing_date() {
        let logger = AuditLogger {
            dir: PathBuf::from("/tmp/pkt_audit_no_such_dir_xyz"),
            current_date: "2000-01-01".into(),
            writer: None,
        };
        let result = logger.read_entries("2000-01-01", 0, 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_logger_entry_count_missing_date() {
        let logger = AuditLogger {
            dir: PathBuf::from("/tmp/pkt_audit_no_such_dir_xyz"),
            current_date: "2000-01-01".into(),
            writer: None,
        };
        assert_eq!(logger.entry_count("2000-01-01"), 0);
    }

    // ── extract_key_id ────────────────────────────────────────────────────

    #[test]
    fn test_extract_key_id_present() {
        use axum::http::Request;
        let req = Request::builder()
            .header("x-api-key", "abcd1234efgh5678")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_key_id(&req), "abcd1234");
    }

    #[test]
    fn test_extract_key_id_absent() {
        use axum::http::Request;
        let req = Request::builder().body(Body::empty()).unwrap();
        assert_eq!(extract_key_id(&req), "-");
    }

    #[test]
    fn test_extract_key_id_short_key() {
        use axum::http::Request;
        let req = Request::builder()
            .header("x-api-key", "abc")
            .body(Body::empty())
            .unwrap();
        // Only 3 chars available → return those 3
        assert_eq!(extract_key_id(&req), "abc");
    }

    // ── admin_router builds ────────────────────────────────────────────────

    #[test]
    fn test_admin_router_builds() {
        let db = AuditLogger::open_default();
        let _r = admin_router(db);
    }
}
