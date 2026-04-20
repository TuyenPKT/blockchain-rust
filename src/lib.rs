//! pkt-core library — expose các module cần thiết cho Tauri desktop app.
//!
//! Chỉ khai báo những module mà pkt_testnet_web cần (và transitive deps).
//! Tauri crate add dependency: blockchain-rust = { path = "../.." }

// ── Base types ────────────────────────────────────────────────────────────────
pub mod script;
pub mod wallet;
pub mod taproot;
pub mod lightning;
pub mod transaction;
pub mod reward;
pub mod api_auth;
pub mod url_guard;
pub mod rlp;
pub mod gas_model;
pub mod evm_state;
pub mod pkt_evm;
pub mod evm_precompiles;
pub mod abi;
pub mod eth_rpc;
pub mod eth_wire;
pub mod uncle;
pub mod receipts;
pub mod pkt_address;
pub mod pkt_health;
pub mod pkt_export;

// ── PKT network layer ─────────────────────────────────────────────────────────
pub mod pkt_genesis;
pub mod pkt_config;
pub mod evm_address;
pub mod pkt_paths;
pub mod pkt_kv;
pub mod pkt_wire;
pub mod pkt_peer;
pub mod pkt_checkpoints;

// ── Storage / sync ────────────────────────────────────────────────────────────
pub mod pkt_sync;
pub mod pkt_utxo_sync;
pub mod pkt_addr_index;
pub mod pkt_reorg;
pub mod pkt_mempool;
pub mod pkt_mempool_sync;
pub mod pkt_block_sync;
pub mod pkt_labels;
pub mod pkt_search;
pub mod pkt_analytics;
pub mod pkt_snapshot;

// ── API / UI ──────────────────────────────────────────────────────────────────
pub mod pkt_explorer_api;
pub mod pkt_sync_ui;
pub mod pkt_testnet_web;
pub mod openapi;
