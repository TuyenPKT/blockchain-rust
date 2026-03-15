#![allow(dead_code)]

/// v4.7 — Testnet Config & Network Parameters
///
/// Định nghĩa các tham số mạng cho từng network type:
///   - Regtest  : local development, difficulty=1, instant mining
///   - Testnet  : public test network, difficulty=3, port 18333
///   - Mainnet  : production (placeholder), difficulty=5, port 8333
///
/// GenesisConfig: tham số tạo genesis block
/// NetworkParams: coin params, port, magic bytes, bootstrap peers

use serde::{Serialize, Deserialize};

// ─── Network type ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkType {
    Regtest,
    Testnet,
    Mainnet,
}

impl std::fmt::Display for NetworkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkType::Regtest => write!(f, "regtest"),
            NetworkType::Testnet => write!(f, "testnet"),
            NetworkType::Mainnet => write!(f, "mainnet"),
        }
    }
}

// ─── Network parameters ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkParams {
    pub network:            NetworkType,
    /// 4-byte magic để phân biệt network (giống Bitcoin)
    pub magic:              [u8; 4],
    /// Tên hiển thị
    pub name:               String,
    /// Ticker symbol
    pub ticker:             String,
    /// Port P2P mặc định
    pub p2p_port:           u16,
    /// Port REST API mặc định
    pub api_port:           u16,
    /// Difficulty ban đầu
    pub initial_difficulty: usize,
    /// Block reward (paklets = satoshi equivalent)
    pub block_reward:       u64,
    /// 1 PKT = N paklets
    pub paklets_per_pkt:    u64,
    /// Hard cap tổng supply (0 = không giới hạn)
    pub max_supply:         u64,
    /// Tần suất điều chỉnh difficulty (số blocks)
    pub difficulty_interval: u64,
    /// Block time mục tiêu (giây)
    pub block_time_secs:    u64,
    /// Bootstrap peers (host:port)
    pub bootstrap_peers:    Vec<String>,
    /// Genesis message (giống Bitcoin's "The Times 03/Jan/2009...")
    pub genesis_message:    String,
}

// ─── Predefined configs ───────────────────────────────────────────────────────

/// Regtest — local development
/// Difficulty 1, port 18444, không có bootstrap peers
pub fn regtest() -> NetworkParams {
    NetworkParams {
        network:            NetworkType::Regtest,
        magic:              [0xfa, 0xbf, 0xb5, 0xda],
        name:               "PKT Regtest".to_string(),
        ticker:             "rPKT".to_string(),
        p2p_port:           18444,
        api_port:           18445,
        initial_difficulty: 1,
        block_reward:       5_000_000_000,
        paklets_per_pkt:    100_000_000,
        max_supply:         0,
        difficulty_interval: 5,
        block_time_secs:    1,
        bootstrap_peers:    vec![],
        genesis_message:    "PKT Regtest — local development chain".to_string(),
    }
}

/// Testnet — public test network
pub fn testnet() -> NetworkParams {
    NetworkParams {
        network:            NetworkType::Testnet,
        magic:              [0x0b, 0x11, 0x09, 0x07],
        name:               "PKT Testnet".to_string(),
        ticker:             "tPKT".to_string(),
        p2p_port:           18333,
        api_port:           18334,
        initial_difficulty: 3,
        block_reward:       5_000_000_000,
        paklets_per_pkt:    100_000_000,
        max_supply:         0,
        difficulty_interval: 5,
        block_time_secs:    10,
        bootstrap_peers:    vec![
            "seed.testnet.oceif.com:18333".to_string(),
        ],
        genesis_message:    "PKT Testnet genesis — 2031-01-01".to_string(),
    }
}

/// Mainnet — production (placeholder, chưa launch)
pub fn mainnet() -> NetworkParams {
    NetworkParams {
        network:            NetworkType::Mainnet,
        magic:              [0xf9, 0xbe, 0xb4, 0xd9],
        name:               "PKT".to_string(),
        ticker:             "PKT".to_string(),
        p2p_port:           8333,
        api_port:           8334,
        initial_difficulty: 5,
        block_reward:       5_000_000_000,
        paklets_per_pkt:    100_000_000,
        max_supply:         0,
        difficulty_interval: 5,
        block_time_secs:    10,
        bootstrap_peers:    vec![
            // Khi có domain: "seed.pkt.cash:8333"
        ],
        genesis_message:    "PKT Mainnet — bandwidth-hard PoW for a better internet".to_string(),
    }
}

/// Chọn config theo tên
pub fn by_name(name: &str) -> Option<NetworkParams> {
    match name {
        "regtest"  => Some(regtest()),
        "testnet"  => Some(testnet()),
        "mainnet"  => Some(mainnet()),
        _          => None,
    }
}

// ─── Genesis block builder ────────────────────────────────────────────────────

use crate::block::Block;

/// Tạo genesis block theo NetworkParams
/// Genesis message được encode vào prev_hash field (không dùng được như chain thật,
/// nhưng đủ để identify network — giống Bitcoin's "Chancellor on brink..." trong coinbase)
pub fn build_genesis(params: &NetworkParams) -> Block {
    use sha2::{Sha256, Digest};
    // Encode genesis message thành prev_hash hex (64 chars)
    let msg_hash = {
        let mut h = Sha256::new();
        h.update(params.genesis_message.as_bytes());
        hex::encode(h.finalize())
    };
    let mut genesis = Block::new(0, vec![], msg_hash);
    // Mine với difficulty của network
    let target = "0".repeat(params.initial_difficulty);
    loop {
        let hash = Block::calculate_hash(
            genesis.index, genesis.timestamp,
            &genesis.transactions, &genesis.prev_hash, genesis.nonce,
        );
        if hash.starts_with(&target) {
            genesis.hash = hash;
            break;
        }
        genesis.nonce += 1;
    }
    genesis
}

// ─── Testnet simulation ───────────────────────────────────────────────────────

use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use crate::node::Node;
use crate::message::Message;

/// Chạy local testnet với N nodes trên các cổng liên tiếp
/// Kết nối thành chuỗi: node0 ← node1 ← node2 ... ← nodeN-1
pub async fn run_local_testnet(n_nodes: usize, base_port: u16, miner_addr: &str) {
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           🧪  PKT Local Testnet ({} nodes)                   ║", n_nodes);
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Tạo và start tất cả nodes
    let mut nodes: Vec<Arc<Node>> = Vec::new();
    for i in 0..n_nodes {
        let port = base_port + i as u16;
        let node = Arc::new(Node::new(port));
        let n    = Arc::clone(&node);
        tokio::spawn(async move { n.start().await });
        nodes.push(node);
        println!("  ✅ Node {} started on port {}", i, port);
    }

    // Đợi nodes lắng nghe
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Kết nối thành chuỗi: node[i] biết node[i-1]
    for i in 1..n_nodes {
        let peer_addr = format!("127.0.0.1:{}", base_port + (i - 1) as u16);
        let hello = Message::Hello {
            version: 1,
            host:    "127.0.0.1".to_string(),
            port:    base_port + i as u16,
        };
        if let Some(_) = Node::send_to_peer(&peer_addr, &hello).await {
            nodes[i].peers.lock().await.push(peer_addr.clone());
            println!("  🔗 Node {} → Node {} ({})", i, i-1, peer_addr);
        }
    }
    println!();

    // Mine 3 blocks trên node 0
    let miner_addr = miner_addr.to_string();
    println!("  ⛏  Mining 3 blocks trên Node 0...");
    for block_num in 1..=3u64 {
        nodes[0].mine_and_broadcast(&miner_addr).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        println!("  ✅ Block #{} mined & broadcast", block_num);
    }
    println!();

    // Đợi propagation
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // Kiểm tra chain height của tất cả nodes
    println!("  📊 Chain status:");
    for (i, node) in nodes.iter().enumerate() {
        let height = node.chain.lock().await.chain.len() - 1;
        let peers  = node.peers.lock().await.len();
        println!("    Node {} (port {}) | height={} | peers={}",
            i, base_port + i as u16, height, peers);
    }

    // Kiểm tra đồng bộ
    let heights: Vec<usize> = {
        let mut h = Vec::new();
        for node in &nodes {
            h.push(node.chain.lock().await.chain.len() - 1);
        }
        h
    };
    let all_synced = heights.iter().all(|&h| h == heights[0]);
    println!();
    if all_synced {
        println!("  ✅ Tất cả {} nodes đã đồng bộ tại height={}", n_nodes, heights[0]);
    } else {
        println!("  ⚠️  Nodes chưa đồng bộ: {:?}", heights);
        println!("     (Bình thường nếu propagation chưa xong — thử tăng sleep)");
    }
    println!();
}

// ─── Shared Arc<Mutex<Blockchain>> (dùng cho testnet + api cùng lúc) ──────────

use crate::chain::Blockchain;

pub type SharedChain = Arc<TokioMutex<Blockchain>>;

pub fn new_shared_chain() -> SharedChain {
    Arc::new(TokioMutex::new(Blockchain::new()))
}
