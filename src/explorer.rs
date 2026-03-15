#![allow(dead_code)]

/// v4.6 — Block Explorer CLI
///
/// Đọc chain từ RocksDB local storage (read-only), hoặc query remote node qua TCP.
///
/// Commands:
///   cargo run -- explorer chain [node]      chain summary + 5 blocks gần nhất
///   cargo run -- explorer block <height>    chi tiết một block
///   cargo run -- explorer tx <tx_id>        tìm transaction theo tx_id (hoặc prefix)
///   cargo run -- explorer balance <addr>    số dư của địa chỉ
///   cargo run -- explorer utxo <addr>       danh sách UTXO của địa chỉ
///
/// Nếu <node> được chỉ định (ví dụ: seed.testnet.oceif.com:8333), explorer sẽ
/// query remote node thay vì đọc local DB.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;
use crate::storage;
use crate::message::Message;

// ─── Entry point ──────────────────────────────────────────────────────────────

pub fn run_explorer(args: &[String]) {
    match args.get(2).map(|s| s.as_str()) {
        Some("chain") => {
            // Optional node addr: cargo run -- explorer chain seed.testnet.oceif.com:8333
            let node = args.get(3).map(|s| s.as_str())
                .or(Some(crate::miner::DEFAULT_NODE));
            cmd_chain(node);
        }
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
/// node_addr: nếu Some, query remote node; nếu None, đọc local DB
pub fn cmd_chain(node_addr: Option<&str>) {
    let (blocks, utxos) = match node_addr {
        Some(addr) => match fetch_remote_chain(addr) {
            Some(blocks) => {
                let utxos = std::collections::HashMap::new();
                (blocks, utxos)
            }
            None => {
                eprintln!("  Không kết nối được node {} — đọc local DB", addr);
                load_or_exit()
            }
        },
        None => load_or_exit(),
    };
    if blocks.is_empty() {
        println!("  (Chain trống — chưa có block nào sau genesis)");
        return;
    }
    let height     = blocks.last().map(|b| b.index).unwrap_or(0);
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
    match storage::load_chain() {
        Err(e) => { eprintln!("Lỗi load chain: {}", e); std::process::exit(1); }
        Ok(Some(blocks)) => {
            let utxos = storage::load_utxo().unwrap_or_default().unwrap_or_default();
            (blocks, utxos)
        }
        Ok(None) => {
            // Chưa mine lần nào — dùng genesis blockchain
            let bc = storage::load_or_new();
            (bc.chain, bc.utxo_set.utxos)
        }
    }
}

/// Fetch toàn bộ blocks từ remote node qua TCP GetBlocks
fn fetch_remote_chain(node_addr: &str) -> Option<Vec<crate::block::Block>> {
    println!("  Kết nối node {} ...", node_addr);
    let mut stream = match TcpStream::connect(node_addr) {
        Ok(s)  => s,
        Err(e) => { eprintln!("  ✗ connect: {}", e); return None; }
    };
    stream.set_read_timeout(Some(Duration::from_secs(15))).ok()?;

    let msg = Message::GetBlocks { from_index: 0 };
    if let Err(e) = stream.write_all(&msg.serialize()) {
        eprintln!("  ✗ write: {}", e); return None;
    }

    let mut line = String::new();
    match BufReader::new(stream).read_line(&mut line) {
        Ok(0)  => { eprintln!("  ✗ connection closed"); None }
        Err(e) => { eprintln!("  ✗ read: {}", e); None }
        Ok(_)  => match Message::deserialize(line.trim_end_matches('\n').as_bytes()) {
            Some(Message::Blocks { blocks }) => {
                println!("  ✓ nhận {} blocks từ {}", blocks.len(), node_addr);
                Some(blocks)
            }
            other => { eprintln!("  ✗ unexpected response: {:?}", other); None }
        }
    }
}

fn print_explorer_help() {
    println!();
    println!("  Usage: cargo run -- explorer <command> [args]");
    println!();
    println!("  Commands:");
    println!("    chain [node]       chain summary + 5 blocks gần nhất");
    println!("                         node mặc định: {}", crate::miner::DEFAULT_NODE);
    println!("    block <height>     chi tiết block tại height");
    println!("    tx <tx_id>         tìm transaction (có thể dùng prefix)");
    println!("    balance <addr>     số dư của địa chỉ (pubkey_hash_hex)");
    println!("    utxo <addr>        danh sách UTXO của địa chỉ");
    println!();
}
