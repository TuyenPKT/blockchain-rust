use serde::{Serialize, Deserialize};
use crate::block::Block;
use crate::transaction::Transaction;

/// Tất cả các loại message giữa các node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// Node mới kết nối, giới thiệu bản thân
    Hello { version: u32, host: String, port: u16 },

    /// Yêu cầu danh sách peers từ node khác
    GetPeers,

    /// Trả về danh sách peers đang biết
    Peers { addrs: Vec<String> },

    /// Thông báo có block mới vừa được mine
    NewBlock { block: Block },

    /// Yêu cầu đồng bộ chain từ height nhất định
    GetBlocks { from_index: u64 },

    /// Trả về danh sách blocks
    Blocks { blocks: Vec<Block> },

    /// Broadcast transaction mới vào network
    NewTransaction { tx: Transaction },

    /// v4.3: Query chain height (không cần download block)
    GetHeight,
    Height { height: u64 },

    /// v4.3: Ping để kiểm tra node còn sống không
    Ping,
    Pong,

    /// v4.5: Miner yêu cầu danh sách TX đang chờ trong mempool
    GetMempool,
    /// v4.5: Node trả về danh sách TX trong mempool (tối đa 500 TX)
    MempoolTxs { txs: Vec<Transaction> },

    /// v4.8: Query số lượng peers đang kết nối
    GetPeerCount,
    /// v4.8: Trả về peer count
    PeerCount { count: usize },
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        let mut data = serde_json::to_vec(self).unwrap_or_default();
        // Thêm newline để dễ parse từng message qua TCP stream
        data.push(b'\n');
        data
    }

    pub fn deserialize(data: &[u8]) -> Option<Self> {
        serde_json::from_slice(data).ok()
    }
}
