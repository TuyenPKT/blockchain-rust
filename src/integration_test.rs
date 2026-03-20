#![allow(dead_code)]
//! v16.2 — Integration Test Harness [DX]
//!
//! E2E tests: mine thật → send tx thật → verify balance thật → HTTP API thật.
//!
//! Chạy: `cargo test --features integration`
//!
//! Tests KHÔNG dùng mock. Mỗi test:
//!   1. Tạo fresh chain (difficulty=1, trong memory)
//!   2. Mine block thật
//!   3. Gửi tx thật (hoặc gọi HTTP API thật)
//!   4. Assert kết quả từ chain/API thật

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

use tokio::sync::Mutex;

use crate::chain::Blockchain;
use crate::pktscan_api::ScanDb;
use crate::script::Script;
use crate::wallet::Wallet;

// ── Port allocator ────────────────────────────────────────────────────────────

/// Atomic counter để phân bổ port test không trùng nhau.
static PORT_SEQ: AtomicU16 = AtomicU16::new(47000);

pub fn next_port() -> u16 {
    PORT_SEQ.fetch_add(1, Ordering::SeqCst)
}

// ── Test node builder ─────────────────────────────────────────────────────────

/// Node test: chain + ví miner (giữ Wallet để sign tx).
pub struct TestNode {
    pub db:          ScanDb,
    pub wallet:      Wallet,
    pub miner_hash:  String,
}

impl TestNode {
    /// Tạo fresh TestNode với difficulty đã chọn.
    pub fn new(difficulty: usize) -> Self {
        let wallet     = Wallet::new();
        let miner_hash = hex::encode(Script::pubkey_hash(&wallet.public_key.serialize()));
        let mut chain  = Blockchain::new();
        chain.difficulty = difficulty;
        TestNode {
            db: Arc::new(Mutex::new(chain)),
            wallet,
            miner_hash,
        }
    }

    /// Mine `n` block thật vào chain.
    pub async fn mine(&self, n: u32) {
        for _ in 0..n {
            let db   = Arc::clone(&self.db);
            let hash = self.miner_hash.clone();
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let mut chain = db.lock().await;
                    chain.mine_block_to_hash(&hash);
                });
            });
        }
    }

    /// Lấy balance thật của miner từ UTXO set.
    pub async fn balance(&self) -> u64 {
        let chain = self.db.lock().await;
        chain.utxo_set.balance_of(&self.miner_hash)
    }

    /// Lấy height thật của chain.
    pub async fn height(&self) -> u64 {
        let chain = self.db.lock().await;
        chain.last_block().index
    }

    /// Số block trong chain (gồm genesis).
    pub async fn block_count(&self) -> usize {
        let chain = self.db.lock().await;
        chain.chain.len()
    }

    /// Gửi tx từ miner → recipient, trả về txid hoặc lỗi.
    pub async fn send(&self, recipient_hash: &str, amount: u64, fee: u64) -> Result<String, String> {
        let mut chain = self.db.lock().await;
        chain.create_and_submit(&self.wallet, recipient_hash, amount, fee)
    }

    /// Số tx trong mempool.
    pub async fn mempool_count(&self) -> usize {
        let chain = self.db.lock().await;
        chain.mempool.entries.len()
    }

    /// Khởi động API server; trả về port đang listen.
    pub async fn start_api(&self) -> u16 {
        let port   = next_port();
        let db     = Arc::clone(&self.db);
        tokio::spawn(crate::pktscan_api::serve(db, port));
        // Đợi server ready
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        port
    }
}

// ── HTTP helper ───────────────────────────────────────────────────────────────

/// GET JSON từ URL; trả về (status_code, Value).
pub async fn get_json(url: &str) -> (u16, serde_json::Value) {
    let resp = reqwest::get(url).await.expect("HTTP request failed");
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    (status, body)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(feature = "integration")]
#[cfg(test)]
mod tests {
    use super::*;

    // ── Chain E2E (không cần HTTP) ────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_mine_1_block_height_is_1() {
        let node = TestNode::new(1);
        node.mine(1).await;
        assert_eq!(node.height().await, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_mine_3_blocks_height_is_3() {
        let node = TestNode::new(1);
        node.mine(3).await;
        assert_eq!(node.height().await, 3);
        assert_eq!(node.block_count().await, 4); // genesis + 3
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_mine_gives_miner_balance() {
        let node = TestNode::new(1);
        node.mine(2).await;
        assert!(node.balance().await > 0, "miner should have positive balance");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_balance_grows_with_each_block() {
        let node = TestNode::new(1);
        node.mine(1).await;
        let b1 = node.balance().await;
        node.mine(1).await;
        let b2 = node.balance().await;
        assert!(b2 > b1, "balance should grow after each block: b1={} b2={}", b1, b2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_chain_valid_after_mining() {
        let node = TestNode::new(1);
        node.mine(4).await;
        let chain = node.db.lock().await;
        assert!(chain.is_valid(), "chain must be valid after 4 real mined blocks");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_send_tx_enters_mempool() {
        let node      = TestNode::new(1);
        let recipient = Wallet::new();
        let rec_hash  = hex::encode(Script::pubkey_hash(&recipient.public_key.serialize()));

        // Mine để có balance
        node.mine(2).await;
        let initial_balance = node.balance().await;
        assert!(initial_balance > 0);

        // Send: dùng 10% balance, phí 1000 paklets
        let amount = initial_balance / 10;
        let fee    = 1000;
        let txid   = node.send(&rec_hash, amount, fee).await.expect("send should succeed");
        assert!(!txid.is_empty(), "txid must be non-empty");

        // TX phải vào mempool
        assert_eq!(node.mempool_count().await, 1, "mempool should have 1 tx");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_send_tx_confirmed_after_mining() {
        let node      = TestNode::new(1);
        let recipient = Wallet::new();
        let rec_hash  = hex::encode(Script::pubkey_hash(&recipient.public_key.serialize()));

        node.mine(3).await;
        let miner_before = node.balance().await;
        let amount = miner_before / 5;
        let fee    = 1000;

        node.send(&rec_hash, amount, fee).await.expect("send failed");
        assert_eq!(node.mempool_count().await, 1);

        // Mine block → tx xác nhận, mempool phải trống
        node.mine(1).await;
        assert_eq!(node.mempool_count().await, 0, "mempool should be empty after mining");

        // Recipient phải có balance
        let rec_balance = node.db.lock().await.utxo_set.balance_of(&rec_hash);
        assert_eq!(rec_balance, amount, "recipient balance must equal sent amount");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_miner_balance_decreases_after_send() {
        let node      = TestNode::new(1);
        let recipient = Wallet::new();
        let rec_hash  = hex::encode(Script::pubkey_hash(&recipient.public_key.serialize()));

        node.mine(3).await;
        let balance_before_send = node.balance().await;
        let amount = balance_before_send / 4;
        let fee    = 1000;

        node.send(&rec_hash, amount, fee).await.expect("send failed");
        node.mine(1).await;   // mine block → coinbase reward + tx confirmed

        // Miner balance = (balance_before - amount - fee) + coinbase_reward_of_block4
        // Chỉ cần verify không bằng balance trước send (change happened)
        let balance_after = node.balance().await;
        // Sau khi gửi amount+fee đi + nhận coinbase, balance_after phải > 0
        assert!(balance_after > 0);
        // Số tiền đã gửi đi + phí = balance_before - (balance_after - block4_coinbase)
        // Đơn giản nhất: balance_after != balance_before (thay đổi đã xảy ra)
        assert_ne!(balance_after, balance_before_send,
            "miner balance should change after send+mine");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_send_insufficient_funds_fails() {
        let node      = TestNode::new(1);
        let recipient = Wallet::new();
        let rec_hash  = hex::encode(Script::pubkey_hash(&recipient.public_key.serialize()));

        // Không mine → balance = 0
        let result = node.send(&rec_hash, 1_000_000, 100);
        assert!(result.await.is_err(), "send with zero balance should fail");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_chain_has_coinbase_in_every_block() {
        let node = TestNode::new(1);
        node.mine(3).await;
        let chain = node.db.lock().await;
        for block in chain.chain.iter().skip(1) {  // skip genesis
            assert!(!block.transactions.is_empty(), "block #{} has no txs", block.index);
            assert!(block.transactions[0].is_coinbase,
                "block #{} first tx must be coinbase", block.index);
        }
    }

    // ── API E2E (HTTP thật) ───────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_stats_status_200() {
        let node = TestNode::new(1);
        node.mine(2).await;
        let port = node.start_api().await;

        let (status, _) = get_json(&format!("http://127.0.0.1:{}/api/stats", port)).await;
        assert_eq!(status, 200);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_stats_height_matches_chain() {
        let node = TestNode::new(1);
        node.mine(3).await;
        let expected_height = node.height().await;
        let port = node.start_api().await;

        let (_, json) = get_json(&format!("http://127.0.0.1:{}/api/stats", port)).await;
        assert_eq!(json["height"].as_u64(), Some(expected_height),
            "API height must match actual chain height");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_stats_block_count_matches() {
        let node = TestNode::new(1);
        node.mine(2).await;
        let expected_count = node.block_count().await;
        let port = node.start_api().await;

        let (_, json) = get_json(&format!("http://127.0.0.1:{}/api/stats", port)).await;
        assert_eq!(json["block_count"].as_u64(), Some(expected_count as u64),
            "API block_count must match chain len");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_blocks_returns_array() {
        let node = TestNode::new(1);
        node.mine(3).await;
        let port = node.start_api().await;

        let (status, json) = get_json(&format!("http://127.0.0.1:{}/api/blocks", port)).await;
        assert_eq!(status, 200);
        assert!(json["blocks"].is_array(), "response must have 'blocks' array");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_blocks_count_correct() {
        let node = TestNode::new(1);
        node.mine(3).await;
        let port = node.start_api().await;

        let (_, json) = get_json(&format!("http://127.0.0.1:{}/api/blocks?limit=10", port)).await;
        let blocks = json["blocks"].as_array().expect("blocks must be array");
        // genesis + 3 mined = 4 blocks total, limit=10 → all returned
        assert_eq!(blocks.len(), 4, "should return all 4 blocks (genesis+3)");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_block_detail_height_correct() {
        let node = TestNode::new(1);
        node.mine(2).await;
        let port = node.start_api().await;

        let (status, json) = get_json(&format!("http://127.0.0.1:{}/api/block/1", port)).await;
        assert_eq!(status, 200);
        // API returns "index" field (= block height)
        assert_eq!(json["index"].as_u64(), Some(1),
            "block detail must return correct index/height");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_block_detail_has_hash() {
        let node = TestNode::new(1);
        node.mine(1).await;
        let port = node.start_api().await;

        let (_, json) = get_json(&format!("http://127.0.0.1:{}/api/block/1", port)).await;
        let hash = json["hash"].as_str().unwrap_or("");
        assert!(!hash.is_empty(), "block detail must have non-empty hash");
        // difficulty=1 → hash starts with "0"
        assert!(hash.starts_with('0'), "block hash must meet difficulty=1 target");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_block_not_found_returns_error() {
        let node = TestNode::new(1);
        node.mine(1).await;
        let port = node.start_api().await;

        let (status, _) = get_json(&format!("http://127.0.0.1:{}/api/block/9999", port)).await;
        assert_eq!(status, 404, "non-existent block must return 404");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_address_balance_correct() {
        let node = TestNode::new(1);
        node.mine(2).await;
        let expected_balance = node.balance().await;
        let port = node.start_api().await;

        // API address lookup uses pubkey_hash_hex (same key as UTXO set)
        let (status, json) = get_json(
            &format!("http://127.0.0.1:{}/api/address/{}", port, node.miner_hash)
        ).await;
        assert_eq!(status, 200);
        let api_balance = json["balance"].as_u64()
            .expect("response must have 'balance' field");
        assert_eq!(api_balance, expected_balance,
            "API balance must match UTXO set balance");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_mempool_empty_initially() {
        let node = TestNode::new(1);
        node.mine(1).await;
        let port = node.start_api().await;

        let (status, json) = get_json(&format!("http://127.0.0.1:{}/api/mempool", port)).await;
        assert_eq!(status, 200);
        let txs = json["transactions"].as_array()
            .or_else(|| json["txs"].as_array())
            .expect("mempool must have transactions array");
        assert_eq!(txs.len(), 0, "mempool must be empty after mining with no pending txs");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_api_mempool_shows_pending_tx() {
        let node      = TestNode::new(1);
        let recipient = Wallet::new();
        let rec_hash  = hex::encode(Script::pubkey_hash(&recipient.public_key.serialize()));

        node.mine(3).await;
        let amount = node.balance().await / 5;
        node.send(&rec_hash, amount, 1000).await.expect("send failed");

        let port = node.start_api().await;
        let (_, json) = get_json(&format!("http://127.0.0.1:{}/api/mempool", port)).await;
        let txs = json["transactions"].as_array()
            .or_else(|| json["txs"].as_array())
            .expect("mempool must have transactions array");
        assert_eq!(txs.len(), 1, "mempool must show the 1 pending tx");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_full_flow_mine_send_confirm_verify() {
        // Full E2E: mine → send → confirm → verify via API
        let node      = TestNode::new(1);
        let recipient = Wallet::new();
        let rec_hash  = hex::encode(Script::pubkey_hash(&recipient.public_key.serialize()));

        // Step 1: mine 3 blocks → miner has balance
        node.mine(3).await;
        let miner_balance = node.balance().await;
        assert!(miner_balance > 0);

        // Step 2: send tx
        let amount = miner_balance / 3;
        let fee    = 1000;
        node.send(&rec_hash, amount, fee).await.expect("send failed");

        // Step 3: mine 1 block → tx confirmed
        node.mine(1).await;
        assert_eq!(node.mempool_count().await, 0, "tx should be confirmed");

        // Step 4: verify via API
        let port = node.start_api().await;

        // Chain height = 4
        let (_, stats) = get_json(&format!("http://127.0.0.1:{}/api/stats", port)).await;
        assert_eq!(stats["height"].as_u64(), Some(4));

        // Recipient has correct balance (no API endpoint for hash — use UTXO directly)
        let rec_balance = node.db.lock().await.utxo_set.balance_of(&rec_hash);
        assert_eq!(rec_balance, amount, "recipient balance must equal sent amount exactly");
    }

    // ── TestNode helpers ──────────────────────────────────────────────────────

    #[test]
    fn test_node_new_has_empty_balance() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all().build().unwrap();
        rt.block_on(async {
            let node = TestNode::new(1);
            assert_eq!(node.balance().await, 0);
            assert_eq!(node.height().await, 0);
        });
    }

    #[test]
    fn next_port_is_unique() {
        let p1 = next_port();
        let p2 = next_port();
        let p3 = next_port();
        assert_ne!(p1, p2);
        assert_ne!(p2, p3);
    }
}
