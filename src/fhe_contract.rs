#![allow(dead_code)]

/// v3.7 — Privacy Smart Contract (FHE)
///
/// Simplified BFV/LWE-style Fully Homomorphic Encryption.
/// Contracts compute over encrypted data — the chain never sees plaintext.
///
/// ─── LWE Encryption Scheme ───────────────────────────────────────────────────
///
///   Params: n=8 (secret dim), k=16 (pk rows), q=65537, t=64, Δ=q/t=1024
///
///   Keygen:
///     sk:  s ∈ {0,1}^n   (binary secret)
///     pk:  A ∈ Z_q^{k×n},  b = A·s + e  where e_i ∈ [-B, B]
///
///   Encrypt(pk, m ∈ [0,t)):
///     r ∈ {0,1}^k  (random selector)
///     u = A^T · r  ∈ Z_q^n
///     v = b^T · r + Δ·m  ∈ Z_q
///     ct = (u, v)
///
///   Decrypt(sk, ct=(u,v)):
///     phase = v - u^T·s  (mod q)  ≈ Δ·m + noise
///     m = round(phase · t / q)  mod t
///
///   Correctness:
///     phase = b^T·r + Δ·m - (A^T·r)^T·s
///           = r^T·(b - A·s) + Δ·m
///           = r^T·e + Δ·m   (≈ Δ·m if |noise| < Δ/2 = 512)
///
/// ─── Homomorphic Operations ──────────────────────────────────────────────────
///
///   HE-ADD(ct1, ct2):   (u1+u2, v1+v2) → decrypts to m1+m2 (mod t)
///   HE-ADD-PLAIN(ct, m): (u, v + Δ·m)  → decrypts to ct.m + m
///   HE-MUL-PLAIN(ct, c): (c·u, c·v)    → decrypts to c·m (mod t)
///   HE-MUL(ct1, ct2): requires relinearization key — not implemented here
///
/// ─── Noise Budget ────────────────────────────────────────────────────────────
///
///   Initial noise: |r^T·e| ≤ k·B = 16 (k=16, B=1)
///   After n adds:  noise ≤ n·16
///   Budget = Δ/2 - noise = 512 - n·16  (safe for ~31 additions)
///   mul_plain(c): budget divides by c
///
/// ─── Privacy Contract Model ──────────────────────────────────────────────────
///
///   User encrypts input locally, sends ciphertext to chain
///   Contract operates on ciphertexts (no plaintext ever seen)
///   Authorized party decrypts final output
///   Chain observes: encrypted inputs, encrypted outputs, operations — no values
///
/// References: Regev LWE (2009), BFV scheme (2012), TFHE, Zama concrete

use sha2::{Sha256, Digest};

// ─── Parameters ───────────────────────────────────────────────────────────────

pub const FHE_N: usize = 8;    // LWE secret key dimension
pub const FHE_K: usize = 16;   // public key samples (more = more security)
pub const FHE_Q: i64  = 65537; // ciphertext modulus (Fermat prime 2^16 + 1)
pub const FHE_T: i64  = 64;    // plaintext modulus — supports values 0..63
pub const FHE_DELTA: i64 = FHE_Q / FHE_T;  // 1024 — scaling factor
pub const FHE_B: i64  = 1;     // key generation noise bound |e| ≤ B

// ─── Types ────────────────────────────────────────────────────────────────────

pub type Scalar = i64;
pub type SecVec = [Scalar; FHE_N];
pub type PkRow  = [Scalar; FHE_N];

/// Secret key: binary vector s ∈ {0,1}^N
pub struct FheSecretKey {
    pub s: SecVec,
}

/// Public key: k-row matrix A and vector b = A·s + e
pub struct FhePublicKey {
    pub a: Vec<PkRow>,        // k × N matrix
    pub b: Vec<Scalar>,       // k-dim vector
}

/// Ciphertext: (u ∈ Z_q^N, v ∈ Z_q)
#[derive(Clone, Debug)]
pub struct FheCiphertext {
    pub u: SecVec,
    pub v: Scalar,
}

// ─── Math helpers ─────────────────────────────────────────────────────────────

fn mq(x: i64) -> i64 {
    x.rem_euclid(FHE_Q)
}

fn dot(a: &SecVec, b: &SecVec) -> i64 {
    mq(a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<i64>())
}

fn dot_k(a: &[Scalar], b: &[Scalar]) -> i64 {
    mq(a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<i64>())
}

fn vec_add(a: &SecVec, b: &SecVec) -> SecVec {
    let mut r = [0i64; FHE_N];
    for i in 0..FHE_N { r[i] = mq(a[i] + b[i]); }
    r
}

fn vec_scale(a: &SecVec, c: i64) -> SecVec {
    let mut r = [0i64; FHE_N];
    for i in 0..FHE_N { r[i] = mq(a[i] * c); }
    r
}

// ─── Pseudo-random generation ─────────────────────────────────────────────────

/// Deterministic PRG: H(seed ‖ counter) → integer in [0, range)
fn prg(seed: &[u8], counter: u64, range: i64) -> i64 {
    let mut h = Sha256::new();
    h.update(seed);
    h.update(&counter.to_le_bytes());
    let out = h.finalize();
    let v = i64::from_le_bytes(out[..8].try_into().unwrap()).abs();
    v % range
}

/// Sample from {-B, ..., B} using PRG
fn sample_noise(seed: &[u8], counter: u64) -> i64 {
    prg(seed, counter, 2 * FHE_B + 1) - FHE_B
}

/// Sample binary {0, 1} using PRG
fn sample_binary(seed: &[u8], counter: u64) -> i64 {
    prg(seed, counter, 2)
}

/// Sample from Z_q using PRG
fn sample_zq(seed: &[u8], counter: u64) -> i64 {
    prg(seed, counter, FHE_Q)
}

// ─── Key generation ───────────────────────────────────────────────────────────

pub struct FheKeypair {
    pub sk: FheSecretKey,
    pub pk: FhePublicKey,
}

pub fn keygen(seed: &[u8]) -> FheKeypair {
    let mut ctr: u64 = 0;

    // Secret key: binary vector
    let mut s = [0i64; FHE_N];
    for i in 0..FHE_N {
        s[i] = sample_binary(seed, ctr);
        ctr += 1;
    }

    // Public key: k rows of (a_i, b_i = <a_i, s> + e_i)
    let mut a_rows: Vec<PkRow> = Vec::with_capacity(FHE_K);
    let mut b_vec:  Vec<Scalar> = Vec::with_capacity(FHE_K);

    for _row in 0..FHE_K {
        let mut a = [0i64; FHE_N];
        for j in 0..FHE_N {
            a[j] = sample_zq(seed, ctr);
            ctr += 1;
        }
        let noise = sample_noise(seed, ctr); ctr += 1;
        let b = mq(dot(&a, &s) + noise);
        a_rows.push(a);
        b_vec.push(b);
    }

    FheKeypair {
        sk: FheSecretKey { s },
        pk: FhePublicKey { a: a_rows, b: b_vec },
    }
}

// ─── Encryption ───────────────────────────────────────────────────────────────

pub fn encrypt(pk: &FhePublicKey, m: i64, seed: &[u8]) -> FheCiphertext {
    assert!(m >= 0 && m < FHE_T, "plaintext must be in [0, {FHE_T})");
    let mut ctr: u64 = 0;

    // Random binary selector r ∈ {0,1}^K
    let mut r = vec![0i64; FHE_K];
    for i in 0..FHE_K {
        r[i] = sample_binary(seed, ctr);
        ctr += 1;
    }

    // u = A^T · r  (N-dimensional)
    let mut u = [0i64; FHE_N];
    for j in 0..FHE_N {
        let col: Vec<i64> = pk.a.iter().map(|row| row[j]).collect();
        u[j] = mq(dot_k(&col, &r));
    }

    // v = b^T · r + Δ·m
    let v = mq(dot_k(&pk.b, &r) + FHE_DELTA * m);

    FheCiphertext { u, v }
}

// ─── Decryption ───────────────────────────────────────────────────────────────

pub fn decrypt(sk: &FheSecretKey, ct: &FheCiphertext) -> i64 {
    let phase = mq(ct.v - dot(&ct.u, &sk.s));
    // round(phase * T / Q) mod T
    let scaled = (phase as f64 * FHE_T as f64 / FHE_Q as f64).round() as i64;
    scaled.rem_euclid(FHE_T)
}

/// Returns (plaintext, noise) — noise should stay below Δ/2 = 512
pub fn decrypt_with_noise(sk: &FheSecretKey, ct: &FheCiphertext) -> (i64, i64) {
    let phase = mq(ct.v - dot(&ct.u, &sk.s));
    let m = (phase as f64 * FHE_T as f64 / FHE_Q as f64).round() as i64;
    let m = m.rem_euclid(FHE_T);
    // Centered phase
    let phase_c = if phase > FHE_Q / 2 { phase - FHE_Q } else { phase };
    let noise = phase_c - FHE_DELTA * m;
    (m, noise)
}

/// Remaining noise budget (positive = safe, 0 or negative = corrupted)
pub fn noise_budget(sk: &FheSecretKey, ct: &FheCiphertext) -> i64 {
    let (_, noise) = decrypt_with_noise(sk, ct);
    FHE_DELTA / 2 - noise.abs()
}

// ─── Homomorphic Operations ───────────────────────────────────────────────────

/// HE-ADD: ct1 + ct2 → encrypts m1+m2 (mod T)
pub fn he_add(ct1: &FheCiphertext, ct2: &FheCiphertext) -> FheCiphertext {
    FheCiphertext {
        u: vec_add(&ct1.u, &ct2.u),
        v: mq(ct1.v + ct2.v),
    }
}

/// HE-ADD-PLAIN: ct + m_plain → encrypts ct.m + m_plain
pub fn he_add_plain(ct: &FheCiphertext, m_plain: i64) -> FheCiphertext {
    FheCiphertext {
        u: ct.u,
        v: mq(ct.v + FHE_DELTA * m_plain),
    }
}

/// HE-MUL-PLAIN: ct * c → encrypts c · ct.m (mod T)
/// Noise grows by factor c — use small c values
pub fn he_mul_plain(ct: &FheCiphertext, c: i64) -> FheCiphertext {
    FheCiphertext {
        u: vec_scale(&ct.u, c),
        v: mq(ct.v * c),
    }
}

/// HE-SUB: ct1 - ct2 → encrypts m1-m2 (mod T)
pub fn he_sub(ct1: &FheCiphertext, ct2: &FheCiphertext) -> FheCiphertext {
    let neg_u = vec_scale(&ct2.u, FHE_Q - 1);  // -u2 mod q
    let neg_v = mq(FHE_Q - ct2.v);             // -v2 mod q
    FheCiphertext {
        u: vec_add(&ct1.u, &neg_u),
        v: mq(ct1.v + neg_v),
    }
}

// ─── Privacy Contracts ────────────────────────────────────────────────────────

/// Privacy-preserving vote counter
/// Inputs: encrypted ballots (0=against, 1=for)
/// Output: encrypted tally → only key holder learns the result
pub struct EncryptedVoteContract {
    pub title:       String,
    pub tally:       Option<FheCiphertext>,
    pub vote_count:  usize,
    pub max_votes:   usize,
    pub closed:      bool,
}

impl EncryptedVoteContract {
    pub fn new(title: &str, max_votes: usize) -> Self {
        EncryptedVoteContract {
            title: title.to_string(),
            tally: None,
            vote_count: 0,
            max_votes,
            closed: false,
        }
    }

    /// Cast an encrypted vote (0=no, 1=yes) — chain only sees ciphertext
    pub fn cast_vote(&mut self, pk: &FhePublicKey, vote: i64, seed: &[u8]) -> Result<(), String> {
        if self.closed {
            return Err("Voting closed".to_string());
        }
        if self.vote_count >= self.max_votes {
            return Err("Max votes reached".to_string());
        }
        let ct = encrypt(pk, vote, seed);
        self.tally = Some(match &self.tally {
            None    => ct,
            Some(t) => he_add(t, &ct),
        });
        self.vote_count += 1;
        Ok(())
    }

    /// Cast a pre-encrypted vote (user encrypts off-chain)
    pub fn cast_encrypted_vote(&mut self, ct: FheCiphertext) -> Result<(), String> {
        if self.closed {
            return Err("Voting closed".to_string());
        }
        self.tally = Some(match &self.tally {
            None    => ct,
            Some(t) => he_add(t, &ct),
        });
        self.vote_count += 1;
        Ok(())
    }

    pub fn close(&mut self) { self.closed = true; }

    /// Reveal tally — only the key holder can do this
    pub fn reveal(&self, sk: &FheSecretKey) -> Option<i64> {
        self.tally.as_ref().map(|t| decrypt(sk, t))
    }
}

/// Privacy-preserving payroll: compute total salary sum without revealing individual salaries
pub struct SalaryContract {
    pub company:      String,
    pub salary_sum:   Option<FheCiphertext>,
    pub employee_count: usize,
}

impl SalaryContract {
    pub fn new(company: &str) -> Self {
        SalaryContract { company: company.to_string(), salary_sum: None, employee_count: 0 }
    }

    /// Employee submits encrypted salary — HR never sees individual amounts
    pub fn submit_salary(&mut self, pk: &FhePublicKey, salary: i64, seed: &[u8]) {
        let ct = encrypt(pk, salary, seed);
        self.salary_sum = Some(match &self.salary_sum {
            None    => ct,
            Some(s) => he_add(s, &ct),
        });
        self.employee_count += 1;
    }

    /// Submit pre-encrypted salary
    pub fn submit_encrypted(&mut self, ct: FheCiphertext) {
        self.salary_sum = Some(match &self.salary_sum {
            None    => ct,
            Some(s) => he_add(s, &ct),
        });
        self.employee_count += 1;
    }

    /// Decrypt total (authorized party only)
    pub fn total(&self, sk: &FheSecretKey) -> Option<i64> {
        self.salary_sum.as_ref().map(|s| decrypt(sk, s))
    }

    /// Compute average: divide total by employee count (plaintext scalar division)
    /// Note: integer division of the plaintext result
    pub fn average(&self, sk: &FheSecretKey) -> Option<i64> {
        if self.employee_count == 0 { return None; }
        self.total(sk).map(|t| t / self.employee_count as i64)
    }
}

/// Sealed-bid auction: bids encrypted, winner determined without revealing losing bids
/// Simplified: we compare by decrypting only after all bids are in
pub struct SealedBidAuction {
    pub item:          String,
    pub encrypted_bids: Vec<(String, FheCiphertext)>,  // (bidder, encrypted_bid)
    pub closed:        bool,
}

impl SealedBidAuction {
    pub fn new(item: &str) -> Self {
        SealedBidAuction { item: item.to_string(), encrypted_bids: Vec::new(), closed: false }
    }

    pub fn submit_bid(&mut self, bidder: &str, pk: &FhePublicKey, bid: i64, seed: &[u8]) -> Result<(), String> {
        if self.closed {
            return Err("Auction closed".to_string());
        }
        let ct = encrypt(pk, bid, seed);
        self.encrypted_bids.push((bidder.to_string(), ct));
        Ok(())
    }

    pub fn close(&mut self) { self.closed = true; }

    /// Reveal results — decrypt all bids to find winner
    /// In a real system, this uses threshold decryption or MPC
    pub fn reveal_winner(&self, sk: &FheSecretKey) -> Option<(String, i64)> {
        if !self.closed { return None; }
        self.encrypted_bids.iter()
            .map(|(bidder, ct)| (bidder.clone(), decrypt(sk, ct)))
            .max_by_key(|(_, bid)| *bid)
    }

    pub fn reveal_all(&self, sk: &FheSecretKey) -> Vec<(String, i64)> {
        self.encrypted_bids.iter()
            .map(|(bidder, ct)| (bidder.clone(), decrypt(sk, ct)))
            .collect()
    }
}

// ─── Noise tracking display ───────────────────────────────────────────────────

pub fn noise_bar(budget: i64) -> String {
    let max = FHE_DELTA / 2;  // 512
    let pct = (budget * 100 / max).max(0).min(100);
    let filled = (pct / 10) as usize;
    let bar = "█".repeat(filled) + &"░".repeat(10 - filled);
    format!("{} {}% ({}/{})", bar, pct, budget, max)
}
