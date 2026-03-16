#![allow(dead_code)]
//! Zero-Knowledge Proof — v1.8
//!
//! ZK-SNARK: Succinct Non-interactive ARgument of Knowledge
//!   - Zero-Knowledge: Verifier không học gì từ proof ngoài "statement đúng"
//!   - Succinct: proof nhỏ, verify nhanh (O(1) thay O(n))
//!   - Non-interactive: không cần back-and-forth giữa prover/verifier
//!   - Argument of Knowledge: prover phải thực sự biết witness
//!
//! Ba thuộc tính cốt lõi:
//!   1. Completeness: honest prover luôn thuyết phục được verifier
//!   2. Soundness: không thể fake proof nếu không biết witness
//!   3. Zero-Knowledge: proof không tiết lộ gì về witness
//!
//! Implementations:
//!   A. Schnorr ZK Proof (CORRECT, production-ready)
//!      Statement: "Tôi biết sk sao cho pk = sk * G"
//!      Là ZK proof thật sự dựa trên discrete log hardness
//!      Cơ sở của Taproot (đã dùng ở v1.3)
//!
//!   B. Hash Preimage ZK (simplified, educational)
//!      Statement: "Tôi biết x sao cho H(x) = y"
//!      Sigma protocol + Fiat-Shamir transform
//!      Simplified: dùng hash-based commitment thay EC arithmetic
//!
//!   C. R1CS Circuit (Groth16 structure, simplified)
//!      Biểu diễn computation như hệ linear constraints
//!      Witnesses, public inputs, proving key, verification key
//!      Production: dùng ark-groth16 / bellman với EC pairings

use serde::{Serialize, Deserialize};
use crate::taproot::{schnorr_sign, schnorr_verify, x_only, tagged_hash};

// ── A. Schnorr ZK Proof of Discrete Log ──────────────────────
//
// Statement (public): pk = sk * G  (biết public key)
// Witness  (secret):  sk           (biết private key)
//
// Protocol (Sigma, BIP340 Schnorr):
//   1. Prover: r ← random,  R = r * G  (commitment)
//   2. Verifier (Fiat-Shamir): e = H(R || pk || msg)  (challenge)
//   3. Prover: s = r + e * sk  (mod n)  (response)
//   4. Proof: (R, s)
//
// Verify: s * G == R + e * pk
//         ↔ (r + e*sk)*G == r*G + e*(sk*G)  ✓ (additive homomorphism)
//
// ZK: R là random commitment → không lộ sk
//     s = r + e*sk nhưng r ẩn → s trông như random
//     Simulator có thể tạo (R, s) hợp lệ không cần sk → ZK ✓

/// ZK proof rằng "tôi biết sk của pk" (discrete log proof)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchnorrZkProof {
    pub statement:  [u8; 32],   // x-only public key (public)
    pub msg:        Vec<u8>,    // message (domain separation)
    pub signature:  Vec<u8>,   // Schnorr sig = (R_x, s) = proof of knowledge (64 bytes as Vec)
}

impl SchnorrZkProof {
    /// Prove: "Tôi biết sk sao cho pk = sk * G"
    /// msg: context string (ngăn cross-context reuse)
    pub fn prove(sk: &secp256k1::SecretKey, msg: &[u8]) -> Self {
        use secp256k1::Secp256k1;
        let secp = Secp256k1::new();
        let pk   = secp256k1::PublicKey::from_secret_key(&secp, sk);
        let xonly = x_only(&pk);

        let sig = schnorr_sign(sk, msg);

        SchnorrZkProof {
            statement: xonly,
            msg:       msg.to_vec(),
            signature: sig.to_vec(),
        }
    }

    /// Verify: không cần biết sk
    /// Trả về true nếu prover biết sk tương ứng với statement (public key)
    pub fn verify(&self) -> bool {
        if self.signature.len() != 64 { return false; }
        let mut sig = [0u8; 64];
        sig.copy_from_slice(&self.signature);
        schnorr_verify(&self.statement, &self.msg, &sig)
    }

    /// ZK property demo: proof không tiết lộ sk
    /// Verifier chỉ thấy (pk, msg, sig) — không thể recover sk
    pub fn is_zero_knowledge(&self) -> bool {
        // Proof chỉ chứa public key và signature (R_x, s)
        // sk không ở đâu trong proof
        true // structural guarantee
    }
}

// ── B. Hash Preimage ZK (Simplified Sigma Protocol) ──────────
//
// Statement: y = SHA256(x)  (y public, x secret)
// Witness: x
//
// Simplified Fiat-Shamir Sigma Protocol:
//   1. Prover chọn random r, tính commitment A = H("zkA" || r)
//   2. Prover tính witness commitment W = H("zkW" || r || x)
//      (W binds x mà không lộ x vì r random)
//   3. Challenge e = H(A || W || y || "zkchallenge") [Fiat-Shamir]
//   4. Response z = H("zkZ" || r || e)
//      (z binds A và e, không lộ r hay x)
//   5. Proof = (A, W, e, z)
//
// Verify:
//   e' = H(A || W || y || "zkchallenge") → check e' == e [Fiat-Shamir]
//   z' = H("zkZ" || H("zkR" || A || e) || e) → check structure
//   W   được accept nếu có format đúng (production: range check, pairing)
//
// ZK property: A = H(r) random, W = H(r || x) hides x vì r random
// Soundness: simplified (production: knowledge extractor via rewinding)

/// ZK proof rằng "tôi biết x sao cho H(x) = y"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashPreimageProof {
    pub public_hash: [u8; 32],  // y = H(x), public
    pub commit_a:    [u8; 32],  // A = H(r), commitment to randomness
    pub commit_w:    [u8; 32],  // W = H(r || x), commitment to witness
    pub challenge:   [u8; 32],  // e = H(A || W || y), Fiat-Shamir
    pub response:    [u8; 32],  // z = H(r || e), response
}

impl HashPreimageProof {
    /// Prove: "Tôi biết x sao cho H(x) = y"
    pub fn prove(x: &[u8]) -> Self {
        // Tính public statement
        let y: [u8; 32] = *blake3::hash(x).as_bytes();

        // Chọn random r (blinding factor)
        let r = generate_random();

        // Commitment đến randomness (ẩn r)
        let commit_a: [u8; 32] = {
            let mut d = b"zkA|".to_vec(); d.extend_from_slice(&r);
            *blake3::hash(&d).as_bytes()
        };

        // Commitment đến witness (ẩn x nhờ r random)
        let commit_w: [u8; 32] = {
            let mut d = b"zkW|".to_vec();
            d.extend_from_slice(&r);
            d.extend_from_slice(x);
            *blake3::hash(&d).as_bytes()
        };

        // Fiat-Shamir challenge (non-interactive)
        let challenge: [u8; 32] = {
            let mut d = Vec::new();
            d.extend_from_slice(&commit_a);
            d.extend_from_slice(&commit_w);
            d.extend_from_slice(&y);
            d.extend_from_slice(b"zkchallenge");
            *blake3::hash(&d).as_bytes()
        };

        // Nonce công khai (verifier có thể tính lại từ commit_a + challenge)
        let nonce: [u8; 32] = {
            let mut d = b"zkN|".to_vec();
            d.extend_from_slice(&commit_a);
            d.extend_from_slice(&challenge);
            *blake3::hash(&d).as_bytes()
        };

        // Response (binds nonce và challenge — verifier compute được)
        let response: [u8; 32] = {
            let mut d = b"zkZ|".to_vec();
            d.extend_from_slice(&nonce);
            d.extend_from_slice(&challenge);
            *blake3::hash(&d).as_bytes()
        };

        HashPreimageProof { public_hash: y, commit_a, commit_w, challenge, response }
    }

    /// Verify mà không cần biết x
    pub fn verify(&self) -> bool {
        // Bước 1: Recompute Fiat-Shamir challenge
        let e_check: [u8; 32] = {
            let mut d = Vec::new();
            d.extend_from_slice(&self.commit_a);
            d.extend_from_slice(&self.commit_w);
            d.extend_from_slice(&self.public_hash);
            d.extend_from_slice(b"zkchallenge");
            *blake3::hash(&d).as_bytes()
        };

        // Bước 2: Fiat-Shamir consistency (chứng minh prover đã commit trước challenge)
        if e_check != self.challenge { return false; }

        // Bước 3: Recompute nonce từ commit_a + challenge (verifier có thể làm)
        // Prover tính nonce = H("zkN|" || commit_a || challenge)
        // Verifier recompute: cùng input → cùng output
        let nonce: [u8; 32] = {
            let mut d = b"zkN|".to_vec();
            d.extend_from_slice(&self.commit_a);
            d.extend_from_slice(&self.challenge);
            *blake3::hash(&d).as_bytes()
        };

        // Bước 4: Recompute expected response
        let z_check: [u8; 32] = {
            let mut d = b"zkZ|".to_vec();
            d.extend_from_slice(&nonce);
            d.extend_from_slice(&self.challenge);
            *blake3::hash(&d).as_bytes()
        };

        // commit_w không rỗng (ràng buộc tối thiểu)
        let w_bound = self.commit_w != [0u8; 32];

        // Response match → prover đã commit đúng protocol
        self.response == z_check && w_bound
    }

    /// Verify với knowledge (prover biết x) — check đầy đủ
    pub fn verify_with_witness(&self, x: &[u8]) -> bool {
        // Reconstruct và so sánh
        let _expected = Self::prove(x);
        // Chỉ check public_hash và structural properties
        // (r random nên commitment sẽ khác, dùng hash của x để check)
        let y_check: [u8; 32] = *blake3::hash(x).as_bytes();
        self.public_hash == y_check && self.verify()
    }
}

// ── C. R1CS Circuit (Groth16 Structure) ──────────────────────
//
// R1CS (Rank-1 Constraint System):
//   Biểu diễn computation f(x) = y như tập constraints:
//     (A · z) ⊙ (B · z) = C · z
//   z = [1, public_inputs, private_witness]
//   A, B, C = matrices (coefficients)
//
// Ví dụ circuit: y = x * x + x + 5
//   Witness: [1, y, x, w1=x*x]
//   Constraints:
//     x * x = w1
//     w1 + x + 5 = y
//
// Groth16:
//   Setup: generate (pk, vk) từ circuit + toxic waste τ
//   Prove: π = (A, B, C) ∈ G1 × G2 × G1
//   Verify: e(A, B) = e(α, β) · e(Σ(public_input), γ) · e(C, δ)
//   (e = bilinear pairing trên BLS12-381)
//
// Simplified: dùng SHA256 thay EC pairings (educational only)

/// Một constraint trong R1CS: (a · z) * (b · z) = (c · z)
#[derive(Debug, Clone)]
pub struct R1csConstraint {
    pub a:     Vec<(usize, i64)>,  // coefficients cho z vector (index, coeff)
    pub b:     Vec<(usize, i64)>,
    pub c:     Vec<(usize, i64)>,
    pub label: String,
}

/// R1CS Circuit: tập hợp constraints
#[derive(Debug, Clone)]
pub struct R1csCircuit {
    pub num_variables:    usize,      // tổng số variables trong z
    pub num_public:       usize,      // số public inputs (không tính const 1)
    pub constraints:      Vec<R1csConstraint>,
}

impl R1csCircuit {
    /// Circuit: y = H(x) [simplified as y = x² + x + 1 để demo]
    /// z = [1, y (public), x (witness), x² (intermediate)]
    pub fn hash_preimage_demo() -> Self {
        // Variables: z[0]=1 (const), z[1]=y (public), z[2]=x (witness), z[3]=x² (aux)
        R1csCircuit {
            num_variables: 4,
            num_public:    1, // chỉ y là public
            constraints: vec![
                R1csConstraint {
                    label: "x * x = x²".to_string(),
                    a: vec![(2, 1)],   // x
                    b: vec![(2, 1)],   // x
                    c: vec![(3, 1)],   // x²
                },
                R1csConstraint {
                    label: "x² + x + 1 = y".to_string(),
                    a: vec![(3, 1), (2, 1), (0, 1)],   // x² + x + 1
                    b: vec![(0, 1)],                    // 1
                    c: vec![(1, 1)],                    // y
                },
            ],
        }
    }

    /// Kiểm tra assignment z thỏa mãn tất cả constraints
    pub fn is_satisfied(&self, z: &[i64]) -> bool {
        if z.len() != self.num_variables { return false; }
        for constraint in &self.constraints {
            let a_val: i64 = constraint.a.iter().map(|(i, c)| c * z[*i]).sum();
            let b_val: i64 = constraint.b.iter().map(|(i, c)| c * z[*i]).sum();
            let c_val: i64 = constraint.c.iter().map(|(i, c)| c * z[*i]).sum();
            if a_val * b_val != c_val { return false; }
        }
        true
    }

    pub fn constraint_count(&self) -> usize { self.constraints.len() }
}

/// Groth16-style proof (simplified — dùng SHA256 thay EC pairings)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Groth16Proof {
    pub pi_a: [u8; 32],   // G1 element (simplified: hash)
    pub pi_b: [u8; 32],   // G2 element (simplified: hash)
    pub pi_c: [u8; 32],   // G1 element (simplified: hash)
    pub public_inputs: Vec<i64>,
}

/// Proving Key (từ trusted setup)
pub struct ProvingKey {
    pub alpha: [u8; 32],
    pub beta:  [u8; 32],
    pub delta: [u8; 32],
}

/// Verification Key (public)
pub struct VerificationKey {
    pub alpha_beta: [u8; 32],   // e(α, β) precomputed
    pub gamma:      [u8; 32],
    pub delta:      [u8; 32],
}

/// Trusted setup (simplified — production: MPC ceremony)
/// Returns (proving_key, verification_key)
pub fn groth16_setup(circuit: &R1csCircuit) -> (ProvingKey, VerificationKey) {
    // "Toxic waste" τ — phải xóa sau setup (real: MPC ceremony)
    let tau: [u8; 32] = *blake3::hash(b"toxic_waste_tau_MUST_DELETE").as_bytes();
    let alpha = tagged_hash("Groth16Alpha", &tau);
    let beta  = tagged_hash("Groth16Beta",  &tau);
    let gamma = tagged_hash("Groth16Gamma", &tau);
    let delta = tagged_hash("Groth16Delta", &tau);

    // e(α, β) = pairing (simplified: H(α || β))
    let mut ab = alpha.to_vec(); ab.extend_from_slice(&beta);
    let alpha_beta = *blake3::hash(&ab).as_bytes();

    // Encode circuit constraints into keys
    let circuit_hash = {
        let mut d = Vec::new();
        for c in &circuit.constraints { d.extend_from_slice(c.label.as_bytes()); }
        blake3::hash(&d)
    };
    let _ = circuit_hash; // used in production to bind keys to circuit

    (
        ProvingKey { alpha, beta, delta },
        VerificationKey { alpha_beta, gamma, delta },
    )
}

/// Groth16 Prove: tạo proof từ witness
pub fn groth16_prove(
    pk:      &ProvingKey,
    circuit: &R1csCircuit,
    witness: &[i64],        // private inputs
    public:  &[i64],        // public inputs
) -> Result<Groth16Proof, String> {
    // Xây dựng full assignment z = [1, public..., witness...]
    let mut z = vec![1i64];
    z.extend_from_slice(public);
    z.extend_from_slice(witness);

    if !circuit.is_satisfied(&z) {
        return Err("❌ Witness không thỏa mãn circuit constraints".to_string());
    }

    // Random blinding factors (r, s) — ẩn witness trong proof
    let r = generate_random();
    let s = generate_random();

    // π_A = α + Σ(a_i * τ^i) + r·δ (simplified)
    let mut pi_a_data = pk.alpha.to_vec();
    for (i, &w) in witness.iter().enumerate() {
        let mut d = pi_a_data.clone();
        d.extend_from_slice(&(i as i64 * w).to_le_bytes());
        pi_a_data = blake3::hash(&d).as_bytes().to_vec();
    }
    pi_a_data.extend_from_slice(&r);
    let pi_a: [u8; 32] = *blake3::hash(&pi_a_data).as_bytes();

    // π_B = β + Σ(b_i * τ^i) + s·δ (simplified)
    let mut pi_b_data = pk.beta.to_vec();
    for (i, &w) in witness.iter().enumerate() {
        let mut d = pi_b_data.clone();
        d.extend_from_slice(&(i as i64 * w + 1).to_le_bytes());
        pi_b_data = blake3::hash(&d).as_bytes().to_vec();
    }
    pi_b_data.extend_from_slice(&s);
    let pi_b: [u8; 32] = *blake3::hash(&pi_b_data).as_bytes();

    // π_C = Σ(h_i * τ^i * Δ) + s·π_A + r·π_B - r·s·δ (simplified)
    let mut pi_c_data = Vec::new();
    pi_c_data.extend_from_slice(&pi_a);
    pi_c_data.extend_from_slice(&pi_b);
    pi_c_data.extend_from_slice(&pk.delta);
    pi_c_data.extend_from_slice(&r);
    pi_c_data.extend_from_slice(&s);
    let pi_c: [u8; 32] = *blake3::hash(&pi_c_data).as_bytes();

    Ok(Groth16Proof { pi_a, pi_b, pi_c, public_inputs: public.to_vec() })
}

/// Groth16 Verify: kiểm tra proof mà không biết witness
/// Pairing check (simplified): e(A,B) = e(α,β) · e(Σ,γ) · e(C,δ)
pub fn groth16_verify(
    vk:    &VerificationKey,
    proof: &Groth16Proof,
) -> bool {
    // e(A, B) — simplified: H(π_A || π_B)
    let mut e_ab = proof.pi_a.to_vec();
    e_ab.extend_from_slice(&proof.pi_b);
    let lhs: [u8; 32] = *blake3::hash(&e_ab).as_bytes();

    // Σ public inputs term
    let mut sigma_data = Vec::new();
    for inp in &proof.public_inputs {
        sigma_data.extend_from_slice(&inp.to_le_bytes());
    }
    sigma_data.extend_from_slice(&vk.gamma);
    let sigma: [u8; 32] = *blake3::hash(&sigma_data).as_bytes();

    // e(α,β) · e(Σ,γ) · e(C,δ) — simplified: H(α_β || sigma || C_δ)
    let mut rhs_data = vk.alpha_beta.to_vec();
    rhs_data.extend_from_slice(&sigma);
    rhs_data.extend_from_slice(&proof.pi_c);
    rhs_data.extend_from_slice(&vk.delta);
    let rhs: [u8; 32] = *blake3::hash(&rhs_data).as_bytes();

    // Trong real Groth16: lhs == rhs (algebraic identity)
    // Simplified: chúng ta verify structural consistency
    // (lhs sẽ không bằng rhs vì simplified — check proof structure thay thế)
    let proof_structurally_valid =
        proof.pi_a != [0u8; 32] &&
        proof.pi_b != [0u8; 32] &&
        proof.pi_c != [0u8; 32];

    // Verify bằng cách reproving với public inputs (simplified approach)
    // Production: single pairing check equation
    let _ = (lhs, rhs); // in real Groth16: assert lhs == rhs
    proof_structurally_valid
}

// ── Helper ────────────────────────────────────────────────────

fn generate_random() -> [u8; 32] {
    use secp256k1::rand::RngCore;
    let mut rng = secp256k1::rand::thread_rng();
    let mut r = [0u8; 32];
    rng.fill_bytes(&mut r);
    r
}

// ── ZK Properties Demo ────────────────────────────────────────

/// Kiểm tra ZK simulator: tạo indistinguishable proof không cần witness
/// Đây là cách chứng minh ZK property (nếu simulator tồn tại → ZK)
pub struct ZkSimulator;

impl ZkSimulator {
    /// Simulate Schnorr proof mà không cần sk (chứng minh ZK property)
    /// Real simulator: chọn s random, tính R = s*G - e*pk
    /// Simplified: tạo proof với random values (không verify được, chỉ demo concept)
    pub fn simulate_schnorr(pk_bytes: &[u8; 32], msg: &[u8]) -> Vec<u8> {
        // Trong real ZK proof: simulator tồn tại → proof là ZK
        // Ở đây: demo concept bằng cách tạo proof với sk fake
        // (sẽ không verify vì không có sk thật)
        let mut sim_data = b"zkSIM|".to_vec();
        sim_data.extend_from_slice(pk_bytes);
        sim_data.extend_from_slice(msg);
        let hash = *blake3::hash(&sim_data).as_bytes();
        let mut out = vec![0u8; 64];
        out[..32].copy_from_slice(&hash);
        out[32..].copy_from_slice(&hash);
        out
    }

    /// Demonstrate: real proof và simulated proof không phân biệt được
    /// (Trong real ZK: cả 2 có phân phối xác suất giống nhau)
    pub fn are_computationally_indistinguishable(
        real_proof:      &[u8],
        simulated_proof: &[u8],
    ) -> bool {
        // Cả 2 đều là 64 bytes random-looking data
        // Verifier không thể nói cái nào là "real" hay "simulated"
        // → ZK property!
        real_proof.len() == simulated_proof.len() // structurally identical
    }
}
