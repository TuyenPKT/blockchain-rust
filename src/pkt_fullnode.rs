#![allow(dead_code)]
//! v25.4 — Full Node Mode (in-process sync)
//!
//! Chạy sync + pktscan web server trong một process duy nhất.
//!
//! ```bash
//! blockchain-rust fullnode [port] [peer] [--mainnet]
//! # Defaults: port=8081  peer=seed.testnet.oceif.com:8333  testnet
//! ```
//!
//! ## Architecture
//!
//! ```
//! Tokio runtime  ──► pktscan_api::serve(port)          REST API + Web UI (async)
//! spawn_blocking  ──► loop { pkt_sync::run_sync(peer) } sync loop (blocking thread)
//! ```
//!
//! v25.4: sync chạy trong blocking thread (tokio::task::spawn_blocking) thay vì
//! OS subprocess. Cùng process → DB_REGISTRY chia sẻ Arc<Database> → redb hoạt động.

use std::sync::Arc;
use std::time::Duration;

const DEFAULT_PORT:        u16  = 8081;
const DEFAULT_PEER:        &str = "seed.testnet.oceif.com:8333";
const RESTART_DELAY_SECS:  u64  = 5;

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullnodeConfig {
    pub port:    u16,
    pub peer:    String,
    pub mainnet: bool,
}

impl Default for FullnodeConfig {
    fn default() -> Self {
        FullnodeConfig {
            port:    DEFAULT_PORT,
            peer:    DEFAULT_PEER.to_string(),
            mainnet: false,
        }
    }
}

/// Parse CLI args: `[port_u16] [host:port_peer] [--mainnet]`
/// Thứ tự không quan trọng — nhận biết bằng type/format.
pub fn parse_fullnode_args(args: &[String]) -> FullnodeConfig {
    // Port: first arg that parses as u16
    let port = args.iter()
        .find_map(|a| a.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    // Peer: first arg chứa ':' và không parse được thành u16
    let peer = args.iter()
        .find(|a| a.contains(':') && a.parse::<u16>().is_err())
        .cloned()
        .unwrap_or_else(|| DEFAULT_PEER.to_string());

    let mainnet = args.iter().any(|a| a == "--mainnet");

    FullnodeConfig { port, peer, mainnet }
}

// ── CLI ───────────────────────────────────────────────────────────────────────

pub fn cmd_fullnode(args: &[String]) {
    let cfg = parse_fullnode_args(args);

    println!("[fullnode] port={}  peer={}  network={}",
        cfg.port, cfg.peer, if cfg.mainnet { "mainnet" } else { "testnet" });

    let peer    = cfg.peer.clone();
    let mainnet = cfg.mainnet;

    let rt = tokio::runtime::Runtime::new()
        .expect("tokio runtime");

    rt.block_on(async move {
        // 1. Sync loop trong blocking thread (redb-safe: cùng process = shared Arc<Database>)
        tokio::task::spawn_blocking(move || {
            loop {
                crate::pkt_sync::run_sync(&peer, mainnet);
                eprintln!("[fullnode] sync terminated — restarting in {}s", RESTART_DELAY_SECS);
                std::thread::sleep(Duration::from_secs(RESTART_DELAY_SECS));
            }
        });

        // 2. pktscan REST API (async — chạy đến khi Ctrl+C)
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
        assert_eq!(cfg.port,    DEFAULT_PORT);
        assert_eq!(cfg.peer,    DEFAULT_PEER);
        assert!(!cfg.mainnet);
    }

    #[test]
    fn test_custom_port() {
        let cfg = parse_fullnode_args(&sv(&["9090"]));
        assert_eq!(cfg.port, 9090);
        assert_eq!(cfg.peer, DEFAULT_PEER);
    }

    #[test]
    fn test_default_port_when_peer_only() {
        let cfg = parse_fullnode_args(&sv(&["node.example.com:8333"]));
        assert_eq!(cfg.port, DEFAULT_PORT);
    }

    #[test]
    fn test_custom_peer() {
        let cfg = parse_fullnode_args(&sv(&["node.example.com:8333"]));
        assert_eq!(cfg.peer, "node.example.com:8333");
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
        let cfg = parse_fullnode_args(&sv(&["8082", "mynode.example.com:8333", "--mainnet"]));
        assert_eq!(cfg.port,    8082);
        assert_eq!(cfg.peer,    "mynode.example.com:8333");
        assert!(cfg.mainnet);
    }

    #[test]
    fn test_port_before_peer() {
        let cfg = parse_fullnode_args(&sv(&["9000", "peer.example.com:8333"]));
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.peer, "peer.example.com:8333");
    }

    #[test]
    fn test_peer_before_port() {
        let cfg = parse_fullnode_args(&sv(&["peer.example.com:8333", "9000"]));
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.peer, "peer.example.com:8333");
    }

    #[test]
    fn test_mainnet_flag_first() {
        let cfg = parse_fullnode_args(&sv(&["--mainnet", "8090", "peer.example.com:8333"]));
        assert!(cfg.mainnet);
        assert_eq!(cfg.port, 8090);
        assert_eq!(cfg.peer, "peer.example.com:8333");
    }

    #[test]
    fn test_default_instance() {
        let cfg = FullnodeConfig::default();
        assert_eq!(cfg.port,    DEFAULT_PORT);
        assert_eq!(cfg.peer,    DEFAULT_PEER);
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
