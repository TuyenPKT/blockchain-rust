#![allow(dead_code)]

/// v4.6 — Block Explorer CLI
///
/// Đọc chain từ RocksDB local storage (read-only).
///
/// Commands:
///   cargo run -- explorer chain             hiển thị chain summary + 5 blocks gần nhất
///   cargo run -- explorer block <height>    chi tiết một block
///   cargo run -- explorer tx <tx_id>        tìm transaction theo tx_id (hoặc prefix)
///   cargo run -- explorer balance <addr>    số dư của địa chỉ
///   cargo run -- explorer utxo <addr>       danh sách UTXO của địa chỉ

use crate::storage;

// ─── Entry point ──────────────────────────────────────────────────────────────

pub fn run_explorer(args: &[String]) {
    match args.get(2).map(|s| s.as_str()) {
        Some("chain")              => cmd_chain(),
        Some("block")              => {
            match args.get(3).and_then(|s| s.parse::<u64>().ok()) {
                Some(h) => cmd_block(h),
                None    => eprintln!("Usage: explorer block <height>"),
            }
        }
        Some("tx")                 => {
            match args.get(3) {
                Some(id) => cmd_tx(id),
                None     => eprintln!("Usage: explorer tx <tx_id>"),
            }
        }
        Some("balance")            => {
            match args.get(3) {
                Some(addr) => cmd_balance(addr),
                None       => eprintln!("Usage: explorer balance <addr>"),
            }
        }
        Some("utxo")               => {
            match args.get(3) {
                Some(addr) => cmd_utxo(addr),
                None       => eprintln!("Usage: explorer utxo <addr>"),
            }
        }
        _ => print_explorer_help(),
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/// explorer chain — tóm tắt toàn bộ chain
pub fn cmd_chain() {
    let (blocks, utxos) = load_or_exit();
    let height     = blocks.len().saturating_sub(1);
    let utxo_count = utxos.len();
    let supply: u64 = utxos.values().map(|o| o.amount).sum();
    let tip        = blocks.last().unwrap();

    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                  ⛓   Block Explorer                        ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  Height       : {}", height);
    println!("  Blocks       : {}", blocks.len());
    println!("  UTXO count   : {}", utxo_count);
    println!("  Total supply : {} paklets  ({:.8} PKT)", supply, supply as f64 / 1e8);
    println!("  Tip hash     : {}...{}", &tip.hash[..16], &tip.hash[56..]);
    println!();

    // Hiển thị 5 blocks gần nhất
    let start = blocks.len().saturating_sub(5);
    println!("  Recent blocks:");
    println!("  {:>6}  {:>6}  {:>64}  {:>4}", "height", "txs", "hash", "nonce");
    println!("  {}  {}  {}  {}", "─".repeat(6), "─".repeat(6), "─".repeat(64), "─".repeat(10));
    for b in &blocks[start..] {
        println!("  {:>6}  {:>6}  {}  {:>10}",
            b.index, b.transactions.len(), b.hash, b.nonce);
    }
    println!();
}

/// explorer block <height>
pub fn cmd_block(height: u64) {
    let (blocks, _) = load_or_exit();
    let block = match blocks.iter().find(|b| b.index == height) {
        Some(b) => b,
        None    => { eprintln!("Block #{} không tồn tại", height); return; }
    };

    println!();
    println!("  ┌─ Block #{}", block.index);
    println!("  │  hash       = {}", block.hash);
    println!("  │  prev_hash  = {}", block.prev_hash);
    println!("  │  timestamp  = {}", block.timestamp);
    println!("  │  nonce      = {}", block.nonce);
    println!("  │  txs        = {}", block.transactions.len());
    println!("  │  wit_root   = {}", block.witness_root);
    println!("  │");
    println!("  │  Transactions:");
    for (i, tx) in block.transactions.iter().enumerate() {
        let kind = if tx.is_coinbase { "coinbase" } else { "regular" };
        println!("  │  [{:>3}] {}  fee={} sat  ({} in → {} out)  [{}]",
            i, &tx.tx_id[..32], tx.fee,
            tx.inputs.len(), tx.outputs.len(), kind);
    }
    println!("  └─");
    println!();
}

/// explorer tx <tx_id_prefix>
pub fn cmd_tx(tx_id_prefix: &str) {
    let (blocks, _) = load_or_exit();
    let prefix = tx_id_prefix.to_lowercase();

    let mut found = false;
    'outer: for block in &blocks {
        for tx in &block.transactions {
            if tx.tx_id.starts_with(&prefix) {
                found = true;
                println!();
                println!("  ┌─ Transaction");
                println!("  │  tx_id      = {}", tx.tx_id);
                println!("  │  wtx_id     = {}", tx.wtx_id);
                println!("  │  fee        = {} sat", tx.fee);
                println!("  │  coinbase   = {}", tx.is_coinbase);
                println!("  │  block      = #{} ({}...)", block.index, &block.hash[..16]);
                println!("  │");
                println!("  │  Inputs ({}):", tx.inputs.len());
                for (i, inp) in tx.inputs.iter().enumerate() {
                    if inp.tx_id == "0000000000000000000000000000000000000000000000000000000000000000" {
                        println!("  │    [{}] coinbase", i);
                    } else {
                        println!("  │    [{}] {}:{}",
                            i, &inp.tx_id[..32], inp.output_index);
                    }
                }
                println!("  │  Outputs ({}):", tx.outputs.len());
                for (i, out) in tx.outputs.iter().enumerate() {
                    println!("  │    [{}] {} paklets  ({:.8} PKT)",
                        i, out.amount, out.amount as f64 / 1e8);
                }
                println!("  └─");
                println!();
                break 'outer;
            }
        }
    }
    if !found {
        println!("  Không tìm thấy TX với prefix '{}'", tx_id_prefix);
    }
}

/// explorer balance <addr>
pub fn cmd_balance(addr: &str) {
    let bc = storage::load_or_new();
    let balance = bc.utxo_set.balance_of(addr);
    println!();
    println!("  Address : {}", addr);
    println!("  Balance : {} paklets  ({:.8} PKT)", balance, balance as f64 / 1e8);
    println!();
}

/// explorer utxo <addr>
pub fn cmd_utxo(addr: &str) {
    let bc     = storage::load_or_new();
    let utxos  = bc.utxo_set.utxos_of(addr);
    let total: u64 = utxos.iter().map(|u| u.output.amount).sum();

    println!();
    println!("  UTXOs for {}", addr);
    println!("  {:>6}  {:>64}  {:>12}", "idx", "tx_id", "amount (sat)");
    println!("  {}  {}  {}", "─".repeat(6), "─".repeat(64), "─".repeat(12));
    if utxos.is_empty() {
        println!("  (none)");
    }
    for u in &utxos {
        println!("  {:>6}  {}  {:>12}", u.output_index, u.tx_id, u.output.amount);
    }
    println!("  {} UTXOs  |  total = {} paklets  ({:.8} PKT)",
        utxos.len(), total, total as f64 / 1e8);
    println!();
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn load_or_exit() -> (Vec<crate::block::Block>, std::collections::HashMap<String, crate::transaction::TxOutput>) {
    let blocks = match storage::load_chain() {
        Ok(Some(b)) => b,
        Ok(None)    => { eprintln!("Chưa có chain. Chạy miner trước: cargo run -- mine"); std::process::exit(1); }
        Err(e)      => { eprintln!("Lỗi load chain: {}", e); std::process::exit(1); }
    };
    let utxos = storage::load_utxo().unwrap_or_default().unwrap_or_default();
    (blocks, utxos)
}

fn print_explorer_help() {
    println!();
    println!("  Usage: cargo run -- explorer <command> [args]");
    println!();
    println!("  Commands:");
    println!("    chain              chain summary + 5 blocks gần nhất");
    println!("    block <height>     chi tiết block tại height");
    println!("    tx <tx_id>         tìm transaction (có thể dùng prefix)");
    println!("    balance <addr>     số dư của địa chỉ (pubkey_hash_hex)");
    println!("    utxo <addr>        danh sách UTXO của địa chỉ");
    println!();
}
