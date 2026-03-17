#![allow(dead_code)]
//! v7.3 — Token Transfer TX

use serde::{Serialize, Deserialize};
use crate::transaction::Transaction;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTx {
    pub token_id: String,
    pub from: String,
    pub to: String,
    pub amount: u128,
    pub memo: String,
    pub nonce: u64,
}

impl TokenTx {
    pub fn new(token_id: &str, from: &str, to: &str, amount: u128) -> Self {
        TokenTx {
            token_id: token_id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            amount,
            memo: String::new(),
            nonce: 0,
        }
    }

    pub fn with_memo(mut self, memo: &str) -> Self {
        self.memo = memo.to_string();
        self
    }

    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    pub fn decode(data: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(data).map_err(|e| e.to_string())
    }

    pub fn txid(&self) -> String {
        let encoded = self.encode();
        hex::encode(blake3::hash(&encoded).as_bytes())
    }

    pub fn to_op_return_data(&self) -> Vec<u8> {
        let prefix = b"TOKEN:";
        let encoded = self.encode();
        let mut data = Vec::with_capacity(prefix.len() + encoded.len());
        data.extend_from_slice(prefix);
        data.extend_from_slice(&encoded);
        // Truncate to 80 bytes max
        data.truncate(80);
        data
    }

    pub fn from_op_return(data: &[u8]) -> Option<Self> {
        let prefix = b"TOKEN:";
        if data.len() < prefix.len() {
            return None;
        }
        if &data[..prefix.len()] != prefix {
            return None;
        }
        Self::decode(&data[prefix.len()..]).ok()
    }
}

#[derive(Debug, Default)]
pub struct TokenTxBuilder {
    token_id: Option<String>,
    from: Option<String>,
    to: Option<String>,
    amount: Option<u128>,
}

impl TokenTxBuilder {
    pub fn new() -> Self {
        TokenTxBuilder::default()
    }

    pub fn token(mut self, id: &str) -> Self {
        self.token_id = Some(id.to_string());
        self
    }

    pub fn from_addr(mut self, addr: &str) -> Self {
        self.from = Some(addr.to_string());
        self
    }

    pub fn to_addr(mut self, addr: &str) -> Self {
        self.to = Some(addr.to_string());
        self
    }

    pub fn amount(mut self, v: u128) -> Self {
        self.amount = Some(v);
        self
    }

    pub fn build(self) -> Result<TokenTx, String> {
        let token_id = self.token_id.ok_or("token_id required")?;
        let from = self.from.ok_or("from required")?;
        let to = self.to.ok_or("to required")?;
        let amount = self.amount.ok_or("amount required")?;
        Ok(TokenTx::new(&token_id, &from, &to, amount))
    }
}

pub fn extract_token_txs(block_txs: &[Transaction]) -> Vec<TokenTx> {
    let mut result = Vec::new();
    for tx in block_txs {
        for output in &tx.outputs {
            // Look for OP_RETURN outputs
            for op in &output.script_pubkey.ops {
                if let crate::script::Opcode::OpReturn = op {
                    // Find the data push after OP_RETURN
                    // We need to find data in the ops
                    break;
                }
                if let crate::script::Opcode::OpPushData(data) = op {
                    if let Some(ttx) = TokenTx::from_op_return(data) {
                        result.push(ttx);
                    }
                }
            }
            // Also check if the script is an OP_RETURN output
            let ops = &output.script_pubkey.ops;
            if ops.len() >= 2 {
                if let crate::script::Opcode::OpReturn = &ops[0] {
                    if let crate::script::Opcode::OpPushData(data) = &ops[1] {
                        if let Some(ttx) = TokenTx::from_op_return(data) {
                            result.push(ttx);
                        }
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let ttx = TokenTx::new("PKT", "alice", "bob", 1000);
        let encoded = ttx.encode();
        let decoded = TokenTx::decode(&encoded).unwrap();
        assert_eq!(decoded.token_id, "PKT");
        assert_eq!(decoded.from, "alice");
        assert_eq!(decoded.to, "bob");
        assert_eq!(decoded.amount, 1000);
    }

    #[test]
    fn test_txid_deterministic() {
        let ttx1 = TokenTx::new("PKT", "alice", "bob", 1000);
        let ttx2 = TokenTx::new("PKT", "alice", "bob", 1000);
        assert_eq!(ttx1.txid(), ttx2.txid());
    }

    #[test]
    fn test_txid_different_for_different_tx() {
        let ttx1 = TokenTx::new("PKT", "alice", "bob", 1000);
        let ttx2 = TokenTx::new("PKT", "alice", "bob", 2000);
        assert_ne!(ttx1.txid(), ttx2.txid());
    }

    #[test]
    fn test_op_return_roundtrip_short() {
        // Create a short token tx that fits in 80 bytes
        let ttx = TokenTx::new("A", "B", "C", 1);
        let data = ttx.to_op_return_data();
        // Should start with TOKEN:
        assert!(data.starts_with(b"TOKEN:"));
        let recovered = TokenTx::from_op_return(&data).unwrap();
        assert_eq!(recovered.token_id, "A");
        assert_eq!(recovered.from, "B");
        assert_eq!(recovered.to, "C");
    }

    #[test]
    fn test_op_return_invalid_prefix() {
        let data = b"NOTTOKEN:{}";
        assert!(TokenTx::from_op_return(data).is_none());
    }

    #[test]
    fn test_builder_success() {
        let ttx = TokenTxBuilder::new()
            .token("PKT")
            .from_addr("alice")
            .to_addr("bob")
            .amount(500)
            .build()
            .unwrap();
        assert_eq!(ttx.token_id, "PKT");
        assert_eq!(ttx.amount, 500);
    }

    #[test]
    fn test_builder_missing_field() {
        let result = TokenTxBuilder::new()
            .token("PKT")
            .from_addr("alice")
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_with_memo() {
        let ttx = TokenTx::new("PKT", "alice", "bob", 100)
            .with_memo("payment");
        assert_eq!(ttx.memo, "payment");
    }

    #[test]
    fn test_extract_token_txs_empty() {
        let txs: Vec<Transaction> = vec![];
        let result = extract_token_txs(&txs);
        assert!(result.is_empty());
    }
}
