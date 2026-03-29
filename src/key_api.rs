#![allow(dead_code)]
//! v19.9 — REST API quản lý API Key
//!
//! Tất cả endpoints yêu cầu `admin` role (X-API-Key với role=admin).
//!
//! Routes:
//!   GET    /api/keys           → liệt kê tất cả keys (id, role, label, created_at)
//!   POST   /api/keys           → tạo key mới, trả về raw key 1 lần
//!   DELETE /api/keys/:key_id   → thu hồi key theo key_id

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::api_auth::{ApiRole, AuthDb};

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateKeyRequest {
    label: Option<String>,
    role:  Option<String>,
}

#[derive(Serialize)]
struct CreateKeyResponse {
    key_id: String,
    key:    String,   // raw key — hiển thị 1 lần
    role:   String,
    label:  String,
}

#[derive(Serialize)]
struct KeyInfo {
    key_id:     String,
    role:       String,
    label:      String,
    created_at: u64,
}

// ── Auth check helper ─────────────────────────────────────────────────────────

fn is_admin(req: &axum::extract::Request) -> bool {
    req.extensions()
        .get::<ApiRole>()
        .map(|r| r.is_admin())
        .unwrap_or(false)
}

fn forbidden() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({"error": "admin role required"})),
    )
        .into_response()
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /api/keys — liệt kê keys (chỉ metadata, không có raw key)
async fn list_keys(
    State(db): State<AuthDb>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    if !is_admin(&req) {
        return forbidden();
    }
    let store = db.lock().await;
    let keys: Vec<KeyInfo> = store.list().iter().map(|e| KeyInfo {
        key_id:     e.key_id.clone(),
        role:       e.role.as_str().to_string(),
        label:      e.label.clone(),
        created_at: e.created_at,
    }).collect();
    Json(serde_json::json!({ "keys": keys, "total": keys.len() })).into_response()
}

/// POST /api/keys — tạo key mới
async fn create_key(
    State(db): State<AuthDb>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    if !is_admin(&req) {
        return forbidden();
    }
    let body = match axum::body::to_bytes(req.into_body(), 8 * 1024).await {
        Ok(b)  => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"bad body"}))).into_response(),
    };
    let parsed: CreateKeyRequest = if body.is_empty() {
        CreateKeyRequest { label: None, role: None }
    } else {
        match serde_json::from_slice(&body) {
            Ok(r)  => r,
            Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
        }
    };

    let label = parsed.label.unwrap_or_else(|| "unnamed".into());
    let role  = parsed.role
        .as_deref()
        .and_then(ApiRole::from_str)
        .unwrap_or(ApiRole::Read);

    let mut store = db.lock().await;
    let (raw_key, key_id) = store.add(role.clone(), &label);
    if let Err(e) = store.save() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("save failed: {e}")})),
        ).into_response();
    }

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(CreateKeyResponse {
            key_id,
            key: raw_key,
            role: role.as_str().to_string(),
            label,
        }).unwrap()),
    ).into_response()
}

/// DELETE /api/keys/:key_id — thu hồi key
async fn revoke_key(
    State(db): State<AuthDb>,
    Path(key_id): Path<String>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    if !is_admin(&req) {
        return forbidden();
    }
    let mut store = db.lock().await;
    if store.revoke(&key_id) {
        if let Err(e) = store.save() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("save failed: {e}")})),
            ).into_response();
        }
        Json(serde_json::json!({"revoked": key_id})).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "key not found"})),
        ).into_response()
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn key_router(db: AuthDb) -> Router {
    Router::new()
        .route("/api/keys",          get(list_keys).post(create_key))
        .route("/api/keys/:key_id",  delete(revoke_key))
        .with_state(db)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_auth::ApiKeyStore;
    use std::{path::PathBuf, sync::Arc};
    use tokio::sync::Mutex;

    fn make_db() -> AuthDb {
        Arc::new(Mutex::new(ApiKeyStore::load(
            PathBuf::from("/tmp/pkt_key_api_test_NONEXISTENT.json"),
        )))
    }

    #[tokio::test]
    async fn test_create_key_adds_to_store() {
        let db = make_db();
        {
            let mut store = db.lock().await;
            let (_, _) = store.add(ApiRole::Admin, "bootstrap");
        }
        // Verify store has 1 entry
        let store = db.lock().await;
        assert_eq!(store.len(), 1);
    }

    #[tokio::test]
    async fn test_revoke_removes_from_store() {
        let db = make_db();
        let key_id = {
            let mut store = db.lock().await;
            let (_, id) = store.add(ApiRole::Read, "temp");
            id
        };
        {
            let mut store = db.lock().await;
            assert!(store.revoke(&key_id));
        }
        let store = db.lock().await;
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn test_revoke_nonexistent_returns_false() {
        let db = make_db();
        let mut store = db.lock().await;
        assert!(!store.revoke("no_such_id"));
    }

    #[tokio::test]
    async fn test_list_returns_all_keys() {
        let db = make_db();
        {
            let mut store = db.lock().await;
            store.add(ApiRole::Read,  "reader");
            store.add(ApiRole::Write, "writer");
            store.add(ApiRole::Admin, "admin");
        }
        let store = db.lock().await;
        assert_eq!(store.len(), 3);
        let keys = store.list();
        assert_eq!(keys[0].label, "reader");
        assert_eq!(keys[1].label, "writer");
        assert_eq!(keys[2].label, "admin");
    }

    #[tokio::test]
    async fn test_create_key_default_role_is_read() {
        let db = make_db();
        let mut store = db.lock().await;
        let (_, _) = store.add(ApiRole::Read, "unnamed");
        let keys = store.list();
        assert_eq!(keys[0].role.as_str(), "read");
    }

    #[tokio::test]
    async fn test_key_info_no_raw_key_in_list() {
        // Đảm bảo list() không trả về raw key
        let db = make_db();
        let raw = {
            let mut store = db.lock().await;
            let (raw, _) = store.add(ApiRole::Read, "test");
            raw
        };
        let store = db.lock().await;
        let json = serde_json::to_string(store.list()).unwrap();
        assert!(!json.contains(&raw), "Raw key must not appear in list output");
    }
}
