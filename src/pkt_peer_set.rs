#![allow(dead_code)]
//! v23.3 — Multi-peer Manager
//!
//! `PeerSet` quản lý N kết nối outbound song song, track height per peer,
//! và ban peer có hành vi xấu.
//!
//! Architecture:
//! ```text
//! PeerSet
//!   ├── slots: HashMap<addr, PeerSlot>   ← trạng thái từng peer
//!   ├── connect-thread per peer          ← TCP + handshake + message loop
//!   └── manager-thread                  ← reconnect / expire bans mỗi 10s
//! ```
//!
//! States:
//! ```text
//! Connecting → Connected → Disconnected → (retry) → Connecting
//!                       ↘ Banned (TTL) → (expired) → Disconnected
//! ```
//!
//! Ban trigger:
//!   - `peer_set.strike(addr)` — tăng strike counter
//!   - Đủ `MAX_STRIKES` (3) → auto-ban `DEFAULT_BAN_SECS` (3600s)
//!   - Có thể ban thủ công: `peer_set.ban(addr, duration_secs)`

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::pkt_peer::{PeerConfig, connect_once, PeerError};
use crate::pkt_wire::{PktMsg, TESTNET_MAGIC};
use crate::pkt_peer::{send_msg, recv_msg};

// ── Constants ─────────────────────────────────────────────────────────────────

pub const MAX_STRIKES:        u32 = 3;
pub const DEFAULT_BAN_SECS:   u64 = 3600;   // 1 giờ
pub const MANAGER_INTERVAL:   u64 = 10;     // giây giữa các lần manager wakeup
pub const BASE_RETRY_SECS:    u64 = 5;
pub const MAX_RETRY_SECS:     u64 = 300;    // 5 phút
pub const CONNECT_TIMEOUT:    u64 = 10;
pub const READ_TIMEOUT:       u64 = 60;

// ── PeerStatus ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum PeerStatus {
    /// Đang kết nối / handshake.
    Connecting,
    /// Kết nối thành công, handshake xong.
    Connected,
    /// Mất kết nối — `retry_at` là thời điểm thử lại.
    Disconnected {
        since:    Instant,
        retry_at: Instant,
        attempt:  u32,
    },
    /// Bị ban đến `until_unix` (Unix timestamp giây).
    Banned { until_unix: u64 },
}

impl PeerStatus {
    pub fn is_connected(&self)    -> bool { matches!(self, Self::Connected) }
    pub fn is_connecting(&self)   -> bool { matches!(self, Self::Connecting) }
    pub fn is_banned(&self)       -> bool {
        matches!(self, Self::Banned { until_unix } if unix_now() < *until_unix)
    }
    pub fn is_ready_to_retry(&self) -> bool {
        match self {
            Self::Disconnected { retry_at, .. } => Instant::now() >= *retry_at,
            Self::Banned { until_unix } => unix_now() >= *until_unix,
            _ => false,
        }
    }
}

// ── PeerSlot ──────────────────────────────────────────────────────────────────

/// Trạng thái của một outbound peer.
#[derive(Debug, Clone)]
pub struct PeerSlot {
    pub addr:    String,
    pub status:  PeerStatus,
    /// Best height peer báo cáo (từ Version message).
    pub height:  i32,
    /// User-agent string (từ handshake).
    pub agent:   String,
    /// Số lần vi phạm — đủ MAX_STRIKES thì auto-ban.
    pub strikes: u32,
}

impl PeerSlot {
    fn new(addr: &str) -> Self {
        PeerSlot {
            addr:    addr.to_string(),
            status:  PeerStatus::Disconnected {
                since:    Instant::now(),
                retry_at: Instant::now(), // retry ngay lập tức
                attempt:  0,
            },
            height:  0,
            agent:   String::new(),
            strikes: 0,
        }
    }
}

// ── PeerSetInner ──────────────────────────────────────────────────────────────

struct PeerSetInner {
    slots:         HashMap<String, PeerSlot>,
    max_outbound:  usize,
    ban_secs:      u64,
}

// ── PeerSet ───────────────────────────────────────────────────────────────────

/// Thread-safe multi-peer outbound manager.
///
/// Clone `Arc<PeerSet>` để share giữa các threads.
#[derive(Clone)]
pub struct PeerSet {
    inner:      Arc<Mutex<PeerSetInner>>,
    magic:      [u8; 4],
    our_height: Arc<AtomicI32>,
}

impl PeerSet {
    /// Tạo PeerSet mới.
    ///
    /// - `magic`: network magic bytes (TESTNET / MAINNET)
    /// - `our_height`: chain height hiện tại của node ta
    /// - `max_outbound`: số kết nối outbound tối đa
    pub fn new(magic: [u8; 4], our_height: i32, max_outbound: usize) -> Arc<Self> {
        let ps = Arc::new(PeerSet {
            inner: Arc::new(Mutex::new(PeerSetInner {
                slots:        HashMap::new(),
                max_outbound,
                ban_secs:     DEFAULT_BAN_SECS,
            })),
            magic,
            our_height: Arc::new(AtomicI32::new(our_height)),
        });

        // Spawn manager thread
        let ps_mgr = Arc::clone(&ps);
        thread::spawn(move || manager_loop(ps_mgr));

        ps
    }

    /// Thêm peer vào set. Nếu đã tồn tại thì không làm gì.
    pub fn add_peer(self: &Arc<Self>, addr: &str) {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.slots.contains_key(addr) { return; }
            if inner.slots.len() >= inner.max_outbound { return; }
            inner.slots.insert(addr.to_string(), PeerSlot::new(addr));
        }
        self.spawn_connect(addr);
    }

    /// Ban peer `addr` trong `duration_secs` giây.
    pub fn ban(&self, addr: &str, duration_secs: u64) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(slot) = inner.slots.get_mut(addr) {
            println!("[peer-set] ban {} for {}s", addr, duration_secs);
            slot.status = PeerStatus::Banned { until_unix: unix_now() + duration_secs };
        }
    }

    /// Tăng strike counter của peer. Đủ MAX_STRIKES → auto-ban.
    pub fn strike(&self, addr: &str) {
        let ban_secs = self.inner.lock().unwrap().ban_secs;
        let mut inner = self.inner.lock().unwrap();
        if let Some(slot) = inner.slots.get_mut(addr) {
            slot.strikes += 1;
            println!("[peer-set] strike {}/{} for {}", slot.strikes, MAX_STRIKES, addr);
            if slot.strikes >= MAX_STRIKES {
                println!("[peer-set] auto-ban {} ({} strikes)", addr, slot.strikes);
                slot.status = PeerStatus::Banned { until_unix: unix_now() + ban_secs };
                slot.strikes = 0;
            }
        }
    }

    /// Update our chain height — gửi kèm Version khi reconnect.
    pub fn set_our_height(&self, height: i32) {
        self.our_height.store(height, Ordering::Relaxed);
    }

    // ── Query ─────────────────────────────────────────────────────────────────

    /// Số peer đang Connected.
    pub fn peer_count(&self) -> usize {
        self.inner.lock().unwrap().slots.values()
            .filter(|s| s.status.is_connected())
            .count()
    }

    /// Height cao nhất trong tất cả Connected peers.
    pub fn best_height(&self) -> i32 {
        self.inner.lock().unwrap().slots.values()
            .filter(|s| s.status.is_connected())
            .map(|s| s.height)
            .max()
            .unwrap_or(0)
    }

    /// Snapshot trạng thái tất cả peers (để hiển thị / logging).
    pub fn slots(&self) -> Vec<PeerSlot> {
        self.inner.lock().unwrap().slots.values().cloned().collect()
    }

    /// Danh sách địa chỉ Connected peers.
    pub fn connected_addrs(&self) -> Vec<String> {
        self.inner.lock().unwrap().slots.values()
            .filter(|s| s.status.is_connected())
            .map(|s| s.addr.clone())
            .collect()
    }

    // ── Internal state updates (gọi từ connect thread) ────────────────────────

    pub(crate) fn mark_connecting(&self, addr: &str) {
        if let Some(slot) = self.inner.lock().unwrap().slots.get_mut(addr) {
            slot.status = PeerStatus::Connecting;
        }
    }

    pub(crate) fn mark_connected(&self, addr: &str, agent: &str, height: i32) {
        if let Some(slot) = self.inner.lock().unwrap().slots.get_mut(addr) {
            slot.status = PeerStatus::Connected;
            slot.agent  = agent.to_string();
            slot.height = height;
            slot.strikes = 0; // reset strikes khi kết nối thành công
            println!("[peer-set] connected {} height={} agent=\"{}\"", addr, height, agent);
        }
    }

    pub(crate) fn mark_disconnected(&self, addr: &str, attempt: u32) {
        if let Some(slot) = self.inner.lock().unwrap().slots.get_mut(addr) {
            if slot.status.is_banned() { return; } // giữ ban, không override
            let delay = backoff(attempt);
            slot.status = PeerStatus::Disconnected {
                since:    Instant::now(),
                retry_at: Instant::now() + delay,
                attempt,
            };
            println!("[peer-set] disconnected {} — retry in {}s", addr, delay.as_secs());
        }
    }

    pub(crate) fn update_height(&self, addr: &str, height: i32) {
        if let Some(slot) = self.inner.lock().unwrap().slots.get_mut(addr) {
            if height > slot.height { slot.height = height; }
        }
    }

    fn spawn_connect(self: &Arc<Self>, addr: &str) {
        let ps   = Arc::clone(self);
        let addr = addr.to_string();
        thread::spawn(move || peer_connect_loop(ps, addr));
    }
}

// ── Connect loop per peer ─────────────────────────────────────────────────────

fn peer_connect_loop(ps: Arc<PeerSet>, addr: String) {
    let mut attempt = 0u32;

    loop {
        // Kiểm tra ban trước khi connect
        {
            let inner = ps.inner.lock().unwrap();
            if let Some(slot) = inner.slots.get(&addr) {
                if slot.status.is_banned() {
                    return; // manager sẽ respawn sau khi ban hết hạn
                }
            } else {
                return; // slot bị xoá
            }
        }

        ps.mark_connecting(&addr);

        let our_h = ps.our_height.load(Ordering::Relaxed);
        let cfg   = PeerConfig {
            host:                 addr.rsplit_once(':')
                                      .map(|(h, _)| h.to_string())
                                      .unwrap_or_else(|| addr.clone()),
            port:                 addr.rsplit_once(':')
                                      .and_then(|(_, p)| p.parse().ok())
                                      .unwrap_or(if ps.magic == TESTNET_MAGIC { 8333 } else { 64764 }),
            magic:                ps.magic,
            connect_timeout_secs: CONNECT_TIMEOUT,
            read_timeout_secs:    READ_TIMEOUT,
            our_height:           our_h,
            max_retries:          1,
            ..PeerConfig::default()
        };

        match connect_once(&cfg) {
            Ok((mut stream, info)) => {
                attempt = 0;
                ps.mark_connected(&addr, &info.user_agent, info.start_height);

                // ── Message loop ──────────────────────────────────────────────
                loop {
                    match recv_msg(&mut stream, ps.magic) {
                        Ok(msg) => match msg {
                            PktMsg::Ping { nonce } => {
                                if send_msg(&mut stream, PktMsg::Pong { nonce }, ps.magic).is_err() {
                                    break;
                                }
                            }
                            PktMsg::Pong { .. } => {}
                            PktMsg::Inv { items } => {
                                println!("[peer-set] inv from {}: {} items", addr, items.len());
                            }
                            PktMsg::Version(v) => {
                                // peer có thể gửi Version update height
                                ps.update_height(&addr, v.start_height);
                            }
                            _ => {}
                        },
                        Err(PeerError::Timeout) => {
                            // Keepalive ping
                            let nonce = unix_now();
                            if send_msg(&mut stream, PktMsg::Ping { nonce }, ps.magic).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }

                ps.mark_disconnected(&addr, attempt);
                return; // manager thread sẽ respawn sau retry_at
            }
            Err(e) => {
                attempt += 1;
                println!("[peer-set] connect {} failed (attempt {}): {}", addr, attempt, e);
                ps.mark_disconnected(&addr, attempt);
                return; // manager sẽ respawn
            }
        }
    }
}

// ── Manager loop ──────────────────────────────────────────────────────────────

fn manager_loop(ps: Arc<PeerSet>) {
    loop {
        thread::sleep(Duration::from_secs(MANAGER_INTERVAL));

        // Collect addrs cần reconnect (không lock lâu)
        let to_reconnect: Vec<(String, u32)> = {
            let inner = ps.inner.lock().unwrap();
            inner.slots.values()
                .filter_map(|slot| {
                    match &slot.status {
                        PeerStatus::Disconnected { retry_at, attempt, .. }
                            if Instant::now() >= *retry_at =>
                        {
                            Some((slot.addr.clone(), *attempt))
                        }
                        PeerStatus::Banned { until_unix }
                            if unix_now() >= *until_unix =>
                        {
                            // Ban hết hạn → reset về Disconnected
                            Some((slot.addr.clone(), 0))
                        }
                        _ => None,
                    }
                })
                .collect()
        };

        for (addr, attempt) in to_reconnect {
            // Reset Banned → Disconnected nếu ban hết hạn
            {
                let mut inner = ps.inner.lock().unwrap();
                if let Some(slot) = inner.slots.get_mut(&addr) {
                    if let PeerStatus::Banned { until_unix } = slot.status {
                        if unix_now() >= until_unix {
                            println!("[peer-set] ban expired for {}", addr);
                            slot.status = PeerStatus::Disconnected {
                                since:    Instant::now(),
                                retry_at: Instant::now(),
                                attempt:  0,
                            };
                        }
                    }
                }
            }
            // Chỉ spawn nếu vẫn cần reconnect (không bị ban tiếp)
            {
                let inner = ps.inner.lock().unwrap();
                if let Some(slot) = inner.slots.get(&addr) {
                    if slot.status.is_ready_to_retry() || matches!(slot.status, PeerStatus::Disconnected { .. }) {
                        drop(inner);
                        println!("[peer-set] reconnecting {} (attempt {})", addr, attempt + 1);
                        ps.spawn_connect(&addr);
                    }
                }
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn backoff(attempt: u32) -> Duration {
    let secs = BASE_RETRY_SECS.saturating_mul(1u64 << attempt.min(6));
    Duration::from_secs(secs.min(MAX_RETRY_SECS))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ps() -> Arc<PeerSet> {
        PeerSet::new(TESTNET_MAGIC, 0, 10)
    }

    fn insert_connected(ps: &Arc<PeerSet>, addr: &str, height: i32) {
        {
            let mut inner = ps.inner.lock().unwrap();
            inner.slots.insert(addr.to_string(), PeerSlot::new(addr));
        }
        ps.mark_connected(addr, "test-agent/1.0", height);
    }

    // ── PeerSlot ──────────────────────────────────────────────────────────────

    #[test]
    fn new_slot_starts_as_disconnected_ready() {
        let slot = PeerSlot::new("1.2.3.4:8333");
        assert!(slot.status.is_ready_to_retry());
        assert_eq!(slot.height, 0);
        assert_eq!(slot.strikes, 0);
    }

    // ── PeerSet basic ops ─────────────────────────────────────────────────────

    #[test]
    fn peer_count_counts_only_connected() {
        let ps = make_ps();
        insert_connected(&ps, "1.1.1.1:8333", 100);
        insert_connected(&ps, "2.2.2.2:8333", 200);
        // Add a disconnected slot
        ps.inner.lock().unwrap().slots.insert(
            "3.3.3.3:8333".to_string(),
            PeerSlot::new("3.3.3.3:8333"),
        );
        assert_eq!(ps.peer_count(), 2);
    }

    #[test]
    fn best_height_max_of_connected_peers() {
        let ps = make_ps();
        insert_connected(&ps, "a:8333", 500);
        insert_connected(&ps, "b:8333", 1200);
        insert_connected(&ps, "c:8333", 800);
        assert_eq!(ps.best_height(), 1200);
    }

    #[test]
    fn best_height_zero_when_no_peers() {
        let ps = make_ps();
        assert_eq!(ps.best_height(), 0);
    }

    #[test]
    fn best_height_ignores_disconnected() {
        let ps = make_ps();
        insert_connected(&ps, "a:8333", 500);
        // Add disconnected slot with high height
        let mut slot = PeerSlot::new("b:8333");
        slot.height = 9999;
        ps.inner.lock().unwrap().slots.insert("b:8333".to_string(), slot);
        assert_eq!(ps.best_height(), 500);
    }

    // ── Ban ───────────────────────────────────────────────────────────────────

    #[test]
    fn ban_sets_banned_status() {
        let ps = make_ps();
        insert_connected(&ps, "peer1:8333", 0);
        ps.ban("peer1:8333", 3600);
        let inner = ps.inner.lock().unwrap();
        let slot  = inner.slots.get("peer1:8333").unwrap();
        assert!(slot.status.is_banned());
    }

    #[test]
    fn ban_expired_peer_is_not_banned() {
        let ps = make_ps();
        insert_connected(&ps, "peer1:8333", 0);
        // Ban with expiry in the past
        {
            let mut inner = ps.inner.lock().unwrap();
            inner.slots.get_mut("peer1:8333").unwrap().status =
                PeerStatus::Banned { until_unix: unix_now().saturating_sub(1) };
        }
        let inner = ps.inner.lock().unwrap();
        assert!(!inner.slots["peer1:8333"].status.is_banned());
    }

    #[test]
    fn banned_peer_not_counted() {
        let ps = make_ps();
        insert_connected(&ps, "peer1:8333", 100);
        ps.ban("peer1:8333", 3600);
        assert_eq!(ps.peer_count(), 0);
    }

    // ── Strike ────────────────────────────────────────────────────────────────

    #[test]
    fn three_strikes_auto_ban() {
        let ps = make_ps();
        insert_connected(&ps, "bad:8333", 0);
        ps.strike("bad:8333");
        ps.strike("bad:8333");
        // Sau 2 strikes vẫn Connected
        assert!(ps.inner.lock().unwrap().slots["bad:8333"].status.is_connected());
        ps.strike("bad:8333"); // 3rd strike → ban
        assert!(ps.inner.lock().unwrap().slots["bad:8333"].status.is_banned());
    }

    #[test]
    fn strike_resets_after_ban() {
        let ps = make_ps();
        insert_connected(&ps, "bad:8333", 0);
        for _ in 0..MAX_STRIKES { ps.strike("bad:8333"); }
        // strikes reset về 0 sau khi ban
        assert_eq!(ps.inner.lock().unwrap().slots["bad:8333"].strikes, 0);
    }

    #[test]
    fn strike_unknown_peer_noop() {
        let ps = make_ps();
        ps.strike("unknown:8333"); // không panic
    }

    // ── update_height ─────────────────────────────────────────────────────────

    #[test]
    fn update_height_only_increases() {
        let ps = make_ps();
        insert_connected(&ps, "peer:8333", 100);
        ps.update_height("peer:8333", 50);   // thấp hơn → không thay đổi
        assert_eq!(ps.inner.lock().unwrap().slots["peer:8333"].height, 100);
        ps.update_height("peer:8333", 200);  // cao hơn → cập nhật
        assert_eq!(ps.inner.lock().unwrap().slots["peer:8333"].height, 200);
    }

    // ── connected_addrs ───────────────────────────────────────────────────────

    #[test]
    fn connected_addrs_only_connected() {
        let ps = make_ps();
        insert_connected(&ps, "ok:8333", 10);
        ps.inner.lock().unwrap().slots.insert("disc:8333".to_string(), PeerSlot::new("disc:8333"));
        let addrs = ps.connected_addrs();
        assert_eq!(addrs, vec!["ok:8333".to_string()]);
    }

    // ── set_our_height ────────────────────────────────────────────────────────

    #[test]
    fn set_our_height_updates_atomic() {
        let ps = make_ps();
        ps.set_our_height(9999);
        assert_eq!(ps.our_height.load(Ordering::Relaxed), 9999);
    }

    // ── backoff ───────────────────────────────────────────────────────────────

    #[test]
    fn backoff_increases_with_attempt() {
        let d0 = backoff(0);
        let d1 = backoff(1);
        let d2 = backoff(2);
        assert!(d0 < d1);
        assert!(d1 < d2);
    }

    #[test]
    fn backoff_capped_at_max() {
        let d = backoff(100);
        assert_eq!(d.as_secs(), MAX_RETRY_SECS);
    }

    // ── PeerStatus helpers ────────────────────────────────────────────────────

    #[test]
    fn status_connected_is_connected() {
        assert!(PeerStatus::Connected.is_connected());
        assert!(!PeerStatus::Connecting.is_connected());
    }

    #[test]
    fn status_disconnected_ready_to_retry() {
        let s = PeerStatus::Disconnected {
            since:    Instant::now(),
            retry_at: Instant::now() - Duration::from_secs(1),
            attempt:  0,
        };
        assert!(s.is_ready_to_retry());
    }

    #[test]
    fn status_disconnected_not_ready() {
        let s = PeerStatus::Disconnected {
            since:    Instant::now(),
            retry_at: Instant::now() + Duration::from_secs(60),
            attempt:  1,
        };
        assert!(!s.is_ready_to_retry());
    }

    #[test]
    fn status_banned_active_is_banned() {
        let s = PeerStatus::Banned { until_unix: unix_now() + 3600 };
        assert!(s.is_banned());
        assert!(!s.is_ready_to_retry()); // active ban → chưa hết hạn → NOT ready
    }

    #[test]
    fn status_banned_expired_not_banned() {
        let s = PeerStatus::Banned { until_unix: unix_now().saturating_sub(1) };
        assert!(!s.is_banned());
    }
}
