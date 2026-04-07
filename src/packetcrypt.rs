#![allow(dead_code)]

/// v4.1 — OCEIF PoW (bandwidth-hard announcement mining)
///
/// Thuật toán PoW bandwidth-hard của OCEIF network.
///
/// Cơ chế:
///   1. Announcement Miners tạo "announcements" — mỗi ann prove computation + bandwidth
///   2. Block Miners thu thập announcements từ ann miners
///   3. Càng nhiều announcements → độ khó block càng giảm
///      effective_bits = base_bits - floor(log2(ann_count + 1))
///   4. Block hash commit vào announcement Merkle root


// ─── Difficulty helpers ────────────────────────────────────────────────────────

/// Đếm số leading zero bits trong hash
pub fn leading_zero_bits(hash: &[u8]) -> u32 {
    let mut bits = 0u32;
    for byte in hash {
        if *byte == 0 {
            bits += 8;
        } else {
            bits += byte.leading_zeros();
            break;
        }
    }
    bits
}

/// Kiểm tra hash có đủ leading zero bits không
pub fn meets_difficulty(hash: &[u8], required_bits: u32) -> bool {
    leading_zero_bits(hash) >= required_bits
}

/// Tính effective difficulty khi có N announcements
/// Công thức: effective_bits = base_bits - floor(log2(ann_count + 1))
/// Ví dụ:
///   0 ann  → -0  (base)
///   1 ann  → -1  (2^1)
///   3 ann  → -2  (2^2 = 4 > 3, nên floor(log2(4)) = 2)
///   7 ann  → -3
///   15 ann → -4
pub fn effective_difficulty(base_bits: u32, ann_count: u32) -> u32 {
    let reduction = if ann_count == 0 {
        0
    } else {
        (u32::BITS - (ann_count + 1).leading_zeros()) as u32 - 1
    };
    base_bits.saturating_sub(reduction)
}

// ─── Announcement ─────────────────────────────────────────────────────────────

/// Một PacketCrypt Announcement
/// Ann miners tạo announcements, block miners thu thập chúng
#[derive(Clone, Debug)]
pub struct Announcement {
    /// Hash của block cha (ann phải reference block gần nhất)
    pub parent_block_hash: [u8; 32],
    /// Identity của ann miner (pubkey hash 32 bytes)
    pub miner_key: [u8; 32],
    /// Nonce để đạt difficulty
    pub nonce: u64,
    /// Hash đạt ann_difficulty — kết quả proof of work
    pub work_hash: [u8; 32],
}

impl Announcement {
    /// Tính hash của announcement (không dùng stored work_hash)
    pub fn compute_hash(parent: &[u8; 32], key: &[u8; 32], nonce: u64) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"OCEIF_Ann_v1:");
        h.update(parent);
        h.update(key);
        h.update(&nonce.to_le_bytes());
        *h.finalize().as_bytes()
    }

    /// Xác minh announcement hợp lệ với min_difficulty
    pub fn verify(&self, min_difficulty: u32) -> bool {
        let expected = Self::compute_hash(
            &self.parent_block_hash,
            &self.miner_key,
            self.nonce,
        );
        expected == self.work_hash && meets_difficulty(&self.work_hash, min_difficulty)
    }

    /// Hex string của work_hash
    pub fn work_hex(&self) -> String {
        hex::encode(self.work_hash)
    }
}

// ─── Announcement Miner ───────────────────────────────────────────────────────

/// Ann Miner: tạo announcements có độ khó tối thiểu
pub struct AnnouncementMiner {
    /// Identity key (pubkey hash hex → 32 bytes)
    pub key: [u8; 32],
    /// Số ann đã tạo
    pub total_mined: u64,
}

impl AnnouncementMiner {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key, total_mined: 0 }
    }

    /// Tạo announcement từ parent_block_hash với min_difficulty
    pub fn mine(&mut self, parent_block_hash: [u8; 32], min_difficulty: u32) -> Announcement {
        let mut nonce = self.total_mined * 1_000_000; // tránh nonce collision giữa các miners
        loop {
            let hash = Announcement::compute_hash(&parent_block_hash, &self.key, nonce);
            if meets_difficulty(&hash, min_difficulty) {
                self.total_mined += 1;
                return Announcement {
                    parent_block_hash,
                    miner_key: self.key,
                    nonce,
                    work_hash: hash,
                };
            }
            nonce += 1;
        }
    }
}

// ─── Announcement Pool ────────────────────────────────────────────────────────

/// Tập hợp announcements hợp lệ cho một block miner
pub struct AnnouncementPool {
    pub announcements: Vec<Announcement>,
    pub min_difficulty: u32,
}

impl AnnouncementPool {
    pub fn new(min_difficulty: u32) -> Self {
        Self {
            announcements: Vec::new(),
            min_difficulty,
        }
    }

    /// Thêm announcement (chỉ accept nếu verify pass)
    pub fn add(&mut self, ann: Announcement) -> bool {
        if ann.verify(self.min_difficulty) {
            self.announcements.push(ann);
            true
        } else {
            false
        }
    }

    /// Số announcements hợp lệ
    pub fn count(&self) -> u32 {
        self.announcements.len() as u32
    }

    /// Tính Merkle root của tất cả work_hash
    pub fn merkle_root(&self) -> [u8; 32] {
        if self.announcements.is_empty() {
            return [0u8; 32];
        }
        let mut layer: Vec<[u8; 32]> = self.announcements.iter()
            .map(|a| a.work_hash)
            .collect();
        while layer.len() > 1 {
            let mut next = Vec::new();
            for pair in layer.chunks(2) {
                let mut h = blake3::Hasher::new();
                h.update(&pair[0]);
                h.update(pair.get(1).unwrap_or(&pair[0]));
                next.push(*h.finalize().as_bytes());
            }
            layer = next;
        }
        layer[0]
    }
}

// ─── PacketCrypt Block ────────────────────────────────────────────────────────

/// Một block được mine với PacketCrypt PoW
#[derive(Clone, Debug)]
pub struct PcBlock {
    pub height: u64,
    pub prev_hash: String,
    pub miner_address: String,
    /// Merkle root của announcements (commit vào block)
    pub ann_root: String,
    /// Số announcements được sử dụng
    pub ann_count: u32,
    /// Độ khó cơ bản của chain
    pub base_difficulty: u32,
    /// Độ khó thực tế sau khi tính announcements
    pub effective_difficulty: u32,
    pub nonce: u64,
    pub hash: String,
    pub timestamp: u64,
}

impl PcBlock {
    /// Tính hash của block (commit vào ann_root)
    pub fn compute_hash(
        height: u64,
        prev_hash: &str,
        miner_address: &str,
        ann_root: &str,
        ann_count: u32,
        nonce: u64,
    ) -> String {
        let mut h = blake3::Hasher::new();
        h.update(b"OCEIF_Block_v1:");
        h.update(&height.to_le_bytes());
        h.update(prev_hash.as_bytes());
        h.update(miner_address.as_bytes());
        h.update(ann_root.as_bytes());
        h.update(&ann_count.to_le_bytes());
        h.update(&nonce.to_le_bytes());
        hex::encode(h.finalize().as_bytes())
    }

    pub fn verify(&self) -> bool {
        let expected = Self::compute_hash(
            self.height,
            &self.prev_hash,
            &self.miner_address,
            &self.ann_root,
            self.ann_count,
            self.nonce,
        );
        if expected != self.hash {
            return false;
        }
        let hash_bytes = hex::decode(&self.hash).unwrap_or_default();
        meets_difficulty(&hash_bytes, self.effective_difficulty)
    }
}

// ─── Block Miner ──────────────────────────────────────────────────────────────

/// Block Miner: thu thập announcements → mine block với reduced difficulty
pub struct BlockMiner {
    pub pool: AnnouncementPool,
    pub base_difficulty: u32,
}

impl BlockMiner {
    pub fn new(base_difficulty: u32, ann_min_difficulty: u32) -> Self {
        Self {
            pool: AnnouncementPool::new(ann_min_difficulty),
            base_difficulty,
        }
    }

    pub fn add_announcement(&mut self, ann: Announcement) -> bool {
        self.pool.add(ann)
    }

    pub fn current_effective_difficulty(&self) -> u32 {
        effective_difficulty(self.base_difficulty, self.pool.count())
    }

    /// Mine một block với tất cả announcements hiện có
    pub fn mine(
        &self,
        height: u64,
        prev_hash: &str,
        miner_address: &str,
    ) -> PcBlock {
        let ann_root   = hex::encode(self.pool.merkle_root());
        let ann_count  = self.pool.count();
        let eff_diff   = effective_difficulty(self.base_difficulty, ann_count);
        let timestamp  = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut nonce = 0u64;
        loop {
            let hash = PcBlock::compute_hash(
                height, prev_hash, miner_address, &ann_root, ann_count, nonce,
            );
            let hash_bytes = hex::decode(&hash).unwrap_or_default();
            if meets_difficulty(&hash_bytes, eff_diff) {
                return PcBlock {
                    height,
                    prev_hash: prev_hash.to_string(),
                    miner_address: miner_address.to_string(),
                    ann_root,
                    ann_count,
                    base_difficulty: self.base_difficulty,
                    effective_difficulty: eff_diff,
                    nonce,
                    hash,
                    timestamp,
                };
            }
            nonce += 1;
        }
    }
}

// ─── PacketCrypt Chain ────────────────────────────────────────────────────────

/// Một chain đơn giản dùng PacketCrypt PoW
pub struct PcChain {
    pub blocks: Vec<PcBlock>,
    pub base_difficulty: u32,
    pub ann_min_difficulty: u32,
}

impl PcChain {
    pub fn new(base_difficulty: u32, ann_min_difficulty: u32) -> Self {
        // Genesis block (không cần ann)
        let genesis = PcBlock {
            height: 0,
            prev_hash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            miner_address: "genesis".to_string(),
            ann_root: hex::encode([0u8; 32]),
            ann_count: 0,
            base_difficulty,
            effective_difficulty: 0,  // genesis không cần PoW
            nonce: 0,
            hash: hex::encode(blake3::hash(b"PKT_Genesis_v4.1").as_bytes()),
            timestamp: 0,
        };
        Self {
            blocks: vec![genesis],
            base_difficulty,
            ann_min_difficulty,
        }
    }

    pub fn tip_hash(&self) -> String {
        self.blocks.last().map(|b| b.hash.clone()).unwrap_or_default()
    }

    pub fn height(&self) -> u64 {
        self.blocks.len() as u64 - 1
    }

    pub fn add_block(&mut self, block: PcBlock) -> bool {
        if block.prev_hash != self.tip_hash() {
            return false;
        }
        if block.height != self.height() + 1 {
            return false;
        }
        if !block.verify() {
            return false;
        }
        self.blocks.push(block);
        true
    }
}
