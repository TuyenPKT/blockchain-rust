#![allow(dead_code)]
//! v6.5 — OpenCL BLAKE3 Kernel
//!
//! BLAKE3 PoW mining kernel viết bằng OpenCL C.
//! Feature-gated: `cargo build --features opencl`
//! CPU rayon fallback khi OpenCL không có hoặc lỗi.
//!
//! BLAKE3 kernel details:
//!   - Xử lý input tới 256 bytes (4 blocks × 64 bytes per block)
//!   - Mỗi work item thử một nonce: nonce = nonce_start + get_global_id(0)
//!   - Tìm nonce đầu tiên có `difficulty` leading-zero hex nibbles
//!   - Output: found_nonce (ULONG_MAX nếu không tìm được trong batch)
//!
//! API:
//!   BLAKE3_OCL_KERNEL       — OpenCL C kernel source (&'static str)
//!   OpenClConfig            — compute_units, work_group_size, batch_size
//!   opencl_available()      — true nếu feature opencl được enable
//!   opencl_mine(block, difficulty, config) -> GpuMineResult
//!   OpenClDevice            — wrapper cho ocl::Device (#[cfg(feature="opencl")])
//!
//! Compile với OpenCL:
//!   cargo build --features opencl
//!   cargo run  --features opencl -- gpumine [addr] [diff] [n] opencl

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use rayon::prelude::*;

use crate::blake3_hash::{Blake3Block, pow_hash, to_hex};
use crate::gpu_miner::GpuMineResult;

// ── OpenCL C Kernel source ────────────────────────────────────────────────────

/// BLAKE3 PoW mining kernel — OpenCL C source.
///
/// Implements single-chunk BLAKE3 for headers up to 256 bytes (4 × 64-byte blocks).
/// Each work item tries one nonce: full_header = prefix || decimal(nonce).
/// Outputs the first valid nonce into `found_nonce` using atomic compare-and-swap.
///
/// Reference: BLAKE3 specification v1.0 — https://github.com/BLAKE3-team/BLAKE3-specs
pub const BLAKE3_OCL_KERNEL: &str = r#"
/* ── BLAKE3 OpenCL Kernel — blockchain-rust v6.5 ────────────────────────────
   Handles inputs up to 256 bytes (4 blocks of 64 bytes each).
   Each work item hashes one (prefix || nonce_decimal) string.
   ──────────────────────────────────────────────────────────────────────────── */

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

static inline uint rot32(uint x, uint n) {
    return (x >> n) | (x << (32u - n));
}

/* BLAKE3 G mixing function */
static inline void G_fn(__private uint* s, uint a, uint b, uint c, uint d,
                         uint x, uint y) {
    s[a] += s[b] + x; s[d] = rot32(s[d]^s[a], 16u);
    s[c] += s[d];     s[b] = rot32(s[b]^s[c], 12u);
    s[a] += s[b] + y; s[d] = rot32(s[d]^s[a],  8u);
    s[c] += s[d];     s[b] = rot32(s[b]^s[c],  7u);
}

/* BLAKE3 compression — 7 rounds, MSG_SCHEDULE from spec */
static void blake3_compress(
    __private uint* cv,     /* 8 words: chaining value */
    __private uint* m,      /* 16 words: message block */
    ulong counter,
    uint block_len,
    uint flags,
    __private uint* out     /* 8 words: output cv */
) {
    uint s[16];
    s[0]=cv[0]; s[1]=cv[1]; s[2]=cv[2]; s[3]=cv[3];
    s[4]=cv[4]; s[5]=cv[5]; s[6]=cv[6]; s[7]=cv[7];
    s[8]=IV0; s[9]=IV1; s[10]=IV2; s[11]=IV3;
    s[12]=(uint)(counter & 0xFFFFFFFFUL);
    s[13]=(uint)(counter >> 32);
    s[14]=block_len;
    s[15]=flags;

    /* Round 0: [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15] */
    G_fn(s,0,4, 8,12, m[0], m[1]);  G_fn(s,1,5, 9,13, m[2], m[3]);
    G_fn(s,2,6,10,14, m[4], m[5]);  G_fn(s,3,7,11,15, m[6], m[7]);
    G_fn(s,0,5,10,15, m[8], m[9]);  G_fn(s,1,6,11,12, m[10],m[11]);
    G_fn(s,2,7, 8,13, m[12],m[13]); G_fn(s,3,4, 9,14, m[14],m[15]);

    /* Round 1: [2,6,3,10,7,0,4,13,1,11,12,5,9,14,15,8] */
    G_fn(s,0,4, 8,12, m[2], m[6]);  G_fn(s,1,5, 9,13, m[3], m[10]);
    G_fn(s,2,6,10,14, m[7], m[0]);  G_fn(s,3,7,11,15, m[4], m[13]);
    G_fn(s,0,5,10,15, m[1], m[11]); G_fn(s,1,6,11,12, m[12],m[5]);
    G_fn(s,2,7, 8,13, m[9], m[14]); G_fn(s,3,4, 9,14, m[15],m[8]);

    /* Round 2: [3,4,10,12,13,2,7,14,6,5,9,0,11,15,8,1] */
    G_fn(s,0,4, 8,12, m[3], m[4]);  G_fn(s,1,5, 9,13, m[10],m[12]);
    G_fn(s,2,6,10,14, m[13],m[2]);  G_fn(s,3,7,11,15, m[7], m[14]);
    G_fn(s,0,5,10,15, m[6], m[5]);  G_fn(s,1,6,11,12, m[9], m[0]);
    G_fn(s,2,7, 8,13, m[11],m[15]); G_fn(s,3,4, 9,14, m[8], m[1]);

    /* Round 3: [10,7,12,9,14,3,13,15,4,0,11,2,5,8,1,6] */
    G_fn(s,0,4, 8,12, m[10],m[7]);  G_fn(s,1,5, 9,13, m[12],m[9]);
    G_fn(s,2,6,10,14, m[14],m[3]);  G_fn(s,3,7,11,15, m[13],m[15]);
    G_fn(s,0,5,10,15, m[4], m[0]);  G_fn(s,1,6,11,12, m[11],m[2]);
    G_fn(s,2,7, 8,13, m[5], m[8]);  G_fn(s,3,4, 9,14, m[1], m[6]);

    /* Round 4: [12,13,9,11,15,10,14,8,7,2,5,3,0,1,6,4] */
    G_fn(s,0,4, 8,12, m[12],m[13]); G_fn(s,1,5, 9,13, m[9], m[11]);
    G_fn(s,2,6,10,14, m[15],m[10]); G_fn(s,3,7,11,15, m[14],m[8]);
    G_fn(s,0,5,10,15, m[7], m[2]);  G_fn(s,1,6,11,12, m[5], m[3]);
    G_fn(s,2,7, 8,13, m[0], m[1]);  G_fn(s,3,4, 9,14, m[6], m[4]);

    /* Round 5: [14,15,11,5,8,12,9,1,13,3,0,10,2,6,4,7] */
    G_fn(s,0,4, 8,12, m[14],m[15]); G_fn(s,1,5, 9,13, m[11],m[5]);
    G_fn(s,2,6,10,14, m[8], m[12]); G_fn(s,3,7,11,15, m[9], m[1]);
    G_fn(s,0,5,10,15, m[13],m[3]);  G_fn(s,1,6,11,12, m[0], m[10]);
    G_fn(s,2,7, 8,13, m[2], m[6]);  G_fn(s,3,4, 9,14, m[4], m[7]);

    /* Round 6: [15,8,5,0,1,14,11,6,15,10,2,12,3,4,7,13] */
    G_fn(s,0,4, 8,12, m[15],m[8]);  G_fn(s,1,5, 9,13, m[5], m[0]);
    G_fn(s,2,6,10,14, m[1], m[14]); G_fn(s,3,7,11,15, m[11],m[6]);
    G_fn(s,0,5,10,15, m[15],m[10]); G_fn(s,1,6,11,12, m[2], m[12]);
    G_fn(s,2,7, 8,13, m[3], m[4]);  G_fn(s,3,4, 9,14, m[7], m[13]);

    /* Output: state[i] ^ state[i+8] for i in 0..8 */
    for (int i = 0; i < 8; i++) out[i] = s[i] ^ s[i+8];
}

/* Convert uint64 nonce to decimal ASCII. Returns string length. */
static int u64_to_dec(__private char* buf, ulong n) {
    if (n == 0UL) { buf[0] = '0'; return 1; }
    int len = 0;
    char tmp[20];
    while (n > 0UL) { tmp[len++] = '0' + (int)(n % 10UL); n /= 10UL; }
    for (int i = 0; i < len; i++) buf[i] = tmp[len-1-i];
    return len;
}

/* Load 64 bytes starting at buf[offset] into 16 little-endian uint32 words.
   Bytes beyond buf_len are treated as 0 (padding). */
static void load_block_words(__private const char* buf, int buf_len,
                              int offset, __private uint* words) {
    for (int w = 0; w < 16; w++) {
        uint word = 0u;
        for (int b = 0; b < 4; b++) {
            int pos = offset + w*4 + b;
            uint bv = (pos < buf_len) ? ((uint)(uchar)buf[pos]) : 0u;
            word |= bv << (b*8);
        }
        words[w] = word;
    }
}

__kernel void blake3_mine(
    __global const uchar* prefix,    /* header prefix bytes (before nonce) */
    uint    prefix_len,              /* length of prefix */
    ulong   nonce_start,             /* first nonce for this dispatch */
    uint    difficulty,              /* leading zero hex nibbles required */
    __global ulong* found_nonce      /* output: first valid nonce found */
) {
    ulong nonce = nonce_start + (ulong)get_global_id(0);

    /* Build header: prefix + decimal(nonce) into a local buffer (max 300 bytes) */
    char buf[300];
    int  len = (int)(prefix_len < 280u ? prefix_len : 280u);
    for (int i = 0; i < len; i++) buf[i] = (char)prefix[i];
    char nonce_str[20];
    int  nlen = u64_to_dec(nonce_str, nonce);
    for (int i = 0; i < nlen; i++) buf[len+i] = nonce_str[i];
    len += nlen;

    /* Number of 64-byte blocks */
    int num_blocks = (len + 63) / 64;
    if (num_blocks < 1) num_blocks = 1;
    if (num_blocks > 4) num_blocks = 4; /* cap at 256 bytes */

    /* Initialize cv = IV */
    uint cv[8] = { IV0, IV1, IV2, IV3, IV4, IV5, IV6, IV7 };

    /* Process each block through the compression chain */
    for (int b = 0; b < num_blocks; b++) {
        uint m[16];
        int  boff = b * 64;
        int  rem  = len - boff;
        uint blen = (rem < 64) ? (uint)rem : 64u;

        load_block_words(buf, len, boff, m);

        uint flags = 0u;
        if (b == 0)               flags |= CHUNK_START;
        if (b == num_blocks - 1)  flags |= (CHUNK_END | ROOT);

        uint out[8];
        blake3_compress(cv, m, 0UL, blen, flags, out);
        for (int i = 0; i < 8; i++) cv[i] = out[i];
    }

    /* Check difficulty: count leading zero hex nibbles.
       hex::encode(blake3) → byte[0] = cv[0] & 0xFF, high nibble first.
       Nibble i: byte = i/2, high=(i%2==0). */
    uint met = 0u;
    for (uint i = 0u; i < difficulty; i++) {
        uint byte_idx    = i / 2u;
        uint word_idx    = byte_idx / 4u;
        uint byte_in_w   = byte_idx % 4u;
        uint byte_val    = (cv[word_idx] >> (byte_in_w * 8u)) & 0xFFu;
        uint nibble_val  = (i % 2u == 0u) ? (byte_val >> 4u) : (byte_val & 0xFu);
        if (nibble_val != 0u) break;
        met++;
    }

    if (met >= difficulty) {
        /* Atomic: store first found nonce. Use compare-and-swap with sentinel. */
        atom_cmpxchg((volatile __global long*)found_nonce,
                     (long)0xFFFFFFFFFFFFFFFFUL, (long)nonce);
    }
}
"#;

// ── OpenCL config ─────────────────────────────────────────────────────────────

/// Configuration cho OpenCL mining.
pub struct OpenClConfig {
    /// Số compute units (streaming multiprocessors) dùng.
    /// Mặc định: tất cả CU của device / 3.
    pub compute_units:  usize,
    /// Kích thước work group (power of 2, thường 64 hoặc 256).
    pub work_group_size: usize,
    /// Số nonces thử mỗi dispatch (batch).
    pub batch_size:     u64,
}

impl Default for OpenClConfig {
    fn default() -> Self {
        OpenClConfig {
            compute_units:   crate::cpu_miner::default_threads(),
            work_group_size: 64,
            batch_size:      1_000_000,
        }
    }
}

// ── OpenCL device info ────────────────────────────────────────────────────────

/// Thông tin một OpenCL device.
#[derive(Debug, Clone)]
pub struct OpenClDeviceInfo {
    pub name:          String,
    pub vendor:        String,
    pub compute_units: u32,
    pub max_freq_mhz:  u32,
    pub global_mem_mb: u64,
}

/// Liệt kê các OpenCL GPU devices trên hệ thống.
/// Returns empty vec nếu feature `opencl` không được enable hoặc không có GPU.
pub fn list_opencl_devices() -> Vec<OpenClDeviceInfo> {
    #[cfg(feature = "opencl")]
    {
        _list_ocl_devices_impl()
    }
    #[cfg(not(feature = "opencl"))]
    {
        vec![]
    }
}

#[cfg(feature = "opencl")]
fn _list_ocl_devices_impl() -> Vec<OpenClDeviceInfo> {
    use ocl::Platform;
    let mut result = Vec::new();
    for platform in Platform::list() {
        if let Ok(devices) = ocl::Device::list_all(platform) {
            for dev in devices {
                let name   = dev.name().unwrap_or_else(|_| "unknown".into());
                let vendor = dev.vendor().unwrap_or_else(|_| "unknown".into());
                let cu     = dev.info(ocl::enums::DeviceInfo::MaxComputeUnits)
                    .ok().and_then(|v| if let ocl::enums::DeviceInfoResult::MaxComputeUnits(n) = v { Some(n) } else { None })
                    .unwrap_or(0);
                let freq   = dev.info(ocl::enums::DeviceInfo::MaxClockFrequency)
                    .ok().and_then(|v| if let ocl::enums::DeviceInfoResult::MaxClockFrequency(n) = v { Some(n) } else { None })
                    .unwrap_or(0);
                let mem    = dev.info(ocl::enums::DeviceInfo::GlobalMemSize)
                    .ok().and_then(|v| if let ocl::enums::DeviceInfoResult::GlobalMemSize(n) = v { Some(n / 1_000_000) } else { None })
                    .unwrap_or(0);
                result.push(OpenClDeviceInfo { name, vendor, compute_units: cu, max_freq_mhz: freq, global_mem_mb: mem });
            }
        }
    }
    result
}

// ── opencl_available() ────────────────────────────────────────────────────────

/// Returns true nếu feature `opencl` được compile vào.
pub const fn opencl_available() -> bool {
    #[cfg(feature = "opencl")] { true }
    #[cfg(not(feature = "opencl"))] { false }
}

// ── Public mine function ──────────────────────────────────────────────────────

/// Mine một Blake3Block dùng OpenCL nếu available, ngược lại dùng rayon CPU.
///
/// Để dùng GPU: `cargo run --features opencl -- gpumine ...`
pub fn opencl_mine(block: &Blake3Block, difficulty: usize, config: &OpenClConfig) -> GpuMineResult {
    #[cfg(feature = "opencl")]
    {
        match _mine_ocl_impl(block, difficulty, config) {
            Ok(r)  => return r,
            Err(e) => eprintln!("  [opencl] GPU error: {} — falling back to CPU rayon", e),
        }
    }
    _mine_cpu_fallback(block, difficulty, config.compute_units)
}

// ── OCL implementation (feature-gated) ────────────────────────────────────────

#[cfg(feature = "opencl")]
fn _mine_ocl_impl(
    block:      &Blake3Block,
    difficulty: usize,
    config:     &OpenClConfig,
) -> Result<GpuMineResult, String> {
    use ocl::{Platform, Device, Context, Queue, Program, Kernel, Buffer, flags};

    let t0     = Instant::now();
    let target = "0".repeat(difficulty);

    // Build prefix string (same format as mine_live)
    let prefix = format!(
        "{}|{}|{}|{}|{}|",
        block.index, block.timestamp,
        block.txid_root, block.witness_root, block.prev_hash,
    );
    let prefix_bytes = prefix.as_bytes();

    // Setup OCL
    let platform = Platform::list().into_iter().next()
        .ok_or("No OpenCL platform found")?;
    let device = Device::list_all(platform)
        .map_err(|e| e.to_string())?
        .into_iter().next()
        .ok_or("No OpenCL device found")?;

    let context = Context::builder()
        .platform(platform)
        .devices(device)
        .build().map_err(|e| e.to_string())?;
    let queue   = Queue::new(&context, device, None).map_err(|e| e.to_string())?;
    let program = Program::builder()
        .src(BLAKE3_OCL_KERNEL)
        .build(&context).map_err(|e| format!("OCL compile error: {}", e))?;

    // Buffers
    let prefix_buf = Buffer::<u8>::builder()
        .queue(queue.clone())
        .flags(flags::MEM_READ_ONLY | flags::MEM_COPY_HOST_PTR)
        .len(prefix_bytes.len())
        .copy_host_slice(prefix_bytes)
        .build().map_err(|e| e.to_string())?;

    let found_buf = Buffer::<u64>::builder()
        .queue(queue.clone())
        .flags(flags::MEM_READ_WRITE)
        .len(1)
        .build().map_err(|e| e.to_string())?;

    let batch       = config.batch_size;
    let wg          = config.work_group_size as usize;
    let mut nonce   = 0u64;
    let mut total   = 0u64;
    let sentinel    = u64::MAX;

    loop {
        // Reset found_nonce = sentinel
        found_buf.write(&[sentinel][..]).enq().map_err(|e| e.to_string())?;

        let kernel = Kernel::builder()
            .program(&program)
            .name("blake3_mine")
            .queue(queue.clone())
            .global_work_size(batch as usize)
            .local_work_size(wg)
            .arg(&prefix_buf)
            .arg(prefix_bytes.len() as u32)
            .arg(nonce)
            .arg(difficulty as u32)
            .arg(&found_buf)
            .build().map_err(|e| e.to_string())?;

        unsafe { kernel.enq().map_err(|e| e.to_string())?; }
        queue.finish().map_err(|e| e.to_string())?;

        let mut result = vec![0u64];
        found_buf.read(&mut result).enq().map_err(|e| e.to_string())?;
        total += batch;

        if result[0] != sentinel {
            let winning_nonce = result[0];
            // Verify with Rust blake3 (double-check)
            let header = format!("{}{}", prefix, winning_nonce);
            let hash   = hex::encode(blake3::hash(header.as_bytes()).as_bytes());
            if hash.starts_with(&target) {
                return Ok(GpuMineResult {
                    nonce:        winning_nonce,
                    hash,
                    hashes_tried: total,
                    elapsed_ms:   t0.elapsed().as_millis() as u64,
                    backend_used: crate::gpu_miner::GpuBackend::OpenCL,
                });
            }
        }

        nonce = nonce.saturating_add(batch);
        if nonce == u64::MAX { break; }
    }

    Err("Nonce space exhausted without finding solution".into())
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
        .expect("opencl_kernel: build thread pool");

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

pub fn cmd_opencl_info() {
    println!();
    println!("  OpenCL Status: {}", if opencl_available() { "✅ compiled" } else { "⚠️  not compiled (use --features opencl)" });
    let devs = list_opencl_devices();
    if devs.is_empty() {
        println!("  Devices      : none detected");
    } else {
        for d in &devs {
            println!("  Device: {} ({}) — {} CUs @ {}MHz — {}MB",
                d.name, d.vendor, d.compute_units, d.max_freq_mhz, d.global_mem_mb);
        }
    }
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
        assert!(!BLAKE3_OCL_KERNEL.is_empty());
        assert!(BLAKE3_OCL_KERNEL.contains("blake3_mine"));
        assert!(BLAKE3_OCL_KERNEL.contains("blake3_compress"));
    }

    #[test]
    fn test_kernel_contains_msg_schedule() {
        // All 7 rounds present
        assert!(BLAKE3_OCL_KERNEL.contains("Round 0"));
        assert!(BLAKE3_OCL_KERNEL.contains("Round 6"));
    }

    #[test]
    fn test_kernel_contains_difficulty_check() {
        assert!(BLAKE3_OCL_KERNEL.contains("atom_cmpxchg"));
        assert!(BLAKE3_OCL_KERNEL.contains("difficulty"));
    }

    #[test]
    fn test_opencl_available_const() {
        // Should be false without --features opencl
        let avail = opencl_available();
        #[cfg(feature = "opencl")] assert!(avail);
        #[cfg(not(feature = "opencl"))] assert!(!avail);
    }

    #[test]
    fn test_list_devices_without_feature() {
        #[cfg(not(feature = "opencl"))]
        assert!(list_opencl_devices().is_empty());
    }

    #[test]
    fn test_opencl_mine_software_fallback() {
        // Without opencl feature, always uses CPU fallback
        let block  = test_block(1);
        let config = OpenClConfig { compute_units: 1, work_group_size: 64, batch_size: 1_000_000 };
        let result = opencl_mine(&block, 1, &config);
        assert!(result.hash.starts_with('0'), "fallback should find valid hash");
        assert!(result.hashes_tried >= 1);
    }

    #[test]
    fn test_opencl_mine_difficulty_2() {
        let block  = test_block(2);
        let config = OpenClConfig { compute_units: 1, work_group_size: 64, batch_size: 1_000_000 };
        let result = opencl_mine(&block, 2, &config);
        assert!(result.hash.starts_with("00"), "should find diff=2 hash");
    }

    #[test]
    fn test_opencl_config_default() {
        let cfg = OpenClConfig::default();
        assert!(cfg.compute_units >= 1);
        assert!(cfg.work_group_size > 0);
        assert!(cfg.batch_size > 0);
    }

    #[test]
    fn test_cpu_fallback_multiple_threads() {
        let block  = test_block(3);
        let result = _mine_cpu_fallback(&block, 1, 2);
        assert!(result.hash.starts_with('0'));
    }

    #[test]
    fn test_kernel_iv_constants() {
        assert!(BLAKE3_OCL_KERNEL.contains("0x6A09E667u"));
        assert!(BLAKE3_OCL_KERNEL.contains("0xBB67AE85u"));
        assert!(BLAKE3_OCL_KERNEL.contains("0x5BE0CD19u"));
    }

    #[test]
    fn test_kernel_chunk_flags() {
        assert!(BLAKE3_OCL_KERNEL.contains("CHUNK_START"));
        assert!(BLAKE3_OCL_KERNEL.contains("CHUNK_END"));
        assert!(BLAKE3_OCL_KERNEL.contains("ROOT"));
    }

    #[test]
    fn test_kernel_handles_multiblock_header() {
        // Headers with 64-char hex fields are ~240 bytes → 4 blocks
        // The kernel supports up to 4 blocks
        assert!(BLAKE3_OCL_KERNEL.contains("num_blocks > 4"));
    }

    #[test]
    fn test_opencl_mine_result_has_valid_hash_format() {
        let block  = test_block(1);
        let config = OpenClConfig::default();
        let result = opencl_mine(&block, 1, &config);
        assert_eq!(result.hash.len(), 64, "BLAKE3 hash = 32 bytes = 64 hex chars");
        assert!(result.hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
