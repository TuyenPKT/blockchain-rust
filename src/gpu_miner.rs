#![allow(dead_code)]
//! v6.4 — GPU Miner Abstraction
//!
//! Unified interface cho Software / OpenCL / CUDA backends.
//! Actual kernel implementations: v6.5 (OpenCL), v6.6 (CUDA).
//!
//! Software fallback: rayon work-stealing — giống cpu_miner nhưng
//! thông qua GPU abstraction layer, chiếm 1/3 logical compute units.
//!
//! Structs:
//!   GpuBackend         — enum: Software | OpenCL | Cuda
//!   GpuDeviceInfo      — thông tin thiết bị (tên, compute_units, memory_mb)
//!   GpuMinerConfig     — cấu hình: backend, address, difficulty, compute_units
//!   GpuMineResult      — nonce, hash, hashes_tried, elapsed_ms, backend_used
//!   GpuMinerStats      — blocks_mined, total_hashes, avg_hashrate()
//!   GpuMiner           — miner chính, mine_block() -> GpuMineResult
//!
//! Free functions:
//!   detect_devices()             -> Vec<GpuDeviceInfo>
//!   default_compute_units()      -> usize  (logical_cores / 3)
//!   cmd_gpu_mine(addr, diff, n, backend_str)
//!
//! CLI:
//!   cargo run -- gpumine [addr] [diff] [blocks] [software|opencl|cuda]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use rayon::prelude::*;

use crate::blake3_hash::{Blake3Block, HASH_VERSION_BLAKE3, pow_hash, to_hex};
use crate::transaction::Transaction;

// ── Backend enum ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum GpuBackend {
    /// CPU-based rayon fallback — always available.
    Software,
    /// OpenCL — requires feature `opencl` (v6.5). Currently stub.
    OpenCL,
    /// CUDA — requires feature `cuda` (v6.6). Currently stub.
    Cuda,
}

impl GpuBackend {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "opencl" | "ocl" => GpuBackend::OpenCL,
            "cuda"           => GpuBackend::Cuda,
            _                => GpuBackend::Software,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            GpuBackend::Software => "Software (CPU rayon)",
            GpuBackend::OpenCL   => "OpenCL (v6.5 — BLAKE3 kernel)",
            GpuBackend::Cuda     => "CUDA   (v6.6 — BLAKE3 PTX kernel)",
        }
    }

    pub fn is_available(&self) -> bool {
        match self {
            GpuBackend::Software => true,
            GpuBackend::OpenCL   => crate::opencl_kernel::opencl_available(),
            GpuBackend::Cuda     => crate::cuda_kernel::cuda_available(),
        }
    }
}

// ── Device info ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GpuDeviceInfo {
    pub backend:       GpuBackend,
    pub name:          String,
    pub compute_units: usize,
    /// VRAM or system RAM (MB)
    pub memory_mb:     u64,
    pub available:     bool,
}

impl GpuDeviceInfo {
    pub fn print(&self) {
        let avail = if self.available { "✅" } else { "⚠️  (not compiled)" };
        println!("  [{:8}] {:30} CU={:<4} MEM={}MB  {}",
            match self.backend {
                GpuBackend::Software => "SOFTWARE",
                GpuBackend::OpenCL   => "OPENCL  ",
                GpuBackend::Cuda     => "CUDA    ",
            },
            self.name,
            self.compute_units,
            self.memory_mb,
            avail,
        );
    }
}

/// Enumerate available compute devices.
/// Currently only Software is truly available.
pub fn detect_devices() -> Vec<GpuDeviceInfo> {
    let cores = num_cpus::get();
    let cu    = default_compute_units();

    vec![
        GpuDeviceInfo {
            backend:       GpuBackend::Software,
            name:          format!("CPU ({} logical cores)", cores),
            compute_units: cu,
            memory_mb:     0, // system RAM — not tracked
            available:     true,
        },
        GpuDeviceInfo {
            backend:       GpuBackend::OpenCL,
            name:          if crate::opencl_kernel::opencl_available() {
                               "OpenCL GPU (compiled)".into()
                           } else {
                               "OpenCL device (build with --features opencl)".into()
                           },
            compute_units: 0,
            memory_mb:     0,
            available:     crate::opencl_kernel::opencl_available(),
        },
        GpuDeviceInfo {
            backend:       GpuBackend::Cuda,
            name:          if crate::cuda_kernel::cuda_available() {
                               "CUDA GPU (compiled)".into()
                           } else {
                               "CUDA device (build with --features cuda)".into()
                           },
            compute_units: 0,
            memory_mb:     0,
            available:     crate::cuda_kernel::cuda_available(),
        },
    ]
}

/// Default compute units = max(1, logical_cores / 3).
/// Mirrors cpu_miner::default_threads() — leave 2/3 for node + OS.
pub fn default_compute_units() -> usize {
    (num_cpus::get() / 3).max(1)
}

// ── Config ────────────────────────────────────────────────────────────────────

pub struct GpuMinerConfig {
    pub backend:       GpuBackend,
    pub address:       String,
    pub difficulty:    usize,
    pub max_blocks:    Option<u32>,
    pub compute_units: usize,
}

impl GpuMinerConfig {
    pub fn new(address: &str) -> Self {
        GpuMinerConfig {
            backend:       GpuBackend::Software,
            address:       address.to_string(),
            difficulty:    3,
            max_blocks:    None,
            compute_units: default_compute_units(),
        }
    }

    pub fn with_backend(mut self, b: GpuBackend) -> Self {
        self.backend = b; self
    }

    pub fn with_difficulty(mut self, d: usize) -> Self {
        self.difficulty = d; self
    }

    pub fn with_max_blocks(mut self, n: u32) -> Self {
        self.max_blocks = Some(n); self
    }

    pub fn with_compute_units(mut self, n: usize) -> Self {
        self.compute_units = n.max(1); self
    }
}

// ── Mine result ───────────────────────────────────────────────────────────────

pub struct GpuMineResult {
    pub nonce:        u64,
    pub hash:         String,
    pub hashes_tried: u64,
    pub elapsed_ms:   u64,
    pub backend_used: GpuBackend,
}

// ── Stats ─────────────────────────────────────────────────────────────────────

pub struct GpuMinerStats {
    pub blocks_mined:  u32,
    pub total_hashes:  u64,
    pub total_time_ms: u64,
}

impl GpuMinerStats {
    fn new() -> Self {
        GpuMinerStats { blocks_mined: 0, total_hashes: 0, total_time_ms: 0 }
    }

    fn record(&mut self, r: &GpuMineResult) {
        self.blocks_mined  += 1;
        self.total_hashes  += r.hashes_tried;
        self.total_time_ms += r.elapsed_ms;
    }

    pub fn avg_hashrate(&self) -> f64 {
        let secs = self.total_time_ms as f64 / 1000.0;
        if secs < 0.001 { 0.0 } else { self.total_hashes as f64 / secs }
    }
}

// ── Core: software backend (rayon) ───────────────────────────────────────────

fn mine_software(block: &Blake3Block, compute_units: usize, difficulty: usize) -> GpuMineResult {
    let t0           = Instant::now();
    let stop         = Arc::new(AtomicBool::new(false));
    let total_hashes = Arc::new(AtomicU64::new(0));
    let target       = "0".repeat(difficulty);

    let n     = compute_units.max(1);
    let chunk = u64::MAX / n as u64;
    let hv    = block.hash_version;

    let prefix = format!(
        "{}|{}|{}|{}|{}|",
        block.index, block.timestamp,
        block.txid_root, block.witness_root,
        block.prev_hash,
    );

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .build()
        .expect("gpu_miner: build thread pool");

    let found = pool.install(|| {
        (0..n).into_par_iter().find_map_any(|tid| {
            let start = (tid as u64).saturating_mul(chunk);
            let end   = if tid == n - 1 { u64::MAX } else { start.saturating_add(chunk) };

            let mut local = 0u64;
            for nonce in start..end {
                if stop.load(Ordering::Relaxed) {
                    total_hashes.fetch_add(local, Ordering::Relaxed);
                    return None;
                }
                let header = format!("{}{}|{}", prefix, nonce, hv);
                let raw    = pow_hash(header.as_bytes(), hv);
                local     += 1;
                if to_hex(&raw).starts_with(&target) {
                    stop.store(true, Ordering::Relaxed);
                    total_hashes.fetch_add(local, Ordering::Relaxed);
                    return Some((nonce, to_hex(&raw)));
                }
            }
            total_hashes.fetch_add(local, Ordering::Relaxed);
            None
        })
    });

    let (nonce, hash) = found.unwrap_or((0, "f".repeat(64)));
    let hashes_tried  = total_hashes.load(Ordering::Relaxed);
    let elapsed_ms    = t0.elapsed().as_millis() as u64;

    GpuMineResult { nonce, hash, hashes_tried, elapsed_ms, backend_used: GpuBackend::Software }
}

// ── Stub backends ─────────────────────────────────────────────────────────────

fn mine_opencl(block: &Blake3Block, difficulty: usize, compute_units: usize) -> GpuMineResult {
    use crate::opencl_kernel::{opencl_mine, OpenClConfig};
    let cfg = OpenClConfig { compute_units, ..OpenClConfig::default() };
    opencl_mine(block, difficulty, &cfg)
}

fn mine_cuda(block: &Blake3Block, difficulty: usize, compute_units: usize) -> GpuMineResult {
    use crate::cuda_kernel::{cuda_mine, CudaConfig};
    let block_size = (compute_units as u32).min(256).max(32);
    let grid_size  = (compute_units as u32).min(128).max(1);
    let cfg = CudaConfig::new(block_size, grid_size);
    cuda_mine(block, difficulty, &cfg)
}

// ── GpuMiner ──────────────────────────────────────────────────────────────────

pub struct GpuMiner {
    pub config: GpuMinerConfig,
    pub stats:  GpuMinerStats,
}

impl GpuMiner {
    pub fn new(config: GpuMinerConfig) -> Self {
        GpuMiner { config, stats: GpuMinerStats::new() }
    }

    /// Mine a block. Falls back to Software if requested backend unavailable.
    pub fn mine_block(&mut self, block: &Blake3Block) -> GpuMineResult {
        let result = match &self.config.backend {
            GpuBackend::Software => mine_software(block, self.config.compute_units, self.config.difficulty),
            GpuBackend::OpenCL   => mine_opencl(block, self.config.difficulty, self.config.compute_units),
            GpuBackend::Cuda     => mine_cuda(block, self.config.difficulty, self.config.compute_units),
        };
        self.stats.record(&result);
        result
    }

    pub fn run(&mut self) {
        let backends = detect_devices();

        println!();
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║              🎮  GPU Miner Abstraction  v6.4                ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();
        println!("  Devices detected:");
        for d in &backends { d.print(); }
        println!();
        println!("  Backend        : {}", self.config.backend.name());
        println!("  Compute units  : {}", self.config.compute_units);
        println!("  Reward address : {}", self.config.address);
        println!("  Difficulty     : {}", self.config.difficulty);
        match self.config.max_blocks {
            Some(n) => println!("  Target         : {} blocks", n),
            None    => println!("  Target         : ∞  (Ctrl-C to stop)"),
        }
        println!();

        let mut block_idx: u64 = 1;
        loop {
            if let Some(max) = self.config.max_blocks {
                if self.stats.blocks_mined >= max { break; }
            }

            let coinbase = Transaction::coinbase_at(&self.config.address, 0, block_idx);
            let txid_root    = format!("{:064x}", block_idx);
            let witness_root = format!("{:064x}", block_idx);

            let block = Blake3Block::new(
                block_idx,
                chrono::Utc::now().timestamp(),
                txid_root,
                witness_root,
                "0".repeat(64),
                HASH_VERSION_BLAKE3,
            );

            println!("  ┌─ Block #{:<5}  diff={}  backend={}", block_idx, self.config.difficulty, self.config.backend.name());

            let result = self.mine_block(&block);
            let rate   = result.hashes_tried as f64 / (result.elapsed_ms.max(1) as f64 / 1000.0);

            println!("  │  nonce={:<12}  hashes={:<12}  {}",
                result.nonce, result.hashes_tried, hashrate_str(rate));
            println!("  │  hash  = {}...{}", &result.hash[..16], &result.hash[56..]);
            println!("  │  time  = {}ms", result.elapsed_ms);
            println!("  └─ total_blocks={}  avg_hashrate={}",
                self.stats.blocks_mined, hashrate_str(self.stats.avg_hashrate()));
            println!();

            let _ = coinbase; // suppress unused warning
            block_idx += 1;
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn hashrate_str(h: f64) -> String {
    if h >= 1_000_000.0      { format!("{:.2} MH/s", h / 1_000_000.0) }
    else if h >= 1_000.0     { format!("{:.1} KH/s", h / 1_000.0) }
    else                     { format!("{:.0} H/s",  h) }
}

// ── CLI ───────────────────────────────────────────────────────────────────────

pub fn cmd_gpu_mine(addr: &str, diff: usize, blocks: Option<u32>, backend_str: &str) {
    let backend = GpuBackend::from_str(backend_str);
    let mut cfg = GpuMinerConfig::new(addr)
        .with_backend(backend)
        .with_difficulty(diff);
    if let Some(n) = blocks {
        cfg = cfg.with_max_blocks(n);
    }
    let mut miner = GpuMiner::new(cfg);
    miner.run();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_block(index: u64) -> Blake3Block {
        Blake3Block::new(
            index,
            1_700_000_000,
            format!("{:064x}", index),
            format!("{:064x}", index),
            "0".repeat(64),
            HASH_VERSION_BLAKE3,
        )
    }

    #[test]
    fn test_gpu_backend_from_str() {
        assert_eq!(GpuBackend::from_str("software"), GpuBackend::Software);
        assert_eq!(GpuBackend::from_str("opencl"),   GpuBackend::OpenCL);
        assert_eq!(GpuBackend::from_str("cuda"),     GpuBackend::Cuda);
        assert_eq!(GpuBackend::from_str("unknown"),  GpuBackend::Software);
    }

    #[test]
    fn test_backend_availability() {
        assert!(GpuBackend::Software.is_available());
        assert!(!GpuBackend::OpenCL.is_available());
        assert!(!GpuBackend::Cuda.is_available());
    }

    #[test]
    fn test_detect_devices_returns_three() {
        let devs = detect_devices();
        assert_eq!(devs.len(), 3);
        assert_eq!(devs[0].backend, GpuBackend::Software);
        assert!(devs[0].available);
        assert!(!devs[1].available); // OpenCL stub
        assert!(!devs[2].available); // CUDA stub
    }

    #[test]
    fn test_default_compute_units_positive() {
        assert!(default_compute_units() >= 1);
    }

    #[test]
    fn test_mine_software_finds_hash() {
        let block  = test_block(1);
        let result = mine_software(&block, 1, 1);
        assert!(result.hash.starts_with('0'), "hash should meet diff=1");
        assert!(result.hashes_tried >= 1);
        assert!(result.elapsed_ms < 60_000, "should finish in < 60s");
    }

    #[test]
    fn test_gpu_miner_software_mine_block() {
        let cfg    = GpuMinerConfig::new("aabbccddee112233445566778899aabbccddee11")
            .with_difficulty(1)
            .with_compute_units(1);
        let mut miner = GpuMiner::new(cfg);
        let block  = test_block(1);
        let result = miner.mine_block(&block);
        assert!(result.hash.starts_with('0'));
        assert_eq!(miner.stats.blocks_mined, 1);
    }

    #[test]
    fn test_gpu_miner_stats_accumulate() {
        let cfg = GpuMinerConfig::new("aabbccddee112233445566778899aabbccddee11")
            .with_difficulty(1)
            .with_compute_units(1);
        let mut miner = GpuMiner::new(cfg);
        for i in 1..=3u64 {
            let b = test_block(i);
            miner.mine_block(&b);
        }
        assert_eq!(miner.stats.blocks_mined, 3);
        assert!(miner.stats.total_hashes >= 3);
    }

    #[test]
    fn test_gpu_miner_opencl_fallback() {
        let cfg = GpuMinerConfig::new("aabbccddee112233445566778899aabbccddee11")
            .with_backend(GpuBackend::OpenCL)
            .with_difficulty(1)
            .with_compute_units(1);
        let mut miner = GpuMiner::new(cfg);
        let block  = test_block(1);
        let result = miner.mine_block(&block);
        // OpenCL stub falls back to software — still finds valid hash
        assert!(result.hash.starts_with('0'));
    }

    #[test]
    fn test_gpu_miner_cuda_fallback() {
        let cfg = GpuMinerConfig::new("aabbccddee112233445566778899aabbccddee11")
            .with_backend(GpuBackend::Cuda)
            .with_difficulty(1)
            .with_compute_units(1);
        let mut miner = GpuMiner::new(cfg);
        let result = miner.mine_block(&test_block(1));
        assert!(result.hash.starts_with('0'));
    }

    #[test]
    fn test_gpu_mine_result_backend_field() {
        let block  = test_block(1);
        let result = mine_software(&block, 1, 1);
        assert_eq!(result.backend_used, GpuBackend::Software);
    }

    #[test]
    fn test_hashrate_str() {
        assert!(hashrate_str(500.0).contains("H/s"));
        assert!(hashrate_str(5_000.0).contains("KH/s"));
        assert!(hashrate_str(5_000_000.0).contains("MH/s"));
    }

    #[test]
    fn test_gpu_miner_stats_avg_hashrate() {
        let mut stats = GpuMinerStats::new();
        stats.total_hashes  = 10_000;
        stats.total_time_ms = 1_000;
        let rate = stats.avg_hashrate();
        assert!((rate - 10_000.0).abs() < 1.0);
    }

    #[test]
    fn test_gpu_miner_config_builder() {
        let cfg = GpuMinerConfig::new("addr")
            .with_backend(GpuBackend::Cuda)
            .with_difficulty(5)
            .with_max_blocks(10)
            .with_compute_units(4);
        assert_eq!(cfg.backend,       GpuBackend::Cuda);
        assert_eq!(cfg.difficulty,    5);
        assert_eq!(cfg.max_blocks,    Some(10));
        assert_eq!(cfg.compute_units, 4);
    }
}
