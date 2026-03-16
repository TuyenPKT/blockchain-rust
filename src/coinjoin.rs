#![allow(dead_code)]
//! CoinJoin — v1.6
//!
//! CoinJoin: nhiều người gộp inputs vào 1 TX chung để ẩn danh.
//!   - Equal-amount outputs: phá vỡ liên kết input → output
//!     (observer không biết output nào là của ai)
//!   - Coordinator pattern: thu thập inputs từ participants, build TX
//!   - PayJoin (P2EP): cả sender lẫn receiver đóng góp inputs,
//!     khiến observer không xác định được chiều giao dịch
//!
//! Ví dụ CoinJoin 3 người, denomination = 1_000_000 sat:
//!   Inputs:  [Alice 3M], [Bob 1.5M], [Carol 2M]
//!   Outputs: [1M], [1M], [1M],  ← 3 equal outputs (ai nhận gì?)
//!            [2M],              ← Alice change
//!            [0.5M],            ← Bob change
//!            [1M]               ← Carol change
//!
//! Observer thấy 3 inputs và 6 outputs — không thể biết input nào → output nào.
//! Anonymity set = N (số participants) với equal-amount outputs.

use crate::transaction::{Transaction, TxOutput};
use crate::wallet::Wallet;
use crate::script::Script;

// ── CoinJoin Participant ──────────────────────────────────────

/// Một người tham gia CoinJoin round
pub struct CoinJoinParticipant {
    pub wallet:        Wallet,
    pub utxo_tx_id:   String,
    pub utxo_index:   usize,
    pub utxo_amount:  u64,
    pub denomination: u64,    // amount muốn gộp vào CoinJoin
    pub change_amount: u64,   // utxo_amount - denomination - fee_share
}

impl CoinJoinParticipant {
    /// Tạo participant; trả None nếu UTXO không đủ denomination
    pub fn new(
        wallet:       Wallet,
        utxo_tx_id:  String,
        utxo_index:  usize,
        utxo_amount: u64,
        denomination: u64,
    ) -> Option<Self> {
        if utxo_amount < denomination { return None; }
        Some(CoinJoinParticipant {
            wallet, utxo_tx_id, utxo_index, utxo_amount, denomination,
            change_amount: 0,
        })
    }

    pub fn pubkey_hash(&self) -> String {
        hex::encode(Script::pubkey_hash(&self.wallet.public_key.serialize()))
    }
}

// ── CoinJoin Session (Coordinator) ───────────────────────────

/// Coordinator tập hợp participants và build CoinJoin TX
///
/// Coordinator model (giống Wasabi Wallet v1 — centralized):
///   1. Coordinator công bố denomination + fee
///   2. Participants gửi signed input + output address (blinded nếu WabiSabi)
///   3. Coordinator build TX với equal-amount outputs
///   4. Participants ký final TX
///   5. Coordinator broadcast
pub struct CoinJoinSession {
    pub denomination:   u64,
    pub fee_per_input:  u64,
    participants:       Vec<CoinJoinParticipant>,
}

impl CoinJoinSession {
    pub fn new(denomination: u64, fee_per_input: u64) -> Self {
        CoinJoinSession { denomination, fee_per_input, participants: vec![] }
    }

    /// Participant join session — check denomination + đủ UTXO
    pub fn join(&mut self, mut p: CoinJoinParticipant) -> Result<(), String> {
        if p.denomination != self.denomination {
            return Err(format!(
                "❌ Denomination không khớp: {} ≠ {}", p.denomination, self.denomination
            ));
        }
        let needed = self.denomination + self.fee_per_input;
        if p.utxo_amount < needed {
            return Err(format!(
                "❌ UTXO {} < denomination+fee {}", p.utxo_amount, needed
            ));
        }
        p.change_amount = p.utxo_amount - self.denomination - self.fee_per_input;
        self.participants.push(p);
        Ok(())
    }

    pub fn participant_count(&self) -> usize { self.participants.len() }

    /// Build CoinJoin TX:
    ///   Inputs:  [p1.utxo, p2.utxo, ..., pN.utxo]
    ///   Outputs: [denomination×N (equal, shuffled), change outputs]
    ///
    /// Mỗi participant ký input của mình với cùng signing_data
    pub fn build(&self) -> Result<CoinJoinTx, String> {
        if self.participants.len() < 2 {
            return Err("❌ CoinJoin cần ít nhất 2 participants".to_string());
        }

        // Thu thập inputs
        let inputs_raw: Vec<(String, usize)> = self.participants.iter()
            .map(|p| (p.utxo_tx_id.clone(), p.utxo_index))
            .collect();

        // Equal-amount denomination outputs (thứ tự ngẫu nhiên ideally — ở đây list thứ tự)
        // Trong production: shuffle outputs trước khi ký để coordinator không biết mapping
        let mut outputs: Vec<TxOutput> = self.participants.iter()
            .map(|p| TxOutput::p2pkh(p.denomination, &p.pubkey_hash()))
            .collect();

        // Change outputs (nếu > 0)
        for p in &self.participants {
            if p.change_amount > 0 {
                outputs.push(TxOutput::p2pkh(p.change_amount, &p.pubkey_hash()));
            }
        }

        let total_fee = self.fee_per_input * self.participants.len() as u64;
        let mut tx = Transaction::new_unsigned(inputs_raw, outputs, total_fee);

        // Mỗi participant ký input của mình (index = vị trí trong inputs)
        let signing_data = tx.signing_data();
        for (i, p) in self.participants.iter().enumerate() {
            let sig    = p.wallet.sign(&signing_data);
            let pubkey = p.wallet.public_key_hex();
            tx.inputs[i].script_sig = Script::p2pkh_sig(&sig, &pubkey);
        }

        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();

        Ok(CoinJoinTx {
            tx,
            participant_count: self.participants.len(),
            denomination:      self.denomination,
            change_count:      self.participants.iter().filter(|p| p.change_amount > 0).count(),
        })
    }
}

// ── CoinJoin TX (kết quả) ─────────────────────────────────────

pub struct CoinJoinTx {
    pub tx:                Transaction,
    pub participant_count: usize,
    pub denomination:      u64,
    pub change_count:      usize,
}

impl CoinJoinTx {
    /// Anonymity set: số lượng equal-amount outputs
    /// Observer phải thử N! mapping để identify input→output
    pub fn anonymity_set(&self) -> usize { self.participant_count }

    /// Phân tích TX từ góc nhìn observer ngoài
    pub fn observer_analysis(&self) -> String {
        let n     = self.participant_count;
        let in_ct = self.tx.inputs.len();
        let out_ct = self.tx.outputs.len();
        let denom = self.denomination;
        format!(
            "Observer thấy: {} inputs, {} outputs ({} equal @ {} sat, {} change)\n  \
             Số combinations cần thử: {}! = {} — không thể trace",
            in_ct, out_ct, n, denom, self.change_count,
            n, factorial(n)
        )
    }
}

fn factorial(n: usize) -> u64 {
    (1..=n as u64).product()
}

// ── PayJoin / P2EP ────────────────────────────────────────────
//
// Pay-to-Endpoint (BIP78): cả sender VÀ receiver đóng góp inputs.
//   Observer nhìn vào thấy:
//     - 2 inputs (không biết ai là sender, ai là receiver)
//     - Không thể dùng "tổng output > tổng input − 1 output" heuristic
//   → Không xác định được chiều payment
//
// Flow:
//   1. Sender tạo PSBT gốc: [sender_input] → [receiver gets amount, sender change]
//   2. Sender gửi PSBT cho receiver (qua BIP21 URI / HTTP endpoint)
//   3. Receiver thêm input của mình, tăng output tương ứng
//   4. Receiver trả PSBT đã sửa
//   5. Sender ký → broadcast

/// PayJoin session: sender + receiver cùng đóng góp inputs
pub struct PayJoinSession {
    pub payment_amount: u64,
    pub fee:            u64,
    sender_wallet:      Option<Wallet>,
    sender_utxo:        Option<(String, usize, u64)>,
    receiver_wallet:    Option<Wallet>,
    receiver_utxo:      Option<(String, usize, u64)>,
}

impl PayJoinSession {
    pub fn new(payment_amount: u64, fee: u64) -> Self {
        PayJoinSession {
            payment_amount, fee,
            sender_wallet: None,   sender_utxo: None,
            receiver_wallet: None, receiver_utxo: None,
        }
    }

    /// Sender cung cấp UTXO của mình
    pub fn add_sender(
        &mut self,
        wallet:      Wallet,
        utxo_tx_id: String,
        utxo_index: usize,
        utxo_amount: u64,
    ) {
        self.sender_wallet = Some(wallet);
        self.sender_utxo   = Some((utxo_tx_id, utxo_index, utxo_amount));
    }

    /// Receiver cung cấp UTXO bổ sung (step 3)
    pub fn add_receiver(
        &mut self,
        wallet:      Wallet,
        utxo_tx_id: String,
        utxo_index: usize,
        utxo_amount: u64,
    ) {
        self.receiver_wallet = Some(wallet);
        self.receiver_utxo   = Some((utxo_tx_id, utxo_index, utxo_amount));
    }

    /// Build PayJoin TX:
    ///   Inputs:  [sender_utxo, receiver_utxo]   ← 2 inputs, không ai biết ai
    ///   Outputs: [receiver_output, sender_change]
    ///
    /// Receiver output = payment_amount + receiver_utxo_amount
    /// → trông như receiver gộp UTXO, không phải nhận payment
    pub fn build(&self) -> Result<Transaction, String> {
        let sender   = self.sender_wallet.as_ref().ok_or("❌ Thiếu sender")?;
        let receiver = self.receiver_wallet.as_ref().ok_or("❌ Thiếu receiver")?;
        let (s_txid, s_idx, s_amount) = self.sender_utxo.as_ref().ok_or("❌ Thiếu sender UTXO")?;
        let (r_txid, r_idx, r_amount) = self.receiver_utxo.as_ref().ok_or("❌ Thiếu receiver UTXO")?;

        let total_in = s_amount + r_amount;
        if total_in < self.payment_amount + self.fee {
            return Err(format!(
                "❌ Không đủ funds: {} + {} < {} + fee",
                s_amount, r_amount, self.payment_amount
            ));
        }

        let sender_hash   = hex::encode(Script::pubkey_hash(&sender.public_key.serialize()));
        let receiver_hash = hex::encode(Script::pubkey_hash(&receiver.public_key.serialize()));

        // Receiver output = payment + receiver UTXO "absorbed"
        // → observer thấy large output, không biết đó là payment hay consolidation
        let receiver_out = self.payment_amount + r_amount;
        let sender_change = total_in - self.payment_amount - self.fee;

        let inputs_raw = vec![
            (s_txid.clone(), *s_idx),
            (r_txid.clone(), *r_idx),
        ];

        let mut tx_outputs = vec![
            TxOutput::p2pkh(receiver_out, &receiver_hash),
        ];
        if sender_change > 0 {
            tx_outputs.push(TxOutput::p2pkh(sender_change, &sender_hash));
        }

        let mut tx = Transaction::new_unsigned(inputs_raw, tx_outputs, self.fee);

        // Cả 2 ký — signing_data giống nhau (bao gồm cả 2 inputs + outputs)
        let signing_data     = tx.signing_data();
        let sender_sig       = sender.sign(&signing_data);
        let receiver_sig     = receiver.sign(&signing_data);

        tx.inputs[0].script_sig = Script::p2pkh_sig(&sender_sig,   &sender.public_key_hex());
        tx.inputs[1].script_sig = Script::p2pkh_sig(&receiver_sig, &receiver.public_key_hex());

        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();

        Ok(tx)
    }
}

// ── PayJoin Observer Analysis ─────────────────────────────────

/// Phân tích TX thông thường vs PayJoin từ góc nhìn observer
pub struct TxAnalysis;

impl TxAnalysis {
    /// Heuristic thông thường: "change detection"
    /// Thất bại với PayJoin vì cả 2 inputs đều hợp lệ
    pub fn naive_change_heuristic(tx: &Transaction) -> String {
        let in_count  = tx.inputs.len();
        let out_count = tx.outputs.len();
        if in_count == 1 && out_count == 2 {
            "Thông thường: 1 input → 2 outputs (payment + change)".to_string()
        } else if in_count == 2 && out_count == 2 {
            "PayJoin: 2 inputs → 2 outputs — không xác định được sender/receiver ✅".to_string()
        } else {
            format!("TX: {} inputs → {} outputs", in_count, out_count)
        }
    }

    /// Common input ownership heuristic (CIOH):
    /// Giả sử tất cả inputs thuộc cùng 1 người — SAI với CoinJoin/PayJoin
    pub fn common_input_heuristic(tx: &Transaction) -> String {
        if tx.inputs.len() > 1 {
            "CIOH: giả sử tất cả inputs cùng chủ — SAI với CoinJoin/PayJoin ❌".to_string()
        } else {
            "CIOH: 1 input → giả sử valid".to_string()
        }
    }
}

// ── CoinJoin Transcript ───────────────────────────────────────

/// Ghi lại session để audit (không lưu identities)
#[derive(Debug)]
pub struct CoinJoinTranscript {
    pub session_id:        String,
    pub denomination:      u64,
    pub participant_count: usize,
    pub total_fee:         u64,
    pub anonymity_set:     usize,
    pub tx_id:             String,
}

impl CoinJoinTranscript {
    pub fn from_session(session: &CoinJoinSession, cj_tx: &CoinJoinTx) -> Self {
        // session_id = H(tx_id)
        let session_id = hex::encode(blake3::hash(cj_tx.tx.tx_id.as_bytes()).as_bytes());
        CoinJoinTranscript {
            session_id:        session_id[..16].to_string(),
            denomination:      session.denomination,
            participant_count: cj_tx.participant_count,
            total_fee:         session.fee_per_input * cj_tx.participant_count as u64,
            anonymity_set:     cj_tx.anonymity_set(),
            tx_id:             cj_tx.tx.tx_id[..16].to_string(),
        }
    }
}
