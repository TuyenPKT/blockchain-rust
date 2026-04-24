#![allow(dead_code)]
//! v25.6 — Full Node Mode (in-process sync + P2P listener)
//!
//! Chạy sync + pktscan web server + P2P listener trong một process duy nhất.
//!
//! ```bash
//! blockchain-rust fullnode [port] [peer] [--mainnet] [--p2p-port N]
//! # Defaults: port=8081  peer=seed.testnet.oceif.com:8333  p2p=8333  testnet
//! ```
//!
//! ## Architecture
//!
//! ```
//! Tokio runtime  ──► pktscan_api::serve(port)           REST API + Web UI (async)
//! spawn_blocking  ──► loop { pkt_sync::run_sync(peer) }  sync loop (blocking thread)
//! thread          ──► run_pkt_node(p2p_port)             P2P listener (blocking thread)
//! ```
//!
//! v25.6: gộp blockchain-node.service vào fullnode — 1 process duy nhất,
//! không còn 2 process cùng ghi ~/.pkt/testnet/ → redb không bị conflict.

use std::sync::Arc;
use std::time::Duration;

const DEFAULT_PORT:        u16  = 8081;
const DEFAULT_P2P_PORT:    u16  = 8336;
const DEFAULT_RPC_PORT:    u16  = 8334;  // template server (miner)
const DEFAULT_POOL_PORT:   u16  = 8337;
const DEFAULT_STATS_PORT:  u16  = 8338;
const DEFAULT_PEER:        &str = "seed.testnet.oceif.com:8333";
const RESTART_DELAY_SECS:  u64  = 5;

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullnodeConfig {
    pub port:     u16,
    pub p2p_port: u16,
    pub rpc_port: u16,
    pub peer:     String,
    pub mainnet:  bool,
}

impl Default for FullnodeConfig {
    fn default() -> Self {
        FullnodeConfig {
            port:     DEFAULT_PORT,
            p2p_port: DEFAULT_P2P_PORT,
            rpc_port: DEFAULT_RPC_PORT,
            peer:     DEFAULT_PEER.to_string(),
            mainnet:  false,
        }
    }
}

/// Parse CLI args: `[port_u16] [host:port_peer] [--mainnet] [--p2p-port N]`
/// Thứ tự không quan trọng — nhận biết bằng type/format.
pub fn parse_fullnode_args(args: &[String]) -> FullnodeConfig {
    // --p2p-port N
    let p2p_port = args.windows(2)
        .find(|w| w[0] == "--p2p-port")
        .and_then(|w| w[1].parse::<u16>().ok())
        .unwrap_or(DEFAULT_P2P_PORT);

    // Port: first arg that parses as u16 (không phải sau --p2p-port)
    let skip_next: std::collections::HashSet<usize> = args.windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] == "--p2p-port")
        .map(|(i, _)| i + 1)
        .collect();
    let port = args.iter().enumerate()
        .filter(|(i, _)| !skip_next.contains(i))
        .find_map(|(_, a)| a.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    // Peer: first arg chứa ':' và không parse được thành u16
    let peer = args.iter()
        .find(|a| a.contains(':') && a.parse::<u16>().is_err())
        .cloned()
        .unwrap_or_else(|| DEFAULT_PEER.to_string());

    let rpc_port = args.windows(2)
        .find(|w| w[0] == "--rpc-port")
        .and_then(|w| w[1].parse::<u16>().ok())
        .unwrap_or(DEFAULT_RPC_PORT);

    let mainnet = args.iter().any(|a| a == "--mainnet");

    FullnodeConfig { port, p2p_port, rpc_port, peer, mainnet }
}

// ── CLI ───────────────────────────────────────────────────────────────────────

pub fn cmd_fullnode(args: &[String]) {
    let cfg = parse_fullnode_args(args);

    println!("[fullnode] port={}  p2p={}  peer={}  network={}",
        cfg.port, cfg.p2p_port, cfg.peer,
        if cfg.mainnet { "mainnet" } else { "testnet" });

    let peer      = cfg.peer.clone();
    let mainnet   = cfg.mainnet;
    let p2p_port  = cfg.p2p_port;
    let rpc_port  = cfg.rpc_port;

    let rt = tokio::runtime::Runtime::new()
        .expect("tokio runtime");

    rt.block_on(async move {
        // 1. P2P listener (8333) + template server (8334) — chia sẻ shared_chain
        //
        // shared_chain là nguồn sự thật duy nhất:
        //   - Template server nhận NewBlock từ miner → commit vào shared_chain + DB
        //   - P2P listener serve blocks từ shared_chain → sync đọc được block mới
        //   - Sync kết nối localhost:8333, nhận block, cập nhật secondary indexes
        //
        // Không còn self-loop vì P2P listener KHÔNG trống — nó phản ánh chain thật
        // (khởi tạo từ DB + được template server cập nhật real-time).
        let shared_chain = Arc::new(std::sync::Mutex::new(crate::storage::load_or_new()));
        let node_cfg = if mainnet {
            crate::pkt_node::NodeConfig::mainnet(p2p_port)
        } else {
            crate::pkt_node::NodeConfig::testnet(p2p_port)
        };
        let relay_hub     = crate::pkt_node::run_pkt_node(node_cfg, Arc::clone(&shared_chain));
        let template_port = rpc_port;
        {
            let chain_tmpl = Arc::clone(&shared_chain);
            let hub_tmpl   = Arc::clone(&relay_hub);
            std::thread::spawn(move || {
                crate::pkt_node::run_template_server(template_port, chain_tmpl, hub_tmpl);
            });
        }

        // 2. Pool proxy (8337) — external miners kết nối vào đây
        // Proxy tới template server (p2p_port+1) — chạy trong thread riêng.
        {
            let node_addr  = format!("127.0.0.1:{}", template_port);
            let pool_port  = DEFAULT_POOL_PORT;
            let stats_port = DEFAULT_STATS_PORT;
            std::thread::spawn(move || {
                crate::pkt_pool::run_pool(&node_addr, pool_port, stats_port);
            });
            println!("[fullnode] pool proxy on :{} → template :{}", pool_port, template_port);
        }

        // 3. Sync loop trong blocking thread (redb-safe: cùng process = shared Arc<Database>)
        // Sau mỗi lần sync terminate (kể cả DB reset), reload shared_chain từ DB
        // để P2P listener không serve stale tip → tránh "chain changed" loop.
        let shared_for_sync = Arc::clone(&shared_chain);
        tokio::task::spawn_blocking(move || {
            loop {
                crate::pkt_sync::run_sync(&peer, mainnet);
                // Reload shared_chain từ DB (sync có thể đã reset DB)
                if let Ok(mut chain) = shared_for_sync.lock() {
                    *chain = crate::storage::load_or_new();
                }
                eprintln!("[fullnode] sync terminated — restarting in {}s", RESTART_DELAY_SECS);
                std::thread::sleep(Duration::from_secs(RESTART_DELAY_SECS));
            }
        });

        // 3. pktscan REST API (async — chạy đến khi Ctrl+C)
        println!("[fullnode] web server on :{}", cfg.port);
        let bc = crate::storage::load_or_new();
        let db = Arc::new(tokio::sync::Mutex::new(bc));
        crate::pktscan_api::serve(db, cfg.port).await;
    });
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    // ── parse_fullnode_args ───────────────────────────────────────────────────

    #[test]
    fn test_defaults_no_args() {
        let cfg = parse_fullnode_args(&[]);
        assert_eq!(cfg.port,     DEFAULT_PORT);
        assert_eq!(cfg.p2p_port, DEFAULT_P2P_PORT);
        assert_eq!(cfg.peer,     DEFAULT_PEER);
        assert!(!cfg.mainnet);
    }

    #[test]
    fn test_custom_port() {
        let cfg = parse_fullnode_args(&sv(&["9090"]));
        assert_eq!(cfg.port,     9090);
        assert_eq!(cfg.p2p_port, DEFAULT_P2P_PORT);
        assert_eq!(cfg.peer,     DEFAULT_PEER);
    }

    #[test]
    fn test_custom_p2p_port() {
        let cfg = parse_fullnode_args(&sv(&["--p2p-port", "9333"]));
        assert_eq!(cfg.p2p_port, 9333);
        assert_eq!(cfg.port,     DEFAULT_PORT);
    }

    #[test]
    fn test_p2p_port_not_confused_with_api_port() {
        let cfg = parse_fullnode_args(&sv(&["8081", "--p2p-port", "9333"]));
        assert_eq!(cfg.port,     8081);
        assert_eq!(cfg.p2p_port, 9333);
    }

    #[test]
    fn test_default_port_when_peer_only() {
        let cfg = parse_fullnode_args(&sv(&["127.0.0.1:8333"]));
        assert_eq!(cfg.port, DEFAULT_PORT);
    }

    #[test]
    fn test_custom_peer() {
        let cfg = parse_fullnode_args(&sv(&["127.0.0.1:8333"]));
        assert_eq!(cfg.peer, "127.0.0.1:8333");
    }

    #[test]
    fn test_default_peer_when_port_only() {
        let cfg = parse_fullnode_args(&sv(&["8081"]));
        assert_eq!(cfg.peer, DEFAULT_PEER);
    }

    #[test]
    fn test_mainnet_flag() {
        let cfg = parse_fullnode_args(&sv(&["--mainnet"]));
        assert!(cfg.mainnet);
    }

    #[test]
    fn test_no_mainnet_flag_defaults_testnet() {
        let cfg = parse_fullnode_args(&sv(&["8081", "peer:8333"]));
        assert!(!cfg.mainnet);
    }

    #[test]
    fn test_all_args_together() {
        let cfg = parse_fullnode_args(&sv(&["8082", "127.0.0.1:8333", "--mainnet", "--p2p-port", "9333"]));
        assert_eq!(cfg.port,     8082);
        assert_eq!(cfg.p2p_port, 9333);
        assert_eq!(cfg.peer,     "127.0.0.1:8333");
        assert!(cfg.mainnet);
    }

    #[test]
    fn test_port_before_peer() {
        let cfg = parse_fullnode_args(&sv(&["9000", "127.0.0.1:8333"]));
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.peer, "127.0.0.1:8333");
    }

    #[test]
    fn test_peer_before_port() {
        let cfg = parse_fullnode_args(&sv(&["127.0.0.1:8333", "9000"]));
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.peer, "127.0.0.1:8333");
    }

    #[test]
    fn test_mainnet_flag_first() {
        let cfg = parse_fullnode_args(&sv(&["--mainnet", "8090", "127.0.0.1:8333"]));
        assert!(cfg.mainnet);
        assert_eq!(cfg.port, 8090);
        assert_eq!(cfg.peer, "127.0.0.1:8333");
    }

    #[test]
    fn test_default_instance() {
        let cfg = FullnodeConfig::default();
        assert_eq!(cfg.port,     DEFAULT_PORT);
        assert_eq!(cfg.p2p_port, DEFAULT_P2P_PORT);
        assert_eq!(cfg.peer,     DEFAULT_PEER);
        assert!(!cfg.mainnet);
    }

    #[test]
    fn test_port_65535_accepted() {
        let cfg = parse_fullnode_args(&sv(&["65535"]));
        assert_eq!(cfg.port, 65535);
    }

    #[test]
    fn test_port_1_accepted() {
        let cfg = parse_fullnode_args(&sv(&["1"]));
        assert_eq!(cfg.port, 1);
    }

}
