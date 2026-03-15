#![allow(dead_code)]

/// v3.0 — Post-Quantum: CRYSTALS-Dilithium (Lattice-based Signature)
///
/// Kiến trúc:
///
///   Security based on: Module Learning With Errors (MLWE)
///
///   Polynomial ring: R_q = Z_q[X] / (X^n + 1)
///     n = 256, q = 8,380,417 (prime, q ≡ 5 mod 8)
///
///   Key structure (Dilithium2):
///     Matrix A ∈ R_q^(k×l)      — public, sampled from seed
///     s1 ∈ R_q^l                — secret, small coefficients [-eta, eta]
///     s2 ∈ R_q^k                — secret, small coefficients [-eta, eta]
///     t  = A·s1 + s2 ∈ R_q^k   — public commitment
///
///   Signing (Fiat-Shamir with Aborts):
///     1. Sample masking y ∈ R_q^l  (coefficients in [-gamma1, gamma1])
///     2. Compute w = A·y
///     3. c = H(message || HighBits(w))   (challenge, sparse polynomial)
///     4. z = y + c·s1                     (response)
///     5. Abort if |z|_inf >= gamma1 - beta  (rejection sampling)
///     6. Signature = (c_hash, z, hint_h)
///
///   Verification:
///     1. Recompute w' = A·z - c·t
///     2. Check c_hash == H(message || UseHint(h, w'))
///     3. Check |z|_inf < gamma1 - beta
///
///   Why quantum-safe:
///     - Shor's algorithm breaks DLP/factoring (ECDSA, RSA)
///     - Best quantum attack on LWE: still exponential (BKZ lattice reduction)
///     - Dilithium selected by NIST PQC (FIPS 204, 2024)
///
///   Simplified vs real:
///     - This uses pseudo-randomness from SHA-256 instead of SHAKE-256/AES-CTR
///     - Polynomial multiplication is schoolbook O(n^2) not NTT O(n log n)
///     - No constant-time implementation (educational only)
///
/// Tham khảo: CRYSTALS-Dilithium spec (v3.1), NIST FIPS 204

use sha2::{Sha256, Digest};

// ─── Parameters ───────────────────────────────────────────────────────────────

pub const N:       usize = 256;           // polynomial degree
pub const Q:       i64   = 8_380_417;    // prime modulus
pub const K:       usize = 4;             // Dilithium2: k=4, l=4
pub const L:       usize = 4;
pub const ETA:     i64   = 2;             // secret key bound [-eta, eta]
pub const GAMMA1:  i64   = 1 << 17;      // 131072 — masking range
pub const GAMMA2:  i64   = 95_232;       // (Q-1)/88
pub const TAU:     usize = 39;            // challenge weight (sparse polynomial)
pub const BETA:    i64   = ETA * TAU as i64;  // 78 — response bound slack

/// Parameter sets comparison table
pub struct ParamSet {
    pub name:       &'static str,
    pub k:          usize,
    pub l:          usize,
    pub eta:        i64,
    pub tau:        usize,
    pub gamma1_log: u32,    // gamma1 = 2^gamma1_log
    pub pk_bytes:   usize,
    pub sk_bytes:   usize,
    pub sig_bytes:  usize,
    pub security:   &'static str,
}

pub const PARAM_SETS: [ParamSet; 3] = [
    ParamSet { name: "Dilithium2", k: 4, l: 4, eta: 2, tau: 39,  gamma1_log: 17, pk_bytes: 1312, sk_bytes: 2528, sig_bytes: 2420, security: "NIST Level 2 (128-bit)" },
    ParamSet { name: "Dilithium3", k: 6, l: 5, eta: 4, tau: 49,  gamma1_log: 19, pk_bytes: 1952, sk_bytes: 4000, sig_bytes: 3293, security: "NIST Level 3 (192-bit)" },
    ParamSet { name: "Dilithium5", k: 8, l: 7, eta: 2, tau: 60,  gamma1_log: 19, pk_bytes: 2592, sk_bytes: 4864, sig_bytes: 4595, security: "NIST Level 5 (256-bit)" },
];

// ─── Polynomial ───────────────────────────────────────────────────────────────

/// Polynomial in R_q = Z_q[X] / (X^n + 1)
#[derive(Debug, Clone, PartialEq)]
pub struct Poly {
    pub coeffs: [i64; N],
}

impl Poly {
    pub fn zero() -> Self {
        Poly { coeffs: [0i64; N] }
    }

    /// Reduce all coefficients mod q, centered in (-q/2, q/2]
    pub fn reduce(&mut self) {
        for c in &mut self.coeffs {
            *c = mod_centered(*c, Q);
        }
    }

    /// Add two polynomials mod q
    pub fn add(&self, other: &Poly) -> Poly {
        let mut r = Poly::zero();
        for i in 0..N {
            r.coeffs[i] = mod_q(self.coeffs[i] + other.coeffs[i]);
        }
        r
    }

    /// Subtract
    pub fn sub(&self, other: &Poly) -> Poly {
        let mut r = Poly::zero();
        for i in 0..N {
            r.coeffs[i] = mod_q(self.coeffs[i] - other.coeffs[i]);
        }
        r
    }

    /// Schoolbook multiplication in R_q = Z_q[X]/(X^n+1)
    /// When degree i+j >= N, coefficient negates (X^N ≡ -1)
    pub fn mul(&self, other: &Poly) -> Poly {
        let mut r = Poly::zero();
        for i in 0..N {
            for j in 0..N {
                let idx = i + j;
                if idx < N {
                    r.coeffs[idx] = mod_q(r.coeffs[idx] + self.coeffs[i] * other.coeffs[j]);
                } else {
                    r.coeffs[idx - N] = mod_q(r.coeffs[idx - N] - self.coeffs[i] * other.coeffs[j]);
                }
            }
        }
        r
    }

    /// Infinity norm: max |coeff|
    pub fn norm_inf(&self) -> i64 {
        self.coeffs.iter().map(|&c| {
            let c = mod_centered(c, Q);
            c.abs()
        }).max().unwrap_or(0)
    }

    /// HighBits: round coefficient to nearest multiple of 2*gamma2
    pub fn high_bits(&self) -> Poly {
        let mut r = Poly::zero();
        for i in 0..N {
            r.coeffs[i] = decompose_high(self.coeffs[i]);
        }
        r
    }
}

// ─── Math helpers ─────────────────────────────────────────────────────────────

#[inline]
pub fn mod_q(x: i64) -> i64 {
    ((x % Q) + Q) % Q
}

#[inline]
pub fn mod_centered(x: i64, modulus: i64) -> i64 {
    let r = ((x % modulus) + modulus) % modulus;
    if r > modulus / 2 { r - modulus } else { r }
}

/// HighBits: r1 = round(r / (2*gamma2))
pub fn decompose_high(r: i64) -> i64 {
    let r = mod_q(r);
    (r + GAMMA2) / (2 * GAMMA2)
}

// ─── Deterministic sampling from seed ─────────────────────────────────────────

/// Expand seed into a polynomial with coefficients in [0, q)
/// (Simulates SHAKE-128 XOF — we use iterated SHA-256)
pub fn sample_uniform(seed: &[u8], domain: u8, row: usize, col: usize) -> Poly {
    let mut p = Poly::zero();
    let mut counter = 0u64;
    let mut idx = 0;
    let mut buf = [0u8; 32];

    while idx < N {
        let mut h = Sha256::new();
        h.update(seed);
        h.update([domain]);
        h.update(row.to_le_bytes());
        h.update(col.to_le_bytes());
        h.update(counter.to_le_bytes());
        buf.copy_from_slice(&h.finalize());
        counter += 1;

        // Pack 3 bytes → 1 coefficient (like real Dilithium)
        let mut i = 0;
        while i + 2 < 32 && idx < N {
            let val = (buf[i] as i64)
                | ((buf[i+1] as i64) << 8)
                | ((buf[i+2] as i64) << 16);
            let val = val & 0x7F_FFFF; // 23 bits
            if val < Q {
                p.coeffs[idx] = val;
                idx += 1;
            }
            i += 3;
        }
    }
    p
}

/// Sample small polynomial with coefficients in [-eta, eta]
pub fn sample_small(seed: &[u8], domain: u8, idx: usize) -> Poly {
    let mut p = Poly::zero();
    let mut ctr = 0u64;
    let mut filled = 0;

    while filled < N {
        let mut h = Sha256::new();
        h.update(seed);
        h.update([domain]);
        h.update(idx.to_le_bytes());
        h.update(ctr.to_le_bytes());
        let hash = h.finalize();
        ctr += 1;

        for &byte in hash.iter() {
            if filled >= N { break; }
            // Each nibble: if < 2*eta+1, use it; reject otherwise (eta=2: range 0..4)
            let lo = (byte & 0x0F) as i64;
            let hi = ((byte >> 4) & 0x0F) as i64;
            let bound = 2 * ETA + 1; // 5 for eta=2
            if lo < bound {
                p.coeffs[filled] = ETA - lo; // center: [2, 1, 0, -1, -2]
                filled += 1;
            }
            if filled >= N { break; }
            if hi < bound {
                p.coeffs[filled] = ETA - hi;
                filled += 1;
            }
        }
    }
    p
}

/// Sample masking polynomial with coefficients in (-gamma1, gamma1]
pub fn sample_mask(seed: &[u8], nonce: u16, idx: usize) -> Poly {
    let mut p = Poly::zero();
    let mut ctr = 0u64;
    let mut filled = 0;
    let range = 2 * GAMMA1;

    while filled < N {
        let mut h = Sha256::new();
        h.update(seed);
        h.update(nonce.to_le_bytes());
        h.update(idx.to_le_bytes());
        h.update(ctr.to_le_bytes());
        let hash = h.finalize();
        ctr += 1;

        // 5 bytes → 2 coefficients in [0, 2*gamma1)
        let mut i = 0;
        while i + 4 < 32 && filled < N {
            let v1 = (buf5_as_i64(&hash, i)) % range;
            p.coeffs[filled] = v1 - GAMMA1 + 1; // center
            filled += 1;
            i += 3;
        }
    }
    p
}

fn buf5_as_i64(buf: &[u8], start: usize) -> i64 {
    if start + 2 >= buf.len() { return 0; }
    (buf[start] as i64)
        | ((buf[start+1] as i64) << 8)
        | ((buf[start+2] as i64) << 16)
}

/// Sample challenge polynomial: sparse, weight TAU, coefficients ±1
pub fn sample_challenge(hash: &[u8]) -> Poly {
    let mut c = Poly::zero();
    let mut positions = Vec::with_capacity(TAU);
    let mut ctr = 0u64;

    while positions.len() < TAU {
        let mut h = Sha256::new();
        h.update(hash);
        h.update(ctr.to_le_bytes());
        let buf = h.finalize();
        ctr += 1;

        for chunk in buf.chunks(3) {
            if chunk.len() < 2 || positions.len() >= TAU { break; }
            let pos = ((chunk[0] as usize) | ((chunk[1] as usize) << 8)) % N;
            if !positions.contains(&pos) {
                let sign = if chunk.len() > 2 { (chunk[2] & 1) as i64 } else { 1 };
                c.coeffs[pos] = if sign == 0 { 1 } else { -1 };
                positions.push(pos);
            }
        }
    }
    c
}

// ─── Module arithmetic (vectors/matrices of polynomials) ──────────────────────

/// k-vector of polynomials
#[derive(Debug, Clone)]
pub struct PolyVec {
    pub polys: Vec<Poly>,
}

impl PolyVec {
    pub fn zero(len: usize) -> Self {
        PolyVec { polys: vec![Poly::zero(); len] }
    }

    pub fn add(&self, other: &PolyVec) -> PolyVec {
        assert_eq!(self.polys.len(), other.polys.len());
        PolyVec { polys: self.polys.iter().zip(&other.polys).map(|(a,b)| a.add(b)).collect() }
    }

    pub fn sub(&self, other: &PolyVec) -> PolyVec {
        assert_eq!(self.polys.len(), other.polys.len());
        PolyVec { polys: self.polys.iter().zip(&other.polys).map(|(a,b)| a.sub(b)).collect() }
    }

    pub fn scale(&self, c: &Poly) -> PolyVec {
        PolyVec { polys: self.polys.iter().map(|p| p.mul(c)).collect() }
    }

    pub fn norm_inf(&self) -> i64 {
        self.polys.iter().map(|p| p.norm_inf()).max().unwrap_or(0)
    }

    pub fn high_bits(&self) -> PolyVec {
        PolyVec { polys: self.polys.iter().map(|p| p.high_bits()).collect() }
    }

    /// Hash all coefficients to a 32-byte digest
    pub fn hash(&self) -> Vec<u8> {
        let mut h = Sha256::new();
        for p in &self.polys {
            for &c in &p.coeffs {
                h.update(c.to_le_bytes());
            }
        }
        h.finalize().to_vec()
    }
}

/// k×l matrix of polynomials
pub struct PolyMatrix {
    pub rows: Vec<PolyVec>,   // rows[i] has L polynomials
}

impl PolyMatrix {
    /// A·v: matrix-vector product (k rows × l cols) × l-vec → k-vec
    pub fn mul_vec(&self, v: &PolyVec) -> PolyVec {
        let k = self.rows.len();
        let mut result = PolyVec::zero(k);
        for i in 0..k {
            let row = &self.rows[i];
            assert_eq!(row.polys.len(), v.polys.len());
            let mut acc = Poly::zero();
            for j in 0..row.polys.len() {
                let prod = row.polys[j].mul(&v.polys[j]);
                acc = acc.add(&prod);
            }
            acc.reduce();
            result.polys[i] = acc;
        }
        result
    }
}

// ─── Keys ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PublicKey {
    pub seed_a:  Vec<u8>,       // 32-byte seed to regenerate A
    pub t:       PolyVec,       // t = A·s1 + s2 (k polynomials)
    pub t_bytes: usize,         // serialized size
}

#[derive(Debug, Clone)]
pub struct SecretKey {
    pub seed_a:  Vec<u8>,
    pub seed_s:  Vec<u8>,       // seed for s1, s2
    pub s1:      PolyVec,       // L small polynomials
    pub s2:      PolyVec,       // K small polynomials
    pub t:       PolyVec,       // cached for signing
}

#[derive(Debug, Clone)]
pub struct Signature {
    pub c_hash:  Vec<u8>,       // 32-byte hash of challenge
    pub z:       PolyVec,       // response vector (L polys)
    pub aborts:  u32,           // how many rejections before success
}

// ─── Key Generation ───────────────────────────────────────────────────────────

pub fn keygen(seed: &[u8]) -> (PublicKey, SecretKey) {
    // Derive seeds
    let mut h = Sha256::new();
    h.update(b"dilithium_keygen_v30");
    h.update(seed);
    let master = h.finalize();

    let seed_a = master[..16].to_vec();
    let seed_s = master[16..].to_vec();

    // Generate public matrix A ∈ R_q^(k×l)
    let a = expand_a(&seed_a);

    // Sample secret vectors s1 ∈ R_q^l, s2 ∈ R_q^k
    let s1 = PolyVec {
        polys: (0..L).map(|i| sample_small(&seed_s, 0x01, i)).collect()
    };
    let s2 = PolyVec {
        polys: (0..K).map(|i| sample_small(&seed_s, 0x02, i)).collect()
    };

    // t = A·s1 + s2
    let as1 = a.mul_vec(&s1);
    let t = as1.add(&s2);

    let pk = PublicKey {
        seed_a: seed_a.clone(),
        t: t.clone(),
        t_bytes: 32 + K * N * 3, // approximate
    };
    let sk = SecretKey { seed_a, seed_s, s1, s2, t };
    (pk, sk)
}

/// Expand seed into A matrix
pub fn expand_a(seed_a: &[u8]) -> PolyMatrix {
    PolyMatrix {
        rows: (0..K).map(|i| PolyVec {
            polys: (0..L).map(|j| sample_uniform(seed_a, 0xAA, i, j)).collect()
        }).collect()
    }
}

// ─── Sign ─────────────────────────────────────────────────────────────────────

pub fn sign(sk: &SecretKey, message: &[u8]) -> Signature {
    let a = expand_a(&sk.seed_a);
    let mut rng_seed = Sha256::new();
    rng_seed.update(b"dilithium_sign_v30");
    rng_seed.update(&sk.seed_s);
    rng_seed.update(message);
    let rng_base = rng_seed.finalize();

    let mut nonce: u16 = 0;
    let mut aborts = 0u32;

    loop {
        // 1. Sample masking vector y ∈ R_q^l
        let y = PolyVec {
            polys: (0..L).map(|i| sample_mask(&rng_base, nonce, i)).collect()
        };
        nonce = nonce.wrapping_add(1);

        // 2. w = A·y
        let w = a.mul_vec(&y);

        // 3. Compute challenge c = H(message || HighBits(w))
        let mut h = Sha256::new();
        h.update(b"dilithium_challenge");
        h.update(message);
        h.update(&w.high_bits().hash());
        let c_hash = h.finalize().to_vec();

        let c_poly = sample_challenge(&c_hash);

        // 4. z = y + c·s1
        let cs1 = sk.s1.scale(&c_poly);
        let z   = y.add(&cs1);

        // 5. Rejection sampling: abort if ||z||_inf >= gamma1 - beta
        let bound = GAMMA1 - BETA;
        if z.norm_inf() >= bound {
            aborts += 1;
            if aborts > 100 { // safety limit for demo
                // Return with best effort
                return Signature { c_hash, z, aborts };
            }
            continue;
        }

        // Also check: ||c·s2||_inf < gamma2 - beta  (hint check simplified)
        let cs2 = sk.s2.scale(&c_poly);
        if cs2.norm_inf() >= GAMMA2 - BETA {
            aborts += 1;
            continue;
        }

        return Signature { c_hash, z, aborts };
    }
}

// ─── Verify ───────────────────────────────────────────────────────────────────

pub fn verify(pk: &PublicKey, message: &[u8], sig: &Signature) -> bool {
    // 1. Check z bound
    let bound = GAMMA1 - BETA;
    if sig.z.norm_inf() >= bound {
        return false;
    }

    // 2. Regenerate A from seed
    let a = expand_a(&pk.seed_a);

    // 3. Recompute challenge polynomial from stored hash
    let c_poly = sample_challenge(&sig.c_hash);

    // 4. Recompute w' = A·z - c·t
    let az  = a.mul_vec(&sig.z);
    let ct  = pk.t.scale(&c_poly);
    let w_prime = az.sub(&ct);

    // 5. Recompute challenge hash from w'
    let mut h = Sha256::new();
    h.update(b"dilithium_challenge");
    h.update(message);
    h.update(&w_prime.high_bits().hash());
    let expected_hash = h.finalize().to_vec();

    // 6. Check c_hash matches
    sig.c_hash == expected_hash
}

// ─── DilithiumKeypair (convenience wrapper) ───────────────────────────────────

pub struct DilithiumKeypair {
    pub pk: PublicKey,
    pub sk: SecretKey,
    pub address: String,   // H(pk) truncated
}

impl DilithiumKeypair {
    pub fn generate(seed: &[u8]) -> Self {
        let (pk, sk) = keygen(seed);
        let mut h = Sha256::new();
        h.update(b"dilithium_address");
        h.update(&pk.seed_a);
        h.update(&pk.t.hash());
        let address = format!("dil1{}", &hex::encode(h.finalize())[..40]);
        DilithiumKeypair { pk, sk, address }
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        sign(&self.sk, message)
    }

    pub fn verify(&self, message: &[u8], sig: &Signature) -> bool {
        verify(&self.pk, message, sig)
    }
}

// ─── Comparison: Classical vs Post-Quantum ────────────────────────────────────

pub struct AlgorithmComparison {
    pub name:        &'static str,
    pub pk_bytes:    usize,
    pub sk_bytes:    usize,
    pub sig_bytes:   usize,
    pub classical_security: &'static str,
    pub quantum_security:   &'static str,
    pub assumption:         &'static str,
}

pub const COMPARISON: [AlgorithmComparison; 4] = [
    AlgorithmComparison {
        name: "ECDSA (secp256k1)",
        pk_bytes: 33, sk_bytes: 32, sig_bytes: 64,
        classical_security: "128-bit (ECDLP)",
        quantum_security:   "0-bit (broken by Shor's)",
        assumption: "Elliptic Curve DLP",
    },
    AlgorithmComparison {
        name: "RSA-2048",
        pk_bytes: 256, sk_bytes: 1192, sig_bytes: 256,
        classical_security: "112-bit",
        quantum_security:   "0-bit (broken by Shor's)",
        assumption: "Integer Factorization",
    },
    AlgorithmComparison {
        name: "Dilithium2 (FIPS 204)",
        pk_bytes: 1312, sk_bytes: 2528, sig_bytes: 2420,
        classical_security: "128-bit",
        quantum_security:   "128-bit (Grover + BKZ)",
        assumption: "Module-LWE + Module-SIS",
    },
    AlgorithmComparison {
        name: "Dilithium5 (FIPS 204)",
        pk_bytes: 2592, sk_bytes: 4864, sig_bytes: 4595,
        classical_security: "256-bit",
        quantum_security:   "256-bit",
        assumption: "Module-LWE + Module-SIS",
    },
];

// ─── LWE intuition demo ───────────────────────────────────────────────────────

/// Demonstrate why LWE is hard:
/// Given (A, b = A·s + e), find s
/// e = small error vector makes it computationally hard
pub struct LweProblem {
    pub n:    usize,
    pub q:    i64,
    pub a:    Vec<i64>,   // public random vector
    pub s:    Vec<i64>,   // SECRET: what attacker wants
    pub e:    Vec<i64>,   // small error
    pub b:    Vec<i64>,   // public: b = A·s + e
}

impl LweProblem {
    pub fn new(n: usize, q: i64, seed: &[u8]) -> Self {
        let mut h = Sha256::new();
        h.update(seed);
        let hash = h.finalize();

        // Sample A (random), s (small secret), e (small error)
        let a: Vec<i64> = (0..n).map(|i| ((hash[i % 32] as i64) * 37 + i as i64 * 13) % q).collect();
        let s: Vec<i64> = (0..n).map(|i| ((hash[i % 32] as i64) & 3) - 1).collect(); // [-1,2]
        let e: Vec<i64> = (0..n).map(|i| (hash[(i+1) % 32] as i64) & 1).collect(); // {0,1}

        // b[i] = sum_j(A[j] * s[j]) + e[i]  mod q  (simplified 1D)
        let dot: i64 = a.iter().zip(&s).map(|(ai, si)| ai * si).sum();
        let b: Vec<i64> = e.iter().map(|ei| mod_q(dot + ei)).collect();

        LweProblem { n, q, a, s, e, b }
    }

    pub fn secret_norm(&self) -> i64 {
        self.s.iter().map(|x| x.abs()).max().unwrap_or(0)
    }

    pub fn error_norm(&self) -> i64 {
        self.e.iter().map(|x| x.abs()).max().unwrap_or(0)
    }
}
