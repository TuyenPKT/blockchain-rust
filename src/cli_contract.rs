#![allow(dead_code)]
//! v11.7 — CLI Contract
//!
//! Deploy và gọi smart contracts từ command line.
//! Dữ liệu lưu trong RocksDB (`~/.pkt/db/`, key: `contract_registry:snapshot`).
//!
//! Cách dùng:
//!   cargo run -- contract deploy  <template>              deploy built-in template
//!   cargo run -- contract list                            liệt kê contracts đã deploy
//!   cargo run -- contract info    <address>               thông tin contract
//!   cargo run -- contract call    <address> <fn> [args…]  gọi function
//!   cargo run -- contract state   <address> [key]         xem storage state
//!   cargo run -- contract estimate <address> <fn>         ước tính gas
//!
//! Templates hỗ trợ: counter | token | voting
//! Gas limit mặc định: 100_000 per call

use std::collections::HashMap;

use crate::smart_contract::{ContractRegistry, counter_contract, token_contract, voting_contract};
use crate::storage::{load_contract_registry, save_contract_registry};
use crate::write_api::estimate_gas_for;

const DEFAULT_GAS_LIMIT: u64 = 100_000;

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn cmd_contract(args: &[String]) {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("help");
    match sub {
        "deploy"   => cmd_deploy(args),
        "list"     => cmd_list(),
        "info"     => cmd_info(args),
        "call"     => cmd_call(args),
        "state"    => cmd_state(args),
        "estimate" => cmd_estimate(args),
        _          => print_help(),
    }
}

// ─── Sub-commands ─────────────────────────────────────────────────────────────

/// cargo run -- contract deploy <template> [creator]
fn cmd_deploy(args: &[String]) {
    let template = require_arg(args, 3, "template");
    let creator  = args.get(4).cloned().unwrap_or_else(|| "cli-user".to_string());

    let module = match template_to_module(&template) {
        Ok(m)  => m,
        Err(e) => die(&e),
    };

    let (mut reg, mut tmap) = load_contract_registry();
    let address = reg.deploy(module, &creator);
    tmap.insert(address.clone(), template.clone());

    save_or_die(&reg, &tmap);

    println!();
    println!("  Contract deployed:");
    println!("  Address  : {}", address);
    println!("  Template : {}", template);
    println!("  Creator  : {}", creator);
    println!();
    if let Some(inst) = reg.contracts.get(&address) {
        let exports: Vec<_> = inst.module.exports.iter().collect();
        println!("  Exported functions: {}", exports.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }
    println!();
    curl_hint_deploy(&template, &creator);
}

/// cargo run -- contract list
fn cmd_list() {
    let (reg, tmap) = load_contract_registry();
    if reg.contracts.is_empty() {
        println!("  No contracts deployed. Use: cargo run -- contract deploy <template>");
        return;
    }
    println!();
    println!("  Deployed contracts ({}):", reg.contracts.len());
    println!("  {:<44} {:<10} {:<10} {}", "Address", "Template", "Calls", "Gas Used");
    println!("  {}", "-".repeat(80));
    let mut addrs: Vec<_> = reg.contracts.keys().collect();
    addrs.sort();
    for addr in addrs {
        let inst     = &reg.contracts[addr];
        let template = tmap.get(addr).map(|s| s.as_str()).unwrap_or("?");
        println!("  {:<44} {:<10} {:<10} {}",
            addr, template, inst.call_count, inst.total_gas);
    }
    println!();
}

/// cargo run -- contract info <address>
fn cmd_info(args: &[String]) {
    let address         = require_arg(args, 3, "address");
    let (reg, tmap)     = load_contract_registry();
    match reg.contracts.get(&address) {
        None       => die(&format!("Contract '{}' not found", address)),
        Some(inst) => {
            let template = tmap.get(&address).map(|s| s.as_str()).unwrap_or("unknown");
            println!();
            println!("  Contract: {}", address);
            println!("  Template    : {}", template);
            println!("  Creator     : {}", inst.creator);
            println!("  Deploy block: {}", inst.deploy_block);
            println!("  Call count  : {}", inst.call_count);
            println!("  Total gas   : {}", inst.total_gas);
            println!("  Storage root: {}", inst.storage.storage_root());
            println!("  Exports     : {}", inst.module.exports.join(", "));
            println!();
        }
    }
}

/// cargo run -- contract call <address> <function> [arg1 arg2 …] [--gas N]
fn cmd_call(args: &[String]) {
    let address  = require_arg(args, 3, "address");
    let fn_name  = require_arg(args, 4, "function");

    // Parse args — stop at --gas flag
    let mut call_args: Vec<i64> = Vec::new();
    let mut gas_limit = DEFAULT_GAS_LIMIT;
    let mut i = 5usize;
    while i < args.len() {
        if args[i] == "--gas" {
            gas_limit = args.get(i + 1)
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| die("--gas requires a number"));
            i += 2;
        } else {
            match args[i].parse::<i64>() {
                Ok(v)  => { call_args.push(v); i += 1; }
                Err(_) => die(&format!("invalid argument '{}': expected i64", args[i])),
            }
        }
    }

    let (mut reg, tmap) = load_contract_registry();
    match reg.call(&address, &fn_name, call_args.clone(), gas_limit) {
        Err(e) => die(&format!("call failed: {}", e)),
        Ok(result) => {
            save_or_die(&reg, &tmap);
            println!();
            println!("  Call: {}::{}", address, fn_name);
            println!("  Args    : {:?}", call_args);
            println!("  Success : {}", result.success);
            println!("  Return  : {:?}", result.return_value);
            println!("  Gas used: {}", result.gas_used);
            if let Some(err) = &result.error {
                println!("  Error   : {}", err);
            }
            println!("  Storage root: {}", result.storage_root);
            println!();
            curl_hint_call(&address, &fn_name, &call_args, gas_limit);
        }
    }
}

/// cargo run -- contract state <address> [key]
fn cmd_state(args: &[String]) {
    let address     = require_arg(args, 3, "address");
    let key_filter  = args.get(4).cloned();
    let (reg, _)    = load_contract_registry();

    match reg.contracts.get(&address) {
        None       => die(&format!("Contract '{}' not found", address)),
        Some(inst) => {
            println!();
            println!("  Storage state: {}", address);
            println!("  Root: {}", inst.storage.storage_root());
            if inst.storage.data.is_empty() {
                println!("  (empty)");
            } else {
                let mut keys: Vec<_> = inst.storage.data.keys().collect();
                keys.sort();
                for k in keys {
                    if key_filter.as_deref().map(|f| k == f).unwrap_or(true) {
                        println!("    {:30} = {}", k, inst.storage.data[k]);
                    }
                }
            }
            println!();
        }
    }
}

/// cargo run -- contract estimate <address> <function>
fn cmd_estimate(args: &[String]) {
    let address = require_arg(args, 3, "address");
    let fn_name = require_arg(args, 4, "function");
    let (reg, _) = load_contract_registry();

    match estimate_gas_for(&reg, &address, &fn_name) {
        None      => die(&format!("Contract or function '{}::{}' not found", address, fn_name)),
        Some(est) => {
            println!();
            println!("  Gas estimate: {}::{}", address, fn_name);
            println!("  Estimated gas: {}", est);
            println!("  (Static instruction count — actual may vary by branch)");
            println!();
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn template_to_module(template: &str) -> Result<crate::smart_contract::WasmModule, String> {
    match template {
        "counter" => Ok(counter_contract()),
        "token"   => Ok(token_contract(0, 0)),
        "voting"  => Ok(voting_contract()),
        other     => Err(format!(
            "unknown template '{}'; supported: counter | token | voting", other)),
    }
}

fn require_arg(args: &[String], idx: usize, name: &str) -> String {
    args.get(idx).cloned()
        .unwrap_or_else(|| die(&format!("missing argument: <{}>", name)))
}

fn save_or_die(reg: &ContractRegistry, tmap: &HashMap<String, String>) {
    if let Err(e) = save_contract_registry(reg, tmap) {
        die(&format!("failed to save registry: {}", e));
    }
}

fn die(msg: &str) -> ! {
    eprintln!("  Error: {}", msg);
    std::process::exit(1);
}

// ─── Curl hints ───────────────────────────────────────────────────────────────

fn curl_hint_deploy(template: &str, creator: &str) {
    println!("  REST API equivalent:");
    println!("  POST http://localhost:8080/api/write/contract/deploy");
    println!("  Body: {{\"template\":\"{}\",\"creator\":\"{}\",\"nonce\":1,",
        template, creator);
    println!("         \"pubkey_hex\":\"<pubkey>\",\"signature\":\"<ecdsa_sig>\"}}");
    println!();
}

fn curl_hint_call(address: &str, fn_name: &str, args: &[i64], gas_limit: u64) {
    println!("  REST API equivalent:");
    println!("  POST http://localhost:8080/api/write/contract/call");
    println!("  Body: {{\"address\":\"{}\",\"function\":\"{}\",\"args\":{:?},",
        address, fn_name, args);
    println!("         \"gas_limit\":{},\"dry_run\":false,\"nonce\":1,", gas_limit);
    println!("         \"pubkey_hex\":\"<pubkey>\",\"signature\":\"<ecdsa_sig>\"}}");
    println!();
}

// ─── Help ─────────────────────────────────────────────────────────────────────

fn print_help() {
    println!();
    println!("  contract commands:");
    println!("    cargo run -- contract deploy   <template> [creator]");
    println!("    cargo run -- contract list");
    println!("    cargo run -- contract info     <address>");
    println!("    cargo run -- contract call     <address> <function> [args…] [--gas N]");
    println!("    cargo run -- contract state    <address> [key]");
    println!("    cargo run -- contract estimate <address> <function>");
    println!();
    println!("  Templates: counter | token | voting");
    println!("  Default gas limit: {}", DEFAULT_GAS_LIMIT);
    println!();
    println!("  Examples:");
    println!("    cargo run -- contract deploy counter myaddr");
    println!("    cargo run -- contract call   0x1234... increment");
    println!("    cargo run -- contract call   0x1234... get_count");
    println!("    cargo run -- contract state  0x1234...");
    println!("    cargo run -- contract estimate 0x1234... increment");
    println!();
    println!("  Data stored at: ~/.pkt/db/ (RocksDB, key: contract_registry:snapshot)");
    println!();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::smart_contract::{ContractRegistry, ContractRegistrySnapshot};
    use std::collections::HashMap;

    fn deploy_counter() -> (ContractRegistry, HashMap<String, String>, String) {
        let mut reg  = ContractRegistry::new();
        let module   = crate::smart_contract::counter_contract();
        let address  = reg.deploy(module, "creator");
        let mut tmap = HashMap::new();
        tmap.insert(address.clone(), "counter".to_string());
        (reg, tmap, address)
    }

    // ── ContractRegistrySnapshot ──────────────────────────────────────────────

    #[test]
    fn test_snapshot_roundtrip_empty() {
        let reg  = ContractRegistry::new();
        let tmap = HashMap::new();
        let snap = reg.snapshot(&tmap);
        let (reg2, tmap2) = ContractRegistry::from_snapshot(snap);
        assert_eq!(reg2.contracts.len(), 0);
        assert!(tmap2.is_empty());
    }

    #[test]
    fn test_snapshot_roundtrip_counter() {
        let (reg, tmap, addr) = deploy_counter();
        let snap              = reg.snapshot(&tmap);
        let (reg2, tmap2)     = ContractRegistry::from_snapshot(snap);
        assert!(reg2.contracts.contains_key(&addr));
        assert_eq!(tmap2.get(&addr).map(|s| s.as_str()), Some("counter"));
    }

    #[test]
    fn test_snapshot_preserves_creator() {
        let (reg, tmap, addr) = deploy_counter();
        let snap              = reg.snapshot(&tmap);
        let (reg2, _)         = ContractRegistry::from_snapshot(snap);
        assert_eq!(reg2.contracts[&addr].creator, "creator");
    }

    #[test]
    fn test_snapshot_preserves_storage() {
        let (mut reg, tmap, addr) = deploy_counter();
        reg.call(&addr, "increment", vec![], 10_000).unwrap();
        let snap          = reg.snapshot(&tmap);
        let (reg2, _)     = ContractRegistry::from_snapshot(snap);
        let count         = reg2.contracts[&addr].storage.get("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_snapshot_preserves_call_count() {
        let (mut reg, tmap, addr) = deploy_counter();
        reg.call(&addr, "increment", vec![], 10_000).unwrap();
        reg.call(&addr, "increment", vec![], 10_000).unwrap();
        let snap      = reg.snapshot(&tmap);
        let (reg2, _) = ContractRegistry::from_snapshot(snap);
        assert_eq!(reg2.contracts[&addr].call_count, 2);
    }

    #[test]
    fn test_snapshot_serializes_to_json() {
        let (reg, tmap, _) = deploy_counter();
        let snap           = reg.snapshot(&tmap);
        let json           = serde_json::to_string(&snap);
        assert!(json.is_ok());
        assert!(json.unwrap().contains("counter"));
    }

    #[test]
    fn test_snapshot_deserializes_from_json() {
        let (reg, tmap, addr) = deploy_counter();
        let snap              = reg.snapshot(&tmap);
        let json              = serde_json::to_vec(&snap).unwrap();
        let snap2: ContractRegistrySnapshot = serde_json::from_slice(&json).unwrap();
        let (reg2, _)         = ContractRegistry::from_snapshot(snap2);
        assert!(reg2.contracts.contains_key(&addr));
    }

    #[test]
    fn test_snapshot_unknown_template_skipped() {
        // Manually craft a snapshot with unknown template
        use crate::smart_contract::ContractInstanceSnapshot;
        let snap = ContractRegistrySnapshot {
            contracts: vec![ContractInstanceSnapshot {
                address:      "0xdeadbeef".into(),
                template:     "unknown_xyz".into(),
                creator:      "x".into(),
                deploy_block: 0,
                call_count:   0,
                total_gas:    0,
                storage:      HashMap::new(),
            }],
            block_height: 0,
        };
        let (reg, _) = ContractRegistry::from_snapshot(snap);
        // Unknown template → skipped, registry empty
        assert!(reg.contracts.is_empty());
    }

    // ── template_to_module ────────────────────────────────────────────────────

    #[test]
    fn test_template_counter_exports() {
        let m = super::template_to_module("counter").unwrap();
        assert!(m.exports.contains(&"increment".to_string()));
        assert!(m.exports.contains(&"get_count".to_string()));
    }

    #[test]
    fn test_template_token_ok() {
        assert!(super::template_to_module("token").is_ok());
    }

    #[test]
    fn test_template_voting_ok() {
        assert!(super::template_to_module("voting").is_ok());
    }

    #[test]
    fn test_template_unknown_err() {
        let err = super::template_to_module("ponzi").unwrap_err();
        assert!(err.contains("unknown template"));
    }

    // ── estimate_gas_for ──────────────────────────────────────────────────────

    #[test]
    fn test_estimate_gas_counter_increment() {
        let (reg, _, addr) = deploy_counter();
        let est = crate::write_api::estimate_gas_for(&reg, &addr, "increment");
        assert!(est.is_some());
        assert!(est.unwrap() > 0);
    }

    #[test]
    fn test_estimate_gas_unknown_fn() {
        let (reg, _, addr) = deploy_counter();
        assert!(crate::write_api::estimate_gas_for(&reg, &addr, "nope").is_none());
    }

    // ── full deploy + call roundtrip ──────────────────────────────────────────

    #[test]
    fn test_deploy_and_call_counter() {
        let mut reg = ContractRegistry::new();
        let module  = crate::smart_contract::counter_contract();
        let addr    = reg.deploy(module, "user");
        let result  = reg.call(&addr, "increment", vec![], 10_000).unwrap();
        assert!(result.success);
        assert_eq!(reg.contracts[&addr].storage.get("count"), 1);
    }

    #[test]
    fn test_deploy_and_call_get_count() {
        let mut reg = ContractRegistry::new();
        let module  = crate::smart_contract::counter_contract();
        let addr    = reg.deploy(module, "user");
        reg.call(&addr, "increment", vec![], 10_000).unwrap();
        reg.call(&addr, "increment", vec![], 10_000).unwrap();
        let result  = reg.call(&addr, "get_count", vec![], 10_000).unwrap();
        assert!(result.success);
        assert_eq!(result.return_value, Some(2));
    }
}
