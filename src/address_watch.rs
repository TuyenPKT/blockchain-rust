#![allow(dead_code)]
//! v11.4 — Address Watch
//!
//! Watch địa chỉ blockchain, trigger HTTP callback khi có TX mới.
//!
//! Cơ chế:
//!   1. API key write role đăng ký watch (địa chỉ + callback_url)
//!   2. Background poller (spawn_watcher) quét chain mỗi POLL_INTERVAL_SECS
//!   3. Khi phát hiện TX height > last_seen_height cho địa chỉ → POST callback
//!   4. Callback payload dùng cùng format với WebhookPayload::address_activity()
//!
//! Endpoints:
//!   POST   /api/watch         — write role; đăng ký watch mới
//!   GET    /api/watch         — write role; liệt kê watches của API key
//!   DELETE /api/watch/:id     — write role; xóa watch (chỉ owner)
//!
//! Callback HTTP POST body:
//!   { "event": "address_activity", "address": "...", "tx_id": "...",
//!     "amount": N, "block_height": N, "timestamp": N }
//! Header: X-Watch-ID: <watch_id>

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    body::to_bytes,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use crate::address_index::history_for_addr;
use crate::api_auth::ApiRole;
use crate::pktscan_api::ScanDb;
use crate::scam_registry::validate_address;

// ─── Constants ────────────────────────────────────────────────────────────────

const POLL_INTERVAL_SECS: u64 = 30;
/// Tối đa số watch entries mỗi API key.
const MAX_WATCHES_PER_KEY: usize = 20;
/// Tối đa tổng số watch entries.
const MAX_TOTAL_WATCHES: usize = 500;

// ─── WatchEntry ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchEntry {
    pub id:               String,
    pub address:          String,
    pub callback_url:     String,
    /// API key ID (8 ký tự đầu) của người đăng ký.
    pub api_key_id:       String,
    pub created_at:       u64,
    /// Block height của TX cuối cùng đã notify.
    /// 0 = chưa từng thấy TX nào → sẽ notify từ block hiện tại trở đi.
    pub last_seen_height: u64,
}

impl WatchEntry {
    pub fn new(
        address:      impl Into<String>,
        callback_url: impl Into<String>,
        api_key_id:   impl Into<String>,
        start_height: u64,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let address = address.into();
        let id = {
            let h = blake3::hash(format!("{address}{now}").as_bytes());
            hex::encode(&h.as_bytes()[..8])
        };
        WatchEntry {
            id,
            address,
            callback_url: callback_url.into(),
            api_key_id:   api_key_id.into(),
            created_at:   now,
            last_seen_height: start_height,
        }
    }
}

// ─── WatchRegistry ────────────────────────────────────────────────────────────

pub struct WatchRegistry {
    /// watch_id → entry
    watches: HashMap<String, WatchEntry>,
}

impl WatchRegistry {
    pub fn new() -> Self {
        WatchRegistry { watches: HashMap::new() }
    }

    /// Thêm watch. Trả về `Err` nếu vượt quá giới hạn hoặc đã có duplicate.
    pub fn add(&mut self, entry: WatchEntry) -> Result<String, String> {
        if self.watches.len() >= MAX_TOTAL_WATCHES {
            return Err(format!("max {} total watches reached", MAX_TOTAL_WATCHES));
        }
        let per_key = self.by_key(&entry.api_key_id).len();
        if per_key >= MAX_WATCHES_PER_KEY {
            return Err(format!("max {} watches per API key", MAX_WATCHES_PER_KEY));
        }
        // Không cho duplicate (cùng address + api_key_id)
        let dup = self.watches.values()
            .any(|w| w.address == entry.address && w.api_key_id == entry.api_key_id);
        if dup {
            return Err("already watching this address with this API key".into());
        }
        let id = entry.id.clone();
        self.watches.insert(id.clone(), entry);
        Ok(id)
    }

    /// Xóa watch. Chỉ owner (api_key_id khớp) được xóa.
    pub fn remove(&mut self, watch_id: &str, api_key_id: &str) -> Result<(), String> {
        match self.watches.get(watch_id) {
            None => Err("watch not found".into()),
            Some(w) if w.api_key_id != api_key_id => Err("not the watch owner".into()),
            Some(_) => { self.watches.remove(watch_id); Ok(()) }
        }
    }

    /// Tất cả watches của một API key.
    pub fn by_key(&self, api_key_id: &str) -> Vec<&WatchEntry> {
        self.watches.values()
            .filter(|w| w.api_key_id == api_key_id)
            .collect()
    }

    /// Snapshot tất cả entries để poller xử lý mà không giữ lock lâu.
    pub fn snapshot(&self) -> Vec<WatchEntry> {
        self.watches.values().cloned().collect()
    }

    /// Cập nhật last_seen_height sau khi notify thành công.
    pub fn update_height(&mut self, watch_id: &str, height: u64) {
        if let Some(w) = self.watches.get_mut(watch_id) {
            if height > w.last_seen_height {
                w.last_seen_height = height;
            }
        }
    }

    pub fn count(&self) -> usize {
        self.watches.len()
    }

    pub fn get(&self, watch_id: &str) -> Option<&WatchEntry> {
        self.watches.get(watch_id)
    }
}

// ─── Poller ───────────────────────────────────────────────────────────────────

/// Kiểm tra một watch entry — trả về danh sách (tx_id, amount, height) mới.
/// "Mới" = block_height > last_seen_height.
pub fn check_new_activity(
    entry:  &WatchEntry,
    chain:  &crate::chain::Blockchain,
) -> Vec<(String, u64, u64)> {
    let hist = history_for_addr(&entry.address, &chain.chain, &chain.utxo_set);
    hist.into_iter()
        .filter(|r| r.block_height > entry.last_seen_height)
        .map(|r| (r.tx_id, r.amount, r.block_height))
        .collect()
}

/// Payload gửi đến callback URL.
pub fn build_callback_payload(
    watch_id: &str,
    address:  &str,
    tx_id:    &str,
    amount:   u64,
    height:   u64,
) -> serde_json::Value {
    json!({
        "event":        "address_activity",
        "watch_id":     watch_id,
        "address":      address,
        "tx_id":        tx_id,
        "amount":       amount,
        "block_height": height,
        "timestamp":    SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
    })
}

/// Gửi HTTP POST callback (fire-and-forget). Timeout 10 giây.
pub async fn deliver_callback(entry: &WatchEntry, payload: &serde_json::Value) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    let body = payload.to_string();
    if let Err(e) = client
        .post(&entry.callback_url)
        .header("Content-Type", "application/json")
        .header("X-Watch-ID", &entry.id)
        .body(body)
        .send()
        .await
    {
        tracing::warn!(watch_id = entry.id, url = entry.callback_url, error = %e,
            "address_watch: callback delivery failed");
    }
}

/// Spawn background poller. Chạy mỗi `interval_secs` giây.
/// Cần `tokio::runtime` active.
pub fn spawn_watcher(db: WatchDb, chain: ScanDb, interval_secs: u64) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;

            // 1. Snapshot watches (giải phóng lock sớm)
            let watches = db.lock().await.snapshot();
            if watches.is_empty() { continue; }

            // 2. Xử lý từng watch (giữ chain lock ngắn mỗi entry)
            for entry in &watches {
                let new_activity = {
                    let bc = chain.lock().await;
                    check_new_activity(entry, &bc)
                };
                if new_activity.is_empty() { continue; }

                let max_height = new_activity.iter().map(|(_, _, h)| *h).max().unwrap_or(0);

                for (tx_id, amount, height) in &new_activity {
                    let payload = build_callback_payload(
                        &entry.id, &entry.address, tx_id, *amount, *height);
                    let entry_clone = entry.clone();
                    let payload_clone = payload.clone();
                    tokio::spawn(async move {
                        deliver_callback(&entry_clone, &payload_clone).await;
                    });
                }

                // Cập nhật last_seen_height
                db.lock().await.update_height(&entry.id, max_height);
            }
        }
    });
}

// ─── State ────────────────────────────────────────────────────────────────────

pub type WatchDb = Arc<Mutex<WatchRegistry>>;

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

// ─── Request / Response ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RegisterWatchRequest {
    address:      String,
    callback_url: String,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// POST /api/watch — write role required.
async fn post_watch(
    State((db, chain)): State<(WatchDb, ScanDb)>,
    req: Request,
) -> axum::response::Response {
    match get_role(&req) {
        Some(r) if r.can_write() => {}
        _ => return err_resp(StatusCode::FORBIDDEN, "write role required"),
    }
    let key_id = get_key_id(&req);

    let body = match to_bytes(req.into_body(), 4 * 1024).await {
        Ok(b)  => b,
        Err(_) => return err_resp(StatusCode::BAD_REQUEST, "cannot read body"),
    };
    let r: RegisterWatchRequest = match serde_json::from_slice(&body) {
        Ok(v)  => v,
        Err(e) => return err_resp(StatusCode::BAD_REQUEST, &format!("invalid JSON: {e}")),
    };

    // Validate address
    if let Err(e) = validate_address(&r.address) {
        return err_resp(StatusCode::BAD_REQUEST, e);
    }

    // Validate callback_url (basic: must start with http)
    if !r.callback_url.starts_with("http://") && !r.callback_url.starts_with("https://") {
        return err_resp(StatusCode::BAD_REQUEST, "callback_url must be http or https");
    }

    // start_height = chain tip so watches only fire on NEW activity
    let start_height = {
        let bc = chain.lock().await;
        bc.chain.len().saturating_sub(1) as u64
    };

    let entry = WatchEntry::new(&r.address, &r.callback_url, &key_id, start_height);
    let mut reg = db.lock().await;
    match reg.add(entry.clone()) {
        Ok(id) => {
            tracing::info!(id = id, address = r.address, key_id = key_id,
                "address_watch: registered");
            (StatusCode::CREATED, Json(json!({
                "status":       "watching",
                "id":           id,
                "address":      r.address,
                "callback_url": r.callback_url,
                "from_height":  start_height,
            }))).into_response()
        }
        Err(e) => err_resp(StatusCode::CONFLICT, &e),
    }
}

/// GET /api/watch — write role; liệt kê watches của API key.
async fn get_watches(
    State((db, _chain)): State<(WatchDb, ScanDb)>,
    req: Request,
) -> axum::response::Response {
    match get_role(&req) {
        Some(r) if r.can_write() => {}
        _ => return err_resp(StatusCode::FORBIDDEN, "write role required"),
    }
    let key_id = get_key_id(&req);
    let reg    = db.lock().await;
    let list: Vec<_> = reg.by_key(&key_id).into_iter().map(|w| json!({
        "id":               w.id,
        "address":          w.address,
        "callback_url":     w.callback_url,
        "created_at":       w.created_at,
        "last_seen_height": w.last_seen_height,
    })).collect();
    Json(json!({ "count": list.len(), "watches": list })).into_response()
}

/// DELETE /api/watch/:id — write role; chỉ owner được xóa.
async fn delete_watch(
    Path(watch_id):         Path<String>,
    State((db, _chain)):    State<(WatchDb, ScanDb)>,
    req: Request,
) -> axum::response::Response {
    match get_role(&req) {
        Some(r) if r.can_write() => {}
        _ => return err_resp(StatusCode::FORBIDDEN, "write role required"),
    }
    let key_id = get_key_id(&req);
    let mut reg = db.lock().await;
    match reg.remove(&watch_id, &key_id) {
        Ok(()) => Json(json!({ "status": "removed", "id": watch_id })).into_response(),
        Err(e) => {
            let status = if e.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::FORBIDDEN
            };
            err_resp(status, &e)
        }
    }
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn watch_router(db: WatchDb, chain: ScanDb) -> Router {
    let state = (db, chain);
    Router::new()
        .route("/api/watch",     get(get_watches).post(post_watch))
        .route("/api/watch/:id", axum::routing::delete(delete_watch))
        .with_state(state)
}

pub fn open_default() -> WatchDb {
    Arc::new(Mutex::new(WatchRegistry::new()))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(addr: &str, key: &str) -> WatchEntry {
        WatchEntry::new(addr, "https://example.com/hook", key, 0)
    }

    // ── WatchEntry ────────────────────────────────────────────────────────────

    #[test]
    fn test_watch_entry_id_non_empty() {
        let e = make_entry(&"a".repeat(40), "key1");
        assert!(!e.id.is_empty());
        assert_eq!(e.id.len(), 16); // 8 bytes → 16 hex
    }

    #[test]
    fn test_watch_entry_id_unique() {
        // Different addresses → different IDs
        let e1 = make_entry(&"a".repeat(40), "key1");
        let e2 = make_entry(&"b".repeat(40), "key1");
        assert_ne!(e1.id, e2.id);
    }

    #[test]
    fn test_watch_entry_fields() {
        let e = make_entry(&"c".repeat(40), "mykey");
        assert_eq!(e.address, "c".repeat(40));
        assert_eq!(e.callback_url, "https://example.com/hook");
        assert_eq!(e.api_key_id, "mykey");
        assert_eq!(e.last_seen_height, 0);
    }

    // ── WatchRegistry ─────────────────────────────────────────────────────────

    #[test]
    fn test_registry_add_ok() {
        let mut reg = WatchRegistry::new();
        let e   = make_entry(&"d".repeat(40), "key1");
        assert!(reg.add(e).is_ok());
        assert_eq!(reg.count(), 1);
    }

    #[test]
    fn test_registry_add_duplicate_rejected() {
        let mut reg = WatchRegistry::new();
        let addr    = "e".repeat(40);
        reg.add(make_entry(&addr, "key1")).unwrap();
        let err = reg.add(make_entry(&addr, "key1")).unwrap_err();
        assert!(err.contains("already watching"));
    }

    #[test]
    fn test_registry_add_different_key_same_addr_allowed() {
        let mut reg = WatchRegistry::new();
        let addr    = "f".repeat(40);
        reg.add(make_entry(&addr, "key1")).unwrap();
        // Different API key can watch same address
        assert!(reg.add(make_entry(&addr, "key2")).is_ok());
    }

    #[test]
    fn test_registry_remove_owner_ok() {
        let mut reg = WatchRegistry::new();
        let e   = make_entry(&"g".repeat(40), "key1");
        let id  = reg.add(e).unwrap();
        assert!(reg.remove(&id, "key1").is_ok());
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn test_registry_remove_non_owner_rejected() {
        let mut reg = WatchRegistry::new();
        let e   = make_entry(&"h".repeat(40), "key1");
        let id  = reg.add(e).unwrap();
        let err = reg.remove(&id, "key2").unwrap_err();
        assert!(err.contains("not the watch owner"));
    }

    #[test]
    fn test_registry_remove_nonexistent() {
        let mut reg = WatchRegistry::new();
        let err = reg.remove("nope", "key1").unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn test_registry_by_key_filters_correctly() {
        let mut reg = WatchRegistry::new();
        reg.add(make_entry(&"i".repeat(40), "key1")).unwrap();
        reg.add(make_entry(&"j".repeat(40), "key1")).unwrap();
        reg.add(make_entry(&"k".repeat(40), "key2")).unwrap();
        assert_eq!(reg.by_key("key1").len(), 2);
        assert_eq!(reg.by_key("key2").len(), 1);
        assert_eq!(reg.by_key("key3").len(), 0);
    }

    #[test]
    fn test_registry_snapshot_returns_all() {
        let mut reg = WatchRegistry::new();
        reg.add(make_entry(&"l".repeat(40), "key1")).unwrap();
        reg.add(make_entry(&"m".repeat(40), "key2")).unwrap();
        assert_eq!(reg.snapshot().len(), 2);
    }

    #[test]
    fn test_registry_update_height() {
        let mut reg = WatchRegistry::new();
        let e   = make_entry(&"n".repeat(40), "key1");
        let id  = reg.add(e).unwrap();
        reg.update_height(&id, 100);
        assert_eq!(reg.get(&id).unwrap().last_seen_height, 100);
    }

    #[test]
    fn test_registry_update_height_no_regression() {
        let mut reg = WatchRegistry::new();
        let e  = make_entry(&"o".repeat(40), "key1");
        let id = reg.add(e).unwrap();
        reg.update_height(&id, 100);
        reg.update_height(&id, 50); // lower than current — should not regress
        assert_eq!(reg.get(&id).unwrap().last_seen_height, 100);
    }

    // ── build_callback_payload ────────────────────────────────────────────────

    #[test]
    fn test_build_callback_payload_fields() {
        let p = build_callback_payload("w123", "addr1", "tx1", 999, 42);
        assert_eq!(p["event"],        "address_activity");
        assert_eq!(p["watch_id"],     "w123");
        assert_eq!(p["address"],      "addr1");
        assert_eq!(p["tx_id"],        "tx1");
        assert_eq!(p["amount"],       999);
        assert_eq!(p["block_height"], 42);
        assert!(p["timestamp"].as_u64().unwrap() > 0);
    }

    // ── check_new_activity ────────────────────────────────────────────────────

    #[test]
    fn test_check_new_activity_empty_chain() {
        let bc    = crate::chain::Blockchain::new();
        let entry = WatchEntry::new(&"a".repeat(40), "http://cb", "key", 0);
        let acts  = check_new_activity(&entry, &bc);
        assert!(acts.is_empty());
    }

    #[test]
    fn test_check_new_activity_filters_by_height() {
        let bc    = crate::chain::Blockchain::new();
        // last_seen_height = u64::MAX → tất cả TX cũ đều bị lọc
        let entry = WatchEntry {
            id:               "x".into(),
            address:          "a".repeat(40),
            callback_url:     "http://cb".into(),
            api_key_id:       "key".into(),
            created_at:       0,
            last_seen_height: u64::MAX,
        };
        let acts = check_new_activity(&entry, &bc);
        assert!(acts.is_empty());
    }
}
