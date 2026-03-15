#![allow(dead_code)]

/// v1.9 — Advanced PoW: GHOST Protocol + Uncle Blocks
///
/// Khác với v0.3 (longest chain, basic PoW):
///   - Uncle blocks: orphan block gần main chain được include để nhận reward
///   - GHOST selection: chọn chain nặng nhất (subchain có nhiều work nhất)
///   - Uncle reward: uncle miner nhận 7/8 block reward, proposer nhận thêm 1/32 mỗi uncle
///   - Uncle rate: điều chỉnh difficulty dựa trên tần suất uncle (Ethereum-style)
///
/// Tham khảo: Ethereum Yellow Paper, GHOST paper (Sompolinsky & Zohar 2013)

use sha2::{Sha256, Digest};
use std::collections::HashMap;

// ─── Constants ────────────────────────────────────────────────────────────────

pub const BASE_REWARD: u64    = 2_000_000_000; // 2 ETH in gwei (simplified)
pub const MAX_UNCLES: usize   = 2;             // tối đa 2 uncles/block (Ethereum)
pub const UNCLE_DEPTH: u64    = 6;             // uncle phải trong vòng 6 block
pub const UNCLE_REWARD_NUM: u64 = 7;           // uncle nhận (depth+8-uncle_height)/8 × reward
pub const UNCLE_REWARD_DEN: u64 = 8;
pub const NEPHEW_BONUS_NUM: u64 = 1;           // proposer thêm 1/32 mỗi uncle
pub const NEPHEW_BONUS_DEN: u64 = 32;

// ─── UncleBlock ───────────────────────────────────────────────────────────────

/// Orphan block hợp lệ — gần main chain, có thể được include
#[derive(Debug, Clone)]
pub struct UncleBlock {
    pub hash:      String,
    pub height:    u64,
    pub miner:     String,   // địa chỉ miner của uncle
    pub parent:    String,   // hash của parent (phải là ancestor của nephew)
    pub nonce:     u64,
    pub difficulty: u64,
}

impl UncleBlock {
    pub fn new(height: u64, miner: impl Into<String>, parent: impl Into<String>, nonce: u64, difficulty: u64) -> Self {
        let miner = miner.into();
        let parent = parent.into();
        let hash = Self::compute_hash_fields(height, &miner, &parent, nonce);
        UncleBlock { hash, height, miner, parent, nonce, difficulty }
    }

    fn compute_hash_fields(height: u64, miner: &str, parent: &str, nonce: u64) -> String {
        let mut h = Sha256::new();
        h.update(b"uncle_v19");
        h.update(height.to_le_bytes());
        h.update(miner.as_bytes());
        h.update(parent.as_bytes());
        h.update(nonce.to_le_bytes());
        hex::encode(h.finalize())
    }

    /// Uncle reward = (uncle_height + 8 - nephew_height) / 8 × base_reward
    pub fn reward(&self, nephew_height: u64) -> u64 {
        if nephew_height <= self.height || nephew_height - self.height > UNCLE_DEPTH {
            return 0;
        }
        let depth = nephew_height - self.height;  // 1..=6
        BASE_REWARD * (UNCLE_REWARD_DEN + 1 - depth) / UNCLE_REWARD_DEN
    }
}

// ─── GhostBlock ───────────────────────────────────────────────────────────────

/// Block trong GHOST chain — có thể include uncles
#[derive(Debug, Clone)]
pub struct GhostBlock {
    pub height:     u64,
    pub hash:       String,
    pub prev_hash:  String,
    pub miner:      String,
    pub nonce:      u64,
    pub difficulty: u64,
    pub timestamp:  u64,
    pub payload:    String,

    /// Uncle blocks được include trong block này (tối đa MAX_UNCLES)
    pub uncles:     Vec<UncleBlock>,

    /// Reward cho miner của block này (base + nephew bonus)
    pub miner_reward: u64,
}

impl GhostBlock {
    pub fn new(
        height: u64, prev_hash: impl Into<String>,
        miner: impl Into<String>, nonce: u64,
        difficulty: u64, timestamp: u64,
        payload: impl Into<String>,
    ) -> Self {
        let prev_hash = prev_hash.into();
        let miner = miner.into();
        let payload = payload.into();
        let hash = Self::compute_hash_fields(height, &prev_hash, &miner, nonce, timestamp, &payload);

        GhostBlock {
            height, hash, prev_hash, miner, nonce, difficulty, timestamp, payload,
            uncles: vec![],
            miner_reward: BASE_REWARD,
        }
    }

    fn compute_hash_fields(
        height: u64, prev_hash: &str, miner: &str,
        nonce: u64, timestamp: u64, payload: &str,
    ) -> String {
        let mut h = Sha256::new();
        h.update(b"ghost_v19");
        h.update(height.to_le_bytes());
        h.update(prev_hash.as_bytes());
        h.update(miner.as_bytes());
        h.update(nonce.to_le_bytes());
        h.update(timestamp.to_le_bytes());
        h.update(payload.as_bytes());
        hex::encode(h.finalize())
    }

    /// Thêm uncle vào block, tính lại rewards
    pub fn add_uncle(&mut self, uncle: UncleBlock) -> Result<(), String> {
        if self.uncles.len() >= MAX_UNCLES {
            return Err(format!("Tối đa {} uncles/block", MAX_UNCLES));
        }
        if self.uncles.iter().any(|u| u.hash == uncle.hash) {
            return Err("Uncle đã được include".to_string());
        }
        // Uncle phải trong vòng UNCLE_DEPTH
        if self.height <= uncle.height || self.height - uncle.height > UNCLE_DEPTH {
            return Err(format!(
                "Uncle height {} quá xa block height {} (max depth {})",
                uncle.height, self.height, UNCLE_DEPTH
            ));
        }

        // Nephew bonus: +1/32 base reward mỗi uncle
        self.miner_reward += BASE_REWARD * NEPHEW_BONUS_NUM / NEPHEW_BONUS_DEN;
        self.uncles.push(uncle);
        Ok(())
    }

    /// Kiểm tra PoW: hash phải bắt đầu bằng đủ số zero
    pub fn meets_difficulty(&self) -> bool {
        let leading_zeros = self.difficulty as usize;
        self.hash.starts_with(&"0".repeat(leading_zeros))
    }
}

// ─── Mining ───────────────────────────────────────────────────────────────────

/// Mine 1 block với GHOST-style PoW
pub fn mine_ghost_block(
    height: u64,
    prev_hash: &str,
    miner: &str,
    payload: &str,
    difficulty: u64,
    timestamp: u64,
) -> GhostBlock {
    let mut nonce = 0u64;
    loop {
        let block = GhostBlock::new(height, prev_hash, miner, nonce, difficulty, timestamp, payload);
        if block.meets_difficulty() {
            return block;
        }
        nonce += 1;
    }
}

// ─── GhostChain ───────────────────────────────────────────────────────────────

/// Chain GHOST — chọn fork nặng nhất (tổng work của toàn cây)
///
/// GHOST score của 1 node = số lượng block trong subtree của nó (bao gồm uncles)
pub struct GhostChain {
    /// Main chain (canonical)
    pub main_chain: Vec<GhostBlock>,

    /// Tất cả blocks đã biết (bao gồm orphans / forks) — key = hash
    pub all_blocks: HashMap<String, GhostBlock>,

    /// Uncle pool: orphan blocks chờ được include
    pub uncle_pool: Vec<UncleBlock>,

    pub difficulty:  u64,
    pub uncle_count: u64,  // tổng số uncles đã được include
    pub block_count: u64,  // tổng số blocks đã mine
}

impl GhostChain {
    pub fn new(difficulty: u64) -> Self {
        let genesis = GhostBlock::new(0, "0000000000000000", "satoshi", 0, difficulty, 0, "Genesis");
        let mut all_blocks = HashMap::new();
        all_blocks.insert(genesis.hash.clone(), genesis.clone());

        GhostChain {
            main_chain: vec![genesis],
            all_blocks,
            uncle_pool: vec![],
            difficulty,
            uncle_count: 0,
            block_count: 1,
        }
    }

    pub fn tip(&self) -> &GhostBlock {
        self.main_chain.last().unwrap()
    }

    pub fn height(&self) -> u64 {
        self.tip().height
    }

    /// Mine và thêm block tiếp theo vào main chain
    /// Tự động include uncles từ uncle_pool nếu hợp lệ
    pub fn mine_next(&mut self, miner: &str, payload: &str, timestamp: u64) -> String {
        let prev_hash = self.tip().hash.clone();
        let height = self.height() + 1;

        let mut block = mine_ghost_block(height, &prev_hash, miner, payload, self.difficulty, timestamp);

        // Include uncles từ pool (hợp lệ, chưa include, trong vòng UNCLE_DEPTH)
        let included_hashes = self.main_chain.iter()
            .flat_map(|b| b.uncles.iter().map(|u| u.hash.clone()))
            .collect::<std::collections::HashSet<_>>();

        let eligible: Vec<UncleBlock> = self.uncle_pool.iter()
            .filter(|u| {
                !included_hashes.contains(&u.hash)
                && height > u.height
                && height - u.height <= UNCLE_DEPTH
            })
            .take(MAX_UNCLES)
            .cloned()
            .collect();

        for uncle in eligible {
            let _ = block.add_uncle(uncle);
        }

        self.uncle_count += block.uncles.len() as u64;
        self.block_count += 1;

        let hash = block.hash.clone();
        self.all_blocks.insert(hash.clone(), block.clone());
        self.main_chain.push(block);
        hash
    }

    /// Thêm orphan block vào uncle pool (simulating receiving a competing block)
    pub fn add_orphan(&mut self, uncle: UncleBlock) {
        if !self.uncle_pool.iter().any(|u| u.hash == uncle.hash) {
            self.uncle_pool.push(uncle);
        }
    }

    /// GHOST weight: tổng số blocks + uncles trong cây từ genesis
    /// (simplified: main chain length + tổng uncles included)
    pub fn ghost_weight(&self) -> u64 {
        self.block_count + self.uncle_count
    }

    /// Uncle rate = uncles / (blocks + uncles) — dùng để điều chỉnh difficulty
    pub fn uncle_rate(&self) -> f64 {
        if self.ghost_weight() == 0 { return 0.0; }
        self.uncle_count as f64 / self.ghost_weight() as f64
    }

    /// Tổng rewards đã phát ra (main + uncle rewards)
    pub fn total_rewards(&self) -> u64 {
        let main_rewards: u64 = self.main_chain.iter().map(|b| b.miner_reward).sum();
        let uncle_rewards: u64 = self.main_chain.iter()
            .flat_map(|b| b.uncles.iter().map(|u| u.reward(b.height)))
            .sum();
        main_rewards + uncle_rewards
    }

    /// Rewards breakdown per miner
    pub fn rewards_per_miner(&self) -> HashMap<String, u64> {
        let mut rewards: HashMap<String, u64> = HashMap::new();

        for block in &self.main_chain {
            *rewards.entry(block.miner.clone()).or_insert(0) += block.miner_reward;
            for uncle in &block.uncles {
                *rewards.entry(uncle.miner.clone()).or_insert(0) += uncle.reward(block.height);
            }
        }
        rewards
    }
}

// ─── Difficulty Adjustment ────────────────────────────────────────────────────

/// Điều chỉnh difficulty dựa trên uncle rate (Ethereum-style)
///
/// Nếu uncle_rate cao → network quá nhanh → tăng difficulty
/// Nếu uncle_rate thấp → network ổn → giữ nguyên hoặc giảm nhẹ
pub fn adjust_difficulty(current: u64, uncle_rate: f64) -> u64 {
    if uncle_rate > 0.10 {
        // Uncle rate > 10%: tăng difficulty
        (current + 1).min(8)
    } else if uncle_rate < 0.02 && current > 1 {
        // Uncle rate rất thấp: giảm nhẹ
        current - 1
    } else {
        current
    }
}
