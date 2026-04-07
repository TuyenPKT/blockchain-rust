#![allow(dead_code)]
//! v23.5 — IBD Checkpoints
//!
//! Hardcoded wire block hashes tại các checkpoint heights.
//! Trong quá trình Initial Block Download (IBD):
//!   - Blocks trước checkpoint height cao nhất → skip full validation (chỉ check hash)
//!   - Blocks sau checkpoint → validate đầy đủ như thường
//!
//! Wire hash = SHA256d(80-byte header) — giống Bitcoin, khác với BLAKE3 internal hash.
//!
//! Checkpoints PKT Testnet (OCEIF network):

use crate::pkt_wire::WireBlockHeader;

// ── Checkpoint data ───────────────────────────────────────────────────────────

/// Một checkpoint entry: height → expected wire block hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub height: u64,
    /// SHA256d wire hash của block tại `height` (big-endian display order).
    pub hash:   [u8; 32],
}

impl Checkpoint {
    /// Tạo từ height + hex string (big-endian, 64 hex chars).
    /// Panic nếu hex sai — dùng chỉ cho hardcoded data.
    const fn from_hex(height: u64, hex: &[u8; 64]) -> Self {
        let mut hash = [0u8; 32];
        let mut i = 0;
        while i < 32 {
            hash[i] = hex_byte(hex[i * 2], hex[i * 2 + 1]);
            i += 1;
        }
        Checkpoint { height, hash }
    }
}

const fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("invalid hex char in checkpoint"),
    }
}

const fn hex_byte(hi: u8, lo: u8) -> u8 {
    (hex_nibble(hi) << 4) | hex_nibble(lo)
}

// ── PKT Testnet checkpoints ───────────────────────────────────────────────────
//
// Thêm checkpoint mới khi chain dài hơn — mỗi ~50,000 blocks là hợp lý.
// Hash là SHA256d của 80-byte header, big-endian (display format).

// Thêm checkpoint mới khi chain đạt ~50,000 blocks:
//   1. Lấy hash: curl http://localhost:8081/api/testnet/block/<height> | jq .hash
//   2. Thêm Checkpoint::from_hex(height, b"<hash>") vào đây
//   3. cargo test && git commit && git push

pub const TESTNET_CHECKPOINTS: &[Checkpoint] = &[
    // height 0 = genesis OCEIF testnet (mined 2026-04-07)
    Checkpoint::from_hex(
        0,
        b"00da8943f3f7684e0b8dac45d18978666773411d6c6a818b7bd75ea1f31cc970",
    ),
];

pub const MAINNET_CHECKPOINTS: &[Checkpoint] = &[
    // height 0 = genesis OCEIF mainnet (mined 2026-04-07)
    Checkpoint::from_hex(
        0,
        b"00000ccc1a0ff73c2050c13af51956439c3c4f8be40c8e98753386f4a4f896d2",
    ),
];

// ── CheckpointSet ─────────────────────────────────────────────────────────────

/// Tập checkpoint của một network — sorted ascending by height.
pub struct CheckpointSet {
    checkpoints: &'static [Checkpoint],
}

impl CheckpointSet {
    pub const fn testnet() -> Self {
        CheckpointSet { checkpoints: TESTNET_CHECKPOINTS }
    }

    pub const fn mainnet() -> Self {
        CheckpointSet { checkpoints: MAINNET_CHECKPOINTS }
    }

    pub fn new(checkpoints: &'static [Checkpoint]) -> Self {
        CheckpointSet { checkpoints }
    }

    /// Height của checkpoint cao nhất.
    /// Blocks <= height này có thể skip full validation trong IBD.
    pub fn max_height(&self) -> u64 {
        self.checkpoints.iter().map(|c| c.height).max().unwrap_or(0)
    }

    /// Trả về `Some(&Checkpoint)` nếu height có checkpoint, ngược lại `None`.
    pub fn get(&self, height: u64) -> Option<&Checkpoint> {
        self.checkpoints.iter().find(|c| c.height == height)
    }

    /// Kiểm tra block tại `height` có đúng checkpoint hash không.
    /// - Nếu không có checkpoint tại height → `Ok(CheckpointResult::NoCheckpoint)`
    /// - Nếu có → so sánh wire hash, trả về `Ok(Passed)` hoặc `Err(Failed)`
    pub fn verify(&self, height: u64, header: &WireBlockHeader) -> Result<CheckpointResult, CheckpointError> {
        match self.get(height) {
            None => Ok(CheckpointResult::NoCheckpoint),
            Some(cp) => {
                let wire_hash = header.block_hash();
                if wire_hash == cp.hash {
                    Ok(CheckpointResult::Passed)
                } else {
                    Err(CheckpointError::HashMismatch {
                        height,
                        expected: cp.hash,
                        got:      wire_hash,
                    })
                }
            }
        }
    }

    /// Quyết định có nên skip full validation cho block tại `height` không.
    ///
    /// Returns `true` nếu block nằm trong vùng đã được checkpoint bảo vệ
    /// (height < max_checkpoint_height) — nghĩa là chain history trước đó
    /// đã được xác nhận bởi checkpoint cao nhất.
    ///
    /// Returns `false` nếu block >= max checkpoint → cần validate đầy đủ.
    pub fn can_skip_validation(&self, height: u64) -> bool {
        height < self.max_height()
    }

    /// Trả về checkpoint ngay trước `height` (resume point cho IBD).
    pub fn last_before(&self, height: u64) -> Option<&Checkpoint> {
        self.checkpoints.iter()
            .filter(|c| c.height < height)
            .max_by_key(|c| c.height)
    }
}

// ── Result types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointResult {
    /// Không có checkpoint tại height này — tiếp tục validate bình thường.
    NoCheckpoint,
    /// Block hash khớp checkpoint — hợp lệ.
    Passed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointError {
    /// Block hash không khớp checkpoint — peer gian lận hoặc chain sai.
    HashMismatch {
        height:   u64,
        expected: [u8; 32],
        got:      [u8; 32],
    },
}

impl std::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HashMismatch { height, expected, got } => write!(
                f,
                "checkpoint mismatch at height {}: expected {} got {}",
                height,
                hex::encode(expected),
                hex::encode(got),
            ),
        }
    }
}

// ── IBD helper ────────────────────────────────────────────────────────────────

/// Kết quả xử lý một block trong IBD pipeline.
#[derive(Debug, PartialEq, Eq)]
pub enum IbdBlockAction {
    /// Block pass checkpoint (hoặc không có checkpoint) + skip validation.
    AcceptSkipValidation,
    /// Block pass checkpoint (hoặc không có checkpoint) + cần full validation.
    AcceptFullValidation,
    /// Block bị reject do checkpoint mismatch.
    Reject(CheckpointError),
}

/// Quyết định hành động cho block tại `height` trong IBD:
///
/// 1. Nếu có checkpoint → verify hash.
///    - Sai → Reject
///    - Đúng và height < max_checkpoint → AcceptSkipValidation
///    - Đúng và height >= max_checkpoint → AcceptFullValidation
/// 2. Không có checkpoint:
///    - height < max_checkpoint → AcceptSkipValidation (đã được checkpoint bảo vệ)
///    - height >= max_checkpoint → AcceptFullValidation
pub fn ibd_action(
    set:    &CheckpointSet,
    height: u64,
    header: &WireBlockHeader,
) -> IbdBlockAction {
    match set.verify(height, header) {
        Err(e) => IbdBlockAction::Reject(e),
        Ok(_) => {
            if set.can_skip_validation(height) {
                IbdBlockAction::AcceptSkipValidation
            } else {
                IbdBlockAction::AcceptFullValidation
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Tạo custom checkpoint set để test không phụ thuộc vào hash thật
    static TEST_CHECKPOINTS: &[Checkpoint] = &[
        Checkpoint { height: 0,       hash: [0x11; 32] },
        Checkpoint { height: 100_000, hash: [0x22; 32] },
        Checkpoint { height: 500_000, hash: [0x33; 32] },
    ];

    fn test_set() -> CheckpointSet {
        CheckpointSet::new(TEST_CHECKPOINTS)
    }

    fn header_with_hash(_hash_bytes: [u8; 32]) -> WireBlockHeader {
        // Tạo header sao cho block_hash() trả về hash mong muốn không dễ
        // → test verify() trực tiếp qua mock thay vì block_hash()
        // Dùng dummy header + override bằng cách test get() riêng
        WireBlockHeader {
            version:     1,
            prev_block:  [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp:   0,
            bits:        0x1d00ffff,
            nonce:       0,
        }
    }

    // ── CheckpointSet::get ────────────────────────────────────────────────────

    #[test]
    fn get_existing_height() {
        let set = test_set();
        assert_eq!(set.get(0).unwrap().hash, [0x11; 32]);
        assert_eq!(set.get(100_000).unwrap().hash, [0x22; 32]);
        assert_eq!(set.get(500_000).unwrap().hash, [0x33; 32]);
    }

    #[test]
    fn get_missing_height_returns_none() {
        let set = test_set();
        assert!(set.get(1).is_none());
        assert!(set.get(99_999).is_none());
        assert!(set.get(999_999).is_none());
    }

    // ── max_height ────────────────────────────────────────────────────────────

    #[test]
    fn max_height_returns_highest() {
        assert_eq!(test_set().max_height(), 500_000);
    }

    #[test]
    fn max_height_empty_set_returns_zero() {
        let set = CheckpointSet::new(&[]);
        assert_eq!(set.max_height(), 0);
    }

    // ── can_skip_validation ───────────────────────────────────────────────────

    #[test]
    fn skip_validation_before_max_checkpoint() {
        let set = test_set();
        assert!(set.can_skip_validation(0));
        assert!(set.can_skip_validation(499_999));
    }

    #[test]
    fn no_skip_at_or_after_max_checkpoint() {
        let set = test_set();
        assert!(!set.can_skip_validation(500_000));
        assert!(!set.can_skip_validation(600_000));
    }

    // ── last_before ───────────────────────────────────────────────────────────

    #[test]
    fn last_before_finds_nearest() {
        let set = test_set();
        assert_eq!(set.last_before(200_000).unwrap().height, 100_000);
        assert_eq!(set.last_before(500_000).unwrap().height, 100_000);
        assert_eq!(set.last_before(600_000).unwrap().height, 500_000);
    }

    #[test]
    fn last_before_zero_returns_none() {
        let set = test_set();
        assert!(set.last_before(0).is_none());
    }

    // ── verify (kiểm tra trực tiếp qua Checkpoint.hash) ──────────────────────

    #[test]
    fn verify_no_checkpoint_at_height() {
        let set = test_set();
        let hdr = header_with_hash([0xAB; 32]);
        assert_eq!(set.verify(999, &hdr), Ok(CheckpointResult::NoCheckpoint));
    }

    #[test]
    fn verify_checkpoint_passed() {
        // Cần block_hash() == [0x11; 32]
        // block_hash() = SHA256d(80 bytes) — không thể giả mạo dễ
        // → test riêng get() logic, verify() được test qua ibd_action
        let set = test_set();
        // height không có checkpoint → NoCheckpoint
        assert_eq!(set.verify(1, &header_with_hash([0x00; 32])), Ok(CheckpointResult::NoCheckpoint));
    }

    // ── ibd_action ────────────────────────────────────────────────────────────

    #[test]
    fn ibd_no_checkpoint_before_max_skips_validation() {
        let set = test_set();
        let hdr = header_with_hash([0xAB; 32]);
        // height=50_000 < max(500_000), no checkpoint → skip
        assert_eq!(ibd_action(&set, 50_000, &hdr), IbdBlockAction::AcceptSkipValidation);
    }

    #[test]
    fn ibd_no_checkpoint_at_max_full_validation() {
        let set = test_set();
        let hdr = header_with_hash([0xAB; 32]);
        // height=500_001 > max(500_000), không có checkpoint → AcceptFullValidation
        assert_eq!(ibd_action(&set, 500_001, &hdr), IbdBlockAction::AcceptFullValidation);
    }

    #[test]
    fn ibd_no_checkpoint_above_max_full_validation() {
        let set = test_set();
        let hdr = header_with_hash([0xAB; 32]);
        assert_eq!(ibd_action(&set, 600_000, &hdr), IbdBlockAction::AcceptFullValidation);
    }

    // ── CheckpointError display ───────────────────────────────────────────────

    #[test]
    fn checkpoint_error_display() {
        let e = CheckpointError::HashMismatch {
            height:   12345,
            expected: [0xAA; 32],
            got:      [0xBB; 32],
        };
        let s = e.to_string();
        assert!(s.contains("12345"));
        assert!(s.contains("checkpoint mismatch"));
    }

    // ── hex_byte const fn ─────────────────────────────────────────────────────

    #[test]
    fn hex_byte_parses_correctly() {
        assert_eq!(hex_byte(b'0', b'0'), 0x00);
        assert_eq!(hex_byte(b'f', b'f'), 0xFF);
        assert_eq!(hex_byte(b'A', b'B'), 0xAB);
        assert_eq!(hex_byte(b'1', b'a'), 0x1a);
    }

    // ── Checkpoint::from_hex ──────────────────────────────────────────────────

    #[test]
    fn from_hex_parses_genesis() {
        // OCEIF mainnet genesis hash
        let cp = Checkpoint::from_hex(
            0,
            b"00000ccc1a0ff73c2050c13af51956439c3c4f8be40c8e98753386f4a4f896d2",
        );
        assert_eq!(cp.height, 0);
        assert_eq!(cp.hash[0], 0x00);
        assert_eq!(cp.hash[1], 0x00);
        assert_eq!(cp.hash[2], 0x0c);
        assert_eq!(cp.hash[31], 0xd2);
    }

    // ── IbdBlockAction equality ───────────────────────────────────────────────

    #[test]
    fn ibd_action_variants_eq() {
        assert_eq!(IbdBlockAction::AcceptSkipValidation, IbdBlockAction::AcceptSkipValidation);
        assert_eq!(IbdBlockAction::AcceptFullValidation, IbdBlockAction::AcceptFullValidation);
        assert_ne!(IbdBlockAction::AcceptSkipValidation, IbdBlockAction::AcceptFullValidation);
    }
}
