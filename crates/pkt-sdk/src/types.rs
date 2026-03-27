//! Core data types — clean, serializable structs cho PKT blockchain data.

use serde::{Deserialize, Serialize};

// ── Block ──────────────────────────────────────────────────────────────────────

/// Thông tin một block header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height:    u64,
    pub hash:      String,
    pub prev_hash: String,
    pub timestamp: u32,
    pub bits:      u32,
    pub nonce:     u32,
    pub version:   u32,
}

/// Kết quả paginated block list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPage {
    pub headers:     Vec<BlockHeader>,
    pub tip:         u64,
    pub next_cursor: Option<u64>,
}

// ── Transaction ────────────────────────────────────────────────────────────────

/// Tham chiếu gọn đến một transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TxRef {
    pub txid:      String,
    pub height:    u64,
    pub timestamp: u64,
}

/// Kết quả paginated tx list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxPage {
    pub txs:         Vec<TxRef>,
    pub next_cursor: Option<u64>,
}

// ── Address ────────────────────────────────────────────────────────────────────

/// Số dư và lịch sử giao dịch của một address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressInfo {
    pub address:     String,
    pub balance_sat: u64,
    pub balance_pkt: f64,
    pub tx_count:    usize,
    pub txs:         Vec<TxRef>,
}

/// Số dư đơn giản.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddressBalance {
    pub address:     String,
    pub balance_sat: u64,
    pub balance_pkt: f64,
}

// ── UTXO ───────────────────────────────────────────────────────────────────────

/// Một unspent transaction output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utxo {
    pub txid:  String,
    pub vout:  u32,
    pub value: u64,
}

// ── Sync status ────────────────────────────────────────────────────────────────

/// Trạng thái đồng bộ của node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub phase:            String,
    pub sync_height:      u64,
    pub utxo_height:      u64,
    pub overall_progress: f64,
}

// ── Network summary ────────────────────────────────────────────────────────────

/// Tóm tắt toàn mạng — dùng cho home page / mobile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSummary {
    pub height:                  u64,
    pub tip_hash:                String,
    pub synced:                  bool,
    pub utxo_count:              u64,
    pub total_value_sat:         u64,
    pub total_value_pkt:         f64,
    pub hashrate:                f64,
    pub block_time_avg:          f64,
    pub mempool_count:           u64,
    pub mempool_top_fee_msat_vb: u64,
}
