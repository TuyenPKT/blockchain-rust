#![allow(dead_code)]

//! v5.2 — P2P improvements: peer scoring and bounded message deduplication
//!
//! Two standalone components (node.rs unchanged):
//!   1. `PeerRegistry` / `PeerScore` — track per-peer quality score from events
//!      (valid blocks, invalid blocks, latency, timeouts, disconnects).
//!      Provides `best_peers()` for preferred sync targets and `should_ban()`
//!      to feed into `security::BanList`.
//!
//!   2. `MessageDedup`  — bounded LRU-like dedup cache for block hashes and
//!      tx ids. Replaces the unbounded `HashSet` in node.rs. Capped at
//!      `max_size` entries; oldest entries evicted when full.

use std::collections::{HashMap, VecDeque, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

// ─── Score constants ──────────────────────────────────────────────────────────

const SCORE_VALID_BLOCK:       i32 = 10;
const SCORE_INVALID_BLOCK:     i32 = -20;
const SCORE_VALID_TX:          i32 = 1;
const SCORE_INVALID_TX:        i32 = -5;
const SCORE_TIMEOUT:           i32 = -10;
const SCORE_DISCONNECT:        i32 = -3;
const SCORE_INITIAL:           i32 = 0;
/// Score below this threshold triggers `should_ban()`.
pub const BAN_SCORE_THRESHOLD: i32 = -50;
/// Default dedup cache capacity for block hashes.
pub const DEFAULT_BLOCK_DEDUP_SIZE: usize = 1_000;
/// Default dedup cache capacity for tx ids.
pub const DEFAULT_TX_DEDUP_SIZE:    usize = 5_000;

// ─── 1. Peer Scoring ──────────────────────────────────────────────────────────

/// Events that affect a peer's score.
#[derive(Debug, Clone)]
pub enum ScoreEvent {
    /// Peer delivered a block that passed validation.
    ValidBlock,
    /// Peer delivered a block that failed validation.
    InvalidBlock,
    /// Peer delivered a valid mempool TX.
    ValidTransaction,
    /// Peer delivered an invalid / malformed TX.
    InvalidTransaction,
    /// Peer responded with measured round-trip latency.
    Responsive { latency_ms: u64 },
    /// Peer failed to respond within timeout.
    Timeout,
    /// Peer disconnected unexpectedly.
    Disconnect,
}

/// Per-peer quality data.
#[derive(Debug, Clone)]
pub struct PeerScore {
    pub address:       String,
    /// Cumulative score (higher = better).
    pub score:         i32,
    /// Exponential moving average of latency (None until first measurement).
    pub latency_ms:    Option<u64>,
    /// Unix timestamp of last successful message.
    pub last_seen:     u64,
    pub valid_blocks:  u32,
    pub invalid_blocks: u32,
    pub timeouts:      u32,
    pub disconnects:   u32,
}

impl PeerScore {
    fn new(address: &str) -> Self {
        PeerScore {
            address:       address.to_string(),
            score:         SCORE_INITIAL,
            latency_ms:    None,
            last_seen:     unix_now(),
            valid_blocks:  0,
            invalid_blocks: 0,
            timeouts:      0,
            disconnects:   0,
        }
    }

    fn apply(&mut self, event: &ScoreEvent) {
        match event {
            ScoreEvent::ValidBlock => {
                self.score        += SCORE_VALID_BLOCK;
                self.valid_blocks += 1;
                self.last_seen     = unix_now();
            }
            ScoreEvent::InvalidBlock => {
                self.score         += SCORE_INVALID_BLOCK;
                self.invalid_blocks += 1;
            }
            ScoreEvent::ValidTransaction => {
                self.score    += SCORE_VALID_TX;
                self.last_seen = unix_now();
            }
            ScoreEvent::InvalidTransaction => {
                self.score += SCORE_INVALID_TX;
            }
            ScoreEvent::Responsive { latency_ms } => {
                // Exponential moving average: new = 0.3*sample + 0.7*old
                self.latency_ms = Some(match self.latency_ms {
                    None      => *latency_ms,
                    Some(old) => (latency_ms * 3 + old * 7) / 10,
                });
                self.last_seen = unix_now();
            }
            ScoreEvent::Timeout => {
                self.score   += SCORE_TIMEOUT;
                self.timeouts += 1;
            }
            ScoreEvent::Disconnect => {
                self.score      += SCORE_DISCONNECT;
                self.disconnects += 1;
            }
        }
    }
}

/// Registry of all known peers and their scores.
pub struct PeerRegistry {
    peers: HashMap<String, PeerScore>,
}

impl Default for PeerRegistry {
    fn default() -> Self { Self::new() }
}

impl PeerRegistry {
    pub fn new() -> Self {
        PeerRegistry { peers: HashMap::new() }
    }

    /// Record a scoring event for `addr`. Creates entry on first call.
    pub fn record(&mut self, addr: &str, event: ScoreEvent) {
        self.peers
            .entry(addr.to_string())
            .or_insert_with(|| PeerScore::new(addr))
            .apply(&event);
    }

    /// Current score for `addr` (0 if unknown).
    pub fn score_of(&self, addr: &str) -> i32 {
        self.peers.get(addr).map(|p| p.score).unwrap_or(0)
    }

    /// Get score entry for `addr` if it exists.
    pub fn get(&self, addr: &str) -> Option<&PeerScore> {
        self.peers.get(addr)
    }

    /// `true` if the peer's score is at or below `BAN_SCORE_THRESHOLD`.
    pub fn should_ban(&self, addr: &str) -> bool {
        self.score_of(addr) <= BAN_SCORE_THRESHOLD
    }

    /// Return up to `n` peers sorted by score descending (best first).
    pub fn best_peers(&self, n: usize) -> Vec<&PeerScore> {
        let mut sorted: Vec<&PeerScore> = self.peers.values().collect();
        sorted.sort_by(|a, b| b.score.cmp(&a.score));
        sorted.truncate(n);
        sorted
    }

    /// Remove the `keep` highest-scoring peers, return addresses of evicted ones.
    /// Useful when the peer table is full and a new peer wants to connect.
    pub fn evict_worst(&mut self, keep: usize) -> Vec<String> {
        let mut addrs: Vec<(String, i32)> = self.peers.iter()
            .map(|(k, v)| (k.clone(), v.score))
            .collect();
        addrs.sort_by(|a, b| b.1.cmp(&a.1)); // descending score

        let to_evict: Vec<String> = addrs.into_iter()
            .skip(keep)
            .map(|(addr, _)| addr)
            .collect();

        for addr in &to_evict {
            self.peers.remove(addr);
        }
        to_evict
    }

    pub fn len(&self)     -> usize { self.peers.len() }
    pub fn is_empty(&self) -> bool { self.peers.is_empty() }
}

// ─── 2. Bounded Message Dedup ─────────────────────────────────────────────────

/// Bounded deduplication cache for P2P message IDs (block hashes, tx ids).
///
/// The current `node.rs` uses an unbounded `HashSet<String>` — over a long
/// uptime it grows without limit. `MessageDedup` caps the set at `max_size`:
/// when full, the oldest entry is evicted before inserting the new one.
///
/// This is a FIFO eviction policy (not true LRU) — good enough for dedup
/// where recent entries matter most.
pub struct MessageDedup {
    seen:     HashSet<String>,
    order:    VecDeque<String>, // tracks insertion order for eviction
    max_size: usize,
}

impl MessageDedup {
    pub fn new(max_size: usize) -> Self {
        assert!(max_size > 0, "max_size must be > 0");
        MessageDedup {
            seen:     HashSet::with_capacity(max_size),
            order:    VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    /// Returns `true` if `key` is new and was inserted.
    /// Returns `false` if `key` was already seen (duplicate — discard message).
    pub fn check_and_insert(&mut self, key: &str) -> bool {
        if self.seen.contains(key) {
            return false;
        }
        // Evict oldest if full
        if self.seen.len() >= self.max_size {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.remove(&oldest);
            }
        }
        self.seen.insert(key.to_string());
        self.order.push_back(key.to_string());
        true
    }

    pub fn contains(&self, key: &str) -> bool {
        self.seen.contains(key)
    }

    pub fn len(&self)     -> usize { self.seen.len() }
    pub fn is_empty(&self) -> bool { self.seen.is_empty() }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_score_events() {
        let mut reg = PeerRegistry::new();
        let ip = "10.0.0.1";

        assert_eq!(reg.score_of(ip), 0);

        reg.record(ip, ScoreEvent::ValidBlock);
        assert_eq!(reg.score_of(ip), SCORE_VALID_BLOCK);

        reg.record(ip, ScoreEvent::InvalidBlock);
        assert_eq!(reg.score_of(ip), SCORE_VALID_BLOCK + SCORE_INVALID_BLOCK);

        reg.record(ip, ScoreEvent::Timeout);
        assert_eq!(
            reg.score_of(ip),
            SCORE_VALID_BLOCK + SCORE_INVALID_BLOCK + SCORE_TIMEOUT
        );

        // Counters
        let p = reg.get(ip).unwrap();
        assert_eq!(p.valid_blocks,   1);
        assert_eq!(p.invalid_blocks, 1);
        assert_eq!(p.timeouts,       1);
    }

    #[test]
    fn test_peer_score_latency_ema() {
        let mut reg = PeerRegistry::new();
        let ip = "10.0.0.2";

        reg.record(ip, ScoreEvent::Responsive { latency_ms: 100 });
        assert_eq!(reg.get(ip).unwrap().latency_ms, Some(100));

        // EMA: new = 0.3*50 + 0.7*100 = 85
        reg.record(ip, ScoreEvent::Responsive { latency_ms: 50 });
        let ema = reg.get(ip).unwrap().latency_ms.unwrap();
        assert!(ema > 50 && ema < 100, "EMA {} should be between 50 and 100", ema);
    }

    #[test]
    fn test_peer_registry_should_ban() {
        let mut reg = PeerRegistry::new();
        let ip = "10.0.0.3";

        assert!(!reg.should_ban(ip));

        // Drive score below threshold
        for _ in 0..3 {
            reg.record(ip, ScoreEvent::InvalidBlock); // -20 each = -60 total
        }
        assert!(reg.should_ban(ip), "score {} should trigger ban", reg.score_of(ip));
    }

    #[test]
    fn test_peer_registry_best_peers() {
        let mut reg = PeerRegistry::new();

        reg.record("a.b.c.1", ScoreEvent::ValidBlock);
        reg.record("a.b.c.1", ScoreEvent::ValidBlock);
        reg.record("a.b.c.2", ScoreEvent::ValidBlock);
        reg.record("a.b.c.3", ScoreEvent::Timeout);

        let best = reg.best_peers(2);
        assert_eq!(best.len(), 2);
        assert!(best[0].score >= best[1].score, "best_peers must be sorted desc");
        assert_eq!(best[0].address, "a.b.c.1");
    }

    #[test]
    fn test_peer_registry_evict_worst() {
        let mut reg = PeerRegistry::new();

        reg.record("good1", ScoreEvent::ValidBlock);
        reg.record("good1", ScoreEvent::ValidBlock); // score = 20
        reg.record("good2", ScoreEvent::ValidBlock); // score = 10
        reg.record("bad1",  ScoreEvent::InvalidBlock); // score = -20
        reg.record("bad2",  ScoreEvent::Timeout);      // score = -10

        let evicted = reg.evict_worst(2);
        assert_eq!(evicted.len(), 2);
        assert_eq!(reg.len(), 2);

        // Survivors should be the two best
        assert!(reg.get("good1").is_some());
        assert!(reg.get("good2").is_some());
        assert!(reg.get("bad1").is_none());
        assert!(reg.get("bad2").is_none());
    }

    #[test]
    fn test_message_dedup_basic() {
        let mut dedup = MessageDedup::new(3);

        assert!(dedup.check_and_insert("hash1")); // new
        assert!(dedup.check_and_insert("hash2")); // new
        assert!(!dedup.check_and_insert("hash1")); // duplicate
        assert_eq!(dedup.len(), 2);
    }

    #[test]
    fn test_message_dedup_eviction() {
        let mut dedup = MessageDedup::new(3);

        dedup.check_and_insert("a");
        dedup.check_and_insert("b");
        dedup.check_and_insert("c");
        assert_eq!(dedup.len(), 3);

        // Insert 4th: "a" (oldest) should be evicted
        dedup.check_and_insert("d");
        assert_eq!(dedup.len(), 3);
        assert!(!dedup.contains("a"), "oldest entry should be evicted");
        assert!(dedup.contains("b"));
        assert!(dedup.contains("c"));
        assert!(dedup.contains("d"));
    }

    #[test]
    fn test_message_dedup_after_eviction_readmit() {
        let mut dedup = MessageDedup::new(2);

        dedup.check_and_insert("x");
        dedup.check_and_insert("y"); // full

        // "x" evicted when "z" added
        assert!(dedup.check_and_insert("z"));
        assert!(!dedup.contains("x"));

        // "x" is now unknown again — re-admitted
        assert!(dedup.check_and_insert("x"), "evicted key should be re-admittable");
    }
}
