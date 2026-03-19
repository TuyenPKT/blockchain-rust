#![allow(dead_code)]

/// Miner — Online-only PoW miner, kết nối P2P node
///
/// Miner KHÔNG lưu chain local. Node là authority.
/// Flow mỗi block:
///   1. GetTemplate  → node trả prev_hash, height, difficulty, mempool txs
///   2. Mine block với difficulty từ node
///   3. NewBlock     → node validate + save + broadcast
///
/// Default node: seed.testnet.oceif.com:8333
///
/// Usage:
///   cargo run -- mine                           → dùng ví ~/.pkt/wallet.key, kết nối seed
///   cargo run -- mine <addr>                    → địa chỉ cụ thể, kết nối seed
///   cargo run -- mine <addr> <node>             → kết nối node cụ thể
///   cargo run -- mine <addr> <n> <node>         → dừng sau n blocks

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use rayon::prelude::*;

use crate::block::Block;
use crate::message::Message;
use crate::transaction::{Transaction, TxOutput};
use crate::cpu_miner::default_threads;
use crate::reward::RewardEngine;
use crate::staking::StakingPool;

/// Paklets per PKT (1 PKT = 1_000_000_000 paklets)
const PAKLETS_PER_PKT: u64 = 1_000_000_000;

/// Format paklets → "50.000 PKT"
fn fmt_pkt(paklets: u64) -> String {
    let whole = paklets / PAKLETS_PER_PKT;
    let frac  = paklets % PAKLETS_PER_PKT;
    format!("{}.{:09} PKT", whole, frac)
}

pub const DEFAULT_NODE: &str = "seed.testnet.oceif.com:8333";

// ─── Config ───────────────────────────────────────────────────────────────────

pub struct MinerConfig {
    /// Địa chỉ nhận coinbase reward (pubkey_hash hex 40 chars)
    pub address:    String,
    /// None = mine vô hạn; Some(n) = dừng sau n blocks
    pub max_blocks: Option<u32>,
    /// P2P node để lấy template + submit blocks
    pub node_addr:  String,
    /// Số rayon threads (mặc định = cores/3)
    pub threads:    usize,
}

impl MinerConfig {
    pub fn new(address: &str) -> Self {
        MinerConfig { address: address.to_string(), max_blocks: None,
            node_addr: DEFAULT_NODE.to_string(), threads: default_threads() }
    }
    pub fn with_limit(address: &str, n: u32) -> Self {
        MinerConfig { address: address.to_string(), max_blocks: Some(n),
            node_addr: DEFAULT_NODE.to_string(), threads: default_threads() }
    }
    pub fn with_node(mut self, node_addr: &str) -> Self {
        self.node_addr = node_addr.to_string(); self
    }
    pub fn with_threads(mut self, n: usize) -> Self {
        self.threads = n.max(1); self
    }
}

// ─── Stats ────────────────────────────────────────────────────────────────────

pub struct MinerStats {
    pub blocks_mined:   u32,
    pub total_hashes:   u64,
    pub total_earnings: u64,
    pub best_time_ms:   u64,
    pub worst_time_ms:  u64,
    start:              Instant,
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

// ─── TCP RPC (sync, không cần async) ─────────────────────────────────────────

fn node_rpc(addr: &str, msg: &Message) -> Option<Message> {
    let mut stream = match TcpStream::connect(addr) {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("  [rpc] connect {} FAILED: {}", addr, e);
            return None;
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    if let Err(e) = stream.write_all(&msg.serialize()) {
        eprintln!("  [rpc] write FAILED: {}", e);
        return None;
    }
    let mut line = String::new();
    match BufReader::new(stream).read_line(&mut line) {
        Ok(0)  => { eprintln!("  [rpc] connection closed before response"); None }
        Err(e) => { eprintln!("  [rpc] read FAILED: {}", e); None }
        Ok(_)  => Message::deserialize(line.trim_end_matches('\n').as_bytes()),
    }
}

// ─── Live mining loop (rayon parallel) ───────────────────────────────────────

struct MineResult { hashes: u64, elapsed_ms: u64 }

fn mine_live(block: &mut Block, difficulty: usize, threads: usize) -> MineResult {
    let t0           = Instant::now();
    let stop         = Arc::new(AtomicBool::new(false));
    let total_hashes = Arc::new(AtomicU64::new(0));
    let target       = "0".repeat(difficulty);
    let n            = threads.max(1);
    let chunk        = u64::MAX / n as u64;

    // Precompute roots once — only nonce changes per iteration.
    let txid_root    = Block::merkle_root_txid(&block.transactions);
    let witness_root = block.witness_root.clone();
    let prefix = format!(
        "{}|{}|{}|{}|{}|",
        block.index, block.timestamp, txid_root, witness_root, block.prev_hash,
    );

    // Progress reporter: shares latest_nonce to display hash preview.
    let latest_nonce = Arc::new(AtomicU64::new(0));
    let stop_prog    = Arc::clone(&stop);
    let hashes_prog  = Arc::clone(&total_hashes);
    let nonce_prog   = Arc::clone(&latest_nonce);
    let prefix_prog  = prefix.clone();
    let start_prog   = t0;
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(300));
            if stop_prog.load(Ordering::Relaxed) { break; }
            let h      = hashes_prog.load(Ordering::Relaxed);
            let nonce  = nonce_prog.load(Ordering::Relaxed);
            let rate   = h as f64 / start_prog.elapsed().as_secs_f64().max(0.001);
            let header = format!("{}{}", prefix_prog, nonce);
            let hash   = hex::encode(blake3::hash(header.as_bytes()).as_bytes());
            print!("\r  ⛏  hashes={:<12}  {:<12}  {}...",
                h, hashrate_str(rate), &hash[..10]);
            let _ = std::io::stdout().flush();
        }
    });

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .build()
        .expect("miner: build thread pool");

    let found = pool.install(|| {
        (0..n).into_par_iter().find_map_any(|tid| {
            let start_nonce = (tid as u64).saturating_mul(chunk);
            let end_nonce   = if tid == n - 1 { u64::MAX } else { start_nonce.saturating_add(chunk) };
            let mut local   = 0u64;

            for nonce in start_nonce..end_nonce {
                if stop.load(Ordering::Relaxed) {
                    total_hashes.fetch_add(local, Ordering::Relaxed);
                    return None;
                }
                let header = format!("{}{}", prefix, nonce);
                let hash   = hex::encode(blake3::hash(header.as_bytes()).as_bytes());
                local     += 1;
                // Flush every 50k so progress reporter sees live data
                if local % 50_000 == 0 {
                    total_hashes.fetch_add(50_000, Ordering::Relaxed);
                    latest_nonce.store(nonce, Ordering::Relaxed);
                    local = 0;
                }
                if hash.starts_with(&target) {
                    stop.store(true, Ordering::Relaxed);
                    total_hashes.fetch_add(local, Ordering::Relaxed);
                    return Some((nonce, hash));
                }
            }
            total_hashes.fetch_add(local, Ordering::Relaxed);
            None
        })
    });

    print!("\r{}\r", " ".repeat(72));
    let _ = std::io::stdout().flush();

    let (nonce, hash) = found.unwrap_or((0, "f".repeat(64)));
    block.nonce = nonce;
    block.hash  = hash;

    MineResult {
        hashes:     total_hashes.load(Ordering::Relaxed),
        elapsed_ms: t0.elapsed().as_millis() as u64,
    }
}

// ─── Miner ────────────────────────────────────────────────────────────────────

pub struct Miner {
    pub cfg:          MinerConfig,
    pub stats:        MinerStats,
    /// v10.5 — local staking pool: reward delegators via coinbase outputs each block.
    pub staking_pool: StakingPool,
}

impl Miner {
    pub fn new(cfg: MinerConfig) -> Self {
        Miner { cfg, stats: MinerStats::new(), staking_pool: StakingPool::new() }
    }

    pub fn run(&mut self) {
        println!();
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║              ⛏   Blockchain Rust Miner                      ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();
        println!("  Reward address : {}", self.cfg.address);
        println!("  Threads        : {}  (cores/3 default = {})", self.cfg.threads, default_threads());
        println!("  Node           : {}", self.cfg.node_addr);
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

    fn mine_one(&mut self) {
        let node = self.cfg.node_addr.clone();

        // ── 1. Lấy block template từ node ────────────────────────────────────
        let (prev_hash, height, diff, pending_txs) = match node_rpc(&node, &Message::GetTemplate) {
            Some(Message::Template { prev_hash, height, difficulty, txs }) => {
                (prev_hash, height, difficulty, txs)
            }
            _ => {
                eprintln!("  [miner] ⚠️  Node {} không phản hồi — thử lại sau 3s", node);
                std::thread::sleep(Duration::from_secs(3));
                return;
            }
        };

        // ── 2. Build block ────────────────────────────────────────────────────
        let valid_txs: Vec<Transaction> = pending_txs.into_iter()
            .filter(|t| !t.is_coinbase)
            .collect();
        let total_fee: u64  = valid_txs.iter().map(|t| t.fee).sum();
        let coinbase_reward = RewardEngine::subsidy_at(height);
        let earned          = coinbase_reward + total_fee;

        // v10.5 — distribute staking rewards in coinbase TX
        let staking_payouts = self.staking_pool.collect_block_rewards(coinbase_reward);
        let mut coinbase    = Transaction::coinbase_at(&self.cfg.address, total_fee, height);
        for (addr, amount) in &staking_payouts {
            coinbase.outputs.push(TxOutput::p2pkh(*amount, addr));
        }
        if !staking_payouts.is_empty() {
            coinbase.tx_id  = coinbase.calculate_txid();
            coinbase.wtx_id = coinbase.calculate_wtxid();
        }
        let mut all_txs = vec![coinbase];
        all_txs.extend(valid_txs);
        let mut block   = Block::new(height, all_txs, prev_hash);

        println!("  ┌─ Block #{:<5}  diff={}  txs={}  node={}",
            height, diff, block.transactions.len() - 1, &node);

        // ── 3. Mine ───────────────────────────────────────────────────────────
        let result = mine_live(&mut block, diff, self.cfg.threads);

        // ── 4. Submit về node ─────────────────────────────────────────────────
        match node_rpc(&node, &Message::NewBlock { block: block.clone() }) {
            Some(Message::Height { height: tip }) => {
                println!("  [miner] ✅ Block #{} accepted  node_height={}", block.index, tip);
                // Persist earnings to local RocksDB
                if let Err(e) = crate::storage::add_mined_earnings(&self.cfg.address, earned) {
                    eprintln!("  [miner] ⚠️  Cannot save balance: {}", e);
                }
            }
            _ => {
                println!("  [miner] ⚠️  Không nhận được ack từ node (block vẫn có thể đã được nhận)");
            }
        }

        // ── 5. Stats + display ────────────────────────────────────────────────
        self.stats.update(result.hashes, result.elapsed_ms, earned);
        let rate = result.hashes as f64 / (result.elapsed_ms.max(1) as f64 / 1000.0);
        println!("  │  nonce={:<12}  hashes={:<12}  {:<12}",
            block.nonce, result.hashes, hashrate_str(rate));
        println!("  │  hash  = {}...{}", &block.hash[..16], &block.hash[56..]);
        println!("  │  time  = {}  earned = {}",
            elapsed_str(result.elapsed_ms), fmt_pkt(earned));
        println!("  └─ total_blocks={}  total_hashes={}  uptime={}",
            self.stats.blocks_mined,
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
        println!("  Total earnings : {}", fmt_pkt(s.total_earnings));
        if s.blocks_mined > 0 {
            let best = if s.best_time_ms == u64::MAX { 0 } else { s.best_time_ms };
            println!("  Fastest block  : {}", elapsed_str(best));
            println!("  Slowest block  : {}", elapsed_str(s.worst_time_ms));
        }
        println!("  Uptime         : {}", elapsed_str(s.uptime_secs() as u64 * 1000));
        println!();
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn hashrate_str(h: f64) -> String {
    if h >= 1_000_000.0 { format!("{:.2} MH/s", h / 1_000_000.0) }
    else if h >= 1_000.0 { format!("{:.1} KH/s", h / 1_000.0) }
    else { format!("{:.0} H/s ", h) }
}

fn elapsed_str(ms: u64) -> String {
    if ms >= 60_000 {
        format!("{}m{}s", ms / 60_000, (ms % 60_000) / 1000)
    } else if ms >= 1_000 {
        format!("{:.2}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

fn fmt_big(n: u64) -> String {
    if n >= 1_000_000_000 { format!("{:.2}G", n as f64 / 1e9) }
    else if n >= 1_000_000 { format!("{:.2}M", n as f64 / 1e6) }
    else if n >= 1_000     { format!("{:.1}K", n as f64 / 1e3) }
    else                   { format!("{}", n) }
}

pub fn demo_miner(address: &str, blocks: u32) {
    let cfg = MinerConfig::with_limit(address, blocks);
    let mut miner = Miner::new(cfg);
    miner.run();
}
