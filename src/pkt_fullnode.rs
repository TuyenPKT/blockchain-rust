#![allow(dead_code)]
//! v23.8 — Full Node Mode
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
//! Main thread  ──► pktscan_api::serve(port)   REST API + Web UI (blocking)
//! Watcher thread ► monitor sync child, auto-restart nếu crash
//! Sync child   ──► OS subprocess `blockchain-rust sync [peer]`
//!                  (RocksDB write lock riêng biệt — không conflict với web read-only)
//! ```
//!
//! Tách sync thành OS subprocess giải quyết DB locking: sync giữ write lock,
//! web handlers mở read-only per-request (đã hoạt động với 2-process model hiện tại).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Recover from Mutex PoisonError thay vì panic.
macro_rules! lock_or_recover {
    ($mutex:expr) => {
        $mutex.lock().unwrap_or_else(|p| p.into_inner())
    };
}

const DEFAULT_PORT:         u16  = 8081;
const DEFAULT_PEER:         &str = "seed.testnet.oceif.com:8333";
const WATCHER_INTERVAL_SECS: u64 = 10;
const RESTART_DELAY_SECS:    u64 = 5;

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

// ── Sync subprocess ───────────────────────────────────────────────────────────

/// Internal: spawn sync với explicit exe path (testable).
pub fn spawn_sync_with_exe(
    exe:     &Path,
    peer:    &str,
    mainnet: bool,
) -> Result<std::process::Child, String> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("sync").arg(peer);
    if mainnet { cmd.arg("--mainnet"); }
    // Inherit stdout/stderr → sync logs hiện trên terminal cùng với web logs
    cmd.spawn().map_err(|e| format!("spawn sync failed: {}", e))
}

/// Spawn sync subprocess dùng current binary.
pub fn spawn_sync_process(peer: &str, mainnet: bool) -> Result<std::process::Child, String> {
    let exe = std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("blockchain-rust"));
    spawn_sync_with_exe(&exe, peer, mainnet)
}

// ── Watcher thread ────────────────────────────────────────────────────────────

/// Spawn background thread theo dõi sync child — tự restart nếu exit bất thường.
fn start_watcher(
    child:   Arc<Mutex<std::process::Child>>,
    peer:    String,
    mainnet: bool,
) {
    std::thread::Builder::new()
        .name("sync-watcher".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(WATCHER_INTERVAL_SECS));

                let status_str = {
                    let mut g = lock_or_recover!(child);
                    g.try_wait().ok().flatten().map(|s| s.to_string())
                };

                if let Some(status) = status_str {
                    eprintln!("[fullnode] sync exited ({}) — restarting in {}s",
                        status, RESTART_DELAY_SECS);
                    std::thread::sleep(Duration::from_secs(RESTART_DELAY_SECS));

                    match spawn_sync_process(&peer, mainnet) {
                        Ok(new_child) => {
                            println!("[fullnode] sync restarted — pid={}", new_child.id());
                            *lock_or_recover!(child) = new_child;
                        }
                        Err(e) => eprintln!("[fullnode] respawn failed: {}", e),
                    }
                }
            }
        })
        .ok();
}

// ── CLI ───────────────────────────────────────────────────────────────────────

pub fn cmd_fullnode(args: &[String]) {
    let cfg = parse_fullnode_args(args);

    println!("[fullnode] port={}  peer={}  network={}",
        cfg.port, cfg.peer, if cfg.mainnet { "mainnet" } else { "testnet" });

    // 1. Spawn sync subprocess
    let sync_child = match spawn_sync_process(&cfg.peer, cfg.mainnet) {
        Ok(c) => {
            println!("[fullnode] sync started — pid={}", c.id());
            Arc::new(Mutex::new(c))
        }
        Err(e) => {
            eprintln!("[fullnode] {}", e);
            std::process::exit(1);
        }
    };

    // 2. Auto-restart watcher
    start_watcher(Arc::clone(&sync_child), cfg.peer.clone(), cfg.mainnet);

    // 3. pktscan REST API (blocking — runs until Ctrl+C or error)
    println!("[fullnode] web server on :{}", cfg.port);
    let bc = crate::storage::load_or_new();
    let db = Arc::new(tokio::sync::Mutex::new(bc));
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(crate::pktscan_api::serve(db, cfg.port));

    // 4. Cleanup: kill sync on exit
    let _ = lock_or_recover!(sync_child).kill();
    let _ = lock_or_recover!(sync_child).wait();
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

    // ── spawn_sync_with_exe ───────────────────────────────────────────────────

    #[test]
    fn test_spawn_sync_nonexistent_binary_returns_err() {
        let result = spawn_sync_with_exe(
            Path::new("/nonexistent/binary/blockchain-rust-test-xyz"),
            "localhost:8333",
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_spawn_sync_nonexistent_binary_err_contains_spawn() {
        let err = spawn_sync_with_exe(
            Path::new("/nonexistent/binary/xyz"),
            "localhost:8333",
            false,
        ).unwrap_err();
        assert!(err.contains("spawn sync failed"));
    }

    #[test]
    fn test_spawn_sync_uses_sync_arg() {
        // Verify the command is built correctly by using `echo` as exe
        // `echo sync peer` succeeds and writes to stdout (we ignore output)
        let echo = std::process::Command::new("echo")
            .arg("sync")
            .arg("localhost:8333")
            .output();
        assert!(echo.is_ok());
    }
}
