#![allow(dead_code)]
//! v23.2 — Block + TX Relay
//!
//! Sau khi một block/tx được validate và commit, `RelayHub` broadcast `Inv`
//! tới tất cả connected peers (trừ nguồn gửi đến).
//!
//! Architecture:
//! ```text
//! [new block/tx accepted]
//!        │
//!        ▼
//!   RelayHub::broadcast_block(hash, except)
//!        │
//!        ├──► Sender<RelayEvent> ──► write_thread(peer A) ──► TcpStream → Inv
//!        ├──► Sender<RelayEvent> ──► write_thread(peer B) ──► TcpStream → Inv
//!        └──► Sender<RelayEvent> ──► write_thread(peer C) ──► TcpStream → Inv
//! ```
//!
//! Mỗi peer connection đăng ký 1 `Sender` qua `relay_hub.register(addr)`.
//! Write-thread của peer đọc từ `Receiver` và gửi `PktMsg::Inv` ra wire.
//! Khi peer disconnect, `relay_hub.deregister(addr)` loại bỏ sender.

use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver, Sender};

use sha2::{Digest, Sha256};

use crate::pkt_wire::{InvItem, INV_MSG_BLOCK, INV_MSG_TX};

// ── RelayEvent ────────────────────────────────────────────────────────────────

/// Event được broadcast tới tất cả connected peers.
#[derive(Debug, Clone)]
pub enum RelayEvent {
    /// Một block mới đã được validate — hash là wire SHA256d hash (32 bytes).
    NewBlock { hash: [u8; 32] },
    /// Một tx mới đã được validate — hash là txid (32 bytes).
    NewTx    { hash: [u8; 32] },
}

impl RelayEvent {
    /// Chuyển thành `InvItem` để đóng gói trong `PktMsg::Inv`.
    pub fn to_inv_item(&self) -> InvItem {
        match self {
            RelayEvent::NewBlock { hash } => InvItem { inv_type: INV_MSG_BLOCK, hash: *hash },
            RelayEvent::NewTx    { hash } => InvItem { inv_type: INV_MSG_TX,    hash: *hash },
        }
    }
}

// ── RelayHub ──────────────────────────────────────────────────────────────────

/// Shared broadcast hub — giữ một `Sender<RelayEvent>` per connected peer.
///
/// Clone `Arc<RelayHub>` để share across threads.
#[derive(Clone)]
pub struct RelayHub {
    inner: Arc<Mutex<RelayHubInner>>,
}

struct RelayHubInner {
    /// (peer_addr, sender) — sender bị loại khỏi list nếu receiver đã bị drop.
    senders: Vec<(String, Sender<RelayEvent>)>,
}

impl Default for RelayHub {
    fn default() -> Self {
        Self::new()
    }
}

impl RelayHub {
    pub fn new() -> Self {
        RelayHub {
            inner: Arc::new(Mutex::new(RelayHubInner { senders: Vec::new() })),
        }
    }

    /// Đăng ký peer mới — trả về `Receiver` để write-thread của peer drain.
    /// Nếu `addr` đã tồn tại, sender cũ bị thay thế.
    pub fn register(&self, addr: &str) -> Receiver<RelayEvent> {
        let (tx, rx) = mpsc::channel();
        let mut inner = self.inner.lock().unwrap();
        inner.senders.retain(|(a, _)| a != addr);
        inner.senders.push((addr.to_string(), tx));
        rx
    }

    /// Huỷ đăng ký peer sau khi disconnect.
    pub fn deregister(&self, addr: &str) {
        self.inner.lock().unwrap().senders.retain(|(a, _)| a != addr);
    }

    /// Broadcast `NewBlock` tới tất cả peers, trừ `except_addr` (nguồn gửi đến).
    pub fn broadcast_block(&self, hash: [u8; 32], except_addr: Option<&str>) {
        self.broadcast(RelayEvent::NewBlock { hash }, except_addr);
    }

    /// Broadcast `NewTx` tới tất cả peers, trừ `except_addr`.
    pub fn broadcast_tx(&self, hash: [u8; 32], except_addr: Option<&str>) {
        self.broadcast(RelayEvent::NewTx { hash }, except_addr);
    }

    fn broadcast(&self, event: RelayEvent, except_addr: Option<&str>) {
        let mut inner = self.inner.lock().unwrap();
        inner.senders.retain(|(addr, tx)| {
            if except_addr.map(|e| e == addr.as_str()).unwrap_or(false) {
                return true; // giữ sender của nguồn, chỉ skip gửi
            }
            tx.send(event.clone()).is_ok() // loại sender nếu receiver đã drop
        });
    }

    /// Số lượng peers đang đăng ký.
    pub fn peer_count(&self) -> usize {
        self.inner.lock().unwrap().senders.len()
    }

    /// Danh sách địa chỉ peers đang đăng ký.
    pub fn peer_addrs(&self) -> Vec<String> {
        self.inner.lock().unwrap().senders.iter().map(|(a, _)| a.clone()).collect()
    }
}

// ── Wire hash helpers ─────────────────────────────────────────────────────────

/// Tính SHA256d (double SHA-256) của 80-byte block header từ wire payload.
/// Trả về `None` nếu payload < 80 bytes.
pub fn wire_block_hash(block_payload: &[u8]) -> Option<[u8; 32]> {
    if block_payload.len() < 80 { return None; }
    let round1 = Sha256::digest(&block_payload[..80]);
    let round2 = Sha256::digest(round1);
    Some(round2.into())
}

/// Parse txid từ raw tx bytes: SHA256d của toàn bộ raw tx.
pub fn wire_tx_hash(raw_tx: &[u8]) -> [u8; 32] {
    let round1 = Sha256::digest(raw_tx);
    let round2 = Sha256::digest(round1);
    round2.into()
}

// ── SeenHashes ────────────────────────────────────────────────────────────────

/// Bounded set của wire hashes đã thấy — tránh relay lại những gì đã biết.
/// Giới hạn `cap` entries; khi đầy, xoá nửa cũ nhất (FIFO order).
pub struct SeenHashes {
    hashes: std::collections::HashSet<[u8; 32]>,
    order:  std::collections::VecDeque<[u8; 32]>,
    cap:    usize,
}

impl SeenHashes {
    pub fn new(cap: usize) -> Self {
        SeenHashes {
            hashes: std::collections::HashSet::with_capacity(cap),
            order:  std::collections::VecDeque::with_capacity(cap),
            cap,
        }
    }

    /// Trả về `true` nếu hash đã được thấy trước đó (không insert lại).
    /// Trả về `false` nếu hash mới — tự động insert.
    pub fn insert(&mut self, hash: [u8; 32]) -> bool {
        if self.hashes.contains(&hash) {
            return true; // đã thấy
        }
        if self.hashes.len() >= self.cap {
            // Xoá nửa cũ nhất
            let drain_count = self.cap / 2;
            for _ in 0..drain_count {
                if let Some(old) = self.order.pop_front() {
                    self.hashes.remove(&old);
                }
            }
        }
        self.hashes.insert(hash);
        self.order.push_back(hash);
        false // hash mới
    }

    pub fn contains(&self, hash: &[u8; 32]) -> bool {
        self.hashes.contains(hash)
    }

    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_hash(n: u8) -> [u8; 32] {
        let mut h = [0u8; 32];
        h[0] = n;
        h
    }

    // ── RelayHub tests ────────────────────────────────────────────────────────

    #[test]
    fn register_receive_block_event() {
        let hub = RelayHub::new();
        let rx = hub.register("peer1");
        hub.broadcast_block(dummy_hash(1), None);
        let ev = rx.recv().unwrap();
        match ev {
            RelayEvent::NewBlock { hash } => assert_eq!(hash[0], 1),
            _ => panic!("expected NewBlock"),
        }
    }

    #[test]
    fn register_receive_tx_event() {
        let hub = RelayHub::new();
        let rx = hub.register("peer1");
        hub.broadcast_tx(dummy_hash(7), None);
        match rx.recv().unwrap() {
            RelayEvent::NewTx { hash } => assert_eq!(hash[0], 7),
            _ => panic!("expected NewTx"),
        }
    }

    #[test]
    fn broadcast_excludes_source_peer() {
        let hub = RelayHub::new();
        let rx_a = hub.register("peer_a");
        let rx_b = hub.register("peer_b");

        // broadcast from peer_a → peer_a không nhận, peer_b nhận
        hub.broadcast_block(dummy_hash(2), Some("peer_a"));

        assert!(rx_a.try_recv().is_err(), "source peer must not receive");
        assert!(rx_b.try_recv().is_ok(), "other peer must receive");
    }

    #[test]
    fn deregister_removes_peer() {
        let hub = RelayHub::new();
        let _rx = hub.register("peer1");
        assert_eq!(hub.peer_count(), 1);
        hub.deregister("peer1");
        assert_eq!(hub.peer_count(), 0);
    }

    #[test]
    fn dropped_receiver_cleaned_on_next_broadcast() {
        let hub = RelayHub::new();
        {
            let _rx = hub.register("peer1");
            assert_eq!(hub.peer_count(), 1);
            // rx dropped khi ra khỏi scope
        }
        // Sau broadcast, sender detect receiver dropped → tự cleanup
        hub.broadcast_block(dummy_hash(3), None);
        assert_eq!(hub.peer_count(), 0);
    }

    #[test]
    fn multiple_peers_all_receive() {
        let hub = RelayHub::new();
        let rxs: Vec<_> = (0..5).map(|i| hub.register(&format!("peer{}", i))).collect();
        hub.broadcast_block(dummy_hash(42), None);
        for rx in &rxs {
            assert!(rx.try_recv().is_ok());
        }
    }

    #[test]
    fn peer_addrs_returns_registered() {
        let hub = RelayHub::new();
        let _r1 = hub.register("1.2.3.4:8333");
        let _r2 = hub.register("5.6.7.8:8333");
        let mut addrs = hub.peer_addrs();
        addrs.sort();
        assert_eq!(addrs, vec!["1.2.3.4:8333", "5.6.7.8:8333"]);
    }

    #[test]
    fn re_register_replaces_old_sender() {
        let hub = RelayHub::new();
        let _rx_old = hub.register("peer1");
        // Drop old rx, re-register same addr
        let rx_new = hub.register("peer1");
        assert_eq!(hub.peer_count(), 1); // vẫn 1 peer

        hub.broadcast_block(dummy_hash(9), None);
        assert!(rx_new.try_recv().is_ok());
    }

    // ── RelayEvent tests ──────────────────────────────────────────────────────

    #[test]
    fn relay_event_to_inv_item_block() {
        let ev = RelayEvent::NewBlock { hash: dummy_hash(1) };
        let item = ev.to_inv_item();
        assert_eq!(item.inv_type, INV_MSG_BLOCK);
        assert_eq!(item.hash[0], 1);
    }

    #[test]
    fn relay_event_to_inv_item_tx() {
        let ev = RelayEvent::NewTx { hash: dummy_hash(2) };
        let item = ev.to_inv_item();
        assert_eq!(item.inv_type, INV_MSG_TX);
        assert_eq!(item.hash[0], 2);
    }

    // ── wire_block_hash tests ─────────────────────────────────────────────────

    #[test]
    fn wire_block_hash_requires_80_bytes() {
        assert!(wire_block_hash(&[0u8; 79]).is_none());
        assert!(wire_block_hash(&[0u8; 80]).is_some());
        assert!(wire_block_hash(&[0u8; 200]).is_some()); // chỉ dùng 80 đầu
    }

    #[test]
    fn wire_block_hash_deterministic() {
        let payload = vec![0xab; 80];
        let h1 = wire_block_hash(&payload).unwrap();
        let h2 = wire_block_hash(&payload).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn wire_block_hash_different_payloads() {
        let mut p1 = vec![0u8; 80];
        let mut p2 = vec![0u8; 80];
        p1[0] = 1;
        p2[0] = 2;
        assert_ne!(wire_block_hash(&p1), wire_block_hash(&p2));
    }

    #[test]
    fn wire_tx_hash_deterministic() {
        let raw = b"fake raw tx bytes";
        let h1 = wire_tx_hash(raw);
        let h2 = wire_tx_hash(raw);
        assert_eq!(h1, h2);
    }

    // ── SeenHashes tests ──────────────────────────────────────────────────────

    #[test]
    fn seen_hashes_new_returns_false() {
        let mut seen = SeenHashes::new(100);
        assert!(!seen.insert(dummy_hash(1)));
    }

    #[test]
    fn seen_hashes_duplicate_returns_true() {
        let mut seen = SeenHashes::new(100);
        seen.insert(dummy_hash(1));
        assert!(seen.insert(dummy_hash(1)));
    }

    #[test]
    fn seen_hashes_evicts_when_full() {
        let cap = 10;
        let mut seen = SeenHashes::new(cap);
        for i in 0..cap as u8 {
            seen.insert(dummy_hash(i));
        }
        assert_eq!(seen.len(), cap);

        // Thêm 1 phần tử mới → trigger eviction (xoá 5 cũ nhất)
        seen.insert(dummy_hash(100));
        assert!(seen.len() < cap);
    }

    #[test]
    fn seen_hashes_contains_after_insert() {
        let mut seen = SeenHashes::new(100);
        seen.insert(dummy_hash(5));
        assert!(seen.contains(&dummy_hash(5)));
        assert!(!seen.contains(&dummy_hash(6)));
    }
}
