#![allow(dead_code)]
//! v7.6 — Contract Deployment

use std::collections::HashMap;
use crate::contract_state::ContractStore;
use crate::evm_lite::{EvmLiteContext, EvmLiteOp, EvmLiteResult, EvmLiteVm};

#[derive(Debug, Clone)]
pub struct DeployParams {
    pub bytecode: Vec<u8>,
    pub constructor_args: Vec<u8>,
    pub value: u64,
    pub gas_limit: u64,
    pub deployer: String,
    pub nonce: u64,
}

#[derive(Debug, Clone)]
pub struct DeployedContract {
    pub address: String,
    pub code_hash: String,
    pub deployer: String,
    pub block_height: u64,
    pub deploy_tx_id: String,
}

pub struct ContractAddress;

impl ContractAddress {
    pub fn create(deployer: &str, nonce: u64) -> String {
        let data = format!("{}:{}", deployer, nonce);
        let hash = blake3::hash(data.as_bytes());
        hex::encode(&hash.as_bytes()[..20])
    }

    pub fn create2(deployer: &str, salt: &[u8], code_hash: &str) -> String {
        let mut data = Vec::new();
        data.extend_from_slice(b"CREATE2:");
        data.extend_from_slice(deployer.as_bytes());
        data.extend_from_slice(salt);
        data.extend_from_slice(code_hash.as_bytes());
        let hash = blake3::hash(&data);
        hex::encode(&hash.as_bytes()[..20])
    }
}

pub struct ContractDeployer {
    pub store: ContractStore,
    pub deployed: Vec<DeployedContract>,
}

impl ContractDeployer {
    pub fn new() -> Self {
        ContractDeployer {
            store: ContractStore::new(),
            deployed: Vec::new(),
        }
    }

    pub fn deploy(&mut self, params: &DeployParams, block_height: u64) -> Result<DeployedContract, String> {
        let address = ContractAddress::create(&params.deployer, params.nonce);
        let code_hash = hex::encode(blake3::hash(&params.bytecode).as_bytes());

        // Compute deploy_tx_id
        let tx_data = format!("deploy:{}:{}:{}", address, code_hash, block_height);
        let deploy_tx_id = hex::encode(blake3::hash(tx_data.as_bytes()).as_bytes());

        self.store.deploy(&address, &code_hash, params.value)?;

        let deployed = DeployedContract {
            address: address.clone(),
            code_hash,
            deployer: params.deployer.clone(),
            block_height,
            deploy_tx_id,
        };
        self.deployed.push(deployed.clone());
        Ok(deployed)
    }

    pub fn call(
        &mut self,
        address: &str,
        caller: &str,
        input: Vec<u8>,
        value: u64,
        gas: u64,
        block_height: u64,
    ) -> EvmLiteResult {
        let ctx = EvmLiteContext {
            caller: caller.to_string(),
            callee: address.to_string(),
            value,
            gas_limit: gas,
            block_height,
            input: input.clone(),
        };
        let storage: HashMap<[u8; 32], [u8; 32]> = HashMap::new();
        let mut vm = EvmLiteVm::new(ctx, storage);
        let ops = vec![
            EvmLiteOp::Push(input),
            EvmLiteOp::Stop,
        ];
        vm.execute(&ops)
    }

    pub fn get_deployed(&self, address: &str) -> Option<&DeployedContract> {
        self.deployed.iter().find(|d| d.address == address)
    }
}

pub struct AbiEncoder;

impl AbiEncoder {
    pub fn encode_u256(v: u64) -> Vec<u8> {
        let mut bytes = vec![0u8; 32];
        let vb = v.to_be_bytes();
        bytes[24..].copy_from_slice(&vb);
        bytes
    }

    pub fn encode_address(addr: &str) -> Vec<u8> {
        // 32 bytes: 12 zero bytes + 20 address bytes
        let mut bytes = vec![0u8; 32];
        // Decode hex if possible, otherwise use raw bytes
        let addr_bytes = if addr.len() == 40 {
            hex::decode(addr).unwrap_or_else(|_| addr.as_bytes()[..20.min(addr.len())].to_vec())
        } else {
            let raw = addr.as_bytes();
            raw[..20.min(raw.len())].to_vec()
        };
        let start = 32 - addr_bytes.len().min(20);
        bytes[start..start + addr_bytes.len().min(20)].copy_from_slice(&addr_bytes[..addr_bytes.len().min(20)]);
        bytes
    }

    pub fn decode_u256(data: &[u8]) -> u64 {
        if data.len() < 8 {
            return 0;
        }
        let start = if data.len() >= 32 { data.len() - 8 } else { 0 };
        let slice = &data[start..start + 8.min(data.len() - start)];
        let mut arr = [0u8; 8];
        let copy_start = 8 - slice.len();
        arr[copy_start..].copy_from_slice(slice);
        u64::from_be_bytes(arr)
    }

    pub fn encode_call(selector: &[u8; 4], args: &[Vec<u8>]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(selector);
        for arg in args {
            data.extend_from_slice(arg);
        }
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_address_create_deterministic() {
        let a1 = ContractAddress::create("alice", 0);
        let a2 = ContractAddress::create("alice", 0);
        assert_eq!(a1, a2);
        assert_eq!(a1.len(), 40); // 20 bytes = 40 hex chars
    }

    #[test]
    fn test_contract_address_create_unique() {
        let a0 = ContractAddress::create("alice", 0);
        let a1 = ContractAddress::create("alice", 1);
        assert_ne!(a0, a1);
    }

    #[test]
    fn test_create2_deterministic() {
        let salt = b"mysalt";
        let a1 = ContractAddress::create2("alice", salt, "codehash");
        let a2 = ContractAddress::create2("alice", salt, "codehash");
        assert_eq!(a1, a2);
        assert_eq!(a1.len(), 40);
    }

    #[test]
    fn test_create2_different_salt() {
        let a1 = ContractAddress::create2("alice", b"salt1", "code");
        let a2 = ContractAddress::create2("alice", b"salt2", "code");
        assert_ne!(a1, a2);
    }

    #[test]
    fn test_deploy() {
        let mut deployer = ContractDeployer::new();
        let params = DeployParams {
            bytecode: b"contract code".to_vec(),
            constructor_args: vec![],
            value: 1000,
            gas_limit: 100_000,
            deployer: "alice".to_string(),
            nonce: 0,
        };
        let deployed = deployer.deploy(&params, 1).unwrap();
        assert_eq!(deployed.deployer, "alice");
        assert_eq!(deployed.block_height, 1);
        assert!(!deployed.address.is_empty());
        assert!(!deployed.code_hash.is_empty());
    }

    #[test]
    fn test_abi_encode_u256() {
        let encoded = AbiEncoder::encode_u256(255);
        assert_eq!(encoded.len(), 32);
        assert_eq!(encoded[31], 255);
        assert_eq!(encoded[30], 0);
    }

    #[test]
    fn test_abi_decode_u256() {
        let encoded = AbiEncoder::encode_u256(12345);
        let decoded = AbiEncoder::decode_u256(&encoded);
        assert_eq!(decoded, 12345);
    }

    #[test]
    fn test_encode_call_length() {
        let selector = [0xab, 0xcd, 0xef, 0x12];
        let arg1 = AbiEncoder::encode_u256(1);
        let arg2 = AbiEncoder::encode_u256(2);
        let call = AbiEncoder::encode_call(&selector, &[arg1, arg2]);
        // 4 + 32 + 32 = 68 bytes
        assert_eq!(call.len(), 68);
        assert_eq!(&call[..4], &selector);
    }

    #[test]
    fn test_get_deployed() {
        let mut deployer = ContractDeployer::new();
        let params = DeployParams {
            bytecode: vec![1, 2, 3],
            constructor_args: vec![],
            value: 0,
            gas_limit: 100_000,
            deployer: "bob".to_string(),
            nonce: 5,
        };
        let d = deployer.deploy(&params, 10).unwrap();
        let addr = d.address.clone();
        let found = deployer.get_deployed(&addr);
        assert!(found.is_some());
        assert_eq!(found.unwrap().deployer, "bob");
    }
}
