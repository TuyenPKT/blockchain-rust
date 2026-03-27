//! # pkt-sdk
//!
//! PKT blockchain SDK — types, helpers, and error types cho third-party developers
//! xây dựng ứng dụng trên nền PKTScan / PKT network.
//!
//! ## Quick start
//!
//! ```rust
//! use pkt_sdk::{paklets_to_pkt, BlockHeader, AddressBalance, PAKLETS_PER_PKT};
//!
//! let sat = 2_147_483_648u64;  // 2 PKT
//! assert_eq!(paklets_to_pkt(sat), 2.0);
//! ```

pub mod types;
pub mod convert;
pub mod error;

pub use types::*;
pub use convert::*;
pub use error::*;

// ── Constants ──────────────────────────────────────────────────────────────────

/// 1 PKT = 2^30 paklets (smallest unit).
pub const PAKLETS_PER_PKT: u64 = 1_073_741_824;

/// PKT testnet default P2P port.
pub const TESTNET_PORT: u16 = 8333;

/// PKT mainnet default P2P port.
pub const MAINNET_PORT: u16 = 64764;

/// PKT testnet default API port (PKTScan).
pub const API_PORT: u16 = 8080;

/// SDK version.
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");
