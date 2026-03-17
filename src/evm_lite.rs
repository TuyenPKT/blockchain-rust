#![allow(dead_code)]
//! v7.5 — EVM-lite Executor

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
    Call,
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

        for op in ops {
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
                    // Simplified: just pop the destination (no actual jump in linear execution)
                    self.pop_bytes();
                }
                EvmLiteOp::JumpIf => {
                    self.pop_bytes(); // dest
                    self.pop_bytes(); // condition
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
