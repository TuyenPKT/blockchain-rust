#![allow(dead_code)]

/// v2.6 — Smart Contract Engine (WASM-based)
///
/// Kiến trúc:
///
///   Developer viết contract (Rust/Solidity)
///       ↓ compile
///   WasmModule  (bytecode + exports + memory layout)
///       ↓ deploy
///   ContractRegistry  (address → ContractInstance)
///       ↓ call
///   WasmRuntime  (interpreter + gas meter + storage)
///       ↓ result
///   Return value + updated storage + gas receipt
///
/// WASM subset hỗ trợ:
///   i32.const, i32.add/sub/mul/div/mod, i32.lt/gt/eq
///   local.get/set, global.get/set
///   memory.load/store (key-value storage)
///   if/else, block, return, call (inter-function)
///   drop (discard top of stack)
///
/// Gas model:
///   Mỗi instruction có chi phí cố định
///   Storage read/write đắt hơn computation
///   Out-of-gas → revert toàn bộ call
///
/// Tham khảo: Ethereum EVM, CosmWasm, Near WASM, Polkadot ink!

use std::collections::HashMap;

// ─── WasmValue ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum WasmValue {
    I32(i32),
    I64(i64),
    Bool(bool),
}

impl WasmValue {
    pub fn as_i32(&self) -> i32 {
        match self {
            WasmValue::I32(v) => *v,
            WasmValue::I64(v) => *v as i32,
            WasmValue::Bool(b) => if *b { 1 } else { 0 },
        }
    }
    pub fn as_i64(&self) -> i64 {
        match self {
            WasmValue::I32(v) => *v as i64,
            WasmValue::I64(v) => *v,
            WasmValue::Bool(b) => if *b { 1 } else { 0 },
        }
    }
    pub fn is_truthy(&self) -> bool {
        self.as_i32() != 0
    }
}

// ─── WasmInstr ────────────────────────────────────────────────────────────────

/// WASM instruction set (simplified subset)
#[derive(Debug, Clone)]
pub enum WasmInstr {
    // Constants
    I32Const(i32),
    I64Const(i64),

    // Arithmetic (i32)
    I32Add,
    I32Sub,
    I32Mul,
    I32Div,
    I32Mod,

    // Comparison (i32)
    I32Lt,
    I32Gt,
    I32Eq,
    I32Ne,
    I32Le,
    I32Ge,

    // Stack
    Drop,
    Select,          // cond ? a : b

    // Locals (function-scoped variables)
    LocalGet(usize),
    LocalSet(usize),
    LocalTee(usize), // set + keep on stack

    // Globals (contract-scoped persistent variables)
    GlobalGet(String),
    GlobalSet(String),

    // Storage (key-value, most expensive)
    StorageLoad(String),
    StorageStore(String),

    // Control flow
    If(Vec<WasmInstr>, Vec<WasmInstr>),   // condition, then, else
    Block(Vec<WasmInstr>),
    Return,
    ReturnValue,

    // Function call
    Call(String),   // call named function
}

impl WasmInstr {
    pub fn gas_cost(&self) -> u64 {
        match self {
            WasmInstr::I32Const(_)    => 1,
            WasmInstr::I64Const(_)    => 1,
            WasmInstr::I32Add         => 1,
            WasmInstr::I32Sub         => 1,
            WasmInstr::I32Mul         => 2,
            WasmInstr::I32Div         => 3,
            WasmInstr::I32Mod         => 3,
            WasmInstr::I32Lt          => 1,
            WasmInstr::I32Gt          => 1,
            WasmInstr::I32Eq          => 1,
            WasmInstr::I32Ne          => 1,
            WasmInstr::I32Le          => 1,
            WasmInstr::I32Ge          => 1,
            WasmInstr::Drop           => 1,
            WasmInstr::Select         => 2,
            WasmInstr::LocalGet(_)    => 1,
            WasmInstr::LocalSet(_)    => 1,
            WasmInstr::LocalTee(_)    => 1,
            WasmInstr::GlobalGet(_)   => 5,
            WasmInstr::GlobalSet(_)   => 5,
            WasmInstr::StorageLoad(_) => 200,   // expensive: disk read
            WasmInstr::StorageStore(_)=> 5000,  // very expensive: disk write
            WasmInstr::If(_, _)       => 3,
            WasmInstr::Block(_)       => 1,
            WasmInstr::Return         => 1,
            WasmInstr::ReturnValue    => 1,
            WasmInstr::Call(_)        => 40,
        }
    }
}

// ─── WasmFunction ─────────────────────────────────────────────────────────────

/// 1 function trong WASM module
#[derive(Debug, Clone)]
pub struct WasmFunction {
    pub name:        String,
    pub params:      Vec<String>,    // param names (all i32 for simplicity)
    pub local_count: usize,
    pub body:        Vec<WasmInstr>,
    pub exported:    bool,
}

impl WasmFunction {
    pub fn new(name: impl Into<String>, params: Vec<String>, local_count: usize, body: Vec<WasmInstr>) -> Self {
        WasmFunction {
            name: name.into(),
            params,
            local_count,
            body,
            exported: true,
        }
    }
}

// ─── WasmModule ───────────────────────────────────────────────────────────────

/// Compiled WASM module = 1 smart contract
#[derive(Debug, Clone)]
pub struct WasmModule {
    pub name:      String,
    pub functions: HashMap<String, WasmFunction>,
    pub exports:   Vec<String>,  // exported function names
}

impl WasmModule {
    pub fn new(name: impl Into<String>) -> Self {
        WasmModule {
            name: name.into(),
            functions: HashMap::new(),
            exports:   vec![],
        }
    }

    pub fn add_function(&mut self, func: WasmFunction) {
        if func.exported {
            self.exports.push(func.name.clone());
        }
        self.functions.insert(func.name.clone(), func);
    }

    pub fn bytecode_hash(&self) -> String {
        let mut h = blake3::Hasher::new();
        h.update(b"wasm_module_v26");
        h.update(self.name.as_bytes());
        let mut names: Vec<_> = self.functions.keys().collect();
        names.sort();
        for name in names {
            h.update(name.as_bytes());
        }
        hex::encode(h.finalize().as_bytes())
    }
}

// ─── ContractStorage ──────────────────────────────────────────────────────────

/// Persistent key-value storage per contract
#[derive(Debug, Clone)]
pub struct ContractStorage {
    pub data: HashMap<String, i64>,
}

impl ContractStorage {
    pub fn new() -> Self {
        ContractStorage { data: HashMap::new() }
    }

    pub fn get(&self, key: &str) -> i64 {
        self.data.get(key).copied().unwrap_or(0)
    }

    pub fn set(&mut self, key: &str, value: i64) {
        self.data.insert(key.to_string(), value);
    }

    pub fn storage_root(&self) -> String {
        let mut keys: Vec<_> = self.data.keys().collect();
        keys.sort();
        let mut h = blake3::Hasher::new();
        h.update(b"storage_v26");
        for k in keys {
            h.update(k.as_bytes());
            h.update(&self.data[k].to_le_bytes());
        }
        hex::encode(h.finalize().as_bytes())
    }
}

// ─── GasMeter ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GasMeter {
    pub limit:    u64,
    pub used:     u64,
    pub log:      Vec<(String, u64)>,  // (instruction, gas)
}

impl GasMeter {
    pub fn new(limit: u64) -> Self {
        GasMeter { limit, used: 0, log: vec![] }
    }

    pub fn charge(&mut self, label: impl Into<String>, cost: u64) -> Result<(), String> {
        if self.used + cost > self.limit {
            return Err(format!("Out of gas: used={}, cost={}, limit={}", self.used, cost, self.limit));
        }
        self.used += cost;
        self.log.push((label.into(), cost));
        Ok(())
    }

    pub fn remaining(&self) -> u64 {
        self.limit - self.used
    }
}

// ─── CallFrame ────────────────────────────────────────────────────────────────

/// Stack frame cho 1 function call
struct CallFrame {
    locals:  Vec<i64>,
    stack:   Vec<i64>,
}

impl CallFrame {
    fn new(local_count: usize, params: Vec<i64>) -> Self {
        let mut locals = params;
        locals.resize(local_count.max(locals.len()), 0);
        CallFrame { locals, stack: vec![] }
    }

    fn push(&mut self, v: i64) { self.stack.push(v); }

    fn pop(&mut self) -> Result<i64, String> {
        self.stack.pop().ok_or_else(|| "Stack underflow".to_string())
    }

    fn peek(&self) -> Option<i64> { self.stack.last().copied() }
}

// ─── WasmRuntime ─────────────────────────────────────────────────────────────

/// Interpreter — execute function with gas metering
pub struct WasmRuntime<'a> {
    pub module:   &'a WasmModule,
    pub storage:  &'a mut ContractStorage,
    pub globals:  HashMap<String, i64>,
    pub gas:      GasMeter,
}

impl<'a> WasmRuntime<'a> {
    pub fn new(module: &'a WasmModule, storage: &'a mut ContractStorage, gas_limit: u64) -> Self {
        WasmRuntime {
            module,
            storage,
            globals: HashMap::new(),
            gas: GasMeter::new(gas_limit),
        }
    }

    /// Call an exported function
    pub fn call(&mut self, fn_name: &str, args: Vec<i64>) -> Result<Option<i64>, String> {
        let func = self.module.functions.get(fn_name)
            .ok_or_else(|| format!("Function '{}' not found", fn_name))?
            .clone();

        let mut frame = CallFrame::new(func.local_count, args);
        self.exec_block(&func.body, &mut frame)
    }

    fn exec_block(&mut self, instrs: &[WasmInstr], frame: &mut CallFrame) -> Result<Option<i64>, String> {
        for instr in instrs {
            let cost = instr.gas_cost();
            self.gas.charge(format!("{:?}", instr).chars().take(20).collect::<String>(), cost)?;

            match instr {
                WasmInstr::I32Const(v) => frame.push(*v as i64),
                WasmInstr::I64Const(v) => frame.push(*v),

                WasmInstr::I32Add => { let b = frame.pop()?; let a = frame.pop()?; frame.push(a.wrapping_add(b)); }
                WasmInstr::I32Sub => { let b = frame.pop()?; let a = frame.pop()?; frame.push(a.wrapping_sub(b)); }
                WasmInstr::I32Mul => { let b = frame.pop()?; let a = frame.pop()?; frame.push(a.wrapping_mul(b)); }
                WasmInstr::I32Div => {
                    let b = frame.pop()?; let a = frame.pop()?;
                    if b == 0 { return Err("Division by zero".to_string()); }
                    frame.push(a / b);
                }
                WasmInstr::I32Mod => {
                    let b = frame.pop()?; let a = frame.pop()?;
                    if b == 0 { return Err("Modulo by zero".to_string()); }
                    frame.push(a % b);
                }

                WasmInstr::I32Lt => { let b = frame.pop()?; let a = frame.pop()?; frame.push(if a < b { 1 } else { 0 }); }
                WasmInstr::I32Gt => { let b = frame.pop()?; let a = frame.pop()?; frame.push(if a > b { 1 } else { 0 }); }
                WasmInstr::I32Eq => { let b = frame.pop()?; let a = frame.pop()?; frame.push(if a == b { 1 } else { 0 }); }
                WasmInstr::I32Ne => { let b = frame.pop()?; let a = frame.pop()?; frame.push(if a != b { 1 } else { 0 }); }
                WasmInstr::I32Le => { let b = frame.pop()?; let a = frame.pop()?; frame.push(if a <= b { 1 } else { 0 }); }
                WasmInstr::I32Ge => { let b = frame.pop()?; let a = frame.pop()?; frame.push(if a >= b { 1 } else { 0 }); }

                WasmInstr::Drop => { frame.pop()?; }
                WasmInstr::Select => {
                    let cond = frame.pop()?;
                    let b    = frame.pop()?;
                    let a    = frame.pop()?;
                    frame.push(if cond != 0 { a } else { b });
                }

                WasmInstr::LocalGet(idx) => {
                    let v = frame.locals.get(*idx).copied().unwrap_or(0);
                    frame.push(v);
                }
                WasmInstr::LocalSet(idx) => {
                    let v = frame.pop()?;
                    if *idx < frame.locals.len() { frame.locals[*idx] = v; }
                }
                WasmInstr::LocalTee(idx) => {
                    let v = frame.peek().ok_or("LocalTee: stack empty")?;
                    if *idx < frame.locals.len() { frame.locals[*idx] = v; }
                }

                WasmInstr::GlobalGet(key) => {
                    let v = self.globals.get(key).copied().unwrap_or(0);
                    frame.push(v);
                }
                WasmInstr::GlobalSet(key) => {
                    let v = frame.pop()?;
                    self.globals.insert(key.clone(), v);
                }

                WasmInstr::StorageLoad(key) => {
                    let v = self.storage.get(key);
                    frame.push(v);
                }
                WasmInstr::StorageStore(key) => {
                    let v = frame.pop()?;
                    self.storage.set(key, v);
                }

                WasmInstr::If(then_body, else_body) => {
                    let cond = frame.pop()?;
                    let body = if cond != 0 { then_body } else { else_body };
                    if let Some(ret) = self.exec_block(body, frame)? {
                        return Ok(Some(ret));
                    }
                }

                WasmInstr::Block(body) => {
                    if let Some(ret) = self.exec_block(body, frame)? {
                        return Ok(Some(ret));
                    }
                }

                WasmInstr::Return => return Ok(None),
                WasmInstr::ReturnValue => {
                    let v = frame.pop()?;
                    return Ok(Some(v));
                }

                WasmInstr::Call(fn_name) => {
                    // Simplified: no args passed (use globals/storage for communication)
                    let _ = self.call(fn_name, vec![])?;
                }
            }
        }
        Ok(frame.peek())
    }
}

// ─── ContractInstance ─────────────────────────────────────────────────────────

/// Deployed contract instance on-chain
#[derive(Debug, Clone)]
pub struct ContractInstance {
    pub address:      String,
    pub module:       WasmModule,
    pub storage:      ContractStorage,
    pub creator:      String,
    pub deploy_block: u64,
    pub call_count:   u64,
    pub total_gas:    u64,
}

impl ContractInstance {
    pub fn new(module: WasmModule, creator: impl Into<String>, block: u64) -> Self {
        let creator = creator.into();
        let mut h = blake3::Hasher::new();
        h.update(b"contract_addr_v26");
        h.update(module.bytecode_hash().as_bytes());
        h.update(creator.as_bytes());
        h.update(&block.to_le_bytes());
        let address = format!("0x{}", &hex::encode(h.finalize().as_bytes())[..40]);

        ContractInstance {
            address,
            module,
            storage:      ContractStorage::new(),
            creator,
            deploy_block: block,
            call_count:   0,
            total_gas:    0,
        }
    }

    pub fn call(&mut self, fn_name: &str, args: Vec<i64>, gas_limit: u64) -> CallResult {
        let mut runtime = WasmRuntime::new(&self.module, &mut self.storage, gas_limit);
        match runtime.call(fn_name, args.clone()) {
            Ok(ret) => {
                let gas_used = runtime.gas.used;
                self.call_count += 1;
                self.total_gas  += gas_used;
                CallResult {
                    success:      true,
                    return_value: ret,
                    gas_used,
                    gas_log:      runtime.gas.log,
                    error:        None,
                    storage_root: self.storage.storage_root(),
                }
            }
            Err(e) => {
                let gas_used = runtime.gas.used;
                // Revert storage (storage was mutated by runtime, need to undo)
                // Simplified: we accept the mutation; in prod, snapshot & revert
                CallResult {
                    success:      false,
                    return_value: None,
                    gas_used,
                    gas_log:      runtime.gas.log,
                    error:        Some(e),
                    storage_root: self.storage.storage_root(),
                }
            }
        }
    }
}

// ─── CallResult ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CallResult {
    pub success:      bool,
    pub return_value: Option<i64>,
    pub gas_used:     u64,
    pub gas_log:      Vec<(String, u64)>,
    pub error:        Option<String>,
    pub storage_root: String,
}

// ─── ContractRegistry ─────────────────────────────────────────────────────────

/// On-chain contract registry — deploy + lookup
pub struct ContractRegistry {
    pub contracts:    HashMap<String, ContractInstance>,
    pub block_height: u64,
}

impl ContractRegistry {
    pub fn new() -> Self {
        ContractRegistry { contracts: HashMap::new(), block_height: 0 }
    }

    pub fn deploy(&mut self, module: WasmModule, creator: &str) -> String {
        let instance = ContractInstance::new(module, creator, self.block_height);
        let address  = instance.address.clone();
        self.contracts.insert(address.clone(), instance);
        address
    }

    pub fn call(&mut self, address: &str, fn_name: &str, args: Vec<i64>, gas_limit: u64) -> Result<CallResult, String> {
        let instance = self.contracts.get_mut(address)
            .ok_or_else(|| format!("Contract not found: {}", address))?;
        Ok(instance.call(fn_name, args, gas_limit))
    }

    pub fn storage_of(&self, address: &str, key: &str) -> i64 {
        self.contracts.get(address)
            .map(|c| c.storage.get(key))
            .unwrap_or(0)
    }
}

// ─── Contract builders ───────────────────────────────────────────────────────
// Helpers to build common contract patterns

/// Counter contract:  increment(), decrement(), get_count()
pub fn counter_contract() -> WasmModule {
    let mut module = WasmModule::new("Counter");

    module.add_function(WasmFunction::new(
        "increment",
        vec![],
        0,
        vec![
            WasmInstr::StorageLoad("count".to_string()),
            WasmInstr::I32Const(1),
            WasmInstr::I32Add,
            WasmInstr::StorageStore("count".to_string()),
        ],
    ));

    module.add_function(WasmFunction::new(
        "decrement",
        vec![],
        0,
        vec![
            WasmInstr::StorageLoad("count".to_string()),
            WasmInstr::I32Const(1),
            WasmInstr::I32Sub,
            WasmInstr::StorageStore("count".to_string()),
        ],
    ));

    module.add_function(WasmFunction::new(
        "get_count",
        vec![],
        0,
        vec![
            WasmInstr::StorageLoad("count".to_string()),
            WasmInstr::ReturnValue,
        ],
    ));

    module
}

/// Token contract: transfer(amount), balance_of_alice(), balance_of_bob()
/// Simplified: two hardcoded accounts alice/bob
pub fn token_contract(alice_balance: i64, bob_balance: i64) -> WasmModule {
    let mut module = WasmModule::new("Token");

    // init: set initial balances
    module.add_function(WasmFunction::new(
        "init",
        vec![],
        0,
        vec![
            WasmInstr::I64Const(alice_balance),
            WasmInstr::StorageStore("balance_alice".to_string()),
            WasmInstr::I64Const(bob_balance),
            WasmInstr::StorageStore("balance_bob".to_string()),
        ],
    ));

    // transfer(amount): alice → bob
    // Checks: alice_balance >= amount, then alice -= amount, bob += amount
    module.add_function(WasmFunction::new(
        "transfer",
        vec!["amount".to_string()],
        2,  // local 0 = amount, local 1 = alice_bal
        vec![
            // local[0] = amount (from param)
            WasmInstr::LocalGet(0),
            WasmInstr::LocalSet(0),
            // local[1] = alice_balance
            WasmInstr::StorageLoad("balance_alice".to_string()),
            WasmInstr::LocalSet(1),
            // check: alice_balance >= amount
            WasmInstr::LocalGet(1),
            WasmInstr::LocalGet(0),
            WasmInstr::I32Ge,
            WasmInstr::If(
                // then: sufficient balance
                vec![
                    // alice -= amount
                    WasmInstr::LocalGet(1),
                    WasmInstr::LocalGet(0),
                    WasmInstr::I32Sub,
                    WasmInstr::StorageStore("balance_alice".to_string()),
                    // bob += amount
                    WasmInstr::StorageLoad("balance_bob".to_string()),
                    WasmInstr::LocalGet(0),
                    WasmInstr::I32Add,
                    WasmInstr::StorageStore("balance_bob".to_string()),
                    // return 1 (success)
                    WasmInstr::I32Const(1),
                    WasmInstr::ReturnValue,
                ],
                // else: insufficient balance
                vec![
                    WasmInstr::I32Const(0),  // return 0 (fail)
                    WasmInstr::ReturnValue,
                ],
            ),
        ],
    ));

    module.add_function(WasmFunction::new(
        "balance_of_alice",
        vec![],
        0,
        vec![
            WasmInstr::StorageLoad("balance_alice".to_string()),
            WasmInstr::ReturnValue,
        ],
    ));

    module.add_function(WasmFunction::new(
        "balance_of_bob",
        vec![],
        0,
        vec![
            WasmInstr::StorageLoad("balance_bob".to_string()),
            WasmInstr::ReturnValue,
        ],
    ));

    module
}

/// Voting contract: vote(candidate_id 0 or 1), get_votes(candidate_id)
pub fn voting_contract() -> WasmModule {
    let mut module = WasmModule::new("Voting");

    // vote(candidate): increment vote count for candidate 0 or 1
    module.add_function(WasmFunction::new(
        "vote_a",
        vec![],
        0,
        vec![
            WasmInstr::StorageLoad("votes_a".to_string()),
            WasmInstr::I32Const(1),
            WasmInstr::I32Add,
            WasmInstr::StorageStore("votes_a".to_string()),
        ],
    ));

    module.add_function(WasmFunction::new(
        "vote_b",
        vec![],
        0,
        vec![
            WasmInstr::StorageLoad("votes_b".to_string()),
            WasmInstr::I32Const(1),
            WasmInstr::I32Add,
            WasmInstr::StorageStore("votes_b".to_string()),
        ],
    ));

    module.add_function(WasmFunction::new(
        "get_winner",
        vec![],
        0,
        vec![
            // return 1 if votes_a > votes_b, else 0
            WasmInstr::StorageLoad("votes_a".to_string()),
            WasmInstr::StorageLoad("votes_b".to_string()),
            WasmInstr::I32Gt,
            WasmInstr::ReturnValue,
        ],
    ));

    module.add_function(WasmFunction::new(
        "get_votes_a",
        vec![],
        0,
        vec![WasmInstr::StorageLoad("votes_a".to_string()), WasmInstr::ReturnValue],
    ));

    module.add_function(WasmFunction::new(
        "get_votes_b",
        vec![],
        0,
        vec![WasmInstr::StorageLoad("votes_b".to_string()), WasmInstr::ReturnValue],
    ));

    module
}
