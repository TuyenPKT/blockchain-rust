#![allow(dead_code)]
//! v27.1 — EVM World State
//!
//! Shared mutable state across CALL depth:
//!   - Code storage (contract bytecode by address)
//!   - Account balances
//!   - Contract storage (per-address key→value mapping)
//!   - Account nonces (for CREATE address derivation)

use std::collections::HashMap;

pub type Addr        = [u8; 20];
pub type StorageKey  = [u8; 32];
pub type StorageVal  = [u8; 32];

#[derive(Debug, Clone, Default)]
pub struct WorldState {
    pub codes:    HashMap<Addr, Vec<u8>>,
    pub balances: HashMap<Addr, u64>,
    pub storage:  HashMap<Addr, HashMap<StorageKey, StorageVal>>,
    pub nonces:   HashMap<Addr, u64>,
}

impl WorldState {
    pub fn new() -> Self { Self::default() }

    pub fn get_code(&self, addr: &Addr) -> &[u8] {
        self.codes.get(addr).map(|v| v.as_slice()).unwrap_or(&[])
    }
    pub fn set_code(&mut self, addr: Addr, code: Vec<u8>) {
        self.codes.insert(addr, code);
    }

    pub fn get_balance(&self, addr: &Addr) -> u64 {
        self.balances.get(addr).copied().unwrap_or(0)
    }
    pub fn set_balance(&mut self, addr: Addr, bal: u64) {
        self.balances.insert(addr, bal);
    }
    pub fn transfer(&mut self, from: &Addr, to: &Addr, amount: u64) -> bool {
        let bal = self.get_balance(from);
        if bal < amount { return false; }
        self.balances.insert(*from, bal - amount);
        let to_bal = self.get_balance(to);
        self.balances.insert(*to, to_bal + amount);
        true
    }

    pub fn get_storage(&self, addr: &Addr, key: &StorageKey) -> StorageVal {
        self.storage.get(addr).and_then(|m| m.get(key)).copied().unwrap_or([0; 32])
    }
    pub fn set_storage(&mut self, addr: Addr, key: StorageKey, val: StorageVal) {
        self.storage.entry(addr).or_default().insert(key, val);
    }

    pub fn get_nonce(&self, addr: &Addr) -> u64 {
        self.nonces.get(addr).copied().unwrap_or(0)
    }
    pub fn inc_nonce(&mut self, addr: &Addr) -> u64 {
        let n = self.nonces.entry(*addr).or_insert(0);
        let old = *n;
        *n += 1;
        old
    }

    /// CREATE address: SHA256(deployer || nonce) → last 20 bytes (simplified keccak)
    pub fn create_address(deployer: &Addr, nonce: u64) -> Addr {
        use sha2::{Sha256, Digest};
        let mut data = deployer.to_vec();
        data.extend_from_slice(&nonce.to_le_bytes());
        let hash = Sha256::digest(&data);
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hash[12..]);
        addr
    }

    /// CREATE2 address: SHA256(0xFF || deployer || salt || code_hash) → last 20 bytes
    pub fn create2_address(deployer: &Addr, salt: &[u8; 32], code_hash: &[u8; 32]) -> Addr {
        use sha2::{Sha256, Digest};
        let mut data = vec![0xFF];
        data.extend_from_slice(deployer);
        data.extend_from_slice(salt);
        data.extend_from_slice(code_hash);
        let hash = Sha256::digest(&data);
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hash[12..]);
        addr
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_set_code() {
        let mut w = WorldState::new();
        let addr  = [0x01u8; 20];
        assert!(w.get_code(&addr).is_empty());
        w.set_code(addr, vec![0x60, 0x00, 0x00]);
        assert_eq!(w.get_code(&addr), &[0x60, 0x00, 0x00]);
    }

    #[test]
    fn test_get_set_balance() {
        let mut w = WorldState::new();
        let addr  = [0x02u8; 20];
        assert_eq!(w.get_balance(&addr), 0);
        w.set_balance(addr, 1_000_000);
        assert_eq!(w.get_balance(&addr), 1_000_000);
    }

    #[test]
    fn test_transfer_success() {
        let mut w    = WorldState::new();
        let alice    = [0x01u8; 20];
        let bob      = [0x02u8; 20];
        w.set_balance(alice, 1_000);
        assert!(w.transfer(&alice, &bob, 400));
        assert_eq!(w.get_balance(&alice), 600);
        assert_eq!(w.get_balance(&bob),   400);
    }

    #[test]
    fn test_transfer_insufficient_funds() {
        let mut w = WorldState::new();
        let alice = [0x01u8; 20];
        let bob   = [0x02u8; 20];
        w.set_balance(alice, 100);
        assert!(!w.transfer(&alice, &bob, 200));
        assert_eq!(w.get_balance(&alice), 100);
        assert_eq!(w.get_balance(&bob),   0);
    }

    #[test]
    fn test_get_set_storage() {
        let mut w  = WorldState::new();
        let addr   = [0x03u8; 20];
        let key    = [0x01u8; 32];
        let val    = [0xFFu8; 32];
        assert_eq!(w.get_storage(&addr, &key), [0u8; 32]);
        w.set_storage(addr, key, val);
        assert_eq!(w.get_storage(&addr, &key), val);
    }

    #[test]
    fn test_inc_nonce() {
        let mut w = WorldState::new();
        let addr  = [0x04u8; 20];
        assert_eq!(w.inc_nonce(&addr), 0);
        assert_eq!(w.inc_nonce(&addr), 1);
        assert_eq!(w.get_nonce(&addr), 2);
    }

    #[test]
    fn test_create_address_deterministic() {
        let deployer = [0x11u8; 20];
        let a1 = WorldState::create_address(&deployer, 0);
        let a2 = WorldState::create_address(&deployer, 0);
        assert_eq!(a1, a2);
    }

    #[test]
    fn test_create_address_differs_by_nonce() {
        let deployer = [0x11u8; 20];
        let a0 = WorldState::create_address(&deployer, 0);
        let a1 = WorldState::create_address(&deployer, 1);
        assert_ne!(a0, a1);
    }

    #[test]
    fn test_create2_address_deterministic() {
        let deployer = [0x22u8; 20];
        let salt     = [0xAAu8; 32];
        let code_hash = [0xBBu8; 32];
        let a1 = WorldState::create2_address(&deployer, &salt, &code_hash);
        let a2 = WorldState::create2_address(&deployer, &salt, &code_hash);
        assert_eq!(a1, a2);
    }

    #[test]
    fn test_create2_differs_by_salt() {
        let deployer  = [0x22u8; 20];
        let code_hash = [0xBBu8; 32];
        let a1 = WorldState::create2_address(&deployer, &[0u8; 32], &code_hash);
        let a2 = WorldState::create2_address(&deployer, &[1u8; 32], &code_hash);
        assert_ne!(a1, a2);
    }

    #[test]
    fn test_world_state_clone() {
        let mut w1 = WorldState::new();
        let addr   = [0x05u8; 20];
        w1.set_balance(addr, 999);
        let w2 = w1.clone();
        assert_eq!(w2.get_balance(&addr), 999);
    }
}
