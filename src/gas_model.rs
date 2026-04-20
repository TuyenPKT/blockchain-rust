#![allow(dead_code)]
//! v26.0 — EIP-1559 Gas Model
//!
//! Implements:
//!   - EIP-1559 base fee adjustment per block
//!   - Effective gas price = min(max_fee, base_fee + priority_fee)
//!   - Gas cost table (Yellow Paper / Berlin / London)
//!   - Block gas limit management (30M target, 60M max)

use serde::{Deserialize, Serialize};

// ─── Block gas constants ──────────────────────────────────────────────────────

pub const BLOCK_GAS_TARGET:  u64 = 15_000_000; // 15M gas target per block
pub const BLOCK_GAS_LIMIT:   u64 = 30_000_000; // 30M gas limit per block
pub const BASE_FEE_MAX_CHANGE_DENOMINATOR: u64 = 8; // 12.5% max change per block
pub const MIN_BASE_FEE:      u64 = 1_000_000;  // 1 Gwei minimum
pub const INITIAL_BASE_FEE:  u64 = 1_000_000_000; // 1 Gwei initial

// ─── Opcode gas costs (Berlin / London) ──────────────────────────────────────

pub const GAS_ZERO:          u64 = 0;
pub const GAS_JUMPDEST:      u64 = 1;
pub const GAS_BASE:          u64 = 2;   // ADDRESS, ORIGIN, CALLER, etc.
pub const GAS_VERYLOW:       u64 = 3;   // ADD, SUB, MUL, NOT, etc.
pub const GAS_LOW:           u64 = 5;   // MUL, DIV, SDIV, MOD, SMOD, SIGNEXTEND
pub const GAS_MID:           u64 = 8;   // ADDMOD, MULMOD, JUMP
pub const GAS_HIGH:          u64 = 10;  // JUMPI
pub const GAS_WARM_ACCESS:   u64 = 100; // EIP-2929 warm storage/account access
pub const GAS_COLD_SLOAD:    u64 = 2_100; // EIP-2929 cold SLOAD
pub const GAS_COLD_ACCOUNT:  u64 = 2_600; // EIP-2929 cold account access
pub const GAS_SSTORE_SET:    u64 = 20_000; // SSTORE zero → nonzero
pub const GAS_SSTORE_RESET:  u64 = 2_900;  // SSTORE nonzero → nonzero
pub const GAS_SSTORE_CLEAR:  u64 = 15_000; // SSTORE refund for clear
pub const GAS_SELFDESTRUCT:  u64 = 5_000;
pub const GAS_CREATE:        u64 = 32_000;
pub const GAS_CODEDEPOSIT:   u64 = 200;    // per byte for storing deployed bytecode
pub const GAS_CALL:          u64 = 100; // base (warm)
pub const GAS_CALL_VALUE:    u64 = 9_000;   // extra for CALL with value
pub const GAS_CALL_NEWACCT:  u64 = 25_000;  // creating new account in CALL
pub const GAS_EXP:           u64 = 10;
pub const GAS_EXP_BYTE:      u64 = 50;      // per byte of exponent
pub const GAS_MEMORY:        u64 = 3;       // per 32-byte word
pub const GAS_TX_ZERO_DATA:  u64 = 4;       // per zero byte in calldata
pub const GAS_TX_NONZERO:    u64 = 16;      // per nonzero byte in calldata
pub const GAS_TX_BASE:       u64 = 21_000;  // base cost of any tx
pub const GAS_SHA3:          u64 = 30;
pub const GAS_SHA3_WORD:     u64 = 6;       // per 32-byte word
pub const GAS_COPY:          u64 = 3;       // per 32-byte word (CALLDATACOPY etc)
pub const GAS_BLOCKHASH:     u64 = 20;
pub const GAS_LOG:           u64 = 375;
pub const GAS_LOG_DATA:      u64 = 8;       // per byte
pub const GAS_LOG_TOPIC:     u64 = 375;     // per topic
pub const GAS_KECCAK256:     u64 = 30;

// ─── Transaction gas price ────────────────────────────────────────────────────

/// EIP-1559 transaction pricing fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasPrice {
    /// Maximum fee per gas (paklets/gas) — EIP-1559 max_fee_per_gas.
    pub max_fee_per_gas: u64,
    /// Maximum priority fee (tip) per gas — EIP-1559 max_priority_fee_per_gas.
    pub max_priority_fee_per_gas: u64,
}

impl GasPrice {
    /// Legacy pricing: single gas price field (pre-EIP-1559).
    pub fn legacy(gas_price: u64) -> Self {
        GasPrice {
            max_fee_per_gas:          gas_price,
            max_priority_fee_per_gas: gas_price,
        }
    }

    /// Effective gas price given current base fee.
    /// `min(max_fee_per_gas, base_fee + max_priority_fee_per_gas)`
    pub fn effective(&self, base_fee: u64) -> Option<u64> {
        if self.max_fee_per_gas < base_fee {
            return None; // tx can't be included — max_fee below base
        }
        let priority_cap = self.max_fee_per_gas - base_fee;
        let priority = self.max_priority_fee_per_gas.min(priority_cap);
        Some(base_fee + priority)
    }

    /// Miner tip portion at given base fee.
    pub fn miner_tip(&self, base_fee: u64) -> u64 {
        self.effective(base_fee)
            .map(|p| p.saturating_sub(base_fee))
            .unwrap_or(0)
    }
}

// ─── Base fee adjustment (EIP-1559) ──────────────────────────────────────────

/// Tính base fee cho block tiếp theo theo EIP-1559.
///
/// Công thức:
///   delta = parent_gas_used - gas_target
///   if delta > 0:
///     base_fee += max(base_fee * delta / gas_target / 8, 1)
///   else:
///     base_fee -= base_fee * |delta| / gas_target / 8
pub fn next_base_fee(
    parent_base_fee: u64,
    parent_gas_used: u64,
    parent_gas_limit: u64,
) -> u64 {
    let gas_target = parent_gas_limit / 2; // target = 50% of limit
    if parent_gas_used == gas_target {
        return parent_base_fee;
    }
    if parent_gas_used > gas_target {
        let gas_delta = parent_gas_used - gas_target;
        let base_fee_delta = (parent_base_fee * gas_delta / gas_target
            / BASE_FEE_MAX_CHANGE_DENOMINATOR)
            .max(1);
        parent_base_fee.saturating_add(base_fee_delta)
    } else {
        let gas_delta = gas_target - parent_gas_used;
        let base_fee_delta = parent_base_fee * gas_delta / gas_target
            / BASE_FEE_MAX_CHANGE_DENOMINATOR;
        parent_base_fee.saturating_sub(base_fee_delta).max(MIN_BASE_FEE)
    }
}

/// PKT burned = base_fee * gas_used (deflation mechanism).
pub fn burn_amount(gas_used: u64, base_fee: u64) -> u64 {
    gas_used.saturating_mul(base_fee)
}

/// Tính intrinsic gas cost của transaction (trước khi execute).
pub fn intrinsic_gas(calldata: &[u8], is_create: bool) -> u64 {
    let mut gas = GAS_TX_BASE;
    if is_create { gas += GAS_CREATE; }
    for byte in calldata {
        gas += if *byte == 0 { GAS_TX_ZERO_DATA } else { GAS_TX_NONZERO };
    }
    gas
}

/// Memory expansion gas cost per EVM Yellow Paper.
/// `3 * words + floor(words^2 / 512)`
pub fn memory_gas(words: u64) -> u64 {
    GAS_MEMORY * words + words * words / 512
}

// ─── Block header gas fields ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasHeader {
    pub gas_limit:   u64,
    pub gas_used:    u64,
    pub base_fee:    u64,
    pub burned:      u64, // base_fee * gas_used — cung PKT thực sự giảm
}

impl GasHeader {
    pub fn genesis() -> Self {
        GasHeader {
            gas_limit: BLOCK_GAS_LIMIT,
            gas_used:  0,
            base_fee:  INITIAL_BASE_FEE,
            burned:    0,
        }
    }

    /// Tạo GasHeader cho block tiếp theo từ block hiện tại.
    pub fn next(&self, new_gas_used: u64) -> Self {
        let new_base_fee = next_base_fee(self.base_fee, self.gas_used, self.gas_limit);
        let burned = burn_amount(new_gas_used, new_base_fee);
        GasHeader {
            gas_limit: BLOCK_GAS_LIMIT,
            gas_used:  new_gas_used,
            base_fee:  new_base_fee,
            burned,
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_fee_unchanged_at_target() {
        let fee = next_base_fee(1_000_000_000, 15_000_000, 30_000_000);
        assert_eq!(fee, 1_000_000_000);
    }

    #[test]
    fn test_base_fee_increases_when_above_target() {
        let fee = next_base_fee(1_000_000_000, 30_000_000, 30_000_000);
        assert!(fee > 1_000_000_000);
        // Max increase = 12.5%
        assert!(fee <= 1_000_000_000 + 1_000_000_000 / 8 + 1);
    }

    #[test]
    fn test_base_fee_decreases_when_below_target() {
        let fee = next_base_fee(1_000_000_000, 0, 30_000_000);
        assert!(fee < 1_000_000_000);
        // Max decrease = 12.5%
        assert!(fee >= 1_000_000_000 - 1_000_000_000 / 8);
    }

    #[test]
    fn test_base_fee_never_below_minimum() {
        let fee = next_base_fee(MIN_BASE_FEE, 0, 30_000_000);
        assert_eq!(fee, MIN_BASE_FEE);
    }

    #[test]
    fn test_effective_price_normal() {
        let gp = GasPrice { max_fee_per_gas: 2_000_000_000, max_priority_fee_per_gas: 500_000_000 };
        let effective = gp.effective(1_000_000_000).unwrap();
        assert_eq!(effective, 1_500_000_000); // base + priority
    }

    #[test]
    fn test_effective_price_capped_at_max_fee() {
        let gp = GasPrice { max_fee_per_gas: 1_100_000_000, max_priority_fee_per_gas: 500_000_000 };
        let effective = gp.effective(1_000_000_000).unwrap();
        assert_eq!(effective, 1_100_000_000); // capped at max_fee
    }

    #[test]
    fn test_effective_price_none_below_base() {
        let gp = GasPrice { max_fee_per_gas: 500_000_000, max_priority_fee_per_gas: 0 };
        assert!(gp.effective(1_000_000_000).is_none());
    }

    #[test]
    fn test_intrinsic_gas_simple_transfer() {
        assert_eq!(intrinsic_gas(&[], false), GAS_TX_BASE);
    }

    #[test]
    fn test_intrinsic_gas_with_calldata() {
        // 1 zero byte + 1 nonzero byte
        let data = [0u8, 1u8];
        assert_eq!(intrinsic_gas(&data, false), GAS_TX_BASE + GAS_TX_ZERO_DATA + GAS_TX_NONZERO);
    }

    #[test]
    fn test_intrinsic_gas_create() {
        assert_eq!(intrinsic_gas(&[], true), GAS_TX_BASE + GAS_CREATE);
    }

    #[test]
    fn test_burn_amount() {
        assert_eq!(burn_amount(21_000, 1_000_000_000), 21_000 * 1_000_000_000);
    }

    #[test]
    fn test_gas_header_next_genesis() {
        // Genesis gas_used=0 < target → base fee decreases for next block
        let genesis = GasHeader::genesis();
        let next = genesis.next(0);
        assert_eq!(next.gas_limit, BLOCK_GAS_LIMIT);
        assert!(next.base_fee < INITIAL_BASE_FEE);

        // Parent with gas_used == target → base_fee unchanged
        let at_target = GasHeader { gas_limit: BLOCK_GAS_LIMIT, gas_used: BLOCK_GAS_TARGET, base_fee: INITIAL_BASE_FEE, burned: 0 };
        let next2 = at_target.next(0);
        assert_eq!(next2.base_fee, INITIAL_BASE_FEE);
    }
}
