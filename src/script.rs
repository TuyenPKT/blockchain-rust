//! Bitcoin Script — v1.0 thêm P2SH + Multisig hoàn chỉnh
//!
//! Script types:
//!   P2PK   — Pay-to-Public-Key (Bitcoin 0.1)
//!   P2PKH  — Pay-to-Public-Key-Hash (standard)
//!   P2SH   — Pay-to-Script-Hash (BIP16, 2012)
//!            cho phép ẩn script phức tạp (multisig) vào 1 hash 20 bytes
//!
//! P2SH flow:
//!   redeemScript = <M> <pub1> ... <pubN> <N> OP_CHECKMULTISIG
//!   scriptPubKey = OP_HASH160 <HASH160(redeemScript)> OP_EQUAL
//!   scriptSig    = OP_0 <sig1> ... <sigM> <redeemScript_bytes>
//!
//! Interpreter chạy P2SH:
//!   1. Chạy scriptSig → stack có [OP_0, sig1..sigM, redeemScript_bytes]
//!   2. Chạy scriptPubKey → check HASH160(redeemScript) == expected → push 1
//!   3. Detect P2SH: deserialize redeemScript từ stack, chạy tiếp
//!   4. Stack top = 1 → hợp lệ

use ripemd::{Ripemd160, Digest as RipemdDigest};
use serde::{Serialize, Deserialize};

// ── Opcodes ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Opcode {
    // Stack
    OpDup, OpDrop, OpSwap, OpOver,
    // Crypto
    OpHash160, OpHash256,
    OpCheckSig, OpCheckMultiSig,
    // Comparison
    OpEqualVerify, OpEqual,
    // Flow
    OpVerify, OpReturn,
    // Numbers — dùng cho multisig M và N
    OpNum(i64),
    // SegWit version push (OP_0 = witness version 0)
    Op0,          // ← v1.1: witness version 0 (SegWit)
    Op1,          // ← v1.3: witness version 1 (Taproot)
    OpCheckTemplateVerify, // ← v1.4: CTV BIP119
    // Data push
    OpPushData(Vec<u8>),
}

// ── Script ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Script {
    pub ops: Vec<Opcode>,
}

#[allow(dead_code)]
impl Script {
    pub fn new(ops: Vec<Opcode>) -> Self { Script { ops } }
    pub fn empty() -> Self { Script { ops: vec![] } }

    // ── Serialization ────────────────────────────────────────

    /// Serialize script thành bytes (để hash hoặc nhúng vào scriptSig P2SH)
    /// Format đơn giản: mỗi opcode encode thành tagged bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let json = serde_json::to_vec(self).unwrap_or_default();
        json
    }

    /// Deserialize từ bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    /// HASH160 của serialized script — dùng làm P2SH address identifier
    pub fn script_hash(script_bytes: &[u8]) -> Vec<u8> {
        Ripemd160::digest(blake3::hash(script_bytes).as_bytes()).to_vec()
    }

    // ── Standard script builders ─────────────────────────────

    /// P2PK scriptPubKey: <pubkey> OP_CHECKSIG
    pub fn p2pk_pubkey(pubkey_hex: &str) -> Self {
        Script::new(vec![
            Opcode::OpPushData(hex::decode(pubkey_hex).unwrap_or_default()),
            Opcode::OpCheckSig,
        ])
    }

    /// P2PK scriptSig: <sig>
    pub fn p2pk_sig(sig_hex: &str) -> Self {
        Script::new(vec![Opcode::OpPushData(hex::decode(sig_hex).unwrap_or_default())])
    }

    /// P2PKH scriptPubKey: OP_DUP OP_HASH160 <hash20> OP_EQUALVERIFY OP_CHECKSIG
    pub fn p2pkh_pubkey(pubkey_hash_hex: &str) -> Self {
        Script::new(vec![
            Opcode::OpDup,
            Opcode::OpHash160,
            Opcode::OpPushData(hex::decode(pubkey_hash_hex).unwrap_or_default()),
            Opcode::OpEqualVerify,
            Opcode::OpCheckSig,
        ])
    }

    /// P2PKH scriptSig: <sig> <pubkey>
    pub fn p2pkh_sig(sig_hex: &str, pubkey_hex: &str) -> Self {
        Script::new(vec![
            Opcode::OpPushData(hex::decode(sig_hex).unwrap_or_default()),
            Opcode::OpPushData(hex::decode(pubkey_hex).unwrap_or_default()),
        ])
    }

    /// P2SH scriptPubKey: OP_HASH160 <script_hash20> OP_EQUAL
    pub fn p2sh_pubkey(script_hash_hex: &str) -> Self {
        Script::new(vec![
            Opcode::OpHash160,
            Opcode::OpPushData(hex::decode(script_hash_hex).unwrap_or_default()),
            Opcode::OpEqual,
        ])
    }

    /// P2SH scriptSig: OP_0 <sig1> ... <sigM> <redeem_script_bytes>
    /// OP_0 = dummy vì off-by-one bug của OP_CHECKMULTISIG
    pub fn p2sh_sig(sig_hexes: &[String], redeem_script: &Script) -> Self {
        let mut ops = vec![Opcode::OpPushData(vec![])]; // OP_0 dummy
        for sig_hex in sig_hexes {
            ops.push(Opcode::OpPushData(hex::decode(sig_hex).unwrap_or_default()));
        }
        // Cuối cùng push serialized redeemScript
        ops.push(Opcode::OpPushData(redeem_script.to_bytes()));
        Script::new(ops)
    }

    /// RedeemScript cho M-of-N multisig:
    /// <M> <pub1> <pub2> ... <pubN> <N> OP_CHECKMULTISIG
    pub fn multisig_redeem(m: usize, pubkey_hexes: &[String]) -> Self {
        let n = pubkey_hexes.len();
        let mut ops = vec![Opcode::OpNum(m as i64)];
        for pk in pubkey_hexes {
            ops.push(Opcode::OpPushData(hex::decode(pk).unwrap_or_default()));
        }
        ops.push(Opcode::OpNum(n as i64));
        ops.push(Opcode::OpCheckMultiSig);
        Script::new(ops)
    }

    /// OP_RETURN data output (unspendable, lưu metadata on-chain)
    /// P2WPKH scriptPubKey ← v1.1: OP_0 <20-byte pubkey_hash>
    pub fn p2wpkh_pubkey(pubkey_hash_hex: &str) -> Self {
        Script::new(vec![
            Opcode::Op0,
            Opcode::OpPushData(hex::decode(pubkey_hash_hex).unwrap_or_default()),
        ])
    }

    /// Kiểm tra script có phải P2WPKH không: OP_0 <20 bytes>
    pub fn is_p2wpkh(&self) -> bool {
        matches!(
            self.ops.as_slice(),
            [Opcode::Op0, Opcode::OpPushData(d)] if d.len() == 20
        )
    }

    /// Lấy pubkey_hash từ P2WPKH script
    pub fn p2wpkh_hash(&self) -> Option<&Vec<u8>> {
        if let [Opcode::Op0, Opcode::OpPushData(d)] = self.ops.as_slice() {
            if d.len() == 20 { return Some(d); }
        }
        None
    }

    /// P2TR scriptPubKey ← v1.3: OP_1 <32-byte x-only tweaked pubkey>
    pub fn p2tr_pubkey(tweaked_xonly_hex: &str) -> Self {
        Script::new(vec![
            Opcode::Op1,
            Opcode::OpPushData(hex::decode(tweaked_xonly_hex).unwrap_or_default()),
        ])
    }

    /// Kiểm tra script có phải P2TR không: OP_1 <32 bytes>
    pub fn is_p2tr(&self) -> bool {
        matches!(
            self.ops.as_slice(),
            [Opcode::Op1, Opcode::OpPushData(d)] if d.len() == 32
        )
    }

    /// Lấy tweaked x-only pubkey từ P2TR script (32 bytes)
    pub fn p2tr_xonly(&self) -> Option<&Vec<u8>> {
        if let [Opcode::Op1, Opcode::OpPushData(d)] = self.ops.as_slice() {
            if d.len() == 32 { return Some(d); }
        }
        None
    }

    /// CTV scriptPubKey ← v1.4: <32-byte template_hash> OP_CTV
    pub fn ctv_pubkey(template_hash_hex: &str) -> Self {
        Script::new(vec![
            Opcode::OpPushData(hex::decode(template_hash_hex).unwrap_or_default()),
            Opcode::OpCheckTemplateVerify,
        ])
    }

    /// Kiểm tra script có phải CTV không: OpPushData(32) OP_CTV
    pub fn is_ctv(&self) -> bool {
        matches!(
            self.ops.as_slice(),
            [Opcode::OpPushData(d), Opcode::OpCheckTemplateVerify] if d.len() == 32
        )
    }

    /// Lấy template hash từ CTV script (32 bytes)
    pub fn ctv_template_hash(&self) -> Option<&Vec<u8>> {
        if let [Opcode::OpPushData(d), Opcode::OpCheckTemplateVerify] = self.ops.as_slice() {
            if d.len() == 32 { return Some(d); }
        }
        None
    }

    pub fn op_return(data: &[u8]) -> Self {
        Script::new(vec![Opcode::OpReturn, Opcode::OpPushData(data.to_vec())])
    }

    // ── Helpers ──────────────────────────────────────────────

    /// RIPEMD160(SHA256(pubkey)) — 20 bytes
    pub fn pubkey_hash(pubkey_bytes: &[u8]) -> Vec<u8> {
        Ripemd160::digest(blake3::hash(pubkey_bytes).as_bytes()).to_vec()
    }

    /// Kiểm tra script có phải P2SH không:
    /// OP_HASH160 <20 bytes> OP_EQUAL — đúng 3 ops
    pub fn is_p2sh(&self) -> bool {
        matches!(
            self.ops.as_slice(),
            [Opcode::OpHash160, Opcode::OpPushData(d), Opcode::OpEqual]
            if d.len() == 20
        )
    }

    pub fn serialize_ops(&self) -> String {
        self.ops.iter().map(|op| match op {
            Opcode::OpPushData(d) => format!("PUSH({}b)", d.len()),
            Opcode::OpNum(n)      => format!("NUM({})", n),
            Opcode::Op0           => "OP_0".to_string(),
            Opcode::Op1                     => "OP_1".to_string(),
            Opcode::OpCheckTemplateVerify   => "OP_CTV".to_string(),
            other                 => format!("{:?}", other),
        }).collect::<Vec<_>>().join(" ")
    }
}

// ── ScriptInterpreter ────────────────────────────────────────────────────────

pub struct ScriptInterpreter {
    stack: Vec<Vec<u8>>,
}

impl ScriptInterpreter {
    pub fn new() -> Self { ScriptInterpreter { stack: vec![] } }

    /// Chạy scriptSig + scriptPubKey.
    /// Nếu scriptPubKey là P2SH → tự động chạy redeemScript từ stack.
    pub fn execute(
        &mut self,
        script_sig:    &Script,
        script_pubkey: &Script,
        signing_data:  &[u8],
    ) -> bool {
        // Bước 1: chạy scriptSig
        for op in &script_sig.ops {
            if !self.step(op, signing_data) { return false; }
        }

        // Bước 2: P2SH — lưu lại redeemScript trước khi chạy scriptPubKey
        let redeem_script_bytes = if script_pubkey.is_p2sh() {
            // redeemScript là phần tử cuối cùng scriptSig push vào stack
            self.stack.last().cloned()
        } else {
            None
        };

        // Bước 3: chạy scriptPubKey
        for op in &script_pubkey.ops {
            if !self.step(op, signing_data) { return false; }
        }

        // Kiểm tra stack top = true
        let top_ok = match self.stack.last() {
            Some(top) => !top.is_empty() && top.iter().any(|&b| b != 0),
            None      => false,
        };
        if !top_ok { return false; }

        // Bước 4: P2SH — deserialize và chạy redeemScript
        if let Some(rs_bytes) = redeem_script_bytes {
            let redeem = match Script::from_bytes(&rs_bytes) {
                Some(s) => s,
                None    => return false,
            };
            // Pop redeemScript bytes khỏi stack (đã được hash-check)
            self.stack.pop();
            // Chạy redeemScript với stack hiện tại (chứa sigs)
            for op in &redeem.ops {
                if !self.step(op, signing_data) { return false; }
            }
            return match self.stack.last() {
                Some(top) => !top.is_empty() && top.iter().any(|&b| b != 0),
                None      => false,
            };
        }

        true
    }

    fn pop(&mut self) -> Option<Vec<u8>> { self.stack.pop() }

    fn step(&mut self, op: &Opcode, signing_data: &[u8]) -> bool {
        match op {
            Opcode::OpPushData(data) => { self.stack.push(data.clone()); }

            Opcode::OpNum(n) => {
                self.stack.push(vec![*n as u8]);
            }

            Opcode::Op0 => {
                self.stack.push(vec![]);
            }

            Opcode::Op1 => {
                // OP_1 push [1] (witness version 1 = Taproot)
                self.stack.push(vec![1]);
            }

            Opcode::OpCheckTemplateVerify => {
                // ← v1.4: BIP119 CTV
                // Stack top = expected template_hash (32 bytes)
                // Runtime hash của spending TX phải match
                // Simplified: pop template_hash, push 1 (validation done in chain.rs)
                // Full validation: chain.rs::validate_ctv() compare spending TX hash
                if self.stack.is_empty() { return false; }
                let top = self.stack.last().unwrap();
                if top.len() != 32 { return false; }
                // Push success marker (actual TX hash check happens in chain.rs)
                self.stack.push(vec![1]);
            }

            Opcode::OpDup => {
                let top = match self.stack.last() { Some(t) => t.clone(), None => return false };
                self.stack.push(top);
            }

            Opcode::OpDrop => { if self.pop().is_none() { return false; } }

            Opcode::OpSwap => {
                let len = self.stack.len();
                if len < 2 { return false; }
                self.stack.swap(len - 1, len - 2);
            }

            Opcode::OpOver => {
                let len = self.stack.len();
                if len < 2 { return false; }
                let item = self.stack[len - 2].clone();
                self.stack.push(item);
            }

            Opcode::OpHash160 => {
                let top = match self.pop() { Some(t) => t, None => return false };
                self.stack.push(Script::pubkey_hash(&top));
            }

            Opcode::OpHash256 => {
                let top = match self.pop() { Some(t) => t, None => return false };
                self.stack.push(blake3::hash(blake3::hash(&top).as_bytes()).as_bytes().to_vec());
            }

            Opcode::OpEqual => {
                let b = match self.pop() { Some(t) => t, None => return false };
                let a = match self.pop() { Some(t) => t, None => return false };
                self.stack.push(if a == b { vec![1] } else { vec![0] });
            }

            Opcode::OpEqualVerify => {
                let b = match self.pop() { Some(t) => t, None => return false };
                let a = match self.pop() { Some(t) => t, None => return false };
                if a != b { return false; }
            }

            Opcode::OpVerify => {
                let top = match self.pop() { Some(t) => t, None => return false };
                if top.is_empty() || top.iter().all(|&b| b == 0) { return false; }
            }

            Opcode::OpCheckSig => {
                let pubkey_bytes = match self.pop() { Some(t) => t, None => return false };
                let sig_bytes    = match self.pop() { Some(t) => t, None => return false };
                self.stack.push(
                    if verify_ecdsa(&pubkey_bytes, signing_data, &sig_bytes) { vec![1] } else { vec![0] }
                );
            }

            Opcode::OpCheckMultiSig => {
                // Stack (bottom→top): OP_0 <sig1..sigK> <K> <pub1..pubM> <M>
                // pop M pubkeys, pop K sigs, verify K-of-M
                let m = match self.pop() {
                    Some(t) => t.first().copied().unwrap_or(0) as usize,
                    None    => return false,
                };
                let mut pubkeys = vec![];
                for _ in 0..m {
                    match self.pop() { Some(t) => pubkeys.push(t), None => return false }
                }
                let k = match self.pop() {
                    Some(t) => t.first().copied().unwrap_or(0) as usize,
                    None    => return false,
                };
                let mut sigs = vec![];
                for _ in 0..k {
                    match self.pop() { Some(t) => sigs.push(t), None => return false }
                }
                self.pop(); // consume OP_0 dummy (Bitcoin off-by-one)

                // Mỗi sig phải match 1 pubkey theo thứ tự
                let mut pub_idx = 0;
                let mut valid   = true;
                for sig in &sigs {
                    let mut matched = false;
                    while pub_idx < pubkeys.len() {
                        if verify_ecdsa(&pubkeys[pub_idx], signing_data, sig) {
                            matched  = true;
                            pub_idx += 1;
                            break;
                        }
                        pub_idx += 1;
                    }
                    if !matched { valid = false; break; }
                }
                self.stack.push(if valid { vec![1] } else { vec![0] });
            }

            Opcode::OpReturn => { return false; }
        }
        true
    }
}

// ── ECDSA helper ─────────────────────────────────────────────────────────────

fn verify_ecdsa(pubkey_bytes: &[u8], data: &[u8], sig_bytes: &[u8]) -> bool {
    use secp256k1::{Secp256k1, PublicKey, Message, ecdsa::Signature};
    let secp   = Secp256k1::new();
    let pubkey = match PublicKey::from_slice(pubkey_bytes) { Ok(k) => k, Err(_) => return false };
    let hash   = blake3::hash(data);
    let msg    = match Message::from_slice(hash.as_bytes()) { Ok(m) => m, Err(_) => return false };
    let sig    = match Signature::from_compact(sig_bytes)  { Ok(s) => s, Err(_) => return false };
    secp.verify_ecdsa(&msg, &sig, &pubkey).is_ok()
}
