//! Transaction — v1.1 SegWit (BIP141/143)
//!
//! Thay đổi so với v1.0:
//!   - TxInput thêm `witness: Vec<Vec<u8>>`
//!   - txid  = hash không bao gồm witness  → fix malleability
//!   - wtxid = hash bao gồm witness
//!   - segwit_signing_data() dùng BIP143 format (bao gồm input amount)
//!   - vsize() tính theo weight (witness discount ×0.25)
//!
//! P2WPKH flow:
//!   scriptPubKey: OP_0 <hash20>
//!   scriptSig:    (rỗng)
//!   witness:      [<sig_bytes>, <pubkey_bytes>]

use serde::{Serialize, Deserialize};
use crate::script::Script;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub tx_id:        String,
    pub output_index: usize,
    pub script_sig:   Script,
    pub sequence:     u32,
    pub witness:      Vec<Vec<u8>>, // ← v1.1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub amount:        u64,
    pub script_pubkey: Script,
}

#[allow(dead_code)]
impl TxOutput {
    pub fn p2pkh(amount: u64, pubkey_hash_hex: &str) -> Self {
        TxOutput { amount, script_pubkey: Script::p2pkh_pubkey(pubkey_hash_hex) }
    }
    pub fn p2pk(amount: u64, pubkey_hex: &str) -> Self {
        TxOutput { amount, script_pubkey: Script::p2pk_pubkey(pubkey_hex) }
    }
    pub fn p2sh(amount: u64, script_hash_hex: &str) -> Self {
        TxOutput { amount, script_pubkey: Script::p2sh_pubkey(script_hash_hex) }
    }
    /// P2WPKH output ← v1.1: OP_0 <hash20>
    pub fn p2wpkh(amount: u64, pubkey_hash_hex: &str) -> Self {
        TxOutput { amount, script_pubkey: Script::p2wpkh_pubkey(pubkey_hash_hex) }
    }
    /// CTV output ← v1.4: <32-byte template_hash> OP_CTV
    pub fn ctv_output(amount: u64, template_hash_hex: &str) -> Self {
        TxOutput { amount, script_pubkey: Script::ctv_pubkey(template_hash_hex) }
    }

    /// P2TR output ← v1.3: OP_1 <32-byte tweaked x-only pubkey>
    pub fn p2tr(amount: u64, tweaked_xonly_hex: &str) -> Self {
        TxOutput { amount, script_pubkey: Script::p2tr_pubkey(tweaked_xonly_hex) }
    }

    pub fn op_return(data: &[u8]) -> Self {
        TxOutput { amount: 0, script_pubkey: Script::op_return(data) }
    }
    pub fn to_address_hint(&self) -> String {
        for op in &self.script_pubkey.ops {
            if let crate::script::Opcode::OpPushData(data) = op {
                if data.len() == 20 { return format!("hash20:{}", hex::encode(data)); }
            }
        }
        "unknown".to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub tx_id:       String, // không có witness
    pub wtx_id:      String, // có witness ← v1.1
    pub inputs:      Vec<TxInput>,
    pub outputs:     Vec<TxOutput>,
    pub is_coinbase: bool,
    pub fee:         u64,
}

impl Transaction {
    pub fn new_unsigned(
        inputs_raw: Vec<(String, usize)>,
        outputs:    Vec<TxOutput>,
        fee:        u64,
    ) -> Self {
        let inputs = inputs_raw.into_iter().map(|(tx_id, output_index)| TxInput {
            tx_id,
            output_index,
            script_sig: Script::empty(),
            sequence:   0xFFFFFFFF,
            witness:    vec![],
        }).collect();
        let mut tx = Transaction {
            tx_id: String::new(), wtx_id: String::new(),
            inputs, outputs, is_coinbase: false, fee,
        };
        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();
        tx
    }

    /// txid: hash inputs+outputs BỎ QUA witness
    pub fn calculate_txid(&self) -> String {
        let data = format!(
            "txid|{:?}|{:?}|{}",
            self.inputs.iter().map(|i| (&i.tx_id, i.output_index, i.sequence)).collect::<Vec<_>>(),
            self.outputs.iter().map(|o| o.amount).collect::<Vec<_>>(),
            self.is_coinbase,
        );
        hex::encode(blake3::hash(data.as_bytes()).as_bytes())
    }

    /// wtxid: hash inputs+outputs+witness
    pub fn calculate_wtxid(&self) -> String {
        let wit: Vec<Vec<String>> = self.inputs.iter()
            .map(|i| i.witness.iter().map(hex::encode).collect())
            .collect();
        let data = format!(
            "wtxid|{:?}|{:?}|{}|{:?}",
            self.inputs.iter().map(|i| (&i.tx_id, i.output_index, i.sequence)).collect::<Vec<_>>(),
            self.outputs.iter().map(|o| o.amount).collect::<Vec<_>>(),
            self.is_coinbase,
            wit,
        );
        hex::encode(blake3::hash(data.as_bytes()).as_bytes())
    }

    /// BIP143 sighash cho SegWit — bao gồm input_amount
    /// ngăn tấn công: signer biết chính xác bao nhiêu sat đang tiêu
    pub fn segwit_signing_data(&self, input_index: usize, input_amount: u64) -> Vec<u8> {
        let data = format!(
            "segwit|{:?}|{:?}|{}|{}",
            self.inputs.iter().map(|i| (&i.tx_id, i.output_index)).collect::<Vec<_>>(),
            self.outputs.iter().map(|o| o.amount).collect::<Vec<_>>(),
            input_index,
            input_amount,
        );
        blake3::hash(data.as_bytes()).as_bytes().to_vec()
    }

    /// Legacy signing_data (P2PKH, P2SH)
    pub fn signing_data(&self) -> Vec<u8> {
        let data = format!(
            "{:?}|{:?}",
            self.inputs.iter().map(|i| (&i.tx_id, i.output_index)).collect::<Vec<_>>(),
            self.outputs.iter().map(|o| o.amount).collect::<Vec<_>>(),
        );
        blake3::hash(data.as_bytes()).as_bytes().to_vec()
    }

    pub fn calculate_id(&self) -> String { self.calculate_txid() }

    #[allow(dead_code)]
    pub fn coinbase(miner_pubkey_hash: &str, total_fee: u64) -> Self {
        Self::coinbase_at(miner_pubkey_hash, total_fee, 0)
    }

    /// BIP34-style coinbase: encode block height vào coinbase input
    /// để mỗi block có tx_id duy nhất dù cùng miner address và fee.
    pub fn coinbase_at(miner_pubkey_hash: &str, total_fee: u64, height: u64) -> Self {
        let subsidy = 50_000_000_00u64;
        let outputs = vec![TxOutput::p2pkh(subsidy + total_fee, miner_pubkey_hash)];
        // Coinbase input: tx_id = all zeros, output_index = block height (BIP34)
        let coinbase_input = TxInput {
            tx_id:        "0".repeat(64),
            output_index: height as usize,
            script_sig:   Script::empty(),
            sequence:     0xFFFFFFFF,
            witness:      vec![],
        };
        let mut tx = Transaction {
            tx_id: String::new(), wtx_id: String::new(),
            inputs: vec![coinbase_input], outputs, is_coinbase: true, fee: 0,
        };
        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();
        tx
    }

    pub fn total_output(&self) -> u64 { self.outputs.iter().map(|o| o.amount).sum() }

    pub fn is_segwit(&self) -> bool { self.inputs.iter().any(|i| !i.witness.is_empty()) }

    /// Virtual size (vbytes): witness data tính weight ×0.25
    /// vsize = (base_weight + witness_weight) / 4
    /// base_weight = non-witness bytes × 4
    /// witness_weight = witness bytes × 1
    pub fn vsize(&self) -> usize {
        let base  = 10 + self.inputs.len() * 41 + self.outputs.len() * 31;
        let wit: usize = self.inputs.iter()
            .map(|i| i.witness.iter().map(|w| w.len() + 1).sum::<usize>() + 2)
            .sum();
        if self.is_segwit() {
            (base * 4 + wit + 2) / 4
        } else {
            base
        }
    }

    pub fn is_valid(&self) -> bool {
        if self.is_coinbase { return self.outputs.len() == 1; }
        !self.inputs.is_empty() && !self.outputs.is_empty()
    }
}
