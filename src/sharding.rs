#![allow(dead_code)]

/// v2.1 — Sharding
///
/// Kiến trúc:
///
///   ┌─────────────────────────────────────────────────────────────┐
///   │  BEACON CHAIN  (điều phối toàn bộ)                         │
///   │  - Lưu header của tất cả shard blocks                      │
///   │  - Assign validators vào shards mỗi epoch                  │
///   │  - Finalize cross-shard receipts                           │
///   └─────────┬──────────┬──────────┬──────────┬─────────────────┘
///             │          │          │          │
///         Shard 0    Shard 1    Shard 2    Shard 3
///         (TXs)      (TXs)      (TXs)      (TXs)
///
/// Cross-shard TX flow:
///   1. Alice (shard 0) gửi TX → debit shard 0
///   2. Shard 0 emit CrossShardReceipt
///   3. Beacon chain include receipt
///   4. Shard 1 consume receipt → credit Bob
///
/// Tham khảo: Ethereum 2.0 Phase 1 sharding design

use sha2::{Sha256, Digest};
use std::collections::HashMap;

// ─── Constants ────────────────────────────────────────────────────────────────

pub const NUM_SHARDS: u32   = 4;     // số shard trong network
pub const EPOCH_LEN:  u64   = 8;     // blocks/epoch (validator reassignment)

// ─── ShardId ──────────────────────────────────────────────────────────────────

pub type ShardId = u32;

/// Xác định TX thuộc shard nào dựa trên địa chỉ
pub fn shard_of(address: &str) -> ShardId {
    let mut h = Sha256::new();
    h.update(address.as_bytes());
    let bytes = h.finalize();
    u32::from_le_bytes(bytes[..4].try_into().unwrap()) % NUM_SHARDS
}

// ─── ShardTx ──────────────────────────────────────────────────────────────────

/// Giao dịch trong 1 shard — có thể là intra-shard hoặc cross-shard
#[derive(Debug, Clone)]
pub struct ShardTx {
    pub from:      String,
    pub to:        String,
    pub amount:    u64,
    pub from_shard: ShardId,
    pub to_shard:   ShardId,
    pub tx_hash:   String,
}

impl ShardTx {
    pub fn new(from: impl Into<String>, to: impl Into<String>, amount: u64) -> Self {
        let from = from.into();
        let to   = to.into();
        let from_shard = shard_of(&from);
        let to_shard   = shard_of(&to);

        let mut h = Sha256::new();
        h.update(from.as_bytes());
        h.update(to.as_bytes());
        h.update(amount.to_le_bytes());
        let tx_hash = hex::encode(h.finalize());

        ShardTx { from, to, amount, from_shard, to_shard, tx_hash }
    }

    pub fn is_cross_shard(&self) -> bool {
        self.from_shard != self.to_shard
    }
}

// ─── CrossShardReceipt ────────────────────────────────────────────────────────

/// Receipt do shard nguồn emit sau khi debit sender
/// Shard đích sẽ consume để credit receiver
#[derive(Debug, Clone)]
pub struct CrossShardReceipt {
    pub receipt_id:   String,
    pub from_shard:   ShardId,
    pub to_shard:     ShardId,
    pub to_address:   String,
    pub amount:       u64,
    pub source_block: u64,   // height của shard block đã debit
    pub consumed:     bool,
}

impl CrossShardReceipt {
    pub fn new(tx: &ShardTx, source_block: u64) -> Self {
        let mut h = Sha256::new();
        h.update(b"receipt_v21");
        h.update(tx.tx_hash.as_bytes());
        h.update(source_block.to_le_bytes());
        let receipt_id = hex::encode(h.finalize());

        CrossShardReceipt {
            receipt_id,
            from_shard:   tx.from_shard,
            to_shard:     tx.to_shard,
            to_address:   tx.to.clone(),
            amount:       tx.amount,
            source_block,
            consumed:     false,
        }
    }
}

// ─── ShardBlock ───────────────────────────────────────────────────────────────

/// Block trong 1 shard chain
#[derive(Debug, Clone)]
pub struct ShardBlock {
    pub shard_id:    ShardId,
    pub height:      u64,
    pub prev_hash:   String,
    pub proposer:    String,
    pub txs:         Vec<ShardTx>,
    pub receipts_out: Vec<CrossShardReceipt>,  // cross-shard receipts emitted
    pub receipts_in:  Vec<String>,             // receipt IDs consumed từ beacon
    pub state_root:  String,    // simplified: hash của tất cả balances
    pub hash:        String,
}

impl ShardBlock {
    pub fn new(
        shard_id: ShardId,
        height: u64,
        prev_hash: impl Into<String>,
        proposer: impl Into<String>,
        txs: Vec<ShardTx>,
        receipts_in: Vec<String>,
        balances: &HashMap<String, u64>,
    ) -> Self {
        let prev_hash = prev_hash.into();
        let proposer  = proposer.into();

        // Emit cross-shard receipts cho các TX cross-shard
        let receipts_out: Vec<CrossShardReceipt> = txs.iter()
            .filter(|tx| tx.is_cross_shard() && tx.from_shard == shard_id)
            .map(|tx| CrossShardReceipt::new(tx, height))
            .collect();

        let state_root = Self::compute_state_root(balances);

        let mut block = ShardBlock {
            shard_id, height, prev_hash, proposer,
            txs, receipts_out, receipts_in,
            state_root,
            hash: String::new(),
        };
        block.hash = block.compute_hash();
        block
    }

    fn compute_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(b"shard_v21");
        h.update(self.shard_id.to_le_bytes());
        h.update(self.height.to_le_bytes());
        h.update(self.prev_hash.as_bytes());
        h.update(self.proposer.as_bytes());
        h.update(self.state_root.as_bytes());
        for tx in &self.txs {
            h.update(tx.tx_hash.as_bytes());
        }
        hex::encode(h.finalize())
    }

    fn compute_state_root(balances: &HashMap<String, u64>) -> String {
        let mut keys: Vec<_> = balances.keys().collect();
        keys.sort();
        let mut h = Sha256::new();
        for k in keys {
            h.update(k.as_bytes());
            h.update(balances[k].to_le_bytes());
        }
        hex::encode(h.finalize())
    }
}

// ─── ShardChain ───────────────────────────────────────────────────────────────

/// Chain của 1 shard — quản lý state + block production
pub struct ShardChain {
    pub shard_id:  ShardId,
    pub blocks:    Vec<ShardBlock>,
    pub balances:  HashMap<String, u64>,
    pub pending_receipts_out: Vec<CrossShardReceipt>,  // chờ beacon include
}

impl ShardChain {
    pub fn new(shard_id: ShardId) -> Self {
        ShardChain {
            shard_id,
            blocks:   vec![],
            balances: HashMap::new(),
            pending_receipts_out: vec![],
        }
    }

    pub fn height(&self) -> u64 { self.blocks.len() as u64 }

    pub fn tip_hash(&self) -> String {
        self.blocks.last().map(|b| b.hash.clone())
            .unwrap_or_else(|| format!("genesis_shard_{}", self.shard_id))
    }

    /// Thêm balance cho 1 account (genesis/faucet)
    pub fn fund(&mut self, address: &str, amount: u64) {
        *self.balances.entry(address.to_string()).or_insert(0) += amount;
    }

    /// Produce 1 block mới với danh sách TXs
    /// receipts_in: cross-shard receipts từ beacon chain để consume
    pub fn produce_block(
        &mut self,
        proposer: &str,
        txs: Vec<ShardTx>,
        receipts_in: Vec<CrossShardReceipt>,
    ) -> Result<String, String> {
        // Apply intra-shard TXs
        for tx in &txs {
            if tx.from_shard == self.shard_id && tx.to_shard == self.shard_id {
                let from_bal = self.balances.get(&tx.from).copied().unwrap_or(0);
                if from_bal < tx.amount {
                    return Err(format!("{} không đủ balance: {} < {}", tx.from, from_bal, tx.amount));
                }
                *self.balances.entry(tx.from.clone()).or_insert(0) -= tx.amount;
                *self.balances.entry(tx.to.clone()).or_insert(0) += tx.amount;
            } else if tx.from_shard == self.shard_id {
                // Cross-shard: chỉ debit, emit receipt
                let from_bal = self.balances.get(&tx.from).copied().unwrap_or(0);
                if from_bal < tx.amount {
                    return Err(format!("{} không đủ balance cho cross-shard TX", tx.from));
                }
                *self.balances.entry(tx.from.clone()).or_insert(0) -= tx.amount;
            }
        }

        // Consume incoming cross-shard receipts → credit receivers
        let receipt_ids: Vec<String> = receipts_in.iter().map(|r| r.receipt_id.clone()).collect();
        for receipt in &receipts_in {
            if receipt.to_shard == self.shard_id {
                *self.balances.entry(receipt.to_address.clone()).or_insert(0) += receipt.amount;
            }
        }

        let block = ShardBlock::new(
            self.shard_id,
            self.height(),
            self.tip_hash(),
            proposer,
            txs,
            receipt_ids,
            &self.balances,
        );

        let hash = block.hash.clone();

        // Collect outgoing receipts
        for r in &block.receipts_out {
            self.pending_receipts_out.push(r.clone());
        }

        self.blocks.push(block);
        Ok(hash)
    }

    pub fn balance_of(&self, address: &str) -> u64 {
        self.balances.get(address).copied().unwrap_or(0)
    }
}

// ─── BeaconBlock ─────────────────────────────────────────────────────────────

/// Block trên beacon chain — không chứa TXs, chứa shard block headers
#[derive(Debug, Clone)]
pub struct BeaconBlock {
    pub height:       u64,
    pub prev_hash:    String,
    pub proposer:     String,

    /// Shard block hash được finalize tại beacon height này
    pub shard_headers: HashMap<ShardId, String>,

    /// Cross-shard receipts được relay qua beacon
    pub receipts:     Vec<CrossShardReceipt>,

    pub hash:         String,
}

impl BeaconBlock {
    pub fn new(
        height: u64,
        prev_hash: impl Into<String>,
        proposer: impl Into<String>,
        shard_headers: HashMap<ShardId, String>,
        receipts: Vec<CrossShardReceipt>,
    ) -> Self {
        let prev_hash = prev_hash.into();
        let proposer  = proposer.into();

        let mut b = BeaconBlock {
            height, prev_hash, proposer,
            shard_headers, receipts,
            hash: String::new(),
        };
        b.hash = b.compute_hash();
        b
    }

    fn compute_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(b"beacon_v21");
        h.update(self.height.to_le_bytes());
        h.update(self.prev_hash.as_bytes());
        h.update(self.proposer.as_bytes());
        // Sort shard headers deterministically
        let mut ids: Vec<ShardId> = self.shard_headers.keys().copied().collect();
        ids.sort();
        for id in ids {
            h.update(id.to_le_bytes());
            h.update(self.shard_headers[&id].as_bytes());
        }
        for r in &self.receipts {
            h.update(r.receipt_id.as_bytes());
        }
        hex::encode(h.finalize())
    }
}

// ─── BeaconChain ─────────────────────────────────────────────────────────────

/// Beacon chain điều phối toàn bộ shards
pub struct BeaconChain {
    pub blocks:   Vec<BeaconBlock>,
    pub shards:   HashMap<ShardId, ShardChain>,

    /// Receipts đã được beacon include nhưng chưa consumed bởi shard đích
    pub pending_receipts: Vec<CrossShardReceipt>,

    /// Validator → shard assignment (epoch-based)
    pub validator_assignment: HashMap<String, ShardId>,
}

impl BeaconChain {
    pub fn new() -> Self {
        let mut shards = HashMap::new();
        for id in 0..NUM_SHARDS {
            shards.insert(id, ShardChain::new(id));
        }

        BeaconChain {
            blocks:               vec![],
            shards,
            pending_receipts:     vec![],
            validator_assignment: HashMap::new(),
        }
    }

    pub fn height(&self) -> u64 { self.blocks.len() as u64 }

    pub fn tip_hash(&self) -> String {
        self.blocks.last().map(|b| b.hash.clone())
            .unwrap_or_else(|| "genesis_beacon".to_string())
    }

    /// Assign validators vào shards (round-robin theo epoch)
    pub fn assign_validators(&mut self, validators: &[&str], epoch: u64) {
        self.validator_assignment.clear();
        for (i, v) in validators.iter().enumerate() {
            let shard = ((i as u64 + epoch) as u32) % NUM_SHARDS;
            self.validator_assignment.insert(v.to_string(), shard);
        }
    }

    /// Produce 1 beacon block — collect shard headers + relay receipts
    pub fn produce_beacon_block(&mut self, proposer: &str) -> String {
        let height = self.height();

        // Collect shard block headers
        let shard_headers: HashMap<ShardId, String> = self.shards.iter()
            .map(|(id, sc)| (*id, sc.tip_hash()))
            .collect();

        // Collect pending cross-shard receipts từ tất cả shards
        let mut new_receipts = vec![];
        for sc in self.shards.values_mut() {
            new_receipts.extend(sc.pending_receipts_out.drain(..));
        }
        self.pending_receipts.extend(new_receipts.clone());

        let block = BeaconBlock::new(height, self.tip_hash(), proposer, shard_headers, new_receipts);
        let hash  = block.hash.clone();
        self.blocks.push(block);
        hash
    }

    /// Consume pending receipts vào shard đích
    pub fn deliver_receipts(&mut self, proposer: &str) {
        // Group receipts by to_shard
        let mut by_shard: HashMap<ShardId, Vec<CrossShardReceipt>> = HashMap::new();
        for r in self.pending_receipts.drain(..) {
            by_shard.entry(r.to_shard).or_default().push(r);
        }

        for (shard_id, receipts) in by_shard {
            if let Some(sc) = self.shards.get_mut(&shard_id) {
                let _ = sc.produce_block(proposer, vec![], receipts);
            }
        }
    }

    /// Tổng balance của 1 account trên tất cả shards
    pub fn total_balance(&self, address: &str) -> u64 {
        self.shards.values().map(|sc| sc.balance_of(address)).sum()
    }
}

// ─── ValidatorAssignment ─────────────────────────────────────────────────────

/// Hiển thị validator assignment cho 1 epoch
pub fn display_assignment(assignment: &HashMap<String, ShardId>) {
    let mut by_shard: HashMap<ShardId, Vec<&String>> = HashMap::new();
    for (v, s) in assignment {
        by_shard.entry(*s).or_default().push(v);
    }
    let mut shard_ids: Vec<ShardId> = by_shard.keys().copied().collect();
    shard_ids.sort();
    for sid in shard_ids {
        let mut validators = by_shard[&sid].clone();
        validators.sort();
        println!("    Shard {}: {:?}", sid, validators);
    }
}
