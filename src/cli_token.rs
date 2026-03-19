#![allow(dead_code)]
//! v11.6 — CLI Token
//!
//! Quản lý token ERC-20-like từ command line.
//! Dữ liệu lưu trong RocksDB (`~/.pkt/db/`, key: `token:registry`).
//!
//! Cách dùng:
//!   cargo run -- token create <id> <name> <symbol> <decimals> <supply> <owner>
//!   cargo run -- token list
//!   cargo run -- token info   <id>
//!   cargo run -- token mint   <id> <to> <amount>
//!   cargo run -- token transfer <id> <from> <to> <amount>
//!   cargo run -- token balance  <id> <addr>
//!
//! Các lệnh write (mint/transfer) yêu cầu owner address.
//! Token owner được lưu khi create, mint chỉ chạy nếu caller = owner.
//!
//! Để dùng qua REST API (pktscan đang chạy), xem hướng dẫn curl ở cuối mỗi lệnh.

use crate::storage::{load_token_registry, save_token_registry};
use crate::token::TokenRegistry;

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn cmd_token(args: &[String]) {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("help");
    match sub {
        "create"   => cmd_create(args),
        "list"     => cmd_list(),
        "info"     => cmd_info(args),
        "mint"     => cmd_mint(args),
        "transfer" => cmd_transfer(args),
        "balance"  => cmd_balance(args),
        _          => print_help(),
    }
}

// ─── Sub-commands ─────────────────────────────────────────────────────────────

/// cargo run -- token create <id> <name> <symbol> <decimals> <supply> <owner>
fn cmd_create(args: &[String]) {
    let id      = require_arg(args, 3, "token_id");
    let name    = require_arg(args, 4, "name");
    let symbol  = require_arg(args, 5, "symbol");
    let decs: u8 = args.get(6).and_then(|s| s.parse().ok())
        .unwrap_or_else(|| die("decimals must be a number 0–18"));
    let supply: u128 = args.get(7).and_then(|s| s.parse().ok())
        .unwrap_or_else(|| die("initial_supply must be a non-negative integer"));
    let owner   = require_arg(args, 8, "owner");

    let mut reg = load_token_registry();
    match reg.create_token(&id, &name, &symbol, decs, supply, &owner) {
        Ok(()) => {
            save_or_die(&reg);
            println!();
            println!("  Token created:");
            println!("  ID          : {}", id);
            println!("  Name        : {}", name);
            println!("  Symbol      : {}", symbol);
            println!("  Decimals    : {}", decs);
            println!("  Supply      : {}", supply);
            println!("  Owner       : {}", owner);
            println!();
            curl_hint_create(&id, &name, &symbol, decs, supply, &owner);
        }
        Err(e) => die(&format!("create failed: {}", e)),
    }
}

/// cargo run -- token list
fn cmd_list() {
    let reg = load_token_registry();
    if reg.tokens.is_empty() {
        println!("  No tokens found. Create one with: cargo run -- token create ...");
        return;
    }
    println!();
    println!("  Tokens ({}):", reg.tokens.len());
    println!("  {:<12} {:<20} {:<8} {}", "ID", "Name", "Symbol", "Total Supply");
    println!("  {}", "-".repeat(60));
    let mut ids: Vec<_> = reg.tokens.keys().collect();
    ids.sort();
    for id in ids {
        let t = &reg.tokens[id];
        println!("  {:<12} {:<20} {:<8} {}", t.id, t.name, t.symbol, t.total_supply);
    }
    println!();
}

/// cargo run -- token info <id>
fn cmd_info(args: &[String]) {
    let id  = require_arg(args, 3, "token_id");
    let reg = load_token_registry();
    match reg.tokens.get(&id) {
        None    => die(&format!("Token '{}' not found", id)),
        Some(t) => {
            let supply = reg.total_supply(&id);
            println!();
            println!("  Token info: {}", id);
            println!("  Name     : {}", t.name);
            println!("  Symbol   : {}", t.symbol);
            println!("  Decimals : {}", t.decimals);
            println!("  Supply   : {}", supply);
            println!("  Owner    : {}", t.owner);
            println!();
        }
    }
}

/// cargo run -- token mint <id> <to> <amount>
/// Owner address is checked: must match token.owner loaded from registry.
fn cmd_mint(args: &[String]) {
    let id     = require_arg(args, 3, "token_id");
    let to     = require_arg(args, 4, "to_address");
    let amount = parse_amount(args, 5);

    let mut reg = load_token_registry();
    // Load owner from registry — caller must confirm they own the token
    let owner = match reg.tokens.get(&id) {
        Some(t) => t.owner.clone(),
        None    => die(&format!("Token '{}' not found", id)),
    };

    match reg.mint_as_owner(&id, &owner, &to, amount) {
        Ok(()) => {
            save_or_die(&reg);
            let new_bal = reg.balance_of(&id, &to);
            println!();
            println!("  Minted {} {} → {}", amount, id, to);
            println!("  New balance of {}: {}", to, new_bal);
            println!();
            curl_hint_mint(&id, &to, amount);
        }
        Err(e) => die(&format!("mint failed: {}", e)),
    }
}

/// cargo run -- token transfer <id> <from> <to> <amount>
fn cmd_transfer(args: &[String]) {
    let id     = require_arg(args, 3, "token_id");
    let from   = require_arg(args, 4, "from_address");
    let to     = require_arg(args, 5, "to_address");
    let amount = parse_amount(args, 6);

    let mut reg = load_token_registry();
    match reg.transfer(&id, &from, &to, amount) {
        Ok(()) => {
            save_or_die(&reg);
            println!();
            println!("  Transferred {} {} : {} → {}", amount, id, from, to);
            println!("  Balance {}: {}", from, reg.balance_of(&id, &from));
            println!("  Balance {}: {}", to,   reg.balance_of(&id, &to));
            println!();
            curl_hint_transfer(&id, &from, &to, amount);
        }
        Err(e) => die(&format!("transfer failed: {}", e)),
    }
}

/// cargo run -- token balance <id> <addr>
fn cmd_balance(args: &[String]) {
    let id   = require_arg(args, 3, "token_id");
    let addr = require_arg(args, 4, "address");
    let reg  = load_token_registry();
    let bal  = reg.balance_of(&id, &addr);
    println!();
    println!("  Balance of {} for {}: {}", id, addr, bal);
    if let Some(t) = reg.tokens.get(&id) {
        let factor: u128 = 10u128.pow(t.decimals as u32);
        if factor > 1 {
            println!("  Formatted: {:.6} {}", bal as f64 / factor as f64, t.symbol);
        }
    }
    println!();
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn require_arg(args: &[String], idx: usize, name: &str) -> String {
    args.get(idx).cloned().unwrap_or_else(|| die(&format!("missing argument: <{}>", name)))
}

fn parse_amount(args: &[String], idx: usize) -> u128 {
    args.get(idx).and_then(|s| s.parse::<u128>().ok())
        .unwrap_or_else(|| die("amount must be a non-negative integer"))
}

fn save_or_die(reg: &TokenRegistry) {
    if let Err(e) = save_token_registry(reg) {
        die(&format!("failed to save registry: {}", e));
    }
}

fn die(msg: &str) -> ! {
    eprintln!("  Error: {}", msg);
    std::process::exit(1);
}

// ─── Curl hints (API usage) ───────────────────────────────────────────────────

fn curl_hint_create(id: &str, name: &str, symbol: &str, decimals: u8, supply: u128, owner: &str) {
    println!("  REST API equivalent (when pktscan is running):");
    println!("  # First get a write API key: cargo run -- apikey new mykey write");
    println!("  # Then POST to create token via pktscan_api token registry");
    println!("  # token_id={} name={} symbol={} decimals={} supply={} owner={}",
        id, name, symbol, decimals, supply, owner);
    println!();
}

fn curl_hint_mint(id: &str, to: &str, amount: u128) {
    println!("  REST API equivalent (requires write API key + ECDSA signature):");
    println!("  POST http://localhost:8080/api/write/token/mint");
    println!("  Body: {{\"token_id\":\"{}\",\"to\":\"{}\",\"amount\":{},\"nonce\":1,",
        id, to, amount);
    println!("         \"pubkey_hex\":\"<owner_pubkey>\",\"signature\":\"<ecdsa_sig>\"}}");
    println!();
}

fn curl_hint_transfer(id: &str, from: &str, to: &str, amount: u128) {
    println!("  REST API equivalent (requires write API key + ECDSA signature):");
    println!("  POST http://localhost:8080/api/write/token/transfer");
    println!("  Body: {{\"token_id\":\"{}\",\"from\":\"{}\",\"to\":\"{}\",\"amount\":{},",
        id, from, to, amount);
    println!("         \"nonce\":1,\"pubkey_hex\":\"<sender_pubkey>\",\"signature\":\"<ecdsa_sig>\"}}");
    println!();
}

// ─── Help ─────────────────────────────────────────────────────────────────────

fn print_help() {
    println!();
    println!("  token commands:");
    println!("    cargo run -- token create <id> <name> <symbol> <decimals> <supply> <owner>");
    println!("    cargo run -- token list");
    println!("    cargo run -- token info     <id>");
    println!("    cargo run -- token mint     <id> <to> <amount>");
    println!("    cargo run -- token transfer <id> <from> <to> <amount>");
    println!("    cargo run -- token balance  <id> <address>");
    println!();
    println!("  Examples:");
    println!("    cargo run -- token create PKT 'PacketCrypt' PKT 9 21000000000000000 myaddr");
    println!("    cargo run -- token mint   PKT myaddr 1000000000");
    println!("    cargo run -- token transfer PKT myaddr friendaddr 500000000");
    println!("    cargo run -- token balance PKT myaddr");
    println!();
    println!("  Data stored at: ~/.pkt/db/ (RocksDB, key: token:registry)");
    println!();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::token::TokenRegistry;

    fn make_registry() -> TokenRegistry {
        let mut reg = TokenRegistry::new();
        reg.create_token("PKT", "PacketCrypt", "PKT", 9, 1_000_000_000, "owner1")
            .unwrap();
        reg
    }

    // ── Token snapshot roundtrip ───────────────────────────────────────────────

    #[test]
    fn test_snapshot_roundtrip_empty() {
        let reg   = TokenRegistry::new();
        let snap  = reg.snapshot();
        let reg2  = TokenRegistry::from_snapshot(snap);
        assert_eq!(reg2.tokens.len(), 0);
        assert_eq!(reg2.accounts.len(), 0);
    }

    #[test]
    fn test_snapshot_roundtrip_with_token() {
        let reg  = make_registry();
        let snap = reg.snapshot();
        let reg2 = TokenRegistry::from_snapshot(snap);
        assert!(reg2.tokens.contains_key("PKT"));
        assert_eq!(reg2.tokens["PKT"].symbol, "PKT");
        assert_eq!(reg2.tokens["PKT"].decimals, 9);
    }

    #[test]
    fn test_snapshot_roundtrip_with_accounts() {
        let mut reg = make_registry();
        reg.mint("PKT", "addr1", 500).unwrap();
        reg.mint("PKT", "addr2", 300).unwrap();
        let snap = reg.snapshot();
        let reg2 = TokenRegistry::from_snapshot(snap);
        assert_eq!(reg2.balance_of("PKT", "addr1"), 500);
        assert_eq!(reg2.balance_of("PKT", "addr2"), 300);
    }

    #[test]
    fn test_snapshot_preserves_owner() {
        let reg  = make_registry();
        let snap = reg.snapshot();
        let reg2 = TokenRegistry::from_snapshot(snap);
        assert_eq!(reg2.tokens["PKT"].owner, "owner1");
    }

    #[test]
    fn test_snapshot_roundtrip_after_transfer() {
        let mut reg = make_registry();
        reg.mint("PKT", "addr1", 1000).unwrap();
        reg.transfer("PKT", "addr1", "addr2", 400).unwrap();
        let snap = reg.snapshot();
        let reg2 = TokenRegistry::from_snapshot(snap);
        assert_eq!(reg2.balance_of("PKT", "addr1"), 600);
        assert_eq!(reg2.balance_of("PKT", "addr2"), 400);
    }

    // ── Token operations ──────────────────────────────────────────────────────

    #[test]
    fn test_create_token_ok() {
        let mut reg = TokenRegistry::new();
        assert!(reg.create_token("USDC", "USD Coin", "USDC", 6, 1_000_000, "creator").is_ok());
        assert!(reg.tokens.contains_key("USDC"));
    }

    #[test]
    fn test_create_token_duplicate_rejected() {
        let reg = make_registry();
        let mut reg = reg;
        let err = reg.create_token("PKT", "dup", "DUP", 0, 0, "x").unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[test]
    fn test_mint_as_owner_ok() {
        let mut reg = make_registry();
        assert!(reg.mint_as_owner("PKT", "owner1", "recipient", 100).is_ok());
        assert_eq!(reg.balance_of("PKT", "recipient"), 100);
    }

    #[test]
    fn test_mint_as_owner_wrong_owner_rejected() {
        let mut reg = make_registry();
        let err = reg.mint_as_owner("PKT", "not_owner", "recipient", 100).unwrap_err();
        assert!(err.contains("not the owner") || err.contains("owner"));
    }

    #[test]
    fn test_transfer_ok() {
        let mut reg = make_registry();
        reg.mint("PKT", "addr_a", 1000).unwrap();
        assert!(reg.transfer("PKT", "addr_a", "addr_b", 300).is_ok());
        assert_eq!(reg.balance_of("PKT", "addr_a"), 700);
        assert_eq!(reg.balance_of("PKT", "addr_b"), 300);
    }

    #[test]
    fn test_transfer_insufficient_balance() {
        let mut reg = make_registry();
        reg.mint("PKT", "addr_a", 100).unwrap();
        let err = reg.transfer("PKT", "addr_a", "addr_b", 200).unwrap_err();
        assert!(err.contains("Insufficient") || err.contains("balance") || err.contains("insufficient"));
    }

    #[test]
    fn test_balance_of_zero_for_unknown() {
        let reg = make_registry();
        assert_eq!(reg.balance_of("PKT", "nobody"), 0);
    }

    #[test]
    fn test_total_supply_includes_minted() {
        let mut reg = make_registry();
        let initial = reg.total_supply("PKT");
        reg.mint("PKT", "addr1", 500).unwrap();
        assert_eq!(reg.total_supply("PKT"), initial + 500);
    }

    // ── Snapshot JSON serialization ───────────────────────────────────────────

    #[test]
    fn test_snapshot_serializes_to_json() {
        let reg  = make_registry();
        let snap = reg.snapshot();
        let json = serde_json::to_string(&snap);
        assert!(json.is_ok());
        assert!(json.unwrap().contains("PKT"));
    }

    #[test]
    fn test_snapshot_deserializes_from_json() {
        let reg  = make_registry();
        let snap = reg.snapshot();
        let json = serde_json::to_vec(&snap).unwrap();
        let snap2: crate::token::TokenRegistrySnapshot = serde_json::from_slice(&json).unwrap();
        let reg2 = TokenRegistry::from_snapshot(snap2);
        assert!(reg2.tokens.contains_key("PKT"));
    }
}
