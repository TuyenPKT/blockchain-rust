#![allow(dead_code)]
//! v13.2 — PKT Bandwidth Incentive Layer
//!
//! OCEIF Bandwidth Incentive Layer — khuyến khích routing bandwidth thật sự qua mạng:
//!   - Nodes submit `BandwidthProof` — chứng minh đã route packets
//!   - `AnnouncerPool` tích lũy reward từ block reward
//!   - Cuối mỗi epoch, pool phân phối reward tỉ lệ theo bytes đã route
//!   - `RouteAnnouncement` — node công bố khả năng routing đến destination
//!   - `BandwidthLedger` — per-node accounting: packets, bytes, earned

use std::collections::HashMap;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Phần trăm block reward vào announcer pool (50%)
pub const ANNOUNCER_REWARD_PCT: u64 = 50;
/// Số bytes tối thiểu trong một proof hợp lệ
pub const MIN_PROOF_BYTES: u64 = 512;
/// Số blocks mỗi epoch (phân phối reward mỗi epoch)
pub const EPOCH_BLOCKS: u64 = 256;
/// Số tối đa RouteAnnouncements mỗi node
pub const MAX_ROUTES_PER_NODE: usize = 32;
/// Route announcement hết hạn sau N blocks
pub const ROUTE_EXPIRY_BLOCKS: u64 = 128;

// ── Bandwidth Proof ───────────────────────────────────────────────────────────

/// Node submit proof để chứng minh đã route traffic
#[derive(Debug, Clone)]
pub struct BandwidthProof {
    /// Node ID (pubkey hash 32 bytes)
    pub node_id: [u8; 32],
    /// BLAKE3(node_id + packet_count + byte_count + block_height + nonce)
    pub proof_hash: [u8; 32],
    /// Số packets đã route
    pub packet_count: u32,
    /// Số bytes đã route
    pub byte_count: u64,
    /// Block height khi submit
    pub block_height: u64,
    /// Nonce để tạo proof_hash hợp lệ
    pub nonce: u32,
}

impl BandwidthProof {
    /// Tính proof_hash từ các fields
    pub fn compute_hash(
        node_id: &[u8; 32],
        packet_count: u32,
        byte_count: u64,
        block_height: u64,
        nonce: u32,
    ) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"PktBandwidth_v1:");
        h.update(node_id);
        h.update(&packet_count.to_le_bytes());
        h.update(&byte_count.to_le_bytes());
        h.update(&block_height.to_le_bytes());
        h.update(&nonce.to_le_bytes());
        *h.finalize().as_bytes()
    }

    /// Tạo proof mới (tự tính hash)
    pub fn new(
        node_id: [u8; 32],
        packet_count: u32,
        byte_count: u64,
        block_height: u64,
    ) -> Self {
        let nonce = (block_height as u32).wrapping_add(packet_count);
        let proof_hash = Self::compute_hash(
            &node_id, packet_count, byte_count, block_height, nonce,
        );
        BandwidthProof { node_id, proof_hash, packet_count, byte_count, block_height, nonce }
    }

    /// Xác minh proof hợp lệ
    pub fn verify(&self) -> bool {
        if self.byte_count < MIN_PROOF_BYTES { return false; }
        if self.packet_count == 0 { return false; }
        let expected = Self::compute_hash(
            &self.node_id, self.packet_count, self.byte_count,
            self.block_height, self.nonce,
        );
        expected == self.proof_hash
    }

    pub fn node_hex(&self) -> String { hex::encode(self.node_id) }
}

// ── Route Announcement ────────────────────────────────────────────────────────

/// Node công bố khả năng routing đến một destination
#[derive(Debug, Clone)]
pub struct RouteAnnouncement {
    /// Node ID
    pub node_id: [u8; 32],
    /// Destination prefix (ví dụ: "192.168.1.0/24" hoặc domain)
    pub destination: String,
    /// Bandwidth cam kết (Mbps)
    pub bandwidth_mbps: u32,
    /// Hết hạn tại block height này
    pub valid_until: u64,
    /// Block height khi announce
    pub announced_at: u64,
}

impl RouteAnnouncement {
    pub fn new(
        node_id: [u8; 32],
        destination: &str,
        bandwidth_mbps: u32,
        current_height: u64,
    ) -> Self {
        RouteAnnouncement {
            node_id,
            destination: destination.to_string(),
            bandwidth_mbps,
            valid_until: current_height + ROUTE_EXPIRY_BLOCKS,
            announced_at: current_height,
        }
    }

    pub fn is_active(&self, current_height: u64) -> bool {
        current_height <= self.valid_until
    }
}

// ── Per-node Stats ────────────────────────────────────────────────────────────

/// Thống kê per-node
#[derive(Debug, Clone, Default)]
pub struct NodeStats {
    pub total_packets: u64,
    pub total_bytes: u64,
    pub total_earned: u64,  // paklets
    pub proof_count: u32,
    pub last_proof_height: u64,
}

impl NodeStats {
    pub fn apply_proof(&mut self, proof: &BandwidthProof) {
        self.total_packets   += proof.packet_count as u64;
        self.total_bytes     += proof.byte_count;
        self.proof_count     += 1;
        self.last_proof_height = proof.block_height;
    }

    pub fn throughput_mbps(&self, elapsed_blocks: u64) -> f64 {
        if elapsed_blocks == 0 { return 0.0; }
        // Ước tính: bytes / (blocks * 60s) → Mbps
        let secs = elapsed_blocks as f64 * 60.0;
        self.total_bytes as f64 * 8.0 / secs / 1_000_000.0
    }
}

// ── Announcer Pool ────────────────────────────────────────────────────────────

/// Pool tích lũy và phân phối reward cho bandwidth providers
#[derive(Debug, Clone)]
pub struct AnnouncerPool {
    /// Số dư hiện tại (paklets)
    pub balance: u64,
    /// Tổng đã phân phối
    pub total_distributed: u64,
    /// Epoch hiện tại
    pub epoch: u64,
}

impl AnnouncerPool {
    pub fn new() -> Self {
        AnnouncerPool { balance: 0, total_distributed: 0, epoch: 0 }
    }

    /// Tính phần reward vào pool từ block reward
    pub fn pool_amount(block_reward: u64) -> u64 {
        block_reward * ANNOUNCER_REWARD_PCT / 100
    }

    /// Thêm funds vào pool
    pub fn fund(&mut self, amount: u64) {
        self.balance = self.balance.saturating_add(amount);
    }

    /// Phân phối pool reward tỉ lệ theo bytes trong epoch
    /// Trả về map node_id_hex → amount paid
    pub fn distribute(&mut self, proofs: &[BandwidthProof]) -> HashMap<String, u64> {
        let mut payouts: HashMap<String, u64> = HashMap::new();
        if self.balance == 0 || proofs.is_empty() { return payouts; }

        // Tổng bytes từ proofs hợp lệ
        let total_bytes: u64 = proofs.iter()
            .filter(|p| p.verify())
            .map(|p| p.byte_count)
            .sum();
        if total_bytes == 0 { return payouts; }

        let pool = self.balance;
        let mut paid = 0u64;
        // Tính payout tỉ lệ
        for proof in proofs.iter().filter(|p| p.verify()) {
            let share = pool.saturating_mul(proof.byte_count) / total_bytes;
            let hex   = proof.node_hex();
            *payouts.entry(hex).or_insert(0) += share;
            paid += share;
        }
        self.balance     = self.balance.saturating_sub(paid);
        self.total_distributed += paid;
        self.epoch += 1;
        payouts
    }
}

// ── Bandwidth Ledger ──────────────────────────────────────────────────────────

/// Ledger tổng hợp: proofs, routes, stats, pool
pub struct BandwidthLedger {
    pub pool: AnnouncerPool,
    /// Proofs trong epoch hiện tại (reset sau distribute)
    pub epoch_proofs: Vec<BandwidthProof>,
    /// Route table: node_id_hex → routes
    pub routes: HashMap<String, Vec<RouteAnnouncement>>,
    /// Per-node stats tích lũy
    pub node_stats: HashMap<String, NodeStats>,
    /// Block height hiện tại
    pub current_height: u64,
}

impl BandwidthLedger {
    pub fn new() -> Self {
        BandwidthLedger {
            pool:           AnnouncerPool::new(),
            epoch_proofs:   Vec::new(),
            routes:         HashMap::new(),
            node_stats:     HashMap::new(),
            current_height: 0,
        }
    }

    /// Xử lý block mới: fund pool, nhận proofs, distribute nếu đủ epoch
    /// Trả về payouts nếu là cuối epoch, None nếu không
    pub fn process_block(
        &mut self,
        block_height: u64,
        block_reward: u64,
        proofs: Vec<BandwidthProof>,
    ) -> Option<HashMap<String, u64>> {
        self.current_height = block_height;

        // Fund pool
        let pool_amt = AnnouncerPool::pool_amount(block_reward);
        self.pool.fund(pool_amt);

        // Collect valid proofs + update stats
        for proof in proofs {
            if proof.verify() {
                let hex = proof.node_hex();
                self.node_stats
                    .entry(hex)
                    .or_default()
                    .apply_proof(&proof);
                self.epoch_proofs.push(proof);
            }
        }

        // Distribute at end of epoch
        if block_height > 0 && block_height % EPOCH_BLOCKS == 0 {
            let proofs_snapshot = std::mem::take(&mut self.epoch_proofs);
            let payouts = self.pool.distribute(&proofs_snapshot);
            // Ghi payouts vào node_stats
            for (hex, amount) in &payouts {
                self.node_stats.entry(hex.clone()).or_default().total_earned += amount;
            }
            return Some(payouts);
        }
        None
    }

    /// Thêm route announcement
    pub fn announce_route(&mut self, ann: RouteAnnouncement) -> Result<(), String> {
        let hex = hex::encode(ann.node_id);
        let entry = self.routes.entry(hex).or_default();
        if entry.len() >= MAX_ROUTES_PER_NODE {
            return Err(format!("Max {} routes per node reached", MAX_ROUTES_PER_NODE));
        }
        // Loại bỏ route cũ cùng destination
        entry.retain(|r| r.destination != ann.destination);
        entry.push(ann);
        Ok(())
    }

    /// Lấy routes đang active cho một node
    pub fn active_routes(&self, node_id: &[u8; 32]) -> Vec<&RouteAnnouncement> {
        let hex = hex::encode(node_id);
        let h   = self.current_height;
        self.routes.get(&hex)
            .map(|rs| rs.iter().filter(|r| r.is_active(h)).collect())
            .unwrap_or_default()
    }

    /// Tìm nodes có thể route đến destination
    pub fn find_routes(&self, destination: &str) -> Vec<(&RouteAnnouncement, &NodeStats)> {
        let h = self.current_height;
        self.routes.values()
            .flat_map(|rs| rs.iter())
            .filter(|r| r.is_active(h) && r.destination == destination)
            .filter_map(|r| {
                let hex = hex::encode(r.node_id);
                self.node_stats.get(&hex).map(|s| (r, s))
            })
            .collect()
    }

    /// Thống kê tổng hợp
    pub fn summary(&self) -> LedgerSummary {
        let total_bytes: u64 = self.node_stats.values().map(|s| s.total_bytes).sum();
        let total_earned: u64 = self.node_stats.values().map(|s| s.total_earned).sum();
        LedgerSummary {
            active_nodes: self.node_stats.len(),
            total_bytes_routed: total_bytes,
            pool_balance: self.pool.balance,
            total_distributed: self.pool.total_distributed,
            total_earned,
            epoch: self.pool.epoch,
            current_height: self.current_height,
        }
    }
}

#[derive(Debug)]
pub struct LedgerSummary {
    pub active_nodes: usize,
    pub total_bytes_routed: u64,
    pub pool_balance: u64,
    pub total_distributed: u64,
    pub total_earned: u64,
    pub epoch: u64,
    pub current_height: u64,
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u8) -> [u8; 32] { [id; 32] }
    fn proof(node_id: [u8;32], bytes: u64, height: u64) -> BandwidthProof {
        BandwidthProof::new(node_id, 10, bytes, height)
    }

    // ── BandwidthProof ────────────────────────────────────────────────────────

    #[test]
    fn test_proof_verify_ok() {
        let p = proof(node(1), 1024, 10);
        assert!(p.verify());
    }

    #[test]
    fn test_proof_verify_too_few_bytes() {
        let p = proof(node(1), 100, 10); // < MIN_PROOF_BYTES
        assert!(!p.verify());
    }

    #[test]
    fn test_proof_tampered_fails() {
        let mut p = proof(node(1), 1024, 10);
        p.byte_count += 1; // tamper
        assert!(!p.verify());
    }

    #[test]
    fn test_proof_node_hex_length() {
        let p = proof(node(1), 1024, 10);
        assert_eq!(p.node_hex().len(), 64);
    }

    // ── RouteAnnouncement ─────────────────────────────────────────────────────

    #[test]
    fn test_route_active_within_expiry() {
        let r = RouteAnnouncement::new(node(1), "10.0.0.0/8", 100, 50);
        assert!(r.is_active(50));
        assert!(r.is_active(50 + ROUTE_EXPIRY_BLOCKS));
    }

    #[test]
    fn test_route_expired_after_window() {
        let r = RouteAnnouncement::new(node(1), "10.0.0.0/8", 100, 50);
        assert!(!r.is_active(50 + ROUTE_EXPIRY_BLOCKS + 1));
    }

    // ── AnnouncerPool ─────────────────────────────────────────────────────────

    #[test]
    fn test_pool_amount_50_pct() {
        assert_eq!(AnnouncerPool::pool_amount(1000), 500);
    }

    #[test]
    fn test_pool_fund_and_balance() {
        let mut pool = AnnouncerPool::new();
        pool.fund(1000);
        assert_eq!(pool.balance, 1000);
    }

    #[test]
    fn test_pool_distribute_proportional() {
        let mut pool = AnnouncerPool::new();
        pool.fund(1000);
        let proofs = vec![
            proof(node(1), 3000, 10),
            proof(node(2), 1000, 10),
        ];
        let payouts = pool.distribute(&proofs);
        let p1 = *payouts.get(&hex::encode(node(1))).unwrap_or(&0);
        let p2 = *payouts.get(&hex::encode(node(2))).unwrap_or(&0);
        // node1 có 3x bytes → 3x reward
        assert!(p1 > p2);
        assert_eq!(p1 + p2, 1000);
    }

    #[test]
    fn test_pool_distribute_empty_proofs_no_change() {
        let mut pool = AnnouncerPool::new();
        pool.fund(500);
        pool.distribute(&[]);
        assert_eq!(pool.balance, 500); // unchanged
    }

    #[test]
    fn test_pool_epoch_increments_on_distribute() {
        let mut pool = AnnouncerPool::new();
        pool.fund(100);
        pool.distribute(&[proof(node(1), 1024, 1)]);
        assert_eq!(pool.epoch, 1);
    }

    #[test]
    fn test_pool_total_distributed_accumulates() {
        let mut pool = AnnouncerPool::new();
        pool.fund(1000);
        pool.distribute(&[proof(node(1), 1024, 1)]);
        assert!(pool.total_distributed > 0);
    }

    // ── NodeStats ─────────────────────────────────────────────────────────────

    #[test]
    fn test_node_stats_apply_proof() {
        let mut s = NodeStats::default();
        let p = proof(node(1), 2048, 5);
        s.apply_proof(&p);
        assert_eq!(s.total_bytes, 2048);
        assert_eq!(s.total_packets, 10);
        assert_eq!(s.proof_count, 1);
    }

    // ── BandwidthLedger ───────────────────────────────────────────────────────

    #[test]
    fn test_ledger_process_block_funds_pool() {
        let mut ledger = BandwidthLedger::new();
        ledger.process_block(1, 1000, vec![]);
        assert_eq!(ledger.pool.balance, 500); // 50% of 1000
    }

    #[test]
    fn test_ledger_valid_proof_updates_stats() {
        let mut ledger = BandwidthLedger::new();
        let p = proof(node(1), 1024, 1);
        ledger.process_block(1, 1000, vec![p.clone()]);
        let stats = &ledger.node_stats[&p.node_hex()];
        assert_eq!(stats.total_bytes, 1024);
    }

    #[test]
    fn test_ledger_invalid_proof_ignored() {
        let mut ledger = BandwidthLedger::new();
        let mut p = proof(node(1), 1024, 1);
        p.byte_count += 999; // tamper
        ledger.process_block(1, 1000, vec![p]);
        assert!(ledger.node_stats.is_empty());
    }

    #[test]
    fn test_ledger_distribute_at_epoch() {
        let mut ledger = BandwidthLedger::new();
        let p = proof(node(1), 1024, 1);
        // Submit proofs before epoch end
        for h in 1..EPOCH_BLOCKS {
            ledger.process_block(h, 1000, vec![p.clone()]);
        }
        // Epoch boundary
        let payouts = ledger.process_block(EPOCH_BLOCKS, 1000, vec![p]);
        assert!(payouts.is_some());
        assert!(!payouts.unwrap().is_empty());
    }

    #[test]
    fn test_ledger_no_distribute_mid_epoch() {
        let mut ledger = BandwidthLedger::new();
        let result = ledger.process_block(1, 1000, vec![]);
        assert!(result.is_none());
    }

    #[test]
    fn test_ledger_announce_route_ok() {
        let mut ledger = BandwidthLedger::new();
        let r = RouteAnnouncement::new(node(1), "10.0.0.0/8", 100, 0);
        assert!(ledger.announce_route(r).is_ok());
    }

    #[test]
    fn test_ledger_active_routes_filtered() {
        let mut ledger = BandwidthLedger::new();
        ledger.current_height = 200;
        // Route expired
        let r_old = RouteAnnouncement::new(node(1), "10.0.0.0/8", 50, 0);
        // Route active
        let r_new = RouteAnnouncement::new(node(1), "192.168.0.0/16", 100, 200);
        ledger.announce_route(r_old).unwrap();
        ledger.announce_route(r_new).unwrap();
        let active = ledger.active_routes(&node(1));
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].destination, "192.168.0.0/16");
    }

    #[test]
    fn test_ledger_find_routes() {
        let mut ledger = BandwidthLedger::new();
        ledger.process_block(1, 1000, vec![proof(node(1), 1024, 1)]);
        ledger.announce_route(
            RouteAnnouncement::new(node(1), "pkt.cash", 200, 1)
        ).unwrap();
        ledger.current_height = 2;
        let found = ledger.find_routes("pkt.cash");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn test_ledger_summary() {
        let mut ledger = BandwidthLedger::new();
        ledger.process_block(1, 1000, vec![proof(node(1), 1024, 1)]);
        let s = ledger.summary();
        assert_eq!(s.active_nodes, 1);
        assert_eq!(s.total_bytes_routed, 1024);
        assert_eq!(s.pool_balance, 500);
    }
}
