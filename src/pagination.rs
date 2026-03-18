#![allow(dead_code)]
//! v8.7 — Cursor-based Pagination + CSV Export
//!
//! Provides cursor-based pagination (by block height) and CSV export for
//! blocks and transactions.  Used by the PKTScan REST API.
//!
//! Cursor pagination:
//!   paginate_blocks(chain, from_height, limit) → &[Block]  (newest-first from cursor)
//!   paginate_txs(chain, from_height, limit)    → Vec<TxRow> (newest-first)
//!
//! CSV export:
//!   blocks_to_csv(blocks)                → CSV string
//!   tx_rows_to_csv(rows)                 → CSV string
//!
//! API endpoints added in pktscan_api.rs:
//!   GET /api/blocks?from=<height>&limit=20
//!   GET /api/txs?from=<height>&limit=20
//!   GET /api/blocks.csv?from=<height>&limit=100
//!   GET /api/txs.csv?from=<height>&limit=100

use crate::block::Block;
use crate::transaction::Transaction;

// ─── TxRow ────────────────────────────────────────────────────────────────────

/// A flattened transaction row for pagination / CSV.
#[derive(Debug, Clone)]
pub struct TxRow<'a> {
    pub block_height:    u64,
    pub block_timestamp: i64,
    pub tx:              &'a Transaction,
}

// ─── Cursor pagination ────────────────────────────────────────────────────────

/// Return up to `limit` blocks starting from (and including) `from_height`,
/// going downward (newest-first).  `None` means start from the tip.
pub fn paginate_blocks(chain: &[Block], from_height: Option<u64>, limit: usize) -> &[Block] {
    if chain.is_empty() { return &[]; }
    let limit = limit.min(500);

    // Find the starting index in the chain slice (chain is ordered 0..tip)
    let start_idx = match from_height {
        None => chain.len() - 1,
        Some(h) => {
            // Find the last block with index <= h
            match chain.iter().rposition(|b| b.index <= h) {
                Some(i) => i,
                None    => return &[],
            }
        }
    };

    // We want newest-first: start_idx..=0 (descending)
    // Return a contiguous slice that covers start_idx downward by `limit`
    let end_idx = start_idx.saturating_sub(limit - 1);
    // Reverse slice: chain[end_idx..=start_idx] reversed
    // We can't return a reversed slice directly; return the slice and note
    // callers iterate with .rev()
    &chain[end_idx..=start_idx]
}

/// Newest block height just below `from_height` (next cursor value).
/// Returns `None` if there are no older blocks.
pub fn next_block_cursor(chain: &[Block], from_height: Option<u64>, limit: usize) -> Option<u64> {
    if chain.is_empty() { return None; }
    let limit = limit.min(500);
    let start_h = from_height.unwrap_or_else(|| chain.last().unwrap().index);
    let start_h_saturated = start_h.saturating_sub(limit as u64);
    if start_h_saturated == 0 { None } else { Some(start_h_saturated) }
}

/// Return up to `limit` TxRows starting from blocks at or below `from_height`,
/// newest-first.
pub fn paginate_txs<'a>(
    chain:       &'a [Block],
    from_height: Option<u64>,
    limit:       usize,
) -> Vec<TxRow<'a>> {
    let limit = limit.min(500);
    let cutoff = from_height.unwrap_or(u64::MAX);

    chain.iter().rev()
        .filter(|b| b.index <= cutoff)
        .flat_map(|b| b.transactions.iter().map(move |tx| TxRow {
            block_height:    b.index,
            block_timestamp: b.timestamp,
            tx,
        }))
        .take(limit)
        .collect()
}

// ─── CSV export ───────────────────────────────────────────────────────────────

/// Serialize `blocks` (in the order given) to CSV.
/// Columns: height, hash, prev_hash, timestamp, tx_count, nonce
pub fn blocks_to_csv(blocks: &[Block]) -> String {
    let mut out = String::from("height,hash,prev_hash,timestamp,tx_count,nonce\n");
    for b in blocks.iter().rev() {   // newest-first in output
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            b.index, b.hash, b.prev_hash, b.timestamp,
            b.transactions.len(), b.nonce,
        ));
    }
    out
}

/// Serialize `rows` to CSV.
/// Columns: block_height, block_timestamp, tx_id, is_coinbase, fee, output_total, input_count, output_count
pub fn tx_rows_to_csv(rows: &[TxRow<'_>]) -> String {
    let mut out = String::from(
        "block_height,block_timestamp,tx_id,is_coinbase,fee,output_total,input_count,output_count\n"
    );
    for r in rows {
        let output_total: u64 = r.tx.outputs.iter().map(|o| o.amount).sum();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            r.block_height, r.block_timestamp,
            r.tx.tx_id, r.tx.is_coinbase, r.tx.fee,
            output_total, r.tx.inputs.len(), r.tx.outputs.len(),
        ));
    }
    out
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Block;
    use crate::chain::Blockchain;
    use crate::transaction::Transaction;

    const ADDR: &str = "aabbccdd00112233445566778899aabbccddeeff";

    fn make_chain(n: u64) -> Blockchain {
        let mut bc = Blockchain::new();
        for i in 1..=n {
            let cb = Transaction::coinbase_at(ADDR, 0, i);
            let mut blk = Block::new(i, vec![cb], bc.chain.last().unwrap().hash.clone());
            blk.mine(2);
            bc.chain.push(blk);
        }
        bc
    }

    // ── paginate_blocks ───────────────────────────────────────────────────

    #[test]
    fn test_paginate_blocks_from_none() {
        let bc = make_chain(5);
        // from=None → start from tip (height 5), limit 3 → heights 3,4,5
        let slice = paginate_blocks(&bc.chain, None, 3);
        assert_eq!(slice.len(), 3);
        assert_eq!(slice.last().unwrap().index, 5); // tip included
    }

    #[test]
    fn test_paginate_blocks_from_height() {
        let bc = make_chain(5);
        let slice = paginate_blocks(&bc.chain, Some(3), 2);
        assert!(slice.iter().all(|b| b.index <= 3));
        assert!(slice.len() <= 2);
    }

    #[test]
    fn test_paginate_blocks_limit_respected() {
        let bc = make_chain(10);
        let slice = paginate_blocks(&bc.chain, None, 4);
        assert_eq!(slice.len(), 4);
    }

    #[test]
    fn test_paginate_blocks_empty_chain() {
        let slice = paginate_blocks(&[], None, 10);
        assert!(slice.is_empty());
    }

    #[test]
    fn test_paginate_blocks_out_of_range() {
        let bc = make_chain(3);
        // from_height=99 — no block has index <= 99 — should start from tip
        let slice = paginate_blocks(&bc.chain, Some(99), 5);
        // Should return up to 5 blocks ending at the last block with index <= 99
        assert!(!slice.is_empty());
    }

    #[test]
    fn test_paginate_blocks_limit_capped_500() {
        let bc = make_chain(5);
        let slice = paginate_blocks(&bc.chain, None, 9999);
        assert!(slice.len() <= 500);
    }

    // ── paginate_txs ──────────────────────────────────────────────────────

    #[test]
    fn test_paginate_txs_from_none() {
        let bc = make_chain(4);
        let rows = paginate_txs(&bc.chain, None, 10);
        // Should include txs from all blocks (genesis has 0 txs in this chain)
        assert!(rows.len() >= 1);
    }

    #[test]
    fn test_paginate_txs_from_height() {
        let bc = make_chain(4);
        let rows = paginate_txs(&bc.chain, Some(2), 10);
        for r in &rows {
            assert!(r.block_height <= 2);
        }
    }

    #[test]
    fn test_paginate_txs_limit_respected() {
        let bc = make_chain(5);
        let rows = paginate_txs(&bc.chain, None, 2);
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn test_paginate_txs_newest_first() {
        let bc = make_chain(4);
        let rows = paginate_txs(&bc.chain, None, 10);
        if rows.len() >= 2 {
            assert!(rows[0].block_height >= rows[1].block_height);
        }
    }

    // ── next_block_cursor ─────────────────────────────────────────────────

    #[test]
    fn test_next_cursor_returns_none_at_genesis() {
        let bc = make_chain(2);
        let c = next_block_cursor(&bc.chain, Some(2), 5);
        assert!(c.is_none()); // 2 - 5 = 0 → None
    }

    #[test]
    fn test_next_cursor_empty_chain() {
        let c = next_block_cursor(&[], None, 10);
        assert!(c.is_none());
    }

    // ── blocks_to_csv ─────────────────────────────────────────────────────

    #[test]
    fn test_blocks_to_csv_header() {
        let bc = make_chain(2);
        let csv = blocks_to_csv(&bc.chain);
        assert!(csv.starts_with("height,hash,prev_hash,timestamp,tx_count,nonce\n"));
    }

    #[test]
    fn test_blocks_to_csv_row_count() {
        let bc = make_chain(3);
        let csv = blocks_to_csv(&bc.chain);
        let rows = csv.lines().count();
        assert_eq!(rows, 1 + bc.chain.len()); // header + one per block
    }

    #[test]
    fn test_blocks_to_csv_no_empty() {
        let bc = make_chain(2);
        let csv = blocks_to_csv(&bc.chain);
        assert!(!csv.is_empty());
    }

    // ── tx_rows_to_csv ────────────────────────────────────────────────────

    #[test]
    fn test_tx_rows_to_csv_header() {
        let bc = make_chain(2);
        let rows = paginate_txs(&bc.chain, None, 10);
        let csv = tx_rows_to_csv(&rows);
        assert!(csv.starts_with("block_height,block_timestamp,tx_id,is_coinbase,fee,output_total,input_count,output_count\n"));
    }

    #[test]
    fn test_tx_rows_to_csv_row_count() {
        let bc = make_chain(3);
        let rows = paginate_txs(&bc.chain, None, 100);
        let csv = tx_rows_to_csv(&rows);
        let lines = csv.lines().count();
        assert_eq!(lines, 1 + rows.len());
    }

    #[test]
    fn test_tx_rows_to_csv_coinbase_flagged() {
        let bc = make_chain(2);
        let rows = paginate_txs(&bc.chain, None, 10);
        let csv = tx_rows_to_csv(&rows);
        // coinbase = true in every row
        for line in csv.lines().skip(1) {
            assert!(line.contains("true"), "coinbase field should be true");
        }
    }
}
