#![allow(dead_code)]
//! v26.0 — ETH/68 P2P Wire Protocol (message type definitions + codec)
//!
//! Implements the Ethereum devp2p eth/68 sub-protocol message set:
//!   Status (0x00), NewBlockHashes (0x01), Transactions (0x02),
//!   GetBlockHeaders (0x03), BlockHeaders (0x04),
//!   GetBlockBodies (0x05), BlockBodies (0x06),
//!   NewBlock (0x07), NewPooledTransactionHashes (0x08),
//!   GetPooledTransactions (0x09), PooledTransactions (0x0A),
//!   GetReceipts (0x0F), Receipts (0x10)
//!
//! Wire encoding: RLP-style length-prefixed frames (simplified for OCEIF).
//! Each frame: [1-byte msg-id][4-byte LE body-len][body bytes]

use serde::{Deserialize, Serialize};

// ─── Protocol constants ───────────────────────────────────────────────────────

pub const ETH_PROTOCOL_VERSION: u64 = 68;
pub const MAX_HEADERS_PER_REQUEST: usize = 1024;
pub const MAX_BODIES_PER_REQUEST:  usize = 256;
pub const MAX_TXS_PER_PACKET:      usize = 4096;
pub const FRAME_HEADER_LEN:        usize = 5; // 1 (id) + 4 (len)

// ─── Message IDs ──────────────────────────────────────────────────────────────

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgId {
    Status                      = 0x00,
    NewBlockHashes              = 0x01,
    Transactions                = 0x02,
    GetBlockHeaders             = 0x03,
    BlockHeaders                = 0x04,
    GetBlockBodies              = 0x05,
    BlockBodies                 = 0x06,
    NewBlock                    = 0x07,
    NewPooledTransactionHashes  = 0x08,
    GetPooledTransactions       = 0x09,
    PooledTransactions          = 0x0A,
    GetReceipts                 = 0x0F,
    Receipts                    = 0x10,
}

impl TryFrom<u8> for MsgId {
    type Error = String;
    fn try_from(b: u8) -> Result<Self, Self::Error> {
        Ok(match b {
            0x00 => MsgId::Status,
            0x01 => MsgId::NewBlockHashes,
            0x02 => MsgId::Transactions,
            0x03 => MsgId::GetBlockHeaders,
            0x04 => MsgId::BlockHeaders,
            0x05 => MsgId::GetBlockBodies,
            0x06 => MsgId::BlockBodies,
            0x07 => MsgId::NewBlock,
            0x08 => MsgId::NewPooledTransactionHashes,
            0x09 => MsgId::GetPooledTransactions,
            0x0A => MsgId::PooledTransactions,
            0x0F => MsgId::GetReceipts,
            0x10 => MsgId::Receipts,
            _    => return Err(format!("unknown msg id 0x{b:02X}")),
        })
    }
}

// ─── Wire message types ───────────────────────────────────────────────────────

/// Status — sent on connection to negotiate protocol version and genesis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusMsg {
    pub version:           u64,
    pub network_id:        u64,
    pub total_difficulty:  u64,
    pub best_hash:         [u8; 32],
    pub genesis_hash:      [u8; 32],
    pub fork_id:           [u8; 8],  // EIP-2124 fork identifier
}

/// A (hash, number) pair for NewBlockHashes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockHashNum {
    pub hash:   [u8; 32],
    pub number: u64,
}

/// NewBlockHashes — announce new block header hashes to peers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewBlockHashesMsg {
    pub hashes: Vec<BlockHashNum>,
}

/// Raw signed transaction bytes (RLP-encoded)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawTx(pub Vec<u8>);

/// Transactions — propagate signed transactions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransactionsMsg {
    pub txs: Vec<RawTx>,
}

/// Header request origin — either by number or by hash
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HeaderOrigin {
    Number(u64),
    Hash([u8; 32]),
}

/// GetBlockHeaders — request up to `limit` headers starting from `origin`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetBlockHeadersMsg {
    pub request_id: u64,
    pub origin:     HeaderOrigin,
    pub limit:      u64,
    pub skip:       u64,
    pub reverse:    bool,
}

/// A minimal block header (fields needed for syncing)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockHeader {
    pub number:      u64,
    pub hash:        [u8; 32],
    pub parent_hash: [u8; 32],
    pub timestamp:   u64,
    pub gas_limit:   u64,
    pub gas_used:    u64,
    pub base_fee:    u64,
    pub difficulty:  u64,
}

/// BlockHeaders — response to GetBlockHeaders
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockHeadersMsg {
    pub request_id: u64,
    pub headers:    Vec<BlockHeader>,
}

/// GetBlockBodies — request block bodies by hash
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetBlockBodiesMsg {
    pub request_id: u64,
    pub hashes:     Vec<[u8; 32]>,
}

/// Block body (transactions + uncles)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockBody {
    pub transactions: Vec<RawTx>,
    pub uncles:       Vec<BlockHeader>,
}

/// BlockBodies — response to GetBlockBodies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockBodiesMsg {
    pub request_id: u64,
    pub bodies:     Vec<BlockBody>,
}

/// NewBlock — broadcast a new block to a peer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewBlockMsg {
    pub header:           BlockHeader,
    pub body:             BlockBody,
    pub total_difficulty: u64,
}

/// NewPooledTransactionHashes (eth/68) — lightweight tx announcement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewPooledTxHashesMsg {
    pub types:  Vec<u8>,    // tx type per hash
    pub sizes:  Vec<u32>,   // byte size per hash
    pub hashes: Vec<[u8; 32]>,
}

/// GetPooledTransactions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetPooledTxsMsg {
    pub request_id: u64,
    pub hashes:     Vec<[u8; 32]>,
}

/// PooledTransactions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PooledTxsMsg {
    pub request_id: u64,
    pub txs:        Vec<RawTx>,
}

/// GetReceipts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetReceiptsMsg {
    pub request_id: u64,
    pub hashes:     Vec<[u8; 32]>,
}

/// Log entry in a receipt
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptLog {
    pub address: [u8; 20],
    pub topics:  Vec<[u8; 32]>,
    pub data:    Vec<u8>,
}

/// Transaction receipt
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxReceipt {
    pub tx_hash:   [u8; 32],
    pub gas_used:  u64,
    pub status:    u8, // 1 = success, 0 = failure
    pub logs:      Vec<ReceiptLog>,
}

/// Receipts — response to GetReceipts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptsMsg {
    pub request_id: u64,
    pub receipts:   Vec<Vec<TxReceipt>>,
}

// ─── Enum wrapping all message types ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum EthMsg {
    Status(StatusMsg),
    NewBlockHashes(NewBlockHashesMsg),
    Transactions(TransactionsMsg),
    GetBlockHeaders(GetBlockHeadersMsg),
    BlockHeaders(BlockHeadersMsg),
    GetBlockBodies(GetBlockBodiesMsg),
    BlockBodies(BlockBodiesMsg),
    NewBlock(NewBlockMsg),
    NewPooledTxHashes(NewPooledTxHashesMsg),
    GetPooledTxs(GetPooledTxsMsg),
    PooledTxs(PooledTxsMsg),
    GetReceipts(GetReceiptsMsg),
    Receipts(ReceiptsMsg),
}

// ─── Wire codec ───────────────────────────────────────────────────────────────
//
// Frame layout: [1-byte msg-id][4-byte LE body-len][body: serde_json encoded]
// Note: production would use RLP; we use JSON for simplicity + debuggability.

pub struct FrameCodec;

impl FrameCodec {
    pub fn encode(msg_id: u8, body: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + body.len());
        frame.push(msg_id);
        let len = body.len() as u32;
        frame.extend_from_slice(&len.to_le_bytes());
        frame.extend_from_slice(body);
        frame
    }

    pub fn decode_header(bytes: &[u8]) -> Option<(u8, u32)> {
        if bytes.len() < FRAME_HEADER_LEN { return None; }
        let msg_id = bytes[0];
        let len    = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        Some((msg_id, len))
    }

    pub fn encode_msg(msg: &EthMsg) -> Result<Vec<u8>, String> {
        let (id, body) = match msg {
            EthMsg::Status(m)           => (MsgId::Status as u8,                     serde_json::to_vec(m)),
            EthMsg::NewBlockHashes(m)   => (MsgId::NewBlockHashes as u8,             serde_json::to_vec(m)),
            EthMsg::Transactions(m)     => (MsgId::Transactions as u8,               serde_json::to_vec(m)),
            EthMsg::GetBlockHeaders(m)  => (MsgId::GetBlockHeaders as u8,            serde_json::to_vec(m)),
            EthMsg::BlockHeaders(m)     => (MsgId::BlockHeaders as u8,               serde_json::to_vec(m)),
            EthMsg::GetBlockBodies(m)   => (MsgId::GetBlockBodies as u8,             serde_json::to_vec(m)),
            EthMsg::BlockBodies(m)      => (MsgId::BlockBodies as u8,                serde_json::to_vec(m)),
            EthMsg::NewBlock(m)         => (MsgId::NewBlock as u8,                   serde_json::to_vec(m)),
            EthMsg::NewPooledTxHashes(m)=> (MsgId::NewPooledTransactionHashes as u8, serde_json::to_vec(m)),
            EthMsg::GetPooledTxs(m)     => (MsgId::GetPooledTransactions as u8,      serde_json::to_vec(m)),
            EthMsg::PooledTxs(m)        => (MsgId::PooledTransactions as u8,         serde_json::to_vec(m)),
            EthMsg::GetReceipts(m)      => (MsgId::GetReceipts as u8,                serde_json::to_vec(m)),
            EthMsg::Receipts(m)         => (MsgId::Receipts as u8,                   serde_json::to_vec(m)),
        };
        let body = body.map_err(|e| e.to_string())?;
        Ok(Self::encode(id, &body))
    }

    pub fn decode_msg(frame: &[u8]) -> Result<EthMsg, String> {
        if frame.len() < FRAME_HEADER_LEN {
            return Err("frame too short".into());
        }
        let (id_byte, body_len) = Self::decode_header(frame)
            .ok_or("decode header failed")?;
        let body_start = FRAME_HEADER_LEN;
        let body_end   = body_start + body_len as usize;
        if frame.len() < body_end {
            return Err(format!("frame truncated: need {} bytes, have {}", body_end, frame.len()));
        }
        let body = &frame[body_start..body_end];
        let msg_id = MsgId::try_from(id_byte)?;
        Ok(match msg_id {
            MsgId::Status
                => EthMsg::Status(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::NewBlockHashes
                => EthMsg::NewBlockHashes(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::Transactions
                => EthMsg::Transactions(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::GetBlockHeaders
                => EthMsg::GetBlockHeaders(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::BlockHeaders
                => EthMsg::BlockHeaders(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::GetBlockBodies
                => EthMsg::GetBlockBodies(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::BlockBodies
                => EthMsg::BlockBodies(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::NewBlock
                => EthMsg::NewBlock(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::NewPooledTransactionHashes
                => EthMsg::NewPooledTxHashes(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::GetPooledTransactions
                => EthMsg::GetPooledTxs(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::PooledTransactions
                => EthMsg::PooledTxs(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::GetReceipts
                => EthMsg::GetReceipts(serde_json::from_slice(body).map_err(|e| e.to_string())?),
            MsgId::Receipts
                => EthMsg::Receipts(serde_json::from_slice(body).map_err(|e| e.to_string())?),
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_hash() -> [u8; 32] { [0u8; 32] }
    fn zero_fork() -> [u8; 8]  { [0u8; 8]  }

    fn status_msg() -> EthMsg {
        EthMsg::Status(StatusMsg {
            version: ETH_PROTOCOL_VERSION,
            network_id: 1,
            total_difficulty: 0,
            best_hash: zero_hash(),
            genesis_hash: zero_hash(),
            fork_id: zero_fork(),
        })
    }

    #[test]
    fn test_status_encode_decode_roundtrip() {
        let msg   = status_msg();
        let frame = FrameCodec::encode_msg(&msg).unwrap();
        assert!(frame.len() > FRAME_HEADER_LEN);
        let decoded = FrameCodec::decode_msg(&frame).unwrap();
        if let EthMsg::Status(s) = decoded {
            assert_eq!(s.version, ETH_PROTOCOL_VERSION);
            assert_eq!(s.network_id, 1);
        } else {
            panic!("expected Status");
        }
    }

    #[test]
    fn test_frame_header_id() {
        let frame = FrameCodec::encode(0x03, b"hello");
        let (id, len) = FrameCodec::decode_header(&frame).unwrap();
        assert_eq!(id, 0x03);
        assert_eq!(len, 5);
    }

    #[test]
    fn test_new_block_hashes_roundtrip() {
        let msg = EthMsg::NewBlockHashes(NewBlockHashesMsg {
            hashes: vec![BlockHashNum { hash: zero_hash(), number: 42 }],
        });
        let frame = FrameCodec::encode_msg(&msg).unwrap();
        let decoded = FrameCodec::decode_msg(&frame).unwrap();
        if let EthMsg::NewBlockHashes(m) = decoded {
            assert_eq!(m.hashes.len(), 1);
            assert_eq!(m.hashes[0].number, 42);
        } else {
            panic!("expected NewBlockHashes");
        }
    }

    #[test]
    fn test_transactions_roundtrip() {
        let msg = EthMsg::Transactions(TransactionsMsg {
            txs: vec![RawTx(vec![0xDE, 0xAD, 0xBE, 0xEF])],
        });
        let frame = FrameCodec::encode_msg(&msg).unwrap();
        let decoded = FrameCodec::decode_msg(&frame).unwrap();
        if let EthMsg::Transactions(m) = decoded {
            assert_eq!(m.txs[0].0, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        } else {
            panic!("expected Transactions");
        }
    }

    #[test]
    fn test_get_block_headers_roundtrip() {
        let msg = EthMsg::GetBlockHeaders(GetBlockHeadersMsg {
            request_id: 1,
            origin: HeaderOrigin::Number(100),
            limit: 10,
            skip: 0,
            reverse: false,
        });
        let frame = FrameCodec::encode_msg(&msg).unwrap();
        let decoded = FrameCodec::decode_msg(&frame).unwrap();
        if let EthMsg::GetBlockHeaders(m) = decoded {
            assert_eq!(m.request_id, 1);
            assert_eq!(m.limit, 10);
        } else {
            panic!("expected GetBlockHeaders");
        }
    }

    #[test]
    fn test_block_headers_roundtrip() {
        let hdr = BlockHeader {
            number: 1, hash: zero_hash(), parent_hash: zero_hash(),
            timestamp: 0, gas_limit: 30_000_000, gas_used: 0,
            base_fee: 1_000_000_000, difficulty: 0,
        };
        let msg = EthMsg::BlockHeaders(BlockHeadersMsg {
            request_id: 1, headers: vec![hdr],
        });
        let frame = FrameCodec::encode_msg(&msg).unwrap();
        let decoded = FrameCodec::decode_msg(&frame).unwrap();
        if let EthMsg::BlockHeaders(m) = decoded {
            assert_eq!(m.headers[0].number, 1);
            assert_eq!(m.headers[0].gas_limit, 30_000_000);
        } else {
            panic!("expected BlockHeaders");
        }
    }

    #[test]
    fn test_new_block_roundtrip() {
        let hdr = BlockHeader {
            number: 5, hash: zero_hash(), parent_hash: zero_hash(),
            timestamp: 1000, gas_limit: 30_000_000, gas_used: 21_000,
            base_fee: 1_000_000_000, difficulty: 0,
        };
        let msg = EthMsg::NewBlock(NewBlockMsg {
            header: hdr, body: BlockBody { transactions: vec![], uncles: vec![] },
            total_difficulty: 5,
        });
        let frame = FrameCodec::encode_msg(&msg).unwrap();
        let decoded = FrameCodec::decode_msg(&frame).unwrap();
        if let EthMsg::NewBlock(m) = decoded {
            assert_eq!(m.header.number, 5);
            assert_eq!(m.total_difficulty, 5);
        } else {
            panic!("expected NewBlock");
        }
    }

    #[test]
    fn test_new_pooled_tx_hashes_roundtrip() {
        let msg = EthMsg::NewPooledTxHashes(NewPooledTxHashesMsg {
            types: vec![2],
            sizes: vec![200],
            hashes: vec![zero_hash()],
        });
        let frame = FrameCodec::encode_msg(&msg).unwrap();
        let decoded = FrameCodec::decode_msg(&frame).unwrap();
        if let EthMsg::NewPooledTxHashes(m) = decoded {
            assert_eq!(m.hashes.len(), 1);
            assert_eq!(m.types[0], 2);
        } else {
            panic!("expected NewPooledTxHashes");
        }
    }

    #[test]
    fn test_get_pooled_txs_roundtrip() {
        let msg = EthMsg::GetPooledTxs(GetPooledTxsMsg {
            request_id: 7, hashes: vec![zero_hash()],
        });
        let frame = FrameCodec::encode_msg(&msg).unwrap();
        let decoded = FrameCodec::decode_msg(&frame).unwrap();
        if let EthMsg::GetPooledTxs(m) = decoded {
            assert_eq!(m.request_id, 7);
        } else {
            panic!("expected GetPooledTxs");
        }
    }

    #[test]
    fn test_get_receipts_roundtrip() {
        let msg = EthMsg::GetReceipts(GetReceiptsMsg {
            request_id: 3, hashes: vec![zero_hash()],
        });
        let frame = FrameCodec::encode_msg(&msg).unwrap();
        let decoded = FrameCodec::decode_msg(&frame).unwrap();
        if let EthMsg::GetReceipts(m) = decoded {
            assert_eq!(m.request_id, 3);
        } else {
            panic!("expected GetReceipts");
        }
    }

    #[test]
    fn test_receipts_roundtrip() {
        let receipt = TxReceipt {
            tx_hash: zero_hash(), gas_used: 21_000, status: 1,
            logs: vec![ReceiptLog { address: [0; 20], topics: vec![], data: vec![] }],
        };
        let msg = EthMsg::Receipts(ReceiptsMsg {
            request_id: 4, receipts: vec![vec![receipt]],
        });
        let frame = FrameCodec::encode_msg(&msg).unwrap();
        let decoded = FrameCodec::decode_msg(&frame).unwrap();
        if let EthMsg::Receipts(m) = decoded {
            assert_eq!(m.receipts[0][0].gas_used, 21_000);
            assert_eq!(m.receipts[0][0].status, 1);
        } else {
            panic!("expected Receipts");
        }
    }

    #[test]
    fn test_frame_too_short_returns_error() {
        let result = FrameCodec::decode_msg(&[0x00, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_msg_id_returns_error() {
        let frame = FrameCodec::encode(0xFF, b"{}");
        let result = FrameCodec::decode_msg(&frame);
        assert!(result.is_err());
    }

    #[test]
    fn test_frame_truncated_body_returns_error() {
        let frame = FrameCodec::encode(0x00, b"hello");
        let truncated = &frame[..frame.len() - 2]; // cut last 2 bytes
        let result = FrameCodec::decode_msg(truncated);
        assert!(result.is_err());
    }

    #[test]
    fn test_max_headers_per_request_cap() {
        assert!(MAX_HEADERS_PER_REQUEST <= 1024);
    }

    #[test]
    fn test_protocol_version_is_68() {
        assert_eq!(ETH_PROTOCOL_VERSION, 68);
    }

    #[test]
    fn test_msg_id_try_from_known() {
        assert_eq!(MsgId::try_from(0x00).unwrap(), MsgId::Status);
        assert_eq!(MsgId::try_from(0x04).unwrap(), MsgId::BlockHeaders);
        assert_eq!(MsgId::try_from(0x10).unwrap(), MsgId::Receipts);
    }

    #[test]
    fn test_msg_id_try_from_unknown() {
        assert!(MsgId::try_from(0xEE).is_err());
    }
}
