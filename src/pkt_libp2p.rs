#![allow(dead_code)]
//! v19.5 — libp2p Transport Layer
//!
//! Cung cấp P2P transport layer dựa trên libp2p cho PKTCore inter-node:
//!   - TCP transport + Noise encryption + Yamux multiplexing
//!   - mDNS local peer discovery
//!   - Identify protocol (`/pkt/1.0.0`)
//!   - Ping keepalive
//!   - PeerManager: score-based reputation + automatic ban
//!
//! **Lưu ý:** Module này song song với `pkt_node` / `pkt_sync` (raw TCP ↔ pktd).
//! `pkt_node` / `pkt_sync` vẫn dùng cho testnet sync.
//! `pkt_libp2p` phục vụ PKTCore node-to-node với Noise encryption.
//!
//! ## Build
//!
//! ```bash
//! cargo build --features libp2p-transport   # compile PktP2pNode
//! cargo build                               # chỉ compile PeerManager (không kéo libp2p)
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};

use futures::StreamExt;
use libp2p::{
    identify, mdns, noise, ping, tcp, yamux,
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, SwarmBuilder,
};

// ── Protocol identifier ───────────────────────────────────────────────────────

pub const PKT_LIBP2P_PROTOCOL: &str = "/pkt/1.0.0";

// ── PeerManager ───────────────────────────────────────────────────────────────

/// Score event để điều chỉnh reputation của peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreEvent {
    /// Peer gửi block/tx hợp lệ.
    ValidBlock,
    /// Peer gửi data không hợp lệ / malformed.
    InvalidData,
    /// Peer timeout hoặc disconnect đột ngột.
    Timeout,
    /// Peer gửi duplicate / spam.
    Spam,
    /// Kết nối thành công (initial bonus).
    Connected,
}

impl ScoreEvent {
    fn delta(&self) -> i32 {
        match self {
            Self::ValidBlock  =>  10,
            Self::Connected   =>   5,
            Self::Timeout     => -10,
            Self::Spam        => -20,
            Self::InvalidData => -50,
        }
    }
}

/// Thông tin score + trạng thái của một peer.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub addr:         String,
    pub score:        i32,
    pub banned_until: Option<Instant>,
    pub first_seen:   Instant,
}

impl PeerInfo {
    fn new(addr: &str) -> Self {
        PeerInfo {
            addr:         addr.to_string(),
            score:        0,
            banned_until: None,
            first_seen:   Instant::now(),
        }
    }

    pub fn is_banned(&self) -> bool {
        self.banned_until.map(|t| Instant::now() < t).unwrap_or(false)
    }
}

/// Quản lý danh sách peers, score, và ban list.
pub struct PeerManager {
    peers:         HashMap<String, PeerInfo>,
    ban_threshold: i32,
    ban_duration:  Duration,
}

impl PeerManager {
    pub fn new() -> Self {
        PeerManager {
            peers:         HashMap::new(),
            ban_threshold: -100,
            ban_duration:  Duration::from_secs(3600),
        }
    }

    pub fn with_config(ban_threshold: i32, ban_duration: Duration) -> Self {
        PeerManager { peers: HashMap::new(), ban_threshold, ban_duration }
    }

    pub fn add_peer(&mut self, addr: &str) {
        self.peers.entry(addr.to_string())
            .or_insert_with(|| PeerInfo::new(addr));
    }

    pub fn remove_peer(&mut self, addr: &str) {
        self.peers.remove(addr);
    }

    pub fn record_event(&mut self, addr: &str, event: ScoreEvent) {
        let threshold    = self.ban_threshold;
        let ban_duration = self.ban_duration;
        let peer = self.peers.entry(addr.to_string())
            .or_insert_with(|| PeerInfo::new(addr));
        peer.score += event.delta();
        if peer.score < threshold && !peer.is_banned() {
            peer.banned_until = Some(Instant::now() + ban_duration);
        }
    }

    pub fn is_banned(&self, addr: &str) -> bool {
        self.peers.get(addr).map(|p| p.is_banned()).unwrap_or(false)
    }

    pub fn score(&self, addr: &str) -> i32 {
        self.peers.get(addr).map(|p| p.score).unwrap_or(0)
    }

    pub fn active_peers(&self) -> Vec<&PeerInfo> {
        self.peers.values().filter(|p| !p.is_banned()).collect()
    }

    pub fn active_count(&self) -> usize {
        self.peers.values().filter(|p| !p.is_banned()).count()
    }

    pub fn banned_count(&self) -> usize {
        self.peers.values().filter(|p| p.is_banned()).count()
    }

    pub fn total_count(&self) -> usize {
        self.peers.len()
    }
}

impl Default for PeerManager {
    fn default() -> Self { Self::new() }
}

// ── libp2p Behaviour + PktP2pNode (chỉ compile khi --features libp2p-transport) ──

#[derive(NetworkBehaviour)]
pub struct PktBehaviour {
    pub ping:     ping::Behaviour,
    pub identify: identify::Behaviour,
    pub mdns:     mdns::tokio::Behaviour,
}

pub struct PktP2pNode {
    swarm: libp2p::Swarm<PktBehaviour>,
}

impl PktP2pNode {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let swarm = SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| {
                Ok(PktBehaviour {
                    ping: ping::Behaviour::default(),
                    identify: identify::Behaviour::new(
                        identify::Config::new(
                            PKT_LIBP2P_PROTOCOL.to_string(),
                            key.public(),
                        ),
                    ),
                    mdns: mdns::tokio::Behaviour::new(
                        mdns::Config::default(),
                        key.public().to_peer_id(),
                    )?,
                })
            })?
            .build();
        Ok(PktP2pNode { swarm })
    }

    pub fn listen(&mut self, port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port).parse()?;
        self.swarm.listen_on(addr)?;
        Ok(())
    }

    pub fn dial(&mut self, multiaddr: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr: Multiaddr = multiaddr.parse()?;
        self.swarm.dial(addr)?;
        Ok(())
    }

    pub fn local_peer_id(&self) -> &libp2p::PeerId {
        self.swarm.local_peer_id()
    }

    pub async fn run_event_loop(&mut self, pm: &mut PeerManager) {
        loop {
            match self.swarm.select_next_some().await {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("[libp2p] listening on {}", address);
                }
                SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                    let addr = endpoint.get_remote_address().to_string();
                    pm.add_peer(&addr);
                    pm.record_event(&addr, ScoreEvent::Connected);
                    println!("[libp2p] connected: {} at {}", peer_id, addr);
                }
                SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                    let reason = cause.map(|e| e.to_string())
                        .unwrap_or_else(|| "clean".to_string());
                    println!("[libp2p] disconnected: {} ({})", peer_id, reason);
                }
                SwarmEvent::Behaviour(PktBehaviourEvent::Mdns(
                    mdns::Event::Discovered(list)
                )) => {
                    for (peer_id, addr) in list {
                        println!("[libp2p] mDNS discovered: {} at {}", peer_id, addr);
                        let _ = self.swarm.dial(addr);
                    }
                }
                SwarmEvent::Behaviour(PktBehaviourEvent::Mdns(
                    mdns::Event::Expired(list)
                )) => {
                    for (peer_id, _addr) in list {
                        println!("[libp2p] mDNS expired: {}", peer_id);
                    }
                }
                SwarmEvent::Behaviour(PktBehaviourEvent::Ping(event)) => {
                    match event.result {
                        Ok(rtt) => println!("[libp2p] ping {} → {}ms", event.peer, rtt.as_millis()),
                        Err(e)  => println!("[libp2p] ping failed {}: {}", event.peer, e),
                    }
                }
                SwarmEvent::Behaviour(PktBehaviourEvent::Identify(
                    identify::Event::Received { peer_id, info }
                )) => {
                    println!(
                        "[libp2p] identified: {} proto={} agent={}",
                        peer_id, info.protocol_version, info.agent_version
                    );
                }
                _ => {}
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_event_deltas_correct() {
        assert_eq!(ScoreEvent::ValidBlock.delta(),   10);
        assert_eq!(ScoreEvent::Connected.delta(),     5);
        assert_eq!(ScoreEvent::Timeout.delta(),     -10);
        assert_eq!(ScoreEvent::Spam.delta(),        -20);
        assert_eq!(ScoreEvent::InvalidData.delta(), -50);
    }

    #[test]
    fn peer_manager_add_and_count() {
        let mut pm = PeerManager::new();
        pm.add_peer("1.2.3.4:8333");
        pm.add_peer("2.3.4.5:8333");
        assert_eq!(pm.total_count(), 2);
        assert_eq!(pm.active_count(), 2);
        assert_eq!(pm.banned_count(), 0);
    }

    #[test]
    fn peer_manager_add_duplicate_is_noop() {
        let mut pm = PeerManager::new();
        pm.add_peer("1.2.3.4:8333");
        pm.add_peer("1.2.3.4:8333");
        assert_eq!(pm.total_count(), 1);
    }

    #[test]
    fn peer_manager_remove_peer() {
        let mut pm = PeerManager::new();
        pm.add_peer("1.2.3.4:8333");
        pm.remove_peer("1.2.3.4:8333");
        assert_eq!(pm.total_count(), 0);
    }

    #[test]
    fn peer_manager_score_increases_with_valid_block() {
        let mut pm = PeerManager::new();
        pm.record_event("p1", ScoreEvent::ValidBlock);
        pm.record_event("p1", ScoreEvent::ValidBlock);
        assert_eq!(pm.score("p1"), 20);
    }

    #[test]
    fn peer_manager_connected_gives_initial_score() {
        let mut pm = PeerManager::new();
        pm.record_event("p1", ScoreEvent::Connected);
        assert_eq!(pm.score("p1"), 5);
    }

    #[test]
    fn peer_manager_invalid_data_reduces_score() {
        let mut pm = PeerManager::new();
        pm.record_event("p1", ScoreEvent::InvalidData);
        assert_eq!(pm.score("p1"), -50);
    }

    #[test]
    fn peer_manager_auto_ban_below_threshold() {
        let mut pm = PeerManager::with_config(-30, Duration::from_secs(60));
        pm.record_event("bad-peer", ScoreEvent::InvalidData);
        assert!(pm.is_banned("bad-peer"));
        assert_eq!(pm.active_count(), 0);
        assert_eq!(pm.banned_count(), 1);
    }

    #[test]
    fn peer_manager_no_ban_above_threshold() {
        let mut pm = PeerManager::with_config(-100, Duration::from_secs(60));
        pm.record_event("p1", ScoreEvent::Timeout);
        assert!(!pm.is_banned("p1"));
    }

    #[test]
    fn peer_manager_unknown_peer_not_banned() {
        let pm = PeerManager::new();
        assert!(!pm.is_banned("unknown"));
        assert_eq!(pm.score("unknown"), 0);
    }

    #[test]
    fn peer_manager_active_peers_excludes_banned() {
        let mut pm = PeerManager::with_config(-30, Duration::from_secs(60));
        pm.add_peer("good");
        pm.record_event("bad", ScoreEvent::InvalidData);
        let active: Vec<_> = pm.active_peers();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].addr, "good");
    }

    #[test]
    fn peer_info_not_banned_by_default() {
        let info = PeerInfo::new("1.2.3.4:9000");
        assert!(!info.is_banned());
    }

    #[test]
    fn peer_manager_default_equals_new() {
        let pm = PeerManager::default();
        assert_eq!(pm.total_count(), 0);
        assert_eq!(pm.ban_threshold, -100);
    }

    #[test]
    fn peer_manager_record_event_creates_peer_if_missing() {
        let mut pm = PeerManager::new();
        pm.record_event("new-peer", ScoreEvent::ValidBlock);
        assert_eq!(pm.total_count(), 1);
        assert_eq!(pm.score("new-peer"), 10);
    }

    #[test]
    fn peer_manager_multiple_events_cumulative() {
        let mut pm = PeerManager::new();
        pm.record_event("p1", ScoreEvent::ValidBlock);  // +10
        pm.record_event("p1", ScoreEvent::ValidBlock);  // +10
        pm.record_event("p1", ScoreEvent::Timeout);     // -10
        pm.record_event("p1", ScoreEvent::Spam);        // -20
        assert_eq!(pm.score("p1"), -10);
        assert!(!pm.is_banned("p1"));
    }
}
