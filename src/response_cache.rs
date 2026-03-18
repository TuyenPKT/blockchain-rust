#![allow(dead_code)]
//! v8.8 — PKTScan Response Cache
//!
//! In-memory TTL cache for API JSON responses with ETag support.
//! Cache entries expire after `ttl_secs` seconds.  ETag is derived from
//! the BLAKE3 hash of the response body (first 16 hex chars, quoted).
//!
//! API:
//!   ResponseCache::new(ttl_secs)       → empty cache
//!   cache.set(key, body)               → store entry
//!   cache.get(key)                     → Option<&CacheEntry> (None if expired)
//!   cache.invalidate(key)              → remove entry
//!   cache.evict_expired()              → prune stale entries
//!   ResponseCache::make_etag(body)     → ETag string e.g. `"a1b2c3d4e5f60718"`

use std::collections::HashMap;
use std::time::Instant;

// ─── CacheEntry ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CacheEntry {
    pub body:      String,
    pub etag:      String,
    pub cached_at: Instant,
    pub ttl_secs:  u64,
}

impl CacheEntry {
    /// True when the entry has lived longer than its TTL.
    pub fn is_expired(&self) -> bool {
        self.cached_at.elapsed().as_secs() >= self.ttl_secs
    }
}

// ─── ResponseCache ────────────────────────────────────────────────────────────

pub struct ResponseCache {
    entries:  HashMap<String, CacheEntry>,
    ttl_secs: u64,
}

impl ResponseCache {
    pub fn new(ttl_secs: u64) -> Self {
        ResponseCache { entries: HashMap::new(), ttl_secs }
    }

    /// Return a live (non-expired) entry, or `None`.
    pub fn get(&self, key: &str) -> Option<&CacheEntry> {
        self.entries.get(key).filter(|e| !e.is_expired())
    }

    /// Store or replace the entry for `key`.
    pub fn set(&mut self, key: String, body: String) {
        let etag = Self::make_etag(&body);
        self.entries.insert(key, CacheEntry {
            etag,
            body,
            cached_at: Instant::now(),
            ttl_secs:  self.ttl_secs,
        });
    }

    /// Remove a specific entry.
    pub fn invalidate(&mut self, key: &str) {
        self.entries.remove(key);
    }

    /// Remove all expired entries.
    pub fn evict_expired(&mut self) {
        self.entries.retain(|_, e| !e.is_expired());
    }

    pub fn len(&self)      -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool  { self.entries.is_empty() }

    /// Number of entries that are still live (not expired).
    pub fn live_count(&self) -> usize {
        self.entries.values().filter(|e| !e.is_expired()).count()
    }

    /// Generate a quoted ETag from the first 16 hex chars of a BLAKE3 hash.
    pub fn make_etag(body: &str) -> String {
        let hash = blake3::hash(body.as_bytes());
        format!("\"{}\"", &hash.to_hex()[..16])
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── basic set/get ─────────────────────────────────────────────────────

    #[test]
    fn test_new_cache_empty() {
        let c = ResponseCache::new(5);
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn test_set_and_get() {
        let mut c = ResponseCache::new(60);
        c.set("/api/stats".into(), r#"{"height":1}"#.into());
        let entry = c.get("/api/stats");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().body, r#"{"height":1}"#);
    }

    #[test]
    fn test_get_expired_returns_none() {
        let mut c = ResponseCache::new(0); // ttl=0 → all entries instantly expired
        c.set("key".into(), "body".into());
        assert!(c.get("key").is_none());
    }

    #[test]
    fn test_get_unknown_key_returns_none() {
        let c = ResponseCache::new(60);
        assert!(c.get("/unknown").is_none());
    }

    #[test]
    fn test_set_overwrites() {
        let mut c = ResponseCache::new(60);
        c.set("k".into(), "v1".into());
        c.set("k".into(), "v2".into());
        assert_eq!(c.get("k").unwrap().body, "v2");
        assert_eq!(c.len(), 1); // still one entry
    }

    // ── invalidate ────────────────────────────────────────────────────────

    #[test]
    fn test_invalidate_removes_entry() {
        let mut c = ResponseCache::new(60);
        c.set("k".into(), "v".into());
        assert!(c.get("k").is_some());
        c.invalidate("k");
        assert!(c.get("k").is_none());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn test_invalidate_nonexistent_is_noop() {
        let mut c = ResponseCache::new(60);
        c.invalidate("nope"); // should not panic
        assert!(c.is_empty());
    }

    // ── evict_expired ─────────────────────────────────────────────────────

    #[test]
    fn test_evict_expired_removes_stale() {
        let mut c = ResponseCache::new(0); // all entries expire instantly
        c.set("k1".into(), "v1".into());
        c.set("k2".into(), "v2".into());
        c.evict_expired();
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn test_evict_expired_keeps_live() {
        let mut c = ResponseCache::new(60);
        c.set("live".into(), "v".into());
        c.evict_expired();
        assert_eq!(c.len(), 1);
    }

    // ── live_count ────────────────────────────────────────────────────────

    #[test]
    fn test_live_count_excludes_expired() {
        let mut c = ResponseCache::new(0);
        c.set("expired".into(), "v".into());
        assert_eq!(c.live_count(), 0);
    }

    #[test]
    fn test_live_count_includes_fresh() {
        let mut c = ResponseCache::new(60);
        c.set("a".into(), "1".into());
        c.set("b".into(), "2".into());
        assert_eq!(c.live_count(), 2);
    }

    // ── ETag ──────────────────────────────────────────────────────────────

    #[test]
    fn test_etag_is_quoted() {
        let etag = ResponseCache::make_etag("hello");
        assert!(etag.starts_with('"'));
        assert!(etag.ends_with('"'));
    }

    #[test]
    fn test_etag_deterministic() {
        let a = ResponseCache::make_etag("same body");
        let b = ResponseCache::make_etag("same body");
        assert_eq!(a, b);
    }

    #[test]
    fn test_etag_different_bodies() {
        let a = ResponseCache::make_etag("body A");
        let b = ResponseCache::make_etag("body B");
        assert_ne!(a, b);
    }

    #[test]
    fn test_etag_stored_in_entry() {
        let mut c = ResponseCache::new(60);
        let body = r#"{"x":1}"#;
        c.set("k".into(), body.into());
        let etag = c.get("k").unwrap().etag.clone();
        assert_eq!(etag, ResponseCache::make_etag(body));
    }

    #[test]
    fn test_etag_length() {
        let etag = ResponseCache::make_etag("test");
        // 16 hex chars + 2 quotes = 18 chars
        assert_eq!(etag.len(), 18);
    }

    // ── multiple keys ─────────────────────────────────────────────────────

    #[test]
    fn test_multiple_keys_independent() {
        let mut c = ResponseCache::new(60);
        c.set("/api/stats".into(),  r#"{"h":1}"#.into());
        c.set("/api/blocks".into(), r#"{"b":[]}"#.into());
        assert_eq!(c.len(), 2);
        assert_eq!(c.get("/api/stats").unwrap().body,  r#"{"h":1}"#);
        assert_eq!(c.get("/api/blocks").unwrap().body, r#"{"b":[]}"#);
    }
}
