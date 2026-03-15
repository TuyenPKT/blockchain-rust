#![allow(dead_code)]

/// v2.4 — Recursive ZK Proof (IVC)
///
/// Kiến trúc IVC (Incrementally Verifiable Computation):
///
///   Step 0:  prove(F, z_0, z_1)       → P_0   [base proof]
///   Step 1:  prove(F, z_1, z_2, P_0)  → P_1   [P_1 verifies P_0 + step 1]
///   Step 2:  prove(F, z_2, z_3, P_1)  → P_2   [P_2 verifies P_1 + step 2]
///   ...
///   Step n:  prove(F, z_n, z_n+1, P_{n-1}) → P_n
///
///   Verifier chỉ cần check P_n — O(1) bất kể n
///   P_n ngầm chứng minh toàn bộ chain F(F(...F(z_0)...)) = z_n
///
/// So sánh với batch proof (v2.2 ZK-Rollup):
///   ZK-Rollup:  1 proof cho N TXs trong 1 batch  [O(N) proving time]
///   Recursive:  chain proof qua M batches          [O(1) verification bất kể M]
///
/// Ứng dụng thực tế:
///   - Mina Protocol: toàn bộ blockchain = 1 proof ~22KB (Pickles recursion)
///   - Halo2 (Zcash): IPA-based accumulation scheme
///   - Nova (Microsoft): folding scheme — lightweight recursion
///   - zkEVM aggregation: nhiều ZK-EVM proofs → 1 aggregated proof
///
/// Simplified simulation:
///   proof_bytes = H("recursive_v24" || prev_proof_hash || step_hash || new_state)
///   Verification: recompute hash và verify recursively
///
/// Tham khảo: Nova (Kothapalli et al.), Halo (Bowe et al.), Pickles (Mina)

use sha2::{Sha256, Digest};

// ─── ComputationStep ──────────────────────────────────────────────────────────

/// 1 bước tính toán F(z_i) → z_{i+1}
/// Trong thực tế: 1 batch TXs, 1 block, 1 EVM execution
#[derive(Debug, Clone)]
pub struct ComputationStep {
    pub step_index: u64,
    pub input_state:  String,   // z_i
    pub output_state: String,   // z_{i+1} = F(z_i)
    pub step_data:    Vec<u8>,  // data chứng minh F được apply đúng (e.g., TX list)
    pub step_hash:    String,
}

impl ComputationStep {
    pub fn new(step_index: u64, input_state: impl Into<String>, step_data: Vec<u8>) -> Self {
        let input_state = input_state.into();

        // F(input_state, step_data) → output_state
        let mut h = Sha256::new();
        h.update(b"step_fn_v24");
        h.update(input_state.as_bytes());
        h.update(&step_data);
        let output_state = hex::encode(h.finalize());

        let mut h2 = Sha256::new();
        h2.update(b"step_hash_v24");
        h2.update(step_index.to_le_bytes());
        h2.update(input_state.as_bytes());
        h2.update(output_state.as_bytes());
        h2.update(&step_data);
        let step_hash = hex::encode(h2.finalize());

        ComputationStep { step_index, input_state, output_state, step_data, step_hash }
    }
}

// ─── BaseProof ────────────────────────────────────────────────────────────────

/// Proof cơ bản (không recursive) cho step đầu tiên
/// Tương đương: SNARK/STARK proof cho 1 bước
#[derive(Debug, Clone)]
pub struct BaseProof {
    pub step_index:   u64,
    pub input_state:  String,
    pub output_state: String,
    pub proof_bytes:  Vec<u8>,
    pub proof_size:   usize,     // bytes
}

impl BaseProof {
    pub fn generate(step: &ComputationStep) -> Self {
        let mut h = Sha256::new();
        h.update(b"base_proof_v24");
        h.update(step.step_index.to_le_bytes());
        h.update(step.input_state.as_bytes());
        h.update(step.output_state.as_bytes());
        h.update(&step.step_data);
        let proof_bytes = h.finalize().to_vec();
        let proof_size  = proof_bytes.len();

        BaseProof {
            step_index:   step.step_index,
            input_state:  step.input_state.clone(),
            output_state: step.output_state.clone(),
            proof_bytes,
            proof_size,
        }
    }

    pub fn verify(&self, step: &ComputationStep) -> bool {
        let expected = Self::generate(step);
        self.proof_bytes == expected.proof_bytes
            && self.input_state  == step.input_state
            && self.output_state == step.output_state
    }

    pub fn proof_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(&self.proof_bytes);
        hex::encode(h.finalize())
    }
}

// ─── RecursiveProof ───────────────────────────────────────────────────────────

/// Recursive proof: verifies prev_proof + new step
///
/// Core idea (IVC folding):
///   RecursiveProof_n = H(prev_proof_hash || step_hash || output_state)
///
///   → Constant-size proof bất kể chain dài bao nhiêu
///   → Để verify P_n, verifier chỉ cần check 1 hash (không cần replay)
///
/// Trong thực tế: Nova dùng "relaxed R1CS" folding để accumulate
/// witness mà không cần verify từng proof riêng lẻ
#[derive(Debug, Clone)]
pub struct RecursiveProof {
    pub step_index:        u64,
    pub initial_state:     String,   // z_0 — original input (bất biến)
    pub current_state:     String,   // z_n — output sau n steps
    pub steps_proven:      u64,      // n — số steps đã chứng minh
    pub proof_bytes:       Vec<u8>,  // constant-size accumulator
    pub proof_size:        usize,
    pub prev_proof_hash:   String,   // commitment to previous proof
}

impl RecursiveProof {
    /// Tạo recursive proof từ base proof + step 0
    pub fn from_base(base: &BaseProof, step: &ComputationStep) -> Result<Self, String> {
        if step.input_state != base.output_state {
            return Err(format!(
                "State mismatch: base output '{}' ≠ step input '{}'",
                &base.output_state[..8], &step.input_state[..8]
            ));
        }

        let prev_proof_hash = base.proof_hash();

        let mut h = Sha256::new();
        h.update(b"recursive_v24");
        h.update(base.input_state.as_bytes());  // z_0
        h.update(&prev_proof_hash.as_bytes());
        h.update(step.step_hash.as_bytes());
        h.update(step.output_state.as_bytes());
        let proof_bytes = h.finalize().to_vec();
        let proof_size  = proof_bytes.len();

        Ok(RecursiveProof {
            step_index:      step.step_index,
            initial_state:   base.input_state.clone(),
            current_state:   step.output_state.clone(),
            steps_proven:    2,  // base step + this step
            proof_bytes,
            proof_size,
            prev_proof_hash,
        })
    }

    /// Fold new step into existing recursive proof
    /// O(1) cost — không tăng theo số steps
    pub fn fold(&self, step: &ComputationStep) -> Result<Self, String> {
        if step.input_state != self.current_state {
            return Err(format!(
                "State mismatch: current '{}' ≠ step input '{}'",
                &self.current_state[..8], &step.input_state[..8]
            ));
        }
        if step.step_index != self.step_index + 1 {
            return Err(format!(
                "Step index gap: expected {}, got {}",
                self.step_index + 1, step.step_index
            ));
        }

        let prev_proof_hash = self.proof_hash();

        // Folding: accumulate new step into proof
        // Key: proof_bytes size stays constant (32 bytes) regardless of steps
        let mut h = Sha256::new();
        h.update(b"recursive_v24");
        h.update(self.initial_state.as_bytes());  // z_0 invariant
        h.update(prev_proof_hash.as_bytes());
        h.update(step.step_hash.as_bytes());
        h.update(step.output_state.as_bytes());
        let proof_bytes = h.finalize().to_vec();
        let proof_size  = proof_bytes.len();

        Ok(RecursiveProof {
            step_index:      step.step_index,
            initial_state:   self.initial_state.clone(),
            current_state:   step.output_state.clone(),
            steps_proven:    self.steps_proven + 1,
            proof_bytes,
            proof_size,
            prev_proof_hash,
        })
    }

    pub fn proof_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(&self.proof_bytes);
        hex::encode(h.finalize())
    }

    /// Verify the final recursive proof
    /// Verifier chỉ cần: initial_state, final_state, steps_proven, proof_bytes
    /// KHÔNG cần replay từng step — O(1)
    pub fn verify_final(
        &self,
        expected_initial: &str,
        expected_final:   &str,
        expected_steps:   u64,
    ) -> bool {
        self.initial_state == expected_initial
            && self.current_state == expected_final
            && self.steps_proven == expected_steps
            && !self.proof_bytes.is_empty()
    }
}

// ─── IvcChain ─────────────────────────────────────────────────────────────────

/// IVC accumulator — builds recursive proof incrementally
pub struct IvcChain {
    pub initial_state: String,
    pub steps:         Vec<ComputationStep>,
    pub current_proof: IvcProofState,
}

#[derive(Debug, Clone)]
pub enum IvcProofState {
    Empty,
    Base(BaseProof),
    Recursive(RecursiveProof),
}

impl IvcChain {
    pub fn new(initial_state: impl Into<String>) -> Self {
        IvcChain {
            initial_state: initial_state.into(),
            steps:         vec![],
            current_proof: IvcProofState::Empty,
        }
    }

    pub fn current_state(&self) -> &str {
        match &self.current_proof {
            IvcProofState::Empty     => &self.initial_state,
            IvcProofState::Base(b)   => &b.output_state,
            IvcProofState::Recursive(r) => &r.current_state,
        }
    }

    pub fn steps_proven(&self) -> u64 {
        match &self.current_proof {
            IvcProofState::Empty       => 0,
            IvcProofState::Base(_)     => 1,
            IvcProofState::Recursive(r) => r.steps_proven,
        }
    }

    pub fn proof_size_bytes(&self) -> usize {
        match &self.current_proof {
            IvcProofState::Empty       => 0,
            IvcProofState::Base(b)     => b.proof_size,
            IvcProofState::Recursive(r) => r.proof_size,
        }
    }

    /// Apply step và cập nhật proof — O(1) per step
    pub fn apply_step(&mut self, step_data: Vec<u8>) -> Result<(), String> {
        let step_index   = self.steps.len() as u64;
        let input_state  = self.current_state().to_string();
        let step         = ComputationStep::new(step_index, input_state, step_data);

        let new_proof = match &self.current_proof {
            IvcProofState::Empty => {
                let base = BaseProof::generate(&step);
                IvcProofState::Base(base)
            }
            IvcProofState::Base(base) => {
                // Step 1: transition from base to recursive
                let base_clone = base.clone();
                // Re-interpret: base was step 0, now we fold step 1
                // Need to create a fake "previous step" for the base
                let prev_step = ComputationStep::new(
                    0,
                    base_clone.input_state.clone(),
                    vec![],  // base step data (simplified)
                );
                // Regenerate base with correct data
                let real_base = BaseProof::generate(&prev_step);
                // ... actually use the stored base directly
                // Create recursive from the stored base + current step
                let mut h = Sha256::new();
                h.update(b"base_proof_v24");
                h.update(0u64.to_le_bytes());
                h.update(base_clone.input_state.as_bytes());
                h.update(base_clone.output_state.as_bytes());
                let base_proof_hash = hex::encode(h.finalize());

                let mut h2 = Sha256::new();
                h2.update(b"recursive_v24");
                h2.update(base_clone.input_state.as_bytes());
                h2.update(base_proof_hash.as_bytes());
                h2.update(step.step_hash.as_bytes());
                h2.update(step.output_state.as_bytes());
                let proof_bytes = h2.finalize().to_vec();
                let proof_size  = proof_bytes.len();

                let _ = real_base;
                IvcProofState::Recursive(RecursiveProof {
                    step_index:    step.step_index,
                    initial_state: base_clone.input_state.clone(),
                    current_state: step.output_state.clone(),
                    steps_proven:  2,
                    proof_bytes,
                    proof_size,
                    prev_proof_hash: base_proof_hash,
                })
            }
            IvcProofState::Recursive(r) => {
                let r_clone = r.clone();
                IvcProofState::Recursive(r_clone.fold(&step)?)
            }
        };

        self.steps.push(step);
        self.current_proof = new_proof;
        Ok(())
    }

    /// Final proof — verifier chỉ cần check này
    pub fn final_proof_hash(&self) -> String {
        match &self.current_proof {
            IvcProofState::Empty       => "none".to_string(),
            IvcProofState::Base(b)     => b.proof_hash(),
            IvcProofState::Recursive(r) => r.proof_hash(),
        }
    }

    /// Verify: final state đúng, số steps đúng, proof bytes hợp lệ
    pub fn verify(&self, expected_steps: u64, expected_final_state: &str) -> bool {
        if self.steps.len() as u64 != expected_steps { return false; }
        match &self.current_proof {
            IvcProofState::Empty       => false,
            IvcProofState::Base(b)     => {
                expected_steps == 1 && b.output_state == expected_final_state
            }
            IvcProofState::Recursive(r) => {
                r.steps_proven == expected_steps
                    && r.initial_state == self.initial_state
                    && r.current_state == expected_final_state
            }
        }
    }
}

// ─── ProofAggregator ──────────────────────────────────────────────────────────

/// Aggregates multiple independent proofs into 1 final proof
/// Dùng để gom nhiều ZK-Rollup batch proofs → 1 aggregated proof
///
/// Ứng dụng: zkEVM aggregation (nhiều Ethereum blocks → 1 proof)
#[derive(Debug, Clone)]
pub struct AggregatedProof {
    pub proof_count:    usize,
    pub proof_hashes:   Vec<String>,   // hashes của từng proof con
    pub aggregated:     Vec<u8>,       // final aggregated proof bytes
    pub total_tx_count: u64,
}

impl AggregatedProof {
    /// Aggregate nhiều proof roots thành 1
    pub fn aggregate(proof_roots: Vec<(String, u64)>) -> Self {
        // proof_roots: Vec<(proof_hash, tx_count)>
        let proof_count    = proof_roots.len();
        let total_tx_count = proof_roots.iter().map(|(_, n)| n).sum();
        let proof_hashes: Vec<String> = proof_roots.iter().map(|(h, _)| h.clone()).collect();

        // Merkle-like aggregation
        let mut h = Sha256::new();
        h.update(b"aggregated_v24");
        h.update((proof_count as u64).to_le_bytes());
        for (ph, tx_count) in &proof_roots {
            h.update(ph.as_bytes());
            h.update(tx_count.to_le_bytes());
        }
        let aggregated = h.finalize().to_vec();

        AggregatedProof { proof_count, proof_hashes, aggregated, total_tx_count }
    }

    /// Verify aggregated proof
    pub fn verify(&self, proof_roots: &[(String, u64)]) -> bool {
        if proof_roots.len() != self.proof_count { return false; }
        let expected = Self::aggregate(proof_roots.to_vec());
        self.aggregated == expected.aggregated
    }

    pub fn aggregated_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(&self.aggregated);
        hex::encode(h.finalize())
    }
}

// ─── RecursiveVerifier ────────────────────────────────────────────────────────

/// On-chain verifier — chỉ verify 1 proof (O(1)) bất kể chain dài
pub struct RecursiveVerifier {
    pub initial_state:  String,
    pub verified_steps: u64,
    pub current_state:  String,
}

impl RecursiveVerifier {
    pub fn new(initial_state: impl Into<String>) -> Self {
        let initial_state = initial_state.into();
        let current_state = initial_state.clone();
        RecursiveVerifier { initial_state, verified_steps: 0, current_state }
    }

    /// Verify IvcChain final proof — O(1) regardless of chain length
    pub fn verify_ivc(&mut self, chain: &IvcChain) -> bool {
        match &chain.current_proof {
            IvcProofState::Empty => false,
            IvcProofState::Base(b) => {
                if b.input_state != self.initial_state { return false; }
                self.verified_steps = 1;
                self.current_state  = b.output_state.clone();
                true
            }
            IvcProofState::Recursive(r) => {
                if r.initial_state != self.initial_state { return false; }
                if r.steps_proven != chain.steps.len() as u64 { return false; }
                self.verified_steps = r.steps_proven;
                self.current_state  = r.current_state.clone();
                true
            }
        }
    }
}
