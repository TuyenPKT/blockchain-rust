#![allow(dead_code)]

/// Miner — Standalone PoW miner với live stats
///
/// Usage:
///   cargo run -- mine                      → mine vô hạn, địa chỉ random
///   cargo run -- mine <pubkey_hash_hex>    → mine đến địa chỉ cụ thể
///   cargo run -- mine <pubkey_hash_hex> 5  → mine 5 blocks rồi dừng
///
/// Features:
///   - Live hashrate display (H/s) trong khi mining
///   - Difficulty auto-adjustment mỗi 5 blocks (giống Bitcoin (PKT))
///   - Dashboard sau mỗi block: nonce, hash, thời gian, earnings
///   - Tổng kết khi kết thúc: tổng blocks, hashes, earnings, uptime

use std::time::{Duration, Instant};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use crate::block::Block;
use crate::chain::Blockchain;
use crate::message::Message;
use crate::transaction::Transaction;

// ─── Config ───────────────────────────────────────────────────────────────────

pub struct MinerConfig {
    /// Địa chỉ nhận coinbase reward (pubkey_hash hex 40 chars)
    pub address:    String,
    /// None = mine vô hạn; Some(n) = dừng sau n blocks
    pub max_blocks: Option<u32>,
    /// v4.5: Địa chỉ P2P node để sync + submit blocks ("host:port")
    pub node_addr:  Option<String>,
}

impl MinerConfig {
    pub fn new(address: &str) -> Self {
        MinerConfig { address: address.to_string(), max_blocks: None, node_addr: None }
    }
    pub fn with_limit(address: &str, n: u32) -> Self {
        MinerConfig { address: address.to_string(), max_blocks: Some(n), node_addr: None }
    }
    pub fn with_node(mut self, node_addr: &str) -> Self {
        self.node_addr = Some(node_addr.to_string());
        self
    }
}

// ─── Stats ────────────────────────────────────────────────────────────────────

pub struct MinerStats {
    pub blocks_mined:    u32,
    pub total_hashes:    u64,
    pub total_earnings:  u64,   // paklets (coinbase + fees)
    pub best_time_ms:    u64,   // fastest block ms
    pub worst_time_ms:   u64,   // slowest block ms
    start:               Instant,
}

impl MinerStats {
    fn new() -> Self {
        MinerStats {
            blocks_mined:   0,
            total_hashes:   0,
            total_earnings: 0,
            best_time_ms:   u64::MAX,
            worst_time_ms:  0,
            start:          Instant::now(),
        }
    }

    fn update(&mut self, hashes: u64, elapsed_ms: u64, earned: u64) {
        self.blocks_mined   += 1;
        self.total_hashes   += hashes;
        self.total_earnings += earned;
        if elapsed_ms < self.best_time_ms  { self.best_time_ms  = elapsed_ms; }
        if elapsed_ms > self.worst_time_ms { self.worst_time_ms = elapsed_ms; }
    }

    fn uptime_secs(&self) -> f64 { self.start.elapsed().as_secs_f64() }

    fn avg_hashrate(&self) -> f64 {
        let t = self.uptime_secs();
        if t < 0.001 { 0.0 } else { self.total_hashes as f64 / t }
    }
}

// ─── Sync TCP RPC (giao tiếp với P2P node, không cần async) ──────────────────

/// Gửi một Message đến node và nhận response đồng bộ
fn node_rpc(addr: &str, msg: &Message) -> Option<Message> {
    let mut stream = TcpStream::connect(addr).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream.write_all(&msg.serialize()).ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    Message::deserialize(line.trim_end_matches('\n').as_bytes())
}

// ─── Inner mining result ──────────────────────────────────────────────────────

struct MineResult {
    hashes:     u64,
    elapsed_ms: u64,
}

// ─── Core: mine one block, printing live progress ────────────────────────────

/// Mine một block, hiện live progress mỗi 300ms.
/// Trả về (nonce, hash, hashes_tried, elapsed_ms).
fn mine_live(block: &mut Block, difficulty: usize) -> MineResult {
    let target  = "0".repeat(difficulty);
    let start   = Instant::now();
    let mut last_report = Instant::now();
    let mut hashes: u64 = 0;
    let report_interval = Duration::from_millis(300);

    loop {
        let hash = Block::calculate_hash(
            block.index, block.timestamp,
            &block.transactions, &block.prev_hash, block.nonce,
        );
        hashes += 1;

        if hash.starts_with(&target) {
            // clear progress line
            print!("\r{}\r", " ".repeat(72));
            let _ = std::io::stdout().flush();
            block.hash = hash;
            return MineResult { hashes, elapsed_ms: start.elapsed().as_millis() as u64 };
        }

        block.nonce += 1;

        // Live progress line mỗi 300ms
        if last_report.elapsed() >= report_interval {
            let secs   = start.elapsed().as_secs_f64();
            let rate   = if secs > 0.001 { hashes as f64 / secs } else { 0.0 };
            let rate_s = hashrate_str(rate);
            let suffix = &hash[..10];
            print!("\r  ⛏  nonce={:<12}  {:<12}  current={}...",
                block.nonce, rate_s, suffix);
            let _ = std::io::stdout().flush();
            last_report = Instant::now();
        }
    }
}

// ─── Miner ────────────────────────────────────────────────────────────────────

pub struct Miner {
    pub cfg:   MinerConfig,
    pub chain: Blockchain,
    pub stats: MinerStats,
}

impl Miner {
    pub fn new(cfg: MinerConfig) -> Self {
        let chain = crate::storage::load_or_new();
        Miner { cfg, chain, stats: MinerStats::new() }
    }

    /// Mine liên tục đến khi đủ max_blocks hoặc Ctrl-C
    pub fn run(&mut self) {
        println!();
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║              ⛏   Blockchain Rust Miner                      ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();
        println!("  Reward address : {}", &self.cfg.address);
        println!("  Starting diff  : {}", self.chain.difficulty);
        match self.cfg.max_blocks {
            Some(n) => println!("  Target         : {} blocks", n),
            None    => println!("  Target         : ∞ (Ctrl-C to stop)"),
        }
        println!();

        loop {
            if let Some(max) = self.cfg.max_blocks {
                if self.stats.blocks_mined >= max { break; }
            }

            self.mine_one();
        }

        self.print_summary();
    }

    /// v4.5: Sync chain từ P2P node (kết nối lần đầu hoặc khi node ở phía trước)
    fn sync_from_node(&mut self, node_addr: &str) {
        let my_height = (self.chain.chain.len() as u64).saturating_sub(1);
        let resp = node_rpc(node_addr, &Message::GetBlocks { from_index: my_height });
        if let Some(Message::Blocks { blocks }) = resp {
            if blocks.is_empty() { return; }
            println!("  [miner] Sync {} blocks từ node {}", blocks.len(), node_addr);
            for block in blocks {
                let last = self.chain.chain.last().unwrap();
                if block.index == last.index + 1
                    && block.prev_hash == last.hash
                    && block.is_valid(self.chain.difficulty)
                {
                    self.chain.utxo_set.apply_block(&block.transactions);
                    self.chain.chain.push(block);
                }
            }
        }
    }

    /// v4.5: Lấy TX từ mempool của node
    fn fetch_mempool_txs(&self, node_addr: &str) -> Vec<Transaction> {
        match node_rpc(node_addr, &Message::GetMempool) {
            Some(Message::MempoolTxs { txs }) => {
                println!("  [miner] Nhận {} TX từ mempool node {}", txs.len(), node_addr);
                txs
            }
            _ => vec![],
        }
    }

    /// v4.5: Gửi block vừa mine về node
    fn submit_to_node(&self, node_addr: &str, block: &Block) {
        let msg = Message::NewBlock { block: block.clone() };
        match node_rpc(node_addr, &msg) {
            Some(_) | None => {
                println!("  [miner] Block #{} đã gửi đến node {}", block.index, node_addr);
            }
        }
    }

    fn mine_one(&mut self) {
        // ── v4.5: Sync + fetch TXs từ node nếu có ───────────────────────────
        let node_txs: Option<Vec<Transaction>> = if let Some(ref addr) = self.cfg.node_addr.clone() {
            self.sync_from_node(addr);
            Some(self.fetch_mempool_txs(addr))
        } else {
            None
        };

        // ── Prepare candidate block ──────────────────────────────────────────
        let selected = match node_txs {
            Some(txs) => txs,
            None      => self.chain.mempool.select_transactions(500),
        };
        let selected_ids: Vec<String> = selected.iter().map(|t| t.tx_id.clone()).collect();

        let valid_txs: Vec<Transaction> = selected
            .into_iter()
            .filter(|tx| !tx.is_coinbase)
            .collect();

        let total_fee: u64  = valid_txs.iter().map(|t| t.fee).sum();
        let coinbase_reward = 5_000_000_000u64; // 50 PKT in paklets (simplified)
        let earned          = coinbase_reward + total_fee;

        let coinbase  = Transaction::coinbase(&self.cfg.address, total_fee);
        let mut all_txs = vec![coinbase];
        all_txs.extend(valid_txs);

        let prev       = self.chain.chain.last().unwrap();
        let mut block  = Block::new(prev.index + 1, all_txs, prev.hash.clone());

        // ── Adjust difficulty ────────────────────────────────────────────────
        let diff = self.chain.difficulty;

        // ── Print header ─────────────────────────────────────────────────────
        let pending_fees = self.chain.mempool.total_pending_fees();
        println!("  ┌─ Block #{:<5}  diff={}  mempool={} tx  pending_fees={} sat",
            block.index, diff,
            self.chain.mempool.len(),
            pending_fees);

        // ── Mine with live display ────────────────────────────────────────────
        let result = mine_live(&mut block, diff);

        // ── Apply to chain ────────────────────────────────────────────────────
        self.chain.utxo_set.apply_block(&block.transactions);
        self.chain.chain.push(block.clone());
        self.chain.mempool.remove_confirmed(&selected_ids);

        // ── Persist to RocksDB ────────────────────────────────────────────────
        let _ = crate::storage::save_blockchain(&self.chain);

        // ── v4.5: Submit block to node ───────────────────────────────────────
        if let Some(ref addr) = self.cfg.node_addr.clone() {
            self.submit_to_node(addr, &block);
        }

        // Adjust difficulty (every 5 blocks)
        let chain_len = self.chain.chain.len() as u64;
        if chain_len > 0 && chain_len % 5 == 0 {
            self.chain.difficulty = {
                let last  = self.chain.chain.last().unwrap();
                let first = &self.chain.chain[(chain_len - 5) as usize];
                let time_taken    = (last.timestamp - first.timestamp).max(1);
                let time_expected = 10i64 * 5; // 10s per block × 5
                let d = self.chain.difficulty;
                if time_taken < time_expected / 2 {
                    (d + 1).min(8)
                } else if time_taken > time_expected * 2 && d > 1 {
                    d - 1
                } else {
                    d
                }
            };
        }

        // ── Update stats ──────────────────────────────────────────────────────
        self.stats.update(result.hashes, result.elapsed_ms, earned);

        // ── Print result ──────────────────────────────────────────────────────
        let rate = result.hashes as f64 / (result.elapsed_ms.max(1) as f64 / 1000.0);
        println!("  │  nonce={:<12}  hashes={:<12}  {:<12}",
            block.nonce, result.hashes, hashrate_str(rate));
        println!("  │  hash  = {}...{}", &block.hash[..16], &block.hash[56..]);
        println!("  │  time  = {}  earned = {} pkt ({:.8} PKT)",
            elapsed_str(result.elapsed_ms), earned, earned as f64 / 1e8);
        println!("  └─ chain height={:<5}  total_hashes={}  uptime={}",
            self.chain.chain.len() - 1,
            fmt_big(self.stats.total_hashes),
            elapsed_str(self.stats.uptime_secs() as u64 * 1000));
        println!();
    }

    fn print_summary(&self) {
        let s = &self.stats;
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║                    Mining Session Summary                    ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();
        println!("  Blocks mined   : {}", s.blocks_mined);
        println!("  Total hashes   : {}", fmt_big(s.total_hashes));
        println!("  Avg hashrate   : {}", hashrate_str(s.avg_hashrate()));
        println!("  Total earnings : {} pkt  ({:.8} PKT)", s.total_earnings, s.total_earnings as f64 / 1e8);
        if s.blocks_mined > 0 {
            let best  = if s.best_time_ms  == u64::MAX { 0 } else { s.best_time_ms };
            let worst = s.worst_time_ms;
            println!("  Fastest block  : {}", elapsed_str(best));
            println!("  Slowest block  : {}", elapsed_str(worst));
        }
        println!("  Uptime         : {}", elapsed_str(s.uptime_secs() as u64 * 1000));
        println!("  Chain valid    : {}", self.chain.is_valid());
        println!();
    }
}

// ─── Formatting helpers ───────────────────────────────────────────────────────

fn hashrate_str(h: f64) -> String {
    if h >= 1_000_000.0 {
        format!("{:.2} MH/s", h / 1_000_000.0)
    } else if h >= 1_000.0 {
        format!("{:.1} KH/s", h / 1_000.0)
    } else {
        format!("{:.0} H/s ", h)
    }
}

fn elapsed_str(ms: u64) -> String {
    if ms >= 60_000 {
        let m = ms / 60_000;
        let s = (ms % 60_000) / 1000;
        format!("{}m{}s", m, s)
    } else if ms >= 1_000 {
        format!("{:.2}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

fn fmt_big(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}G", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1e3)
    } else {
        format!("{}", n)
    }
}

// ─── Quick demo (không cần CLI) ───────────────────────────────────────────────

/// Mine 3 blocks với địa chỉ hardcoded — dùng trong demo_vMiner()
pub fn demo_miner(address: &str, blocks: u32) {
    let cfg = MinerConfig::with_limit(address, blocks);
    let mut miner = Miner::new(cfg);
    miner.run();
}
