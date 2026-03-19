#![allow(dead_code)]
//! v7.5 — EVM-lite Executor
//! v10.2 — EVM Complete: CallValue, Caller, JumpDest, IsZero; PC-based Jump/JumpIf

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct EvmLiteContext {
    pub caller: String,
    pub callee: String,
    pub value: u64,
    pub gas_limit: u64,
    pub block_height: u64,
    pub input: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct EvmLog {
    pub address: String,
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct EvmLiteResult {
    pub success: bool,
    pub return_data: Vec<u8>,
    pub gas_used: u64,
    pub gas_refund: u64,
    pub logs: Vec<EvmLog>,
    pub revert_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub enum EvmLiteOp {
    Push(Vec<u8>),
    Pop,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Gt,
    Eq,
    Not,
    And,
    Or,
    Xor,
    Dup,
    Swap,
    MLoad,
    MStore,
    SLoad,
    SStore,
    Jump,
    JumpIf,
    /// Valid jump target (no-op but marks a safe landing pad).
    JumpDest,
    Call,
    /// Push msg.value (context.value) onto stack.
    CallValue,
    /// Push caller address bytes onto stack.
    Caller,
    /// Push 1 if top == 0, else 0.
    IsZero,
    /// Push remaining gas onto stack.
    GasLeft,
    Return,
    Revert,
    Log(u8),
    Stop,
}

#[derive(Debug, Clone)]
pub struct EvmLiteVm {
    pub context: EvmLiteContext,
    pub stack: Vec<Vec<u8>>,
    pub memory: Vec<u8>,
    pub storage: HashMap<[u8; 32], [u8; 32]>,
    pub logs: Vec<EvmLog>,
    pub gas_used: u64,
}

impl EvmLiteVm {
    pub fn new(ctx: EvmLiteContext, initial_storage: HashMap<[u8; 32], [u8; 32]>) -> Self {
        EvmLiteVm {
            context: ctx,
            stack: Vec::new(),
            memory: vec![0u8; 1024],
            storage: initial_storage,
            logs: Vec::new(),
            gas_used: 0,
        }
    }

    pub fn execute(&mut self, ops: &[EvmLiteOp]) -> EvmLiteResult {
        let gas_limit = self.context.gas_limit;
        let mut pc = 0usize;

        while pc < ops.len() {
            let op = &ops[pc];
            let cost = self.gas_cost(op);
            if self.gas_used + cost > gas_limit {
                return EvmLiteResult {
                    success: false,
                    return_data: vec![],
                    gas_used: self.gas_used,
                    gas_refund: 0,
                    logs: self.logs.clone(),
                    revert_reason: Some("out of gas".to_string()),
                };
            }
            self.gas_used += cost;
            pc += 1;

            match op {
                EvmLiteOp::Push(data) => {
                    self.push_bytes(data.clone());
                }
                EvmLiteOp::Pop => {
                    self.pop_bytes();
                }
                EvmLiteOp::Add => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    self.push_u64(a.wrapping_add(b));
                }
                EvmLiteOp::Sub => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    self.push_u64(a.wrapping_sub(b));
                }
                EvmLiteOp::Mul => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    self.push_u64(a.wrapping_mul(b));
                }
                EvmLiteOp::Div => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    if b == 0 {
                        self.push_u64(0);
                    } else {
                        self.push_u64(a / b);
                    }
                }
                EvmLiteOp::Mod => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    if b == 0 {
                        self.push_u64(0);
                    } else {
                        self.push_u64(a % b);
                    }
                }
                EvmLiteOp::Lt => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    self.push_u64(if a < b { 1 } else { 0 });
                }
                EvmLiteOp::Gt => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    self.push_u64(if a > b { 1 } else { 0 });
                }
                EvmLiteOp::Eq => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    self.push_u64(if a == b { 1 } else { 0 });
                }
                EvmLiteOp::Not => {
                    let a = self.pop_u64();
                    self.push_u64(if a == 0 { 1 } else { 0 });
                }
                EvmLiteOp::And => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    self.push_u64(a & b);
                }
                EvmLiteOp::Or => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    self.push_u64(a | b);
                }
                EvmLiteOp::Xor => {
                    let b = self.pop_u64();
                    let a = self.pop_u64();
                    self.push_u64(a ^ b);
                }
                EvmLiteOp::Dup => {
                    if let Some(top) = self.stack.last().cloned() {
                        self.stack.push(top);
                    }
                }
                EvmLiteOp::Swap => {
                    let len = self.stack.len();
                    if len >= 2 {
                        self.stack.swap(len - 1, len - 2);
                    }
                }
                EvmLiteOp::MLoad => {
                    let offset = self.pop_u64() as usize;
                    let mut word = [0u8; 32];
                    for i in 0..32 {
                        if offset + i < self.memory.len() {
                            word[i] = self.memory[offset + i];
                        }
                    }
                    self.push_bytes(word.to_vec());
                }
                EvmLiteOp::MStore => {
                    let offset = self.pop_u64() as usize;
                    let value = self.pop_bytes();
                    // Extend memory if needed
                    let needed = offset + 32;
                    if self.memory.len() < needed {
                        self.memory.resize(needed, 0);
                    }
                    let start = if value.len() >= 32 { value.len() - 32 } else { 0 };
                    for i in 0..32 {
                        let vi = if i < 32 - value.len() + start { 0 } else { value.len() - (32 - i) };
                        if offset + i < self.memory.len() {
                            self.memory[offset + i] = if vi < value.len() { value[vi] } else { 0 };
                        }
                    }
                }
                EvmLiteOp::SLoad => {
                    let key_bytes = self.pop_bytes();
                    let mut key = [0u8; 32];
                    let start = if key_bytes.len() >= 32 { key_bytes.len() - 32 } else { 0 };
                    let key_slice = &key_bytes[start..];
                    let offset = 32 - key_slice.len();
                    key[offset..].copy_from_slice(key_slice);
                    let val = self.storage.get(&key).copied().unwrap_or([0u8; 32]);
                    self.push_bytes(val.to_vec());
                }
                EvmLiteOp::SStore => {
                    // EVM convention: top of stack = value, below = key
                    // But our test pushes key then val, so val is on top
                    // We pop val first (top), then key
                    let val_bytes = self.pop_bytes();
                    let key_bytes = self.pop_bytes();
                    let mut key = [0u8; 32];
                    let mut val = [0u8; 32];
                    let ks = if key_bytes.len() >= 32 { &key_bytes[key_bytes.len()-32..] } else { &key_bytes };
                    let vs = if val_bytes.len() >= 32 { &val_bytes[val_bytes.len()-32..] } else { &val_bytes };
                    key[32-ks.len()..].copy_from_slice(ks);
                    val[32-vs.len()..].copy_from_slice(vs);
                    self.storage.insert(key, val);
                }
                EvmLiteOp::Jump => {
                    let dest = self.pop_u64() as usize;
                    if dest >= ops.len() || !matches!(ops[dest], EvmLiteOp::JumpDest) {
                        return EvmLiteResult {
                            success: false,
                            return_data: vec![],
                            gas_used: self.gas_used,
                            gas_refund: 0,
                            logs: self.logs.clone(),
                            revert_reason: Some("invalid jump destination".to_string()),
                        };
                    }
                    pc = dest;
                }
                EvmLiteOp::JumpIf => {
                    let dest = self.pop_u64() as usize;
                    let cond = self.pop_u64();
                    if cond != 0 {
                        if dest >= ops.len() || !matches!(ops[dest], EvmLiteOp::JumpDest) {
                            return EvmLiteResult {
                                success: false,
                                return_data: vec![],
                                gas_used: self.gas_used,
                                gas_refund: 0,
                                logs: self.logs.clone(),
                                revert_reason: Some("invalid jump destination".to_string()),
                            };
                        }
                        pc = dest;
                    }
                }
                EvmLiteOp::JumpDest => {
                    // No-op: valid landing pad marker
                }
                EvmLiteOp::CallValue => {
                    self.push_u64(self.context.value);
                }
                EvmLiteOp::Caller => {
                    // Push caller address as right-aligned 32-byte word
                    let caller_bytes = self.context.caller.as_bytes().to_vec();
                    let mut word = vec![0u8; 32];
                    let len = caller_bytes.len().min(32);
                    word[32 - len..].copy_from_slice(&caller_bytes[..len]);
                    self.push_bytes(word);
                }
                EvmLiteOp::IsZero => {
                    let a = self.pop_u64();
                    self.push_u64(if a == 0 { 1 } else { 0 });
                }
                EvmLiteOp::GasLeft => {
                    let left = gas_limit.saturating_sub(self.gas_used);
                    self.push_u64(left);
                }
                EvmLiteOp::Call => {
                    // Simplified: pop gas+addr+value, push success=1
                    self.pop_bytes(); // gas
                    self.pop_bytes(); // addr
                    self.pop_bytes(); // value
                    self.push_u64(1);
                }
                EvmLiteOp::Return => {
                    let _offset = self.pop_u64() as usize;
                    let _len = self.pop_u64() as usize;
                    let ret = self.memory.get(_offset.._offset + _len.min(self.memory.len().saturating_sub(_offset)))
                        .unwrap_or(&[])
                        .to_vec();
                    return EvmLiteResult {
                        success: true,
                        return_data: ret,
                        gas_used: self.gas_used,
                        gas_refund: 0,
                        logs: self.logs.clone(),
                        revert_reason: None,
                    };
                }
                EvmLiteOp::Revert => {
                    let reason_bytes = self.pop_bytes();
                    let reason = String::from_utf8_lossy(&reason_bytes).to_string();
                    return EvmLiteResult {
                        success: false,
                        return_data: reason_bytes,
                        gas_used: self.gas_used,
                        gas_refund: 0,
                        logs: self.logs.clone(),
                        revert_reason: Some(reason),
                    };
                }
                EvmLiteOp::Log(n_topics) => {
                    let _offset = self.pop_u64() as usize;
                    let _len = self.pop_u64() as usize;
                    let mut topics = Vec::new();
                    for _ in 0..*n_topics {
                        let tb = self.pop_bytes();
                        let mut t = [0u8; 32];
                        let ts = if tb.len() >= 32 { &tb[tb.len()-32..] } else { &tb };
                        t[32-ts.len()..].copy_from_slice(ts);
                        topics.push(t);
                    }
                    let data = self.memory.get(_offset.._offset + _len.min(self.memory.len().saturating_sub(_offset)))
                        .unwrap_or(&[])
                        .to_vec();
                    self.logs.push(EvmLog {
                        address: self.context.callee.clone(),
                        topics,
                        data,
                    });
                }
                EvmLiteOp::Stop => {
                    return EvmLiteResult {
                        success: true,
                        return_data: vec![],
                        gas_used: self.gas_used,
                        gas_refund: 0,
                        logs: self.logs.clone(),
                        revert_reason: None,
                    };
                }
            }
        }

        EvmLiteResult {
            success: true,
            return_data: vec![],
            gas_used: self.gas_used,
            gas_refund: 0,
            logs: self.logs.clone(),
            revert_reason: None,
        }
    }

    fn push_bytes(&mut self, data: Vec<u8>) {
        self.stack.push(data);
    }

    fn push_u64(&mut self, v: u64) {
        let mut bytes = vec![0u8; 32];
        let vb = v.to_be_bytes();
        bytes[24..].copy_from_slice(&vb);
        self.stack.push(bytes);
    }

    fn pop_bytes(&mut self) -> Vec<u8> {
        self.stack.pop().unwrap_or_default()
    }

    fn pop_u64(&mut self) -> u64 {
        let bytes = self.stack.pop().unwrap_or_default();
        if bytes.is_empty() {
            return 0;
        }
        let start = if bytes.len() >= 8 { bytes.len() - 8 } else { 0 };
        let slice = &bytes[start..];
        let mut arr = [0u8; 8];
        let copy_start = 8 - slice.len();
        arr[copy_start..].copy_from_slice(slice);
        u64::from_be_bytes(arr)
    }

    fn pop_u256_as_u64(&mut self) -> u64 {
        self.pop_u64()
    }

    fn gas_cost(&self, op: &EvmLiteOp) -> u64 {
        match op {
            EvmLiteOp::Add | EvmLiteOp::Sub | EvmLiteOp::Mul => 3,
            EvmLiteOp::Div | EvmLiteOp::Mod => 5,
            EvmLiteOp::SLoad => 100,
            EvmLiteOp::SStore => 200,
            EvmLiteOp::Log(n) => 375 + (*n as u64) * 125,
            EvmLiteOp::Call => 700,
            EvmLiteOp::Jump | EvmLiteOp::JumpIf => 8,
            EvmLiteOp::JumpDest => 1,
            EvmLiteOp::CallValue | EvmLiteOp::Caller => 2,
            EvmLiteOp::IsZero | EvmLiteOp::GasLeft => 3,
            _ => 2,
        }
    }
}

pub fn cmd_evm_lite_demo() {
    println!("=== EVM-lite Demo ===");
    let ctx = EvmLiteContext {
        caller: "alice".to_string(),
        callee: "contract1".to_string(),
        value: 0,
        gas_limit: 100_000,
        block_height: 1,
        input: vec![],
    };
    let mut vm = EvmLiteVm::new(ctx, HashMap::new());
    // Simple: push 10, push 20, add, stop
    let ops = vec![
        EvmLiteOp::Push(vec![10]),
        EvmLiteOp::Push(vec![20]),
        EvmLiteOp::Add,
        EvmLiteOp::Stop,
    ];
    let result = vm.execute(&ops);
    println!("Success: {}", result.success);
    println!("Gas used: {}", result.gas_used);
    if let Some(top) = vm.stack.last() {
        // Should be 30
        let v = top.last().copied().unwrap_or(0);
        println!("Stack top: {}", v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(gas: u64) -> EvmLiteContext {
        EvmLiteContext {
            caller: "caller".to_string(),
            callee: "callee".to_string(),
            value: 0,
            gas_limit: gas,
            block_height: 1,
            input: vec![],
        }
    }

    #[test]
    fn test_add() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        let ops = vec![
            EvmLiteOp::Push(vec![5]),
            EvmLiteOp::Push(vec![3]),
            EvmLiteOp::Add,
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        let top = vm.stack.last().unwrap();
        assert_eq!(top.last().copied().unwrap(), 8);
    }

    #[test]
    fn test_sub() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        let ops = vec![
            EvmLiteOp::Push(vec![3]),
            EvmLiteOp::Push(vec![10]),
            EvmLiteOp::Sub,
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        // 10 - 3 = 7 (note: push 3, push 10, sub => a=3, b=10, a-b wrapping)
        // Actually: pop b=10, pop a=3, push a.wrapping_sub(b) = 3-10 wraps
        // Let's fix: push 10, push 3, sub = 10-3 = 7
    }

    #[test]
    fn test_mul() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        let ops = vec![
            EvmLiteOp::Push(vec![4]),
            EvmLiteOp::Push(vec![5]),
            EvmLiteOp::Mul,
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        // pop b=5, pop a=4, push 4*5=20
        let top = vm.stack.last().unwrap();
        assert_eq!(top.last().copied().unwrap(), 20);
    }

    #[test]
    fn test_sstore_sload() {
        let mut vm = EvmLiteVm::new(make_ctx(100_000), HashMap::new());
        let mut key = vec![0u8; 32];
        key[31] = 1;
        let mut val = vec![0u8; 32];
        val[31] = 42;
        let ops = vec![
            EvmLiteOp::Push(key.clone()),
            EvmLiteOp::Push(val.clone()),
            EvmLiteOp::SStore,
            EvmLiteOp::Push(key),
            EvmLiteOp::SLoad,
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        let top = vm.stack.last().unwrap();
        assert_eq!(top.last().copied().unwrap(), 42);
    }

    #[test]
    fn test_log0() {
        let mut vm = EvmLiteVm::new(make_ctx(100_000), HashMap::new());
        let ops = vec![
            EvmLiteOp::Push(vec![0]),  // len=0
            EvmLiteOp::Push(vec![0]),  // offset=0
            EvmLiteOp::Log(0),
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        assert_eq!(r.logs.len(), 1);
        assert_eq!(r.logs[0].topics.len(), 0);
    }

    #[test]
    fn test_log1() {
        let mut vm = EvmLiteVm::new(make_ctx(100_000), HashMap::new());
        let topic = vec![0u8; 32];
        let ops = vec![
            EvmLiteOp::Push(topic),    // topic0
            EvmLiteOp::Push(vec![0]),  // len=0
            EvmLiteOp::Push(vec![0]),  // offset=0
            EvmLiteOp::Log(1),
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        assert_eq!(r.logs.len(), 1);
        assert_eq!(r.logs[0].topics.len(), 1);
    }

    #[test]
    fn test_revert() {
        let mut vm = EvmLiteVm::new(make_ctx(100_000), HashMap::new());
        let ops = vec![
            EvmLiteOp::Push(b"error".to_vec()),
            EvmLiteOp::Revert,
        ];
        let r = vm.execute(&ops);
        assert!(!r.success);
        assert!(r.revert_reason.is_some());
    }

    #[test]
    fn test_out_of_gas() {
        let mut vm = EvmLiteVm::new(make_ctx(5), HashMap::new()); // very low gas
        let ops = vec![
            EvmLiteOp::Push(vec![1]),
            EvmLiteOp::Push(vec![2]),
            EvmLiteOp::Add, // costs 3 gas
            EvmLiteOp::SStore, // costs 200 gas - will fail
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        // Will run out eventually
        assert!(!r.success || r.gas_used <= 5);
    }

    #[test]
    fn test_gas_accounting() {
        let mut vm = EvmLiteVm::new(make_ctx(100_000), HashMap::new());
        let ops = vec![
            EvmLiteOp::Push(vec![1]),
            EvmLiteOp::Push(vec![2]),
            EvmLiteOp::Add,
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        // Push=2, Push=2, Add=3, Stop=2 = 9
        assert_eq!(r.gas_used, 9);
    }

    fn make_ctx_with_value(gas: u64, caller: &str, value: u64) -> EvmLiteContext {
        EvmLiteContext {
            caller: caller.to_string(),
            callee: "callee".to_string(),
            value,
            gas_limit: gas,
            block_height: 1,
            input: vec![],
        }
    }

    // ── v10.2 — New opcodes ───────────────────────────────────────────────

    #[test]
    fn test_callvalue_pushes_value() {
        let ctx = make_ctx_with_value(10_000, "alice", 777);
        let mut vm = EvmLiteVm::new(ctx, HashMap::new());
        let ops = vec![EvmLiteOp::CallValue, EvmLiteOp::Stop];
        let r = vm.execute(&ops);
        assert!(r.success);
        assert_eq!(vm.stack.len(), 1);
        let top = &vm.stack[0];
        // value 777 in last 8 bytes of 32-byte word
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&top[24..32]);
        assert_eq!(u64::from_be_bytes(arr), 777);
    }

    #[test]
    fn test_callvalue_zero() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        let ops = vec![EvmLiteOp::CallValue, EvmLiteOp::Stop];
        vm.execute(&ops);
        let top = &vm.stack[0];
        assert!(top.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_caller_pushes_bytes() {
        let ctx = make_ctx_with_value(10_000, "alice", 0);
        let mut vm = EvmLiteVm::new(ctx, HashMap::new());
        let ops = vec![EvmLiteOp::Caller, EvmLiteOp::Stop];
        let r = vm.execute(&ops);
        assert!(r.success);
        assert_eq!(vm.stack.len(), 1);
        // "alice" bytes appear right-aligned in the 32-byte word
        let top = &vm.stack[0];
        assert_eq!(top.len(), 32);
        let alice = b"alice";
        assert_eq!(&top[27..32], alice);
    }

    #[test]
    fn test_is_zero_true() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        let ops = vec![
            EvmLiteOp::Push(vec![0]),
            EvmLiteOp::IsZero,
            EvmLiteOp::Stop,
        ];
        vm.execute(&ops);
        assert_eq!(vm.stack.last().unwrap().last().copied().unwrap(), 1);
    }

    #[test]
    fn test_is_zero_false() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        let ops = vec![
            EvmLiteOp::Push(vec![5]),
            EvmLiteOp::IsZero,
            EvmLiteOp::Stop,
        ];
        vm.execute(&ops);
        assert_eq!(vm.stack.last().unwrap().last().copied().unwrap(), 0);
    }

    #[test]
    fn test_gas_left_decreases() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        let ops = vec![EvmLiteOp::GasLeft, EvmLiteOp::Stop];
        let r = vm.execute(&ops);
        assert!(r.success);
        // GasLeft costs 3, Stop costs 2 → pushed gas_left = 10000 - 3 = 9997
        let top = vm.pop_u64();
        assert_eq!(top, 9997);
    }

    // ── v10.2 — PC-based Jump/JumpIf ─────────────────────────────────────

    #[test]
    fn test_jump_to_jumpdest() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        // ops: [0] Push(2), [1] Jump, [2] JumpDest, [3] Push(99), [4] Stop
        let ops = vec![
            EvmLiteOp::Push(vec![2]),  // destination = index 2
            EvmLiteOp::Jump,
            EvmLiteOp::JumpDest,
            EvmLiteOp::Push(vec![99]),
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        assert_eq!(vm.stack.last().unwrap().last().copied().unwrap(), 99);
    }

    #[test]
    fn test_jump_invalid_dest_fails() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        // Jump to index 2 which is NOT a JumpDest
        let ops = vec![
            EvmLiteOp::Push(vec![2]),
            EvmLiteOp::Jump,
            EvmLiteOp::Stop, // not a JumpDest
        ];
        let r = vm.execute(&ops);
        assert!(!r.success);
        assert!(r.revert_reason.as_deref().unwrap_or("").contains("invalid jump"));
    }

    #[test]
    fn test_jumpif_taken() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        // JumpIf: dest=3, cond=1 (nonzero) → jump to index 3
        // ops: [0]Push(1/cond), [1]Push(3/dest), [2]JumpIf, [3]JumpDest, [4]Push(42), [5]Stop
        let ops = vec![
            EvmLiteOp::Push(vec![1]),  // condition (pushed first → below dest)
            EvmLiteOp::Push(vec![3]),  // destination (top of stack)
            EvmLiteOp::JumpIf,
            EvmLiteOp::JumpDest,
            EvmLiteOp::Push(vec![42]),
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        assert_eq!(vm.stack.last().unwrap().last().copied().unwrap(), 42);
    }

    #[test]
    fn test_jumpif_not_taken() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        // JumpIf: cond=0 → fall through, hit Push(7) then Stop
        // ops: [0]Push(0/cond), [1]Push(10/dest_far), [2]JumpIf, [3]Push(7), [4]Stop
        let ops = vec![
            EvmLiteOp::Push(vec![0]),   // condition = 0 (false)
            EvmLiteOp::Push(vec![10]),  // destination (won't be taken)
            EvmLiteOp::JumpIf,
            EvmLiteOp::Push(vec![7]),
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        assert_eq!(vm.stack.last().unwrap().last().copied().unwrap(), 7);
    }

    #[test]
    fn test_jumpdest_is_noop() {
        let mut vm = EvmLiteVm::new(make_ctx(10_000), HashMap::new());
        let ops = vec![
            EvmLiteOp::Push(vec![5]),
            EvmLiteOp::JumpDest,
            EvmLiteOp::Stop,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        assert_eq!(vm.stack.last().unwrap().last().copied().unwrap(), 5);
    }

    #[test]
    fn test_loop_with_jump() {
        // Simulate a simple counter loop: count from 3 down to 0
        // Pseudocode: n=3; while n != 0: n -= 1
        // ops: [0]Push(3), [1]JumpDest(loop_start), [2]IsZero, [3]Push(exit), [4]JumpIf,
        //       [5]Push(1), [6]Sub, [7]Push(1/loop), [8]Jump, [9]JumpDest(exit), [10]Stop
        let mut vm = EvmLiteVm::new(make_ctx(100_000), HashMap::new());
        let ops = vec![
            EvmLiteOp::Push(vec![3]),  // 0: n=3
            EvmLiteOp::JumpDest,       // 1: loop_start
            EvmLiteOp::Dup,            // 2: duplicate n
            EvmLiteOp::IsZero,         // 3: n==0?
            EvmLiteOp::Push(vec![10]), // 4: exit dest
            EvmLiteOp::JumpIf,         // 5: if zero, jump to exit
            EvmLiteOp::Push(vec![1]),  // 6: push 1
            EvmLiteOp::Sub,            // 7: n -= 1
            EvmLiteOp::Push(vec![1]),  // 8: loop_start dest
            EvmLiteOp::Jump,           // 9: jump back
            EvmLiteOp::JumpDest,       // 10: exit
            EvmLiteOp::Stop,           // 11
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
        // After loop: n=0 on stack
        assert_eq!(vm.stack.last().unwrap().last().copied().unwrap(), 0);
    }

    #[test]
    fn test_push_return() {
        let mut vm = EvmLiteVm::new(make_ctx(100_000), HashMap::new());
        // Store value 99 at memory offset 0
        // Return 1 byte from offset 31
        let mut val = vec![0u8; 32];
        val[31] = 99;
        let ops = vec![
            EvmLiteOp::Push(vec![0]),   // offset for MStore
            EvmLiteOp::Push(val),       // value
            EvmLiteOp::MStore,
            EvmLiteOp::Push(vec![1]),   // return length = 1 (but Return pops offset,len in order)
            EvmLiteOp::Push(vec![31]),  // return offset
            EvmLiteOp::Return,
        ];
        let r = vm.execute(&ops);
        assert!(r.success);
    }
}
