#![allow(dead_code)]

/// v3.2 — Post-Quantum: CRYSTALS-KYBER (ML-KEM)
///
/// Key Encapsulation Mechanism based on Module Learning With Errors (Module-LWE)
///
/// ─── KEM Flow ────────────────────────────────────────────────────────────────
///
///   Alice: KeyGen(seed) → (pk, sk)
///   Bob:   Encapsulate(pk, rand) → (ciphertext, K_bob)
///   Alice: Decapsulate(sk, ciphertext) → K_alice
///   ──────────────────────────────────────────
///   K_bob == K_alice  (shared secret established)
///
/// ─── KeyGen ──────────────────────────────────────────────────────────────────
///
///   A ← R_q^{k×k}        (expand from seed rho — public matrix)
///   s, e ← CBD_η₁         (small secret and error vectors)
///   t = A·s + e           (public key "LWE sample")
///   PK = (rho, t),  SK = s
///
/// ─── Encapsulate ─────────────────────────────────────────────────────────────
///
///   m ← {0,1}^n           (random plaintext)
///   (K̄, r) = G(m ‖ H(pk)) (FO transform: key+randomness from m)
///   r, e₁, e₂ ← CBD       (fresh noise from r)
///   u = Aᵀ·r + e₁          (ciphertext LWE sample)
///   v = tᵀ·r + e₂ + ⌊q/2⌋·m (encode m in high bits)
///   K = KDF(K̄ ‖ H(ct))    (shared key bound to ciphertext)
///
/// ─── Decapsulate ─────────────────────────────────────────────────────────────
///
///   m' = decode(v − sᵀ·u)  (recover m: noise cancels out)
///   Correctness: v − sᵀ·u = e·r + e₂ − sᵀ·e₁ + ⌊q/2⌋·m
///                           ╰────────────────╯ small noise
///   K = KDF(G(m')‖ H(ct))
///
/// ─── Security ────────────────────────────────────────────────────────────────
///
///   Hardness: Module-LWE — given A, t=A·s+e, find s or e
///   Quantum-safe: no quantum speedup beyond Grover (√ factor on key space)
///   FO transform: IND-CCA2 secure KEM from IND-CPA PKE
///
/// ─── Parameters (educational vs real) ───────────────────────────────────────
///
///   This impl: n=16, q=3329, k=2   (fast demo, same q as real Kyber)
///   ML-KEM-512: n=256, q=3329, k=2  (NIST FIPS 203, 128-bit security)
///
/// References: CRYSTALS-Kyber spec v3.02, NIST FIPS 203 (2024)


// ─── Parameters ───────────────────────────────────────────────────────────────

pub const N:    usize = 16;   // polynomial degree (real Kyber: 256)
pub const Q:    i64   = 3329; // prime modulus (same as real Kyber)
pub const K:    usize = 2;    // module rank (Kyber512: k=2)
pub const ETA1: usize = 3;    // noise bound for keygen
pub const ETA2: usize = 2;    // noise bound for encapsulate

// Message = N bits = N/8 bytes (for N=16: 2 bytes)
pub const MSG_BYTES: usize = 2;

// ─── Polynomial ───────────────────────────────────────────────────────────────

/// Polynomial in R_q = Z_q[X]/(X^n + 1)
#[derive(Clone, Debug)]
pub struct Poly(pub [i64; N]);

impl Poly {
    pub fn zero() -> Self {
        Poly([0i64; N])
    }

    pub fn add(&self, other: &Self) -> Self {
        let mut r = [0i64; N];
        for i in 0..N {
            r[i] = (self.0[i] + other.0[i]).rem_euclid(Q);
        }
        Poly(r)
    }

    pub fn sub(&self, other: &Self) -> Self {
        let mut r = [0i64; N];
        for i in 0..N {
            r[i] = (self.0[i] - other.0[i]).rem_euclid(Q);
        }
        Poly(r)
    }

    /// Schoolbook multiplication in R_q = Z_q[X]/(X^n + 1)
    /// X^n ≡ -1  →  coefficient at index i+j≥n gets negated and wrapped
    pub fn mul(&self, other: &Self) -> Self {
        let mut r = [0i64; N];
        for i in 0..N {
            for j in 0..N {
                let c = self.0[i] * other.0[j];
                if i + j < N {
                    r[i + j] = (r[i + j] + c).rem_euclid(Q);
                } else {
                    r[i + j - N] = (r[i + j - N] - c).rem_euclid(Q);
                }
            }
        }
        Poly(r)
    }

    /// Encode message: bit 0 → 0,  bit 1 → ⌈q/2⌉  (= 1665 for q=3329)
    pub fn encode_msg(bytes: &[u8; MSG_BYTES]) -> Self {
        let half = (Q + 1) / 2; // 1665
        let mut p = Poly::zero();
        for i in 0..N {
            let bit = (bytes[i / 8] >> (i % 8)) & 1;
            p.0[i] = if bit == 1 { half } else { 0 };
        }
        p
    }

    /// Decode: bit_i = 1 if c_i ∈ [q/4, 3q/4]  (closer to q/2 than to 0)
    pub fn decode_msg(&self) -> [u8; MSG_BYTES] {
        let q4   = Q / 4;      // 832
        let q3_4 = 3 * Q / 4;  // 2497
        let mut out = [0u8; MSG_BYTES];
        for i in 0..N {
            let c = self.0[i].rem_euclid(Q);
            if c >= q4 && c <= q3_4 {
                out[i / 8] |= 1 << (i % 8);
            }
        }
        out
    }

    /// Infinity norm (centered lift to [-q/2, q/2])
    pub fn inf_norm(&self) -> i64 {
        self.0.iter().map(|&c| {
            let c = c.rem_euclid(Q);
            c.min(Q - c)
        }).max().unwrap_or(0)
    }
}

// ─── PolyVec ──────────────────────────────────────────────────────────────────

/// Vector of k polynomials
#[derive(Clone, Debug)]
pub struct PolyVec(pub Vec<Poly>);

impl PolyVec {
    pub fn zero() -> Self {
        PolyVec(vec![Poly::zero(); K])
    }

    pub fn add(&self, other: &Self) -> Self {
        PolyVec(self.0.iter().zip(&other.0).map(|(a, b)| a.add(b)).collect())
    }

    /// Inner product: Σ self_i · other_i
    pub fn dot(&self, other: &Self) -> Poly {
        let mut r = Poly::zero();
        for (a, b) in self.0.iter().zip(&other.0) {
            r = r.add(&a.mul(b));
        }
        r
    }

    /// Max infinity norm across all polynomials
    pub fn inf_norm(&self) -> i64 {
        self.0.iter().map(|p| p.inf_norm()).max().unwrap_or(0)
    }
}

// ─── PolyMatrix ───────────────────────────────────────────────────────────────

/// k×k matrix — each row is a PolyVec of k polynomials
pub struct PolyMatrix(pub Vec<PolyVec>);

impl PolyMatrix {
    /// A · v
    pub fn mul_vec(&self, v: &PolyVec) -> PolyVec {
        PolyVec(self.0.iter().map(|row| row.dot(v)).collect())
    }

    /// Aᵀ · v  :  result[j] = Σ_i A[i][j] · v[i]
    pub fn mul_vec_t(&self, v: &PolyVec) -> PolyVec {
        let mut result = PolyVec::zero();
        for (i, row) in self.0.iter().enumerate() {
            for (j, aij) in row.0.iter().enumerate() {
                result.0[j] = result.0[j].add(&aij.mul(&v.0[i]));
            }
        }
        result
    }
}

// ─── Hash helpers ─────────────────────────────────────────────────────────────

fn h256(label: &[u8], data: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(label);
    h.update(data);
    *h.finalize().as_bytes()
}

fn h256_2(label: &[u8], a: &[u8], b: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(label);
    h.update(a);
    h.update(b);
    *h.finalize().as_bytes()
}

// ─── Sampling ─────────────────────────────────────────────────────────────────

/// Expand uniform polynomial from (seed, i, j) via rejection sampling
/// Parses 3 bytes → two 12-bit values; accepts if < q
fn sample_uniform(seed: &[u8; 32], row: u8, col: u8) -> Poly {
    let mut poly = Poly::zero();
    let mut count = 0;
    let mut ctr = 0u8;

    'outer: loop {
        let buf = {
            let mut h = blake3::Hasher::new();
            h.update(b"kyber_gen_a");
            h.update(seed);
            h.update(&[row, col, ctr]);
            *h.finalize().as_bytes()
        };
        ctr = ctr.wrapping_add(1);

        let mut pos = 0;
        while pos + 2 < buf.len() {
            let b0 = buf[pos]     as i64;
            let b1 = buf[pos + 1] as i64;
            let b2 = buf[pos + 2] as i64;
            let d1 = b0 | ((b1 & 0x0f) << 8);
            let d2 = (b1 >> 4) | (b2 << 4);
            if d1 < Q { poly.0[count] = d1; count += 1; if count == N { break 'outer; } }
            if d2 < Q { poly.0[count] = d2; count += 1; if count == N { break 'outer; } }
            pos += 3;
        }
    }
    poly
}

/// Centered Binomial Distribution CBD_η
/// For each coefficient: sample η pairs of bits (a, b), output a_sum − b_sum ∈ [−η, η]
fn sample_cbd(seed: &[u8; 32], nonce: u8, eta: usize) -> Poly {
    // Need 2·η·N bits = 2·3·16 = 96 bits for eta=3, n=16 → fits in 32-byte SHA-256 output
    let buf = {
        let mut h = blake3::Hasher::new();
        h.update(b"kyber_prf");
        h.update(seed);
        h.update(&[nonce]);
        *h.finalize().as_bytes()
    };

    let get_bit = |idx: usize| -> i64 {
        ((buf[idx / 8] >> (idx % 8)) & 1) as i64
    };

    let mut p = Poly::zero();
    for i in 0..N {
        let base = i * 2 * eta;
        let a: i64 = (0..eta).map(|e| get_bit((base + e)       % (buf.len() * 8))).sum();
        let b: i64 = (0..eta).map(|e| get_bit((base + eta + e) % (buf.len() * 8))).sum();
        p.0[i] = (a - b).rem_euclid(Q);
    }
    p
}

/// Expand public matrix A from seed rho
fn expand_a(rho: &[u8; 32]) -> PolyMatrix {
    PolyMatrix((0..K).map(|i| {
        PolyVec((0..K).map(|j| sample_uniform(rho, i as u8, j as u8)).collect())
    }).collect())
}

// ─── Keys & Ciphertext ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct KyberPublicKey {
    pub rho: [u8; 32],   // seed to expand A
    pub t:   PolyVec,    // t = A·s + e  (LWE sample)
}

#[derive(Clone, Debug)]
pub struct KyberSecretKey {
    pub s:    PolyVec,    // secret polynomial vector
    pub h_pk: [u8; 32],  // H(pk) — binds shared key to public key
    pub z:    [u8; 32],  // implicit rejection randomness (unused in demo)
}

#[derive(Clone, Debug)]
pub struct KyberCiphertext {
    pub u: PolyVec, // Aᵀ·r + e₁  (encrypts randomness)
    pub v: Poly,    // tᵀ·r + e₂ + encode(m)  (encrypts message)
}

pub struct SharedKey(pub [u8; 32]);

fn hash_pk(pk: &KyberPublicKey) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"kyber_H_pk");
    h.update(&pk.rho);
    for poly in &pk.t.0 {
        for &c in &poly.0 {
            h.update(&c.to_le_bytes());
        }
    }
    *h.finalize().as_bytes()
}

fn hash_ct(ct: &KyberCiphertext) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"kyber_H_ct");
    for poly in &ct.u.0 {
        for &c in &poly.0 {
            h.update(&c.to_le_bytes());
        }
    }
    for &c in &ct.v.0 {
        h.update(&c.to_le_bytes());
    }
    *h.finalize().as_bytes()
}

// ─── KeyGen ───────────────────────────────────────────────────────────────────

pub fn keygen(seed: &[u8]) -> (KyberPublicKey, KyberSecretKey) {
    let rho   = h256(b"kyber_rho",   seed);
    let sigma = h256(b"kyber_sigma", seed);
    let z     = h256(b"kyber_z",     seed);

    // Expand public matrix A from rho
    let a = expand_a(&rho);

    // Sample secret s and error e from sigma
    let s = PolyVec((0..K).map(|i| sample_cbd(&sigma, i as u8,         ETA1)).collect());
    let e = PolyVec((0..K).map(|i| sample_cbd(&sigma, (K + i) as u8,   ETA1)).collect());

    // Public key: t = A·s + e
    let t = a.mul_vec(&s).add(&e);
    let pk = KyberPublicKey { rho, t };
    let h_pk = hash_pk(&pk);
    let sk = KyberSecretKey { s, h_pk, z };

    (pk, sk)
}

// ─── Encapsulate ──────────────────────────────────────────────────────────────

/// Bob calls this with Alice's public key.
/// Returns (ciphertext to send Alice, shared key K_bob).
pub fn encapsulate(pk: &KyberPublicKey, rand: &[u8]) -> (KyberCiphertext, SharedKey) {
    // 1. Random message m ∈ {0,1}^n  (N bits = MSG_BYTES)
    let m_full = h256(b"kyber_m", rand);
    let mut m = [0u8; MSG_BYTES];
    m.copy_from_slice(&m_full[..MSG_BYTES]);

    // 2. FO transform: (K̄, r) = G(m ‖ H(pk))
    let h_pk  = hash_pk(pk);
    let g_out = h256_2(b"kyber_G", &m, &h_pk);
    let k_bar  = h256(b"kyber_Kbar",  &g_out);
    let r_seed = h256(b"kyber_rseed", &g_out);

    // 3. Sample r, e₁, e₂ from r_seed
    let r  = PolyVec((0..K).map(|i| sample_cbd(&r_seed, i as u8,         ETA1)).collect());
    let e1 = PolyVec((0..K).map(|i| sample_cbd(&r_seed, (K + i) as u8,   ETA2)).collect());
    let e2 = sample_cbd(&r_seed, (2 * K) as u8, ETA2);

    let a = expand_a(&pk.rho);

    // 4. u = Aᵀ·r + e₁
    let u = a.mul_vec_t(&r).add(&e1);

    // 5. v = tᵀ·r + e₂ + encode(m)
    let v = pk.t.dot(&r).add(&e2).add(&Poly::encode_msg(&m));

    let ct = KyberCiphertext { u, v };

    // 6. K = KDF(K̄ ‖ H(ct))  — binds key to specific ciphertext
    let h_ct = hash_ct(&ct);
    let k = h256_2(b"kyber_KDF", &k_bar, &h_ct);

    (ct, SharedKey(k))
}

// ─── Decapsulate ──────────────────────────────────────────────────────────────

/// Alice calls this with her secret key and Bob's ciphertext.
/// Returns shared key K_alice (== K_bob if decryption correct).
pub fn decapsulate(sk: &KyberSecretKey, ct: &KyberCiphertext) -> SharedKey {
    // 1. Recover m': subtract sᵀ·u from v
    //    v − sᵀ·u = (tᵀ·r + e₂ + encode(m)) − sᵀ·(Aᵀ·r + e₁)
    //             = (A·s + e)ᵀ·r + e₂ + encode(m) − sᵀ·Aᵀ·r − sᵀ·e₁
    //             = eᵀ·r + e₂ − sᵀ·e₁ + encode(m)
    //                ╰────────────────╯ small noise → decode correctly
    let s_u    = sk.s.dot(&ct.u);
    let noisy  = ct.v.sub(&s_u);
    let m_prime = noisy.decode_msg(); // [u8; MSG_BYTES]

    // 2. Re-derive K̄ from m' ‖ H(pk)
    //    (Real Kyber: re-encapsulate m' and compare ciphertexts for CCA2 security)
    let g_out = h256_2(b"kyber_G", &m_prime, &sk.h_pk);
    let k_bar  = h256(b"kyber_Kbar", &g_out);

    // 3. K = KDF(K̄ ‖ H(ct))
    let h_ct = hash_ct(ct);
    let k = h256_2(b"kyber_KDF", &k_bar, &h_ct);

    SharedKey(k)
}

// ─── Convenience wrapper ──────────────────────────────────────────────────────

pub struct KyberKeypair {
    pub pk:      KyberPublicKey,
    pub sk:      KyberSecretKey,
    pub address: String,
}

impl KyberKeypair {
    pub fn generate(seed: &[u8]) -> Self {
        let (pk, sk) = keygen(seed);
        let h = hash_pk(&pk);
        let address = format!("kem1{}", &hex::encode(&h)[..40]);
        KyberKeypair { pk, sk, address }
    }

    pub fn encapsulate_for(&self, rand: &[u8]) -> (KyberCiphertext, SharedKey) {
        encapsulate(&self.pk, rand)
    }

    pub fn decapsulate(&self, ct: &KyberCiphertext) -> SharedKey {
        decapsulate(&self.sk, ct)
    }

    /// Expose noise for educational inspection
    pub fn inspect_noise(&self, ct: &KyberCiphertext, original_m: &[u8; MSG_BYTES]) -> i64 {
        let s_u   = self.sk.s.dot(&ct.u);
        let noisy = ct.v.sub(&s_u);
        let encoded = Poly::encode_msg(original_m);
        noisy.sub(&encoded).inf_norm()
    }
}

// ─── Parameter sets (real ML-KEM — NIST FIPS 203) ────────────────────────────

pub struct MlKemParams {
    pub name:     &'static str,
    pub n:        usize,
    pub k:        usize,
    pub q:        u32,
    pub eta1:     usize,
    pub eta2:     usize,
    pub du:       usize,  // bits per u coefficient (compression)
    pub dv:       usize,  // bits per v coefficient (compression)
    pub pk_bytes: usize,
    pub sk_bytes: usize,
    pub ct_bytes: usize,
    pub security: &'static str,
}

/// Public helper: derive first MSG_BYTES of H("kyber_m" ‖ rand)
/// Matches what encapsulate uses internally — for demo inspection only.
pub fn derive_m_bytes(rand: &[u8]) -> [u8; MSG_BYTES] {
    let full = h256(b"kyber_m", rand);
    let mut m = [0u8; MSG_BYTES];
    m.copy_from_slice(&full[..MSG_BYTES]);
    m
}

pub const MLKEM_PARAMS: [MlKemParams; 3] = [
    MlKemParams { name:"ML-KEM-512",  n:256, k:2, q:3329, eta1:3, eta2:2, du:10, dv:4,
                  pk_bytes:800,  sk_bytes:1632, ct_bytes:768,  security:"128-bit" },
    MlKemParams { name:"ML-KEM-768",  n:256, k:3, q:3329, eta1:2, eta2:2, du:10, dv:4,
                  pk_bytes:1184, sk_bytes:2400, ct_bytes:1088, security:"192-bit" },
    MlKemParams { name:"ML-KEM-1024", n:256, k:4, q:3329, eta1:2, eta2:2, du:11, dv:5,
                  pk_bytes:1568, sk_bytes:3168, ct_bytes:1568, security:"256-bit" },
];
