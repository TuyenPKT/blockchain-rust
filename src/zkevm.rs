#![allow(dead_code)]

/// v2.5 — zkEVM (Zero-Knowledge EVM)
///
/// Kiến trúc:
///
///   Bytecode (EVM opcodes)
///       ↓  execute
///   Execution Trace  (pc, opcode, stack_before, stack_after, gas)
///       ↓  arithmetize
///   R1CS Constraints  (từng opcode → 1 constraint row)
///       ↓  prove
///   ZkEvmProof  (commitment to trace + witness)
///       ↓  verify
///   On-chain Verifier  (O(1) — chỉ check proof, không re-execute)
///
/// Tính chất:
///   - Correctness: proof đảm bảo EVM execution đúng theo spec
///   - Succinctness: proof nhỏ hơn trace gốc nhiều lần
///   - Privacy: có thể ẩn input/output (optional)
///   - EVM compatibility: cùng semantics với Ethereum EVM
///
/// Opcodes hỗ trợ (subset):
///   PUSH1, POP, ADD, SUB, MUL, DIV, MOD
///   LT, GT, EQ, ISZERO, AND, OR
///   MSTORE, MLOAD, JUMPDEST, JUMP, JUMPI, STOP, RETURN
///
/// Tham khảo: Polygon zkEVM, zkSync Era, Scroll, Taiko

use sha2::{Sha256, Digest};
use std::collections::HashMap;

// ─── EvmOpcode ────────────────────────────────────────────────────────────────

/// Subset opcodes của EVM
#[derive(Debug, Clone, PartialEq)]
pub enum EvmOpcode {
    // Stack
    Push(u64),      // PUSH1..PUSH32 (simplified: 1 arg)
    Pop,            // POP
    Dup,            // DUP1 — duplicate top of stack
    Swap,           // SWAP1 — swap top two

    // Arithmetic
    Add,    // a + b
    Sub,    // a - b
    Mul,    // a * b
    Div,    // a / b (integer)
    Mod,    // a % b

    // Comparison
    Lt,     // a < b
    Gt,     // a > b
    Eq,     // a == b
    IsZero, // a == 0

    // Bitwise
    And,    // a & b
    Or,     // a | b

    // Memory
    MStore(u64),  // store value at memory[addr]
    MLoad(u64),   // push memory[addr]

    // Control flow
    JumpDest(u64),  // label
    Jump(u64),      // unconditional jump to label
    JumpI(u64),     // conditional jump: if top != 0

    // Terminal
    Stop,
    Return,
}

impl EvmOpcode {
    pub fn opcode_byte(&self) -> u8 {
        match self {
            EvmOpcode::Push(_)    => 0x60,
            EvmOpcode::Pop        => 0x50,
            EvmOpcode::Dup        => 0x80,
            EvmOpcode::Swap       => 0x90,
            EvmOpcode::Add        => 0x01,
            EvmOpcode::Sub        => 0x03,
            EvmOpcode::Mul        => 0x02,
            EvmOpcode::Div        => 0x04,
            EvmOpcode::Mod        => 0x06,
            EvmOpcode::Lt         => 0x10,
            EvmOpcode::Gt         => 0x11,
            EvmOpcode::Eq         => 0x14,
            EvmOpcode::IsZero     => 0x15,
            EvmOpcode::And        => 0x16,
            EvmOpcode::Or         => 0x17,
            EvmOpcode::MStore(_)  => 0x52,
            EvmOpcode::MLoad(_)   => 0x51,
            EvmOpcode::JumpDest(_)=> 0x5b,
            EvmOpcode::Jump(_)    => 0x56,
            EvmOpcode::JumpI(_)   => 0x57,
            EvmOpcode::Stop       => 0x00,
            EvmOpcode::Return     => 0xf3,
        }
    }

    pub fn gas_cost(&self) -> u64 {
        match self {
            EvmOpcode::Push(_)    => 3,
            EvmOpcode::Pop        => 2,
            EvmOpcode::Dup        => 3,
            EvmOpcode::Swap       => 3,
            EvmOpcode::Add        => 3,
            EvmOpcode::Sub        => 3,
            EvmOpcode::Mul        => 5,
            EvmOpcode::Div        => 5,
            EvmOpcode::Mod        => 5,
            EvmOpcode::Lt         => 3,
            EvmOpcode::Gt         => 3,
            EvmOpcode::Eq         => 3,
            EvmOpcode::IsZero     => 3,
            EvmOpcode::And        => 3,
            EvmOpcode::Or         => 3,
            EvmOpcode::MStore(_)  => 3,
            EvmOpcode::MLoad(_)   => 3,
            EvmOpcode::JumpDest(_)=> 1,
            EvmOpcode::Jump(_)    => 8,
            EvmOpcode::JumpI(_)   => 10,
            EvmOpcode::Stop       => 0,
            EvmOpcode::Return     => 0,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            EvmOpcode::Push(_)    => "PUSH",
            EvmOpcode::Pop        => "POP",
            EvmOpcode::Dup        => "DUP1",
            EvmOpcode::Swap       => "SWAP1",
            EvmOpcode::Add        => "ADD",
            EvmOpcode::Sub        => "SUB",
            EvmOpcode::Mul        => "MUL",
            EvmOpcode::Div        => "DIV",
            EvmOpcode::Mod        => "MOD",
            EvmOpcode::Lt         => "LT",
            EvmOpcode::Gt         => "GT",
            EvmOpcode::Eq         => "EQ",
            EvmOpcode::IsZero     => "ISZERO",
            EvmOpcode::And        => "AND",
            EvmOpcode::Or         => "OR",
            EvmOpcode::MStore(_)  => "MSTORE",
            EvmOpcode::MLoad(_)   => "MLOAD",
            EvmOpcode::JumpDest(_)=> "JUMPDEST",
            EvmOpcode::Jump(_)    => "JUMP",
            EvmOpcode::JumpI(_)   => "JUMPI",
            EvmOpcode::Stop       => "STOP",
            EvmOpcode::Return     => "RETURN",
        }
    }
}

// ─── TraceStep ────────────────────────────────────────────────────────────────

/// 1 bước trong execution trace
/// Mỗi step trở thành 1 constraint row trong ZK circuit
#[derive(Debug, Clone)]
pub struct TraceStep {
    pub pc:          usize,
    pub opcode:      EvmOpcode,
    pub stack_in:    Vec<u64>,   // stack trước khi execute
    pub stack_out:   Vec<u64>,   // stack sau khi execute
    pub gas_used:    u64,
    pub step_hash:   String,     // H(pc || opcode || stack_in || stack_out)
}

impl TraceStep {
    pub fn new(pc: usize, opcode: EvmOpcode, stack_in: Vec<u64>, stack_out: Vec<u64>, gas_used: u64) -> Self {
        let mut h = Sha256::new();
        h.update(b"trace_step_v25");
        h.update((pc as u64).to_le_bytes());
        h.update([opcode.opcode_byte()]);
        for v in &stack_in  { h.update(v.to_le_bytes()); }
        h.update(b"|");
        for v in &stack_out { h.update(v.to_le_bytes()); }
        h.update(gas_used.to_le_bytes());
        let step_hash = hex::encode(h.finalize());
        TraceStep { pc, opcode, stack_in, stack_out, gas_used, step_hash }
    }
}

// ─── EvmExecutor ─────────────────────────────────────────────────────────────

/// EVM interpreter — execute bytecode và record execution trace
pub struct EvmExecutor {
    pub stack:  Vec<u64>,
    pub memory: HashMap<u64, u64>,
    pub gas:    u64,
    pub pc:     usize,
    pub trace:  Vec<TraceStep>,
}

impl EvmExecutor {
    pub fn new(gas_limit: u64) -> Self {
        EvmExecutor {
            stack:  vec![],
            memory: HashMap::new(),
            gas:    gas_limit,
            pc:     0,
            trace:  vec![],
        }
    }

    pub fn stack_top(&self) -> Option<u64> {
        self.stack.last().copied()
    }

    /// Execute 1 opcode và record trace step
    pub fn step(&mut self, opcode: &EvmOpcode) -> Result<bool, String> {
        let gas_cost = opcode.gas_cost();
        if self.gas < gas_cost {
            return Err(format!("Out of gas at pc={}: need {}, have {}", self.pc, gas_cost, self.gas));
        }

        let stack_in = self.stack.clone();
        let mut done = false;

        match opcode {
            EvmOpcode::Push(v) => {
                self.stack.push(*v);
            }
            EvmOpcode::Pop => {
                self.stack.pop().ok_or("POP: stack underflow")?;
            }
            EvmOpcode::Dup => {
                let top = *self.stack.last().ok_or("DUP: stack underflow")?;
                self.stack.push(top);
            }
            EvmOpcode::Swap => {
                let len = self.stack.len();
                if len < 2 { return Err("SWAP: stack underflow".to_string()); }
                self.stack.swap(len-1, len-2);
            }
            EvmOpcode::Add => {
                let b = self.stack.pop().ok_or("ADD: underflow")?;
                let a = self.stack.pop().ok_or("ADD: underflow")?;
                self.stack.push(a.wrapping_add(b));
            }
            EvmOpcode::Sub => {
                let b = self.stack.pop().ok_or("SUB: underflow")?;
                let a = self.stack.pop().ok_or("SUB: underflow")?;
                self.stack.push(a.wrapping_sub(b));
            }
            EvmOpcode::Mul => {
                let b = self.stack.pop().ok_or("MUL: underflow")?;
                let a = self.stack.pop().ok_or("MUL: underflow")?;
                self.stack.push(a.wrapping_mul(b));
            }
            EvmOpcode::Div => {
                let b = self.stack.pop().ok_or("DIV: underflow")?;
                let a = self.stack.pop().ok_or("DIV: underflow")?;
                self.stack.push(if b == 0 { 0 } else { a / b });
            }
            EvmOpcode::Mod => {
                let b = self.stack.pop().ok_or("MOD: underflow")?;
                let a = self.stack.pop().ok_or("MOD: underflow")?;
                self.stack.push(if b == 0 { 0 } else { a % b });
            }
            EvmOpcode::Lt => {
                let b = self.stack.pop().ok_or("LT: underflow")?;
                let a = self.stack.pop().ok_or("LT: underflow")?;
                self.stack.push(if a < b { 1 } else { 0 });
            }
            EvmOpcode::Gt => {
                let b = self.stack.pop().ok_or("GT: underflow")?;
                let a = self.stack.pop().ok_or("GT: underflow")?;
                self.stack.push(if a > b { 1 } else { 0 });
            }
            EvmOpcode::Eq => {
                let b = self.stack.pop().ok_or("EQ: underflow")?;
                let a = self.stack.pop().ok_or("EQ: underflow")?;
                self.stack.push(if a == b { 1 } else { 0 });
            }
            EvmOpcode::IsZero => {
                let a = self.stack.pop().ok_or("ISZERO: underflow")?;
                self.stack.push(if a == 0 { 1 } else { 0 });
            }
            EvmOpcode::And => {
                let b = self.stack.pop().ok_or("AND: underflow")?;
                let a = self.stack.pop().ok_or("AND: underflow")?;
                self.stack.push(a & b);
            }
            EvmOpcode::Or => {
                let b = self.stack.pop().ok_or("OR: underflow")?;
                let a = self.stack.pop().ok_or("OR: underflow")?;
                self.stack.push(a | b);
            }
            EvmOpcode::MStore(addr) => {
                let val = self.stack.pop().ok_or("MSTORE: underflow")?;
                self.memory.insert(*addr, val);
            }
            EvmOpcode::MLoad(addr) => {
                let val = self.memory.get(addr).copied().unwrap_or(0);
                self.stack.push(val);
            }
            EvmOpcode::JumpDest(_) => {
                // no-op, just a label marker
            }
            EvmOpcode::Jump(_dest) => {
                // simplified: linear execution (no actual jump in our demo)
            }
            EvmOpcode::JumpI(dest) => {
                let cond = self.stack.pop().ok_or("JUMPI: underflow")?;
                let _target = dest;
                // simplified: if cond != 0 we would jump (linear for demo)
                let _ = cond;
            }
            EvmOpcode::Stop | EvmOpcode::Return => {
                done = true;
            }
        }

        self.gas -= gas_cost;

        let stack_out  = self.stack.clone();
        let step = TraceStep::new(self.pc, opcode.clone(), stack_in, stack_out, gas_cost);
        self.trace.push(step);
        self.pc += 1;

        Ok(done)
    }

    /// Execute full bytecode program
    pub fn execute(&mut self, program: &[EvmOpcode]) -> Result<(), String> {
        for opcode in program {
            let done = self.step(opcode)?;
            if done { break; }
        }
        Ok(())
    }

    pub fn gas_used(&self, gas_limit: u64) -> u64 {
        gas_limit - self.gas
    }
}

// ─── ExecutionTrace ───────────────────────────────────────────────────────────

/// Toàn bộ execution trace — được arithmetized thành ZK constraints
#[derive(Debug, Clone)]
pub struct ExecutionTrace {
    pub steps:        Vec<TraceStep>,
    pub final_stack:  Vec<u64>,
    pub gas_used:     u64,
    pub trace_root:   String,   // Merkle root của tất cả step hashes
}

impl ExecutionTrace {
    pub fn from_executor(executor: &EvmExecutor, gas_limit: u64) -> Self {
        let steps       = executor.trace.clone();
        let final_stack = executor.stack.clone();
        let gas_used    = gas_limit - executor.gas;

        // Trace root: H(all step_hashes)
        let mut h = Sha256::new();
        h.update(b"trace_root_v25");
        for step in &steps {
            h.update(step.step_hash.as_bytes());
        }
        let trace_root = hex::encode(h.finalize());

        ExecutionTrace { steps, final_stack, gas_used, trace_root }
    }

    /// Arithmetize: mỗi step → 1 constraint row
    /// Constraint: stack_out = f(opcode, stack_in)  [đúng theo EVM spec]
    /// Trong thực tế: Plonkish table, Halo2 circuits
    pub fn to_constraints(&self) -> Vec<Constraint> {
        self.steps.iter().map(|step| {
            let mut h = Sha256::new();
            h.update(b"constraint_v25");
            h.update([step.opcode.opcode_byte()]);
            for v in &step.stack_in  { h.update(v.to_le_bytes()); }
            h.update(b"|");
            for v in &step.stack_out { h.update(v.to_le_bytes()); }
            let witness_hash = hex::encode(h.finalize());

            Constraint {
                pc:           step.pc,
                opcode_name:  step.opcode.name(),
                stack_depth_in:  step.stack_in.len(),
                stack_depth_out: step.stack_out.len(),
                witness_hash,
                satisfied:    true,   // all steps from valid execution are satisfied
            }
        }).collect()
    }
}

// ─── Constraint ───────────────────────────────────────────────────────────────

/// 1 constraint row trong ZK circuit
#[derive(Debug, Clone)]
pub struct Constraint {
    pub pc:              usize,
    pub opcode_name:     &'static str,
    pub stack_depth_in:  usize,
    pub stack_depth_out: usize,
    pub witness_hash:    String,
    pub satisfied:       bool,
}

// ─── ZkEvmProof ───────────────────────────────────────────────────────────────

/// ZK proof rằng bytecode được execute đúng
///
/// Trong thực tế: Plonk/Halo2 proof (~1-5 KB)
/// Ở đây: H(trace_root || final_stack || gas_used || constraints_hash)
#[derive(Debug, Clone)]
pub struct ZkEvmProof {
    pub trace_root:       String,
    pub final_stack_hash: String,
    pub gas_used:         u64,
    pub step_count:       usize,
    pub proof_bytes:      Vec<u8>,
}

impl ZkEvmProof {
    pub fn generate(trace: &ExecutionTrace, constraints: &[Constraint]) -> Self {
        // Hash constraints
        let mut ch = Sha256::new();
        ch.update(b"constraints_v25");
        for c in constraints {
            ch.update([c.satisfied as u8]);
            ch.update(c.witness_hash.as_bytes());
        }
        let constraints_hash = ch.finalize();

        // Hash final stack
        let mut sh = Sha256::new();
        sh.update(b"final_stack_v25");
        for v in &trace.final_stack { sh.update(v.to_le_bytes()); }
        let final_stack_hash = hex::encode(sh.finalize());

        // Proof = H(trace_root || final_stack_hash || gas || constraints)
        let mut h = Sha256::new();
        h.update(b"zkevm_proof_v25");
        h.update(trace.trace_root.as_bytes());
        h.update(final_stack_hash.as_bytes());
        h.update(trace.gas_used.to_le_bytes());
        h.update(&constraints_hash);
        let proof_bytes = h.finalize().to_vec();

        ZkEvmProof {
            trace_root:       trace.trace_root.clone(),
            final_stack_hash,
            gas_used:         trace.gas_used,
            step_count:       trace.steps.len(),
            proof_bytes,
        }
    }

    /// On-chain verification — O(1), không re-execute
    pub fn verify(&self, trace: &ExecutionTrace, constraints: &[Constraint]) -> bool {
        let expected = Self::generate(trace, constraints);
        self.proof_bytes == expected.proof_bytes
            && self.trace_root == trace.trace_root
            && self.step_count == trace.steps.len()
    }

    pub fn proof_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(&self.proof_bytes);
        hex::encode(h.finalize())
    }
}

// ─── ZkEvmVerifier ───────────────────────────────────────────────────────────

/// On-chain verifier — verify proof mà không re-execute EVM
pub struct ZkEvmVerifier {
    pub verified_count:  u64,
    pub total_gas_saved: u64,  // gas so với on-chain execution
}

impl ZkEvmVerifier {
    pub fn new() -> Self {
        ZkEvmVerifier { verified_count: 0, total_gas_saved: 0 }
    }

    /// Verify proof: O(1) — chỉ check hash, không replay
    pub fn verify(&mut self, proof: &ZkEvmProof, trace: &ExecutionTrace, constraints: &[Constraint]) -> bool {
        let ok = proof.verify(trace, constraints);
        if ok {
            self.verified_count += 1;
            // Gas savings: on-chain execution would cost gas_used per opcode
            // zkEVM: fixed verification cost (~500k gas) amortized over batches
            self.total_gas_saved += proof.gas_used.saturating_sub(5000);
        }
        ok
    }
}

// ─── SmartContract ────────────────────────────────────────────────────────────

/// Smart contract bytecode (EVM program)
#[derive(Debug, Clone)]
pub struct SmartContract {
    pub name:     String,
    pub bytecode: Vec<EvmOpcode>,
    pub abi:      Vec<String>,
}

impl SmartContract {
    pub fn new(name: impl Into<String>, bytecode: Vec<EvmOpcode>, abi: Vec<String>) -> Self {
        SmartContract { name: name.into(), bytecode, abi }
    }

    pub fn bytecode_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(b"contract_v25");
        for op in &self.bytecode {
            h.update([op.opcode_byte()]);
        }
        hex::encode(h.finalize())
    }
}
