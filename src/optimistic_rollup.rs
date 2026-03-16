#![allow(dead_code)]

/// v2.3 — Optimistic Rollup + Fraud Proof
///
/// Kiến trúc:
///
///   Off-chain (Sequencer)           On-chain (L1 Contract)
///   ──────────────────────         ──────────────────────
///   Collect TXs                    Store state_root
///   Apply TXs → new_state          Accept batch (no proof!)
///   Post (calldata, new_root) ───► Start 7-day challenge window
///   [Assume valid unless           Verifier submits fraud proof
///    challenged]                   → Revert batch if fraud found
///
/// So sánh với ZK-Rollup (v2.2):
///   ZK-Rollup:   validity proof → instant finality, proof generation nặng
///   Optimistic:  assume valid  → 7-day window,    đơn giản hơn, EVM-compatible dễ hơn
///
/// Fraud Proof flow:
///   1. Challenger thấy batch có TX sai (invalid state transition)
///   2. Challenger submit fraud proof: pre-state + TX + expected_new_state
///   3. L1 contract re-execute TX on-chain (execution proof)
///   4. Nếu new_state ≠ claimed_root → revert batch, slash sequencer
///
/// Tham khảo: Arbitrum, Optimism (OP Stack)

use std::collections::HashMap;

// ─── OptimisticTx ─────────────────────────────────────────────────────────────

/// Giao dịch trong Optimistic Rollup
#[derive(Debug, Clone)]
pub struct OptimisticTx {
    pub from:    String,
    pub to:      String,
    pub amount:  u64,
    pub nonce:   u64,
    pub fee:     u64,
    pub tx_hash: String,
}

impl OptimisticTx {
    pub fn new(from: impl Into<String>, to: impl Into<String>, amount: u64, nonce: u64, fee: u64) -> Self {
        let from = from.into();
        let to   = to.into();
        let mut h = blake3::Hasher::new();
        h.update(b"opt_tx_v23");
        h.update(from.as_bytes());
        h.update(to.as_bytes());
        h.update(&amount.to_le_bytes());
        h.update(&nonce.to_le_bytes());
        h.update(&fee.to_le_bytes());
        let tx_hash = hex::encode(h.finalize().as_bytes());
        OptimisticTx { from, to, amount, nonce, fee, tx_hash }
    }
}

// ─── OptimisticState ──────────────────────────────────────────────────────────

/// Trạng thái off-chain
#[derive(Debug, Clone)]
pub struct OptimisticState {
    pub balances: HashMap<String, u64>,
    pub nonces:   HashMap<String, u64>,
}

impl OptimisticState {
    pub fn new() -> Self {
        OptimisticState { balances: HashMap::new(), nonces: HashMap::new() }
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

    /// State root: H(sorted accounts || balances || nonces)
    pub fn state_root(&self) -> String {
        let mut keys: Vec<&String> = self.balances.keys().collect();
        keys.sort();
        let mut h = blake3::Hasher::new();
        h.update(b"opt_state_v23");
        for k in keys {
            h.update(k.as_bytes());
            h.update(&self.balances[k].to_le_bytes());
            h.update(&self.nonces.get(k).copied().unwrap_or(0).to_le_bytes());
        }
        hex::encode(h.finalize().as_bytes())
    }

    /// Apply 1 TX. Trả về Err nếu không hợp lệ.
    /// Đây cũng là hàm L1 dùng để re-execute trong fraud proof verification.
    pub fn apply_tx(&mut self, tx: &OptimisticTx) -> Result<(), String> {
        let expected_nonce = self.nonce_of(&tx.from);
        if tx.nonce != expected_nonce {
            return Err(format!("Nonce sai: expected {}, got {}", expected_nonce, tx.nonce));
        }
        let bal   = self.balance_of(&tx.from);
        let total = tx.amount + tx.fee;
        if bal < total {
            return Err(format!("{} không đủ balance: {} < {}", tx.from, bal, total));
        }
        *self.balances.entry(tx.from.clone()).or_insert(0) -= total;
        *self.balances.entry(tx.to.clone()).or_insert(0) += tx.amount;
        *self.nonces.entry(tx.from.clone()).or_insert(0) += 1;
        Ok(())
    }
}

// ─── OptimisticBatch ──────────────────────────────────────────────────────────

/// 1 batch submitted lên L1 — không kèm ZK proof
/// L1 chỉ lưu (old_root, new_root, calldata, timestamp)
/// Challenge window mở sau khi batch được accepted
#[derive(Debug, Clone)]
pub struct OptimisticBatch {
    pub batch_id:        u64,
    pub old_root:        String,
    pub new_root:        String,
    pub txs:             Vec<OptimisticTx>,   // calldata — data availability
    pub sequencer:       String,
    pub submitted_at:    u64,                  // block number / timestamp
    pub challenge_end:   u64,                  // submitted_at + CHALLENGE_WINDOW
    pub finalized:       bool,
    pub reverted:        bool,
    pub total_fees:      u64,
}

pub const CHALLENGE_WINDOW: u64 = 7 * 24 * 3600;  // 7 ngày tính bằng giây

impl OptimisticBatch {
    pub fn is_finalizable(&self, current_time: u64) -> bool {
        !self.finalized && !self.reverted && current_time >= self.challenge_end
    }

    pub fn fee_per_tx(&self) -> f64 {
        if self.txs.is_empty() { return 0.0; }
        self.total_fees as f64 / self.txs.len() as f64
    }
}

// ─── FraudProof ───────────────────────────────────────────────────────────────

/// Proof do challenger submit để disprove 1 batch
///
/// Fraud proof cho 1 TX cụ thể trong batch:
///   - pre_state: state trước khi apply tx_index
///   - tx_index: TX trong batch bị cho là sai
///   - claimed_post_root: root mà sequencer claim sau khi apply TX này
///
/// L1 verifier sẽ:
///   1. Xác minh pre_state.state_root() khớp với pre_root của TX đó
///   2. Re-execute TX trên pre_state
///   3. So sánh kết quả với claimed_post_root
///   4. Nếu không khớp → fraud confirmed
#[derive(Debug, Clone)]
pub struct FraudProof {
    pub batch_id:         u64,
    pub tx_index:         usize,       // TX trong batch bị challenge
    pub pre_state:        OptimisticState,
    pub claimed_post_root: String,     // root sequencer claim sau TX này
    pub challenger:       String,
}

impl FraudProof {
    pub fn new(
        batch_id: u64,
        tx_index: usize,
        pre_state: OptimisticState,
        claimed_post_root: impl Into<String>,
        challenger: impl Into<String>,
    ) -> Self {
        FraudProof {
            batch_id,
            tx_index,
            pre_state,
            claimed_post_root: claimed_post_root.into(),
            challenger: challenger.into(),
        }
    }

    /// L1 re-execute TX và kiểm tra kết quả
    /// Trả về Ok(true) nếu fraud được xác nhận (sequencer lied)
    /// Trả về Ok(false) nếu batch hợp lệ (challenger sai)
    pub fn verify(&self, tx: &OptimisticTx) -> Result<bool, String> {
        let mut state = self.pre_state.clone();

        // Re-execute TX on-chain (simplified: O(1) single TX)
        match state.apply_tx(tx) {
            Ok(()) => {
                let actual_root = state.state_root();
                // Nếu actual ≠ claimed → sequencer lied → fraud!
                Ok(actual_root != self.claimed_post_root)
            }
            Err(_) => {
                // TX itself is invalid (e.g., insufficient balance)
                // Sequencer included an invalid TX → fraud!
                Ok(true)
            }
        }
    }
}

// ─── Sequencer ────────────────────────────────────────────────────────────────

/// Off-chain sequencer: thu thập TXs, tạo batch, post lên L1
pub struct Sequencer {
    pub state:    OptimisticState,
    pub mempool:  Vec<OptimisticTx>,
    pub batch_id: u64,
    pub name:     String,
    pub bond:     u64,   // stake bị slash nếu fraud confirmed
}

impl Sequencer {
    pub fn new(name: impl Into<String>, initial_state: OptimisticState, bond: u64) -> Self {
        Sequencer {
            state: initial_state,
            mempool: vec![],
            batch_id: 0,
            name: name.into(),
            bond,
        }
    }

    pub fn submit_tx(&mut self, tx: OptimisticTx) {
        self.mempool.push(tx);
    }

    /// Tạo batch hợp lệ từ mempool
    pub fn create_batch(&mut self, current_time: u64) -> Result<OptimisticBatch, String> {
        if self.mempool.is_empty() {
            return Err("Mempool rỗng".to_string());
        }

        let old_root = self.state.state_root();
        let mut new_state = self.state.clone();

        let mut valid_txs = vec![];
        for tx in self.mempool.drain(..) {
            if new_state.apply_tx(&tx).is_ok() {
                valid_txs.push(tx);
            }
        }

        if valid_txs.is_empty() {
            return Err("Tất cả TXs không hợp lệ".to_string());
        }

        let new_root   = new_state.state_root();
        let total_fees = valid_txs.iter().map(|t| t.fee).sum();
        let batch_id   = self.batch_id;

        self.state    = new_state;
        self.batch_id += 1;

        Ok(OptimisticBatch {
            batch_id,
            old_root,
            new_root,
            txs: valid_txs,
            sequencer:     self.name.clone(),
            submitted_at:  current_time,
            challenge_end: current_time + CHALLENGE_WINDOW,
            finalized:     false,
            reverted:      false,
            total_fees,
        })
    }

    /// Tạo batch GIẢ MẠO — sequencer xấu claim sai state transition
    /// (để demo fraud proof detection)
    pub fn create_fraudulent_batch(
        &mut self,
        txs: Vec<OptimisticTx>,
        fake_new_root: impl Into<String>,
        current_time: u64,
    ) -> OptimisticBatch {
        let old_root = self.state.state_root();
        let batch_id = self.batch_id;
        self.batch_id += 1;
        // KHÔNG apply TXs vào state → state diverges

        OptimisticBatch {
            batch_id,
            old_root,
            new_root: fake_new_root.into(),
            txs,
            sequencer:     self.name.clone(),
            submitted_at:  current_time,
            challenge_end: current_time + CHALLENGE_WINDOW,
            finalized:     false,
            reverted:      false,
            total_fees:    0,
        }
    }
}

// ─── L1OptimisticContract ─────────────────────────────────────────────────────

/// On-chain contract — lưu state_root, queue pending batches, process fraud proofs
pub struct L1OptimisticContract {
    pub state_root:      String,
    pub pending_batches: Vec<OptimisticBatch>,
    pub finalized_count: u64,
    pub reverted_count:  u64,
    pub total_tx_count:  u64,
    pub sequencer_bonds: HashMap<String, u64>,
}

impl L1OptimisticContract {
    pub fn new(initial_root: &str) -> Self {
        L1OptimisticContract {
            state_root:      initial_root.to_string(),
            pending_batches: vec![],
            finalized_count: 0,
            reverted_count:  0,
            total_tx_count:  0,
            sequencer_bonds: HashMap::new(),
        }
    }

    pub fn register_sequencer(&mut self, name: &str, bond: u64) {
        self.sequencer_bonds.insert(name.to_string(), bond);
    }

    /// Sequencer submit batch — L1 chỉ check old_root khớp, KHÔNG verify TXs
    /// → O(1) gas (rẻ hơn ZK verification)
    pub fn submit_batch(&mut self, batch: OptimisticBatch) -> Result<(), String> {
        if batch.old_root != self.state_root {
            return Err(format!(
                "State root không khớp: expected {:.8}..., got {:.8}...",
                self.state_root, batch.old_root
            ));
        }
        // Optimistically accept — update root immediately
        self.state_root = batch.new_root.clone();
        self.pending_batches.push(batch);
        Ok(())
    }

    /// Challenger submit fraud proof trong challenge window
    /// L1 re-execute TX và kiểm tra
    pub fn submit_fraud_proof(&mut self, fraud_proof: FraudProof) -> FraudResult {
        // Tìm batch bị challenge
        let batch_pos = self.pending_batches.iter()
            .position(|b| b.batch_id == fraud_proof.batch_id);

        let batch_pos = match batch_pos {
            Some(p) => p,
            None => return FraudResult::BatchNotFound,
        };

        {
            let batch = &self.pending_batches[batch_pos];
            if batch.finalized {
                return FraudResult::AlreadyFinalized;
            }
            if batch.reverted {
                return FraudResult::AlreadyReverted;
            }
        }

        let tx = {
            let batch = &self.pending_batches[batch_pos];
            if fraud_proof.tx_index >= batch.txs.len() {
                return FraudResult::InvalidTxIndex;
            }
            batch.txs[fraud_proof.tx_index].clone()
        };

        match fraud_proof.verify(&tx) {
            Ok(true) => {
                // Fraud confirmed: revert batch, slash sequencer
                let batch = &mut self.pending_batches[batch_pos];
                batch.reverted = true;

                // Revert state_root back to old_root
                self.state_root = batch.old_root.clone();
                self.reverted_count += 1;

                let sequencer = batch.sequencer.clone();
                let slash_amount = self.sequencer_bonds.get(&sequencer).copied().unwrap_or(0) / 2;
                if let Some(bond) = self.sequencer_bonds.get_mut(&sequencer) {
                    *bond -= slash_amount;
                }

                FraudResult::FraudConfirmed {
                    sequencer,
                    slashed: slash_amount,
                    reward:  slash_amount / 2,  // challenger gets half
                }
            }
            Ok(false) => FraudResult::NotFraud,
            Err(e)    => FraudResult::VerificationError(e),
        }
    }

    /// Finalize batch sau khi challenge window đóng
    pub fn finalize_batch(&mut self, batch_id: u64, current_time: u64) -> bool {
        let pos = self.pending_batches.iter().position(|b| b.batch_id == batch_id);
        if let Some(p) = pos {
            let batch = &mut self.pending_batches[p];
            if batch.is_finalizable(current_time) {
                batch.finalized = true;
                self.finalized_count += 1;
                self.total_tx_count += batch.txs.len() as u64;
                return true;
            }
        }
        false
    }

    /// Gas so sánh: Optimistic chỉ cần verify calldata hash, không cần ZK proof
    /// On-chain: N × gas_per_tx
    /// Optimistic: gas_per_batch (rẻ hơn ZK về submission cost, nhưng có fraud proof cost)
    pub fn gas_savings_vs_onchain(&self, gas_per_tx: u64, gas_per_batch: u64) -> f64 {
        if self.total_tx_count == 0 { return 1.0; }
        let onchain_gas  = self.total_tx_count * gas_per_tx;
        let rollup_gas   = self.finalized_count * gas_per_batch;
        if rollup_gas == 0 { return 1.0; }
        onchain_gas as f64 / rollup_gas as f64
    }
}

// ─── FraudResult ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum FraudResult {
    FraudConfirmed { sequencer: String, slashed: u64, reward: u64 },
    NotFraud,
    BatchNotFound,
    AlreadyFinalized,
    AlreadyReverted,
    InvalidTxIndex,
    VerificationError(String),
}

// ─── WithdrawalRequest ────────────────────────────────────────────────────────

/// User muốn rút tiền về L1 — phải đợi challenge window
#[derive(Debug, Clone)]
pub struct WithdrawalRequest {
    pub user:         String,
    pub amount:       u64,
    pub batch_id:     u64,   // batch chứa withdrawal TX
    pub submitted_at: u64,
    pub challenge_end: u64,
    pub executed:     bool,
}

impl WithdrawalRequest {
    pub fn new(user: impl Into<String>, amount: u64, batch_id: u64, submitted_at: u64) -> Self {
        WithdrawalRequest {
            user: user.into(),
            amount,
            batch_id,
            submitted_at,
            challenge_end: submitted_at + CHALLENGE_WINDOW,
            executed: false,
        }
    }

    pub fn can_execute(&self, current_time: u64) -> bool {
        !self.executed && current_time >= self.challenge_end
    }
}
