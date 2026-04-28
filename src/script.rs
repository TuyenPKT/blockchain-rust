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
    OpDup, OpDrop, OpSwap, OpOver, OpNip, OpSize,
    // Crypto
    OpHash160, OpHash256, OpSha256,
    OpCheckSig, OpCheckMultiSig,
    // Comparison
    OpEqualVerify, OpEqual,
    // Flow / conditionals
    OpVerify, OpReturn,
    OpIf, OpElse, OpEndIf,
    // Timelocks (BIP65 / BIP112)
    OpCheckLockTimeVerify,
    OpCheckSequenceVerify,
    // Numbers — dùng cho multisig M và N
    OpNum(i64),
    // SegWit version push
    Op0,
    Op1,
    OpCheckTemplateVerify,
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

    /// Serialize script thành Bitcoin wire bytes (76 a9 14 <hash20> 88 ac cho P2PKH, v.v.)
    /// Dùng để lưu vào utxodb — phải khớp với prefix query của query_utxos.
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for op in &self.ops {
            match op {
                Opcode::OpDup                  => out.push(0x76),
                Opcode::OpDrop                 => out.push(0x75),
                Opcode::OpSwap                 => out.push(0x7c),
                Opcode::OpOver                 => out.push(0x78),
                Opcode::OpNip                  => out.push(0x77),
                Opcode::OpSize                 => out.push(0x82),
                Opcode::OpHash160              => out.push(0xa9),
                Opcode::OpHash256              => out.push(0xaa),
                Opcode::OpSha256               => out.push(0xa8),
                Opcode::OpCheckSig             => out.push(0xac),
                Opcode::OpCheckMultiSig        => out.push(0xae),
                Opcode::OpEqualVerify          => out.push(0x88),
                Opcode::OpEqual                => out.push(0x87),
                Opcode::OpVerify               => out.push(0x69),
                Opcode::OpReturn               => out.push(0x6a),
                Opcode::OpIf                   => out.push(0x63),
                Opcode::OpElse                 => out.push(0x67),
                Opcode::OpEndIf                => out.push(0x68),
                Opcode::OpCheckLockTimeVerify  => out.push(0xb1),
                Opcode::OpCheckSequenceVerify  => out.push(0xb2),
                Opcode::OpCheckTemplateVerify  => out.push(0xb3),
                Opcode::Op0                    => out.push(0x00),
                Opcode::Op1                    => out.push(0x51),
                Opcode::OpNum(n) => {
                    let n = *n;
                    if n == 0 { out.push(0x00); }
                    else if (1..=16).contains(&n) { out.push(0x50 + n as u8); }
                    else { out.push(0x01); out.push(n as u8); }
                }
                Opcode::OpPushData(bytes) => {
                    let len = bytes.len();
                    if len == 0 { out.push(0x00); }
                    else if len <= 75 { out.push(len as u8); out.extend_from_slice(bytes); }
                    else if len <= 255 { out.push(0x4c); out.push(len as u8); out.extend_from_slice(bytes); }
                    else { out.push(0x4d); out.push((len & 0xff) as u8); out.push((len >> 8) as u8); out.extend_from_slice(bytes); }
                }
            }
        }
        out
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

    /// CLTV-locked P2PKH: <height> OP_CLTV OP_DROP OP_DUP OP_HASH160 <pkh> OP_EQUALVERIFY OP_CHECKSIG
    pub fn cltv_p2pkh(lock_until: u64, pkh: &[u8]) -> Self {
        Script::new(vec![
            Opcode::OpNum(lock_until as i64),
            Opcode::OpCheckLockTimeVerify,
            Opcode::OpDrop,
            Opcode::OpDup,
            Opcode::OpHash160,
            Opcode::OpPushData(pkh.to_vec()),
            Opcode::OpEqualVerify,
            Opcode::OpCheckSig,
        ])
    }

    /// CSV-locked P2PKH: <blocks> OP_CSV OP_DROP OP_DUP OP_HASH160 <pkh> OP_EQUALVERIFY OP_CHECKSIG
    pub fn csv_p2pkh(csv_blocks: u32, pkh: &[u8]) -> Self {
        Script::new(vec![
            Opcode::OpNum(csv_blocks as i64),
            Opcode::OpCheckSequenceVerify,
            Opcode::OpDrop,
            Opcode::OpDup,
            Opcode::OpHash160,
            Opcode::OpPushData(pkh.to_vec()),
            Opcode::OpEqualVerify,
            Opcode::OpCheckSig,
        ])
    }

    /// Standard offered HTLC:
    /// OP_IF
    ///   OP_SHA256 <payment_hash> OP_EQUALVERIFY OP_DUP OP_HASH160 <receiver_pkh> OP_EQUALVERIFY OP_CHECKSIG
    /// OP_ELSE
    ///   <expiry> OP_CLTV OP_DROP OP_DUP OP_HASH160 <sender_pkh> OP_EQUALVERIFY OP_CHECKSIG
    /// OP_ENDIF
    pub fn htlc_offered(payment_hash: &[u8], receiver_pkh: &[u8], sender_pkh: &[u8], expiry: u64) -> Self {
        Script::new(vec![
            Opcode::OpIf,
              Opcode::OpSha256,
              Opcode::OpPushData(payment_hash.to_vec()),
              Opcode::OpEqualVerify,
              Opcode::OpDup,
              Opcode::OpHash160,
              Opcode::OpPushData(receiver_pkh.to_vec()),
              Opcode::OpEqualVerify,
              Opcode::OpCheckSig,
            Opcode::OpElse,
              Opcode::OpNum(expiry as i64),
              Opcode::OpCheckLockTimeVerify,
              Opcode::OpDrop,
              Opcode::OpDup,
              Opcode::OpHash160,
              Opcode::OpPushData(sender_pkh.to_vec()),
              Opcode::OpEqualVerify,
              Opcode::OpCheckSig,
            Opcode::OpEndIf,
        ])
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

// ── Spend context (for CLTV / CSV) ───────────────────────────────────────────

/// Thông tin về spending transaction — cần thiết để validate timelocks.
#[derive(Debug, Clone, Default)]
pub struct SpendContext {
    pub lock_time: u64,  // nLockTime của spending tx (CLTV so sánh)
    pub sequence:  u32,  // nSequence của input (CSV so sánh)
}

// ── ScriptInterpreter ────────────────────────────────────────────────────────

pub struct ScriptInterpreter {
    stack:      Vec<Vec<u8>>,
    cond_stack: Vec<bool>, // true = executing, false = skipping (OP_IF branching)
}

impl ScriptInterpreter {
    pub fn new() -> Self { ScriptInterpreter { stack: vec![], cond_stack: vec![] } }

    fn is_executing(&self) -> bool { self.cond_stack.iter().all(|&c| c) }

    /// Chạy scriptSig + scriptPubKey với SpendContext mặc định (no timelocks).
    pub fn execute(
        &mut self,
        script_sig:    &Script,
        script_pubkey: &Script,
        signing_data:  &[u8],
    ) -> bool {
        self.execute_with_context(script_sig, script_pubkey, signing_data, &SpendContext::default())
    }

    /// Chạy scriptSig + scriptPubKey với SpendContext (CLTV/CSV).
    pub fn execute_with_context(
        &mut self,
        script_sig:    &Script,
        script_pubkey: &Script,
        signing_data:  &[u8],
        ctx:           &SpendContext,
    ) -> bool {
        if !self.run_ops(&script_sig.ops, signing_data, ctx) { return false; }

        let redeem_script_bytes = if script_pubkey.is_p2sh() {
            self.stack.last().cloned()
        } else {
            None
        };

        if !self.run_ops(&script_pubkey.ops, signing_data, ctx) { return false; }

        let top_ok = match self.stack.last() {
            Some(top) => !top.is_empty() && top.iter().any(|&b| b != 0),
            None      => false,
        };
        if !top_ok { return false; }

        if let Some(rs_bytes) = redeem_script_bytes {
            let redeem = match Script::from_bytes(&rs_bytes) {
                Some(s) => s,
                None    => return false,
            };
            // Pop redeemScript bytes khỏi stack (đã được hash-check)
            self.stack.pop();
            if !self.run_ops(&redeem.ops, signing_data, ctx) { return false; }
            return match self.stack.last() {
                Some(top) => !top.is_empty() && top.iter().any(|&b| b != 0),
                None      => false,
            };
        }

        true
    }

    // ── Conditional op execution (handles OP_IF/ELSE/ENDIF) ──────────────────

    fn run_ops(&mut self, ops: &[Opcode], signing_data: &[u8], ctx: &SpendContext) -> bool {
        for op in ops {
            match op {
                Opcode::OpIf => {
                    if self.is_executing() {
                        let cond = match self.stack.pop() {
                            Some(top) => !top.is_empty() && top.iter().any(|&b| b != 0),
                            None      => return false,
                        };
                        self.cond_stack.push(cond);
                    } else {
                        self.cond_stack.push(false);
                    }
                }
                Opcode::OpElse => {
                    match self.cond_stack.last_mut() {
                        Some(c) => *c = !*c,
                        None    => return false,
                    }
                }
                Opcode::OpEndIf => {
                    if self.cond_stack.pop().is_none() { return false; }
                }
                op => {
                    if self.is_executing() && !self.step(op, signing_data, ctx) { return false; }
                }
            }
        }
        self.cond_stack.is_empty()
    }

    fn pop(&mut self) -> Option<Vec<u8>> { self.stack.pop() }

    fn step(&mut self, op: &Opcode, signing_data: &[u8], ctx: &SpendContext) -> bool {
        match op {
            Opcode::OpIf | Opcode::OpElse | Opcode::OpEndIf => unreachable!("handled in run_ops"),

            Opcode::OpPushData(data) => { self.stack.push(data.clone()); }

            Opcode::OpNum(n) => {
                // Encode as little-endian signed integer (Bitcoin script format)
                let v = *n;
                if v == 0 {
                    self.stack.push(vec![]);
                } else if v >= i8::MIN as i64 && v <= i8::MAX as i64 {
                    self.stack.push(vec![v as i8 as u8]);
                } else {
                    self.stack.push(v.to_le_bytes().iter()
                        .rev().skip_while(|&&b| b == 0).collect::<Vec<_>>()
                        .into_iter().rev().copied().collect());
                }
            }

            Opcode::Op0 => { self.stack.push(vec![]); }
            Opcode::Op1 => { self.stack.push(vec![1]); }

            Opcode::OpCheckTemplateVerify => {
                if self.stack.is_empty() { return false; }
                let top = self.stack.last().unwrap();
                if top.len() != 32 { return false; }
                self.stack.push(vec![1]);
            }

            Opcode::OpDup => {
                let top = match self.stack.last() { Some(t) => t.clone(), None => return false };
                self.stack.push(top);
            }

            Opcode::OpDrop => { if self.pop().is_none() { return false; } }

            Opcode::OpNip => {
                let len = self.stack.len();
                if len < 2 { return false; }
                self.stack.remove(len - 2);
            }

            Opcode::OpSize => {
                let len = match self.stack.last() { Some(t) => t.len(), None => return false };
                self.stack.push(vec![len as u8]);
            }

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

            Opcode::OpSha256 => {
                use sha2::{Sha256, Digest};
                let top = match self.pop() { Some(t) => t, None => return false };
                self.stack.push(Sha256::digest(&top).to_vec());
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

            Opcode::OpCheckLockTimeVerify => {
                // Stack top = required locktime; spending tx locktime must be ≥ required
                let required = match self.stack.last() {
                    Some(t) => le_bytes_to_u64(t),
                    None    => return false,
                };
                if ctx.lock_time < required { return false; }
                // CLTV does NOT pop the stack (leaves value for OP_DROP)
            }

            Opcode::OpCheckSequenceVerify => {
                // Stack top = required CSV; input sequence must be ≥ required
                let required = match self.stack.last() {
                    Some(t) => le_bytes_to_u64(t) as u32,
                    None    => return false,
                };
                if ctx.sequence < required { return false; }
            }

            Opcode::OpCheckSig => {
                let pubkey_bytes = match self.pop() { Some(t) => t, None => return false };
                let sig_bytes    = match self.pop() { Some(t) => t, None => return false };
                self.stack.push(
                    if verify_ecdsa(&pubkey_bytes, signing_data, &sig_bytes) { vec![1] } else { vec![0] }
                );
            }

            Opcode::OpCheckMultiSig => {
                let m = match self.pop() {
                    Some(t) => le_bytes_to_u64(&t) as usize,
                    None    => return false,
                };
                let mut pubkeys = vec![];
                for _ in 0..m {
                    match self.pop() { Some(t) => pubkeys.push(t), None => return false }
                }
                let k = match self.pop() {
                    Some(t) => le_bytes_to_u64(&t) as usize,
                    None    => return false,
                };
                let mut sigs = vec![];
                for _ in 0..k {
                    match self.pop() { Some(t) => sigs.push(t), None => return false }
                }
                self.pop(); // OP_0 dummy (Bitcoin off-by-one)

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

// ── Helpers ───────────────────────────────────────────────────────────────────

fn verify_ecdsa(pubkey_bytes: &[u8], data: &[u8], sig_bytes: &[u8]) -> bool {
    use secp256k1::{Secp256k1, PublicKey, Message, ecdsa::Signature};
    let secp   = Secp256k1::new();
    let pubkey = match PublicKey::from_slice(pubkey_bytes) { Ok(k) => k, Err(_) => return false };
    let hash   = blake3::hash(data);
    let msg    = match Message::from_slice(hash.as_bytes()) { Ok(m) => m, Err(_) => return false };
    let sig    = match Signature::from_compact(sig_bytes)  { Ok(s) => s, Err(_) => return false };
    secp.verify_ecdsa(&msg, &sig, &pubkey).is_ok()
}

/// Decode little-endian unsigned integer bytes → u64 (for timelock values).
fn le_bytes_to_u64(b: &[u8]) -> u64 {
    if b.is_empty() { return 0; }
    let mut val: u64 = 0;
    for (i, &byte) in b.iter().enumerate().take(8) {
        val |= (byte as u64) << (8 * i);
    }
    val
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── OP_IF / OP_ELSE / OP_ENDIF ───────────────────────────────────────────

    #[test]
    fn test_opif_true_branch() {
        // Script: OP_IF OP_1 OP_ELSE OP_0 OP_ENDIF
        // scriptSig pushes 1 → true branch → stack = [1]
        let pubkey = Script::new(vec![
            Opcode::OpIf,
              Opcode::Op1,
            Opcode::OpElse,
              Opcode::Op0,
            Opcode::OpEndIf,
        ]);
        let sig_script = Script::new(vec![Opcode::Op1]);
        let mut vm = ScriptInterpreter::new();
        assert!(vm.execute(&sig_script, &pubkey, &[]));
    }

    #[test]
    fn test_opif_false_branch() {
        // scriptSig pushes 0 → false branch → stack = [0] → script fails
        let pubkey = Script::new(vec![
            Opcode::OpIf,
              Opcode::Op1,
            Opcode::OpElse,
              Opcode::Op0,
            Opcode::OpEndIf,
        ]);
        let sig_script = Script::new(vec![Opcode::Op0]);
        let mut vm = ScriptInterpreter::new();
        assert!(!vm.execute(&sig_script, &pubkey, &[]));
    }

    #[test]
    fn test_opif_nested() {
        // OP_IF OP_IF OP_1 OP_ENDIF OP_ENDIF: outer=true, inner=true → 1
        let script = Script::new(vec![
            Opcode::OpIf,
              Opcode::OpIf,
                Opcode::Op1,
              Opcode::OpEndIf,
            Opcode::OpEndIf,
        ]);
        let sig = Script::new(vec![Opcode::Op1, Opcode::Op1]);
        let mut vm = ScriptInterpreter::new();
        assert!(vm.execute(&sig, &script, &[]));
    }

    // ── CLTV ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_cltv_passes_when_locktime_sufficient() {
        // Script: <100> OP_CLTV OP_DROP OP_1
        let script = Script::new(vec![
            Opcode::OpNum(100),
            Opcode::OpCheckLockTimeVerify,
            Opcode::OpDrop,
            Opcode::Op1,
        ]);
        let ctx = SpendContext { lock_time: 200, sequence: 0xFFFF_FFFF };
        let mut vm = ScriptInterpreter::new();
        assert!(vm.execute_with_context(&Script::empty(), &script, &[], &ctx));
    }

    #[test]
    fn test_cltv_fails_when_locktime_insufficient() {
        let script = Script::new(vec![
            Opcode::OpNum(100),
            Opcode::OpCheckLockTimeVerify,
            Opcode::OpDrop,
            Opcode::Op1,
        ]);
        let ctx = SpendContext { lock_time: 50, sequence: 0 };
        let mut vm = ScriptInterpreter::new();
        assert!(!vm.execute_with_context(&Script::empty(), &script, &[], &ctx));
    }

    #[test]
    fn test_cltv_exact_boundary() {
        let script = Script::new(vec![
            Opcode::OpNum(500),
            Opcode::OpCheckLockTimeVerify,
            Opcode::OpDrop,
            Opcode::Op1,
        ]);
        let ctx = SpendContext { lock_time: 500, sequence: 0 };
        let mut vm = ScriptInterpreter::new();
        assert!(vm.execute_with_context(&Script::empty(), &script, &[], &ctx));
    }

    // ── CSV ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_csv_passes_when_sequence_sufficient() {
        let script = Script::new(vec![
            Opcode::OpNum(144),
            Opcode::OpCheckSequenceVerify,
            Opcode::OpDrop,
            Opcode::Op1,
        ]);
        let ctx = SpendContext { lock_time: 0, sequence: 144 };
        let mut vm = ScriptInterpreter::new();
        assert!(vm.execute_with_context(&Script::empty(), &script, &[], &ctx));
    }

    #[test]
    fn test_csv_fails_when_sequence_insufficient() {
        let script = Script::new(vec![
            Opcode::OpNum(144),
            Opcode::OpCheckSequenceVerify,
            Opcode::OpDrop,
            Opcode::Op1,
        ]);
        let ctx = SpendContext { lock_time: 0, sequence: 100 };
        let mut vm = ScriptInterpreter::new();
        assert!(!vm.execute_with_context(&Script::empty(), &script, &[], &ctx));
    }

    // ── HTLC ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_htlc_hash_path_correct_preimage() {
        use sha2::{Sha256, Digest};
        let preimage = b"secret_payment_preimage_32bytes!";
        let payment_hash = Sha256::digest(preimage).to_vec();
        let receiver_pkh = vec![0x11u8; 20];
        let sender_pkh   = vec![0x22u8; 20];
        let _htlc = Script::htlc_offered(&payment_hash, &receiver_pkh, &sender_pkh, 1000);

        // Hash path scriptSig: <1> <preimage> <receiver_sig> <receiver_pk>
        // Simplified: push 1 (choose if branch), preimage, dummy sig/pk
        // For test: skip sig verify by using empty sig+pk (verify_ecdsa returns false)
        // Instead test the OP_SHA256 hash check works
        let preimage_script = Script::new(vec![
            Opcode::OpPushData(preimage.to_vec()),
            Opcode::OpSha256,
        ]);
        let expected_hash_script = Script::new(vec![
            Opcode::OpPushData(payment_hash.clone()),
            Opcode::OpEqual,
        ]);
        let mut vm = ScriptInterpreter::new();
        // run preimage → sha256 → stack has hash
        assert!(vm.run_ops(&preimage_script.ops, &[], &SpendContext::default()));
        // compare with expected
        assert!(vm.run_ops(&expected_hash_script.ops, &[], &SpendContext::default()));
        let top = vm.stack.last().unwrap();
        assert_eq!(top, &vec![1u8]);
    }

    #[test]
    fn test_htlc_wrong_preimage_fails() {
        use sha2::{Sha256, Digest};
        let real_preimage = b"correct_preimage_exactly_32bytes";
        let payment_hash  = Sha256::digest(real_preimage).to_vec();
        let wrong_preimage = b"wrong_preimage_XXXXXXXXXXXXXXXX_";

        let check = Script::new(vec![
            Opcode::OpPushData(wrong_preimage.to_vec()),
            Opcode::OpSha256,
            Opcode::OpPushData(payment_hash),
            Opcode::OpEqualVerify,
            Opcode::Op1,
        ]);
        let mut vm = ScriptInterpreter::new();
        assert!(!vm.execute_with_context(&Script::empty(), &check, &[], &SpendContext::default()));
    }

    #[test]
    fn test_htlc_timeout_path_via_cltv() {
        let script = Script::new(vec![
            Opcode::OpIf,
              Opcode::Op0, // hash path fails (push 0 into if = false, skipped)
            Opcode::OpElse,
              Opcode::OpNum(1000),
              Opcode::OpCheckLockTimeVerify,
              Opcode::OpDrop,
              Opcode::Op1, // timeout path succeeds
            Opcode::OpEndIf,
        ]);
        // Push 0 → false branch (timeout)
        let sig = Script::new(vec![Opcode::Op0]);
        let ctx = SpendContext { lock_time: 1000, sequence: 0 };
        let mut vm = ScriptInterpreter::new();
        assert!(vm.execute_with_context(&sig, &script, &[], &ctx));
    }

    #[test]
    fn test_htlc_timeout_fails_before_expiry() {
        let script = Script::new(vec![
            Opcode::OpIf,
              Opcode::Op0,
            Opcode::OpElse,
              Opcode::OpNum(1000),
              Opcode::OpCheckLockTimeVerify,
              Opcode::OpDrop,
              Opcode::Op1,
            Opcode::OpEndIf,
        ]);
        let sig = Script::new(vec![Opcode::Op0]);
        let ctx = SpendContext { lock_time: 999, sequence: 0 };
        let mut vm = ScriptInterpreter::new();
        assert!(!vm.execute_with_context(&sig, &script, &[], &ctx));
    }

    // ── OpSha256 ─────────────────────────────────────────────────────────────

    #[test]
    fn test_opsha256_deterministic() {
        use sha2::{Sha256, Digest};
        let data = b"test data";
        let expected = Sha256::digest(data).to_vec();
        let script = Script::new(vec![
            Opcode::OpPushData(data.to_vec()),
            Opcode::OpSha256,
        ]);
        let mut vm = ScriptInterpreter::new();
        assert!(vm.run_ops(&script.ops, &[], &SpendContext::default()));
        assert_eq!(vm.stack.last().unwrap(), &expected);
    }

    // ── OpNip / OpSize ────────────────────────────────────────────────────────

    #[test]
    fn test_opnip_removes_second() {
        let script = Script::new(vec![
            Opcode::OpPushData(vec![1]),
            Opcode::OpPushData(vec![2]),
            Opcode::OpNip,
        ]);
        let mut vm = ScriptInterpreter::new();
        assert!(vm.run_ops(&script.ops, &[], &SpendContext::default()));
        assert_eq!(vm.stack, vec![vec![2u8]]);
    }

    #[test]
    fn test_opsize_pushes_length() {
        let script = Script::new(vec![
            Opcode::OpPushData(vec![0u8; 5]),
            Opcode::OpSize,
        ]);
        let mut vm = ScriptInterpreter::new();
        assert!(vm.run_ops(&script.ops, &[], &SpendContext::default()));
        assert_eq!(vm.stack.last().unwrap(), &vec![5u8]);
    }

    // ── script builders ───────────────────────────────────────────────────────

    #[test]
    fn test_cltv_builder_structure() {
        let s = Script::cltv_p2pkh(700_000, &[0xABu8; 20]);
        assert_eq!(s.ops[1], Opcode::OpCheckLockTimeVerify);
        assert_eq!(s.ops[2], Opcode::OpDrop);
    }

    #[test]
    fn test_csv_builder_structure() {
        let s = Script::csv_p2pkh(144, &[0xCDu8; 20]);
        assert_eq!(s.ops[1], Opcode::OpCheckSequenceVerify);
        assert_eq!(s.ops[2], Opcode::OpDrop);
    }

    #[test]
    fn test_htlc_builder_structure() {
        use sha2::{Sha256, Digest};
        let hash = Sha256::digest(b"preimage").to_vec();
        let s = Script::htlc_offered(&hash, &[0x11u8; 20], &[0x22u8; 20], 1000);
        assert_eq!(s.ops[0], Opcode::OpIf);
        assert_eq!(s.ops[1], Opcode::OpSha256);
        // Find OP_ELSE
        assert!(s.ops.contains(&Opcode::OpElse));
        assert!(s.ops.contains(&Opcode::OpCheckLockTimeVerify));
        assert_eq!(*s.ops.last().unwrap(), Opcode::OpEndIf);
    }

    // ── le_bytes_to_u64 ───────────────────────────────────────────────────────

    #[test]
    fn test_le_bytes_to_u64_empty() {
        assert_eq!(le_bytes_to_u64(&[]), 0);
    }

    #[test]
    fn test_le_bytes_to_u64_single_byte() {
        assert_eq!(le_bytes_to_u64(&[0x64]), 100);
    }
}
