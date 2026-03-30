#![allow(dead_code)]
//! v23.6 — Wire Mempool Bridge
//!
//! Chuyển đổi wire TX từ `MempoolDb` (RocksDB, populated bởi pkt_sync) sang
//! internal `Transaction` format để miner template server có thể include chúng
//! vào block template.
//!
//! PKT wire sử dụng SHA256d TXID (Bitcoin-compatible); internal chain dùng blake3.
//! Hàm `load_wire_mempool_txs()` trả về `Vec<Transaction>` với tx_id = SHA256d hex
//! đảm bảo dedup hoạt động đúng khi merge với bc.mempool.

use std::path::Path;

use crate::script::{Opcode, Script};
use crate::transaction::{TxInput, TxOutput, Transaction};
use crate::pkt_utxo_sync::{decode_wire_tx, WireTx, WireTxIn, WireTxOut};

// ── Script conversion ─────────────────────────────────────────────────────────

/// Parse raw wire P2PKH scriptPubKey bytes thành internal `Script`.
///
/// Wire P2PKH = `76 a9 14 <20 bytes> 88 ac`
/// Wire P2SH  = `a9 14 <20 bytes> 87`
/// Wire P2WPKH = `00 14 <20 bytes>`
/// Wire P2TR   = `51 20 <32 bytes>`
/// Fallback    = single OpPushData(raw_bytes)
fn wire_script_to_script(raw: &[u8]) -> Script {
    // P2PKH: OP_DUP OP_HASH160 OP_PUSH20 <20> OP_EQUALVERIFY OP_CHECKSIG
    if raw.len() == 25
        && raw[0] == 0x76
        && raw[1] == 0xa9
        && raw[2] == 0x14
        && raw[23] == 0x88
        && raw[24] == 0xac
    {
        let hash_hex = hex::encode(&raw[3..23]);
        return Script::p2pkh_pubkey(&hash_hex);
    }

    // P2SH: OP_HASH160 OP_PUSH20 <20> OP_EQUAL
    if raw.len() == 23
        && raw[0] == 0xa9
        && raw[1] == 0x14
        && raw[22] == 0x87
    {
        let hash_hex = hex::encode(&raw[2..22]);
        return Script::p2sh_pubkey(&hash_hex);
    }

    // P2WPKH: OP_0 OP_PUSH20 <20>
    if raw.len() == 22 && raw[0] == 0x00 && raw[1] == 0x14 {
        let hash_hex = hex::encode(&raw[2..22]);
        return Script::p2wpkh_pubkey(&hash_hex);
    }

    // P2TR: OP_1 OP_PUSH32 <32>
    if raw.len() == 34 && raw[0] == 0x51 && raw[1] == 0x20 {
        let key_hex = hex::encode(&raw[2..34]);
        return Script::p2tr_pubkey(&key_hex);
    }

    // Fallback: wrap raw bytes as single push
    Script::new(vec![Opcode::OpPushData(raw.to_vec())])
}

/// Parse wire scriptSig bytes thành internal `Script`.
/// Script::empty() khi script_sig trống (P2WPKH/P2TR native SegWit).
fn wire_scriptsig_to_script(raw: &[u8]) -> Script {
    if raw.is_empty() {
        return Script::empty();
    }
    Script::new(vec![Opcode::OpPushData(raw.to_vec())])
}

// ── WireTx → Transaction ──────────────────────────────────────────────────────

/// Compute SHA256d TXID của raw wire TX bytes, trả về hex display format (reversed).
fn compute_wire_txid(raw: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    let first  = Sha256::digest(raw);
    let second = Sha256::digest(&first);
    let mut bytes: [u8; 32] = second.into();
    bytes.reverse(); // display format: little-endian reversed
    hex::encode(bytes)
}

/// Chuyển `WireTxIn` → `TxInput`.
fn wire_txin_to_txinput(inp: &WireTxIn) -> TxInput {
    // prev_txid trong wire là LE → reversed = display TXID
    let mut prev_bytes = inp.prev_txid;
    prev_bytes.reverse();
    let prev_txid_hex = hex::encode(prev_bytes);

    TxInput {
        tx_id:        prev_txid_hex,
        output_index: inp.prev_vout as usize,
        script_sig:   wire_scriptsig_to_script(&inp.script_sig),
        sequence:     inp.sequence,
        witness:      vec![],
    }
}

/// Chuyển `WireTxOut` → `TxOutput`.
fn wire_txout_to_txoutput(out: &WireTxOut) -> TxOutput {
    TxOutput {
        amount:       out.value,
        script_pubkey: wire_script_to_script(&out.script_pubkey),
    }
}

/// Chuyển `WireTx` sang internal `Transaction`.
///
/// `raw`           = raw wire bytes (dùng để tính SHA256d TXID)
/// `fee_rate_msat` = msat/vB từ MempoolDb
pub fn wire_tx_to_transaction(wire: &WireTx, raw: &[u8], fee_rate_msat: u64) -> Transaction {
    let tx_id      = compute_wire_txid(raw);
    let inputs     = wire.inputs.iter().map(wire_txin_to_txinput).collect::<Vec<_>>();
    let outputs    = wire.outputs.iter().map(wire_txout_to_txoutput).collect::<Vec<_>>();
    let is_coinbase = wire.is_coinbase();

    // Ước tính fee: fee_rate (msat/vB) × size / 1000
    let fee = (fee_rate_msat.saturating_mul(raw.len() as u64)) / 1000;

    Transaction {
        tx_id:      tx_id.clone(),
        wtx_id:     tx_id,          // không có witness segment riêng
        inputs,
        outputs,
        is_coinbase,
        fee,
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Đọc tối đa `limit` TX từ `MempoolDb` tại `mempool_path` và chuyển sang
/// `Vec<Transaction>` để merge vào miner block template.
///
/// Graceful: nếu DB không tồn tại hoặc có lỗi → trả về `vec![]`.
pub fn load_wire_mempool_txs(mempool_path: &Path, limit: usize) -> Vec<Transaction> {
    use crate::pkt_mempool_sync::MempoolDb;

    if limit == 0 { return vec![]; }

    let mdb = match MempoolDb::open_read_only(mempool_path) {
        Ok(db)  => db,
        Err(_)  => return vec![],
    };

    let pending = match mdb.get_pending(limit) {
        Ok(p)   => p,
        Err(_)  => return vec![],
    };

    let mut result = Vec::with_capacity(pending.len());

    for info in &pending {
        let (raw, fee_rate, _ts) = match mdb.get_tx_raw(&info.txid) {
            Some(r) => r,
            None    => continue,
        };

        let mut pos = 0usize;
        let wire = match decode_wire_tx(&raw, &mut pos) {
            Ok(tx)  => tx,
            Err(_)  => continue,
        };

        if wire.is_coinbase() { continue; } // skip spurious coinbase TXs

        result.push(wire_tx_to_transaction(&wire, &raw, fee_rate));
    }

    result
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkt_utxo_sync::{WireTxIn, WireTxOut, WireTx, encode_wire_tx};

    fn sample_wiretx() -> WireTx {
        WireTx {
            version:  1,
            inputs:   vec![WireTxIn {
                prev_txid:  [0xab; 32],
                prev_vout:  0,
                script_sig: vec![0x76, 0x01, 0xff],
                sequence:   0xffffffff,
            }],
            outputs:  vec![WireTxOut {
                value:        500_000_000,
                // valid P2PKH: 76 a9 14 <20 bytes> 88 ac
                script_pubkey: {
                    let mut s = vec![0x76u8, 0xa9, 0x14];
                    s.extend_from_slice(&[0x11u8; 20]);
                    s.push(0x88);
                    s.push(0xac);
                    s
                },
            }],
            locktime: 0,
        }
    }

    // ── wire_script_to_script ─────────────────────────────────────────────────

    #[test]
    fn test_p2pkh_script_converted() {
        let mut raw = vec![0x76u8, 0xa9, 0x14];
        raw.extend_from_slice(&[0x22u8; 20]);
        raw.push(0x88);
        raw.push(0xac);
        let s = wire_script_to_script(&raw);
        // Should produce OP_DUP OP_HASH160 OpPushData(20) OP_EQUALVERIFY OP_CHECKSIG
        assert_eq!(s.ops.len(), 5);
        assert!(matches!(s.ops[0], Opcode::OpDup));
        assert!(matches!(s.ops[1], Opcode::OpHash160));
        assert!(matches!(s.ops[4], Opcode::OpCheckSig));
    }

    #[test]
    fn test_p2sh_script_converted() {
        let mut raw = vec![0xa9u8, 0x14];
        raw.extend_from_slice(&[0x33u8; 20]);
        raw.push(0x87);
        let s = wire_script_to_script(&raw);
        // Should produce OP_HASH160 OpPushData(20) OP_EQUAL
        assert_eq!(s.ops.len(), 3);
        assert!(matches!(s.ops[0], Opcode::OpHash160));
    }

    #[test]
    fn test_p2wpkh_script_converted() {
        let mut raw = vec![0x00u8, 0x14];
        raw.extend_from_slice(&[0x44u8; 20]);
        let s = wire_script_to_script(&raw);
        // P2WPKH: OP_0 OpPushData(20)
        assert_eq!(s.ops.len(), 2);
        assert!(matches!(s.ops[0], Opcode::Op0));
    }

    #[test]
    fn test_fallback_script_wrapped() {
        let raw = vec![0xde, 0xad, 0xbe, 0xef];
        let s = wire_script_to_script(&raw);
        assert_eq!(s.ops.len(), 1);
        assert!(matches!(&s.ops[0], Opcode::OpPushData(d) if d == &raw));
    }

    #[test]
    fn test_empty_scriptsig_returns_empty() {
        assert!(wire_scriptsig_to_script(&[]).ops.is_empty());
    }

    // ── compute_wire_txid ─────────────────────────────────────────────────────

    #[test]
    fn test_compute_wire_txid_is_64_hex_chars() {
        let raw = encode_wire_tx(&sample_wiretx());
        let txid = compute_wire_txid(&raw);
        assert_eq!(txid.len(), 64);
        assert!(txid.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_compute_wire_txid_deterministic() {
        let raw = encode_wire_tx(&sample_wiretx());
        assert_eq!(compute_wire_txid(&raw), compute_wire_txid(&raw));
    }

    #[test]
    fn test_compute_wire_txid_different_inputs() {
        let raw1 = encode_wire_tx(&sample_wiretx());
        let mut tx2 = sample_wiretx();
        tx2.outputs[0].value = 1;
        let raw2 = encode_wire_tx(&tx2);
        assert_ne!(compute_wire_txid(&raw1), compute_wire_txid(&raw2));
    }

    // ── wire_tx_to_transaction ────────────────────────────────────────────────

    #[test]
    fn test_wire_tx_to_transaction_txid_correct() {
        let wire = sample_wiretx();
        let raw  = encode_wire_tx(&wire);
        let tx   = wire_tx_to_transaction(&wire, &raw, 1000);
        assert_eq!(tx.tx_id, compute_wire_txid(&raw));
        assert_eq!(tx.tx_id, tx.wtx_id);
    }

    #[test]
    fn test_wire_tx_to_transaction_not_coinbase() {
        let wire = sample_wiretx();
        let raw  = encode_wire_tx(&wire);
        let tx   = wire_tx_to_transaction(&wire, &raw, 1000);
        assert!(!tx.is_coinbase);
    }

    #[test]
    fn test_wire_tx_to_transaction_fee_estimate() {
        let wire     = sample_wiretx();
        let raw      = encode_wire_tx(&wire);
        let fee_rate = 2000u64; // 2 sat/vB = 2000 msat/vB
        let tx       = wire_tx_to_transaction(&wire, &raw, fee_rate);
        let expected = fee_rate * raw.len() as u64 / 1000;
        assert_eq!(tx.fee, expected);
    }

    #[test]
    fn test_wire_tx_to_transaction_input_count() {
        let wire = sample_wiretx();
        let raw  = encode_wire_tx(&wire);
        let tx   = wire_tx_to_transaction(&wire, &raw, 0);
        assert_eq!(tx.inputs.len(), 1);
    }

    #[test]
    fn test_wire_tx_to_transaction_output_count() {
        let wire = sample_wiretx();
        let raw  = encode_wire_tx(&wire);
        let tx   = wire_tx_to_transaction(&wire, &raw, 0);
        assert_eq!(tx.outputs.len(), 1);
    }

    #[test]
    fn test_wire_tx_to_transaction_output_amount() {
        let wire = sample_wiretx();
        let raw  = encode_wire_tx(&wire);
        let tx   = wire_tx_to_transaction(&wire, &raw, 0);
        assert_eq!(tx.outputs[0].amount, 500_000_000);
    }

    #[test]
    fn test_wire_tx_to_transaction_prev_txid_reversed() {
        // prev_txid = [0xab; 32] → reversed display = hex of reversed bytes
        let wire = sample_wiretx();
        let raw  = encode_wire_tx(&wire);
        let tx   = wire_tx_to_transaction(&wire, &raw, 0);
        // All bytes are 0xab so reversed is same
        assert_eq!(tx.inputs[0].tx_id, hex::encode([0xab; 32]));
    }

    // ── load_wire_mempool_txs ─────────────────────────────────────────────────

    #[test]
    fn test_load_from_nonexistent_path_returns_empty() {
        let path = std::path::Path::new("/nonexistent/pkt_bridge_test_mempooldb");
        let txs  = load_wire_mempool_txs(path, 100);
        assert!(txs.is_empty());
    }

    #[test]
    fn test_load_with_limit_zero_returns_empty() {
        let path = std::env::temp_dir().join("pkt_bridge_empty_test");
        let txs  = load_wire_mempool_txs(&path, 0);
        assert!(txs.is_empty());
    }

    #[test]
    fn test_load_from_empty_db_returns_empty() {
        use crate::pkt_mempool_sync::MempoolDb;
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n    = SEQ.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("pkt_bridge_test_empty_{}", n));
        MempoolDb::open(&path).unwrap();  // create empty DB
        let txs  = load_wire_mempool_txs(&path, 100);
        assert!(txs.is_empty());
    }

    #[test]
    fn test_load_returns_stored_tx() {
        use crate::pkt_mempool_sync::MempoolDb;
        use crate::pkt_utxo_sync::encode_wire_tx;
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n    = SEQ.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("pkt_bridge_test_load_{}", n));

        let wire    = sample_wiretx();
        let raw     = encode_wire_tx(&wire);
        let txid    = compute_wire_txid(&raw);
        let fee_rt  = 1500u64;
        let ts_ns   = 1_000_000_000u64;

        {
            let db = MempoolDb::open(&path).unwrap();
            db.put_tx(&txid, &raw, fee_rt, ts_ns).unwrap();
        }

        let txs = load_wire_mempool_txs(&path, 100);
        assert_eq!(txs.len(), 1);
        assert_eq!(txs[0].tx_id, txid);
        assert_eq!(txs[0].outputs[0].amount, 500_000_000);
    }

    #[test]
    fn test_load_respects_limit() {
        use crate::pkt_mempool_sync::MempoolDb;
        use crate::pkt_utxo_sync::encode_wire_tx;
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n    = SEQ.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("pkt_bridge_test_limit_{}", n));
        let db   = MempoolDb::open(&path).unwrap();

        for i in 0u64..5 {
            let mut wire = sample_wiretx();
            wire.outputs[0].value = i + 1;
            let raw   = encode_wire_tx(&wire);
            let txid  = compute_wire_txid(&raw);
            db.put_tx(&txid, &raw, 1000 + i, 1_000_000 + i).unwrap();
        }

        let txs = load_wire_mempool_txs(&path, 3);
        assert_eq!(txs.len(), 3);
    }

    #[test]
    fn test_load_skips_coinbase() {
        use crate::pkt_mempool_sync::MempoolDb;
        use crate::pkt_utxo_sync::encode_wire_tx;
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n    = SEQ.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("pkt_bridge_test_coinbase_{}", n));
        let db   = MempoolDb::open(&path).unwrap();

        // Coinbase TX: prev_txid=[0;32] prev_vout=0xffffffff
        let coinbase = WireTx {
            version:  1,
            inputs:   vec![WireTxIn {
                prev_txid:  [0u8; 32],
                prev_vout:  0xffffffff,
                script_sig: vec![0x03, 0x01, 0x02, 0x03],
                sequence:   0xffffffff,
            }],
            outputs:  vec![WireTxOut {
                value:        6_250_000_000,
                script_pubkey: vec![0x76, 0xa9, 0x14,
                    0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
                    0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
                    0x88, 0xac],
            }],
            locktime: 0,
        };
        let raw  = encode_wire_tx(&coinbase);
        let txid = compute_wire_txid(&raw);
        db.put_tx(&txid, &raw, 0, 1_000_000).unwrap();

        let txs = load_wire_mempool_txs(&path, 100);
        assert!(txs.is_empty(), "coinbase TXs should be skipped");
    }
}
