#![allow(dead_code)]
//! v7.1 — Fee Calculator

use crate::transaction::Transaction;

#[derive(Debug, Clone)]
pub struct FeePolicy {
    pub min_fee_rate: u64,
    pub target_fee_rate: u64,
    pub max_fee_rate: u64,
}

impl FeePolicy {
    pub fn default() -> Self {
        FeePolicy {
            min_fee_rate: 1,
            target_fee_rate: 10,
            max_fee_rate: 1000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TxFeeEstimate {
    pub vsize: u64,
    pub fee_sat: u64,
    pub fee_rate: u64,
}

#[derive(Debug, Clone)]
pub struct FeeCalculator {
    pub policy: FeePolicy,
}

impl FeeCalculator {
    pub fn new(policy: FeePolicy) -> Self {
        FeeCalculator { policy }
    }

    pub fn estimate(&self, tx: &Transaction, fee_rate_sat_vbyte: u64) -> TxFeeEstimate {
        let vsize = 10 + tx.inputs.len() as u64 * 148 + tx.outputs.len() as u64 * 34;
        let clamped_rate = fee_rate_sat_vbyte
            .max(self.policy.min_fee_rate)
            .min(self.policy.max_fee_rate);
        let fee_sat = vsize * clamped_rate;
        TxFeeEstimate {
            vsize,
            fee_sat,
            fee_rate: clamped_rate,
        }
    }

    pub fn total_fees_in_block(txs: &[Transaction]) -> u64 {
        txs.iter().map(|tx| tx.fee).sum()
    }

    pub fn validate_coinbase(coinbase: &Transaction, subsidy: u64, block_fees: u64) -> bool {
        if !coinbase.is_coinbase {
            return false;
        }
        let output_sum: u64 = coinbase.outputs.iter().map(|o| o.amount).sum();
        output_sum <= subsidy + block_fees
    }

    pub fn fee_rate_from_tx(&self, tx: &Transaction) -> u64 {
        let vsize = 10 + tx.inputs.len() as u64 * 148 + tx.outputs.len() as u64 * 34;
        if vsize == 0 {
            return 0;
        }
        tx.fee / vsize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::{Transaction, TxOutput};

    fn make_tx(n_inputs: usize, n_outputs: usize, fee: u64) -> Transaction {
        let inputs_raw: Vec<(String, usize)> = (0..n_inputs)
            .map(|i| (format!("{:064x}", i), i))
            .collect();
        let outputs: Vec<TxOutput> = (0..n_outputs)
            .map(|i| TxOutput::p2pkh(1000 + i as u64, "aabbccdd00112233445566778899aabb"))
            .collect();
        Transaction::new_unsigned(inputs_raw, outputs, fee)
    }

    #[test]
    fn test_estimate_1input_2outputs() {
        let calc = FeeCalculator::new(FeePolicy::default());
        let tx = make_tx(1, 2, 1000);
        let est = calc.estimate(&tx, 10);
        // vsize = 10 + 1*148 + 2*34 = 10 + 148 + 68 = 226
        assert_eq!(est.vsize, 226);
        assert_eq!(est.fee_sat, 2260);
        assert_eq!(est.fee_rate, 10);
    }

    #[test]
    fn test_estimate_clamp_min() {
        let calc = FeeCalculator::new(FeePolicy::default());
        let tx = make_tx(1, 1, 0);
        let est = calc.estimate(&tx, 0); // below min=1
        assert_eq!(est.fee_rate, 1);
    }

    #[test]
    fn test_estimate_clamp_max() {
        let calc = FeeCalculator::new(FeePolicy::default());
        let tx = make_tx(1, 1, 0);
        let est = calc.estimate(&tx, 9999); // above max=1000
        assert_eq!(est.fee_rate, 1000);
    }

    #[test]
    fn test_total_fees_in_block() {
        let tx1 = make_tx(1, 1, 500);
        let tx2 = make_tx(2, 2, 300);
        let total = FeeCalculator::total_fees_in_block(&[tx1, tx2]);
        assert_eq!(total, 800);
    }

    #[test]
    fn test_validate_coinbase_valid() {
        let coinbase = Transaction::coinbase_at("aabbccdd00112233445566778899aabb", 1000, 0);
        // subsidy at height=0 = 50 PKT = 50_000_000_000 paklets
        let ok = FeeCalculator::validate_coinbase(&coinbase, 50_000_000_000, 1000);
        assert!(ok);
    }

    #[test]
    fn test_validate_coinbase_invalid_overclaim() {
        let coinbase = Transaction::coinbase_at("aabbccdd00112233445566778899aabb", 0, 0);
        // coinbase output is subsidy+fee from coinbase_at which uses 500_000_000_0
        // If subsidy = 0, overclaim
        let ok = FeeCalculator::validate_coinbase(&coinbase, 0, 0);
        assert!(!ok);
    }

    #[test]
    fn test_validate_coinbase_non_coinbase_tx() {
        let tx = make_tx(1, 1, 0);
        let ok = FeeCalculator::validate_coinbase(&tx, 5_000_000_000, 1000);
        assert!(!ok);
    }

    #[test]
    fn test_fee_rate_from_tx() {
        let calc = FeeCalculator::new(FeePolicy::default());
        let tx = make_tx(1, 2, 226); // vsize=226, fee=226 => rate=1
        let rate = calc.fee_rate_from_tx(&tx);
        assert_eq!(rate, 1);
    }

    #[test]
    fn test_fee_rate_from_tx_higher_fee() {
        let calc = FeeCalculator::new(FeePolicy::default());
        let tx = make_tx(1, 2, 2260); // vsize=226, fee=2260 => rate=10
        let rate = calc.fee_rate_from_tx(&tx);
        assert_eq!(rate, 10);
    }
}
