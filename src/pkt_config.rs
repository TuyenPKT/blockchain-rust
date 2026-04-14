#![allow(dead_code)]
//! v24.6.1 — Network Config
//!
//! Single source of truth cho tất cả network configuration.
//! Thay thế giá trị hardcode rải rác ở nhiều file.
//!
//! ## Sử dụng
//! ```rust,ignore
//! // Trong main.rs — gọi trước mọi dispatch:
//! pkt_config::init(args.contains(&"--mainnet".to_string()));
//!
//! // Trong bất kỳ module nào:
//! let cfg = pkt_config::get();
//! let seed = cfg.seed_p2p();    // "seed.testnet.oceif.com:8333"
//! let pool = cfg.seed_pool();   // "seed.testnet.oceif.com:8337"
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::pkt_genesis::{
    HALVING_INTERVAL, INITIAL_BLOCK_REWARD, MAX_SUPPLY_PKT, PAKLETS_PER_PKT,
    MAINNET_MAGIC, TESTNET_MAGIC,
};

// ── Global singleton ──────────────────────────────────────────────────────────

static CONFIG: OnceLock<PktConfig> = OnceLock::new();

/// Khởi tạo config một lần duy nhất ở đầu main().
/// Nếu chưa gọi init(), get() trả về testnet defaults.
pub fn init(mainnet: bool) {
    let cfg = if mainnet { PktConfig::mainnet() } else { PktConfig::testnet() };
    // Sync với pkt_paths network flag
    crate::pkt_paths::set_mainnet(mainnet);
    CONFIG.set(cfg).ok();
}

/// Lấy config hiện tại. Mặc định testnet nếu chưa init().
pub fn get() -> &'static PktConfig {
    CONFIG.get_or_init(PktConfig::testnet)
}

// ── PktConfig ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PktConfig {
    // Network
    pub network:    Network,
    pub seed_host:  String,       // "seed.testnet.oceif.com"
    pub magic:      [u8; 4],

    // Ports
    pub p2p_port:   u16,          // 8333 / 64764
    pub api_port:   u16,          // 8081 / 8081
    pub pool_port:  u16,          // 8337
    pub stats_port: u16,          // 8338
    pub rpc_port:   u16,          // 8334 (direct node template)

    // Data
    pub data_dir:   PathBuf,      // ~/.pkt/testnet/ hoặc ~/.pkt/mainnet/

    // Tokenomics (shared từ pkt_genesis)
    pub initial_block_reward: u64,
    pub halving_interval:     u64,
    pub max_supply_pkt:       u64,
    pub paklets_per_pkt:      u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Network { Testnet, Mainnet }

impl PktConfig {
    pub fn testnet() -> Self {
        Self {
            network:    Network::Testnet,
            seed_host:  "seed.testnet.oceif.com".into(),
            magic:      TESTNET_MAGIC,
            p2p_port:   8333,
            api_port:   8081,
            pool_port:  8337,
            stats_port: 8338,
            rpc_port:   8334,
            data_dir:   Self::home().join(".pkt").join("testnet"),
            initial_block_reward: INITIAL_BLOCK_REWARD,
            halving_interval:     HALVING_INTERVAL,
            max_supply_pkt:       MAX_SUPPLY_PKT,
            paklets_per_pkt:      PAKLETS_PER_PKT,
        }
    }

    pub fn mainnet() -> Self {
        Self {
            network:    Network::Mainnet,
            seed_host:  "seed.mainnet.oceif.com".into(),
            magic:      MAINNET_MAGIC,
            p2p_port:   64764,
            api_port:   8081,
            pool_port:  8337,
            stats_port: 8338,
            rpc_port:   64766,
            data_dir:   Self::home().join(".pkt").join("mainnet"),
            initial_block_reward: INITIAL_BLOCK_REWARD,
            halving_interval:     HALVING_INTERVAL,
            max_supply_pkt:       MAX_SUPPLY_PKT,
            paklets_per_pkt:      PAKLETS_PER_PKT,
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    pub fn is_mainnet(&self) -> bool { self.network == Network::Mainnet }

    /// "seed.testnet.oceif.com:8333"
    pub fn seed_p2p(&self) -> String {
        format!("{}:{}", self.seed_host, self.p2p_port)
    }

    /// "seed.testnet.oceif.com:8337"
    pub fn seed_pool(&self) -> String {
        format!("{}:{}", self.seed_host, self.pool_port)
    }

    /// "seed.testnet.oceif.com:8334"
    pub fn seed_rpc(&self) -> String {
        format!("{}:{}", self.seed_host, self.rpc_port)
    }

    /// "http://seed.testnet.oceif.com:8081"
    pub fn api_base(&self) -> String {
        format!("http://{}:{}", self.seed_host, self.api_port)
    }

    fn home() -> PathBuf {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_testnet_defaults() {
        let cfg = PktConfig::testnet();
        assert_eq!(cfg.seed_host, "seed.testnet.oceif.com");
        assert_eq!(cfg.p2p_port, 8333);
        assert_eq!(cfg.pool_port, 8337);
        assert_eq!(cfg.rpc_port, 8334);
        assert!(!cfg.is_mainnet());
    }

    #[test]
    fn test_mainnet_defaults() {
        let cfg = PktConfig::mainnet();
        assert_eq!(cfg.seed_host, "seed.mainnet.oceif.com");
        assert_eq!(cfg.p2p_port, 64764);
        assert!(cfg.is_mainnet());
    }

    #[test]
    fn test_seed_p2p_format() {
        let cfg = PktConfig::testnet();
        assert_eq!(cfg.seed_p2p(), "seed.testnet.oceif.com:8333");
    }

    #[test]
    fn test_seed_pool_format() {
        let cfg = PktConfig::testnet();
        assert_eq!(cfg.seed_pool(), "seed.testnet.oceif.com:8337");
    }

    #[test]
    fn test_api_base_format() {
        let cfg = PktConfig::testnet();
        assert_eq!(cfg.api_base(), "http://seed.testnet.oceif.com:8081");
    }

    #[test]
    fn test_tokenomics_from_genesis() {
        let cfg = PktConfig::testnet();
        assert_eq!(cfg.initial_block_reward, INITIAL_BLOCK_REWARD);
        assert_eq!(cfg.halving_interval, HALVING_INTERVAL);
        assert_eq!(cfg.max_supply_pkt, MAX_SUPPLY_PKT);
    }

    #[test]
    fn test_testnet_mainnet_different_seed() {
        let t = PktConfig::testnet();
        let m = PktConfig::mainnet();
        assert_ne!(t.seed_host, m.seed_host);
        assert_ne!(t.p2p_port, m.p2p_port);
        assert_ne!(t.data_dir, m.data_dir);
    }

    #[test]
    fn test_data_dir_contains_network_name() {
        let t = PktConfig::testnet();
        let m = PktConfig::mainnet();
        assert!(t.data_dir.to_str().unwrap().contains("testnet"));
        assert!(m.data_dir.to_str().unwrap().contains("mainnet"));
    }
}
