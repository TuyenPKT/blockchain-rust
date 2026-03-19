#![allow(dead_code)]
//! v11.3 — Scam Registry
//!
//! Lưu trữ và tra cứu thông tin rủi ro của địa chỉ blockchain.
//! Hai đường: read-only public + write admin-only.
//!
//! Endpoints:
//!   GET  /api/risk/:addr   — public, không cần auth; trả entry hoặc {"level":"unknown"}
//!   POST /api/risk/:addr   — admin role only; tạo hoặc cập nhật entry
//!   DELETE /api/risk/:addr — admin role only; xóa entry
//!
//! RiskLevel (thấp → cao): unknown → safe → low → medium → high → critical
//! RiskCategory: scam | phishing | mixer | exchange | unknown
//!
//! Input validation:
//!   - addr: 40 hex chars (PKT P2PKH) hoặc 40-66 hex (tổng quát)
//!   - notes: tối đa 512 ký tự
//!   - category/level: phải là giá trị hợp lệ trong enum

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use crate::api_auth::ApiRole;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Mức độ rủi ro của địa chỉ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Unknown,
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "unknown"  => Some(RiskLevel::Unknown),
            "safe"     => Some(RiskLevel::Safe),
            "low"      => Some(RiskLevel::Low),
            "medium"   => Some(RiskLevel::Medium),
            "high"     => Some(RiskLevel::High),
            "critical" => Some(RiskLevel::Critical),
            _          => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Unknown  => "unknown",
            RiskLevel::Safe     => "safe",
            RiskLevel::Low      => "low",
            RiskLevel::Medium   => "medium",
            RiskLevel::High     => "high",
            RiskLevel::Critical => "critical",
        }
    }

    /// Numeric score 0–100 cho sorting/comparison.
    pub fn score(&self) -> u8 {
        match self {
            RiskLevel::Unknown  => 0,
            RiskLevel::Safe     => 5,
            RiskLevel::Low      => 25,
            RiskLevel::Medium   => 50,
            RiskLevel::High     => 75,
            RiskLevel::Critical => 100,
        }
    }
}

/// Phân loại nguồn gốc rủi ro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskCategory {
    Unknown,
    Scam,
    Phishing,
    Mixer,
    Exchange,
    Sanctions,
    Ransomware,
}

impl RiskCategory {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "unknown"    => Some(RiskCategory::Unknown),
            "scam"       => Some(RiskCategory::Scam),
            "phishing"   => Some(RiskCategory::Phishing),
            "mixer"      => Some(RiskCategory::Mixer),
            "exchange"   => Some(RiskCategory::Exchange),
            "sanctions"  => Some(RiskCategory::Sanctions),
            "ransomware" => Some(RiskCategory::Ransomware),
            _            => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RiskCategory::Unknown   => "unknown",
            RiskCategory::Scam      => "scam",
            RiskCategory::Phishing  => "phishing",
            RiskCategory::Mixer     => "mixer",
            RiskCategory::Exchange  => "exchange",
            RiskCategory::Sanctions => "sanctions",
            RiskCategory::Ransomware => "ransomware",
        }
    }
}

// ─── RiskEntry ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskEntry {
    pub address:     String,
    pub level:       RiskLevel,
    pub category:    RiskCategory,
    pub notes:       String,
    /// API key ID của admin reporter (8 ký tự đầu).
    pub reporter:    String,
    pub reported_at: u64,   // unix seconds
    pub updated_at:  u64,
}

impl RiskEntry {
    pub fn new(
        address:  impl Into<String>,
        level:    RiskLevel,
        category: RiskCategory,
        notes:    impl Into<String>,
        reporter: impl Into<String>,
    ) -> Self {
        let now = unix_now();
        RiskEntry {
            address:     address.into(),
            level,
            category,
            notes:       notes.into(),
            reporter:    reporter.into(),
            reported_at: now,
            updated_at:  now,
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

// ─── ScamRegistry ─────────────────────────────────────────────────────────────

pub struct ScamRegistry {
    entries: HashMap<String, RiskEntry>,
}

impl ScamRegistry {
    pub fn new() -> Self {
        ScamRegistry { entries: HashMap::new() }
    }

    /// Thêm hoặc cập nhật entry. Trả về `true` nếu là entry mới.
    pub fn upsert(&mut self, entry: RiskEntry) -> bool {
        let is_new = !self.entries.contains_key(&entry.address);
        self.entries.insert(entry.address.clone(), entry);
        is_new
    }

    /// Tra cứu entry theo địa chỉ.
    pub fn get(&self, address: &str) -> Option<&RiskEntry> {
        self.entries.get(address)
    }

    /// Xóa entry. Trả về `true` nếu tồn tại và đã xóa.
    pub fn remove(&mut self, address: &str) -> bool {
        self.entries.remove(address).is_some()
    }

    /// Tổng số entries.
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// Tất cả entries có level >= threshold score.
    pub fn by_min_level(&self, min_score: u8) -> Vec<&RiskEntry> {
        self.entries.values()
            .filter(|e| e.level.score() >= min_score)
            .collect()
    }

    /// Tất cả entries theo category.
    pub fn by_category(&self, cat: &RiskCategory) -> Vec<&RiskEntry> {
        self.entries.values().filter(|e| &e.category == cat).collect()
    }
}

// ─── State ────────────────────────────────────────────────────────────────────

pub type RiskDb = Arc<Mutex<ScamRegistry>>;

// ─── Input validation ─────────────────────────────────────────────────────────

/// Validate địa chỉ blockchain: 32–66 hex chars (bao gồm PKT P2PKH 40 hex).
pub fn validate_address(addr: &str) -> Result<(), &'static str> {
    if addr.len() < 32 || addr.len() > 66 {
        return Err("address must be 32–66 hex characters");
    }
    if !addr.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("address must be hexadecimal");
    }
    Ok(())
}

// ─── Request body ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RiskReportRequest {
    pub level:    String,
    pub category: String,
    pub notes:    String,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn get_role(req: &Request) -> Option<&ApiRole> {
    req.extensions().get::<ApiRole>()
}

fn get_key_id(req: &Request) -> String {
    req.headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|k| k.chars().take(8).collect())
        .unwrap_or_else(|| "-".to_string())
}

fn err_resp(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/risk/:addr — public, không cần auth.
/// Trả entry nếu có; nếu không → {"level":"unknown","address":"...","score":0}.
async fn get_risk(
    Path(addr): Path<String>,
    State(db):  State<RiskDb>,
) -> axum::response::Response {
    if let Err(e) = validate_address(&addr) {
        return err_resp(StatusCode::BAD_REQUEST, e);
    }

    let reg = db.lock().await;
    match reg.get(&addr) {
        Some(entry) => Json(json!({
            "address":     entry.address,
            "level":       entry.level.as_str(),
            "score":       entry.level.score(),
            "category":    entry.category.as_str(),
            "notes":       entry.notes,
            "reporter":    entry.reporter,
            "reported_at": entry.reported_at,
            "updated_at":  entry.updated_at,
        })).into_response(),
        None => Json(json!({
            "address": addr,
            "level":   "unknown",
            "score":   0,
        })).into_response(),
    }
}

/// POST /api/risk/:addr — admin only; tạo hoặc cập nhật entry.
async fn post_risk(
    Path(addr): Path<String>,
    State(db):  State<RiskDb>,
    req: Request,
) -> axum::response::Response {
    // Admin check
    match get_role(&req) {
        Some(r) if r.is_admin() => {}
        _ => return err_resp(StatusCode::FORBIDDEN, "admin role required"),
    }
    let reporter = get_key_id(&req);

    if let Err(e) = validate_address(&addr) {
        return err_resp(StatusCode::BAD_REQUEST, e);
    }

    // Parse body
    let body = match axum::body::to_bytes(req.into_body(), 4 * 1024).await {
        Ok(b)  => b,
        Err(_) => return err_resp(StatusCode::BAD_REQUEST, "cannot read body"),
    };
    let r: RiskReportRequest = match serde_json::from_slice(&body) {
        Ok(v)  => v,
        Err(e) => return err_resp(StatusCode::BAD_REQUEST, &format!("invalid JSON: {e}")),
    };

    // Validate level
    let level = match RiskLevel::from_str(&r.level) {
        Some(l) => l,
        None    => return err_resp(StatusCode::BAD_REQUEST,
            "invalid level; must be: unknown|safe|low|medium|high|critical"),
    };

    // Validate category
    let category = match RiskCategory::from_str(&r.category) {
        Some(c) => c,
        None    => return err_resp(StatusCode::BAD_REQUEST,
            "invalid category; must be: unknown|scam|phishing|mixer|exchange|sanctions|ransomware"),
    };

    // Validate notes length
    if r.notes.len() > 512 {
        return err_resp(StatusCode::BAD_REQUEST, "notes must be ≤ 512 characters");
    }

    let mut entry = RiskEntry::new(&addr, level, category, r.notes, reporter);

    // Nếu đã có entry cũ → giữ reported_at gốc
    let mut reg = db.lock().await;
    if let Some(existing) = reg.get(&addr) {
        entry.reported_at = existing.reported_at;
    }
    let is_new = reg.upsert(entry.clone());

    tracing::info!(
        address   = addr,
        level     = entry.level.as_str(),
        category  = entry.category.as_str(),
        is_new    = is_new,
        "scam_registry: upsert"
    );

    let status = if is_new { StatusCode::CREATED } else { StatusCode::OK };
    (status, Json(json!({
        "status":   if is_new { "created" } else { "updated" },
        "address":  entry.address,
        "level":    entry.level.as_str(),
        "score":    entry.level.score(),
        "category": entry.category.as_str(),
    }))).into_response()
}

/// DELETE /api/risk/:addr — admin only; xóa entry.
async fn delete_risk(
    Path(addr): Path<String>,
    State(db):  State<RiskDb>,
    req: Request,
) -> axum::response::Response {
    match get_role(&req) {
        Some(r) if r.is_admin() => {}
        _ => return err_resp(StatusCode::FORBIDDEN, "admin role required"),
    }

    if let Err(e) = validate_address(&addr) {
        return err_resp(StatusCode::BAD_REQUEST, e);
    }

    let mut reg = db.lock().await;
    if reg.remove(&addr) {
        Json(json!({ "status": "removed", "address": addr })).into_response()
    } else {
        err_resp(StatusCode::NOT_FOUND, "address not in scam registry")
    }
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn risk_router(db: RiskDb) -> Router {
    Router::new()
        .route("/api/risk/:addr", get(get_risk).post(post_risk).delete(delete_risk))
        .with_state(db)
}

pub fn open_default() -> RiskDb {
    Arc::new(Mutex::new(ScamRegistry::new()))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(addr: &str, level: RiskLevel) -> RiskEntry {
        RiskEntry::new(addr, level, RiskCategory::Scam, "test note", "reporter1")
    }

    // ── RiskLevel ─────────────────────────────────────────────────────────────

    #[test]
    fn test_risk_level_from_str_valid() {
        assert_eq!(RiskLevel::from_str("high"),     Some(RiskLevel::High));
        assert_eq!(RiskLevel::from_str("critical"),  Some(RiskLevel::Critical));
        assert_eq!(RiskLevel::from_str("safe"),      Some(RiskLevel::Safe));
        assert_eq!(RiskLevel::from_str("unknown"),   Some(RiskLevel::Unknown));
    }

    #[test]
    fn test_risk_level_from_str_case_insensitive() {
        assert_eq!(RiskLevel::from_str("HIGH"),   Some(RiskLevel::High));
        assert_eq!(RiskLevel::from_str("Medium"), Some(RiskLevel::Medium));
    }

    #[test]
    fn test_risk_level_from_str_invalid() {
        assert_eq!(RiskLevel::from_str("extreme"), None);
        assert_eq!(RiskLevel::from_str(""),        None);
    }

    #[test]
    fn test_risk_level_score_ordering() {
        assert!(RiskLevel::Unknown.score()  < RiskLevel::Safe.score());
        assert!(RiskLevel::Safe.score()     < RiskLevel::Low.score());
        assert!(RiskLevel::Low.score()      < RiskLevel::Medium.score());
        assert!(RiskLevel::Medium.score()   < RiskLevel::High.score());
        assert!(RiskLevel::High.score()     < RiskLevel::Critical.score());
        assert_eq!(RiskLevel::Critical.score(), 100);
    }

    #[test]
    fn test_risk_level_as_str_roundtrip() {
        for level in [RiskLevel::Unknown, RiskLevel::Safe, RiskLevel::Low,
                      RiskLevel::Medium, RiskLevel::High, RiskLevel::Critical] {
            let s = level.as_str();
            assert_eq!(RiskLevel::from_str(s), Some(level));
        }
    }

    // ── RiskCategory ──────────────────────────────────────────────────────────

    #[test]
    fn test_risk_category_from_str_valid() {
        assert_eq!(RiskCategory::from_str("scam"),      Some(RiskCategory::Scam));
        assert_eq!(RiskCategory::from_str("phishing"),  Some(RiskCategory::Phishing));
        assert_eq!(RiskCategory::from_str("mixer"),     Some(RiskCategory::Mixer));
        assert_eq!(RiskCategory::from_str("exchange"),  Some(RiskCategory::Exchange));
        assert_eq!(RiskCategory::from_str("sanctions"), Some(RiskCategory::Sanctions));
        assert_eq!(RiskCategory::from_str("ransomware"),Some(RiskCategory::Ransomware));
    }

    #[test]
    fn test_risk_category_from_str_invalid() {
        assert_eq!(RiskCategory::from_str("ponzi"), None);
    }

    #[test]
    fn test_risk_category_as_str_roundtrip() {
        for cat in [RiskCategory::Unknown, RiskCategory::Scam, RiskCategory::Phishing,
                    RiskCategory::Mixer, RiskCategory::Exchange,
                    RiskCategory::Sanctions, RiskCategory::Ransomware] {
            let s = cat.as_str();
            assert_eq!(RiskCategory::from_str(s), Some(cat));
        }
    }

    // ── ScamRegistry ──────────────────────────────────────────────────────────

    #[test]
    fn test_registry_upsert_new_returns_true() {
        let mut reg = ScamRegistry::new();
        let entry   = make_entry(&"a".repeat(40), RiskLevel::High);
        assert!(reg.upsert(entry));
    }

    #[test]
    fn test_registry_upsert_existing_returns_false() {
        let mut reg   = ScamRegistry::new();
        let addr      = "b".repeat(40);
        reg.upsert(make_entry(&addr, RiskLevel::Low));
        assert!(!reg.upsert(make_entry(&addr, RiskLevel::High)));
    }

    #[test]
    fn test_registry_get_existing() {
        let mut reg = ScamRegistry::new();
        let addr    = "c".repeat(40);
        reg.upsert(make_entry(&addr, RiskLevel::Medium));
        assert!(reg.get(&addr).is_some());
        assert_eq!(reg.get(&addr).unwrap().level, RiskLevel::Medium);
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let reg = ScamRegistry::new();
        assert!(reg.get(&"d".repeat(40)).is_none());
    }

    #[test]
    fn test_registry_remove_existing() {
        let mut reg = ScamRegistry::new();
        let addr    = "e".repeat(40);
        reg.upsert(make_entry(&addr, RiskLevel::High));
        assert!(reg.remove(&addr));
        assert!(reg.get(&addr).is_none());
    }

    #[test]
    fn test_registry_remove_nonexistent() {
        let mut reg = ScamRegistry::new();
        assert!(!reg.remove(&"f".repeat(40)));
    }

    #[test]
    fn test_registry_count() {
        let mut reg = ScamRegistry::new();
        assert_eq!(reg.count(), 0);
        reg.upsert(make_entry(&"g".repeat(40), RiskLevel::Low));
        reg.upsert(make_entry(&"h".repeat(40), RiskLevel::High));
        assert_eq!(reg.count(), 2);
    }

    #[test]
    fn test_registry_by_min_level() {
        let mut reg = ScamRegistry::new();
        reg.upsert(make_entry(&"i".repeat(40), RiskLevel::Low));
        reg.upsert(make_entry(&"j".repeat(40), RiskLevel::High));
        reg.upsert(make_entry(&"k".repeat(40), RiskLevel::Critical));
        let high_plus = reg.by_min_level(RiskLevel::High.score());
        assert_eq!(high_plus.len(), 2); // High + Critical
    }

    #[test]
    fn test_registry_by_category() {
        let mut reg = ScamRegistry::new();
        let e1 = RiskEntry::new(&"l".repeat(40), RiskLevel::High,
                                RiskCategory::Scam,    "n", "r");
        let e2 = RiskEntry::new(&"m".repeat(40), RiskLevel::Low,
                                RiskCategory::Phishing,"n", "r");
        reg.upsert(e1);
        reg.upsert(e2);
        let scams = reg.by_category(&RiskCategory::Scam);
        assert_eq!(scams.len(), 1);
    }

    // ── validate_address ──────────────────────────────────────────────────────

    #[test]
    fn test_validate_address_valid_40_hex() {
        assert!(validate_address(&"a".repeat(40)).is_ok());
    }

    #[test]
    fn test_validate_address_valid_66_hex() {
        assert!(validate_address(&"b".repeat(66)).is_ok());
    }

    #[test]
    fn test_validate_address_too_short() {
        assert!(validate_address(&"c".repeat(31)).is_err());
    }

    #[test]
    fn test_validate_address_too_long() {
        assert!(validate_address(&"d".repeat(67)).is_err());
    }

    #[test]
    fn test_validate_address_non_hex() {
        assert!(validate_address(&"z".repeat(40)).is_err());
    }
}
