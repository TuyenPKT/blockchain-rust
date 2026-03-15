use std::collections::HashMap;
use crate::transaction::{Transaction, TxOutput};
use crate::script::Opcode;

#[derive(Debug, Clone)]
pub struct Utxo {
    pub tx_id:        String,
    pub output_index: usize,
    pub output:       TxOutput,
}

pub struct UtxoSet {
    pub utxos: HashMap<String, TxOutput>,
}

impl Default for UtxoSet {
    fn default() -> Self { Self::new() }
}

#[allow(dead_code)]
impl UtxoSet {
    pub fn new() -> Self { UtxoSet { utxos: HashMap::new() } }

    fn key(tx_id: &str, index: usize) -> String { format!("{}:{}", tx_id, index) }

    pub fn apply_block(&mut self, transactions: &[Transaction]) {
        for tx in transactions {
            for input in &tx.inputs {
                self.utxos.remove(&Self::key(&input.tx_id, input.output_index));
            }
            for (i, output) in tx.outputs.iter().enumerate() {
                self.utxos.insert(Self::key(&tx.tx_id, i), output.clone());
            }
        }
    }

    /// Lấy identifier từ output script:
    ///   P2PKH:  20-byte pubkey hash (pos 2)
    ///   P2WPKH: 20-byte pubkey hash (pos 1, via p2wpkh_hash)
    ///   P2TR:   32-byte xonly pubkey (pos 1, via p2tr_xonly)
    ///   P2SH:   20-byte script hash (pos 1)
    /// Dùng typed methods để tránh nhầm lẫn giữa các script types.
    fn owner_bytes_of(output: &TxOutput) -> Option<Vec<u8>> {
        // P2WPKH: OP_0 <20 bytes>
        if let Some(h) = output.script_pubkey.p2wpkh_hash() {
            return Some(h.clone());
        }
        // P2TR: OP_1 <32 bytes>
        if let Some(h) = output.script_pubkey.p2tr_xonly() {
            return Some(h.clone());
        }
        // P2PKH: OP_DUP OP_HASH160 <20 bytes> OP_EQUALVERIFY OP_CHECKSIG
        if let [Opcode::OpDup, Opcode::OpHash160, Opcode::OpPushData(d),
                Opcode::OpEqualVerify, Opcode::OpCheckSig] = output.script_pubkey.ops.as_slice() {
            if d.len() == 20 { return Some(d.clone()); }
        }
        // P2SH: OP_HASH160 <20 bytes> OP_EQUAL
        if output.script_pubkey.is_p2sh() {
            for op in &output.script_pubkey.ops {
                if let Opcode::OpPushData(d) = op {
                    if d.len() == 20 { return Some(d.clone()); }
                }
            }
        }
        None
    }

    /// Số dư theo pubkey_hash hex (20 bytes) hoặc xonly hex (32 bytes)
    pub fn balance_of(&self, pubkey_hash_hex: &str) -> u64 {
        let target = hex::decode(pubkey_hash_hex).unwrap_or_default();
        self.utxos.values()
            .filter(|o| Self::owner_bytes_of(o).as_deref() == Some(target.as_slice()))
            .map(|o| o.amount)
            .sum()
    }

    /// UTXO của một pubkey_hash hoặc xonly pubkey
    pub fn utxos_of(&self, pubkey_hash_hex: &str) -> Vec<Utxo> {
        let target = hex::decode(pubkey_hash_hex).unwrap_or_default();
        self.utxos.iter()
            .filter(|(_, o)| Self::owner_bytes_of(o).as_deref() == Some(target.as_slice()))
            .map(|(k, o)| {
                let parts: Vec<&str> = k.splitn(2, ':').collect();
                Utxo {
                    tx_id:        parts[0].to_string(),
                    output_index: parts[1].parse().unwrap_or(0),
                    output:       o.clone(),
                }
            })
            .collect()
    }

    pub fn is_unspent(&self, tx_id: &str, output_index: usize) -> bool {
        self.utxos.contains_key(&Self::key(tx_id, output_index))
    }

    pub fn get_amount(&self, tx_id: &str, output_index: usize) -> Option<u64> {
        self.utxos.get(&Self::key(tx_id, output_index)).map(|o| o.amount)
    }

    pub fn get_script_pubkey(&self, tx_id: &str, output_index: usize) -> Option<&TxOutput> {
        self.utxos.get(&Self::key(tx_id, output_index))
    }

    pub fn total_supply(&self) -> u64 {
        self.utxos.values().map(|o| o.amount).sum()
    }
}

