#![allow(dead_code)]

//! v5.1 — Security hardening: input validation, DoS protection, rate limiting
//!
//! Four layers of defense added as standalone utilities (node.rs unchanged):
//!   1. `RateLimiter`     — sliding-window per-IP message rate limit
//!   2. `BanList`         — temporary IP bans with TTL
//!   3. `PeerGuard`       — combines rate + ban + strike counter + max-peer cap
//!   4. `InputValidator`  — stateless validation of TXs, blocks, and P2P messages
//!
//! `ConnectionLimits` provides named constants for every cap so they can be
//! tuned in one place without touching logic.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::transaction::Transaction;
use crate::block::Block;

// ─── Connection / Message Limits ──────────────────────────────────────────────

pub struct ConnectionLimits;

impl ConnectionLimits {
    /// Maximum outbound + inbound peers accepted by a node.
    pub const MAX_PEERS: usize = 8;
    /// Maximum blocks returned in a single `Blocks` response.
    pub const MAX_BLOCKS_PER_RESPONSE: usize = 100;
    /// Maximum peer addresses returned in a single `Peers` response.
    pub const MAX_PEERS_PER_RESPONSE: usize = 50;
    /// Maximum TXs returned in a single `MempoolTxs` response.
    pub const MAX_TXS_PER_RESPONSE: usize = 500;
    /// Rate limit: max messages per IP per window.
    pub const RATE_LIMIT_MSGS_PER_WINDOW: u32 = 100;
    /// Rate limit: sliding window length in seconds.
    pub const RATE_LIMIT_WINDOW_SECS: u64 = 60;
    /// Default ban duration in seconds (1 hour).
    pub const BAN_DURATION_SECS: u64 = 3_600;
    /// Number of violations before automatic ban.
    pub const MAX_STRIKES_BEFORE_BAN: u32 = 5;
}

// ─── 1. Rate Limiter ──────────────────────────────────────────────────────────

/// Sliding-window per-IP rate limiter.
///
/// Counts how many messages an IP has sent in the current `window_secs` window.
/// Once `max_per_window` is reached, `check` returns `false` until the window
/// rolls over.
pub struct RateLimiter {
    window_secs:    u64,
    max_per_window: u32,
    /// ip → (message_count, window_start_unix)
    counts: HashMap<String, (u32, u64)>,
}

impl RateLimiter {
    pub fn new(max_per_window: u32, window_secs: u64) -> Self {
        RateLimiter { window_secs, max_per_window, counts: HashMap::new() }
    }

    /// Returns `true` if the IP is still within its rate limit.
    /// Increments the counter and resets the window when expired.
    pub fn check(&mut self, ip: &str) -> bool {
        let now = unix_now();
        let entry = self.counts.entry(ip.to_string()).or_insert((0, now));
        // Roll window if expired
        if now - entry.1 >= self.window_secs {
            *entry = (0, now);
        }
        entry.0 += 1;
        entry.0 <= self.max_per_window
    }

    /// Reset the counter for an IP (e.g. after a legitimate handshake).
    pub fn reset(&mut self, ip: &str) {
        self.counts.remove(ip);
    }

    /// Current message count for an IP in this window.
    pub fn count_for(&self, ip: &str) -> u32 {
        self.counts.get(ip).map(|(c, _)| *c).unwrap_or(0)
    }
}

// ─── 2. Ban List ──────────────────────────────────────────────────────────────

/// Temporary IP ban list with per-entry TTL.
///
/// Banned IPs are rejected immediately in `PeerGuard::admit` and
/// `PeerGuard::check_rate` without consuming rate-limit quota.
pub struct BanList {
    /// ip → ban_expiry_unix_secs
    bans: HashMap<String, u64>,
}

impl BanList {
    pub fn new() -> Self { BanList { bans: HashMap::new() } }

    /// Ban an IP for `duration_secs` seconds from now.
    pub fn ban(&mut self, ip: &str, duration_secs: u64) {
        self.bans.insert(ip.to_string(), unix_now() + duration_secs);
    }

    /// `true` if the IP is currently banned (unexpired entry).
    pub fn is_banned(&self, ip: &str) -> bool {
        match self.bans.get(ip) {
            Some(&expiry) => unix_now() < expiry,
            None => false,
        }
    }

    /// Manually unban an IP.
    pub fn unban(&mut self, ip: &str) {
        self.bans.remove(ip);
    }

    /// Remove all expired ban entries.
    pub fn cleanup(&mut self) {
        let now = unix_now();
        self.bans.retain(|_, &mut exp| exp > now);
    }

    pub fn banned_count(&self) -> usize {
        let now = unix_now();
        self.bans.values().filter(|&&exp| exp > now).count()
    }
}

impl Default for BanList {
    fn default() -> Self { Self::new() }
}

// ─── 3. Peer Guard ────────────────────────────────────────────────────────────

/// PeerGuard combines rate limiting, ban enforcement, strike counting, and
/// max-peer connection cap into a single gatekeeper.
///
/// Usage (conceptual, hooked into Node::start):
/// ```text
/// let mut guard = PeerGuard::new(ConnectionLimits::MAX_PEERS);
/// if !guard.admit(&ip, current_count) { reject(); }
/// // per message:
/// if !guard.check_rate(&ip) { guard.strike(&ip); reject(); }
/// ```
pub struct PeerGuard {
    pub rate_limiter: RateLimiter,
    pub ban_list:     BanList,
    pub max_peers:    usize,
    /// ip → violation count; auto-ban after MAX_STRIKES_BEFORE_BAN
    strikes: HashMap<String, u32>,
}

impl PeerGuard {
    pub fn new(max_peers: usize) -> Self {
        PeerGuard {
            rate_limiter: RateLimiter::new(
                ConnectionLimits::RATE_LIMIT_MSGS_PER_WINDOW,
                ConnectionLimits::RATE_LIMIT_WINDOW_SECS,
            ),
            ban_list:  BanList::new(),
            max_peers,
            strikes:   HashMap::new(),
        }
    }

    /// Decide whether a new peer connection from `ip` is allowed.
    /// Rejected when: banned, or connection cap already reached.
    pub fn admit(&self, ip: &str, current_peer_count: usize) -> bool {
        if self.ban_list.is_banned(ip) { return false; }
        if current_peer_count >= self.max_peers { return false; }
        true
    }

    /// Check per-message rate limit for an IP.
    /// Returns `false` if the IP is banned or has exceeded its quota.
    pub fn check_rate(&mut self, ip: &str) -> bool {
        if self.ban_list.is_banned(ip) { return false; }
        self.rate_limiter.check(ip)
    }

    /// Record a protocol violation strike for an IP.
    /// Returns `true` if the IP has now been auto-banned.
    pub fn strike(&mut self, ip: &str) -> bool {
        let count = self.strikes.entry(ip.to_string()).or_insert(0);
        *count += 1;
        if *count >= ConnectionLimits::MAX_STRIKES_BEFORE_BAN {
            self.ban_list.ban(ip, ConnectionLimits::BAN_DURATION_SECS);
            true
        } else {
            false
        }
    }

    /// Number of violations for an IP (0 if none).
    pub fn strikes_for(&self, ip: &str) -> u32 {
        self.strikes.get(ip).copied().unwrap_or(0)
    }
}

// ─── 4. Input Validator ───────────────────────────────────────────────────────

/// Stateless validator for P2P message payloads and blockchain data.
///
/// Each method returns `Ok(())` or `Err(reason)` — callers decide whether to
/// drop the message, strike the peer, or ban immediately.
pub struct InputValidator;

impl InputValidator {
    /// Validate a transaction received via P2P (`NewTransaction`) or mempool.
    ///
    /// Checks:
    ///  - `tx_id` is a 64-char hex string (32 bytes)
    ///  - has at least one output
    ///  - all outputs have `amount > 0`
    ///  - total output doesn't exceed MAX_SUPPLY (21 million PKT in paklets)
    ///  - not a coinbase (coinbase must only come from miners, not P2P)
    pub fn validate_tx(tx: &Transaction) -> Result<(), &'static str> {
        if tx.tx_id.len() != 64 {
            return Err("tx_id must be 64 hex chars");
        }
        if !tx.tx_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("tx_id contains non-hex characters");
        }
        if tx.outputs.is_empty() {
            return Err("transaction must have at least one output");
        }
        for out in &tx.outputs {
            if out.amount == 0 {
                return Err("output amount must be > 0");
            }
        }
        // 21 million PKT × 1e8 paklets/PKT
        const MAX_SUPPLY: u64 = 21_000_000 * 100_000_000;
        if tx.total_output() > MAX_SUPPLY {
            return Err("total output exceeds max supply");
        }
        if tx.is_coinbase {
            return Err("coinbase TX must not be relayed via P2P");
        }
        Ok(())
    }

    /// Validate a `Blocks` response from a peer.
    ///
    /// Checks:
    ///  - doesn't exceed `max_count` (DoS guard)
    ///  - block hashes are non-empty
    ///  - indices are strictly ascending
    pub fn validate_blocks_response(blocks: &[Block], max_count: usize) -> Result<(), &'static str> {
        if blocks.len() > max_count {
            return Err("too many blocks in response");
        }
        let mut prev_index = None::<u64>;
        for b in blocks {
            if b.hash.is_empty() {
                return Err("block has empty hash");
            }
            if let Some(pi) = prev_index {
                if b.index <= pi {
                    return Err("blocks not in ascending index order");
                }
            }
            prev_index = Some(b.index);
        }
        Ok(())
    }

    /// Validate a `Peers` response from a peer.
    ///
    /// Checks:
    ///  - doesn't exceed `max_count`
    ///  - each address has the form "host:port" (contains ':')
    ///  - port is parseable as u16
    pub fn validate_peers_response(addrs: &[String], max_count: usize) -> Result<(), &'static str> {
        if addrs.len() > max_count {
            return Err("too many peers in response");
        }
        for addr in addrs {
            let mut parts = addr.rsplitn(2, ':');
            let port_str = parts.next().unwrap_or("");
            let host     = parts.next().unwrap_or("");
            if host.is_empty() {
                return Err("peer address missing host");
            }
            if port_str.parse::<u16>().is_err() {
                return Err("peer address has invalid port");
            }
        }
        Ok(())
    }

    /// Validate a `Hello` handshake message.
    pub fn validate_hello(version: u32, host: &str, port: u16) -> Result<(), &'static str> {
        if version == 0 {
            return Err("Hello version must be > 0");
        }
        if host.is_empty() || host.len() > 255 {
            return Err("Hello host invalid length");
        }
        if port == 0 {
            return Err("Hello port must be > 0");
        }
        Ok(())
    }
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
    fn test_rate_limiter() {
        let mut rl = RateLimiter::new(3, 60);

        assert!(rl.check("1.2.3.4"));  // 1
        assert!(rl.check("1.2.3.4"));  // 2
        assert!(rl.check("1.2.3.4"));  // 3 — at limit
        assert!(!rl.check("1.2.3.4")); // 4 — over limit

        // Different IP is independent
        assert!(rl.check("5.6.7.8"));

        // Reset clears the count
        rl.reset("1.2.3.4");
        assert!(rl.check("1.2.3.4")); // back to 1
    }

    #[test]
    fn test_ban_list() {
        let mut bl = BanList::new();

        assert!(!bl.is_banned("1.2.3.4"));

        bl.ban("1.2.3.4", 3600);
        assert!(bl.is_banned("1.2.3.4"));
        assert_eq!(bl.banned_count(), 1);

        bl.unban("1.2.3.4");
        assert!(!bl.is_banned("1.2.3.4"));
        assert_eq!(bl.banned_count(), 0);

        // Expired ban (TTL = 0 secs in the past)
        bl.bans.insert("2.3.4.5".to_string(), 0); // expired
        assert!(!bl.is_banned("2.3.4.5"));
        bl.cleanup();
        assert!(!bl.bans.contains_key("2.3.4.5"));
    }

    #[test]
    fn test_peer_guard_admission() {
        let guard = PeerGuard::new(2); // max 2 peers

        assert!(guard.admit("10.0.0.1", 0));
        assert!(guard.admit("10.0.0.1", 1));
        assert!(!guard.admit("10.0.0.1", 2)); // cap reached

        // Banned IP rejected regardless of count
        let mut guard2 = PeerGuard::new(10);
        guard2.ban_list.ban("192.168.1.1", 3600);
        assert!(!guard2.admit("192.168.1.1", 0));
    }

    #[test]
    fn test_peer_guard_strikes_ban() {
        let mut guard = PeerGuard::new(8);
        let ip = "10.0.0.2";

        for i in 1..ConnectionLimits::MAX_STRIKES_BEFORE_BAN {
            let banned = guard.strike(ip);
            assert!(!banned, "should not be banned after {} strikes", i);
            assert_eq!(guard.strikes_for(ip), i);
        }

        // Final strike triggers ban
        let banned = guard.strike(ip);
        assert!(banned, "should be banned after max strikes");
        assert!(guard.ban_list.is_banned(ip));
        assert!(!guard.check_rate(ip), "banned IP must fail rate check");
    }

    #[test]
    fn test_input_validator_tx() {
        use crate::transaction::Transaction;

        // Valid coinbase-like structure but marked non-coinbase
        let mut tx = Transaction::coinbase("aabbccddaabbccddaabbccddaabbccddaabbccdd", 0);
        tx.is_coinbase = false;
        tx.tx_id = format!("{:064x}", 42u64); // valid 64-char hex

        // Relay of coinbase flag → rejected
        let coinbase_tx = Transaction::coinbase("aabb", 0);
        assert!(InputValidator::validate_tx(&coinbase_tx).is_err());

        // Non-coinbase with valid id → ok
        assert!(InputValidator::validate_tx(&tx).is_ok());

        // Invalid tx_id length
        let mut bad = tx.clone();
        bad.tx_id = "short".to_string();
        assert!(InputValidator::validate_tx(&bad).is_err());

        // Zero-amount output
        let mut bad2 = tx.clone();
        bad2.outputs[0].amount = 0;
        assert!(InputValidator::validate_tx(&bad2).is_err());

        // Empty outputs
        let mut bad3 = tx.clone();
        bad3.outputs.clear();
        assert!(InputValidator::validate_tx(&bad3).is_err());
    }

    #[test]
    fn test_input_validator_blocks() {
        use crate::chain::Blockchain;

        let mut bc = Blockchain::new();
        bc.difficulty = 1;
        bc.add_block(vec![], "miner");

        // Valid single-block response
        let blocks = bc.chain.clone();
        assert!(InputValidator::validate_blocks_response(&blocks, 100).is_ok());

        // Too many blocks
        assert!(InputValidator::validate_blocks_response(&blocks, 0).is_err());

        // Non-ascending indices
        let mut bad_blocks = blocks.clone();
        if bad_blocks.len() >= 2 {
            bad_blocks[0].index = 99; // out of order
            assert!(InputValidator::validate_blocks_response(&bad_blocks, 100).is_err());
        }
    }

    #[test]
    fn test_input_validator_peers() {
        let good = vec!["127.0.0.1:8333".to_string(), "10.0.0.1:18333".to_string()];
        assert!(InputValidator::validate_peers_response(&good, 50).is_ok());

        // Too many
        assert!(InputValidator::validate_peers_response(&good, 1).is_err());

        // Missing host
        let bad = vec![":8333".to_string()];
        assert!(InputValidator::validate_peers_response(&bad, 50).is_err());

        // Invalid port
        let bad2 = vec!["host:notaport".to_string()];
        assert!(InputValidator::validate_peers_response(&bad2, 50).is_err());
    }

    #[test]
    fn test_input_validator_hello() {
        assert!(InputValidator::validate_hello(1, "127.0.0.1", 8333).is_ok());
        assert!(InputValidator::validate_hello(0, "127.0.0.1", 8333).is_err()); // version 0
        assert!(InputValidator::validate_hello(1, "", 8333).is_err());           // empty host
        assert!(InputValidator::validate_hello(1, "127.0.0.1", 0).is_err());    // port 0
    }
}
