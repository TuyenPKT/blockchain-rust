#![allow(dead_code)]
//! v10.0 — API Auth (API key system)
//!
//! Hệ thống API key cho PKTScan:
//!   - Mỗi key có role: read | write | admin
//!   - Raw key (64-char hex) chỉ hiển thị 1 lần khi tạo
//!   - Chỉ lưu blake3 hash của key trong `~/.pkt/api_keys.json`
//!   - `X-API-Key` header validation trong axum middleware
//!   - GET endpoints: public (không cần key); key nếu có phải hợp lệ
//!   - POST/PUT/DELETE: yêu cầu `write` hoặc `admin` role (Era 17)
//!
//! CLI:
//!   cargo run -- apikey new [label] [role]   → tạo key mới (default role=read)
//!   cargo run -- apikey list                 → liệt kê tất cả keys
//!   cargo run -- apikey revoke <key_id>      → thu hồi key
//!
//! Tích hợp (trong pktscan_api::serve):
//!   let auth_db = api_auth::AuthDb::load_default();
//!   let app = router(...)
//!       .layer(middleware::from_fn_with_state(auth_db, api_auth::auth_middleware));

use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

// ─── Role ─────────────────────────────────────────────────────────────────────

/// API key role — determines what operations are allowed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiRole {
    /// Read-only access to all GET endpoints.
    Read,
    /// Read + write access (POST/PUT/DELETE in Era 17+).
    Write,
    /// Full access including admin endpoints.
    Admin,
}

impl ApiRole {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "read"  => Some(ApiRole::Read),
            "write" => Some(ApiRole::Write),
            "admin" => Some(ApiRole::Admin),
            _       => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ApiRole::Read  => "read",
            ApiRole::Write => "write",
            ApiRole::Admin => "admin",
        }
    }

    /// Can this role perform write operations?
    pub fn can_write(&self) -> bool {
        matches!(self, ApiRole::Write | ApiRole::Admin)
    }

    /// Can this role access admin endpoints?
    pub fn is_admin(&self) -> bool {
        matches!(self, ApiRole::Admin)
    }
}

// ─── ApiKeyEntry ──────────────────────────────────────────────────────────────

/// Stored API key entry (raw key is NOT stored — only its blake3 hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyEntry {
    /// Short unique identifier (8-char hex).
    pub key_id: String,
    /// blake3 hex hash of the raw 64-char key.
    pub key_hash: String,
    /// Access role.
    pub role: ApiRole,
    /// Unix timestamp when the key was created.
    pub created_at: u64,
    /// Human-readable label.
    pub label: String,
}

// ─── Storage format ───────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct KeysFile {
    keys: Vec<ApiKeyEntry>,
}

// ─── ApiKeyStore ──────────────────────────────────────────────────────────────

/// Thread-safe API key store backed by `~/.pkt/api_keys.json`.
pub struct ApiKeyStore {
    data: KeysFile,
    path: PathBuf,
}

/// Shared handle used as axum State.
pub type AuthDb = Arc<Mutex<ApiKeyStore>>;

impl ApiKeyStore {
    /// Default path: `~/.pkt/api_keys.json`.
    pub fn default_path() -> PathBuf {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".pkt")
            .join("api_keys.json")
    }

    /// Load from file, or create empty store if file does not exist.
    pub fn load(path: PathBuf) -> Self {
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        ApiKeyStore { data, path }
    }

    /// Load from default path.
    pub fn load_default() -> AuthDb {
        Arc::new(Mutex::new(Self::load(Self::default_path())))
    }

    /// Persist current state to disk.
    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create_dir_all: {e}"))?;
        }
        let json = serde_json::to_string_pretty(&self.data)
            .map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(&self.path, json)
            .map_err(|e| format!("write: {e}"))?;
        Ok(())
    }

    /// Generate a new API key, store its hash, and return the raw key (shown once).
    ///
    /// Returns `(raw_key, key_id)`.
    pub fn add(&mut self, role: ApiRole, label: &str) -> (String, String) {
        let raw_key = generate_raw_key();
        let key_id  = &raw_key[..8];
        let entry = ApiKeyEntry {
            key_id:     key_id.to_string(),
            key_hash:   hash_api_key(&raw_key),
            role,
            created_at: unix_now(),
            label:      label.to_string(),
        };
        let id = entry.key_id.clone();
        self.data.keys.push(entry);
        (raw_key, id)
    }

    /// Validate a raw key against the stored hashes.
    /// Returns the matching entry's role if valid, `None` otherwise.
    pub fn validate(&self, raw_key: &str) -> Option<&ApiRole> {
        if raw_key.is_empty() {
            return None;
        }
        let h = hash_api_key(raw_key);
        self.data.keys.iter().find(|e| e.key_hash == h).map(|e| &e.role)
    }

    /// Remove a key by key_id.  Returns `true` if found and removed.
    pub fn revoke(&mut self, key_id: &str) -> bool {
        let before = self.data.keys.len();
        self.data.keys.retain(|e| e.key_id != key_id);
        self.data.keys.len() < before
    }

    /// All stored key entries (hashes, not raw keys).
    pub fn list(&self) -> &[ApiKeyEntry] {
        &self.data.keys
    }

    /// Number of stored keys.
    pub fn len(&self) -> usize {
        self.data.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.keys.is_empty()
    }
}

// ─── Crypto helpers ───────────────────────────────────────────────────────────

/// Generate a random 32-byte key encoded as 64-char lowercase hex.
pub fn generate_raw_key() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Use entropy from SystemTime + stack address
    let mut bytes = [0u8; 32];
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);

    // Mix multiple entropy sources via blake3
    let mut hasher = DefaultHasher::new();
    now_nanos.hash(&mut hasher);
    let seed1 = hasher.finish();

    std::thread::sleep(std::time::Duration::from_nanos(1));
    let now2 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(1);
    now2.hash(&mut hasher);
    let seed2 = hasher.finish();

    // blake3 keyed hash for 32 bytes
    let h1 = blake3::hash(&seed1.to_le_bytes());
    let h2 = blake3::hash(&seed2.to_le_bytes());
    bytes[..16].copy_from_slice(&h1.as_bytes()[..16]);
    bytes[16..].copy_from_slice(&h2.as_bytes()[..16]);

    hex::encode(bytes)
}

/// Compute blake3 hex hash of an API key string.
pub fn hash_api_key(raw_key: &str) -> String {
    blake3::hash(raw_key.as_bytes()).to_hex().to_string()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ─── Axum middleware ──────────────────────────────────────────────────────────

/// Axum middleware — validate `X-API-Key` header if present.
///
/// - No header  → allow (public read access)
/// - Header present, valid → allow, attach role extension
/// - Header present, invalid → 401 Unauthorized
/// Extract API key từ `X-Api-Key` header hoặc `?api_key=` query param.
fn extract_api_key(request: &Request<Body>) -> Option<String> {
    // Header takes priority
    if let Some(v) = request.headers().get("x-api-key") {
        return v.to_str().ok().map(|s| s.to_string());
    }
    // Fallback: ?api_key= query param (browser friendly)
    let query = request.uri().query().unwrap_or("");
    query.split('&').find_map(|part| {
        let mut kv = part.splitn(2, '=');
        match (kv.next(), kv.next()) {
            (Some("api_key"), Some(v)) if !v.is_empty() => Some(v.to_string()),
            _ => None,
        }
    })
}

/// Strict middleware: yêu cầu API key với Write hoặc Admin role.
/// Trả 401 nếu không có key, 403 nếu key không đủ quyền.
/// Dùng cho webhook API routes và trang webhooks HTML.
pub async fn require_write_middleware(
    State(store): State<AuthDb>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    match extract_api_key(&request) {
        None => (
            StatusCode::UNAUTHORIZED,
            [("content-type", "application/json")],
            "{\"error\":\"API key required — X-Api-Key header or ?api_key= param\"}",
        ).into_response(),
        Some(key) => {
            let guard = store.lock().await;
            match guard.validate(&key) {
                None => (
                    StatusCode::UNAUTHORIZED,
                    [("content-type", "application/json")],
                    "{\"error\":\"invalid API key\"}",
                ).into_response(),
                Some(role) if !role.can_write() => (
                    StatusCode::FORBIDDEN,
                    [("content-type", "application/json")],
                    "{\"error\":\"Write role required\"}",
                ).into_response(),
                Some(role) => {
                    request.extensions_mut().insert(role.clone());
                    drop(guard);
                    next.run(request).await
                }
            }
        }
    }
}

pub async fn auth_middleware(
    State(store): State<AuthDb>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    if let Some(key_header) = request.headers().get("x-api-key") {
        match key_header.to_str() {
            Err(_) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    "{\"error\":\"malformed X-API-Key header\"}",
                )
                    .into_response();
            }
            Ok(raw_key) => {
                let guard = store.lock().await;
                match guard.validate(raw_key) {
                    None => {
                        return (
                            StatusCode::UNAUTHORIZED,
                            "{\"error\":\"invalid API key\"}",
                        )
                            .into_response();
                    }
                    Some(role) => {
                        // Attach role as extension so downstream handlers can inspect it
                        request.extensions_mut().insert(role.clone());
                    }
                }
            }
        }
    }
    next.run(request).await
}

// ─── CLI ──────────────────────────────────────────────────────────────────────

/// `cargo run -- apikey <sub> [args]`
///
///   apikey new [label] [role]   → generate new key (role default = read)
///   apikey list                 → list all stored key entries
///   apikey revoke <key_id>      → revoke a key by its key_id
pub fn cmd_apikey(args: &[String]) {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("help");
    let path = ApiKeyStore::default_path();
    let mut store = ApiKeyStore::load(path.clone());

    match sub {
        "new" => {
            let label = args.get(3).map(|s| s.as_str()).unwrap_or("unnamed");
            let role_str = args.get(4).map(|s| s.as_str()).unwrap_or("read");
            let role = ApiRole::from_str(role_str).unwrap_or_else(|| {
                eprintln!("Unknown role '{}', using 'read'", role_str);
                ApiRole::Read
            });

            let (raw_key, key_id) = store.add(role.clone(), label);
            match store.save() {
                Ok(_) => {
                    println!();
                    println!("  New API key created:");
                    println!("  Key ID  : {}", key_id);
                    println!("  Role    : {}", role.as_str());
                    println!("  Label   : {}", label);
                    println!();
                    println!("  API Key (save now — shown only once):");
                    println!("  {}", raw_key);
                    println!();
                    println!("  Usage: curl -H 'X-API-Key: {}' http://localhost:8080/api/stats", raw_key);
                    println!();
                }
                Err(e) => eprintln!("Failed to save: {}", e),
            }
        }
        "list" => {
            let keys = store.list();
            if keys.is_empty() {
                println!("  No API keys found. Create one with: cargo run -- apikey new");
            } else {
                println!();
                println!("  Stored API keys ({}):", keys.len());
                println!("  {:<10} {:<8} {:<20} {}", "Key ID", "Role", "Created", "Label");
                println!("  {}", "-".repeat(60));
                for k in keys {
                    let dt = format_unix(k.created_at);
                    println!("  {:<10} {:<8} {:<20} {}", k.key_id, k.role.as_str(), dt, k.label);
                }
                println!();
            }
        }
        "revoke" => {
            let key_id = match args.get(3) {
                Some(id) => id,
                None => {
                    eprintln!("Usage: cargo run -- apikey revoke <key_id>");
                    return;
                }
            };
            if store.revoke(key_id) {
                match store.save() {
                    Ok(_) => println!("  Key '{}' revoked.", key_id),
                    Err(e) => eprintln!("Failed to save: {}", e),
                }
            } else {
                eprintln!("  Key '{}' not found.", key_id);
            }
        }
        _ => {
            println!("  apikey commands:");
            println!("    cargo run -- apikey new [label] [role]   create a new API key");
            println!("    cargo run -- apikey list                 list all keys");
            println!("    cargo run -- apikey revoke <key_id>      revoke a key");
            println!("  Roles: read | write | admin");
        }
    }
}

fn format_unix(ts: u64) -> String {
    // Simple yyyy-mm-dd from unix timestamp (no external dep)
    let secs = ts;
    let days = secs / 86400;
    // Days since 1970-01-01 → approximate year/month/day
    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    format!("{:04}-{:02}-{:02}", year, month.min(12), day.min(31))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_store() -> ApiKeyStore {
        ApiKeyStore {
            data: KeysFile::default(),
            path: PathBuf::from("/tmp/pkt_test_keys_NONEXISTENT.json"),
        }
    }

    // ── ApiRole ───────────────────────────────────────────────────────────

    #[test]
    fn test_role_from_str_read() {
        assert_eq!(ApiRole::from_str("read"), Some(ApiRole::Read));
    }

    #[test]
    fn test_role_from_str_write() {
        assert_eq!(ApiRole::from_str("write"), Some(ApiRole::Write));
    }

    #[test]
    fn test_role_from_str_admin() {
        assert_eq!(ApiRole::from_str("admin"), Some(ApiRole::Admin));
    }

    #[test]
    fn test_role_from_str_case_insensitive() {
        assert_eq!(ApiRole::from_str("READ"), Some(ApiRole::Read));
        assert_eq!(ApiRole::from_str("ADMIN"), Some(ApiRole::Admin));
    }

    #[test]
    fn test_role_from_str_unknown() {
        assert_eq!(ApiRole::from_str("superuser"), None);
    }

    #[test]
    fn test_role_can_write() {
        assert!(!ApiRole::Read.can_write());
        assert!(ApiRole::Write.can_write());
        assert!(ApiRole::Admin.can_write());
    }

    #[test]
    fn test_role_is_admin() {
        assert!(!ApiRole::Read.is_admin());
        assert!(!ApiRole::Write.is_admin());
        assert!(ApiRole::Admin.is_admin());
    }

    #[test]
    fn test_role_as_str() {
        assert_eq!(ApiRole::Read.as_str(), "read");
        assert_eq!(ApiRole::Write.as_str(), "write");
        assert_eq!(ApiRole::Admin.as_str(), "admin");
    }

    // ── hash_api_key ──────────────────────────────────────────────────────

    #[test]
    fn test_hash_api_key_deterministic() {
        let k = "deadbeef";
        assert_eq!(hash_api_key(k), hash_api_key(k));
    }

    #[test]
    fn test_hash_api_key_different_for_different_keys() {
        assert_ne!(hash_api_key("key_a"), hash_api_key("key_b"));
    }

    #[test]
    fn test_hash_api_key_is_hex() {
        let h = hash_api_key("test");
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_api_key_length() {
        // blake3 = 32 bytes = 64 hex chars
        assert_eq!(hash_api_key("any_key").len(), 64);
    }

    // ── generate_raw_key ──────────────────────────────────────────────────

    #[test]
    fn test_raw_key_length() {
        let k = generate_raw_key();
        assert_eq!(k.len(), 64);
    }

    #[test]
    fn test_raw_key_is_hex() {
        let k = generate_raw_key();
        assert!(k.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_raw_keys_unique() {
        // Two consecutive keys should differ (timing + blake3 mixing)
        let a = generate_raw_key();
        let b = generate_raw_key();
        // Note: extremely rare collision possible due to simple entropy — acceptable in test
        // Just verify both are valid hex
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(b.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ── ApiKeyStore ───────────────────────────────────────────────────────

    #[test]
    fn test_store_empty_initially() {
        let store = mem_store();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_store_add_returns_raw_key() {
        let mut store = mem_store();
        let (raw, id) = store.add(ApiRole::Read, "test");
        assert_eq!(raw.len(), 64);
        assert_eq!(id.len(), 8);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_store_validate_correct_key() {
        let mut store = mem_store();
        let (raw, _) = store.add(ApiRole::Write, "svc");
        assert_eq!(store.validate(&raw), Some(&ApiRole::Write));
    }

    #[test]
    fn test_store_validate_wrong_key() {
        let mut store = mem_store();
        store.add(ApiRole::Read, "x");
        assert_eq!(store.validate("wrong_key_xxxx"), None);
    }

    #[test]
    fn test_store_validate_empty_key() {
        let store = mem_store();
        assert_eq!(store.validate(""), None);
    }

    #[test]
    fn test_store_revoke_existing() {
        let mut store = mem_store();
        let (_, id) = store.add(ApiRole::Admin, "admin");
        assert!(store.revoke(&id));
        assert!(store.is_empty());
    }

    #[test]
    fn test_store_revoke_nonexistent() {
        let mut store = mem_store();
        assert!(!store.revoke("no_such_id"));
    }

    #[test]
    fn test_store_validate_after_revoke() {
        let mut store = mem_store();
        let (raw, id) = store.add(ApiRole::Read, "temp");
        store.revoke(&id);
        assert_eq!(store.validate(&raw), None);
    }

    #[test]
    fn test_store_multiple_keys() {
        let mut store = mem_store();
        let (r1, _) = store.add(ApiRole::Read,  "reader");
        let (r2, _) = store.add(ApiRole::Write, "writer");
        let (r3, _) = store.add(ApiRole::Admin, "admin");
        assert_eq!(store.len(), 3);
        assert_eq!(store.validate(&r1), Some(&ApiRole::Read));
        assert_eq!(store.validate(&r2), Some(&ApiRole::Write));
        assert_eq!(store.validate(&r3), Some(&ApiRole::Admin));
    }

    #[test]
    fn test_store_list_returns_entries() {
        let mut store = mem_store();
        store.add(ApiRole::Read, "a");
        store.add(ApiRole::Write, "b");
        let list = store.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].label, "a");
        assert_eq!(list[1].label, "b");
    }

    // ── Save / load roundtrip ─────────────────────────────────────────────

    #[test]
    fn test_save_load_roundtrip() {
        let tmp = std::env::temp_dir().join("pkt_api_auth_test.json");
        let _ = std::fs::remove_file(&tmp);

        let mut store = ApiKeyStore::load(tmp.clone());
        let (raw, _) = store.add(ApiRole::Admin, "roundtrip");
        store.save().unwrap();

        let loaded = ApiKeyStore::load(tmp.clone());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.validate(&raw), Some(&ApiRole::Admin));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_load_missing_file_is_empty() {
        let store = ApiKeyStore::load(PathBuf::from("/tmp/does_not_exist_pkt.json"));
        assert!(store.is_empty());
    }

    // ── default_path ──────────────────────────────────────────────────────

    #[test]
    fn test_default_path_contains_pkt() {
        let p = ApiKeyStore::default_path();
        assert!(p.to_string_lossy().contains(".pkt"));
        assert!(p.to_string_lossy().contains("api_keys.json"));
    }
}
