#![allow(dead_code)]
//! v7.7 — DeFi Primitives (AMM)

use std::collections::HashMap;

pub type TokenId = String;

#[derive(Debug, Clone)]
pub struct LiquidityPool {
    pub id: String,
    pub token_a: TokenId,
    pub token_b: TokenId,
    pub reserve_a: u128,
    pub reserve_b: u128,
    pub lp_supply: u128,
    pub fee_bps: u32, // basis points, 30 = 0.3%
}

impl LiquidityPool {
    pub fn new(id: &str, token_a: &str, token_b: &str, fee_bps: u32) -> Self {
        LiquidityPool {
            id: id.to_string(),
            token_a: token_a.to_string(),
            token_b: token_b.to_string(),
            reserve_a: 0,
            reserve_b: 0,
            lp_supply: 0,
            fee_bps,
        }
    }

    pub fn add_liquidity(&mut self, amount_a: u128, amount_b: u128) -> u128 {
        let lp_minted = if self.lp_supply == 0 {
            // Initial liquidity: geometric mean
            let product = amount_a.saturating_mul(amount_b);
            (product as f64).sqrt() as u128
        } else {
            // Proportional to existing ratio, take min
            let lp_a = amount_a.saturating_mul(self.lp_supply) / self.reserve_a.max(1);
            let lp_b = amount_b.saturating_mul(self.lp_supply) / self.reserve_b.max(1);
            lp_a.min(lp_b)
        };
        self.reserve_a += amount_a;
        self.reserve_b += amount_b;
        self.lp_supply += lp_minted;
        lp_minted
    }

    pub fn remove_liquidity(&mut self, lp_amount: u128) -> (u128, u128) {
        if self.lp_supply == 0 || lp_amount == 0 {
            return (0, 0);
        }
        let amount_a = lp_amount.saturating_mul(self.reserve_a) / self.lp_supply;
        let amount_b = lp_amount.saturating_mul(self.reserve_b) / self.lp_supply;
        self.reserve_a = self.reserve_a.saturating_sub(amount_a);
        self.reserve_b = self.reserve_b.saturating_sub(amount_b);
        self.lp_supply = self.lp_supply.saturating_sub(lp_amount);
        (amount_a, amount_b)
    }

    pub fn swap_a_for_b(&mut self, amount_in: u128) -> Result<u128, String> {
        if self.reserve_a == 0 || self.reserve_b == 0 {
            return Err("Pool has no liquidity".to_string());
        }
        // Apply fee: amount_in_with_fee = amount_in * (10000 - fee_bps) / 10000
        let fee_factor = 10000u128 - self.fee_bps as u128;
        let amount_in_with_fee = amount_in.saturating_mul(fee_factor) / 10000;
        // x*y=k: amount_out = reserve_b * amount_in_with_fee / (reserve_a + amount_in_with_fee)
        let amount_out = self.reserve_b
            .saturating_mul(amount_in_with_fee)
            / (self.reserve_a + amount_in_with_fee);
        if amount_out == 0 {
            return Err("Insufficient output amount".to_string());
        }
        self.reserve_a += amount_in;
        self.reserve_b = self.reserve_b.saturating_sub(amount_out);
        Ok(amount_out)
    }

    pub fn swap_b_for_a(&mut self, amount_in: u128) -> Result<u128, String> {
        if self.reserve_a == 0 || self.reserve_b == 0 {
            return Err("Pool has no liquidity".to_string());
        }
        let fee_factor = 10000u128 - self.fee_bps as u128;
        let amount_in_with_fee = amount_in.saturating_mul(fee_factor) / 10000;
        let amount_out = self.reserve_a
            .saturating_mul(amount_in_with_fee)
            / (self.reserve_b + amount_in_with_fee);
        if amount_out == 0 {
            return Err("Insufficient output amount".to_string());
        }
        self.reserve_b += amount_in;
        self.reserve_a = self.reserve_a.saturating_sub(amount_out);
        Ok(amount_out)
    }

    pub fn spot_price_a_in_b(&self) -> f64 {
        if self.reserve_a == 0 {
            return 0.0;
        }
        self.reserve_b as f64 / self.reserve_a as f64
    }

    pub fn price_impact(&self, amount_in: u128, is_a_for_b: bool) -> f64 {
        if is_a_for_b {
            if self.reserve_a == 0 {
                return 0.0;
            }
            amount_in as f64 / self.reserve_a as f64 * 100.0
        } else {
            if self.reserve_b == 0 {
                return 0.0;
            }
            amount_in as f64 / self.reserve_b as f64 * 100.0
        }
    }
}

#[derive(Debug, Clone)]
pub struct DEX {
    pub pools: HashMap<String, LiquidityPool>,
}

impl DEX {
    pub fn new() -> Self {
        DEX { pools: HashMap::new() }
    }

    pub fn create_pool(&mut self, token_a: &str, token_b: &str, fee_bps: u32) -> String {
        let id = format!("{}/{}", token_a, token_b);
        let pool = LiquidityPool::new(&id, token_a, token_b, fee_bps);
        self.pools.insert(id.clone(), pool);
        id
    }

    pub fn get_pool(&self, id: &str) -> Option<&LiquidityPool> {
        self.pools.get(id)
    }

    pub fn get_pool_mut(&mut self, id: &str) -> Option<&mut LiquidityPool> {
        self.pools.get_mut(id)
    }

    pub fn best_price(&self, token_in: &str, token_out: &str, amount: u128) -> Option<u128> {
        let id = format!("{}/{}", token_in, token_out);
        let id_rev = format!("{}/{}", token_out, token_in);

        if let Some(pool) = self.pools.get(&id) {
            if pool.reserve_a > 0 && pool.reserve_b > 0 {
                let fee_factor = 10000u128 - pool.fee_bps as u128;
                let amount_in_with_fee = amount.saturating_mul(fee_factor) / 10000;
                let out = pool.reserve_b
                    .saturating_mul(amount_in_with_fee)
                    / (pool.reserve_a + amount_in_with_fee);
                return Some(out);
            }
        }

        if let Some(pool) = self.pools.get(&id_rev) {
            if pool.reserve_a > 0 && pool.reserve_b > 0 {
                let fee_factor = 10000u128 - pool.fee_bps as u128;
                let amount_in_with_fee = amount.saturating_mul(fee_factor) / 10000;
                let out = pool.reserve_a
                    .saturating_mul(amount_in_with_fee)
                    / (pool.reserve_b + amount_in_with_fee);
                return Some(out);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pool() -> LiquidityPool {
        let mut p = LiquidityPool::new("PKT/USDC", "PKT", "USDC", 30);
        p.add_liquidity(1_000_000, 1_000_000);
        p
    }

    #[test]
    fn test_add_liquidity() {
        let mut p = LiquidityPool::new("A/B", "A", "B", 30);
        let lp = p.add_liquidity(1000, 1000);
        assert!(lp > 0);
        assert_eq!(p.reserve_a, 1000);
        assert_eq!(p.reserve_b, 1000);
    }

    #[test]
    fn test_remove_liquidity() {
        let mut p = make_pool();
        let lp_total = p.lp_supply;
        let (a, b) = p.remove_liquidity(lp_total / 2);
        assert!(a > 0);
        assert!(b > 0);
        assert!(p.reserve_a < 1_000_000);
    }

    #[test]
    fn test_swap_a_for_b() {
        let mut p = make_pool();
        let out = p.swap_a_for_b(1000).unwrap();
        assert!(out > 0);
        assert!(out < 1000); // due to fee and slippage
        assert_eq!(p.reserve_a, 1_001_000);
    }

    #[test]
    fn test_swap_b_for_a() {
        let mut p = make_pool();
        let out = p.swap_b_for_a(1000).unwrap();
        assert!(out > 0);
        assert_eq!(p.reserve_b, 1_001_000);
    }

    #[test]
    fn test_spot_price() {
        let p = make_pool(); // 1:1 ratio
        let price = p.spot_price_a_in_b();
        assert!((price - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_price_impact() {
        let p = make_pool();
        let impact = p.price_impact(10_000, true); // 1% of pool
        assert!((impact - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_k_invariant_maintained() {
        let mut p = make_pool();
        let k_before = p.reserve_a * p.reserve_b;
        p.swap_a_for_b(1000).unwrap();
        let k_after = p.reserve_a * p.reserve_b;
        // k should increase slightly due to fees
        assert!(k_after >= k_before);
    }

    #[test]
    fn test_dex_create_pool() {
        let mut dex = DEX::new();
        let id = dex.create_pool("PKT", "USDC", 30);
        assert_eq!(id, "PKT/USDC");
        assert!(dex.get_pool("PKT/USDC").is_some());
    }

    #[test]
    fn test_dex_best_price() {
        let mut dex = DEX::new();
        let id = dex.create_pool("PKT", "USDC", 30);
        dex.get_pool_mut(&id).unwrap().add_liquidity(1_000_000, 1_000_000);
        let price = dex.best_price("PKT", "USDC", 1000);
        assert!(price.is_some());
        assert!(price.unwrap() > 0);
    }
}
