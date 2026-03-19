#![allow(dead_code)]
//! v7.2 — Token Standard (ERC-20-like)

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

pub type TokenId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub id: TokenId,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: u128,
    pub owner: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenAccount {
    pub balance: u128,
    pub allowances: HashMap<String, u128>,
}

#[derive(Debug, Clone)]
pub struct TokenRegistry {
    pub tokens: HashMap<TokenId, Token>,
    pub accounts: HashMap<(TokenId, String), TokenAccount>,
}

/// Snapshot cho serialization — tránh tuple key không hợp lệ trong JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenRegistrySnapshot {
    pub tokens:   HashMap<String, Token>,
    /// (token_id, address, account)
    pub accounts: Vec<(String, String, TokenAccount)>,
}

impl TokenRegistry {
    pub fn new() -> Self {
        TokenRegistry {
            tokens: HashMap::new(),
            accounts: HashMap::new(),
        }
    }

    pub fn snapshot(&self) -> TokenRegistrySnapshot {
        TokenRegistrySnapshot {
            tokens:   self.tokens.clone(),
            accounts: self.accounts.iter()
                .map(|((tid, addr), acct)| (tid.clone(), addr.clone(), acct.clone()))
                .collect(),
        }
    }

    pub fn from_snapshot(s: TokenRegistrySnapshot) -> Self {
        let accounts = s.accounts.into_iter()
            .map(|(tid, addr, acct)| ((tid, addr), acct))
            .collect();
        TokenRegistry { tokens: s.tokens, accounts }
    }

    pub fn create_token(
        &mut self,
        id: &str,
        name: &str,
        symbol: &str,
        decimals: u8,
        initial_supply: u128,
        owner: &str,
    ) -> Result<(), String> {
        if self.tokens.contains_key(id) {
            return Err(format!("Token {} already exists", id));
        }
        let token = Token {
            id: id.to_string(),
            name: name.to_string(),
            symbol: symbol.to_string(),
            decimals,
            total_supply: initial_supply,
            owner: owner.to_string(),
        };
        self.tokens.insert(id.to_string(), token);
        if initial_supply > 0 {
            let acc = self.accounts
                .entry((id.to_string(), owner.to_string()))
                .or_default();
            acc.balance += initial_supply;
        }
        Ok(())
    }

    pub fn mint(&mut self, token_id: &str, to: &str, amount: u128) -> Result<(), String> {
        // We allow any caller to mint in this simplified version;
        // ownership check would require passing caller
        let token = self.tokens.get_mut(token_id)
            .ok_or_else(|| format!("Token {} not found", token_id))?;
        token.total_supply += amount;
        let acc = self.accounts
            .entry((token_id.to_string(), to.to_string()))
            .or_default();
        acc.balance += amount;
        Ok(())
    }

    pub fn mint_as_owner(&mut self, token_id: &str, caller: &str, to: &str, amount: u128) -> Result<(), String> {
        {
            let token = self.tokens.get(token_id)
                .ok_or_else(|| format!("Token {} not found", token_id))?;
            if token.owner != caller {
                return Err("Only owner can mint".to_string());
            }
        }
        self.mint(token_id, to, amount)
    }

    pub fn transfer(&mut self, token_id: &str, from: &str, to: &str, amount: u128) -> Result<(), String> {
        {
            let from_acc = self.accounts
                .entry((token_id.to_string(), from.to_string()))
                .or_default();
            if from_acc.balance < amount {
                return Err(format!("Insufficient balance: {} < {}", from_acc.balance, amount));
            }
            from_acc.balance -= amount;
        }
        let to_acc = self.accounts
            .entry((token_id.to_string(), to.to_string()))
            .or_default();
        to_acc.balance += amount;
        Ok(())
    }

    pub fn burn(&mut self, token_id: &str, from: &str, amount: u128) -> Result<(), String> {
        {
            let from_acc = self.accounts
                .entry((token_id.to_string(), from.to_string()))
                .or_default();
            if from_acc.balance < amount {
                return Err(format!("Insufficient balance: {} < {}", from_acc.balance, amount));
            }
            from_acc.balance -= amount;
        }
        let token = self.tokens.get_mut(token_id)
            .ok_or_else(|| format!("Token {} not found", token_id))?;
        token.total_supply -= amount;
        Ok(())
    }

    pub fn approve(&mut self, token_id: &str, owner: &str, spender: &str, amount: u128) -> Result<(), String> {
        if !self.tokens.contains_key(token_id) {
            return Err(format!("Token {} not found", token_id));
        }
        let acc = self.accounts
            .entry((token_id.to_string(), owner.to_string()))
            .or_default();
        acc.allowances.insert(spender.to_string(), amount);
        Ok(())
    }

    pub fn transfer_from(
        &mut self,
        token_id: &str,
        spender: &str,
        from: &str,
        to: &str,
        amount: u128,
    ) -> Result<(), String> {
        // Check and deduct allowance
        {
            let from_acc = self.accounts
                .entry((token_id.to_string(), from.to_string()))
                .or_default();
            let allowance = *from_acc.allowances.get(spender).unwrap_or(&0);
            if allowance < amount {
                return Err(format!("Insufficient allowance: {} < {}", allowance, amount));
            }
            if from_acc.balance < amount {
                return Err(format!("Insufficient balance: {} < {}", from_acc.balance, amount));
            }
            from_acc.balance -= amount;
            from_acc.allowances.insert(spender.to_string(), allowance - amount);
        }
        let to_acc = self.accounts
            .entry((token_id.to_string(), to.to_string()))
            .or_default();
        to_acc.balance += amount;
        Ok(())
    }

    pub fn balance_of(&self, token_id: &str, addr: &str) -> u128 {
        self.accounts
            .get(&(token_id.to_string(), addr.to_string()))
            .map(|a| a.balance)
            .unwrap_or(0)
    }

    pub fn total_supply(&self, token_id: &str) -> u128 {
        self.tokens.get(token_id).map(|t| t.total_supply).unwrap_or(0)
    }
}

pub fn cmd_token_info() {
    let mut registry = TokenRegistry::new();
    registry.create_token("PKT", "PacketCrypt Token", "PKT", 9, 21_000_000_000_000_000, "alice").unwrap();
    println!("=== Token Info ===");
    let token = registry.tokens.get("PKT").unwrap();
    println!("Name:         {}", token.name);
    println!("Symbol:       {}", token.symbol);
    println!("Decimals:     {}", token.decimals);
    println!("Total Supply: {}", token.total_supply);
    println!("Owner:        {}", token.owner);
    println!("Alice balance: {}", registry.balance_of("PKT", "alice"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> TokenRegistry {
        let mut r = TokenRegistry::new();
        r.create_token("TKN", "Test Token", "TKN", 18, 1_000_000, "alice").unwrap();
        r
    }

    #[test]
    fn test_create_token() {
        let r = make_registry();
        assert_eq!(r.total_supply("TKN"), 1_000_000);
        assert_eq!(r.balance_of("TKN", "alice"), 1_000_000);
    }

    #[test]
    fn test_mint() {
        let mut r = make_registry();
        r.mint("TKN", "bob", 500).unwrap();
        assert_eq!(r.balance_of("TKN", "bob"), 500);
        assert_eq!(r.total_supply("TKN"), 1_000_500);
    }

    #[test]
    fn test_transfer_sufficient() {
        let mut r = make_registry();
        r.transfer("TKN", "alice", "bob", 100).unwrap();
        assert_eq!(r.balance_of("TKN", "alice"), 999_900);
        assert_eq!(r.balance_of("TKN", "bob"), 100);
    }

    #[test]
    fn test_transfer_insufficient() {
        let mut r = make_registry();
        let result = r.transfer("TKN", "alice", "bob", 2_000_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_burn() {
        let mut r = make_registry();
        r.burn("TKN", "alice", 1000).unwrap();
        assert_eq!(r.balance_of("TKN", "alice"), 999_000);
        assert_eq!(r.total_supply("TKN"), 999_000);
    }

    #[test]
    fn test_approve_and_transfer_from() {
        let mut r = make_registry();
        r.approve("TKN", "alice", "spender", 500).unwrap();
        r.transfer_from("TKN", "spender", "alice", "bob", 200).unwrap();
        assert_eq!(r.balance_of("TKN", "alice"), 999_800);
        assert_eq!(r.balance_of("TKN", "bob"), 200);
    }

    #[test]
    fn test_transfer_from_insufficient_allowance() {
        let mut r = make_registry();
        r.approve("TKN", "alice", "spender", 50).unwrap();
        let result = r.transfer_from("TKN", "spender", "alice", "bob", 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_balance_of_unknown() {
        let r = make_registry();
        assert_eq!(r.balance_of("TKN", "unknown"), 0);
    }

    #[test]
    fn test_create_duplicate_fails() {
        let mut r = make_registry();
        let result = r.create_token("TKN", "Another", "TKN", 18, 0, "bob");
        assert!(result.is_err());
    }
}
