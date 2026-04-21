//! pkt_validate.rs — v23.0: Full-node transaction & block validation
//!
//! Validates:
//! 1. Coinbase rules  — exactly one coinbase at index 0, no coinbase elsewhere
//! 2. UTXO existence  — every non-coinbase input references a known unspent output
//! 3. No in-block double-spend — same (txid, vout) not spent twice in one block
//! 4. Value conservation — sum(inputs) >= sum(outputs) for non-coinbase txs
//! 5. Merkle root     — SHA256d tree over `wire_txid` values (same as `pkt_block_sync`)
//!
//! Script-sig verification (OP_CHECKSIG) is deferred to v23.1.

#![allow(dead_code)]

use std::collections::HashSet;

use crate::pkt_sync::SyncError;
use crate::pkt_utxo_sync::{UtxoSyncDb, WireTx, wire_txid};
use crate::pkt_wire::WireBlockHeader;
use sha2::{Digest, Sha256};

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidateError {
    /// No transactions in block
    EmptyBlock,
    /// First TX is not coinbase
    MissingCoinbase,
    /// Coinbase appears after index 0
    ExtraCoinbase { tx_index: usize },
    /// Coinbase has more than one input (PacketCrypt announcement inputs are fine — see note)
    /// Actually PKT coinbase can have many inputs (announcement proofs). We skip this check.
    /// A referenced UTXO does not exist in the DB
    MissingUtxo { txid_hex: String, vout: u32 },
    /// Same (txid, vout) spent twice within the block
    DoubleSpend { txid_hex: String, vout: u32 },
    /// Output value exceeds input value (overflow / theft)
    ValueOverflow { tx_index: usize, input_sum: u64, output_sum: u64 },
    /// Computed merkle root does not match header
    BadMerkleRoot { expected: String, got: String },
    /// DB error during lookup
    Db(String),
}

impl std::fmt::Display for ValidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyBlock                     => write!(f, "block has no transactions"),
            Self::MissingCoinbase                => write!(f, "first tx is not coinbase"),
            Self::ExtraCoinbase { tx_index }     => write!(f, "extra coinbase at tx[{tx_index}]"),
            Self::MissingUtxo { txid_hex, vout } => write!(f, "missing UTXO {txid_hex}:{vout}"),
            Self::DoubleSpend { txid_hex, vout } => write!(f, "double-spend {txid_hex}:{vout}"),
            Self::ValueOverflow { tx_index, input_sum, output_sum } =>
                write!(f, "tx[{tx_index}] outputs({output_sum}) > inputs({input_sum})"),
            Self::BadMerkleRoot { expected, got } =>
                write!(f, "merkle root mismatch: header={expected} computed={got}"),
            Self::Db(e)                          => write!(f, "db error: {e}"),
        }
    }
}

impl From<SyncError> for ValidateError {
    fn from(e: SyncError) -> Self { Self::Db(e.to_string()) }
}

// ── Public result ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct BlockValidation {
    pub height:    u64,
    pub tx_count:  usize,
    /// Total fee collected across all non-coinbase txs (satoshis)
    pub total_fee: u64,
}

// ── Merkle root ───────────────────────────────────────────────────────────────

/// Bitcoin-style double SHA-256 (must match `pkt_block_sync::merkle_root` and `wire_txid`).
fn sha256d(data: &[u8]) -> [u8; 32] {
    let h1 = Sha256::digest(data);
    Sha256::digest(h1).into()
}

/// Merkle root from wire txids (SHA256d pairs), identical to block sync.
pub fn compute_merkle_root(txids: &[[u8; 32]]) -> [u8; 32] {
    if txids.is_empty() {
        return [0u8; 32];
    }
    let mut row: Vec<[u8; 32]> = txids.to_vec();
    while row.len() > 1 {
        if row.len() % 2 == 1 {
            row.push(*row.last().unwrap());
        }
        row = row
            .chunks(2)
            .map(|pair| {
                let mut buf = [0u8; 64];
                buf[..32].copy_from_slice(&pair[0]);
                buf[32..].copy_from_slice(&pair[1]);
                sha256d(&buf)
            })
            .collect();
    }
    row[0]
}

// ── Core validation ───────────────────────────────────────────────────────────

/// Validate all transactions in a block against the UTXO DB.
///
/// Checks:
/// - coinbase rules
/// - UTXO existence for every non-coinbase input
/// - no double-spend within the block
/// - value conservation per non-coinbase tx
/// - merkle root matches header
///
/// Does NOT apply any changes to the DB (read-only).
pub fn validate_block(
    txns:   &[WireTx],
    header: &WireBlockHeader,
    height: u64,
    utxo:   &UtxoSyncDb,
) -> Result<BlockValidation, ValidateError> {
    if txns.is_empty() {
        return Err(ValidateError::EmptyBlock);
    }

    // Rule 1: first tx must be coinbase
    if !txns[0].is_coinbase() {
        return Err(ValidateError::MissingCoinbase);
    }

    // Rule 2: no other tx may be coinbase
    for (i, tx) in txns.iter().enumerate().skip(1) {
        if tx.is_coinbase() {
            return Err(ValidateError::ExtraCoinbase { tx_index: i });
        }
    }

    // Collect all txids for merkle root computation
    let txids: Vec<[u8; 32]> = txns.iter().map(wire_txid).collect();

    // Track (txid_bytes, vout) pairs spent in this block to detect double-spends
    let mut spent_in_block: HashSet<([u8; 32], u32)> = HashSet::new();
    let mut total_fee: u64 = 0;

    // Validate each non-coinbase tx
    for (tx_idx, tx) in txns.iter().enumerate().skip(1) {
        let mut input_sum:  u64 = 0;
        let mut output_sum: u64 = 0;

        for inp in &tx.inputs {
            let key = (inp.prev_txid, inp.prev_vout);

            // Double-spend check within block
            if !spent_in_block.insert(key) {
                return Err(ValidateError::DoubleSpend {
                    txid_hex: hex::encode(inp.prev_txid),
                    vout:     inp.prev_vout,
                });
            }

            // UTXO existence check
            let entry = utxo.get_utxo(&inp.prev_txid, inp.prev_vout)
                .map_err(|e| ValidateError::Db(e.to_string()))?;

            match entry {
                None => return Err(ValidateError::MissingUtxo {
                    txid_hex: hex::encode(inp.prev_txid),
                    vout:     inp.prev_vout,
                }),
                Some(e) => input_sum = input_sum.saturating_add(e.value),
            }
        }

        for out in &tx.outputs {
            output_sum = output_sum.saturating_add(out.value);
        }

        // Value conservation: inputs must cover outputs
        if output_sum > input_sum {
            return Err(ValidateError::ValueOverflow {
                tx_index:   tx_idx,
                input_sum,
                output_sum,
            });
        }

        total_fee = total_fee.saturating_add(input_sum - output_sum);
    }

    // Merkle root check
    let computed = compute_merkle_root(&txids);
    if computed != header.merkle_root {
        return Err(ValidateError::BadMerkleRoot {
            expected: hex::encode(header.merkle_root),
            got:      hex::encode(computed),
        });
    }

    Ok(BlockValidation {
        height,
        tx_count:  txns.len(),
        total_fee,
    })
}

/// Convenience wrapper: validate only a single transaction outside a block context
/// (e.g. mempool acceptance). Checks UTXO existence + value conservation only.
pub fn validate_tx(
    tx:   &WireTx,
    utxo: &UtxoSyncDb,
) -> Result<u64, ValidateError> {
    if tx.is_coinbase() {
        // Coinbase txs are only valid inside a block
        return Ok(0);
    }
    let mut input_sum:  u64 = 0;
    let mut output_sum: u64 = 0;

    for inp in &tx.inputs {
        let entry = utxo.get_utxo(&inp.prev_txid, inp.prev_vout)
            .map_err(|e| ValidateError::Db(e.to_string()))?;
        match entry {
            None => return Err(ValidateError::MissingUtxo {
                txid_hex: hex::encode(inp.prev_txid),
                vout:     inp.prev_vout,
            }),
            Some(e) => input_sum = input_sum.saturating_add(e.value),
        }
    }
    for out in &tx.outputs {
        output_sum = output_sum.saturating_add(out.value);
    }
    if output_sum > input_sum {
        return Err(ValidateError::ValueOverflow {
            tx_index: 0,
            input_sum,
            output_sum,
        });
    }
    Ok(input_sum - output_sum) // fee
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkt_utxo_sync::{WireTxIn, WireTxOut, UtxoSyncDb};

    fn coinbase_tx() -> WireTx {
        WireTx {
            version:  1,
            inputs:   vec![WireTxIn {
                prev_txid:  [0u8; 32],
                prev_vout:  0xffff_ffff,
                script_sig: vec![0xab],
                sequence:   0xffff_ffff,
            }],
            outputs: vec![WireTxOut { value: 4096 * 1_073_741_824, script_pubkey: vec![0x51] }],
            locktime: 0,
        }
    }

    fn regular_tx(prev_txid: [u8; 32], prev_vout: u32, out_value: u64) -> WireTx {
        WireTx {
            version:  1,
            inputs:   vec![WireTxIn { prev_txid, prev_vout, script_sig: vec![], sequence: 0xffff_ffff }],
            outputs:  vec![WireTxOut { value: out_value, script_pubkey: vec![0x51] }],
            locktime: 0,
        }
    }

    fn header_with_merkle(merkle_root: [u8; 32]) -> WireBlockHeader {
        WireBlockHeader {
            version:     1,
            prev_block:  [0u8; 32],
            merkle_root,
            timestamp:   0,
            bits:        0,
            nonce:       0,
        }
    }

    // ── Merkle root ───────────────────────────────────────────────────────────

    #[test]
    fn merkle_single_tx() {
        let txid = [1u8; 32];
        // Single tx: merkle root = txid itself? No — it's hashed with a duplicate.
        // Single element: layer stays as [txid], loop exits immediately.
        let root = compute_merkle_root(&[txid]);
        assert_eq!(root, txid);
    }

    #[test]
    fn merkle_two_txs_deterministic() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let r1 = compute_merkle_root(&[a, b]);
        let r2 = compute_merkle_root(&[a, b]);
        assert_eq!(r1, r2);
        assert_ne!(r1, [0u8; 32]);
    }

    #[test]
    fn merkle_odd_count_duplicates_last() {
        let txids = [[1u8; 32], [2u8; 32], [3u8; 32]];
        // 3 → pad to 4 (duplicate last) → 2 pairs → 1 root
        let root = compute_merkle_root(&txids);
        assert_ne!(root, [0u8; 32]);
    }

    #[test]
    fn merkle_empty_is_zero() {
        assert_eq!(compute_merkle_root(&[]), [0u8; 32]);
    }

    // ── Coinbase rules ────────────────────────────────────────────────────────

    #[test]
    fn validate_rejects_empty_block() {
        let db = UtxoSyncDb::open_temp().unwrap();
        let hdr = header_with_merkle([0u8; 32]);
        let err = validate_block(&[], &hdr, 1, &db).unwrap_err();
        assert_eq!(err, ValidateError::EmptyBlock);
    }

    #[test]
    fn validate_rejects_missing_coinbase() {
        let db = UtxoSyncDb::open_temp().unwrap();
        let prev = [0xaau8; 32];
        // Insert the UTXO so UTXO check passes, but first tx is not coinbase
        db.insert_utxo(&prev, 0, &WireTxOut { value: 1000, script_pubkey: vec![] }, 0).unwrap();
        let tx = regular_tx(prev, 0, 900);
        let txid = wire_txid(&tx);
        let hdr  = header_with_merkle(compute_merkle_root(&[txid]));
        let err  = validate_block(&[tx], &hdr, 1, &db).unwrap_err();
        assert_eq!(err, ValidateError::MissingCoinbase);
    }

    #[test]
    fn validate_rejects_extra_coinbase() {
        let db  = UtxoSyncDb::open_temp().unwrap();
        let cb1 = coinbase_tx();
        let cb2 = coinbase_tx();
        let txids = [wire_txid(&cb1), wire_txid(&cb2)];
        let hdr = header_with_merkle(compute_merkle_root(&txids));
        let err = validate_block(&[cb1, cb2], &hdr, 1, &db).unwrap_err();
        assert!(matches!(err, ValidateError::ExtraCoinbase { tx_index: 1 }));
    }

    // ── UTXO checks ───────────────────────────────────────────────────────────

    #[test]
    fn validate_rejects_missing_utxo() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let cb   = coinbase_tx();
        let bad  = regular_tx([0xbbu8; 32], 0, 100);
        let txids = [wire_txid(&cb), wire_txid(&bad)];
        let hdr  = header_with_merkle(compute_merkle_root(&txids));
        let err  = validate_block(&[cb, bad], &hdr, 1, &db).unwrap_err();
        assert!(matches!(err, ValidateError::MissingUtxo { .. }));
    }

    #[test]
    fn validate_rejects_double_spend_in_block() {
        let db    = UtxoSyncDb::open_temp().unwrap();
        let prev: [u8; 32] = [0xcc; 32];
        db.insert_utxo(&prev, 0, &WireTxOut { value: 2000, script_pubkey: vec![] }, 0).unwrap();

        let cb   = coinbase_tx();
        let tx1  = regular_tx(prev, 0, 900);
        let tx2  = regular_tx(prev, 0, 900); // same input → double-spend
        let txids = [wire_txid(&cb), wire_txid(&tx1), wire_txid(&tx2)];
        let hdr  = header_with_merkle(compute_merkle_root(&txids));
        let err  = validate_block(&[cb, tx1, tx2], &hdr, 1, &db).unwrap_err();
        assert!(matches!(err, ValidateError::DoubleSpend { .. }));
    }

    #[test]
    fn validate_rejects_value_overflow() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let prev: [u8; 32] = [0xdd; 32];
        db.insert_utxo(&prev, 0, &WireTxOut { value: 100, script_pubkey: vec![] }, 0).unwrap();

        let cb   = coinbase_tx();
        let tx   = regular_tx(prev, 0, 200); // output > input
        let txids = [wire_txid(&cb), wire_txid(&tx)];
        let hdr  = header_with_merkle(compute_merkle_root(&txids));
        let err  = validate_block(&[cb, tx], &hdr, 1, &db).unwrap_err();
        assert!(matches!(err, ValidateError::ValueOverflow { .. }));
    }

    // ── Merkle root mismatch ──────────────────────────────────────────────────

    #[test]
    fn validate_rejects_bad_merkle_root() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let cb   = coinbase_tx();
        let hdr  = header_with_merkle([0xffu8; 32]); // wrong merkle root
        let err  = validate_block(&[cb], &hdr, 1, &db).unwrap_err();
        assert!(matches!(err, ValidateError::BadMerkleRoot { .. }));
    }

    // ── Happy path ────────────────────────────────────────────────────────────

    #[test]
    fn validate_coinbase_only_block() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let cb   = coinbase_tx();
        let root = compute_merkle_root(&[wire_txid(&cb)]);
        let hdr  = header_with_merkle(root);
        let result = validate_block(&[cb], &hdr, 1, &db).unwrap();
        assert_eq!(result.tx_count, 1);
        assert_eq!(result.total_fee, 0);
    }

    #[test]
    fn validate_block_with_regular_tx() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let prev = [0xeeu8; 32];
        db.insert_utxo(&prev, 0, &WireTxOut { value: 1000, script_pubkey: vec![] }, 0).unwrap();

        let cb  = coinbase_tx();
        let tx  = regular_tx(prev, 0, 900); // fee = 100
        let txids = [wire_txid(&cb), wire_txid(&tx)];
        let hdr = header_with_merkle(compute_merkle_root(&txids));
        let result = validate_block(&[cb, tx], &hdr, 5, &db).unwrap();
        assert_eq!(result.tx_count, 2);
        assert_eq!(result.total_fee, 100);
        assert_eq!(result.height, 5);
    }

    #[test]
    fn validate_tx_fee_calculation() {
        let db   = UtxoSyncDb::open_temp().unwrap();
        let prev = [0xffu8; 32];
        db.insert_utxo(&prev, 0, &WireTxOut { value: 5000, script_pubkey: vec![] }, 0).unwrap();
        let tx  = regular_tx(prev, 0, 4500);
        let fee = validate_tx(&tx, &db).unwrap();
        assert_eq!(fee, 500);
    }

    #[test]
    fn validate_tx_coinbase_returns_zero_fee() {
        let db  = UtxoSyncDb::open_temp().unwrap();
        let fee = validate_tx(&coinbase_tx(), &db).unwrap();
        assert_eq!(fee, 0);
    }
}
