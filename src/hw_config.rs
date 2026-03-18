#![allow(dead_code)]
//! v6.9 — Hardware Auto-config
//!
//! Detect CPU/GPU/memory và tự động chọn cấu hình miner tối ưu.
//!
//! API:
//!   HardwareProfile::detect()             → profile hiện tại
//!   OptimalMinerConfig::from_hardware()   → config tối ưu từ hardware
//!   cmd_hw_info()                         → `cargo run -- hw-info`

use crate::cpu_miner::CpuMinerConfig;
use crate::gpu_miner::{GpuMinerConfig, GpuBackend, default_compute_units};

// ─── Hardware Profile ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub logical_cores:  usize,
    pub physical_cores: usize,
    /// Ước tính tier: Low / Mid / High
    pub tier: CpuTier,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CpuTier {
    Low,   // <= 4 logical cores
    Mid,   // 5–11
    High,  // >= 12
}

impl CpuTier {
    fn from_cores(logical: usize) -> Self {
        match logical {
            1..=4  => CpuTier::Low,
            5..=11 => CpuTier::Mid,
            _      => CpuTier::High,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryInfo {
    /// Ước tính từ số cores (không có sys API cross-platform đơn giản)
    pub estimated_gb: u64,
}

#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub backend:       GpuBackend,
    pub compute_units: usize,
    /// Tên mô phỏng (thực tế cần opencl/cuda query)
    pub name:          String,
}

/// Snapshot toàn bộ hardware tại thời điểm detect.
#[derive(Debug, Clone)]
pub struct HardwareProfile {
    pub cpu:    CpuInfo,
    pub memory: MemoryInfo,
    pub gpu:    GpuInfo,
}

impl HardwareProfile {
    /// Detect hardware hiện tại.
    pub fn detect() -> Self {
        let logical  = num_cpus::get().max(1);
        let physical = num_cpus::get_physical().max(1);
        let tier     = CpuTier::from_cores(logical);

        // Estimate memory: 2 GB per physical core, min 4 GB
        let estimated_gb = ((physical * 2) as u64).max(4);

        // GPU: detect best available backend
        #[cfg(feature = "cuda")]
        let (backend, gpu_name) = (GpuBackend::Cuda, "CUDA GPU (detected)".to_string());
        #[cfg(all(feature = "opencl", not(feature = "cuda")))]
        let (backend, gpu_name) = (GpuBackend::OpenCL, "OpenCL GPU (detected)".to_string());
        #[cfg(not(any(feature = "cuda", feature = "opencl")))]
        let (backend, gpu_name) = (GpuBackend::Software, "Software (CPU fallback)".to_string());

        let compute_units = default_compute_units();

        HardwareProfile {
            cpu: CpuInfo { logical_cores: logical, physical_cores: physical, tier },
            memory: MemoryInfo { estimated_gb },
            gpu: GpuInfo { backend, compute_units, name: gpu_name },
        }
    }

    pub fn cpu_tier_name(&self) -> &'static str {
        match self.cpu.tier {
            CpuTier::Low  => "Low-end  (≤4 cores)",
            CpuTier::Mid  => "Mid-range (5–11 cores)",
            CpuTier::High => "High-end (≥12 cores)",
        }
    }
}

// ─── Optimal Miner Config ─────────────────────────────────────────────────────

/// Cấu hình miner được tự động tính từ HardwareProfile.
pub struct OptimalMinerConfig {
    pub cpu:          CpuMinerConfig,
    pub gpu:          GpuMinerConfig,
    pub simd_batch:   bool,           // bật SIMD batch nonce scan (v6.8)
    pub recommended:  MinerStrategy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MinerStrategy {
    /// CPU-only — thường phù hợp PacketCrypt PoW (memory-hard)
    CpuOnly,
    /// GPU Software mode — nhiều compute units
    GpuSoftware,
    /// GPU OpenCL
    GpuOpenCL,
    /// GPU CUDA
    GpuCuda,
}

impl OptimalMinerConfig {
    /// Tự động tính cấu hình tối ưu từ hardware.
    pub fn from_hardware(profile: &HardwareProfile) -> Self {
        let logical = profile.cpu.logical_cores;

        // CPU threads: 1/3 logical cores, min 1, max 32
        let cpu_threads = (logical / 3).max(1).min(32);

        // Difficulty target: giữ cố định theo network
        let difficulty = 4;

        let cpu = CpuMinerConfig::new("").with_threads(cpu_threads).with_difficulty(difficulty);

        let gpu = GpuMinerConfig::new("")
            .with_backend(profile.gpu.backend.clone())
            .with_compute_units(profile.gpu.compute_units);

        // SIMD batch: bật khi có đủ cores (>=4) — scalar cũng đúng, SIMD nhanh hơn
        let simd_batch = logical >= 4;

        // Khuyến nghị strategy
        let recommended = match &profile.gpu.backend {
            GpuBackend::Cuda     => MinerStrategy::GpuCuda,
            GpuBackend::OpenCL   => MinerStrategy::GpuOpenCL,
            GpuBackend::Software => match profile.cpu.tier {
                CpuTier::High => MinerStrategy::GpuSoftware, // nhiều cores → software GPU sim
                _             => MinerStrategy::CpuOnly,
            },
        };

        OptimalMinerConfig { cpu, gpu, simd_batch, recommended }
    }

    pub fn strategy_name(&self) -> &'static str {
        match self.recommended {
            MinerStrategy::CpuOnly     => "CPU-only (PacketCrypt optimized)",
            MinerStrategy::GpuSoftware => "GPU Software (multi-core sim)",
            MinerStrategy::GpuOpenCL   => "GPU OpenCL",
            MinerStrategy::GpuCuda     => "GPU CUDA",
        }
    }
}

// ─── CLI ──────────────────────────────────────────────────────────────────────

pub fn cmd_hw_info() {
    let profile = HardwareProfile::detect();
    let optimal  = OptimalMinerConfig::from_hardware(&profile);

    println!();
    println!("  ╔══════════════════════════════════════════════════╗");
    println!("  ║           PKT Hardware Profile                   ║");
    println!("  ╚══════════════════════════════════════════════════╝");
    println!();
    println!("  ── CPU ──────────────────────────────────────────");
    println!("  Logical cores  : {}", profile.cpu.logical_cores);
    println!("  Physical cores : {}", profile.cpu.physical_cores);
    println!("  Tier           : {}", profile.cpu_tier_name());
    println!();
    println!("  ── Memory ───────────────────────────────────────");
    println!("  Estimated      : ~{} GB", profile.memory.estimated_gb);
    println!();
    println!("  ── GPU / Compute ─────────────────────────────────");
    println!("  Backend        : {:?}", profile.gpu.backend);
    println!("  Device         : {}", profile.gpu.name);
    println!("  Compute units  : {}", profile.gpu.compute_units);
    println!();
    println!("  ── Recommended Miner Config ──────────────────────");
    println!("  Strategy       : {}", optimal.strategy_name());
    println!("  CPU threads    : {}", optimal.cpu.threads);
    println!("  SIMD batch     : {}", if optimal.simd_batch { "enabled" } else { "disabled" });
    println!("  Difficulty     : {}", optimal.cpu.difficulty);
    println!();
    println!("  ── Quick Start ───────────────────────────────────");
    println!("  cargo run -- cpumine <addr>");
    if optimal.recommended != MinerStrategy::CpuOnly {
        println!("  cargo run -- gpumine <addr>");
    }
    println!();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_returns_valid_profile() {
        let p = HardwareProfile::detect();
        assert!(p.cpu.logical_cores >= 1);
        assert!(p.cpu.physical_cores >= 1);
        assert!(p.cpu.logical_cores >= p.cpu.physical_cores);
        assert!(p.memory.estimated_gb >= 4);
        assert!(p.gpu.compute_units >= 1);
    }

    #[test]
    fn test_cpu_tier_low() {
        assert_eq!(CpuTier::from_cores(1), CpuTier::Low);
        assert_eq!(CpuTier::from_cores(4), CpuTier::Low);
    }

    #[test]
    fn test_cpu_tier_mid() {
        assert_eq!(CpuTier::from_cores(5),  CpuTier::Mid);
        assert_eq!(CpuTier::from_cores(11), CpuTier::Mid);
    }

    #[test]
    fn test_cpu_tier_high() {
        assert_eq!(CpuTier::from_cores(12), CpuTier::High);
        assert_eq!(CpuTier::from_cores(64), CpuTier::High);
    }

    #[test]
    fn test_optimal_config_threads_bounded() {
        let p = HardwareProfile::detect();
        let opt = OptimalMinerConfig::from_hardware(&p);
        assert!(opt.cpu.threads >= 1);
        assert!(opt.cpu.threads <= 32);
    }

    #[test]
    fn test_optimal_config_simd_on_multicore() {
        let p = HardwareProfile {
            cpu:    CpuInfo { logical_cores: 8, physical_cores: 4, tier: CpuTier::Mid },
            memory: MemoryInfo { estimated_gb: 16 },
            gpu:    GpuInfo { backend: GpuBackend::Software, compute_units: 2, name: "test".into() },
        };
        let opt = OptimalMinerConfig::from_hardware(&p);
        assert!(opt.simd_batch, "SIMD phải bật khi >= 4 cores");
    }

    #[test]
    fn test_optimal_config_simd_off_low_core() {
        let p = HardwareProfile {
            cpu:    CpuInfo { logical_cores: 2, physical_cores: 1, tier: CpuTier::Low },
            memory: MemoryInfo { estimated_gb: 4 },
            gpu:    GpuInfo { backend: GpuBackend::Software, compute_units: 1, name: "test".into() },
        };
        let opt = OptimalMinerConfig::from_hardware(&p);
        assert!(!opt.simd_batch, "SIMD phải tắt khi < 4 cores");
    }

    #[test]
    fn test_strategy_cpu_only_on_software_mid() {
        let p = HardwareProfile {
            cpu:    CpuInfo { logical_cores: 6, physical_cores: 3, tier: CpuTier::Mid },
            memory: MemoryInfo { estimated_gb: 8 },
            gpu:    GpuInfo { backend: GpuBackend::Software, compute_units: 2, name: "test".into() },
        };
        let opt = OptimalMinerConfig::from_hardware(&p);
        assert_eq!(opt.recommended, MinerStrategy::CpuOnly);
    }

    #[test]
    fn test_strategy_gpu_software_on_high_core() {
        let p = HardwareProfile {
            cpu:    CpuInfo { logical_cores: 16, physical_cores: 8, tier: CpuTier::High },
            memory: MemoryInfo { estimated_gb: 32 },
            gpu:    GpuInfo { backend: GpuBackend::Software, compute_units: 5, name: "test".into() },
        };
        let opt = OptimalMinerConfig::from_hardware(&p);
        assert_eq!(opt.recommended, MinerStrategy::GpuSoftware);
    }

    #[test]
    fn test_strategy_cuda() {
        let p = HardwareProfile {
            cpu:    CpuInfo { logical_cores: 8, physical_cores: 4, tier: CpuTier::Mid },
            memory: MemoryInfo { estimated_gb: 16 },
            gpu:    GpuInfo { backend: GpuBackend::Cuda, compute_units: 64, name: "RTX".into() },
        };
        let opt = OptimalMinerConfig::from_hardware(&p);
        assert_eq!(opt.recommended, MinerStrategy::GpuCuda);
    }

    #[test]
    fn test_strategy_opencl() {
        let p = HardwareProfile {
            cpu:    CpuInfo { logical_cores: 8, physical_cores: 4, tier: CpuTier::Mid },
            memory: MemoryInfo { estimated_gb: 16 },
            gpu:    GpuInfo { backend: GpuBackend::OpenCL, compute_units: 32, name: "RX580".into() },
        };
        let opt = OptimalMinerConfig::from_hardware(&p);
        assert_eq!(opt.recommended, MinerStrategy::GpuOpenCL);
    }

    #[test]
    fn test_memory_estimate_min_4gb() {
        let p = HardwareProfile::detect();
        assert!(p.memory.estimated_gb >= 4, "min 4 GB estimate");
    }

    #[test]
    fn test_strategy_name_non_empty() {
        let p = HardwareProfile::detect();
        let opt = OptimalMinerConfig::from_hardware(&p);
        assert!(!opt.strategy_name().is_empty());
    }
}
