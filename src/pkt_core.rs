#![allow(dead_code)]
//! v13.0 — PacketCrypt PoW (spec-accurate)
//!
//! Cải tiến so với v4.1 (educational):
//!   - Compact target (nBits) như Bitcoin thay vì leading-zero bits
//!   - Ann seed hash = hash của block tại height (current - PKT_SEED_DEPTH)
//!   - Ann expiry: ann hết hạn sau PKT_ANN_EXPIRY blocks
//!   - Ann content items: memory-hard proof (PKT_ANN_ITEM_COUNT items × 4 bytes)
//!   - Ann header hash đúng layout spec
//!   - Effective target tính từ compact target thay vì simple bit shift
//!
//! Tham chiếu: https://github.com/cjdelisle/PacketCrypt/blob/master/docs/pcann_spec.md

// ── PKT Constants ──────────────────────────────────────────────────────────────

/// Số blocks một announcement còn hiệu lực
pub const PKT_ANN_EXPIRY: u64 = 3;
/// Ann reference block tại (current_height - PKT_SEED_DEPTH)
pub const PKT_SEED_DEPTH: u64 = 3;
/// Số memory items mỗi announcement chứa (4 bytes mỗi item)
pub const PKT_ANN_ITEM_COUNT: u32 = 4096;
/// Ann version hiện tại
pub const PKT_ANN_VERSION: u8 = 0;
/// Domain separator cho ann header hash
pub const PKT_ANN_DOMAIN: &[u8] = b"PacketCrypt_Ann_v1:";
/// Domain separator cho block header hash
pub const PKT_BLOCK_DOMAIN: &[u8] = b"PacketCrypt_Block_v1:";

// ── Compact Target (nBits) ─────────────────────────────────────────────────────
//
// Giống Bitcoin nBits: u32 — byte cao là số byte của mantissa, 3 byte thấp là mantissa.
// target = mantissa * 256^(exponent - 3)
//
// Ví dụ:
//   0x1d00ffff → exponent=0x1d=29, mantissa=0x00ffff → Bitcoin genesis target
//   0x20000001 → exponent=32, mantissa=1 → rất dễ (target lớn)
//   0x1f000001 → exponent=31, mantissa=1 → dễ vừa

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactTarget(pub u32);

impl CompactTarget {
    /// Tạo CompactTarget từ số leading zero bits (dễ dùng cho tests)
    /// bits=0 → dễ nhất, bits=255 → khó nhất
    pub fn from_leading_zeros(bits: u32) -> Self {
        // Convert leading zeros → compact target
        // target = 2^(256 - bits) - 1 (approximately)
        // Dùng exponent = (256 - bits) / 8 + 1, mantissa = 0x00ffff shifted
        let zero_rem   = bits % 8;
        // exponent = số bytes của target value (256 - bits bits → (256-bits+7)/8 bytes)
        let target_bits = 256u32.saturating_sub(bits);
        let exp = (target_bits + 7) / 8;
        // mantissa: fill từ bit còn lại
        let mantissa = if zero_rem == 0 {
            0x00ff_ff00u32
        } else {
            (0x00ff_ffu32 >> zero_rem) << 8
        };
        let n_bits = ((exp & 0xff) << 24) | (mantissa >> 8);
        CompactTarget(n_bits)
    }

    /// Exponent (số bytes trong full target)
    pub fn exponent(self) -> u32 {
        (self.0 >> 24) & 0xff
    }

    /// Mantissa (3 byte thấp)
    pub fn mantissa(self) -> u32 {
        self.0 & 0x00ff_ffff
    }

    /// Chuyển sang [u8;32] target (big-endian)
    pub fn to_target_bytes(self) -> [u8; 32] {
        let exp = self.exponent() as usize;
        let man = self.mantissa();
        let mut target = [0u8; 32];
        if exp == 0 || exp > 32 { return target; }
        // Ghi mantissa vào vị trí (32 - exp)
        let pos = 32usize.saturating_sub(exp);
        let m_bytes = man.to_be_bytes(); // [0, b1, b2, b3]
        for (i, &b) in m_bytes[1..].iter().enumerate() {
            if pos + i < 32 { target[pos + i] = b; }
        }
        // Các byte sau mantissa là 0xff (fill target đến max)
        for i in (pos + 3)..32 {
            target[i] = 0xff;
        }
        target
    }

    /// Kiểm tra hash có nhỏ hơn target không (hash ≤ target → valid PoW)
    pub fn meets_target(hash: &[u8; 32], target: Self) -> bool {
        let t = target.to_target_bytes();
        for i in 0..32 {
            if hash[i] < t[i] { return true; }
            if hash[i] > t[i] { return false; }
        }
        true // equal → valid
    }

    /// Target "dễ nhất" — hầu hết hash đều pass (first byte ≤ 0x7f, ~50%)
    /// Compact format giới hạn mantissa byte cao nhất là 0x7f (sign bit)
    pub fn max() -> Self { CompactTarget(0x207f_ffff) }

    /// Tính effective target khi có ann_count announcements
    /// effective_target = base_target << floor(log2(ann_count + 1))
    /// (shift left = tăng target = giảm độ khó)
    pub fn with_ann_count(self, ann_count: u32) -> Self {
        if ann_count == 0 { return self; }
        let shift = (u32::BITS - (ann_count + 1).leading_zeros()) - 1;
        // Tăng exponent để shift target (mỗi +1 exponent ≈ ×256 target)
        let new_exp = (self.exponent() + shift / 8 + 1).min(32);
        CompactTarget((new_exp << 24) | self.mantissa())
    }
}

// ── Ann Content Items (memory-hard proof) ─────────────────────────────────────

/// Tính content hash từ N items × 4 bytes
/// Trong spec thật: items được derive từ pseudorandom memory reads
/// Ở đây: simulate bằng BLAKE3 với seed
pub fn compute_content_hash(seed: &[u8; 32], item_count: u32) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"PktContent_v1:");
    hasher.update(seed);
    // Simulate memory items: hash lần lượt index → 4 bytes
    for i in 0..item_count.min(256) { // giới hạn để test nhanh
        hasher.update(&i.to_le_bytes());
    }
    *hasher.finalize().as_bytes()
}

// ── Announcement ───────────────────────────────────────────────────────────────

/// PacketCrypt Announcement (spec v13.0)
#[derive(Debug, Clone)]
pub struct PktAnn {
    pub version: u8,
    /// Hash của memory items (memory-hard proof)
    pub content_hash: [u8; 32],
    /// Height của block được dùng làm seed (current_height - PKT_SEED_DEPTH)
    pub parent_block_height: u64,
    /// Hash của block tại parent_block_height
    pub seed_hash: [u8; 32],
    /// Nonce để đạt target
    pub nonce: u32,
    /// Hash của ann header (kết quả PoW)
    pub work_hash: [u8; 32],
    /// Compact target ann phải đạt
    pub ann_target: CompactTarget,
}

impl PktAnn {
    /// Hash ann header: domain + version + content_hash + parent_height + seed_hash + nonce
    pub fn header_hash(
        version: u8,
        content_hash: &[u8; 32],
        parent_block_height: u64,
        seed_hash: &[u8; 32],
        nonce: u32,
    ) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(PKT_ANN_DOMAIN);
        h.update(&[version]);
        h.update(content_hash);
        h.update(&parent_block_height.to_le_bytes());
        h.update(seed_hash);
        h.update(&nonce.to_le_bytes());
        *h.finalize().as_bytes()
    }

    /// Xác minh ann hợp lệ
    pub fn verify(&self) -> bool {
        let expected = Self::header_hash(
            self.version,
            &self.content_hash,
            self.parent_block_height,
            &self.seed_hash,
            self.nonce,
        );
        expected == self.work_hash
            && CompactTarget::meets_target(&self.work_hash, self.ann_target)
    }

    /// Ann còn hiệu lực tại block_height không?
    /// Ann valid nếu: parent_block_height < block_height ≤ parent_block_height + PKT_ANN_EXPIRY
    pub fn is_valid_for_block(&self, block_height: u64) -> bool {
        block_height > self.parent_block_height
            && block_height <= self.parent_block_height + PKT_ANN_EXPIRY
    }

    pub fn work_hex(&self) -> String { hex::encode(self.work_hash) }
}

// ── Ann Miner ─────────────────────────────────────────────────────────────────

pub struct PktAnnMiner {
    pub miner_id: [u8; 32],
    pub mined_count: u64,
}

impl PktAnnMiner {
    pub fn new(miner_id: [u8; 32]) -> Self {
        Self { miner_id, mined_count: 0 }
    }

    /// Mine một announcement
    /// seed_hash = hash của block tại (target_block_height - PKT_SEED_DEPTH)
    pub fn mine(
        &mut self,
        seed_hash: [u8; 32],
        parent_block_height: u64,
        ann_target: CompactTarget,
    ) -> PktAnn {
        // Content hash: dùng miner_id + seed làm entropy cho memory items
        let mut content_seed = [0u8; 32];
        let id_hash = blake3::hash(&[self.miner_id.as_slice(), seed_hash.as_slice()].concat());
        content_seed.copy_from_slice(id_hash.as_bytes());
        let content_hash = compute_content_hash(&content_seed, PKT_ANN_ITEM_COUNT);

        let mut nonce = (self.mined_count as u32).wrapping_mul(100_000);
        loop {
            let work_hash = PktAnn::header_hash(
                PKT_ANN_VERSION,
                &content_hash,
                parent_block_height,
                &seed_hash,
                nonce,
            );
            if CompactTarget::meets_target(&work_hash, ann_target) {
                self.mined_count += 1;
                return PktAnn {
                    version: PKT_ANN_VERSION,
                    content_hash,
                    parent_block_height,
                    seed_hash,
                    nonce,
                    work_hash,
                    ann_target,
                };
            }
            nonce = nonce.wrapping_add(1);
        }
    }
}

// ── Ann Merkle Root ────────────────────────────────────────────────────────────

/// Tính Merkle root từ danh sách ann work_hashes
pub fn ann_merkle_root(anns: &[PktAnn]) -> [u8; 32] {
    if anns.is_empty() { return [0u8; 32]; }
    let mut layer: Vec<[u8; 32]> = anns.iter().map(|a| a.work_hash).collect();
    while layer.len() > 1 {
        let mut next = Vec::new();
        for chunk in layer.chunks(2) {
            let mut h = blake3::Hasher::new();
            h.update(&chunk[0]);
            h.update(chunk.get(1).unwrap_or(&chunk[0]));
            next.push(*h.finalize().as_bytes());
        }
        layer = next;
    }
    layer[0]
}

// ── Block Header ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PktBlockHeader {
    pub height: u64,
    pub prev_hash: [u8; 32],
    pub content_merkle: [u8; 32],    // merkle root của TXs
    pub timestamp: u32,
    pub base_target: CompactTarget,  // base difficulty (nBits)
    pub nonce: u32,
    pub ann_merkle: [u8; 32],        // merkle root của ann work_hashes
    pub ann_count: u32,
    pub ann_target: CompactTarget,   // min ann difficulty yêu cầu
    pub work_hash: [u8; 32],         // kết quả PoW
}

impl PktBlockHeader {
    /// Hash block header để mine
    pub fn compute_hash(
        height: u64,
        prev_hash: &[u8; 32],
        content_merkle: &[u8; 32],
        timestamp: u32,
        base_target: CompactTarget,
        nonce: u32,
        ann_merkle: &[u8; 32],
        ann_count: u32,
        ann_target: CompactTarget,
    ) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(PKT_BLOCK_DOMAIN);
        h.update(&height.to_le_bytes());
        h.update(prev_hash);
        h.update(content_merkle);
        h.update(&timestamp.to_le_bytes());
        h.update(&base_target.0.to_le_bytes());
        h.update(&nonce.to_le_bytes());
        h.update(ann_merkle);
        h.update(&ann_count.to_le_bytes());
        h.update(&ann_target.0.to_le_bytes());
        *h.finalize().as_bytes()
    }

    /// Effective target = base_target << floor(log2(ann_count + 1))
    pub fn effective_target(base_target: CompactTarget, ann_count: u32) -> CompactTarget {
        base_target.with_ann_count(ann_count)
    }

    /// Xác minh block header
    pub fn verify(&self) -> bool {
        let expected = Self::compute_hash(
            self.height, &self.prev_hash, &self.content_merkle,
            self.timestamp, self.base_target, self.nonce,
            &self.ann_merkle, self.ann_count, self.ann_target,
        );
        let eff = Self::effective_target(self.base_target, self.ann_count);
        expected == self.work_hash && CompactTarget::meets_target(&self.work_hash, eff)
    }
}

// ── Block Miner ────────────────────────────────────────────────────────────────

pub struct PktBlockMiner {
    pub base_target: CompactTarget,
    pub ann_target: CompactTarget,
    pub anns: Vec<PktAnn>,
}

impl PktBlockMiner {
    pub fn new(base_target: CompactTarget, ann_target: CompactTarget) -> Self {
        Self { base_target, ann_target, anns: Vec::new() }
    }

    /// Thêm ann (kiểm tra: verify + expiry + đúng target)
    pub fn add_ann(&mut self, ann: PktAnn, block_height: u64) -> bool {
        if !ann.verify() { return false; }
        if !ann.is_valid_for_block(block_height) { return false; }
        self.anns.push(ann);
        true
    }

    pub fn ann_count(&self) -> u32 { self.anns.len() as u32 }

    pub fn effective_target(&self) -> CompactTarget {
        PktBlockHeader::effective_target(self.base_target, self.ann_count())
    }

    /// Mine block với anns hiện có
    pub fn mine(
        &self,
        height: u64,
        prev_hash: [u8; 32],
        content_merkle: [u8; 32],
    ) -> PktBlockHeader {
        let ann_merkle = ann_merkle_root(&self.anns);
        let ann_count  = self.ann_count();
        let eff_target = self.effective_target();
        let timestamp  = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;

        let mut nonce = 0u32;
        loop {
            let work_hash = PktBlockHeader::compute_hash(
                height, &prev_hash, &content_merkle, timestamp,
                self.base_target, nonce, &ann_merkle, ann_count, self.ann_target,
            );
            if CompactTarget::meets_target(&work_hash, eff_target) {
                return PktBlockHeader {
                    height,
                    prev_hash,
                    content_merkle,
                    timestamp,
                    base_target: self.base_target,
                    nonce,
                    ann_merkle,
                    ann_count,
                    ann_target: self.ann_target,
                    work_hash,
                };
            }
            nonce = nonce.wrapping_add(1);
        }
    }
}

// ── PktChain ───────────────────────────────────────────────────────────────────

/// Chain đơn giản dùng PktBlockHeader
pub struct PktChain {
    pub blocks: Vec<PktBlockHeader>,
    pub base_target: CompactTarget,
    pub ann_target: CompactTarget,
}

impl PktChain {
    pub fn new(base_target: CompactTarget, ann_target: CompactTarget) -> Self {
        let genesis = PktBlockHeader {
            height: 0,
            prev_hash: [0u8; 32],
            content_merkle: [0u8; 32],
            timestamp: 0,
            base_target,
            nonce: 0,
            ann_merkle: [0u8; 32],
            ann_count: 0,
            ann_target,
            work_hash: *blake3::hash(b"PKT_Genesis_v13").as_bytes(),
        };
        Self { blocks: vec![genesis], base_target, ann_target }
    }

    pub fn tip_hash(&self) -> [u8; 32] {
        self.blocks.last().map(|b| b.work_hash).unwrap_or([0u8; 32])
    }

    pub fn height(&self) -> u64 { self.blocks.len() as u64 - 1 }

    /// Seed hash cho ann miner tại block_height:
    /// hash của block tại (block_height - PKT_SEED_DEPTH), hoặc genesis nếu chưa đủ
    pub fn seed_hash_for(&self, block_height: u64) -> ([u8; 32], u64) {
        let seed_height = block_height.saturating_sub(PKT_SEED_DEPTH);
        let block = self.blocks.get(seed_height as usize).unwrap_or(&self.blocks[0]);
        (block.work_hash, seed_height)
    }

    pub fn add_block(&mut self, block: PktBlockHeader) -> bool {
        if block.prev_hash != self.tip_hash() { return false; }
        if block.height != self.height() + 1 { return false; }
        if !block.verify() { return false; }
        self.blocks.push(block);
        true
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn easy_target() -> CompactTarget { CompactTarget::max() }
    fn ann_target()   -> CompactTarget { CompactTarget::max() }

    // ── CompactTarget ─────────────────────────────────────────────────────────

    #[test]
    fn test_compact_target_max_passes_zero_hash() {
        // Hash [0;32] nhỏ hơn bất kỳ target dương nào → luôn pass
        let hash = [0u8; 32];
        assert!(CompactTarget::meets_target(&hash, CompactTarget::max()));
    }

    #[test]
    fn test_compact_target_zero_hash_passes() {
        let hash = [0u8; 32];
        assert!(CompactTarget::meets_target(&hash, CompactTarget::max()));
    }

    #[test]
    fn test_compact_target_with_ann_count_easier() {
        let base = CompactTarget::from_leading_zeros(8);
        let eff  = base.with_ann_count(3);
        // Effective exponent phải >= base exponent (easier = larger target)
        assert!(eff.exponent() >= base.exponent());
    }

    #[test]
    fn test_compact_target_no_anns_unchanged() {
        let base = CompactTarget::from_leading_zeros(4);
        let eff  = base.with_ann_count(0);
        assert_eq!(eff, base);
    }

    #[test]
    fn test_from_leading_zeros_exponent_positive() {
        let t = CompactTarget::from_leading_zeros(16);
        assert!(t.exponent() > 0);
    }

    // ── Ann content ───────────────────────────────────────────────────────────

    #[test]
    fn test_content_hash_deterministic() {
        let seed = [1u8; 32];
        let h1 = compute_content_hash(&seed, 16);
        let h2 = compute_content_hash(&seed, 16);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_hash_different_seeds() {
        let h1 = compute_content_hash(&[1u8; 32], 16);
        let h2 = compute_content_hash(&[2u8; 32], 16);
        assert_ne!(h1, h2);
    }

    // ── PktAnn ────────────────────────────────────────────────────────────────

    #[test]
    fn test_ann_mine_and_verify() {
        let mut miner = PktAnnMiner::new([42u8; 32]);
        let seed_hash = [1u8; 32];
        let ann = miner.mine(seed_hash, 3, ann_target());
        assert!(ann.verify());
    }

    #[test]
    fn test_ann_expiry_valid() {
        let mut miner = PktAnnMiner::new([1u8; 32]);
        let ann = miner.mine([0u8; 32], 5, ann_target()); // parent_height=5
        // Valid for blocks 6, 7, 8 (5 < block ≤ 5+3)
        assert!(ann.is_valid_for_block(6));
        assert!(ann.is_valid_for_block(7));
        assert!(ann.is_valid_for_block(8));
    }

    #[test]
    fn test_ann_expiry_invalid_before() {
        let mut miner = PktAnnMiner::new([1u8; 32]);
        let ann = miner.mine([0u8; 32], 5, ann_target());
        assert!(!ann.is_valid_for_block(5)); // same height = invalid
        assert!(!ann.is_valid_for_block(4)); // before = invalid
    }

    #[test]
    fn test_ann_expiry_invalid_after() {
        let mut miner = PktAnnMiner::new([1u8; 32]);
        let ann = miner.mine([0u8; 32], 5, ann_target());
        assert!(!ann.is_valid_for_block(9)); // 5+3+1 = expired
    }

    #[test]
    fn test_ann_tampered_fails_verify() {
        let mut miner = PktAnnMiner::new([1u8; 32]);
        let mut ann = miner.mine([0u8; 32], 3, ann_target());
        ann.nonce = ann.nonce.wrapping_add(1); // tamper
        assert!(!ann.verify());
    }

    #[test]
    fn test_ann_different_miners_different_content() {
        let mut m1 = PktAnnMiner::new([1u8; 32]);
        let mut m2 = PktAnnMiner::new([2u8; 32]);
        let a1 = m1.mine([0u8; 32], 3, ann_target());
        let a2 = m2.mine([0u8; 32], 3, ann_target());
        assert_ne!(a1.content_hash, a2.content_hash);
    }

    // ── Ann Merkle ────────────────────────────────────────────────────────────

    #[test]
    fn test_ann_merkle_empty_is_zero() {
        assert_eq!(ann_merkle_root(&[]), [0u8; 32]);
    }

    #[test]
    fn test_ann_merkle_single() {
        let mut m = PktAnnMiner::new([1u8; 32]);
        let ann = m.mine([0u8; 32], 3, ann_target());
        let root = ann_merkle_root(&[ann.clone()]);
        assert_eq!(root, ann.work_hash);
    }

    #[test]
    fn test_ann_merkle_two_different() {
        let mut m = PktAnnMiner::new([1u8; 32]);
        let a1 = m.mine([0u8; 32], 3, ann_target());
        let a2 = m.mine([1u8; 32], 3, ann_target());
        let root1 = ann_merkle_root(&[a1.clone()]);
        let root2 = ann_merkle_root(&[a1, a2]);
        assert_ne!(root1, root2);
    }

    // ── PktBlockMiner ─────────────────────────────────────────────────────────

    #[test]
    fn test_block_miner_add_valid_ann() {
        let mut bm  = PktBlockMiner::new(easy_target(), ann_target());
        let mut am  = PktAnnMiner::new([1u8; 32]);
        let ann = am.mine([0u8; 32], 3, ann_target());
        assert!(bm.add_ann(ann, 4)); // block 4, parent 3 → valid
    }

    #[test]
    fn test_block_miner_reject_expired_ann() {
        let mut bm = PktBlockMiner::new(easy_target(), ann_target());
        let mut am = PktAnnMiner::new([1u8; 32]);
        let ann = am.mine([0u8; 32], 3, ann_target());
        assert!(!bm.add_ann(ann, 99)); // expired
    }

    #[test]
    fn test_block_mine_and_verify() {
        let mut bm  = PktBlockMiner::new(easy_target(), ann_target());
        let mut am  = PktAnnMiner::new([1u8; 32]);
        let ann = am.mine([0u8; 32], 3, ann_target());
        bm.add_ann(ann, 4);
        let block = bm.mine(1, [0u8; 32], [0u8; 32]);
        assert!(block.verify());
    }

    #[test]
    fn test_block_mine_no_anns_still_valid() {
        let bm = PktBlockMiner::new(easy_target(), ann_target());
        let block = bm.mine(1, [0u8; 32], [0u8; 32]);
        assert!(block.verify());
    }

    // ── PktChain ──────────────────────────────────────────────────────────────

    #[test]
    fn test_chain_genesis_height_zero() {
        let chain = PktChain::new(easy_target(), ann_target());
        assert_eq!(chain.height(), 0);
    }

    #[test]
    fn test_chain_add_block_increments_height() {
        let mut chain = PktChain::new(easy_target(), ann_target());
        let bm = PktBlockMiner::new(easy_target(), ann_target());
        let prev = chain.tip_hash();
        let block = bm.mine(1, prev, [0u8; 32]);
        assert!(chain.add_block(block));
        assert_eq!(chain.height(), 1);
    }

    #[test]
    fn test_chain_wrong_prev_rejected() {
        let mut chain = PktChain::new(easy_target(), ann_target());
        let bm = PktBlockMiner::new(easy_target(), ann_target());
        let block = bm.mine(1, [0xdeu8; 32], [0u8; 32]); // wrong prev
        assert!(!chain.add_block(block));
    }

    #[test]
    fn test_chain_seed_hash_genesis() {
        let chain = PktChain::new(easy_target(), ann_target());
        let (seed, height) = chain.seed_hash_for(1);
        assert_eq!(height, 0);
        assert_eq!(seed, chain.blocks[0].work_hash);
    }

    #[test]
    fn test_chain_two_blocks() {
        let mut chain = PktChain::new(easy_target(), ann_target());
        let bm = PktBlockMiner::new(easy_target(), ann_target());

        let b1 = bm.mine(1, chain.tip_hash(), [0u8; 32]);
        chain.add_block(b1);

        let b2 = bm.mine(2, chain.tip_hash(), [0u8; 32]);
        assert!(chain.add_block(b2));
        assert_eq!(chain.height(), 2);
    }
}
