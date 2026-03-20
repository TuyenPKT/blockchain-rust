#![allow(dead_code)]
//! v16.0 — Devnet One-Command [DX]
//!
//! `cargo run -- devnet [--port P] [--blocks N] [--difficulty D] [--interval MS]`
//!
//! Khởi động node + miner + API trong một process duy nhất.
//! Chain luôn bắt đầu sạch (không load từ disk).
//! Mine block thật → API trả dữ liệu thật → tests có giá trị.

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;

use crate::chain::Blockchain;
use crate::pktscan_api::ScanDb;
use crate::script::Script;
use crate::wallet::Wallet;

// ── Config ────────────────────────────────────────────────────────────────────

pub struct DevnetConfig {
    pub api_port:        u16,
    pub blocks:          Option<u32>,   // None = chạy mãi
    pub difficulty:      usize,
    pub mine_interval_ms: u64,          // nghỉ giữa các block (ms)
}

impl Default for DevnetConfig {
    fn default() -> Self {
        DevnetConfig {
            api_port:         8080,
            blocks:           None,
            difficulty:       2,
            mine_interval_ms: 0,
        }
    }
}

/// Parse CLI args: `devnet [--port P] [--blocks N] [--difficulty D] [--interval MS]`
pub fn parse_devnet_args(args: &[String]) -> DevnetConfig {
    let mut cfg = DevnetConfig::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                if let Some(v) = args.get(i + 1) {
                    if let Ok(p) = v.parse() { cfg.api_port = p; }
                    i += 1;
                }
            }
            "--blocks" | "-n" => {
                if let Some(v) = args.get(i + 1) {
                    if let Ok(n) = v.parse::<u32>() { cfg.blocks = Some(n); }
                    i += 1;
                }
            }
            "--difficulty" | "-d" => {
                if let Some(v) = args.get(i + 1) {
                    if let Ok(d) = v.parse::<usize>() {
                        cfg.difficulty = d.max(1);
                    }
                    i += 1;
                }
            }
            "--interval" => {
                if let Some(v) = args.get(i + 1) {
                    if let Ok(ms) = v.parse() { cfg.mine_interval_ms = ms; }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    cfg
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct DevnetState {
    pub blocks_mined:    u32,
    pub height:          u64,
    pub balance_paklets: u64,
    pub miner_address:   String,
    pub miner_hash:      String,
    start:               Instant,
}

impl DevnetState {
    pub fn new(miner_address: String, miner_hash: String) -> Self {
        DevnetState {
            blocks_mined:    0,
            height:          0,
            balance_paklets: 0,
            miner_address,
            miner_hash,
            start: Instant::now(),
        }
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.start.elapsed().as_secs()
    }

    /// Balance tính theo PKT (1 PKT = 2^30 paklets).
    pub fn balance_pkt(&self) -> f64 {
        self.balance_paklets as f64 / 1_073_741_824.0
    }

    /// Tốc độ mine trung bình.
    pub fn blocks_per_sec(&self) -> f64 {
        let t = self.start.elapsed().as_secs_f64();
        if t < 0.001 { 0.0 } else { self.blocks_mined as f64 / t }
    }
}

// ── Format ────────────────────────────────────────────────────────────────────

pub fn format_devnet_status(s: &DevnetState) -> String {
    format!(
        "  #{:<5} | height={:<6} | bal={:.4} PKT | {:.2} blk/s | {}s",
        s.blocks_mined, s.height, s.balance_pkt(), s.blocks_per_sec(), s.elapsed_secs(),
    )
}

pub fn format_devnet_summary(s: &DevnetState) -> String {
    format!(
        "blocks={} height={} balance={:.4} PKT elapsed={}s",
        s.blocks_mined, s.height, s.balance_pkt(), s.elapsed_secs(),
    )
}

// ── Devnet builder ────────────────────────────────────────────────────────────

/// Tạo fresh ScanDb (chain sạch, không load từ disk).
pub fn fresh_devnet_db(difficulty: usize) -> ScanDb {
    let mut chain = Blockchain::new();
    chain.difficulty = difficulty;
    Arc::new(Mutex::new(chain))
}

/// Sinh miner wallet; trả về (address, pubkey_hash_hex).
pub fn new_miner_wallet() -> (String, String) {
    let w = Wallet::new();
    let hash = hex::encode(Script::pubkey_hash(&w.public_key.serialize()));
    (w.address, hash)
}

// ── Runner ────────────────────────────────────────────────────────────────────

pub async fn run_devnet_async(config: DevnetConfig) {
    let db = fresh_devnet_db(config.difficulty);
    let (miner_addr, miner_hash) = new_miner_wallet();

    println!();
    println!("  ╔═══════════════════════════════════════════╗");
    println!("  ║          PKT Devnet  v16.0                ║");
    println!("  ╚═══════════════════════════════════════════╝");
    println!();
    println!("  Miner   : {}", miner_addr);
    println!("  API     : http://localhost:{}", config.api_port);
    println!("  Blocks  : {}", config.blocks.map(|n| n.to_string()).unwrap_or_else(|| "∞".to_string()));
    println!("  Diff    : {}", config.difficulty);
    println!();

    // Spawn PKTScan API server in background
    let db_api = Arc::clone(&db);
    let port   = config.api_port;
    tokio::spawn(crate::pktscan_api::serve(db_api, port));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    println!("  PKTScan → http://localhost:{}", port);
    println!("  Press Ctrl+C to stop.\n");

    let mut state      = DevnetState::new(miner_addr, miner_hash.clone());
    let max_blocks     = config.blocks;
    let interval_ms    = config.mine_interval_ms;

    loop {
        if let Some(max) = max_blocks {
            if state.blocks_mined >= max { break; }
        }

        // Mine block (CPU-bound) — block_in_place giữ tokio executor hoạt động
        let db_mine = Arc::clone(&db);
        let h       = miner_hash.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut chain = db_mine.lock().await;
                chain.mine_block_to_hash(&h);
            });
        });

        // Cập nhật state từ chain thật
        {
            let chain = db.lock().await;
            state.height          = chain.last_block().index;
            state.balance_paklets = chain.utxo_set.balance_of(&miner_hash);
            state.blocks_mined   += 1;
        }

        println!("{}", format_devnet_status(&state));

        if interval_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
        }
    }

    println!();
    println!("  ── Devnet complete ─────────────────────────");
    println!("  {}", format_devnet_summary(&state));
    println!();
}

/// Synchronous entry point (tạo tokio runtime, chạy devnet).
pub fn run_devnet(config: DevnetConfig) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("devnet: tokio runtime");
    rt.block_on(run_devnet_async(config));
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// Tất cả tests dùng data thật: mine block thật, đọc kết quả từ chain thật.
// Không dùng mock_data hay hardcode kết quả blockchain.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Blockchain;
    use crate::wallet::Wallet;
    use crate::script::Script;

    // ── parse_devnet_args ─────────────────────────────────────────────────────

    #[test]
    fn parse_default_no_args() {
        let cfg = parse_devnet_args(&[]);
        assert_eq!(cfg.api_port,   8080);
        assert_eq!(cfg.blocks,     None);
        assert_eq!(cfg.difficulty, 2);
        assert_eq!(cfg.mine_interval_ms, 0);
    }

    #[test]
    fn parse_port_long() {
        let args = vec!["--port".to_string(), "9090".to_string()];
        let cfg  = parse_devnet_args(&args);
        assert_eq!(cfg.api_port, 9090);
    }

    #[test]
    fn parse_port_short() {
        let args = vec!["-p".to_string(), "3000".to_string()];
        let cfg  = parse_devnet_args(&args);
        assert_eq!(cfg.api_port, 3000);
    }

    #[test]
    fn parse_blocks_long() {
        let args = vec!["--blocks".to_string(), "10".to_string()];
        let cfg  = parse_devnet_args(&args);
        assert_eq!(cfg.blocks, Some(10));
    }

    #[test]
    fn parse_blocks_short() {
        let args = vec!["-n".to_string(), "5".to_string()];
        let cfg  = parse_devnet_args(&args);
        assert_eq!(cfg.blocks, Some(5));
    }

    #[test]
    fn parse_difficulty_long() {
        let args = vec!["--difficulty".to_string(), "3".to_string()];
        let cfg  = parse_devnet_args(&args);
        assert_eq!(cfg.difficulty, 3);
    }

    #[test]
    fn parse_difficulty_short() {
        let args = vec!["-d".to_string(), "1".to_string()];
        let cfg  = parse_devnet_args(&args);
        assert_eq!(cfg.difficulty, 1);
    }

    #[test]
    fn parse_difficulty_zero_clamped_to_one() {
        let args = vec!["--difficulty".to_string(), "0".to_string()];
        let cfg  = parse_devnet_args(&args);
        assert_eq!(cfg.difficulty, 1);   // min 1
    }

    #[test]
    fn parse_interval() {
        let args = vec!["--interval".to_string(), "500".to_string()];
        let cfg  = parse_devnet_args(&args);
        assert_eq!(cfg.mine_interval_ms, 500);
    }

    #[test]
    fn parse_all_args() {
        let args: Vec<String> = "--port 7777 --blocks 20 --difficulty 1 --interval 100"
            .split_whitespace().map(|s| s.to_string()).collect();
        let cfg = parse_devnet_args(&args);
        assert_eq!(cfg.api_port,         7777);
        assert_eq!(cfg.blocks,           Some(20));
        assert_eq!(cfg.difficulty,       1);
        assert_eq!(cfg.mine_interval_ms, 100);
    }

    #[test]
    fn parse_ignores_unknown_flags() {
        let args = vec!["--unknown".to_string(), "value".to_string()];
        let cfg  = parse_devnet_args(&args);
        // defaults unchanged
        assert_eq!(cfg.api_port, 8080);
    }

    #[test]
    fn parse_invalid_port_ignored() {
        let args = vec!["--port".to_string(), "notanumber".to_string()];
        let cfg  = parse_devnet_args(&args);
        assert_eq!(cfg.api_port, 8080);  // default unchanged
    }

    // ── DevnetState ───────────────────────────────────────────────────────────

    #[test]
    fn state_initial_values() {
        let s = DevnetState::new("addr".to_string(), "hash".to_string());
        assert_eq!(s.blocks_mined,    0);
        assert_eq!(s.height,          0);
        assert_eq!(s.balance_paklets, 0);
        assert_eq!(s.balance_pkt(),   0.0);
    }

    #[test]
    fn state_balance_pkt_conversion() {
        let mut s = DevnetState::new("a".to_string(), "h".to_string());
        s.balance_paklets = 1_073_741_824;   // 1 PKT exactly
        assert!((s.balance_pkt() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn state_balance_pkt_half() {
        let mut s = DevnetState::new("a".to_string(), "h".to_string());
        s.balance_paklets = 536_870_912;  // 0.5 PKT
        assert!((s.balance_pkt() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn state_elapsed_secs_non_negative() {
        let s = DevnetState::new("a".to_string(), "h".to_string());
        // elapsed_secs() is u64, always >= 0
        let _ = s.elapsed_secs();
    }

    #[test]
    fn state_blocks_per_sec_zero_when_no_blocks() {
        let s = DevnetState::new("a".to_string(), "h".to_string());
        // 0 blocks mined → 0.0 (numerator is 0)
        assert_eq!(s.blocks_per_sec(), 0.0);
    }

    // ── format_devnet_status ──────────────────────────────────────────────────

    #[test]
    fn format_status_contains_height() {
        let mut s = DevnetState::new("addr".to_string(), "hash".to_string());
        s.height = 42;
        s.blocks_mined = 1;
        let out = format_devnet_status(&s);
        assert!(out.contains("42"), "status should contain height 42, got: {}", out);
    }

    #[test]
    fn format_status_contains_block_count() {
        let mut s = DevnetState::new("a".to_string(), "h".to_string());
        s.blocks_mined = 7;
        let out = format_devnet_status(&s);
        assert!(out.contains('7'));
    }

    #[test]
    fn format_status_contains_pkt() {
        let s = DevnetState::new("a".to_string(), "h".to_string());
        let out = format_devnet_status(&s);
        assert!(out.contains("PKT"));
    }

    #[test]
    fn format_summary_contains_blocks() {
        let mut s = DevnetState::new("a".to_string(), "h".to_string());
        s.blocks_mined = 3;
        let out = format_devnet_summary(&s);
        assert!(out.contains("blocks=3"));
    }

    // ── new_miner_wallet ──────────────────────────────────────────────────────

    #[test]
    fn miner_wallet_address_non_empty() {
        let (addr, hash) = new_miner_wallet();
        assert!(!addr.is_empty());
        assert!(!hash.is_empty());
    }

    #[test]
    fn miner_wallet_hash_is_hex() {
        let (_addr, hash) = new_miner_wallet();
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "hash should be hex");
    }

    #[test]
    fn miner_wallet_hash_length() {
        let (_addr, hash) = new_miner_wallet();
        // RIPEMD160 output = 20 bytes = 40 hex chars
        assert_eq!(hash.len(), 40);
    }

    #[test]
    fn two_wallets_have_different_hashes() {
        let (_, h1) = new_miner_wallet();
        let (_, h2) = new_miner_wallet();
        assert_ne!(h1, h2);
    }

    // ── Real mining tests (data thật) ────────────────────────────────────────
    //
    // Dùng difficulty=1 để mine nhanh trong tests.
    // Không mock — chain.mine_block_to_hash() thật sự chạy PoW.

    fn make_test_chain_and_miner() -> (Blockchain, String) {
        let wallet     = Wallet::new();
        let miner_hash = hex::encode(Script::pubkey_hash(&wallet.public_key.serialize()));
        let mut chain  = Blockchain::new();
        chain.difficulty = 1;
        (chain, miner_hash)
    }

    #[test]
    fn mine_1_block_height_is_1() {
        let (mut chain, hash) = make_test_chain_and_miner();
        chain.mine_block_to_hash(&hash);
        assert_eq!(chain.last_block().index, 1);
    }

    #[test]
    fn mine_1_block_balance_positive() {
        let (mut chain, hash) = make_test_chain_and_miner();
        chain.mine_block_to_hash(&hash);
        let balance = chain.utxo_set.balance_of(&hash);
        assert!(balance > 0, "miner should have positive balance after mining");
    }

    #[test]
    fn mine_2_blocks_height_is_2() {
        let (mut chain, hash) = make_test_chain_and_miner();
        chain.mine_block_to_hash(&hash);
        chain.mine_block_to_hash(&hash);
        assert_eq!(chain.last_block().index, 2);
    }

    #[test]
    fn mine_2_blocks_balance_greater_than_1() {
        let (mut chain, hash) = make_test_chain_and_miner();
        chain.mine_block_to_hash(&hash);
        let b1 = chain.utxo_set.balance_of(&hash);
        chain.mine_block_to_hash(&hash);
        let b2 = chain.utxo_set.balance_of(&hash);
        assert!(b2 > b1, "balance should increase after second block");
    }

    #[test]
    fn mine_3_blocks_chain_is_valid() {
        let (mut chain, hash) = make_test_chain_and_miner();
        chain.mine_block_to_hash(&hash);
        chain.mine_block_to_hash(&hash);
        chain.mine_block_to_hash(&hash);
        assert!(chain.is_valid(), "chain should be valid after 3 real mined blocks");
    }

    #[test]
    fn mine_1_block_has_coinbase_tx() {
        let (mut chain, hash) = make_test_chain_and_miner();
        chain.mine_block_to_hash(&hash);
        let block = chain.last_block();
        assert!(!block.transactions.is_empty(), "block must have at least coinbase tx");
        assert!(block.transactions[0].is_coinbase, "first tx must be coinbase");
    }

    #[test]
    fn mine_1_block_coinbase_has_output() {
        let (mut chain, hash) = make_test_chain_and_miner();
        chain.mine_block_to_hash(&hash);
        let coinbase = &chain.last_block().transactions[0];
        assert!(!coinbase.outputs.is_empty(), "coinbase must have at least 1 output");
        // UTXO set credits the miner → balance > 0
        assert!(chain.utxo_set.balance_of(&hash) > 0);
    }

    #[test]
    fn mine_1_block_prev_hash_matches() {
        let (mut chain, hash) = make_test_chain_and_miner();
        let genesis_hash = chain.last_block().hash.clone();
        chain.mine_block_to_hash(&hash);
        let block1 = chain.last_block();
        assert_eq!(block1.prev_hash, genesis_hash);
    }

    #[test]
    fn mine_1_block_hash_non_empty() {
        let (mut chain, hash) = make_test_chain_and_miner();
        chain.mine_block_to_hash(&hash);
        let h = &chain.last_block().hash;
        assert!(!h.is_empty());
        // difficulty=1 → hash starts with "0"
        assert!(h.starts_with('0'), "hash should start with 0 at difficulty 1, got: {}", h);
    }

    #[test]
    fn fresh_devnet_db_starts_at_genesis() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build().unwrap();
        rt.block_on(async {
            let db = fresh_devnet_db(1);
            let chain = db.lock().await;
            // Genesis block = index 0
            assert_eq!(chain.last_block().index, 0);
        });
    }

    #[test]
    fn fresh_devnet_db_respects_difficulty() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build().unwrap();
        rt.block_on(async {
            let db = fresh_devnet_db(3);
            let chain = db.lock().await;
            assert_eq!(chain.difficulty, 3);
        });
    }
}
