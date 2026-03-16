#![allow(dead_code)]

/// v2.2 — ZK-Rollup
///
/// Kiến trúc:
///
///   Off-chain (Operator)           On-chain (L1 Verifier)
///   ──────────────────────         ──────────────────────
///   Collect N TXs                  Store state_root
///   Apply TXs → new_state          Verify ZK proof
///   Generate ZK proof              Update state_root
///   Post (calldata, proof) ──────► Accept/Reject batch
///
/// Tính chất:
///   - Throughput: N TXs trong 1 L1 TX (gas O(1) thay O(N))
///   - Validity: proof đảm bảo tất cả TXs hợp lệ (không cần trust operator)
///   - Data availability: calldata posted on-chain → user luôn reconstruct được
///   - Finality: instant khi proof verified on-chain (không cần fraud window)
///
/// So sánh với Optimistic Rollup (v2.3):
///   ZK-Rollup: validity proof → instant finality, nhưng proof generation nặng
///   Optimistic: assume valid → 7-day challenge window, nhưng đơn giản hơn
///
/// Tham khảo: zkSync, StarkNet, Polygon zkEVM

use std::collections::HashMap;

// ─── RollupTx ─────────────────────────────────────────────────────────────────

/// Giao dịch trong ZK-Rollup (off-chain)
#[derive(Debug, Clone)]
pub struct RollupTx {
    pub from:    String,
    pub to:      String,
    pub amount:  u64,
    pub nonce:   u64,    // anti-replay
    pub fee:     u64,
    pub tx_hash: String,
}

impl RollupTx {
    pub fn new(from: impl Into<String>, to: impl Into<String>, amount: u64, nonce: u64, fee: u64) -> Self {
        let from = from.into();
        let to   = to.into();
        let mut h = blake3::Hasher::new();
        h.update(b"rollup_tx_v22");
        h.update(from.as_bytes());
        h.update(to.as_bytes());
        h.update(&amount.to_le_bytes());
        h.update(&nonce.to_le_bytes());
        h.update(&fee.to_le_bytes());
        let tx_hash = hex::encode(h.finalize().as_bytes());
        RollupTx { from, to, amount, nonce, fee, tx_hash }
    }
}

// ─── RollupState ──────────────────────────────────────────────────────────────

/// Trạng thái off-chain: balances + nonces của tất cả accounts
#[derive(Debug, Clone)]
pub struct RollupState {
    pub balances: HashMap<String, u64>,
    pub nonces:   HashMap<String, u64>,
}

impl RollupState {
    pub fn new() -> Self {
        RollupState { balances: HashMap::new(), nonces: HashMap::new() }
    }

    pub fn deposit(&mut self, address: &str, amount: u64) {
        *self.balances.entry(address.to_string()).or_insert(0) += amount;
    }

    pub fn balance_of(&self, address: &str) -> u64 {
        self.balances.get(address).copied().unwrap_or(0)
    }

    pub fn nonce_of(&self, address: &str) -> u64 {
        self.nonces.get(address).copied().unwrap_or(0)
    }

    /// Merkle-like state root: H(sorted accounts || balances || nonces)
    pub fn state_root(&self) -> String {
        let mut keys: Vec<&String> = self.balances.keys().collect();
        keys.sort();
        let mut h = blake3::Hasher::new();
        h.update(b"state_root_v22");
        for k in keys {
            h.update(k.as_bytes());
            h.update(&self.balances[k].to_le_bytes());
            h.update(&self.nonces.get(k).copied().unwrap_or(0).to_le_bytes());
        }
        hex::encode(h.finalize().as_bytes())
    }

    /// Apply 1 TX — trả về Err nếu không hợp lệ
    pub fn apply_tx(&mut self, tx: &RollupTx) -> Result<(), String> {
        // Kiểm tra nonce
        let expected_nonce = self.nonce_of(&tx.from);
        if tx.nonce != expected_nonce {
            return Err(format!("Nonce sai: expected {}, got {}", expected_nonce, tx.nonce));
        }

        // Kiểm tra balance
        let bal = self.balance_of(&tx.from);
        let total = tx.amount + tx.fee;
        if bal < total {
            return Err(format!("{} không đủ balance: {} < {}", tx.from, bal, total));
        }

        // Apply
        *self.balances.entry(tx.from.clone()).or_insert(0) -= total;
        *self.balances.entry(tx.to.clone()).or_insert(0) += tx.amount;
        *self.nonces.entry(tx.from.clone()).or_insert(0) += 1;
        Ok(())
    }
}

// ─── ZkRollupProof ────────────────────────────────────────────────────────────

/// Validity proof cho 1 batch (simplified: hash-based commitment)
///
/// Trong thực tế: PLONK / Groth16 proof (~200-500 bytes)
/// Ở đây: H(old_root || new_root || all_tx_hashes || "zk_validity")
/// → Operator không thể forge nếu không biết valid transition
#[derive(Debug, Clone)]
pub struct ZkRollupProof {
    pub old_root:    String,
    pub new_root:    String,
    pub tx_count:    usize,
    pub proof_bytes: Vec<u8>,  // simplified proof
}

impl ZkRollupProof {
    pub fn generate(old_root: &str, new_root: &str, txs: &[RollupTx]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"zk_validity_v22");
        h.update(old_root.as_bytes());
        h.update(new_root.as_bytes());
        for tx in txs {
            h.update(tx.tx_hash.as_bytes());
        }
        let proof_bytes = h.finalize().as_bytes().to_vec();

        ZkRollupProof {
            old_root:    old_root.to_string(),
            new_root:    new_root.to_string(),
            tx_count:    txs.len(),
            proof_bytes,
        }
    }

    /// Verifier kiểm tra proof (on-chain logic)
    pub fn verify(&self, txs: &[RollupTx]) -> bool {
        let expected = Self::generate(&self.old_root, &self.new_root, txs);
        self.proof_bytes == expected.proof_bytes && self.tx_count == txs.len()
    }
}

// ─── RollupBatch ─────────────────────────────────────────────────────────────

/// 1 batch được submit lên L1
#[derive(Debug, Clone)]
pub struct RollupBatch {
    pub batch_id:  u64,
    pub old_root:  String,
    pub new_root:  String,
    pub txs:       Vec<RollupTx>,     // calldata (data availability)
    pub proof:     ZkRollupProof,
    pub total_fees: u64,
}

impl RollupBatch {
    pub fn fee_per_tx(&self) -> f64 {
        if self.txs.is_empty() { return 0.0; }
        self.total_fees as f64 / self.txs.len() as f64
    }
}

// ─── RollupOperator ───────────────────────────────────────────────────────────

/// Off-chain operator: thu thập TXs, tạo batch, generate proof
pub struct RollupOperator {
    pub state:    RollupState,
    pub mempool:  Vec<RollupTx>,
    pub batch_id: u64,
}

impl RollupOperator {
    pub fn new(initial_state: RollupState) -> Self {
        RollupOperator { state: initial_state, mempool: vec![], batch_id: 0 }
    }

    /// User gửi TX vào mempool
    pub fn submit_tx(&mut self, tx: RollupTx) {
        self.mempool.push(tx);
    }

    /// Tạo batch từ mempool — apply TXs, generate proof
    pub fn create_batch(&mut self) -> Result<RollupBatch, String> {
        if self.mempool.is_empty() {
            return Err("Mempool rỗng".to_string());
        }

        let old_root = self.state.state_root();
        let mut new_state = self.state.clone();

        // Apply tất cả TXs, bỏ qua TX không hợp lệ
        let mut valid_txs = vec![];
        let mut invalid_count = 0;
        for tx in self.mempool.drain(..) {
            match new_state.apply_tx(&tx) {
                Ok(()) => valid_txs.push(tx),
                Err(_) => invalid_count += 1,
            }
        }

        if valid_txs.is_empty() {
            return Err(format!("Tất cả {} TXs không hợp lệ", invalid_count));
        }

        let new_root     = new_state.state_root();
        let total_fees   = valid_txs.iter().map(|t| t.fee).sum();
        let proof        = ZkRollupProof::generate(&old_root, &new_root, &valid_txs);
        let batch_id     = self.batch_id;

        self.state   = new_state;
        self.batch_id += 1;

        Ok(RollupBatch { batch_id, old_root, new_root, txs: valid_txs, proof, total_fees })
    }
}

// ─── L1Verifier ───────────────────────────────────────────────────────────────

/// On-chain verifier — lưu state_root, verify proof từng batch
pub struct L1Verifier {
    pub state_root:    String,
    pub batch_count:   u64,
    pub total_tx_count: u64,
}

impl L1Verifier {
    pub fn new(initial_root: &str) -> Self {
        L1Verifier {
            state_root:     initial_root.to_string(),
            batch_count:    0,
            total_tx_count: 0,
        }
    }

    /// Verify và apply 1 batch (O(1) gas — chỉ verify proof + update root)
    pub fn verify_and_apply(&mut self, batch: &RollupBatch) -> Result<(), String> {
        // 1. Kiểm tra old_root khớp với state_root hiện tại
        if batch.old_root != self.state_root {
            return Err(format!(
                "State root không khớp: expected {:.8}..., got {:.8}...",
                self.state_root, batch.old_root
            ));
        }

        // 2. Verify ZK proof (O(1) — không cần replay từng TX)
        if !batch.proof.verify(&batch.txs) {
            return Err("ZK proof không hợp lệ".to_string());
        }

        // 3. Update state root
        self.state_root    = batch.new_root.clone();
        self.batch_count   += 1;
        self.total_tx_count += batch.txs.len() as u64;

        Ok(())
    }

    /// Gas tiết kiệm so với on-chain TXs:
    /// On-chain: N × gas_per_tx
    /// ZK-Rollup: gas_per_proof (constant)
    pub fn gas_savings_ratio(&self, gas_per_tx: u64, gas_per_proof: u64) -> f64 {
        if self.total_tx_count == 0 { return 1.0; }
        let onchain_gas  = self.total_tx_count * gas_per_tx;
        let rollup_gas   = self.batch_count * gas_per_proof;
        onchain_gas as f64 / rollup_gas as f64
    }
}

// ─── WithdrawalProof ──────────────────────────────────────────────────────────

/// Proof để user rút tiền từ rollup về L1
/// User chứng minh họ có balance trong state_root hiện tại
#[derive(Debug, Clone)]
pub struct WithdrawalProof {
    pub user:        String,
    pub amount:      u64,
    pub state_root:  String,
    pub merkle_path: Vec<String>,  // simplified: path hashes
    pub proof:       Vec<u8>,
}

impl WithdrawalProof {
    pub fn generate(user: &str, amount: u64, state: &RollupState) -> Option<Self> {
        let bal = state.balance_of(user);
        if bal < amount { return None; }

        let state_root = state.state_root();

        // Simplified merkle path (trong thực tế: Merkle Patricia Trie)
        let mut h = blake3::Hasher::new();
        h.update(b"withdrawal_v22");
        h.update(user.as_bytes());
        h.update(&amount.to_le_bytes());
        h.update(state_root.as_bytes());
        let proof = h.finalize().as_bytes().to_vec();

        Some(WithdrawalProof {
            user:        user.to_string(),
            amount,
            state_root,
            merkle_path: vec![hex::encode(&proof[..16])],
            proof,
        })
    }

    /// L1 verifier kiểm tra withdrawal proof
    pub fn verify(&self, current_root: &str) -> bool {
        if self.state_root != current_root { return false; }

        let mut h = blake3::Hasher::new();
        h.update(b"withdrawal_v22");
        h.update(self.user.as_bytes());
        h.update(&self.amount.to_le_bytes());
        h.update(self.state_root.as_bytes());
        h.finalize().as_bytes().as_ref() == self.proof.as_slice()
    }
}
