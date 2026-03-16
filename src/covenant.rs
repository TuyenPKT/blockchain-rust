#![allow(dead_code)]
//! Covenants & CTV — v1.4 (BIP119)
//!
//! Covenant = ràng buộc cách UTXO được tiêu trong tương lai.
//! Không chỉ "ai ký được tiêu" mà còn "tiêu vào đâu, bao nhiêu".
//!
//! OP_CHECKTEMPLATEVERIFY (CTV, BIP119):
//!   - Output commit vào `template_hash` = hash của TX tương lai
//!   - Khi spend, runtime hash lại TX và so sánh với template_hash
//!   - Nếu không khớp → INVALID
//!
//! Template hash bao gồm:
//!   SHA256(version || locktime || scriptSigs_hash || sequences_hash ||
//!          outputs_hash || input_count || input_index)
//!
//! Use cases:
//!   1. Vault: 2-step withdrawal với timelock
//!      [hot wallet] → unvault_TX (delay 144 blocks) → cold_TX
//!      Nếu phát hiện hack → clawback_TX cancel unvault
//!   2. Congestion control: 1 UTXO → batch outputs
//!      [payer] → congestion_TX (1 output) → expand_TX (nhiều outputs)
//!   3. Payment pool: nhiều người dùng chung 1 UTXO, rút dần

use serde::{Serialize, Deserialize};
use crate::transaction::{Transaction, TxOutput};
use crate::script::Script;
use crate::taproot::tagged_hash;

// ── CTV Template Hash ────────────────────────────────────────
//
// BIP119 định nghĩa template_hash commit vào cấu trúc TX:
//   - outputs (ai nhận, bao nhiêu)
//   - input count + index (UTXO nào đang dùng)
//   - sequence + locktime (timelock conditions)
// KHÔNG commit vào inputs' prevouts → flexible về nguồn tiền

/// Tính CTV template hash của một transaction
/// Đây là hash mà scriptPubKey sẽ commit vào
pub fn ctv_template_hash(tx: &CtvTemplate) -> [u8; 32] {
    // outputs_hash = SHA256(concat of all outputs serialized)
    let mut out_data = Vec::new();
    for o in &tx.outputs {
        out_data.extend_from_slice(&o.amount.to_le_bytes());
        let script_bytes = o.script_pubkey.to_bytes();
        out_data.extend_from_slice(&(script_bytes.len() as u32).to_le_bytes());
        out_data.extend_from_slice(&script_bytes);
    }
    let outputs_hash = *blake3::hash(&out_data).as_bytes();

    // sequences_hash = SHA256(concat of all sequences)
    let mut seq_data = Vec::new();
    for s in &tx.sequences {
        seq_data.extend_from_slice(&s.to_le_bytes());
    }
    let sequences_hash = *blake3::hash(&seq_data).as_bytes();

    // template_hash = tagged_hash("CTV", version || locktime || sequences_hash || outputs_hash || input_count || input_index)
    let mut data = Vec::new();
    data.extend_from_slice(&tx.version.to_le_bytes());
    data.extend_from_slice(&tx.locktime.to_le_bytes());
    data.extend_from_slice(&sequences_hash);
    data.extend_from_slice(&outputs_hash);
    data.extend_from_slice(&(tx.input_count as u32).to_le_bytes());
    data.extend_from_slice(&(tx.input_index as u32).to_le_bytes());

    tagged_hash("BIP0119/TemplateHash", &data)
}

/// Template của một CTV-constrained transaction
/// Đây là "hợp đồng" mà UTXO commit vào — spending TX phải match chính xác
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtvTemplate {
    pub version:     u32,
    pub locktime:    u32,
    pub sequences:   Vec<u32>,       // sequence của từng input
    pub outputs:     Vec<TxOutput>,  // outputs cố định
    pub input_count: usize,
    pub input_index: usize,          // input nào đang dùng CTV
}

impl CtvTemplate {
    pub fn new(outputs: Vec<TxOutput>, locktime: u32) -> Self {
        CtvTemplate {
            version:     2,
            locktime,
            sequences:   vec![0xFFFFFFFE], // 1 input, nLocktime-enabled
            outputs,
            input_count: 1,
            input_index: 0,
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        ctv_template_hash(self)
    }

    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash())
    }
}

// ── Vault ────────────────────────────────────────────────────
//
// Bitcoin Vault pattern dùng CTV:
//
//   Funding UTXO
//       │
//       ▼ (trigger withdrawal)
//   Unvault TX  ← scriptPubKey commit vào clawback_hash HOẶC ctv_hash
//       │
//       ├── Sau DELAY blocks: Cold TX → cold wallet (finalize)
//       │
//       └── Trước DELAY blocks: Clawback TX → hot wallet (cancel hack)
//
// Delay thường = 144 blocks ≈ 1 ngày — đủ thời gian phát hiện hack

pub const VAULT_DELAY: u32 = 144; // blocks

/// Vault descriptor: định nghĩa các TX trong vault flow
#[derive(Debug, Clone)]
pub struct Vault {
    pub hot_pubkey_hash:   String,   // hot wallet (trigger + clawback)
    pub cold_pubkey_hash:  String,   // cold wallet (cuối cùng nhận tiền)
    pub amount:            u64,      // sat
    pub fee:               u64,      // fee mỗi hop
}

/// Kết quả của việc khởi tạo vault
#[derive(Debug, Clone)]
pub struct VaultSetup {
    pub funding_script:    Script,   // scriptPubKey của funding UTXO
    pub unvault_template:  CtvTemplate,
    pub cold_template:     CtvTemplate,
    pub clawback_template: CtvTemplate,
    pub unvault_hash:      [u8; 32],
    pub cold_hash:         [u8; 32],
    pub clawback_hash:     [u8; 32],
}

impl Vault {
    pub fn new(
        hot_pubkey_hash:  String,
        cold_pubkey_hash: String,
        amount:           u64,
        fee:              u64,
    ) -> Self {
        Vault { hot_pubkey_hash, cold_pubkey_hash, amount, fee }
    }

    /// Thiết lập toàn bộ vault:
    /// 1. Tạo cold_template (step 2: gửi đến cold wallet)
    /// 2. Tạo unvault_template (step 1: sau delay → cold TX)
    /// 3. Tạo clawback_template (cancel: gửi về hot wallet)
    /// 4. Tạo funding scriptPubKey commit vào unvault_hash
    pub fn setup(&self) -> VaultSetup {
        // Step 2: Cold TX — gửi tiền đến cold wallet (sau delay)
        let cold_output  = TxOutput::p2pkh(self.amount - self.fee, &self.cold_pubkey_hash);
        let cold_tmpl    = CtvTemplate::new(vec![cold_output], 0);
        let cold_hash    = cold_tmpl.hash();

        // Step 1: Unvault TX — commit vào cold_hash, có timelock delay
        // scriptPubKey = <cold_hash> OP_CTV  (simplified)
        // Thực tế: OP_<DELAY> OP_CSV OP_DROP <cold_hash> OP_CTV
        let unvault_out  = TxOutput::ctv_output(self.amount - self.fee, &hex::encode(cold_hash));
        let unvault_tmpl = CtvTemplate::new(vec![unvault_out], 0);
        let unvault_hash = unvault_tmpl.hash();

        // Clawback TX — trả về hot wallet nếu phát hiện hack
        let clawback_out  = TxOutput::p2pkh(self.amount - self.fee, &self.hot_pubkey_hash);
        let clawback_tmpl = CtvTemplate::new(vec![clawback_out], 0);
        let clawback_hash = clawback_tmpl.hash();

        // Funding scriptPubKey: hot_sig OP_CTV với unvault_hash
        let funding_script = Script::ctv_pubkey(&hex::encode(unvault_hash));

        VaultSetup {
            funding_script,
            unvault_template:  unvault_tmpl,
            cold_template:     cold_tmpl,
            clawback_template: clawback_tmpl,
            unvault_hash,
            cold_hash,
            clawback_hash,
        }
    }

    /// Verify một TX có thỏa mãn CTV template không
    /// Tính template_hash của TX đang spend và so sánh với committed hash
    pub fn verify_ctv(spending_tx: &Transaction, expected_hash: &[u8; 32]) -> bool {
        // Tạo CtvTemplate từ spending_tx để hash
        let template = CtvTemplate {
            version:     2,
            locktime:    0,
            sequences:   spending_tx.inputs.iter().map(|i| i.sequence).collect(),
            outputs:     spending_tx.outputs.clone(),
            input_count: spending_tx.inputs.len(),
            input_index: 0,
        };
        &template.hash() == expected_hash
    }
}

// ── Congestion Control ───────────────────────────────────────
//
// Khi mempool tắc nghẽn, fee cao:
//   Thay vì gửi N TX riêng lẻ → gửi 1 TX duy nhất với 1 output CTV
//   Output đó commit vào N outputs thực sự
//   Khi fee giảm → expand TX giải nén N outputs
//
// Tiết kiệm block space, giảm fee trong thời điểm congestion

/// Batch payment plan dùng CTV
#[derive(Debug, Clone)]
pub struct CongestionBatch {
    pub recipients: Vec<(String, u64)>,  // (pubkey_hash, amount)
    pub fee:        u64,
}

impl CongestionBatch {
    pub fn new(recipients: Vec<(String, u64)>, fee: u64) -> Self {
        CongestionBatch { recipients, fee }
    }

    /// Tạo expand template: 1 UTXO → nhiều outputs
    pub fn expand_template(&self) -> CtvTemplate {
        let outputs: Vec<TxOutput> = self.recipients.iter()
            .map(|(hash, amt)| TxOutput::p2pkh(*amt, hash))
            .collect();
        CtvTemplate::new(outputs, 0)
    }

    /// Tạo congestion output: 1 output commit vào expand_template
    /// Người trả chỉ cần broadcast 1 TX nhỏ này trong thời điểm fee cao
    pub fn congestion_output(&self, total_amount: u64) -> TxOutput {
        let tmpl = self.expand_template();
        TxOutput::ctv_output(total_amount - self.fee, &tmpl.hash_hex())
    }

    pub fn total_amount(&self) -> u64 {
        self.recipients.iter().map(|(_, a)| a).sum::<u64>() + self.fee
    }

    pub fn describe(&self) -> String {
        format!(
            "Batch {} recipients, total={} sat, fee={}",
            self.recipients.len(),
            self.total_amount(),
            self.fee
        )
    }
}

// ── Payment Pool ─────────────────────────────────────────────
//
// Nhiều người dùng chung 1 UTXO (off-chain balance tracking)
// Rút tiền = broadcast CTV TX với outputs cho người rút + pool còn lại
// Tương tự channel factory, không cần trust

#[derive(Debug, Clone)]
pub struct PaymentPool {
    pub members: Vec<(String, u64)>,  // (pubkey_hash, balance)
    pub fee:     u64,
}

impl PaymentPool {
    pub fn new(members: Vec<(String, u64)>, fee: u64) -> Self {
        PaymentPool { members, fee }
    }

    pub fn total_balance(&self) -> u64 {
        self.members.iter().map(|(_, b)| b).sum()
    }

    /// Tạo withdrawal template cho một member
    /// Outputs: [member_output, pool_remainder_output]
    pub fn withdrawal_template(&self, member_index: usize) -> Option<CtvTemplate> {
        if member_index >= self.members.len() { return None; }
        let (ref hash, amount) = self.members[member_index];
        let remainder: u64 = self.members.iter()
            .enumerate()
            .filter(|(i, _)| *i != member_index)
            .map(|(_, (_, b))| b)
            .sum::<u64>()
            .saturating_sub(self.fee);

        let mut outputs = vec![TxOutput::p2pkh(amount, hash)];
        if remainder > 0 {
            // Pool remainder → new CTV output with remaining members
            let remaining: Vec<(String, u64)> = self.members.iter()
                .enumerate()
                .filter(|(i, _)| *i != member_index)
                .map(|(_, m)| m.clone())
                .collect();
            let sub_pool = PaymentPool::new(remaining, self.fee);
            let sub_tmpl = sub_pool.withdrawal_template(0)?;
            outputs.push(TxOutput::ctv_output(remainder, &sub_tmpl.hash_hex()));
        }
        Some(CtvTemplate::new(outputs, 0))
    }

    pub fn describe(&self) -> String {
        format!("{} members, pool={} sat", self.members.len(), self.total_balance())
    }
}
