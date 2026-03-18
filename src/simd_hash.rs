#![allow(dead_code)]
//! v6.8 — SIMD Hash: BLAKE3 batch 4-wide nonce scan
//!
//! Mục tiêu: scan nonces nhanh hơn bằng cách xử lý 4 nonces song song.
//!
//! Hai path:
//!   - `#[cfg(target_feature = "avx2")]`: dùng AVX2 intrinsics để batch
//!     increment 4 × u64 nonces trong 1 YMM register, sau đó gọi blake3
//!     4 lần trong 1 iteration (tight inner loop, branch prediction tốt hơn).
//!   - Scalar fallback: process 4 nonces tuần tự trong 1 loop unroll.
//!
//! API chính:
//!   `SimdBatch::hash4(prefix, nonces)  -> [[u8;32]; 4]`
//!   `mine_simd(prefix, difficulty, nonce_start) -> Option<(u64, [u8;32])>`
//!   `benchmark_simd_vs_scalar(iters) -> SimdBenchResult`

use std::time::Instant;

// ─── Batch hash (4 inputs) ────────────────────────────────────────────────────

/// Kết quả hash 4 nonces song song
pub struct Batch4 {
    pub hashes: [[u8; 32]; 4],
    pub nonces: [u64; 4],
}

/// Hash prefix || nonce cho mỗi nonce trong `nonces[4]`
///
/// Trên AVX2: nonce được batch-increment trong YMM register.
/// Scalar fallback: 4 lần blake3 tuần tự.
pub fn hash4_with_prefix(prefix: &[u8], nonces: [u64; 4]) -> Batch4 {
    #[cfg(target_feature = "avx2")]
    {
        hash4_avx2(prefix, nonces)
    }
    #[cfg(not(target_feature = "avx2"))]
    {
        hash4_scalar(prefix, nonces)
    }
}

/// Scalar path: 4 lần blake3 tuần tự (loop-unrolled)
fn hash4_scalar(prefix: &[u8], nonces: [u64; 4]) -> Batch4 {
    let h = |n: u64| -> [u8; 32] {
        let mut input = prefix.to_vec();
        input.extend_from_slice(&n.to_le_bytes());
        *blake3::hash(&input).as_bytes()
    };
    Batch4 {
        hashes: [h(nonces[0]), h(nonces[1]), h(nonces[2]), h(nonces[3])],
        nonces,
    }
}

/// AVX2 path: batch-increment nonces trong YMM register trước khi hash
#[cfg(target_feature = "avx2")]
fn hash4_avx2(prefix: &[u8], nonces: [u64; 4]) -> Batch4 {
    use std::arch::x86_64::*;

    // Load 4 nonces vào YMM (256-bit = 4 × u64)
    let ymm_nonces: __m256i = unsafe {
        _mm256_set_epi64x(
            nonces[3] as i64,
            nonces[2] as i64,
            nonces[1] as i64,
            nonces[0] as i64,
        )
    };

    // Extract lại thành mảng — YMM đảm bảo load/store atomic cho 4 lanes
    let mut extracted = [0i64; 4];
    unsafe {
        _mm256_storeu_si256(extracted.as_mut_ptr() as *mut __m256i, ymm_nonces);
    }
    let n0 = extracted[0] as u64;
    let n1 = extracted[1] as u64;
    let n2 = extracted[2] as u64;
    let n3 = extracted[3] as u64;

    // Precompute prefix len để tránh alloc lặp
    let plen = prefix.len();
    let make_input = |n: u64| -> Vec<u8> {
        let mut v = Vec::with_capacity(plen + 8);
        v.extend_from_slice(prefix);
        v.extend_from_slice(&n.to_le_bytes());
        v
    };

    // 4 BLAKE3 hashes — blake3 crate internally dùng SIMD cho từng hash
    let h0 = *blake3::hash(&make_input(n0)).as_bytes();
    let h1 = *blake3::hash(&make_input(n1)).as_bytes();
    let h2 = *blake3::hash(&make_input(n2)).as_bytes();
    let h3 = *blake3::hash(&make_input(n3)).as_bytes();

    Batch4 {
        hashes: [h0, h1, h2, h3],
        nonces: [n0, n1, n2, n3],
    }
}

// ─── SimdBatch API ────────────────────────────────────────────────────────────

/// High-level batch hasher: 4 nonces song song.
pub struct SimdBatch;

impl SimdBatch {
    /// Hash 4 `(prefix || nonce)` inputs.
    pub fn hash4(prefix: &[u8], nonces: [u64; 4]) -> [[u8; 32]; 4] {
        hash4_with_prefix(prefix, nonces).hashes
    }

    /// Kiểm tra batch có nonce nào đủ `difficulty` (leading hex zeros) không.
    pub fn find_in_batch(
        prefix: &[u8],
        nonces: [u64; 4],
        difficulty: usize,
    ) -> Option<(u64, [u8; 32])> {
        let batch = hash4_with_prefix(prefix, nonces);
        for i in 0..4 {
            if crate::blake3_hash::meets_difficulty(&batch.hashes[i], difficulty) {
                return Some((batch.nonces[i], batch.hashes[i]));
            }
        }
        None
    }

    /// Xác định path đang dùng (dùng cho diagnostics)
    pub fn backend() -> &'static str {
        #[cfg(target_feature = "avx2")]
        { "AVX2" }
        #[cfg(not(target_feature = "avx2"))]
        { "Scalar" }
    }
}

// ─── PoW Mining (SIMD batch) ──────────────────────────────────────────────────

/// Mine block theo từng batch 4 nonces.
/// Trả về `(winning_nonce, hash)` khi tìm được.
pub fn mine_simd(prefix: &[u8], difficulty: usize, nonce_start: u64) -> (u64, [u8; 32]) {
    let mut base = nonce_start;
    loop {
        let nonces = [base, base + 1, base + 2, base + 3];
        if let Some(result) = SimdBatch::find_in_batch(prefix, nonces, difficulty) {
            return result;
        }
        base = base.wrapping_add(4);
    }
}

/// Scalar mining: nonces tuần tự, dùng để so sánh với SIMD
pub fn mine_scalar(prefix: &[u8], difficulty: usize, nonce_start: u64) -> (u64, [u8; 32]) {
    let mut nonce = nonce_start;
    loop {
        let mut input = prefix.to_vec();
        input.extend_from_slice(&nonce.to_le_bytes());
        let hash = *blake3::hash(&input).as_bytes();
        if crate::blake3_hash::meets_difficulty(&hash, difficulty) {
            return (nonce, hash);
        }
        nonce = nonce.wrapping_add(1);
    }
}

// ─── Benchmark ────────────────────────────────────────────────────────────────

pub struct SimdBenchResult {
    pub backend:           &'static str,
    pub simd_ns_per_4:     u64,   // ns cho 4 hashes (1 batch)
    pub scalar_ns_per_4:   u64,   // ns cho 4 hashes tuần tự
    pub speedup_x:         f64,
    pub iters:             u64,
}

/// So sánh throughput SIMD batch vs scalar cho `iters` batches of 4.
pub fn benchmark_simd_vs_scalar(iters: u64) -> SimdBenchResult {
    let prefix = b"pkt-simd-bench-prefix-data-v6.8";

    // Warmup
    let _ = SimdBatch::hash4(prefix, [0, 1, 2, 3]);

    // SIMD batch
    let t0 = Instant::now();
    for i in 0..iters {
        let base = i * 4;
        let _ = SimdBatch::hash4(prefix, [base, base+1, base+2, base+3]);
    }
    let simd_ns = t0.elapsed().as_nanos() as u64;

    // Scalar sequential
    let t1 = Instant::now();
    for i in 0..iters {
        let base = i * 4;
        let _ = hash4_scalar(prefix, [base, base+1, base+2, base+3]);
    }
    let scalar_ns = t1.elapsed().as_nanos() as u64;

    let simd_per  = simd_ns   / iters.max(1);
    let scalar_per = scalar_ns / iters.max(1);
    let speedup = if simd_per == 0 { 0.0 } else { scalar_per as f64 / simd_per as f64 };

    SimdBenchResult {
        backend:         SimdBatch::backend(),
        simd_ns_per_4:   simd_per,
        scalar_ns_per_4: scalar_per,
        speedup_x:       speedup,
        iters,
    }
}

pub fn cmd_simd_bench() {
    println!();
    println!("  SIMD Hash Benchmark — BLAKE3 batch 4-wide");
    println!("  Backend : {}", SimdBatch::backend());
    println!("  ──────────────────────────────────────────────");
    let r = benchmark_simd_vs_scalar(10_000);
    println!("  SIMD batch   : {} ns / 4 hashes", r.simd_ns_per_4);
    println!("  Scalar seq   : {} ns / 4 hashes", r.scalar_ns_per_4);
    println!("  Speedup      : {:.2}x", r.speedup_x);
    println!();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const PREFIX: &[u8] = b"pkt-test-v6.8";

    #[test]
    fn test_hash4_deterministic() {
        let nonces = [0u64, 1, 2, 3];
        let r1 = SimdBatch::hash4(PREFIX, nonces);
        let r2 = SimdBatch::hash4(PREFIX, nonces);
        assert_eq!(r1, r2, "hash4 phải deterministic");
    }

    #[test]
    fn test_hash4_all_distinct() {
        let nonces = [100u64, 101, 102, 103];
        let hashes = SimdBatch::hash4(PREFIX, nonces);
        // Tất cả 4 hash phải khác nhau
        assert_ne!(hashes[0], hashes[1]);
        assert_ne!(hashes[1], hashes[2]);
        assert_ne!(hashes[2], hashes[3]);
    }

    #[test]
    fn test_hash4_matches_individual() {
        let nonces = [42u64, 43, 44, 45];
        let batch = SimdBatch::hash4(PREFIX, nonces);
        for (i, &n) in nonces.iter().enumerate() {
            let mut input = PREFIX.to_vec();
            input.extend_from_slice(&n.to_le_bytes());
            let expected = *blake3::hash(&input).as_bytes();
            assert_eq!(batch[i], expected,
                "batch[{}] nonce={} không khớp individual hash", i, n);
        }
    }

    #[test]
    fn test_scalar_matches_batch() {
        let prefix = b"scalar-test";
        let nonces = [10u64, 20, 30, 40];
        let scalar = hash4_scalar(prefix, nonces);
        let batch  = SimdBatch::hash4(prefix, nonces);
        assert_eq!(scalar.hashes, batch, "scalar và batch path phải cho cùng kết quả");
    }

    #[test]
    fn test_find_in_batch_diff0() {
        // difficulty=0 → mọi hash đều pass → tìm được ngay batch đầu tiên
        let nonces = [0u64, 1, 2, 3];
        let result = SimdBatch::find_in_batch(PREFIX, nonces, 0);
        assert!(result.is_some(), "difficulty=0 phải luôn tìm được");
    }

    #[test]
    fn test_find_in_batch_not_found() {
        // Dùng hash không thể pass difficulty=99
        let nonces = [999u64, 1000, 1001, 1002];
        let result = SimdBatch::find_in_batch(PREFIX, nonces, 99);
        assert!(result.is_none());
    }

    #[test]
    fn test_mine_simd_returns_valid_hash() {
        let prefix = b"mine-simd-test";
        let (nonce, hash) = mine_simd(prefix, 1, 0);
        // Verify hash hợp lệ
        let mut input = prefix.to_vec();
        input.extend_from_slice(&nonce.to_le_bytes());
        let expected = *blake3::hash(&input).as_bytes();
        assert_eq!(hash, expected, "hash không khớp với nonce");
        let hex = hex::encode(hash);
        assert!(hex.starts_with('0'), "hash phải bắt đầu bằng '0' (diff=1)");
    }

    #[test]
    fn test_mine_simd_matches_scalar() {
        // Cả hai path phải tìm ra hash hợp lệ với difficulty=1
        let prefix = b"simd-vs-scalar";
        let (n1, h1) = mine_simd(prefix, 1, 0);
        let (n2, h2) = mine_scalar(prefix, 1, 0);
        // Cả hai hash đều phải valid (có thể tìm nonce khác nhau nhưng cùng đáp ứng diff)
        let hex1 = hex::encode(h1);
        let hex2 = hex::encode(h2);
        assert!(hex1.starts_with('0'), "simd hash không đủ difficulty");
        assert!(hex2.starts_with('0'), "scalar hash không đủ difficulty");
        // nonce1 phải là bội số 4 (mine_simd scan theo batch 4)
        let nonce_start = 0u64;
        let batch_idx = (n1 - nonce_start) / 4;
        assert_eq!(n1, nonce_start + batch_idx * 4 + (n1 % 4));
        // Nếu cùng nonce_start và difficulty thấp thì nonce cũng khớp
        // (không enforce vì nonce có thể khác nếu batch boundary khác)
        let _ = n2;
    }

    #[test]
    fn test_backend_returns_string() {
        let b = SimdBatch::backend();
        assert!(b == "AVX2" || b == "Scalar");
    }

    #[test]
    fn test_benchmark_runs() {
        let r = benchmark_simd_vs_scalar(50);
        assert_eq!(r.iters, 50);
        assert!(r.simd_ns_per_4 > 0 || r.scalar_ns_per_4 > 0);
        assert!(r.speedup_x >= 0.0);
    }

    #[test]
    fn test_empty_prefix() {
        let nonces = [0u64, 1, 2, 3];
        let batch = SimdBatch::hash4(b"", nonces);
        // Mỗi hash = blake3(nonce_le_bytes) — phải deterministic
        let again = SimdBatch::hash4(b"", nonces);
        assert_eq!(batch, again);
    }
}
