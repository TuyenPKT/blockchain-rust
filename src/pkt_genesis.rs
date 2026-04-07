#![allow(dead_code)]
//! v13.4 — PKT Testnet Genesis Params
//!
//! Định nghĩa các tham số mạng PKT (OCEIF):
//!   - Đơn vị tiền: paklet (1 PKT = 2^30 paklets)
//!   - Block reward: 4096 PKT/block, giảm 50% mỗi 1,048,576 blocks (~2 năm)
//!   - Block time: 60 giây
//!   - Treasury nhận 20% block reward (pkt_steward.rs)
//!   - Mainnet, testnet, regtest đều có magic bytes và port riêng

// ── Coin params ─────────────────────────────────────────────────────────────

/// 1 PKT = 2^30 paklets (đơn vị cơ bản không chia được)
pub const PAKLETS_PER_PKT: u64 = 1_073_741_824; // 2^30

/// Block reward ban đầu: 4096 PKT/block
pub const INITIAL_BLOCK_REWARD_PKT: u64 = 4_096;
pub const INITIAL_BLOCK_REWARD: u64 = INITIAL_BLOCK_REWARD_PKT * PAKLETS_PER_PKT;

/// Halving mỗi 1,048,576 blocks (~2 năm ở 1 block/phút)
pub const HALVING_INTERVAL: u64 = 1_048_576;

/// Target block time: 60 giây
pub const TARGET_BLOCK_TIME_SECS: u64 = 60;

/// Tổng cung tối đa: 6,000,000,000 PKT
pub const MAX_SUPPLY_PKT: u64 = 6_000_000_000;
pub const MAX_SUPPLY_PAKLETS: u64 = MAX_SUPPLY_PKT * PAKLETS_PER_PKT;

/// Coinbase maturity: 100 blocks trước khi tiêu được
pub const COINBASE_MATURITY: u64 = 100;

/// Số halvings tối đa (sau đó reward = 0)
pub const MAX_HALVINGS: u32 = 63;

// ── Network magic bytes ─────────────────────────────────────────────────────

pub const MAINNET_MAGIC:  [u8; 4] = [0xd9, 0xb4, 0xbe, 0xf9];
pub const TESTNET_MAGIC:  [u8; 4] = [0x0b, 0x11, 0x09, 0x07];
pub const REGTEST_MAGIC:  [u8; 4] = [0xda, 0xb5, 0xbf, 0xfa];

// ── Network ports ───────────────────────────────────────────────────────────

pub const MAINNET_P2P_PORT:  u16 = 64764;
pub const TESTNET_P2P_PORT:  u16 = 64765;
pub const REGTEST_P2P_PORT:  u16 = 18444;

pub const MAINNET_RPC_PORT:  u16 = 64766;
pub const TESTNET_RPC_PORT:  u16 = 64767;
pub const REGTEST_RPC_PORT:  u16 = 18443;

// ── Bootstrap peers ─────────────────────────────────────────────────────────

pub const MAINNET_BOOTSTRAP_PEERS: &[&str] = &[
    "seed.oceif.com:64764",
];

pub const TESTNET_BOOTSTRAP_PEERS: &[&str] = &[
    "seed.testnet.oceif.com:8333",
];

pub const REGTEST_BOOTSTRAP_PEERS: &[&str] = &[];   // regtest: local only

// ── Genesis block ───────────────────────────────────────────────────────────

/// Genesis block hash PKT mainnet (placeholder — cần mine genesis block thật)
pub const MAINNET_GENESIS_HASH: &str =
    "000000000000000000000000000000000000000000000000000000000000001a";

/// Genesis block hash PKT testnet (placeholder)
pub const TESTNET_GENESIS_HASH: &str =
    "000000000000000000000000000000000000000000000000000000000000002b";

/// Genesis block hash regtest (all-zero — mỗi lần chạy có thể khác nhau)
pub const REGTEST_GENESIS_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// UNIX timestamp genesis block mainnet PKT (2019-08-19T00:00:00Z)
pub const MAINNET_GENESIS_TIME: u64 = 1_566_172_800;

/// UNIX timestamp genesis block testnet
pub const TESTNET_GENESIS_TIME: u64 = 1_566_172_801;

/// UNIX timestamp regtest genesis (không cố định — dùng khi test)
pub const REGTEST_GENESIS_TIME: u64 = 1_296_688_602; // giống Bitcoin regtest

// ── Network params struct ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Network { Mainnet, Testnet, Regtest }

/// Tập hợp tất cả params của một mạng PKT
#[derive(Debug, Clone)]
pub struct PktNetworkParams {
    pub network:           Network,
    pub magic:             [u8; 4],
    pub p2p_port:          u16,
    pub rpc_port:          u16,
    pub hrp:               &'static str,   // bech32 HRP (từ pkt_address)
    pub genesis_hash:      &'static str,
    pub genesis_time:      u64,
    pub bootstrap_peers:   &'static [&'static str],
    pub initial_reward:    u64,            // paklets
    pub halving_interval:  u64,            // blocks
    pub target_block_time: u64,            // seconds
    pub coinbase_maturity: u64,            // blocks
}

impl PktNetworkParams {
    pub fn mainnet() -> Self {
        PktNetworkParams {
            network:           Network::Mainnet,
            magic:             MAINNET_MAGIC,
            p2p_port:          MAINNET_P2P_PORT,
            rpc_port:          MAINNET_RPC_PORT,
            hrp:               "pkt",
            genesis_hash:      MAINNET_GENESIS_HASH,
            genesis_time:      MAINNET_GENESIS_TIME,
            bootstrap_peers:   MAINNET_BOOTSTRAP_PEERS,
            initial_reward:    INITIAL_BLOCK_REWARD,
            halving_interval:  HALVING_INTERVAL,
            target_block_time: TARGET_BLOCK_TIME_SECS,
            coinbase_maturity: COINBASE_MATURITY,
        }
    }

    pub fn testnet() -> Self {
        PktNetworkParams {
            network:           Network::Testnet,
            magic:             TESTNET_MAGIC,
            p2p_port:          TESTNET_P2P_PORT,
            rpc_port:          TESTNET_RPC_PORT,
            hrp:               "tpkt",
            genesis_hash:      TESTNET_GENESIS_HASH,
            genesis_time:      TESTNET_GENESIS_TIME,
            bootstrap_peers:   TESTNET_BOOTSTRAP_PEERS,
            initial_reward:    INITIAL_BLOCK_REWARD,
            halving_interval:  HALVING_INTERVAL,
            target_block_time: TARGET_BLOCK_TIME_SECS,
            coinbase_maturity: COINBASE_MATURITY,
        }
    }

    pub fn regtest() -> Self {
        PktNetworkParams {
            network:           Network::Regtest,
            magic:             REGTEST_MAGIC,
            p2p_port:          REGTEST_P2P_PORT,
            rpc_port:          REGTEST_RPC_PORT,
            hrp:               "rpkt",
            genesis_hash:      REGTEST_GENESIS_HASH,
            genesis_time:      REGTEST_GENESIS_TIME,
            bootstrap_peers:   REGTEST_BOOTSTRAP_PEERS,
            // regtest halving nhanh hơn để test dễ
            initial_reward:    INITIAL_BLOCK_REWARD,
            halving_interval:  150,   // 150 blocks ≈ ~2.5 phút test cycle
            target_block_time: 1,     // 1 giây/block trong regtest
            coinbase_maturity: 3,     // maturity 3 blocks trong regtest
        }
    }

    pub fn is_mainnet(&self) -> bool { self.network == Network::Mainnet }
    pub fn is_testnet(&self) -> bool { self.network == Network::Testnet }
    pub fn is_regtest(&self) -> bool { self.network == Network::Regtest }
}

// ── Block reward calculation ────────────────────────────────────────────────

/// Tính block reward tại height cho một network
/// Mỗi `halving_interval` blocks, reward giảm 50%
pub fn block_reward_at(height: u64, params: &PktNetworkParams) -> u64 {
    if height == 0 { return 0; }  // genesis block không có reward
    let halvings = height / params.halving_interval;
    if halvings >= MAX_HALVINGS as u64 { return 0; }
    params.initial_reward >> halvings
}

/// Ước tính tổng supply phát hành đến một height (paklets)
/// Dùng công thức chuỗi hình học: Σ reward(h) cho h = 1..height
pub fn total_issued_to(height: u64, params: &PktNetworkParams) -> u64 {
    if height == 0 { return 0; }
    let mut total: u64 = 0;
    let mut reward = params.initial_reward;
    let mut remaining = height;

    loop {
        let era_blocks = remaining.min(params.halving_interval);
        // saturating_add để không panic khi gần MAX_SUPPLY
        total = total.saturating_add(era_blocks.saturating_mul(reward));
        remaining -= era_blocks;
        if remaining == 0 || reward == 0 { break; }
        reward >>= 1;
    }
    total
}

/// Halving number tại một height (0 = era đầu tiên)
pub fn halving_at(height: u64, params: &PktNetworkParams) -> u64 {
    height / params.halving_interval
}

/// Block height của halving tiếp theo sau `height`
pub fn next_halving_height(height: u64, params: &PktNetworkParams) -> u64 {
    let current_era = height / params.halving_interval;
    (current_era + 1) * params.halving_interval
}

// ── Genesis block builder ───────────────────────────────────────────────────

/// Minimal genesis block representation cho PKT
#[derive(Debug, Clone)]
pub struct PktGenesisBlock {
    pub height:     u64,
    pub hash:       String,
    pub timestamp:  u64,
    pub network:    Network,
    pub reward:     u64,   // 0 — genesis không mint coins
    pub prev_hash:  String,
    pub merkle_root: String,
}

impl PktGenesisBlock {
    pub fn build(params: &PktNetworkParams) -> Self {
        PktGenesisBlock {
            height:      0,
            hash:        params.genesis_hash.to_string(),
            timestamp:   params.genesis_time,
            network:     params.network.clone(),
            reward:      0,
            prev_hash:   "0000000000000000000000000000000000000000000000000000000000000000"
                             .to_string(),
            merkle_root: "0000000000000000000000000000000000000000000000000000000000000000"
                             .to_string(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.height != 0 {
            return Err(format!("genesis height must be 0, got {}", self.height));
        }
        if self.hash.len() != 64 {
            return Err(format!("genesis hash must be 64 hex chars, got {}", self.hash.len()));
        }
        if self.prev_hash != "00".repeat(32) {
            return Err("genesis prev_hash must be all-zero".to_string());
        }
        if self.reward != 0 {
            return Err(format!("genesis reward must be 0, got {}", self.reward));
        }
        Ok(())
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Coin constants ────────────────────────────────────────────────────

    #[test]
    fn test_paklets_per_pkt_is_2_pow_30() {
        assert_eq!(PAKLETS_PER_PKT, 1u64 << 30);
    }

    #[test]
    fn test_initial_block_reward_paklets() {
        assert_eq!(INITIAL_BLOCK_REWARD, 4_096 * (1u64 << 30));
    }

    #[test]
    fn test_halving_interval_constant() {
        assert_eq!(HALVING_INTERVAL, 1_048_576);
        assert_eq!(HALVING_INTERVAL, 1u64 << 20);
    }

    #[test]
    fn test_max_supply_pkt() {
        assert_eq!(MAX_SUPPLY_PKT, 6_000_000_000);
    }

    // ── Network params ────────────────────────────────────────────────────

    #[test]
    fn test_mainnet_params() {
        let p = PktNetworkParams::mainnet();
        assert!(p.is_mainnet());
        assert_eq!(p.hrp, "pkt");
        assert_eq!(p.p2p_port, 64764);
        assert_eq!(p.halving_interval, HALVING_INTERVAL);
    }

    #[test]
    fn test_testnet_params() {
        let p = PktNetworkParams::testnet();
        assert!(p.is_testnet());
        assert_eq!(p.hrp, "tpkt");
        assert_eq!(p.p2p_port, 64765);
    }

    #[test]
    fn test_regtest_params() {
        let p = PktNetworkParams::regtest();
        assert!(p.is_regtest());
        assert_eq!(p.hrp, "rpkt");
        assert_eq!(p.halving_interval, 150);
        assert_eq!(p.target_block_time, 1);
        assert_eq!(p.coinbase_maturity, 3);
    }

    #[test]
    fn test_magic_bytes_differ_across_networks() {
        let mn = PktNetworkParams::mainnet();
        let tn = PktNetworkParams::testnet();
        let rn = PktNetworkParams::regtest();
        assert_ne!(mn.magic, tn.magic);
        assert_ne!(mn.magic, rn.magic);
        assert_ne!(tn.magic, rn.magic);
    }

    #[test]
    fn test_rpc_port_differs_from_p2p_port() {
        let p = PktNetworkParams::mainnet();
        assert_ne!(p.p2p_port, p.rpc_port);
    }

    // ── block_reward_at ───────────────────────────────────────────────────

    #[test]
    fn test_genesis_block_reward_is_zero() {
        let p = PktNetworkParams::mainnet();
        assert_eq!(block_reward_at(0, &p), 0);
    }

    #[test]
    fn test_block_1_reward_is_initial() {
        let p = PktNetworkParams::mainnet();
        assert_eq!(block_reward_at(1, &p), INITIAL_BLOCK_REWARD);
    }

    #[test]
    fn test_first_halving_halves_reward() {
        let p = PktNetworkParams::mainnet();
        let before = block_reward_at(HALVING_INTERVAL - 1, &p);
        let after  = block_reward_at(HALVING_INTERVAL,     &p);
        assert_eq!(before, INITIAL_BLOCK_REWARD);
        assert_eq!(after,  INITIAL_BLOCK_REWARD / 2);
    }

    #[test]
    fn test_second_halving() {
        let p = PktNetworkParams::mainnet();
        let r = block_reward_at(HALVING_INTERVAL * 2, &p);
        assert_eq!(r, INITIAL_BLOCK_REWARD / 4);
    }

    #[test]
    fn test_reward_after_max_halvings_is_zero() {
        let p = PktNetworkParams::mainnet();
        let height = HALVING_INTERVAL * MAX_HALVINGS as u64;
        assert_eq!(block_reward_at(height, &p), 0);
    }

    #[test]
    fn test_regtest_reward_halves_at_150_blocks() {
        let p = PktNetworkParams::regtest();
        let before = block_reward_at(149, &p);
        let after  = block_reward_at(150, &p);
        assert_eq!(before, INITIAL_BLOCK_REWARD);
        assert_eq!(after,  INITIAL_BLOCK_REWARD / 2);
    }

    // ── total_issued_to ───────────────────────────────────────────────────

    #[test]
    fn test_total_issued_at_0_is_zero() {
        let p = PktNetworkParams::mainnet();
        assert_eq!(total_issued_to(0, &p), 0);
    }

    #[test]
    fn test_total_issued_first_era() {
        let p = PktNetworkParams::mainnet();
        // 1 era = halving_interval blocks × initial_reward
        let expected = HALVING_INTERVAL * INITIAL_BLOCK_REWARD;
        assert_eq!(total_issued_to(HALVING_INTERVAL, &p), expected);
    }

    #[test]
    fn test_total_issued_two_eras() {
        let p = PktNetworkParams::mainnet();
        let era1 = HALVING_INTERVAL * INITIAL_BLOCK_REWARD;
        let era2 = HALVING_INTERVAL * (INITIAL_BLOCK_REWARD / 2);
        assert_eq!(total_issued_to(HALVING_INTERVAL * 2, &p), era1 + era2);
    }

    #[test]
    fn test_total_issued_monotonically_increases() {
        let p = PktNetworkParams::regtest();
        let t1 = total_issued_to(100, &p);
        let t2 = total_issued_to(200, &p);
        assert!(t2 > t1);
    }

    // ── halving helpers ───────────────────────────────────────────────────

    #[test]
    fn test_halving_at_genesis_is_zero() {
        let p = PktNetworkParams::mainnet();
        assert_eq!(halving_at(0, &p), 0);
    }

    #[test]
    fn test_halving_at_interval_is_one() {
        let p = PktNetworkParams::mainnet();
        assert_eq!(halving_at(HALVING_INTERVAL, &p), 1);
    }

    #[test]
    fn test_next_halving_height_from_genesis() {
        let p = PktNetworkParams::mainnet();
        assert_eq!(next_halving_height(0, &p), HALVING_INTERVAL);
    }

    #[test]
    fn test_next_halving_height_mid_era() {
        let p = PktNetworkParams::mainnet();
        let h = HALVING_INTERVAL / 2;
        assert_eq!(next_halving_height(h, &p), HALVING_INTERVAL);
    }

    #[test]
    fn test_next_halving_height_after_first() {
        let p = PktNetworkParams::mainnet();
        assert_eq!(next_halving_height(HALVING_INTERVAL + 1, &p), HALVING_INTERVAL * 2);
    }

    // ── PktGenesisBlock ───────────────────────────────────────────────────

    #[test]
    fn test_genesis_testnet_validates() {
        let p = PktNetworkParams::testnet();
        let g = PktGenesisBlock::build(&p);
        assert!(g.validate().is_ok());
    }

    #[test]
    fn test_genesis_regtest_validates() {
        let p = PktNetworkParams::regtest();
        let g = PktGenesisBlock::build(&p);
        assert!(g.validate().is_ok());
    }

    #[test]
    fn test_genesis_height_is_zero() {
        let p = PktNetworkParams::testnet();
        let g = PktGenesisBlock::build(&p);
        assert_eq!(g.height, 0);
    }

    #[test]
    fn test_genesis_reward_is_zero() {
        let p = PktNetworkParams::testnet();
        let g = PktGenesisBlock::build(&p);
        assert_eq!(g.reward, 0);
    }

    #[test]
    fn test_genesis_prev_hash_all_zero() {
        let p = PktNetworkParams::testnet();
        let g = PktGenesisBlock::build(&p);
        assert_eq!(g.prev_hash, "00".repeat(32));
    }

    #[test]
    fn test_genesis_nonzero_height_fails_validation() {
        let p = PktNetworkParams::testnet();
        let mut g = PktGenesisBlock::build(&p);
        g.height = 1;
        assert!(g.validate().is_err());
    }

    #[test]
    fn test_genesis_nonzero_reward_fails_validation() {
        let p = PktNetworkParams::testnet();
        let mut g = PktGenesisBlock::build(&p);
        g.reward = 1;
        assert!(g.validate().is_err());
    }

    // ── Bootstrap peers ───────────────────────────────────────────────────

    #[test]
    fn test_mainnet_has_bootstrap_peers() {
        assert!(!MAINNET_BOOTSTRAP_PEERS.is_empty());
    }

    #[test]
    fn test_testnet_has_bootstrap_peers() {
        assert!(!TESTNET_BOOTSTRAP_PEERS.is_empty());
    }

    #[test]
    fn test_regtest_no_bootstrap_peers() {
        assert!(REGTEST_BOOTSTRAP_PEERS.is_empty());
    }

    #[test]
    fn test_bootstrap_peers_contain_port() {
        for peer in TESTNET_BOOTSTRAP_PEERS {
            assert!(peer.contains(':'), "peer '{}' missing port", peer);
        }
    }
}
