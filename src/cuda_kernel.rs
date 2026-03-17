#![allow(dead_code)]
//! v6.6 — CUDA BLAKE3 Kernel
//!
//! BLAKE3 PoW mining kernel viết bằng CUDA C.
//! Feature-gated: `cargo build --features cuda`
//! CPU rayon fallback khi CUDA không có hoặc không có nvcc/GPU.
//!
//! CUDA kernel details:
//!   - BLAKE3 7-round compression, MSG_SCHEDULE theo spec
//!   - Mỗi thread thử một nonce: nonce = nonce_start + blockIdx.x*blockDim.x + threadIdx.x
//!   - Tìm nonce đầu tiên có `difficulty` leading-zero hex nibbles
//!   - Output: found_nonce (ULONG_MAX nếu không tìm được trong batch)
//!   - Kernel cần compile bằng `nvcc` thành PTX trước khi nhúng
//!
//! API:
//!   BLAKE3_CUDA_SRC         — CUDA C kernel source (&'static str)
//!   CudaConfig              — block_size, grid_size, batch_size
//!   cuda_available()        — true nếu feature cuda được enable
//!   cuda_mine(block, difficulty, config) -> GpuMineResult
//!   list_cuda_devices()     — Vec<CudaDeviceInfo> (#[cfg(feature="cuda")])
//!
//! Compile với CUDA:
//!   cargo build --features cuda
//!   cargo run  --features cuda -- gpumine [addr] [diff] [n] cuda
//!
//! Note: GPU execution yêu cầu nvcc-compiled PTX + NVIDIA GPU.
//! Nếu không có GPU, tự động fallback về CPU rayon.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use rayon::prelude::*;

use crate::blake3_hash::{Blake3Block, pow_hash, to_hex};
use crate::gpu_miner::GpuMineResult;

// ── CUDA C Kernel source ──────────────────────────────────────────────────────

/// BLAKE3 PoW mining kernel — CUDA C source.
///
/// Implements single-chunk BLAKE3 for headers up to 256 bytes (4 × 64-byte blocks).
/// Each thread tries one nonce: full_header = prefix || decimal(nonce).
/// Outputs the first valid nonce into `found_nonce` via 64-bit atomicCAS.
///
/// Compile to PTX: `nvcc -ptx -arch=sm_75 -o blake3_mine.ptx blake3_mine.cu`
/// Reference: BLAKE3 specification v1.0 — https://github.com/BLAKE3-team/BLAKE3-specs
pub const BLAKE3_CUDA_SRC: &str = r#"
/* ── BLAKE3 CUDA Kernel — blockchain-rust v6.6 ───────────────────────────────
   Handles inputs up to 256 bytes (4 blocks of 64 bytes each).
   Each thread hashes one (prefix || nonce_decimal) string.
   ──────────────────────────────────────────────────────────────────────────── */

#include <stdint.h>

#define CHUNK_START 1u
#define CHUNK_END   2u
#define ROOT        8u

/* BLAKE3 IV = SHA-256 initial hash values */
#define IV0 0x6A09E667u
#define IV1 0xBB67AE85u
#define IV2 0x3C6EF372u
#define IV3 0xA54FF53Au
#define IV4 0x510E527Fu
#define IV5 0x9B05688Cu
#define IV6 0x1F83D9ABu
#define IV7 0x5BE0CD19u

__device__ static inline uint32_t rot32(uint32_t x, uint32_t n) {
    return (x >> n) | (x << (32u - n));
}

/* BLAKE3 G mixing function */
__device__ static inline void G_fn(uint32_t* s, uint32_t a, uint32_t b,
                                    uint32_t c, uint32_t d, uint32_t x, uint32_t y) {
    s[a] += s[b] + x; s[d] = rot32(s[d]^s[a], 16u);
    s[c] += s[d];     s[b] = rot32(s[b]^s[c], 12u);
    s[a] += s[b] + y; s[d] = rot32(s[d]^s[a],  8u);
    s[c] += s[d];     s[b] = rot32(s[b]^s[c],  7u);
}

/* BLAKE3 compression — 7 rounds, MSG_SCHEDULE from spec */
__device__ static void blake3_compress(
    uint32_t* cv,     /* 8 words: chaining value, modified in-place */
    uint32_t* m,      /* 16 words: message block */
    uint64_t  counter,
    uint32_t  blen,
    uint32_t  flags
) {
    uint32_t s[16];
    s[0]=cv[0]; s[1]=cv[1]; s[2]=cv[2]; s[3]=cv[3];
    s[4]=cv[4]; s[5]=cv[5]; s[6]=cv[6]; s[7]=cv[7];
    s[8]=IV0; s[9]=IV1; s[10]=IV2; s[11]=IV3;
    s[12]=(uint32_t)counter; s[13]=(uint32_t)(counter>>32);
    s[14]=blen; s[15]=flags;

    /* MSG_SCHEDULE — identical to BLAKE3 spec Table 2 */
    const uint32_t schedule[7][16] = {
        {0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15},
        {2,6,3,10,7,0,4,13,1,11,12,5,9,14,15,8},
        {3,4,10,12,13,2,7,14,6,5,9,0,11,15,8,1},
        {10,7,12,9,14,3,13,15,4,0,11,2,5,8,1,6},
        {12,13,9,11,15,10,14,8,7,2,5,3,0,1,6,4},
        {9,14,11,5,8,12,15,1,13,3,0,10,2,6,4,7},
        {11,15,5,0,1,9,8,6,14,10,2,12,3,4,7,13}
    };

#pragma unroll
    for (int r = 0; r < 7; r++) {
        const uint32_t* sc = schedule[r];
        G_fn(s,0,4,8, 12, m[sc[0]], m[sc[1]]);
        G_fn(s,1,5,9, 13, m[sc[2]], m[sc[3]]);
        G_fn(s,2,6,10,14, m[sc[4]], m[sc[5]]);
        G_fn(s,3,7,11,15, m[sc[6]], m[sc[7]]);
        G_fn(s,0,5,10,15, m[sc[8]], m[sc[9]]);
        G_fn(s,1,6,11,12, m[sc[10]],m[sc[11]]);
        G_fn(s,2,7,8, 13, m[sc[12]],m[sc[13]]);
        G_fn(s,3,4,9, 14, m[sc[14]],m[sc[15]]);
    }
    for (int i = 0; i < 8; i++) cv[i] = s[i] ^ s[i+8];
}

/* Write decimal representation of nonce into buf, return length */
__device__ static int u64_to_dec(uint64_t v, char* buf) {
    if (v == 0) { buf[0]='0'; return 1; }
    char tmp[20]; int len=0;
    while (v > 0) { tmp[len++] = '0' + (int)(v % 10); v /= 10; }
    for (int i=0; i<len; i++) buf[i] = tmp[len-1-i];
    return len;
}

/* Load up to 64 bytes from data[] into m[16] uint32, little-endian, zero-padded */
__device__ static void load_block_words(
    const char* data, int off, int total_len, uint32_t* m
) {
    for (int w = 0; w < 16; w++) {
        uint32_t word = 0;
        for (int b = 0; b < 4; b++) {
            int idx = off + w*4 + b;
            uint8_t byte = (idx < total_len) ? (uint8_t)data[idx] : 0;
            word |= ((uint32_t)byte) << (b*8);
        }
        m[w] = word;
    }
}

/* Hex nibble from a 4-bit value */
__device__ static inline char nibble_to_hex(uint32_t v) {
    return v < 10 ? ('0' + v) : ('a' + v - 10);
}

/* BLAKE3 mine kernel:
   Each thread builds header = prefix[0..prefix_len] || decimal(nonce_start + gid)
   and checks if the leading `difficulty` hex nibbles are all '0'.
   First winner stores its nonce into found_nonce via 64-bit atomicCAS. */
extern "C" __global__ void blake3_mine(
    const char*          prefix,
    uint32_t             prefix_len,
    unsigned long long   nonce_start,
    uint32_t             difficulty,
    unsigned long long*  found_nonce  /* ULLONG_MAX if none found */
) {
    uint64_t gid   = (uint64_t)blockIdx.x * blockDim.x + threadIdx.x;
    uint64_t nonce = nonce_start + gid;

    /* Build full header into local buffer (max 256 bytes) */
    char hdr[256];
    int  hlen = 0;
    for (uint32_t i = 0; i < prefix_len && hlen < 240; i++)
        hdr[hlen++] = prefix[i];
    hlen += u64_to_dec(nonce, hdr + hlen);

    /* ── BLAKE3 hash ────────────────────────────────────────────── */
    int nblocks = (hlen + 63) / 64;

    /* IV chaining value */
    uint32_t cv[8] = {IV0,IV1,IV2,IV3,IV4,IV5,IV6,IV7};
    uint32_t m[16];

    for (int b = 0; b < nblocks; b++) {
        int   off  = b * 64;
        int   blen = hlen - off;
        if (blen > 64) blen = 64;

        uint32_t flags = 0;
        if (b == 0)           flags |= CHUNK_START;
        if (b == nblocks - 1) flags |= CHUNK_END | ROOT;

        load_block_words(hdr, off, hlen, m);
        blake3_compress(cv, m, 0ULL, (uint32_t)blen, flags);
    }

    /* cv[0] holds MSW of hash. Check difficulty nibbles from MSB. */
    uint32_t zeros_needed = difficulty;
    uint32_t zeros_found  = 0;
    int      done         = 0;

    for (int w = 0; w < 8 && !done; w++) {
        /* Each word = 4 bytes = 8 hex nibbles, big-endian hex order */
        for (int shift = 28; shift >= 0 && !done; shift -= 4) {
            uint32_t nibble = (cv[w] >> shift) & 0xFu;
            if (nibble == 0) {
                zeros_found++;
                if (zeros_found >= zeros_needed) { done = 1; }
            } else {
                done = 2; /* fail */
            }
        }
    }

    if (done == 1) {
        /* Atomic store: only first winner */
        atomicCAS(found_nonce,
                  (unsigned long long)0xFFFFFFFFFFFFFFFFULL,
                  (unsigned long long)nonce);
    }
}
"#;

// ── CUDA config ───────────────────────────────────────────────────────────────

/// Configuration cho CUDA mining.
pub struct CudaConfig {
    /// Threads per block (power of 2, thường 128 hoặc 256).
    pub block_size: u32,
    /// Số blocks trên grid mỗi dispatch.
    pub grid_size:  u32,
    /// Số nonces thử mỗi dispatch = block_size × grid_size.
    pub batch_size: u64,
}

impl Default for CudaConfig {
    fn default() -> Self {
        let block_size: u32 = 256;
        let grid_size:  u32 = 128;
        CudaConfig {
            block_size,
            grid_size,
            batch_size: (block_size as u64) * (grid_size as u64),
        }
    }
}

impl CudaConfig {
    pub fn new(block_size: u32, grid_size: u32) -> Self {
        CudaConfig {
            block_size,
            grid_size,
            batch_size: (block_size as u64) * (grid_size as u64),
        }
    }
}

// ── CUDA device info ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CudaDeviceInfo {
    pub index:          u32,
    pub name:           String,
    pub sm_count:       u32,
    pub sm_version:     u32,
    pub total_mem_mb:   u64,
    pub max_threads_sm: u32,
}

/// Liệt kê các CUDA GPU devices trên hệ thống.
/// Returns empty vec nếu feature `cuda` không được enable hoặc không có NVIDIA GPU.
pub fn list_cuda_devices() -> Vec<CudaDeviceInfo> {
    #[cfg(feature = "cuda")]
    {
        _list_cuda_devices_impl()
    }
    #[cfg(not(feature = "cuda"))]
    {
        vec![]
    }
}

#[cfg(feature = "cuda")]
fn _list_cuda_devices_impl() -> Vec<CudaDeviceInfo> {
    use cust::device::Device;
    if cust::init(cust::CudaFlags::empty()).is_err() {
        return vec![];
    }
    let count = Device::num_devices().unwrap_or(0);
    let mut result = Vec::new();
    for i in 0..count {
        if let Ok(dev) = Device::get_device(i) {
            let name          = dev.name().unwrap_or_else(|_| "unknown".into());
            let sm_count      = dev.multiprocessor_count().unwrap_or(0) as u32;
            let (maj, min)    = dev.compute_capability().unwrap_or((0, 0));
            let sm_version    = maj * 10 + min;
            let total_mem_mb  = dev.total_memory().unwrap_or(0) / 1_000_000;
            let max_threads_sm = dev.max_threads_per_multiprocessor().unwrap_or(0) as u32;
            result.push(CudaDeviceInfo {
                index: i,
                name,
                sm_count,
                sm_version,
                total_mem_mb,
                max_threads_sm,
            });
        }
    }
    result
}

// ── cuda_available() ──────────────────────────────────────────────────────────

/// Returns true nếu feature `cuda` được compile vào.
pub const fn cuda_available() -> bool {
    #[cfg(feature = "cuda")] { true }
    #[cfg(not(feature = "cuda"))] { false }
}

// ── Public mine function ──────────────────────────────────────────────────────

/// Mine một Blake3Block dùng CUDA nếu available, ngược lại dùng rayon CPU.
///
/// GPU yêu cầu:
///   1. Compile với `--features cuda`
///   2. NVIDIA GPU + CUDA driver
///   3. PTX kernel (build/blake3_mine.ptx) được compile bởi nvcc
///
/// Nếu thiếu bất kỳ điều kiện nào, tự động fallback về CPU rayon.
pub fn cuda_mine(block: &Blake3Block, difficulty: usize, config: &CudaConfig) -> GpuMineResult {
    #[cfg(feature = "cuda")]
    {
        match _mine_cuda_impl(block, difficulty, config) {
            Ok(r)  => return r,
            Err(e) => eprintln!("  [cuda] GPU error: {} — falling back to CPU rayon", e),
        }
    }
    let threads = (config.block_size as usize).min(num_cpus::get()).max(1);
    _mine_cpu_fallback(block, difficulty, threads)
}

// ── CUDA implementation (feature-gated) ───────────────────────────────────────

/// Compiled PTX placeholder.
/// Real PTX: `nvcc -ptx -arch=sm_75 blake3_mine.cu -o blake3_mine.ptx`
/// Replace this string with the actual PTX output for real GPU execution.
#[cfg(feature = "cuda")]
const BLAKE3_PTX_PLACEHOLDER: &str = "// placeholder — replace with nvcc output\n";

#[cfg(feature = "cuda")]
fn _mine_cuda_impl(
    block:      &Blake3Block,
    difficulty: usize,
    config:     &CudaConfig,
) -> Result<GpuMineResult, String> {
    use cust::prelude::*;

    let t0     = Instant::now();
    let target = "0".repeat(difficulty);

    // Init CUDA runtime
    cust::init(CudaFlags::empty()).map_err(|e| format!("CUDA init: {}", e))?;
    let device  = Device::get_device(0).map_err(|e| format!("CUDA device: {}", e))?;
    let _ctx    = Context::new(device).map_err(|e| format!("CUDA context: {}", e))?;
    let stream  = Stream::new(StreamFlags::DEFAULT, None)
        .map_err(|e| format!("CUDA stream: {}", e))?;

    // Load PTX — try build output first, fall back to placeholder
    let ptx_path = concat!(env!("CARGO_MANIFEST_DIR"), "/target/blake3_mine.ptx");
    let ptx_src  = std::fs::read_to_string(ptx_path)
        .unwrap_or_else(|_| BLAKE3_PTX_PLACEHOLDER.to_string());
    let module   = Module::from_ptx(&ptx_src, &[])
        .map_err(|e| format!("PTX load: {}", e))?;
    let func     = module.get_function("blake3_mine")
        .map_err(|e| format!("CUDA function: {}", e))?;

    let prefix = format!(
        "{}|{}|{}|{}|{}|",
        block.index, block.timestamp,
        block.txid_root, block.witness_root, block.prev_hash,
    );
    let prefix_bytes = prefix.as_bytes();
    let sentinel: u64 = u64::MAX;

    // Allocate GPU buffers
    let prefix_gpu = DeviceBuffer::from_slice(prefix_bytes)
        .map_err(|e| format!("CUDA alloc prefix: {}", e))?;
    let mut found_gpu = DeviceBuffer::from_slice(&[sentinel])
        .map_err(|e| format!("CUDA alloc found: {}", e))?;

    let batch    = config.batch_size;
    let bsz      = config.block_size;
    let gsz      = config.grid_size;
    let mut nonce = 0u64;
    let mut total = 0u64;

    loop {
        // Reset found sentinel
        found_gpu.copy_from(&[sentinel]).map_err(|e| format!("CUDA copy: {}", e))?;

        unsafe {
            launch!(func<<<gsz, bsz, 0, stream>>>(
                prefix_gpu.as_device_ptr(),
                prefix_bytes.len() as u32,
                nonce,
                difficulty as u32,
                found_gpu.as_device_ptr()
            )).map_err(|e| format!("CUDA launch: {}", e))?;
        }
        stream.synchronize().map_err(|e| format!("CUDA sync: {}", e))?;

        let mut result = [sentinel];
        found_gpu.copy_to(&mut result).map_err(|e| format!("CUDA readback: {}", e))?;
        total += batch;

        if result[0] != sentinel {
            let winning_nonce = result[0];
            let header = format!("{}{}", prefix, winning_nonce);
            let hash   = hex::encode(blake3::hash(header.as_bytes()).as_bytes());
            if hash.starts_with(&target) {
                return Ok(GpuMineResult {
                    nonce:        winning_nonce,
                    hash,
                    hashes_tried: total,
                    elapsed_ms:   t0.elapsed().as_millis() as u64,
                    backend_used: crate::gpu_miner::GpuBackend::Cuda,
                });
            }
        }

        nonce = nonce.saturating_add(batch);
        if nonce == u64::MAX { break; }
    }

    Err("Nonce space exhausted without finding CUDA solution".into())
}

// ── CPU fallback ──────────────────────────────────────────────────────────────

fn _mine_cpu_fallback(block: &Blake3Block, difficulty: usize, threads: usize) -> GpuMineResult {
    let t0           = Instant::now();
    let stop         = Arc::new(AtomicBool::new(false));
    let total_hashes = Arc::new(AtomicU64::new(0));
    let target       = "0".repeat(difficulty);
    let n            = threads.max(1);
    let chunk        = u64::MAX / n as u64;
    let hv           = block.hash_version;

    let prefix = format!(
        "{}|{}|{}|{}|{}|",
        block.index, block.timestamp,
        block.txid_root, block.witness_root, block.prev_hash,
    );

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .build()
        .expect("cuda_kernel: build thread pool");

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
                let header = format!("{}{}", prefix, nonce);
                let raw    = pow_hash(header.as_bytes(), hv);
                local     += 1;
                if local % 50_000 == 0 {
                    total_hashes.fetch_add(50_000, Ordering::Relaxed);
                    local = 0;
                }
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
    GpuMineResult {
        nonce,
        hash,
        hashes_tried: total_hashes.load(Ordering::Relaxed),
        elapsed_ms:   t0.elapsed().as_millis() as u64,
        backend_used: crate::gpu_miner::GpuBackend::Software,
    }
}

// ── CLI helper ────────────────────────────────────────────────────────────────

pub fn cmd_cuda_info() {
    println!();
    println!("  CUDA Status  : {}", if cuda_available() { "✅ compiled" } else { "⚠️  not compiled (use --features cuda)" });
    let devs = list_cuda_devices();
    if devs.is_empty() {
        println!("  Devices      : none detected (NVIDIA GPU + driver required)");
    } else {
        for d in &devs {
            println!("  Device #{}: {} — SM{}  {} SMs  max_threads/SM={}  {}MB",
                d.index, d.name, d.sm_version, d.sm_count, d.max_threads_sm, d.total_mem_mb);
        }
    }
    println!("  PTX note     : compile blake3_mine.cu with nvcc to enable GPU execution");
    println!();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blake3_hash::HASH_VERSION_BLAKE3;

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
    fn test_kernel_source_not_empty() {
        assert!(!BLAKE3_CUDA_SRC.is_empty());
        assert!(BLAKE3_CUDA_SRC.contains("blake3_mine"));
        assert!(BLAKE3_CUDA_SRC.contains("blake3_compress"));
    }

    #[test]
    fn test_kernel_contains_g_function() {
        assert!(BLAKE3_CUDA_SRC.contains("G_fn"));
        assert!(BLAKE3_CUDA_SRC.contains("rot32"));
    }

    #[test]
    fn test_kernel_contains_msg_schedule() {
        assert!(BLAKE3_CUDA_SRC.contains("schedule"));
        assert!(BLAKE3_CUDA_SRC.contains("CHUNK_START"));
        assert!(BLAKE3_CUDA_SRC.contains("CHUNK_END"));
        assert!(BLAKE3_CUDA_SRC.contains("ROOT"));
    }

    #[test]
    fn test_kernel_has_atomic_cas() {
        assert!(BLAKE3_CUDA_SRC.contains("atomicCAS"));
    }

    #[test]
    fn test_kernel_has_global_qualifier() {
        assert!(BLAKE3_CUDA_SRC.contains("__global__"));
        assert!(BLAKE3_CUDA_SRC.contains("__device__"));
    }

    #[test]
    fn test_kernel_uses_grid_stride() {
        assert!(BLAKE3_CUDA_SRC.contains("blockIdx.x"));
        assert!(BLAKE3_CUDA_SRC.contains("threadIdx.x"));
        assert!(BLAKE3_CUDA_SRC.contains("blockDim.x"));
    }

    #[test]
    fn test_cuda_config_default() {
        let c = CudaConfig::default();
        assert!(c.block_size >= 64);
        assert!(c.grid_size  >= 1);
        assert_eq!(c.batch_size, c.block_size as u64 * c.grid_size as u64);
    }

    #[test]
    fn test_cuda_config_new() {
        let c = CudaConfig::new(128, 64);
        assert_eq!(c.block_size, 128);
        assert_eq!(c.grid_size,  64);
        assert_eq!(c.batch_size, 8192);
    }

    #[test]
    fn test_cuda_available_const() {
        // Must compile without error
        let _ = cuda_available();
    }

    #[test]
    fn test_list_cuda_devices_no_panic() {
        let devs = list_cuda_devices();
        // Empty on CI (no GPU) — just must not panic
        let _ = devs;
    }

    #[test]
    fn test_cuda_mine_cpu_fallback_diff1() {
        // No NVIDIA GPU in CI — always uses CPU fallback
        let block = test_block(1);
        let cfg   = CudaConfig::new(4, 4); // block_size=4 → 4 CPU threads fallback
        let r     = cuda_mine(&block, 1, &cfg);
        assert!(r.hash.starts_with('0'), "diff=1 hash must start with 0");
        assert!(r.hashes_tried > 0);
        assert!(r.hashes_tried >= 1); // elapsed_ms may be 0 on fast machines
    }

    #[test]
    fn test_cuda_mine_result_hash_valid() {
        let block = test_block(2);
        let cfg   = CudaConfig::default();
        let r     = cuda_mine(&block, 1, &cfg);
        assert_eq!(r.hash.len(), 64, "BLAKE3 hex = 64 chars");
    }
}
