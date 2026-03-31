#![allow(dead_code)]
//! v10.9 — Outbound HTTP Webhook
//!
//! Gửi HTTP POST đến URL đã đăng ký khi có sự kiện:
//!   new_block       — mỗi block mới được thêm vào chain
//!   new_tx          — mỗi transaction vào mempool
//!   address_activity — khi 1 địa chỉ cụ thể có activity
//!
//! Security:
//!   - HMAC-SHA256 signature trong header `X-PKT-Signature`
//!   - Subscriber tự verify signature với secret nhận được khi đăng ký
//!   - Quản lý webhook (register/list/delete) yêu cầu `write` API key role
//!
//! REST API (yêu cầu X-API-Key với write role):
//!   POST   /api/webhooks           → đăng ký
//!   GET    /api/webhooks           → danh sách
//!   DELETE /api/webhooks/:id       → xoá

use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, post},
    Router,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

// ─── Event types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventType {
    NewBlock,
    NewTx,
    AddressActivity,
}

impl WebhookEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NewBlock        => "new_block",
            Self::NewTx           => "new_tx",
            Self::AddressActivity => "address_activity",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "new_block"        => Some(Self::NewBlock),
            "new_tx"           => Some(Self::NewTx),
            "address_activity" => Some(Self::AddressActivity),
            _                  => None,
        }
    }
}

// ─── Subscription ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSubscription {
    pub id:             String,
    pub url:            String,
    pub events:         Vec<WebhookEventType>,
    /// Chỉ gửi AddressActivity event cho địa chỉ này (None = tất cả)
    pub address_filter: Option<String>,
    pub created_at:     u64,
    pub active:         bool,
    /// Plaintext HMAC secret — chỉ hiển thị 1 lần khi register
    #[serde(skip_serializing)]  // không trả về trong list endpoint
    pub secret:         String,
}

// ─── Payload ──────────────────────────────────────────────────────────────────

/// Body gửi đến subscriber URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event:     String,
    pub timestamp: u64,
    pub data:      serde_json::Value,
}

impl WebhookPayload {
    pub fn new_block(height: u64, hash: &str, tx_count: usize, timestamp: u64) -> Self {
        WebhookPayload {
            event: "new_block".into(),
            timestamp: unix_now(),
            data: serde_json::json!({
                "height":   height,
                "hash":     hash,
                "tx_count": tx_count,
                "block_ts": timestamp,
            }),
        }
    }

    pub fn new_tx(tx_id: &str, fee: u64) -> Self {
        WebhookPayload {
            event: "new_tx".into(),
            timestamp: unix_now(),
            data: serde_json::json!({ "tx_id": tx_id, "fee": fee }),
        }
    }

    pub fn address_activity(address: &str, tx_id: &str, amount: u64) -> Self {
        WebhookPayload {
            event: "address_activity".into(),
            timestamp: unix_now(),
            data: serde_json::json!({
                "address": address,
                "tx_id":   tx_id,
                "amount":  amount,
            }),
        }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ─── HMAC signing ─────────────────────────────────────────────────────────────

/// Compute HMAC-SHA256(secret, body) → hex string.
/// Subscriber verifies: HMAC(secret, body) == X-PKT-Signature header.
pub fn sign_payload(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

// ─── Registry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct WebhookRegistry {
    pub subscriptions: Vec<WebhookSubscription>,
}

impl WebhookRegistry {
    pub fn new() -> Self { Self::default() }

    /// Đăng ký webhook mới. Trả về `(id, secret)`.
    pub fn register(
        &mut self,
        url: impl Into<String>,
        events: Vec<WebhookEventType>,
        address_filter: Option<String>,
    ) -> (String, String) {
        let url    = url.into();
        let id     = gen_id(&url, self.subscriptions.len());
        let secret = gen_secret(&id);
        self.subscriptions.push(WebhookSubscription {
            id: id.clone(),
            url,
            events,
            address_filter,
            created_at: unix_now(),
            active: true,
            secret: secret.clone(),
        });
        (id, secret)
    }

    /// Xoá subscription theo id. Trả về `true` nếu tìm thấy và xoá.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.subscriptions.len();
        self.subscriptions.retain(|s| s.id != id);
        self.subscriptions.len() < before
    }

    /// Lấy tất cả subscriptions đang active cho event type này.
    pub fn matching(&self, event_type: &WebhookEventType) -> Vec<&WebhookSubscription> {
        self.subscriptions.iter()
            .filter(|s| s.active && s.events.contains(event_type))
            .collect()
    }

    /// Số subscriptions đang active.
    pub fn active_count(&self) -> usize {
        self.subscriptions.iter().filter(|s| s.active).count()
    }
}

fn gen_id(url: &str, idx: usize) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"webhook_id_v109");
    h.update(url.as_bytes());
    h.update(&idx.to_le_bytes());
    h.update(&unix_now().to_le_bytes());
    hex::encode(&h.finalize().as_bytes()[..8])
}

fn gen_secret(id: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"webhook_secret_v109");
    h.update(id.as_bytes());
    h.update(&unix_now().to_le_bytes());
    hex::encode(h.finalize().as_bytes())
}

// ─── Delivery ─────────────────────────────────────────────────────────────────

/// Gửi payload đến 1 subscriber — fire-and-forget, không retry trong scope này.
/// Returns HTTP status code hoặc Err nếu network fail.
pub async fn deliver(
    sub: &WebhookSubscription,
    payload: &WebhookPayload,
) -> Result<u16, String> {
    let body = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    let sig  = sign_payload(&sub.secret, &body);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(&sub.url)
        .header("Content-Type", "application/json")
        .header("X-PKT-Signature", &sig)
        .header("X-PKT-Event", payload.event.as_str())
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(resp.status().as_u16())
}

/// Gửi đến tất cả subscribers khớp event_type — spawn tokio task per subscriber.
pub fn broadcast(
    registry: &WebhookRegistry,
    event_type: &WebhookEventType,
    payload: WebhookPayload,
) {
    for sub in registry.matching(event_type) {
        let sub     = sub.clone();
        let payload = payload.clone();
        tokio::spawn(async move {
            if let Err(e) = deliver(&sub, &payload).await {
                tracing::warn!("Webhook delivery failed for {}: {}", sub.url, e);
            }
        });
    }
}

// ─── Axum REST API ────────────────────────────────────────────────────────────

pub type WebhookDb = Arc<Mutex<WebhookRegistry>>;

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    url:            String,
    events:         Vec<String>,
    address_filter: Option<String>,
}

#[derive(Serialize)]
struct RegisterResponse {
    id:     String,
    secret: String,
    url:    String,
    events: Vec<String>,
}

#[derive(Serialize)]
struct SubInfo {
    id:             String,
    url:            String,
    events:         Vec<String>,
    address_filter: Option<String>,
    active:         bool,
    created_at:     u64,
}

fn requires_write(req: &axum::extract::Request) -> bool {
    use crate::api_auth::ApiRole;
    // Auth middleware attaches ApiRole as extension — check can_write()
    req.extensions()
        .get::<ApiRole>()
        .map(|r| r.can_write())
        .unwrap_or(false)
}

/// POST /api/webhooks — register (requires write role)
async fn register_webhook(
    State(db): State<WebhookDb>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    if !requires_write(&req) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"write role required"}))).into_response();
    }
    // Extract body manually
    let body = match axum::body::to_bytes(req.into_body(), 64 * 1024).await {
        Ok(b)  => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"bad body"}))).into_response(),
    };
    let parsed: RegisterRequest = match serde_json::from_slice(&body) {
        Ok(r)  => r,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    if parsed.url.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"url required"}))).into_response();
    }
    let events: Vec<WebhookEventType> = parsed.events.iter()
        .filter_map(|s| WebhookEventType::from_str(s))
        .collect();
    if events.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"at least one valid event required"}))).into_response();
    }

    let mut reg = db.lock().await;
    let (id, secret) = reg.register(parsed.url.clone(), events.clone(), parsed.address_filter);
    let resp = RegisterResponse {
        id, secret,
        url:    parsed.url,
        events: events.iter().map(|e| e.as_str().to_string()).collect(),
    };
    (StatusCode::CREATED, Json(serde_json::to_value(resp).unwrap())).into_response()
}

/// GET /api/webhooks — list (requires write role)
async fn list_webhooks(
    State(db): State<WebhookDb>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    if !requires_write(&req) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"write role required"}))).into_response();
    }
    let reg = db.lock().await;
    let list: Vec<SubInfo> = reg.subscriptions.iter().map(|s| SubInfo {
        id:             s.id.clone(),
        url:            s.url.clone(),
        events:         s.events.iter().map(|e| e.as_str().to_string()).collect(),
        address_filter: s.address_filter.clone(),
        active:         s.active,
        created_at:     s.created_at,
    }).collect();
    Json(serde_json::json!({"webhooks": list})).into_response()
}

/// DELETE /api/webhooks/:id — remove (requires write role)
async fn delete_webhook(
    State(db): State<WebhookDb>,
    Path(id): Path<String>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    if !requires_write(&req) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"write role required"}))).into_response();
    }
    let mut reg = db.lock().await;
    if reg.remove(&id) {
        Json(serde_json::json!({"deleted": id})).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"not found"}))).into_response()
    }
}

pub fn webhook_router(db: WebhookDb, auth: crate::api_auth::AuthDb) -> Router {
    Router::new()
        .route("/api/webhooks",     post(register_webhook).get(list_webhooks))
        .route("/api/webhooks/:id", delete(delete_webhook))
        .layer(axum::middleware::from_fn_with_state(
            auth,
            crate::api_auth::require_write_middleware,
        ))
        .with_state(db)
}

pub fn open_default() -> WebhookDb {
    Arc::new(Mutex::new(WebhookRegistry::new()))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_returns_id_and_secret() {
        let mut reg = WebhookRegistry::new();
        let (id, secret) = reg.register("http://example.com/hook", vec![WebhookEventType::NewBlock], None);
        assert!(!id.is_empty());
        assert_eq!(id.len(), 16);      // 8 bytes hex = 16 chars
        assert_eq!(secret.len(), 64);  // 32 bytes hex = 64 chars
    }

    #[test]
    fn test_register_stores_subscription() {
        let mut reg = WebhookRegistry::new();
        reg.register("http://a.com/hook", vec![WebhookEventType::NewBlock], None);
        assert_eq!(reg.subscriptions.len(), 1);
        assert_eq!(reg.subscriptions[0].url, "http://a.com/hook");
        assert!(reg.subscriptions[0].active);
    }

    #[test]
    fn test_remove_existing() {
        let mut reg = WebhookRegistry::new();
        let (id, _) = reg.register("http://a.com", vec![WebhookEventType::NewTx], None);
        assert!(reg.remove(&id));
        assert!(reg.subscriptions.is_empty());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut reg = WebhookRegistry::new();
        assert!(!reg.remove("deadbeef"));
    }

    #[test]
    fn test_matching_event_type() {
        let mut reg = WebhookRegistry::new();
        reg.register("http://a.com", vec![WebhookEventType::NewBlock], None);
        reg.register("http://b.com", vec![WebhookEventType::NewTx], None);
        assert_eq!(reg.matching(&WebhookEventType::NewBlock).len(), 1);
        assert_eq!(reg.matching(&WebhookEventType::NewTx).len(), 1);
        assert_eq!(reg.matching(&WebhookEventType::AddressActivity).len(), 0);
    }

    #[test]
    fn test_matching_multiple_events() {
        let mut reg = WebhookRegistry::new();
        reg.register("http://a.com",
            vec![WebhookEventType::NewBlock, WebhookEventType::NewTx], None);
        assert_eq!(reg.matching(&WebhookEventType::NewBlock).len(), 1);
        assert_eq!(reg.matching(&WebhookEventType::NewTx).len(), 1);
    }

    #[test]
    fn test_active_count() {
        let mut reg = WebhookRegistry::new();
        reg.register("http://a.com", vec![WebhookEventType::NewBlock], None);
        reg.register("http://b.com", vec![WebhookEventType::NewTx], None);
        assert_eq!(reg.active_count(), 2);
    }

    #[test]
    fn test_sign_payload_deterministic() {
        let sig1 = sign_payload("secret", b"hello");
        let sig2 = sign_payload("secret", b"hello");
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.len(), 64); // SHA-256 → 32 bytes → 64 hex chars
    }

    #[test]
    fn test_sign_payload_different_secret() {
        let sig1 = sign_payload("secret1", b"hello");
        let sig2 = sign_payload("secret2", b"hello");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_sign_payload_different_body() {
        let sig1 = sign_payload("secret", b"hello");
        let sig2 = sign_payload("secret", b"world");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_webhook_event_type_roundtrip() {
        for (s, e) in [
            ("new_block", WebhookEventType::NewBlock),
            ("new_tx", WebhookEventType::NewTx),
            ("address_activity", WebhookEventType::AddressActivity),
        ] {
            assert_eq!(e.as_str(), s);
            assert_eq!(WebhookEventType::from_str(s), Some(e));
        }
    }

    #[test]
    fn test_webhook_event_type_unknown() {
        assert!(WebhookEventType::from_str("unknown").is_none());
    }

    #[test]
    fn test_payload_new_block_serializes() {
        let p = WebhookPayload::new_block(5, "abc123", 3, 1_000_000);
        assert_eq!(p.event, "new_block");
        assert_eq!(p.data["height"], 5);
        assert_eq!(p.data["tx_count"], 3);
    }

    #[test]
    fn test_payload_new_tx_serializes() {
        let p = WebhookPayload::new_tx("txabc", 1000);
        assert_eq!(p.event, "new_tx");
        assert_eq!(p.data["tx_id"], "txabc");
        assert_eq!(p.data["fee"], 1000);
    }

    #[test]
    fn test_payload_address_activity_serializes() {
        let p = WebhookPayload::address_activity("addr1", "tx1", 5000);
        assert_eq!(p.event, "address_activity");
        assert_eq!(p.data["address"], "addr1");
        assert_eq!(p.data["amount"], 5000);
    }

    #[test]
    fn test_subscription_secret_not_serialized() {
        let mut reg = WebhookRegistry::new();
        reg.register("http://a.com", vec![WebhookEventType::NewBlock], None);
        let sub = &reg.subscriptions[0];
        let json = serde_json::to_value(sub).unwrap();
        assert!(json.get("secret").is_none(), "Secret must not appear in JSON output");
    }

    #[test]
    fn test_address_filter_stored() {
        let mut reg = WebhookRegistry::new();
        reg.register("http://a.com",
            vec![WebhookEventType::AddressActivity],
            Some("addr1".into()));
        assert_eq!(reg.subscriptions[0].address_filter, Some("addr1".into()));
    }

    #[test]
    fn test_two_registrations_get_different_ids() {
        let mut reg = WebhookRegistry::new();
        let (id1, _) = reg.register("http://a.com", vec![WebhookEventType::NewBlock], None);
        let (id2, _) = reg.register("http://b.com", vec![WebhookEventType::NewBlock], None);
        assert_ne!(id1, id2);
    }
}
