#![allow(dead_code)]
//! v7.4 — Smart Contract State

use std::collections::HashMap;

pub type Address = String;
pub type StorageKey = [u8; 32];
pub type StorageVal = [u8; 32];

#[derive(Debug, Clone)]
pub struct ContractState {
    pub address: Address,
    pub code_hash: String,
    pub storage: HashMap<StorageKey, StorageVal>,
    pub balance: u64,
    pub nonce: u64,
}

impl ContractState {
    pub fn new(address: &str, code_hash: &str) -> Self {
        ContractState {
            address: address.to_string(),
            code_hash: code_hash.to_string(),
            storage: HashMap::new(),
            balance: 0,
            nonce: 0,
        }
    }

    pub fn set_storage(&mut self, key: StorageKey, val: StorageVal) {
        self.storage.insert(key, val);
    }

    pub fn get_storage(&self, key: &StorageKey) -> StorageVal {
        *self.storage.get(key).unwrap_or(&[0u8; 32])
    }

    pub fn storage_root(&self) -> String {
        let mut pairs: Vec<(&StorageKey, &StorageVal)> = self.storage.iter().collect();
        pairs.sort_by_key(|(k, _)| *k);
        let mut data = Vec::new();
        for (k, v) in pairs {
            data.extend_from_slice(k);
            data.extend_from_slice(v);
        }
        hex::encode(blake3::hash(&data).as_bytes())
    }
}

#[derive(Debug, Clone)]
pub struct ContractStore {
    pub contracts: HashMap<Address, ContractState>,
}

impl ContractStore {
    pub fn new() -> Self {
        ContractStore {
            contracts: HashMap::new(),
        }
    }

    pub fn deploy(&mut self, address: &str, code_hash: &str, initial_balance: u64) -> Result<(), String> {
        if self.contracts.contains_key(address) {
            return Err(format!("Contract at {} already deployed", address));
        }
        let mut state = ContractState::new(address, code_hash);
        state.balance = initial_balance;
        self.contracts.insert(address.to_string(), state);
        Ok(())
    }

    pub fn get(&self, address: &str) -> Option<&ContractState> {
        self.contracts.get(address)
    }

    pub fn get_mut(&mut self, address: &str) -> Option<&mut ContractState> {
        self.contracts.get_mut(address)
    }

    pub fn state_root(&self) -> String {
        let mut pairs: Vec<(&Address, String)> = self
            .contracts
            .iter()
            .map(|(addr, state)| (addr, state.storage_root()))
            .collect();
        pairs.sort_by_key(|(addr, _)| (*addr).clone());
        let mut data = Vec::new();
        for (addr, root) in &pairs {
            data.extend_from_slice(addr.as_bytes());
            data.extend_from_slice(root.as_bytes());
        }
        hex::encode(blake3::hash(&data).as_bytes())
    }

    pub fn transfer_value(&mut self, from: &str, to: &str, amount: u64) -> Result<(), String> {
        {
            let from_state = self.contracts.get(from)
                .ok_or_else(|| format!("Contract {} not found", from))?;
            if from_state.balance < amount {
                return Err(format!("Insufficient balance: {} < {}", from_state.balance, amount));
            }
        }
        if self.contracts.get(to).is_none() {
            return Err(format!("Contract {} not found", to));
        }
        self.contracts.get_mut(from).unwrap().balance -= amount;
        self.contracts.get_mut(to).unwrap().balance += amount;
        Ok(())
    }

    pub fn snapshot(&self) -> HashMap<Address, ContractState> {
        self.contracts.clone()
    }

    pub fn restore(&mut self, snap: HashMap<Address, ContractState>) {
        self.contracts = snap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deploy() {
        let mut store = ContractStore::new();
        store.deploy("0xabc", "codehash1", 1000).unwrap();
        let state = store.get("0xabc").unwrap();
        assert_eq!(state.balance, 1000);
        assert_eq!(state.code_hash, "codehash1");
    }

    #[test]
    fn test_deploy_duplicate_fails() {
        let mut store = ContractStore::new();
        store.deploy("0xabc", "hash1", 0).unwrap();
        let r = store.deploy("0xabc", "hash2", 0);
        assert!(r.is_err());
    }

    #[test]
    fn test_set_get_storage() {
        let mut state = ContractState::new("0xabc", "hash");
        let key = [1u8; 32];
        let val = [2u8; 32];
        state.set_storage(key, val);
        assert_eq!(state.get_storage(&key), val);
    }

    #[test]
    fn test_get_storage_default() {
        let state = ContractState::new("0xabc", "hash");
        let key = [99u8; 32];
        assert_eq!(state.get_storage(&key), [0u8; 32]);
    }

    #[test]
    fn test_storage_root_deterministic() {
        let mut s1 = ContractState::new("0x1", "hash");
        let mut s2 = ContractState::new("0x1", "hash");
        let k1 = [1u8; 32];
        let v1 = [10u8; 32];
        let k2 = [2u8; 32];
        let v2 = [20u8; 32];
        s1.set_storage(k1, v1);
        s1.set_storage(k2, v2);
        s2.set_storage(k2, v2);
        s2.set_storage(k1, v1);
        assert_eq!(s1.storage_root(), s2.storage_root());
    }

    #[test]
    fn test_state_root() {
        let mut store = ContractStore::new();
        store.deploy("0x1", "hash1", 100).unwrap();
        store.deploy("0x2", "hash2", 200).unwrap();
        let root1 = store.state_root();
        let root2 = store.state_root();
        assert_eq!(root1, root2);
    }

    #[test]
    fn test_transfer_value() {
        let mut store = ContractStore::new();
        store.deploy("0x1", "h1", 1000).unwrap();
        store.deploy("0x2", "h2", 0).unwrap();
        store.transfer_value("0x1", "0x2", 300).unwrap();
        assert_eq!(store.get("0x1").unwrap().balance, 700);
        assert_eq!(store.get("0x2").unwrap().balance, 300);
    }

    #[test]
    fn test_snapshot_restore() {
        let mut store = ContractStore::new();
        store.deploy("0x1", "h1", 500).unwrap();
        let snap = store.snapshot();
        store.get_mut("0x1").unwrap().balance = 0;
        assert_eq!(store.get("0x1").unwrap().balance, 0);
        store.restore(snap);
        assert_eq!(store.get("0x1").unwrap().balance, 500);
    }
}
