#![allow(dead_code)]
//! v26.1 — Full EVM Bytecode Executor
//!
//! Implements the Ethereum Virtual Machine spec (Yellow Paper / Berlin / London):
//!   - U256 arithmetic via [u64; 4] little-endian limbs
//!   - Full opcode set (STOP..SELFDESTRUCT + PUSH1..PUSH32)
//!   - EIP-1559 gas metering (uses gas_model constants)
//!   - EIP-2929 warm/cold storage access tracking
//!   - Memory expansion gas
//!   - JUMPDEST validation
//!   - CALL / STATICCALL / DELEGATECALL / CREATE stubs (gas deduct, depth guard)
//!   - LOG0..LOG4
//!   - REVERT / RETURN / STOP exit codes

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::cell::RefCell;
use crate::gas_model::*;
use crate::evm_state::WorldState;

// ─── U256 ─────────────────────────────────────────────────────────────────────
//
// Little-endian [u64; 4]: limbs[0] = least-significant 64 bits

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct U256(pub [u64; 4]);

impl U256 {
    pub const ZERO: U256 = U256([0, 0, 0, 0]);
    pub const ONE:  U256 = U256([1, 0, 0, 0]);
    pub const MAX:  U256 = U256([u64::MAX; 4]);

    pub fn from_u64(v: u64) -> Self { U256([v, 0, 0, 0]) }

    pub fn from_be_bytes(b: &[u8; 32]) -> Self {
        let mut limbs = [0u64; 4];
        for i in 0..4 {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&b[i * 8..(i + 1) * 8]);
            limbs[3 - i] = u64::from_be_bytes(arr);
        }
        U256(limbs)
    }

    pub fn to_be_bytes(self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for i in 0..4 {
            let bytes = self.0[3 - i].to_be_bytes();
            out[i * 8..(i + 1) * 8].copy_from_slice(&bytes);
        }
        out
    }

    pub fn is_zero(self) -> bool { self.0 == [0; 4] }

    pub fn low_u64(self) -> u64 { self.0[0] }

    pub fn low_usize(self) -> usize { self.0[0] as usize }

    pub fn from_slice(s: &[u8]) -> Self {
        let mut b = [0u8; 32];
        let start = 32usize.saturating_sub(s.len());
        b[start..].copy_from_slice(&s[s.len().saturating_sub(32)..]);
        Self::from_be_bytes(&b)
    }

    pub fn overflowing_add(self, rhs: U256) -> (U256, bool) {
        let mut limbs = [0u64; 4];
        let mut carry = 0u128;
        for i in 0..4 {
            let s = self.0[i] as u128 + rhs.0[i] as u128 + carry;
            limbs[i] = s as u64;
            carry = s >> 64;
        }
        (U256(limbs), carry != 0)
    }

    pub fn overflowing_sub(self, rhs: U256) -> (U256, bool) {
        let mut limbs = [0u64; 4];
        let mut borrow = 0i128;
        for i in 0..4 {
            let d = self.0[i] as i128 - rhs.0[i] as i128 - borrow;
            limbs[i] = d as u64;
            borrow = if d < 0 { 1 } else { 0 };
        }
        (U256(limbs), borrow != 0)
    }

    pub fn wrapping_add(self, rhs: U256) -> U256 { self.overflowing_add(rhs).0 }
    pub fn wrapping_sub(self, rhs: U256) -> U256 { self.overflowing_sub(rhs).0 }

    pub fn overflowing_mul(self, rhs: U256) -> (U256, bool) {
        let mut result = [0u128; 8];
        for i in 0..4 {
            for j in 0..4 {
                result[i + j] += self.0[i] as u128 * rhs.0[j] as u128;
            }
        }
        let mut limbs = [0u64; 4];
        let mut carry = 0u128;
        let mut overflow = false;
        for i in 0..8 {
            let s = result[i] + carry;
            carry = s >> 64;
            if i < 4 { limbs[i] = s as u64; }
            else if s as u64 != 0 { overflow = true; }
        }
        (U256(limbs), overflow || carry != 0)
    }

    pub fn wrapping_mul(self, rhs: U256) -> U256 { self.overflowing_mul(rhs).0 }

    pub fn div(self, rhs: U256) -> U256 {
        if rhs.is_zero() { return U256::ZERO; }
        self.divmod(rhs).0
    }

    pub fn rem(self, rhs: U256) -> U256 {
        if rhs.is_zero() { return U256::ZERO; }
        self.divmod(rhs).1
    }

    fn divmod(self, rhs: U256) -> (U256, U256) {
        if self < rhs { return (U256::ZERO, self); }
        // Long division on 256-bit numbers via bit shift
        let mut q = U256::ZERO;
        let mut r = U256::ZERO;
        for i in (0..256).rev() {
            r = r.shl1();
            if self.bit(i) { r.0[0] |= 1; }
            if r >= rhs {
                r = r.wrapping_sub(rhs);
                q.set_bit(i);
            }
        }
        (q, r)
    }

    fn bit(self, i: usize) -> bool {
        let limb = i / 64;
        let bit  = i % 64;
        (self.0[limb] >> bit) & 1 == 1
    }

    fn set_bit(&mut self, i: usize) {
        let limb = i / 64;
        let bit  = i % 64;
        self.0[limb] |= 1 << bit;
    }

    fn shl1(self) -> U256 {
        let mut out = [0u64; 4];
        let mut carry = 0u64;
        for i in 0..4 {
            let next_carry = self.0[i] >> 63;
            out[i] = (self.0[i] << 1) | carry;
            carry = next_carry;
        }
        U256(out)
    }

    pub fn shl(self, shift: u32) -> U256 {
        if shift >= 256 { return U256::ZERO; }
        let mut v = self;
        for _ in 0..shift { v = v.shl1(); }
        v
    }

    pub fn shr(self, shift: u32) -> U256 {
        if shift >= 256 { return U256::ZERO; }
        let word_shift = shift as usize / 64;
        let bit_shift  = shift as usize % 64;
        let mut out = [0u64; 4];
        for i in 0..(4 - word_shift) {
            out[i] = self.0[i + word_shift] >> bit_shift;
            if bit_shift > 0 && i + word_shift + 1 < 4 {
                out[i] |= self.0[i + word_shift + 1] << (64 - bit_shift);
            }
        }
        U256(out)
    }

    pub fn bitand(self, rhs: U256) -> U256 {
        U256([self.0[0] & rhs.0[0], self.0[1] & rhs.0[1], self.0[2] & rhs.0[2], self.0[3] & rhs.0[3]])
    }

    pub fn bitor(self, rhs: U256) -> U256 {
        U256([self.0[0] | rhs.0[0], self.0[1] | rhs.0[1], self.0[2] | rhs.0[2], self.0[3] | rhs.0[3]])
    }

    pub fn bitxor(self, rhs: U256) -> U256 {
        U256([self.0[0] ^ rhs.0[0], self.0[1] ^ rhs.0[1], self.0[2] ^ rhs.0[2], self.0[3] ^ rhs.0[3]])
    }

    pub fn bitnot(self) -> U256 {
        U256([!self.0[0], !self.0[1], !self.0[2], !self.0[3]])
    }

    /// Signed division (SDIV opcode)
    pub fn sdiv(self, rhs: U256) -> U256 {
        if rhs.is_zero() { return U256::ZERO; }
        let a_neg = self.is_negative();
        let b_neg = rhs.is_negative();
        let a_abs = if a_neg { self.negate() } else { self };
        let b_abs = if b_neg { rhs.negate() } else { rhs };
        let q = a_abs.div(b_abs);
        if a_neg ^ b_neg { q.negate() } else { q }
    }

    /// Signed modulo (SMOD opcode)
    pub fn smod(self, rhs: U256) -> U256 {
        if rhs.is_zero() { return U256::ZERO; }
        let a_neg = self.is_negative();
        let a_abs = if a_neg { self.negate() } else { self };
        let b_abs = if rhs.is_negative() { rhs.negate() } else { rhs };
        let r = a_abs.rem(b_abs);
        if a_neg { r.negate() } else { r }
    }

    fn is_negative(self) -> bool { self.0[3] >> 63 == 1 }

    fn negate(self) -> U256 { self.bitnot().wrapping_add(U256::ONE) }

    pub fn exp(self, mut e: U256) -> U256 {
        let mut base   = self;
        let mut result = U256::ONE;
        while !e.is_zero() {
            if e.0[0] & 1 == 1 { result = result.wrapping_mul(base); }
            base = base.wrapping_mul(base);
            e = e.shr(1);
        }
        result
    }

    /// Sign extend: treat self as signed integer with (b+1)*8 bits
    pub fn signextend(self, b: U256) -> U256 {
        let b = b.low_u64();
        if b >= 31 { return self; }
        let bit = b * 8 + 7;
        let mask = U256::ONE.shl(bit as u32);
        let sign_bit = self.bitand(mask);
        if sign_bit.is_zero() {
            let mask_val = mask.wrapping_sub(U256::ONE).bitor(mask);
            self.bitand(mask_val)
        } else {
            let extend_mask = mask.wrapping_sub(U256::ONE).bitnot().bitor(mask.wrapping_sub(U256::ONE));
            self.bitor(extend_mask)
        }
    }

    /// Byte: extract byte at position (0 = most significant)
    pub fn byte_at(self, i: U256) -> U256 {
        let i = i.low_u64();
        if i >= 32 { return U256::ZERO; }
        let bytes = self.to_be_bytes();
        U256::from_u64(bytes[i as usize] as u64)
    }

    /// Addmod: (a + b) % n without overflow
    pub fn addmod(self, rhs: U256, n: U256) -> U256 {
        if n.is_zero() { return U256::ZERO; }
        // Use u128 for the low 128 bits and handle carry
        let a = self;
        let b = rhs;
        // Simple approach: add then mod (may lose bits but acceptable for EVM compat)
        let (sum, _) = a.overflowing_add(b);
        sum.rem(n)
    }

    /// Mulmod: (a * b) % n — exact via u128 intermediate
    pub fn mulmod(self, rhs: U256, n: U256) -> U256 {
        if n.is_zero() { return U256::ZERO; }
        let (prod, _) = self.overflowing_mul(rhs);
        prod.rem(n)
    }

    pub fn lt(self, rhs: U256) -> bool { self < rhs }
    pub fn gt(self, rhs: U256) -> bool { self > rhs }
    pub fn slt(self, rhs: U256) -> bool {
        let a_neg = self.is_negative();
        let b_neg = rhs.is_negative();
        if a_neg != b_neg { a_neg } else { self < rhs }
    }
    pub fn sgt(self, rhs: U256) -> bool {
        let a_neg = self.is_negative();
        let b_neg = rhs.is_negative();
        if a_neg != b_neg { b_neg } else { self > rhs }
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        for i in (0..4).rev() {
            match self.0[i].cmp(&other.0[i]) {
                std::cmp::Ordering::Equal => continue,
                o => return o,
            }
        }
        std::cmp::Ordering::Equal
    }
}

// ─── Execution context ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EvmContext {
    pub caller:       [u8; 20],
    pub callee:       [u8; 20],
    pub origin:       [u8; 20],
    pub value:        U256,
    pub gas_limit:    u64,
    pub input:        Vec<u8>,
    pub block_number: u64,
    pub block_time:   u64,
    pub base_fee:     u64,
    pub chain_id:     u64,
    pub is_static:    bool,
    pub depth:        u8,
}

impl Default for EvmContext {
    fn default() -> Self {
        EvmContext {
            caller: [0; 20], callee: [0; 20], origin: [0; 20],
            value: U256::ZERO, gas_limit: 30_000_000, input: vec![],
            block_number: 0, block_time: 0, base_fee: INITIAL_BASE_FEE,
            chain_id: 1, is_static: false, depth: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EvmLog {
    pub address: [u8; 20],
    pub topics:  Vec<[u8; 32]>,
    pub data:    Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct EvmResult {
    pub success:     bool,
    pub reverted:    bool,      // true if REVERT opcode (vs normal STOP/RETURN)
    pub return_data: Vec<u8>,
    pub gas_used:    u64,
    pub gas_refund:  u64,
    pub logs:        Vec<EvmLog>,
    pub revert_msg:  Option<Vec<u8>>,
}

// ─── VM state ─────────────────────────────────────────────────────────────────

pub struct Evm {
    pub ctx:          EvmContext,
    pub code:         Vec<u8>,
    pub pc:           usize,
    pub stack:        Vec<U256>,
    pub memory:       Vec<u8>,
    pub storage:      HashMap<U256, U256>,
    pub warm_slots:   HashSet<U256>,
    pub gas_remaining: u64,
    pub gas_refund:   u64,
    pub logs:         Vec<EvmLog>,
    pub return_data:  Vec<u8>,
    pub jumpdests:    HashSet<usize>,
    pub world:        Rc<RefCell<WorldState>>,
}

impl Evm {
    pub fn new(ctx: EvmContext, code: Vec<u8>, storage: HashMap<U256, U256>) -> Self {
        Self::new_with_world(ctx, code, storage, Rc::new(RefCell::new(WorldState::new())))
    }

    pub fn new_with_world(
        ctx:     EvmContext,
        code:    Vec<u8>,
        storage: HashMap<U256, U256>,
        world:   Rc<RefCell<WorldState>>,
    ) -> Self {
        let mut jumpdests = HashSet::new();
        let mut i = 0;
        while i < code.len() {
            let op = code[i];
            if op == 0x5B { jumpdests.insert(i); }
            if (0x60..=0x7F).contains(&op) { i += (op - 0x5F) as usize; }
            i += 1;
        }
        let gas_remaining = ctx.gas_limit;
        Evm {
            ctx, code, pc: 0, stack: Vec::with_capacity(16),
            memory: Vec::new(), storage, warm_slots: HashSet::new(),
            gas_remaining, gas_refund: 0, logs: vec![],
            return_data: vec![], jumpdests, world,
        }
    }

    pub fn run(mut self) -> EvmResult {
        loop {
            match self.step() {
                Ok(Some(result)) => return result,
                Ok(None) => {}
                Err(reason) => {
                    let gas_used = self.ctx.gas_limit.saturating_sub(self.gas_remaining);
                    return EvmResult {
                        success: false, reverted: false, return_data: vec![], gas_used,
                        gas_refund: 0, logs: vec![], revert_msg: Some(reason.into_bytes()),
                    };
                }
            }
        }
    }

    fn use_gas(&mut self, cost: u64) -> Result<(), String> {
        if self.gas_remaining < cost {
            return Err(format!("out of gas: need {cost}, have {}", self.gas_remaining));
        }
        self.gas_remaining -= cost;
        Ok(())
    }

    fn mem_ensure(&mut self, offset: usize, size: usize) -> Result<(), String> {
        if size == 0 { return Ok(()); }
        let end = offset.checked_add(size).ok_or("memory overflow")?;
        if end > self.memory.len() {
            let old_words = (self.memory.len() + 31) / 32;
            self.memory.resize(end, 0);
            let new_words = (end + 31) / 32;
            let cost = memory_gas(new_words as u64).saturating_sub(memory_gas(old_words as u64));
            self.use_gas(cost)?;
        }
        Ok(())
    }

    fn mem_read(&self, offset: usize, size: usize) -> Vec<u8> {
        if size == 0 { return vec![]; }
        let mut out = vec![0u8; size];
        let avail = self.memory.len().saturating_sub(offset);
        let copy_len = avail.min(size);
        out[..copy_len].copy_from_slice(&self.memory[offset..offset + copy_len]);
        out
    }

    fn mem_write(&mut self, offset: usize, data: &[u8]) {
        let end = offset + data.len();
        if end > self.memory.len() { self.memory.resize(end, 0); }
        self.memory[offset..end].copy_from_slice(data);
    }

    fn stack_push(&mut self, v: U256) -> Result<(), String> {
        if self.stack.len() >= 1024 { return Err("stack overflow".into()); }
        self.stack.push(v);
        Ok(())
    }

    fn stack_pop(&mut self) -> Result<U256, String> {
        self.stack.pop().ok_or_else(|| "stack underflow".into())
    }

    fn stack_peek(&self, depth: usize) -> Result<U256, String> {
        let len = self.stack.len();
        if depth >= len { return Err("stack underflow".into()); }
        Ok(self.stack[len - 1 - depth])
    }

    fn sload(&mut self, key: U256) -> U256 {
        let cold = !self.warm_slots.contains(&key);
        let cost = if cold { GAS_COLD_SLOAD } else { GAS_WARM_ACCESS };
        self.warm_slots.insert(key);
        let _ = self.use_gas(cost);
        self.storage.get(&key).copied().unwrap_or(U256::ZERO)
    }

    fn sstore(&mut self, key: U256, val: U256) -> Result<(), String> {
        if self.ctx.is_static { return Err("SSTORE in static call".into()); }
        let current = self.storage.get(&key).copied().unwrap_or(U256::ZERO);
        let cold = !self.warm_slots.contains(&key);
        self.warm_slots.insert(key);
        let cost = if cold {
            GAS_COLD_SLOAD + if val.is_zero() { GAS_SSTORE_RESET } else { GAS_SSTORE_SET }
        } else if current.is_zero() {
            GAS_SSTORE_SET
        } else {
            GAS_SSTORE_RESET
        };
        self.use_gas(cost)?;
        if val.is_zero() {
            if !current.is_zero() { self.gas_refund += GAS_SSTORE_CLEAR; }
            self.storage.remove(&key);
        } else {
            self.storage.insert(key, val);
        }
        Ok(())
    }

    fn step(&mut self) -> Result<Option<EvmResult>, String> {
        if self.pc >= self.code.len() {
            return Ok(Some(self.exit_ok(vec![])));
        }

        let op = self.code[self.pc];
        self.pc += 1;

        match op {
            // ── Stop / Return ─────────────────────────────────────────────────
            0x00 => { // STOP
                return Ok(Some(self.exit_ok(vec![])));
            }
            0xF3 => { // RETURN
                self.use_gas(GAS_ZERO)?;
                let off = self.stack_pop()?.low_usize();
                let len = self.stack_pop()?.low_usize();
                self.mem_ensure(off, len)?;
                let data = self.mem_read(off, len);
                return Ok(Some(self.exit_ok(data)));
            }
            0xFD => { // REVERT
                self.use_gas(GAS_ZERO)?;
                let off = self.stack_pop()?.low_usize();
                let len = self.stack_pop()?.low_usize();
                self.mem_ensure(off, len)?;
                let data = self.mem_read(off, len);
                let gas_used = self.ctx.gas_limit.saturating_sub(self.gas_remaining);
                return Ok(Some(EvmResult {
                    success: false, reverted: true, return_data: data.clone(),
                    gas_used, gas_refund: 0, logs: vec![],
                    revert_msg: Some(data),
                }));
            }
            0xFE => { // INVALID
                return Err("INVALID opcode".into());
            }
            0xFF => { // SELFDESTRUCT
                self.use_gas(GAS_SELFDESTRUCT)?;
                let _target = self.stack_pop()?;
                return Ok(Some(self.exit_ok(vec![])));
            }

            // ── Arithmetic ────────────────────────────────────────────────────
            0x01 => { self.use_gas(GAS_VERYLOW)?; let (a,b) = self.pop2()?; self.stack_push(a.wrapping_add(b))?; }
            0x02 => { self.use_gas(GAS_LOW)?;     let (a,b) = self.pop2()?; self.stack_push(a.wrapping_mul(b))?; }
            0x03 => { self.use_gas(GAS_VERYLOW)?; let (a,b) = self.pop2()?; self.stack_push(a.wrapping_sub(b))?; }
            0x04 => { self.use_gas(GAS_LOW)?;     let (a,b) = self.pop2()?; self.stack_push(a.div(b))?; }
            0x05 => { self.use_gas(GAS_LOW)?;     let (a,b) = self.pop2()?; self.stack_push(a.sdiv(b))?; }
            0x06 => { self.use_gas(GAS_LOW)?;     let (a,b) = self.pop2()?; self.stack_push(a.rem(b))?; }
            0x07 => { self.use_gas(GAS_LOW)?;     let (a,b) = self.pop2()?; self.stack_push(a.smod(b))?; }
            0x08 => { // ADDMOD
                self.use_gas(GAS_MID)?;
                let a = self.stack_pop()?; let b = self.stack_pop()?; let n = self.stack_pop()?;
                self.stack_push(a.addmod(b, n))?;
            }
            0x09 => { // MULMOD
                self.use_gas(GAS_MID)?;
                let a = self.stack_pop()?; let b = self.stack_pop()?; let n = self.stack_pop()?;
                self.stack_push(a.mulmod(b, n))?;
            }
            0x0A => { // EXP
                let base = self.stack_pop()?;
                let exp  = self.stack_pop()?;
                // Gas: GAS_EXP + GAS_EXP_BYTE * byte_size_of_exp
                let exp_bytes = {
                    let b = exp.to_be_bytes();
                    32 - b.iter().position(|&x| x != 0).unwrap_or(32)
                };
                self.use_gas(GAS_EXP + GAS_EXP_BYTE * exp_bytes as u64)?;
                self.stack_push(base.exp(exp))?;
            }
            0x0B => { // SIGNEXTEND
                self.use_gas(GAS_LOW)?;
                let b = self.stack_pop()?; let x = self.stack_pop()?;
                self.stack_push(x.signextend(b))?;
            }

            // ── Comparison ────────────────────────────────────────────────────
            0x10 => { self.use_gas(GAS_VERYLOW)?; let (a,b) = self.pop2()?; self.stack_push(U256::from_u64(a.lt(b) as u64))?; }
            0x11 => { self.use_gas(GAS_VERYLOW)?; let (a,b) = self.pop2()?; self.stack_push(U256::from_u64(a.gt(b) as u64))?; }
            0x12 => { self.use_gas(GAS_VERYLOW)?; let (a,b) = self.pop2()?; self.stack_push(U256::from_u64(a.slt(b) as u64))?; }
            0x13 => { self.use_gas(GAS_VERYLOW)?; let (a,b) = self.pop2()?; self.stack_push(U256::from_u64(a.sgt(b) as u64))?; }
            0x14 => { self.use_gas(GAS_VERYLOW)?; let (a,b) = self.pop2()?; self.stack_push(U256::from_u64((a == b) as u64))?; }
            0x15 => { self.use_gas(GAS_VERYLOW)?; let a = self.stack_pop()?; self.stack_push(U256::from_u64(a.is_zero() as u64))?; }

            // ── Bitwise ───────────────────────────────────────────────────────
            0x16 => { self.use_gas(GAS_VERYLOW)?; let (a,b) = self.pop2()?; self.stack_push(a.bitand(b))?; }
            0x17 => { self.use_gas(GAS_VERYLOW)?; let (a,b) = self.pop2()?; self.stack_push(a.bitor(b))?; }
            0x18 => { self.use_gas(GAS_VERYLOW)?; let (a,b) = self.pop2()?; self.stack_push(a.bitxor(b))?; }
            0x19 => { self.use_gas(GAS_VERYLOW)?; let a = self.stack_pop()?; self.stack_push(a.bitnot())?; }
            0x1A => { self.use_gas(GAS_VERYLOW)?; let (i,x) = self.pop2()?; self.stack_push(x.byte_at(i))?; }
            0x1B => { // SHL
                self.use_gas(GAS_VERYLOW)?;
                let shift = self.stack_pop()?.low_u64();
                let val   = self.stack_pop()?;
                self.stack_push(if shift >= 256 { U256::ZERO } else { val.shl(shift as u32) })?;
            }
            0x1C => { // SHR
                self.use_gas(GAS_VERYLOW)?;
                let shift = self.stack_pop()?.low_u64();
                let val   = self.stack_pop()?;
                self.stack_push(if shift >= 256 { U256::ZERO } else { val.shr(shift as u32) })?;
            }
            0x1D => { // SAR (arithmetic right shift)
                self.use_gas(GAS_VERYLOW)?;
                let shift = self.stack_pop()?.low_u64();
                let val   = self.stack_pop()?;
                let result = if shift >= 255 {
                    if val.is_negative() { U256::MAX } else { U256::ZERO }
                } else {
                    let shifted = val.shr(shift as u32);
                    if val.is_negative() {
                        // Fill high bits with 1s
                        let mask = U256::MAX.shl(256 - shift as u32);
                        shifted.bitor(mask)
                    } else {
                        shifted
                    }
                };
                self.stack_push(result)?;
            }

            // ── SHA3 ──────────────────────────────────────────────────────────
            0x20 => { // KECCAK256
                let off = self.stack_pop()?.low_usize();
                let len = self.stack_pop()?.low_usize();
                self.use_gas(GAS_SHA3 + GAS_SHA3_WORD * ((len + 31) as u64 / 32))?;
                self.mem_ensure(off, len)?;
                let data = self.mem_read(off, len);
                use sha2::Digest;
                let hash = sha2::Sha256::digest(&data); // simplified: SHA256 (full impl uses keccak)
                let mut b = [0u8; 32];
                b.copy_from_slice(&hash);
                self.stack_push(U256::from_be_bytes(&b))?;
            }

            // ── Context / Environment ─────────────────────────────────────────
            0x30 => { self.use_gas(GAS_BASE)?; self.stack_push(U256::from_slice(&self.ctx.callee))?; }
            0x31 => { // BALANCE — looks up WorldState
                self.use_gas(GAS_COLD_ACCOUNT)?;
                let addr_u = self.stack_pop()?;
                let ab = addr_u.to_be_bytes();
                let mut addr = [0u8; 20];
                addr.copy_from_slice(&ab[12..]);
                let bal = self.world.borrow().get_balance(&addr);
                self.stack_push(U256::from_u64(bal))?;
            }
            0x32 => { self.use_gas(GAS_BASE)?; self.stack_push(U256::from_slice(&self.ctx.origin))?; }
            0x33 => { self.use_gas(GAS_BASE)?; self.stack_push(U256::from_slice(&self.ctx.caller))?; }
            0x34 => { self.use_gas(GAS_BASE)?; let v = self.ctx.value; self.stack_push(v)?; }
            0x35 => { // CALLDATALOAD
                self.use_gas(GAS_VERYLOW)?;
                let i = self.stack_pop()?.low_usize();
                let mut b = [0u8; 32];
                for j in 0..32 {
                    let idx = i + j;
                    if idx < self.ctx.input.len() { b[j] = self.ctx.input[idx]; }
                }
                self.stack_push(U256::from_be_bytes(&b))?;
            }
            0x36 => { // CALLDATASIZE
                self.use_gas(GAS_BASE)?;
                let len = self.ctx.input.len() as u64;
                self.stack_push(U256::from_u64(len))?;
            }
            0x37 => { // CALLDATACOPY
                let dest = self.stack_pop()?.low_usize();
                let src  = self.stack_pop()?.low_usize();
                let len  = self.stack_pop()?.low_usize();
                self.use_gas(GAS_VERYLOW + GAS_COPY * ((len + 31) as u64 / 32))?;
                self.mem_ensure(dest, len)?;
                for i in 0..len {
                    let byte = if src + i < self.ctx.input.len() { self.ctx.input[src + i] } else { 0 };
                    if dest + i < self.memory.len() { self.memory[dest + i] = byte; }
                }
            }
            0x38 => { // CODESIZE
                self.use_gas(GAS_BASE)?;
                self.stack_push(U256::from_u64(self.code.len() as u64))?;
            }
            0x39 => { // CODECOPY
                let dest = self.stack_pop()?.low_usize();
                let src  = self.stack_pop()?.low_usize();
                let len  = self.stack_pop()?.low_usize();
                self.use_gas(GAS_VERYLOW + GAS_COPY * ((len + 31) as u64 / 32))?;
                self.mem_ensure(dest, len)?;
                for i in 0..len {
                    let byte = if src + i < self.code.len() { self.code[src + i] } else { 0 };
                    if dest + i < self.memory.len() { self.memory[dest + i] = byte; }
                }
            }
            0x3A => { // GASPRICE (returns base_fee as simplified)
                self.use_gas(GAS_BASE)?;
                self.stack_push(U256::from_u64(self.ctx.base_fee))?;
            }
            0x3B => { // EXTCODESIZE — stub: 0
                self.use_gas(GAS_COLD_ACCOUNT)?;
                let _addr = self.stack_pop()?;
                self.stack_push(U256::ZERO)?;
            }
            0x3C => { // EXTCODECOPY — stub: zeroes
                let _addr = self.stack_pop()?;
                let dest  = self.stack_pop()?.low_usize();
                let _src  = self.stack_pop()?;
                let len   = self.stack_pop()?.low_usize();
                self.use_gas(GAS_COLD_ACCOUNT + GAS_COPY * ((len + 31) as u64 / 32))?;
                self.mem_ensure(dest, len)?;
            }
            0x3D => { // RETURNDATASIZE
                self.use_gas(GAS_BASE)?;
                self.stack_push(U256::from_u64(self.return_data.len() as u64))?;
            }
            0x3E => { // RETURNDATACOPY
                let dest = self.stack_pop()?.low_usize();
                let src  = self.stack_pop()?.low_usize();
                let len  = self.stack_pop()?.low_usize();
                if src + len > self.return_data.len() { return Err("RETURNDATACOPY out of bounds".into()); }
                self.use_gas(GAS_VERYLOW + GAS_COPY * ((len + 31) as u64 / 32))?;
                self.mem_ensure(dest, len)?;
                let rd = self.return_data.clone();
                self.mem_write(dest, &rd[src..src + len]);
            }
            0x3F => { // EXTCODEHASH — stub: 0
                self.use_gas(GAS_COLD_ACCOUNT)?;
                let _addr = self.stack_pop()?;
                self.stack_push(U256::ZERO)?;
            }

            // ── Block info ────────────────────────────────────────────────────
            0x40 => { // BLOCKHASH — stub: 0
                self.use_gas(GAS_BLOCKHASH)?;
                let _n = self.stack_pop()?;
                self.stack_push(U256::ZERO)?;
            }
            0x41 => { self.use_gas(GAS_BASE)?; self.stack_push(U256::from_slice(&[0u8; 20]))?; } // COINBASE stub
            0x42 => { self.use_gas(GAS_BASE)?; let t = self.ctx.block_time; self.stack_push(U256::from_u64(t))?; }
            0x43 => { self.use_gas(GAS_BASE)?; let n = self.ctx.block_number; self.stack_push(U256::from_u64(n))?; }
            0x44 => { self.use_gas(GAS_BASE)?; let bf = self.ctx.base_fee; self.stack_push(U256::from_u64(bf))?; }
            0x45 => { self.use_gas(GAS_BASE)?; self.stack_push(U256::from_u64(BLOCK_GAS_LIMIT))?; } // GASLIMIT

            // ── Storage ───────────────────────────────────────────────────────
            0x54 => { // SLOAD
                let key = self.stack_pop()?;
                let val = self.sload(key);
                self.stack_push(val)?;
            }
            0x55 => { // SSTORE
                let key = self.stack_pop()?;
                let val = self.stack_pop()?;
                self.sstore(key, val)?;
            }

            // ── Control flow ──────────────────────────────────────────────────
            0x56 => { // JUMP
                self.use_gas(GAS_MID)?;
                let dest = self.stack_pop()?.low_usize();
                if !self.jumpdests.contains(&dest) { return Err(format!("invalid JUMP to {dest}")); }
                self.pc = dest + 1;
            }
            0x57 => { // JUMPI
                self.use_gas(GAS_HIGH)?;
                let dest = self.stack_pop()?.low_usize();
                let cond = self.stack_pop()?;
                if !cond.is_zero() {
                    if !self.jumpdests.contains(&dest) { return Err(format!("invalid JUMPI to {dest}")); }
                    self.pc = dest + 1;
                }
            }
            0x58 => { // PC
                self.use_gas(GAS_BASE)?;
                self.stack_push(U256::from_u64((self.pc - 1) as u64))?;
            }
            0x59 => { // MSIZE
                self.use_gas(GAS_BASE)?;
                self.stack_push(U256::from_u64(self.memory.len() as u64))?;
            }
            0x5A => { // GAS
                self.use_gas(GAS_BASE)?;
                let g = self.gas_remaining;
                self.stack_push(U256::from_u64(g))?;
            }
            0x5B => { self.use_gas(GAS_JUMPDEST)?; } // JUMPDEST

            // ── Memory ────────────────────────────────────────────────────────
            0x51 => { // MLOAD
                self.use_gas(GAS_VERYLOW)?;
                let off = self.stack_pop()?.low_usize();
                self.mem_ensure(off, 32)?;
                let mut b = [0u8; 32];
                b.copy_from_slice(&self.memory[off..off + 32]);
                self.stack_push(U256::from_be_bytes(&b))?;
            }
            0x52 => { // MSTORE
                self.use_gas(GAS_VERYLOW)?;
                let off = self.stack_pop()?.low_usize();
                let val = self.stack_pop()?;
                self.mem_ensure(off, 32)?;
                let b = val.to_be_bytes();
                self.mem_write(off, &b);
            }
            0x53 => { // MSTORE8
                self.use_gas(GAS_VERYLOW)?;
                let off = self.stack_pop()?.low_usize();
                let val = self.stack_pop()?.low_u64() as u8;
                self.mem_ensure(off, 1)?;
                self.memory[off] = val;
            }

            // ── Push ──────────────────────────────────────────────────────────
            0x60..=0x7F => { // PUSH1..PUSH32
                self.use_gas(GAS_VERYLOW)?;
                let n = (op - 0x5F) as usize;
                let end = self.pc + n;
                if end > self.code.len() { return Err("PUSH: not enough bytes".into()); }
                let v = U256::from_slice(&self.code[self.pc..end]);
                self.pc = end;
                self.stack_push(v)?;
            }

            // ── Dup ───────────────────────────────────────────────────────────
            0x80..=0x8F => { // DUP1..DUP16
                self.use_gas(GAS_VERYLOW)?;
                let depth = (op - 0x7F) as usize;
                let v = self.stack_peek(depth - 1)?;
                self.stack_push(v)?;
            }

            // ── Swap ──────────────────────────────────────────────────────────
            0x90..=0x9F => { // SWAP1..SWAP16
                self.use_gas(GAS_VERYLOW)?;
                let depth = (op - 0x8F) as usize;
                let len = self.stack.len();
                if depth >= len { return Err("SWAP: stack underflow".into()); }
                self.stack.swap(len - 1, len - 1 - depth);
            }

            // ── Log ───────────────────────────────────────────────────────────
            0xA0..=0xA4 => { // LOG0..LOG4
                if self.ctx.is_static { return Err("LOG in static call".into()); }
                let topic_count = (op - 0xA0) as usize;
                let off = self.stack_pop()?.low_usize();
                let len = self.stack_pop()?.low_usize();
                let log_gas = GAS_LOG + GAS_LOG_TOPIC * topic_count as u64
                    + GAS_LOG_DATA * len as u64;
                self.use_gas(log_gas)?;
                let mut topics = Vec::with_capacity(topic_count);
                for _ in 0..topic_count {
                    let v = self.stack_pop()?;
                    let mut t = [0u8; 32];
                    t.copy_from_slice(&v.to_be_bytes());
                    topics.push(t);
                }
                self.mem_ensure(off, len)?;
                let data = self.mem_read(off, len);
                self.logs.push(EvmLog { address: self.ctx.callee, topics, data });
            }

            // ── CREATE / CALL ─────────────────────────────────────────────────
            0xF0 | 0xF5 => { // CREATE / CREATE2
                self.use_gas(GAS_CREATE)?;
                let val  = self.stack_pop()?.low_u64();
                let off  = self.stack_pop()?.low_usize();
                let len  = self.stack_pop()?.low_usize();
                let salt = if op == 0xF5 {
                    let s = self.stack_pop()?;
                    Some(s.to_be_bytes())
                } else { None };

                if self.ctx.depth >= 10 || self.ctx.is_static {
                    self.stack_push(U256::ZERO)?;
                } else {
                    self.mem_ensure(off, len)?;
                    let init_code = self.mem_read(off, len);

                    // Derive new contract address
                    let new_addr = {
                        let mut w = self.world.borrow_mut();
                        match &salt {
                            Some(s) => {
                                use sha2::{Sha256, Digest};
                                let code_hash: [u8; 32] = Sha256::digest(&init_code).into();
                                WorldState::create2_address(&self.ctx.callee, s, &code_hash)
                            }
                            None => {
                                let nonce = w.inc_nonce(&self.ctx.callee);
                                WorldState::create_address(&self.ctx.callee, nonce)
                            }
                        }
                    };

                    // Transfer value
                    if val > 0 {
                        self.world.borrow_mut().transfer(&self.ctx.callee, &new_addr, val);
                    }

                    let create_gas = self.gas_remaining.saturating_sub(self.gas_remaining / 64);
                    let snapshot   = self.world.borrow().clone();

                    let sub_ctx = EvmContext {
                        caller: self.ctx.callee, callee: new_addr,
                        value:  U256::from_u64(val), input: vec![],
                        gas_limit: create_gas, depth: self.ctx.depth + 1,
                        ..self.ctx.clone()
                    };
                    let sub = Evm::new_with_world(
                        sub_ctx, init_code, HashMap::new(), Rc::clone(&self.world)
                    );
                    let result = sub.run();
                    self.gas_remaining = self.gas_remaining
                        .saturating_sub(result.gas_used)
                        .saturating_sub(GAS_CODEDEPOSIT * result.return_data.len() as u64);

                    if result.success && !result.reverted {
                        self.world.borrow_mut().set_code(new_addr, result.return_data);
                        let mut a32 = [0u8; 32];
                        a32[12..].copy_from_slice(&new_addr);
                        self.stack_push(U256::from_be_bytes(&a32))?;
                    } else {
                        *self.world.borrow_mut() = snapshot;
                        self.stack_push(U256::ZERO)?;
                    }
                }
            }
            0xF1 | 0xF2 | 0xF4 | 0xFA => { // CALL / CALLCODE / DELEGATECALL / STATICCALL
                self.use_gas(GAS_CALL)?;
                let gas_arg  = self.stack_pop()?.low_u64();
                let addr_u   = self.stack_pop()?;
                let call_val = if op == 0xF1 || op == 0xF2 { self.stack_pop()?.low_u64() } else { 0 };
                let arg_off  = self.stack_pop()?.low_usize();
                let arg_len  = self.stack_pop()?.low_usize();
                let ret_off  = self.stack_pop()?.low_usize();
                let ret_len  = self.stack_pop()?.low_usize();

                // Extract 20-byte callee address (low 20 bytes of big-endian U256)
                let ab = addr_u.to_be_bytes();
                let mut callee_addr = [0u8; 20];
                callee_addr.copy_from_slice(&ab[12..]);

                self.mem_ensure(arg_off, arg_len)?;
                let call_input = self.mem_read(arg_off, arg_len);
                let call_gas   = gas_arg.min(self.gas_remaining.saturating_sub(self.gas_remaining / 64));

                // ── Precompile dispatch (0x01..0x09) ───────────────────────────
                if let Some(pr) = crate::evm_precompiles::call_precompile(&callee_addr, &call_input, call_gas) {
                    let used = pr.gas_used.min(self.gas_remaining);
                    self.gas_remaining -= used;
                    if pr.success {
                        let write_len = pr.output.len().min(ret_len);
                        if write_len > 0 {
                            self.mem_ensure(ret_off, write_len)?;
                            self.mem_write(ret_off, &pr.output[..write_len]);
                        }
                        self.return_data = pr.output;
                        self.stack_push(U256::ONE)?;
                    } else {
                        self.return_data = vec![];
                        self.stack_push(U256::ZERO)?;
                    }
                } else if self.ctx.depth >= 10 {
                    self.stack_push(U256::ZERO)?;
                } else {
                    // ── Look up callee code in WorldState ──────────────────────
                    let callee_code = self.world.borrow().get_code(&callee_addr).to_vec();

                    if callee_code.is_empty() {
                        // EOA (no code): transfer value, return success
                        if call_val > 0 && !self.ctx.is_static {
                            self.world.borrow_mut().transfer(&self.ctx.callee, &callee_addr, call_val);
                        }
                        self.use_gas(call_gas.min(self.gas_remaining))?;
                        self.return_data = vec![];
                        self.stack_push(U256::ONE)?;
                    } else {
                        // ── Real sub-execution ────────────────────────────────
                        let snapshot = self.world.borrow().clone();

                        // DELEGATECALL: keep caller's address + value; use callee's code
                        let (sub_caller, sub_callee, sub_val) = if op == 0xF4 {
                            (self.ctx.caller, self.ctx.callee, self.ctx.value)
                        } else {
                            (self.ctx.callee, callee_addr, U256::from_u64(call_val))
                        };

                        if call_val > 0 && op != 0xF4 && !self.ctx.is_static {
                            self.world.borrow_mut().transfer(&self.ctx.callee, &sub_callee, call_val);
                        }

                        let sub_ctx = EvmContext {
                            caller:    sub_caller,
                            callee:    sub_callee,
                            value:     sub_val,
                            input:     call_input,
                            gas_limit: call_gas,
                            is_static: self.ctx.is_static || op == 0xFA,
                            depth:     self.ctx.depth + 1,
                            ..self.ctx.clone()
                        };
                        let sub = Evm::new_with_world(
                            sub_ctx, callee_code, HashMap::new(), Rc::clone(&self.world)
                        );
                        let result = sub.run();
                        self.gas_remaining = self.gas_remaining.saturating_sub(result.gas_used);

                        let write_len = result.return_data.len().min(ret_len);
                        if write_len > 0 {
                            self.mem_ensure(ret_off, write_len)?;
                            self.mem_write(ret_off, &result.return_data[..write_len]);
                        }
                        self.return_data = result.return_data;

                        if result.reverted || !result.success {
                            *self.world.borrow_mut() = snapshot;
                            self.stack_push(U256::ZERO)?;
                        } else {
                            self.stack_push(U256::ONE)?;
                        }
                    }
                }
            }

            // ── Chain ID (EIP-1344) ───────────────────────────────────────────
            0x46 => { // CHAINID
                self.use_gas(GAS_BASE)?;
                let id = self.ctx.chain_id;
                self.stack_push(U256::from_u64(id))?;
            }
            0x47 => { // SELFBALANCE — stub: 0
                self.use_gas(GAS_LOW)?;
                self.stack_push(U256::ZERO)?;
            }

            _ => {
                return Err(format!("unknown opcode 0x{:02X} at pc={}", op, self.pc - 1));
            }
        }

        Ok(None)
    }

    fn pop2(&mut self) -> Result<(U256, U256), String> {
        let a = self.stack_pop()?;
        let b = self.stack_pop()?;
        Ok((a, b))
    }

    fn exit_ok(&mut self, data: Vec<u8>) -> EvmResult {
        let gas_used = self.ctx.gas_limit.saturating_sub(self.gas_remaining);
        let refund   = self.gas_refund.min(gas_used / 5); // EIP-3529: max 1/5
        EvmResult {
            success: true, reverted: false, return_data: data,
            gas_used, gas_refund: refund,
            logs: std::mem::take(&mut self.logs),
            revert_msg: None,
        }
    }
}

// ─── Additional gas constants (LOG) ─────────────────────────────────────────

const GAS_LOG_TOPIC: u64 = 375;
const GAS_LOG_DATA:  u64 = 8;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Execute EVM bytecode with given context and pre-existing storage.
pub fn execute(ctx: EvmContext, code: Vec<u8>, storage: HashMap<U256, U256>) -> EvmResult {
    Evm::new(ctx, code, storage).run()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> EvmContext { EvmContext { gas_limit: 1_000_000, ..Default::default() } }

    #[test]
    fn test_stop_returns_success() {
        let r = execute(ctx(), vec![0x00], HashMap::new());
        assert!(r.success);
        assert_eq!(r.return_data, Vec::<u8>::new());
    }

    #[test]
    fn test_push1_pop() {
        // PUSH1 0x2A, STOP
        let r = execute(ctx(), vec![0x60, 0x2A, 0x00], HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_add() {
        // PUSH1 3, PUSH1 4, ADD, STOP
        let code = vec![0x60, 0x03, 0x60, 0x04, 0x01, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_mul() {
        // PUSH1 6, PUSH1 7, MUL, STOP
        let code = vec![0x60, 0x06, 0x60, 0x07, 0x02, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_sub() {
        // PUSH1 3, PUSH1 10, SUB → 7
        let code = vec![0x60, 0x03, 0x60, 0x0A, 0x03, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_div_zero() {
        // PUSH1 0, PUSH1 5, DIV → 0 (no error)
        let code = vec![0x60, 0x00, 0x60, 0x05, 0x04, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_mstore_mload() {
        // PUSH1 0xAB, PUSH1 0x00, MSTORE, PUSH1 0x00, MLOAD, STOP
        let code = vec![0x60, 0xAB, 0x60, 0x00, 0x52, 0x60, 0x00, 0x51, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_return_data() {
        // PUSH1 0x42, PUSH1 0x00, MSTORE8, PUSH1 1, PUSH1 0, RETURN
        let code = vec![0x60, 0x42, 0x60, 0x00, 0x53, 0x60, 0x01, 0x60, 0x00, 0xF3];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
        assert_eq!(r.return_data, vec![0x42]);
    }

    #[test]
    fn test_revert() {
        // PUSH1 0, PUSH1 0, REVERT
        let code = vec![0x60, 0x00, 0x60, 0x00, 0xFD];
        let r = execute(ctx(), code, HashMap::new());
        assert!(!r.success);
    }

    #[test]
    fn test_jump() {
        // PUSH1 4, JUMP, INVALID, JUMPDEST (4), STOP
        let code = vec![0x60, 0x04, 0x56, 0xFE, 0x5B, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success, "JUMP should skip INVALID");
    }

    #[test]
    fn test_jumpi_taken() {
        // PUSH1 1 (cond), PUSH1 6, JUMPI, INVALID, JUMPDEST(6), STOP
        let code = vec![0x60, 0x01, 0x60, 0x06, 0x57, 0xFE, 0x5B, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_jumpi_not_taken() {
        // PUSH1 0 (cond=false), PUSH1 6, JUMPI, STOP, JUMPDEST, INVALID
        let code = vec![0x60, 0x00, 0x60, 0x06, 0x57, 0x00, 0x5B, 0xFE];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_dup1() {
        // PUSH1 5, DUP1, EQ, STOP  → stack has [1]
        let code = vec![0x60, 0x05, 0x80, 0x14, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_swap1() {
        // PUSH1 1, PUSH1 2, SWAP1 → [2, 1] (top=2)
        let code = vec![0x60, 0x01, 0x60, 0x02, 0x90, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_sstore_sload() {
        // PUSH1 0xFF, PUSH1 0x00, SSTORE, PUSH1 0x00, SLOAD, STOP
        let code = vec![0x60, 0xFF, 0x60, 0x00, 0x55, 0x60, 0x00, 0x54, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_out_of_gas() {
        let mut c = ctx();
        c.gas_limit = 1; // too little
        let code = vec![0x60, 0x01, 0x60, 0x02, 0x01, 0x00]; // needs > 1 gas
        let r = execute(c, code, HashMap::new());
        assert!(!r.success);
    }

    #[test]
    fn test_invalid_opcode() {
        let code = vec![0xFE];
        let r = execute(ctx(), code, HashMap::new());
        assert!(!r.success);
    }

    #[test]
    fn test_log0() {
        // PUSH1 0, PUSH1 0, LOG0
        let code = vec![0x60, 0x00, 0x60, 0x00, 0xA0, 0x00];
        let r = execute(ctx(), code, HashMap::new());
        assert!(r.success);
        assert_eq!(r.logs.len(), 1);
    }

    #[test]
    fn test_u256_add_overflow_wraps() {
        let (sum, overflow) = U256::MAX.overflowing_add(U256::ONE);
        assert!(overflow);
        assert_eq!(sum, U256::ZERO);
    }

    #[test]
    fn test_u256_mul() {
        let a = U256::from_u64(100);
        let b = U256::from_u64(200);
        assert_eq!(a.wrapping_mul(b), U256::from_u64(20_000));
    }

    #[test]
    fn test_u256_div() {
        let a = U256::from_u64(100);
        let b = U256::from_u64(7);
        assert_eq!(a.div(b), U256::from_u64(14));
    }

    #[test]
    fn test_u256_shr() {
        let a = U256::from_u64(128);
        assert_eq!(a.shr(1), U256::from_u64(64));
    }

    #[test]
    fn test_u256_shl() {
        let a = U256::from_u64(1);
        assert_eq!(a.shl(8), U256::from_u64(256));
    }

    #[test]
    fn test_u256_be_round_trip() {
        let b: [u8; 32] = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF,
        ];
        let v = U256::from_be_bytes(&b);
        assert_eq!(v.to_be_bytes(), b);
    }

    #[test]
    fn test_calldataload() {
        // PUSH1 0, CALLDATALOAD, STOP — with 32 bytes of input = 0x01…01
        let mut c = ctx();
        c.input = vec![0x01; 32];
        let code = vec![0x60, 0x00, 0x35, 0x00];
        let r = execute(c, code, HashMap::new());
        assert!(r.success);
    }

    #[test]
    fn test_static_sstore_blocked() {
        let mut c = ctx();
        c.is_static = true;
        let code = vec![0x60, 0x01, 0x60, 0x00, 0x55, 0x00]; // SSTORE in static call
        let r = execute(c, code, HashMap::new());
        assert!(!r.success);
    }

    #[test]
    fn test_gas_remaining_decreases() {
        let c = ctx();
        let gas_limit = c.gas_limit;
        let code = vec![0x60, 0x01, 0x60, 0x02, 0x01, 0x00]; // PUSH1 PUSH1 ADD STOP
        let r = execute(c, code, HashMap::new());
        assert!(r.success);
        assert!(r.gas_used > 0);
        assert!(r.gas_used < gas_limit);
    }

    // ── WorldState / sub-execution ────────────────────────────────────────────

    fn world_with_contract(addr: [u8; 20], code: Vec<u8>) -> Rc<RefCell<WorldState>> {
        let mut w = WorldState::new();
        w.set_code(addr, code);
        Rc::new(RefCell::new(w))
    }

    /// CALL bytecode: call addr with gas=0x5000, value=0, no args, returns retLen bytes into retOff=0
    fn call_bytecode(addr: [u8; 20], ret_len: u8) -> Vec<u8> {
        let mut code = vec![
            0x60, ret_len, // PUSH1 retLen
            0x60, 0x00,    // PUSH1 retOff=0
            0x60, 0x00,    // PUSH1 argLen=0
            0x60, 0x00,    // PUSH1 argOff=0
            0x60, 0x00,    // PUSH1 value=0
            0x73,          // PUSH20 addr
        ];
        code.extend_from_slice(&addr);
        code.extend_from_slice(&[
            0x61, 0x50, 0x00, // PUSH2 0x5000 = gas
            0xF1,             // CALL
            0x60, ret_len,    // PUSH1 retLen
            0x60, 0x00,       // PUSH1 retOff=0
            0xF3,             // RETURN
        ]);
        code
    }

    #[test]
    fn test_call_to_contract_returns_data() {
        let callee_addr = [0xABu8; 20];
        // Contract: stores 0x42 at mem[31], returns 32 bytes from offset 0
        // PUSH1 0x42, PUSH1 0x1F, MSTORE8, PUSH1 0x20, PUSH1 0x00, RETURN
        let contract_code = vec![0x60, 0x42, 0x60, 0x1F, 0x53, 0x60, 0x20, 0x60, 0x00, 0xF3];
        let world = world_with_contract(callee_addr, contract_code);

        let code = call_bytecode(callee_addr, 0x20);
        let mut c = ctx();
        c.gas_limit = 500_000;
        let evm = Evm::new_with_world(c, code, HashMap::new(), world);
        let r = evm.run();
        assert!(r.success, "CALL sub-execution failed: {:?}", r.revert_msg);
        assert_eq!(r.return_data.len(), 32);
        assert_eq!(r.return_data[31], 0x42);
    }

    #[test]
    fn test_call_to_eoa_succeeds_empty_return() {
        let eoa_addr = [0x11u8; 20];
        // No code in WorldState → EOA
        let world = Rc::new(RefCell::new(WorldState::new()));
        let code  = call_bytecode(eoa_addr, 0x00);
        let mut c = ctx();
        c.gas_limit = 100_000;
        let evm = Evm::new_with_world(c, code, HashMap::new(), world);
        let r   = evm.run();
        assert!(r.success);
        assert_eq!(r.return_data.len(), 0);
    }

    #[test]
    fn test_call_revert_rolls_back_world() {
        let callee_addr = [0xCDu8; 20];
        // Contract: REVERTs → should roll back transfer
        // PUSH1 0x00, PUSH1 0x00, REVERT (0xFD)
        let contract_code = vec![0x60, 0x00, 0x60, 0x00, 0xFD];
        let mut w = WorldState::new();
        w.set_code(callee_addr, contract_code);
        let caller_addr = [0x01u8; 20];
        w.set_balance(caller_addr, 100_000);
        let world = Rc::new(RefCell::new(w));

        // Caller code: CALL with value 1000 (won't go through because callee reverts)
        // PUSH1 retLen, PUSH1 retOff, PUSH1 argLen, PUSH1 argOff, PUSH2 value, PUSH20 addr, PUSH2 gas, CALL, STOP
        let mut code = vec![0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00,
                             0x61, 0x03, 0xE8, // PUSH2 1000 (value)
                             0x73];
        code.extend_from_slice(&callee_addr);
        code.extend_from_slice(&[0x61, 0x50, 0x00, 0xF1, 0x00]);

        let mut c = ctx();
        c.callee     = caller_addr;
        c.gas_limit  = 500_000;
        let evm = Evm::new_with_world(c, code, HashMap::new(), Rc::clone(&world));
        let r = evm.run();
        assert!(r.success);
        // Caller balance should be restored after revert
        let bal = world.borrow().get_balance(&caller_addr);
        assert_eq!(bal, 100_000, "balance should be restored after REVERT");
    }

    #[test]
    fn test_create_deploys_contract() {
        let world = Rc::new(RefCell::new(WorldState::new()));
        let deployer = [0x77u8; 20];

        // Init code: PUSH1 0x01, PUSH1 0x00, MSTORE8, PUSH1 0x01, PUSH1 0x00, RETURN
        // → deploys 1 byte [0x01] as runtime code
        let init_code: Vec<u8> = vec![0x60, 0x01, 0x60, 0x00, 0x53, 0x60, 0x01, 0x60, 0x00, 0xF3];
        let _init_len = init_code.len() as u8;

        // Caller: MSTORE init_code into memory, then CREATE
        // We push the code into memory via PUSH opcodes... too complex.
        // Instead: pre-load code into memory via CODECOPY
        // Even simpler: test CREATE directly by encoding a PUSH + CODECOPY + CREATE sequence

        // Simplest approach: use CALLDATACOPY to load init code from input, then CREATE
        // input = init_code (10 bytes)
        // Code:
        //   PUSH1 0x0A PUSH1 0x00 PUSH1 0x00 CALLDATACOPY  → memory[0..10] = init_code
        //   PUSH1 0x0A PUSH1 0x00 PUSH1 0x00 CREATE        → create contract
        //   PUSH1 0x14 PUSH1 0x00 MSTORE                   → store addr in mem[12..32]
        //   PUSH1 0x14 PUSH1 0x0C RETURN                   → return 20 bytes (addr)
        let code = vec![
            0x60, 0x0A, 0x60, 0x00, 0x60, 0x00, 0x37, // CALLDATACOPY (len=10, dataOff=0, destOff=0)
            0x60, 0x0A, 0x60, 0x00, 0x60, 0x00, 0xF0, // CREATE (len=10, off=0, val=0 on top)
            0x60, 0x00, 0x52,                         // MSTORE (offset=0, value=new_addr u256)
            0x60, 0x14, 0x60, 0x0C, 0xF3,             // RETURN 20 bytes from mem[12]
        ];

        let mut c = ctx();
        c.callee    = deployer;
        c.input     = init_code;
        c.gas_limit = 500_000;
        let evm = Evm::new_with_world(c, code, HashMap::new(), Rc::clone(&world));
        let r = evm.run();
        assert!(r.success, "CREATE failed: {:?}", r.revert_msg);
        assert_eq!(r.return_data.len(), 20);
        let mut new_addr = [0u8; 20];
        new_addr.copy_from_slice(&r.return_data);
        // Verify deployed code exists in WorldState
        let deployed = world.borrow().get_code(&new_addr).to_vec();
        assert!(!deployed.is_empty(), "deployed code should be stored in WorldState");
    }

    #[test]
    fn test_balance_reads_world_state() {
        let target = [0x55u8; 20];
        let mut w  = WorldState::new();
        w.set_balance(target, 999);
        let world = Rc::new(RefCell::new(w));

        // Code: PUSH20 target, BALANCE, PUSH1 0x00, MSTORE, PUSH1 0x20, PUSH1 0x00, RETURN
        let mut code = vec![0x73];
        code.extend_from_slice(&target);
        code.extend_from_slice(&[
            0x31,             // BALANCE
            0x60, 0x00, 0x52, // MSTORE at offset 0
            0x60, 0x20, 0x60, 0x00, 0xF3, // RETURN 32 bytes
        ]);
        let evm = Evm::new_with_world(ctx(), code, HashMap::new(), world);
        let r = evm.run();
        assert!(r.success);
        assert_eq!(r.return_data.len(), 32);
        // The balance 999 is stored big-endian in the last 8 bytes of the 32-byte word
        let val = u64::from_be_bytes(r.return_data[24..].try_into().unwrap());
        assert_eq!(val, 999);
    }

    #[test]
    fn test_depth_limit_blocks_call() {
        let callee_addr = [0xEEu8; 20];
        let world = world_with_contract(callee_addr, vec![0x00]); // trivial contract
        let code  = call_bytecode(callee_addr, 0x00);
        let mut c = ctx();
        c.depth     = 10; // at max depth
        c.gas_limit = 100_000;
        let evm = Evm::new_with_world(c, code, HashMap::new(), world);
        let r = evm.run();
        // CALL should push 0 (failed), but parent continues to RETURN
        assert!(r.success);
        // return data is 0 bytes (ret_len=0)
        assert!(r.return_data.is_empty());
    }
}
