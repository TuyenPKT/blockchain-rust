#![allow(dead_code)]
//! v24.2 — PKT Data Paths
//!
//! Single source of truth cho tất cả đường dẫn DB/file của PKT node.
//!
//! ## Layout
//! ```
//! ~/.pkt/
//! ├── wallet.key          ← dùng chung (không đổi theo network)
//! ├── testnet/            ← default (không có --mainnet)
//! │   ├── syncdb/
//! │   ├── utxodb/
//! │   ├── addr_index/
//! │   ├── labeldb/
//! │   ├── blockdb/
//! │   ├── mempooldb/
//! │   ├── reorgdb/
//! │   └── peers.txt
//! └── mainnet/            ← khi set_mainnet(true)
//!     ├── syncdb/
//!     ├── ...
//! ```
//!
//! ## Sử dụng
//! ```rust
//! // Trong main.rs — đặt trước tất cả dispatch:
//! pkt_paths::set_mainnet(args.contains(&"--mainnet".to_string()));
//!
//! // Trong các module:
//! let path = pkt_paths::sync_db();      // ~/.pkt/testnet/syncdb
//! let path = pkt_paths::utxo_db();      // ~/.pkt/testnet/utxodb
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

// ── Global network flag ────────────────────────────────────────────────────────

static IS_MAINNET: AtomicBool = AtomicBool::new(false);

/// Đặt network flag — gọi 1 lần duy nhất ở đầu main() trước mọi dispatch.
pub fn set_mainnet(mainnet: bool) {
    IS_MAINNET.store(mainnet, Ordering::Relaxed);
}

pub fn is_mainnet() -> bool {
    IS_MAINNET.load(Ordering::Relaxed)
}

// ── Root dir ───────────────────────────────────────────────────────────────────

fn home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Gốc data dir: ~/.pkt/testnet/ hoặc ~/.pkt/mainnet/
pub fn data_dir() -> PathBuf {
    let net = if is_mainnet() { "mainnet" } else { "testnet" };
    home().join(".pkt").join(net)
}

/// ~/.pkt/ (dùng cho wallet.key — không thay đổi theo network)
pub fn pkt_root() -> PathBuf {
    home().join(".pkt")
}

// ── Per-network paths ──────────────────────────────────────────────────────────

pub fn sync_db()    -> PathBuf { data_dir().join("syncdb")     }
pub fn utxo_db()    -> PathBuf { data_dir().join("utxodb")     }
pub fn addr_index() -> PathBuf { data_dir().join("addr_index") }
pub fn label_db()   -> PathBuf { data_dir().join("labeldb")    }
pub fn block_db()   -> PathBuf { data_dir().join("blockdb")    }
pub fn mempool_db() -> PathBuf { data_dir().join("mempooldb")  }
pub fn reorg_db()   -> PathBuf { data_dir().join("reorgdb")    }
pub fn peers_file() -> PathBuf { data_dir().join("peers.txt")  }

// ── Shared paths (không đổi theo network) ────────────────────────────────────

pub fn wallet_key() -> PathBuf { pkt_root().join("wallet.key") }

// ── Tests ──────────────────────────────────────────────────────────────────────

// ── Pure path helpers cho tests (không đụng global state) ────────────────────

fn data_dir_for(mainnet: bool) -> std::path::PathBuf {
    let net = if mainnet { "mainnet" } else { "testnet" };
    home().join(".pkt").join(net)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Dùng helper pure để tránh race condition với AtomicBool trong tests song song

    #[test]
    fn test_testnet_paths() {
        let d = data_dir_for(false);
        let s = d.to_str().unwrap();
        assert!(s.ends_with("testnet"));
        assert!(d.join("syncdb").to_str().unwrap().contains("testnet/syncdb"));
        assert!(d.join("utxodb").to_str().unwrap().contains("testnet/utxodb"));
        assert!(d.join("addr_index").to_str().unwrap().contains("testnet/addr_index"));
        assert!(d.join("labeldb").to_str().unwrap().contains("testnet/labeldb"));
        assert!(d.join("mempooldb").to_str().unwrap().contains("testnet/mempooldb"));
        assert!(d.join("reorgdb").to_str().unwrap().contains("testnet/reorgdb"));
        assert!(d.join("peers.txt").to_str().unwrap().contains("testnet/peers.txt"));
    }

    #[test]
    fn test_mainnet_paths() {
        let d = data_dir_for(true);
        let s = d.to_str().unwrap();
        assert!(s.ends_with("mainnet"));
        assert!(d.join("syncdb").to_str().unwrap().contains("mainnet/syncdb"));
        assert!(d.join("utxodb").to_str().unwrap().contains("mainnet/utxodb"));
        assert!(d.join("peers.txt").to_str().unwrap().contains("mainnet/peers.txt"));
    }

    #[test]
    fn test_wallet_key_unchanged() {
        // wallet.key luôn ở ~/.pkt/ không thay đổi theo network
        let root = pkt_root();
        assert!(wallet_key().starts_with(&root));
        assert!(!wallet_key().to_str().unwrap().contains("testnet"));
        assert!(!wallet_key().to_str().unwrap().contains("mainnet"));
    }

    #[test]
    fn test_testnet_mainnet_different() {
        let t = data_dir_for(false).join("syncdb");
        let m = data_dir_for(true).join("syncdb");
        assert_ne!(t, m);
    }

    #[test]
    fn test_data_dir_under_pkt_root() {
        let root = pkt_root();
        assert!(data_dir_for(false).starts_with(&root));
        assert!(data_dir_for(true).starts_with(&root));
    }

    #[test]
    fn test_set_mainnet_flag() {
        // Test AtomicBool works (sequential)
        set_mainnet(false);
        assert!(!is_mainnet());
        set_mainnet(true);
        assert!(is_mainnet());
        set_mainnet(false);
        assert!(!is_mainnet());
    }
}
