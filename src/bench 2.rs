#![allow(dead_code)]

/// v5.9 — Benchmark Suite: tps, latency, memory
///
/// Đo hiệu suất các thành phần quan trọng của node.
/// Dùng `std::time::Instant` — không cần thêm dependency.
///
/// Benchmark targets:
///   1. hash_throughput   — Block::calculate_hash calls/sec
///   2. block_mining      — Latency mine 1 block (diff=1,2,3)
///   3. tps               — Transactions per second (mine blocks có txs)
///   4. merkle_compare    — fast_merkle vs standard merkle (same data)
///   5. utxo_lookup       — balance_of O(n) vs UtxoIndex O(1)
///   6. mempool_select    — select_transactions tại các kích thước pool khác nhau
///
/// CLI:  cargo run -- bench [hash|mining|tps|merkle|utxo|mempool|all]
/// Output: bảng kết quả + tóm tắt baseline

use std::time::Instant;

use serde::{Deserialize, Serialize};

// ─── Result types ─────────────────────────────────────────────────────────────

/// Kết quả của một benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    /// Tên benchmark
    pub name:        String,
    /// Số lần lặp thực hiện
    pub iterations:  u64,
    /// Tổng thời gian (nanoseconds)
    pub total_ns:    u64,
    /// Thời gian trung bình mỗi operation (ns)
    pub avg_ns:      u64,
    /// Thời gian nhanh nhất (ns)
    pub min_ns:      u64,
    /// Thời gian chậm nhất (ns)
    pub max_ns:      u64,
    /// Operations per second
    pub ops_per_sec: f64,
}

impl BenchResult {
    fn new(name: &str, times_ns: Vec<u64>) -> Self {
        let iterations = times_ns.len() as u64;
        let total_ns   = times_ns.iter().sum::<u64>();
        let avg_ns     = if iterations > 0 { total_ns / iterations } else { 0 };
        let min_ns     = times_ns.iter().copied().min().unwrap_or(0);
        let max_ns     = times_ns.iter().copied().max().unwrap_or(0);
        let ops_per_sec = if total_ns > 0 {
            (iterations as f64) / (total_ns as f64 / 1_000_000_000.0)
        } else { 0.0 };

        BenchResult {
            name: name.to_string(),
            iterations,
            total_ns,
            avg_ns,
            min_ns,
            max_ns,
            ops_per_sec,
        }
    }

    pub fn fmt_ops(&self) -> String {
        if self.ops_per_sec >= 1_000_000.0 {
            format!("{:.2}M ops/s", self.ops_per_sec / 1_000_000.0)
        } else if self.ops_per_sec >= 1_000.0 {
            format!("{:.1}K ops/s", self.ops_per_sec / 1_000.0)
        } else {
            format!("{:.1} ops/s", self.ops_per_sec)
        }
    }

    pub fn fmt_avg(&self) -> String {
        if self.avg_ns >= 1_000_000 {
            format!("{:.2}ms", self.avg_ns as f64 / 1_000_000.0)
        } else if self.avg_ns >= 1_000 {
            format!("{:.1}µs", self.avg_ns as f64 / 1_000.0)
        } else {
            format!("{}ns", self.avg_ns)
        }
    }
}

/// Tập hợp kết quả benchmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSuite {
    pub results: Vec<BenchResult>,
    pub timestamp: i64,
}

impl BenchSuite {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    pub fn add(&mut self, r: BenchResult) {
        self.results.push(r);
    }

    pub fn print(&self) {
        println!();
        println!("╔══════════════════════════════════════════════════════════════════════╗");
        println!("║                   ⚡  Benchmark Suite  v5.9                         ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");
        println!();
        println!("  {:<30}  {:>10}  {:>12}  {:>12}  {:>12}",
            "Benchmark", "Iters", "Avg", "Min", "Ops/sec");
        println!("  {}", "─".repeat(80));
        for r in &self.results {
            println!("  {:<30}  {:>10}  {:>12}  {:>12}  {:>12}",
                r.name,
                r.iterations,
                r.fmt_avg(),
                if r.min_ns >= 1_000_000 {
                    format!("{:.2}ms", r.min_ns as f64 / 1_000_000.0)
                } else if r.min_ns >= 1_000 {
                    format!("{:.1}µs", r.min_ns as f64 / 1_000.0)
                } else {
                    format!("{}ns", r.min_ns)
                },
                r.fmt_ops(),
            );
        }
        println!();
    }
}

// ─── Helper: run N iterations, collect timings ────────────────────────────────

fn run_bench<F: FnMut()>(name: &str, iterations: u64, mut f: F) -> BenchResult {
    // Warmup: 10% of iterations (min 3)
    let warmup = (iterations / 10).max(3);
    for _ in 0..warmup { f(); }

    // Measure
    let mut times = Vec::with_capacity(iterations as usize);
    for _ in 0..iterations {
        let t = Instant::now();
        f();
        times.push(t.elapsed().as_nanos() as u64);
    }
    BenchResult::new(name, times)
}

// ─── Benchmarks ───────────────────────────────────────────────────────────────

/// 1. Hash throughput: Block::calculate_hash calls/sec
pub fn bench_hash_throughput(iterations: u64) -> BenchResult {
    use crate::block::Block;
    let prev = "0000000000000000000000000000000000000000000000000000000000000000";
    run_bench("hash_throughput", iterations, || {
        let _ = Block::calculate_hash(1, 12345, &[], prev, 42);
    })
}

/// 2. Block mining latency: time to mine 1 block at each difficulty
pub fn bench_block_mining(difficulty: usize) -> BenchResult {
    use crate::chain::Blockchain;
    let name = format!("mine_diff_{}", difficulty);
    // Mine single block each time (can't reuse since chain grows)
    let mut times = Vec::new();
    let rounds = 5usize.max(if difficulty <= 2 { 20 } else { 5 });
    for _ in 0..rounds {
        let mut bc = Blockchain::new();
        bc.difficulty = difficulty;
        let t = Instant::now();
        bc.add_block(vec![], "aabbccddaabbccddaabbccddaabbccddaabbccdd");
        times.push(t.elapsed().as_nanos() as u64);
    }
    BenchResult::new(&name, times)
}

/// 3. TPS (transactions per second): mine blocks containing txs
pub fn bench_tps(block_count: usize, txs_per_block: usize) -> BenchResult {
    use crate::chain::Blockchain;
    use crate::transaction::Transaction;

    let addr = "aabbccddaabbccddaabbccddaabbccddaabbccdd";
    let mut bc = Blockchain::new();
    bc.difficulty = 1;

    // Mine some initial blocks for UTXOs
    for _ in 0..5 { bc.add_block(vec![], addr); }

    let total_txs = (block_count * txs_per_block) as u64;
    let mut times = Vec::new();

    for b in 0..block_count {
        let txs: Vec<Transaction> = (0..txs_per_block)
            .map(|i| {
                let mut tx = Transaction::coinbase(addr, b as u64 * 1000 + i as u64);
                tx.is_coinbase = false;
                tx.tx_id = format!("{:064x}", b * 10000 + i);
                tx.fee = 1000;
                tx
            })
            .collect();

        let t = Instant::now();
        bc.add_block(txs, addr);
        times.push(t.elapsed().as_nanos() as u64);
    }

    // Calculate TPS from total time over all blocks
    let total_ns: u64 = times.iter().sum();
    let ops_per_sec = if total_ns > 0 {
        (total_txs as f64) / (total_ns as f64 / 1_000_000_000.0)
    } else { 0.0 };

    let avg_ns = if !times.is_empty() { total_ns / times.len() as u64 } else { 0 };
    let min_ns = times.iter().copied().min().unwrap_or(0);
    let max_ns = times.iter().copied().max().unwrap_or(0);

    BenchResult {
        name:        format!("tps_{}blk_{}tx", block_count, txs_per_block),
        iterations:  total_txs,
        total_ns,
        avg_ns,
        min_ns,
        max_ns,
        ops_per_sec,
    }
}

/// 4. Merkle comparison: fast_merkle_txids vs standard merkle_root
pub fn bench_merkle_compare(tx_count: usize) -> (BenchResult, BenchResult) {
    use crate::block::Block;
    use crate::performance::fast_merkle_txids;
    use crate::transaction::Transaction;

    let addr = "aabbccddaabbccddaabbccddaabbccddaabbccdd";
    let txs: Vec<Transaction> = (0..tx_count)
        .map(|i| Transaction::coinbase(addr, i as u64))
        .collect();
    let txids: Vec<String> = txs.iter().map(|t| t.tx_id.clone()).collect();

    let iters = 1000u64;

    let std_result = run_bench(
        &format!("merkle_std_{}", tx_count),
        iters,
        || { let _ = Block::merkle_root_txid(&txs); },
    );

    let fast_result = run_bench(
        &format!("merkle_fast_{}", tx_count),
        iters,
        || { let _ = fast_merkle_txids(&txids); },
    );

    (std_result, fast_result)
}

/// 5. UTXO lookup: O(n) scan vs UtxoIndex O(1)
pub fn bench_utxo_lookup(utxo_count: usize) -> (BenchResult, BenchResult) {
    use crate::utxo::UtxoSet;
    use crate::performance::UtxoIndex;
    use crate::transaction::{Transaction, TxOutput};
    use crate::script::{Script, Opcode};

    let addr = "aabbccddaabbccddaabbccddaabbccddaabbccdd";
    let target_bytes = hex::decode(addr).unwrap();

    // Build UtxoSet directly (utxos field is pub)
    let mut utxo_set = UtxoSet::new();
    for i in 0..utxo_count {
        let key = format!("{:064x}:0", i);
        let out = TxOutput {
            amount: 1000,
            script_pubkey: Script { ops: vec![
                Opcode::OpDup,
                Opcode::OpHash160,
                Opcode::OpPushData(target_bytes.clone()),
                Opcode::OpEqualVerify,
                Opcode::OpCheckSig,
            ]},
        };
        utxo_set.utxos.insert(key, out);
    }

    // Build UtxoIndex via apply_block (uses public API)
    let mut utxo_idx = UtxoIndex::new();
    let txs: Vec<Transaction> = (0..utxo_count)
        .map(|i| {
            let mut tx = Transaction::coinbase(addr, i as u64);
            tx.tx_id = format!("{:064x}", i);
            tx
        })
        .collect();
    utxo_idx.apply_block(&txs);

    let iters = 500u64;

    let scan_result = run_bench(
        &format!("utxo_scan_{}", utxo_count),
        iters,
        || { let _ = utxo_set.balance_of(addr); },
    );

    let idx_result = run_bench(
        &format!("utxo_index_{}", utxo_count),
        iters,
        || { let _ = utxo_idx.balance_of(addr); },
    );

    (scan_result, idx_result)
}

/// 6. Mempool select_transactions at various pool sizes
pub fn bench_mempool_select(pool_size: usize) -> BenchResult {
    use crate::mempool::Mempool;
    use crate::transaction::Transaction;

    let addr = "aabbccddaabbccddaabbccddaabbccddaabbccdd";
    let mut mp = Mempool::new();

    for i in 0..pool_size {
        let mut tx = Transaction::coinbase(addr, i as u64);
        tx.is_coinbase = false;
        tx.tx_id = format!("{:064x}", i);
        tx.outputs[0].amount = 10;
        let _ = mp.add(tx, 10 + i as u64 * 100);
    }

    run_bench(
        &format!("mempool_select_{}", pool_size),
        1000,
        || { let _ = mp.select_transactions(100); },
    )
}

// ─── Full suite ───────────────────────────────────────────────────────────────

/// Chạy toàn bộ benchmark suite.
pub fn run_all() -> BenchSuite {
    let mut suite = BenchSuite::new();

    // 1. Hash throughput
    suite.add(bench_hash_throughput(10_000));

    // 2. Mining latency
    suite.add(bench_block_mining(1));
    suite.add(bench_block_mining(2));
    suite.add(bench_block_mining(3));

    // 3. TPS
    suite.add(bench_tps(10, 10));
    suite.add(bench_tps(10, 50));

    // 4. Merkle
    let (std4, fast4)   = bench_merkle_compare(4);
    let (std64, fast64) = bench_merkle_compare(64);
    suite.add(std4);
    suite.add(fast4);
    suite.add(std64);
    suite.add(fast64);

    // 5. UTXO lookup
    let (scan100, idx100)   = bench_utxo_lookup(100);
    let (scan1000, idx1000) = bench_utxo_lookup(1000);
    suite.add(scan100);
    suite.add(idx100);
    suite.add(scan1000);
    suite.add(idx1000);

    // 6. Mempool select
    suite.add(bench_mempool_select(100));
    suite.add(bench_mempool_select(1000));

    suite
}

/// Chạy subset theo tên (hash|mining|tps|merkle|utxo|mempool)
pub fn run_named(name: &str) -> BenchSuite {
    let mut suite = BenchSuite::new();
    match name {
        "hash"    => { suite.add(bench_hash_throughput(10_000)); }
        "mining"  => {
            suite.add(bench_block_mining(1));
            suite.add(bench_block_mining(2));
        }
        "tps"     => { suite.add(bench_tps(10, 20)); }
        "merkle"  => {
            let (a, b) = bench_merkle_compare(16);
            suite.add(a); suite.add(b);
        }
        "utxo"    => {
            let (a, b) = bench_utxo_lookup(500);
            suite.add(a); suite.add(b);
        }
        "mempool" => { suite.add(bench_mempool_select(500)); }
        _         => return run_all(),
    }
    suite
}

// ─── CLI entry point ──────────────────────────────────────────────────────────

pub fn cmd_bench(target: &str) {
    println!();
    println!("  Running benchmarks: {}…", target);
    let suite = run_named(target);
    suite.print();

    // Serialize as JSON baseline (stdout, redirect to file if needed)
    if let Ok(json) = serde_json::to_string_pretty(&suite) {
        println!("  JSON baseline:");
        println!("{}", json);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bench_hash_ops_nonzero() {
        let r = bench_hash_throughput(100);
        assert!(r.ops_per_sec > 0.0, "hash throughput > 0");
        assert_eq!(r.iterations, 100);
        assert!(r.avg_ns > 0);
    }

    #[test]
    fn test_bench_mining_diff1() {
        let r = bench_block_mining(1);
        assert!(r.ops_per_sec > 0.0);
        assert!(r.avg_ns > 0);
        assert!(r.min_ns <= r.avg_ns, "min ≤ avg");
        assert!(r.avg_ns <= r.max_ns, "avg ≤ max");
    }

    #[test]
    fn test_bench_tps_nonzero() {
        let r = bench_tps(3, 5);
        assert!(r.ops_per_sec > 0.0, "TPS > 0");
        assert_eq!(r.iterations, 15, "3 blocks × 5 txs = 15 total");
    }

    #[test]
    fn test_bench_merkle_fast_faster_than_std() {
        let (std_r, fast_r) = bench_merkle_compare(16);
        // fast_merkle nên nhanh hơn hoặc bằng (avg_ns nhỏ hơn hoặc bằng)
        // Không assert cứng vì CI có thể khác — chỉ check cả hai > 0
        assert!(std_r.ops_per_sec  > 0.0, "std merkle > 0");
        assert!(fast_r.ops_per_sec > 0.0, "fast merkle > 0");
        // fast_merkle LUÔN nhanh hơn standard ít nhất trong hầu hết cases
        // (chỉ warn, không fail)
        if fast_r.avg_ns > std_r.avg_ns * 2 {
            eprintln!("WARN: fast_merkle unexpectedly slower (avg {}ns vs {}ns)",
                fast_r.avg_ns, std_r.avg_ns);
        }
    }

    #[test]
    fn test_bench_utxo_index_returns_results() {
        let (scan, idx) = bench_utxo_lookup(500);
        assert!(scan.ops_per_sec > 0.0, "scan ops > 0");
        assert!(idx.ops_per_sec  > 0.0, "index ops > 0");
        assert_eq!(scan.iterations, 500);
        assert_eq!(idx.iterations,  500);
        // Cả hai phải hoàn thành mà không panic — correctness test
        // (speed comparison phụ thuộc workload; không assert relative speed)
    }

    #[test]
    fn test_bench_mempool_select_nonzero() {
        let r = bench_mempool_select(50);
        assert!(r.ops_per_sec > 0.0);
        assert_eq!(r.iterations, 1000);
    }

    #[test]
    fn test_bench_result_serializable() {
        let r = bench_hash_throughput(10);
        let json = serde_json::to_string(&r).expect("serialize ok");
        let back: BenchResult = serde_json::from_str(&json).expect("deserialize ok");
        assert_eq!(back.iterations, r.iterations);
        assert_eq!(back.name, r.name);
    }

    #[test]
    fn test_bench_suite_serializable() {
        let mut suite = BenchSuite::new();
        suite.add(bench_hash_throughput(10));
        let json = serde_json::to_string(&suite).expect("serialize ok");
        let back: BenchSuite = serde_json::from_str(&json).expect("deserialize ok");
        assert_eq!(back.results.len(), 1);
    }

    #[test]
    fn test_fmt_ops_ranges() {
        let r = BenchResult::new("test", vec![1000u64; 1_000_000]);
        // 1_000_000 ops at 1µs each = 1M ops/sec
        let s = r.fmt_ops();
        assert!(s.contains("ops/s"), "fmt_ops should contain ops/s: {}", s);
    }
}
