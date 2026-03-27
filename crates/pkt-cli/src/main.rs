// v19.6 — PKT CLI tool
// Binary: pkt
// Usage:  pkt [--json] [--node <url>] <command>

mod config;
use config::{load_config, save_config, CliConfig};

use clap::{Parser, Subcommand};
use serde_json::Value;
use std::process;

// ── CLI definition ─────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "pkt",
    version = env!("CARGO_PKG_VERSION"),
    about = "PKT CLI — query PKTScan API từ terminal\n\nConfig: ~/.pkt/cli.toml"
)]
struct Cli {
    /// Output raw JSON thay vì pretty table
    #[arg(long, short, global = true)]
    json: bool,

    /// Override node URL (mặc định từ ~/.pkt/cli.toml)
    #[arg(long, global = true, value_name = "URL")]
    node: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Lấy thông tin block theo height
    Block {
        /// Block height
        height: u64,
    },
    /// Lấy thông tin transaction theo txid
    Tx {
        /// Transaction ID (hex)
        txid: String,
    },
    /// Lấy số dư và lịch sử giao dịch của address
    Address {
        /// PKT address
        address: String,
    },
    /// Xem mempool hiện tại
    Mempool,
    /// Xem trạng thái sync của node
    #[command(name = "sync-status")]
    SyncStatus,
    /// Quản lý config (~/.pkt/cli.toml)
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Hiển thị config hiện tại
    Show,
    /// Đặt node URL
    #[command(name = "set-node")]
    SetNode {
        /// URL của PKTScan node, vd: https://oceif.com
        url: String,
    },
}

// ── HTTP helper ────────────────────────────────────────────────────────────────

fn api_get(base: &str, path: &str) -> Result<Value, String> {
    let url = format!("{base}{path}");
    let resp = reqwest::blocking::get(&url)
        .map_err(|e| format!("Lỗi kết nối tới {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} — {url}", resp.status()));
    }
    resp.json::<Value>().map_err(|e| format!("Lỗi parse JSON: {e}"))
}

// ── Conversion ─────────────────────────────────────────────────────────────────

fn paklets_to_pkt(sat: u64) -> f64 {
    sat as f64 / 1_073_741_824.0
}

// ── Pretty-print functions ─────────────────────────────────────────────────────

fn print_block(v: &Value) {
    println!("Block #{}", v["height"].as_u64().unwrap_or(0));
    println!("  Hash       : {}", v["hash"].as_str().unwrap_or("?"));
    println!("  Prev Hash  : {}", v["prev_hash"].as_str().unwrap_or("?"));
    println!("  Timestamp  : {}", v["timestamp"].as_u64().unwrap_or(0));
    println!("  Bits       : {:#010x}", v["bits"].as_u64().unwrap_or(0));
    println!("  Nonce      : {}", v["nonce"].as_u64().unwrap_or(0));
    if let Some(v32) = v["version"].as_u64() {
        println!("  Version    : {v32}");
    }
}

fn print_tx(v: &Value) {
    let txid = v["txid"].as_str().unwrap_or("?");
    println!("Transaction {txid}");
    println!("  Height     : {}", v["height"].as_u64().unwrap_or(0));
    println!("  Timestamp  : {}", v["timestamp"].as_u64().unwrap_or(0));
    if let Some(ins) = v["inputs"].as_array() {
        println!("  Inputs     : {}", ins.len());
    }
    if let Some(outs) = v["outputs"].as_array() {
        println!("  Outputs    : {}", outs.len());
    }
    if let Some(fee) = v["fee"].as_u64() {
        println!("  Fee        : {} paklets  ({:.8} PKT)", fee, paklets_to_pkt(fee));
    }
}

fn print_address(v: &Value) {
    let addr = v["address"].as_str().unwrap_or("?");
    println!("Address: {addr}");
    let sat = v["balance_sat"].as_u64().unwrap_or(0);
    let pkt = v["balance_pkt"].as_f64().unwrap_or_else(|| paklets_to_pkt(sat));
    println!("  Balance    : {pkt:.6} PKT  ({sat} paklets)");
    println!("  TX Count   : {}", v["tx_count"].as_u64().unwrap_or(0));
}

fn print_mempool(v: &Value) {
    let txs = match v.as_array() {
        Some(a) => a,
        None => {
            println!("Mempool trống.");
            return;
        }
    };
    println!("Mempool ({} transactions)", txs.len());
    if txs.is_empty() {
        return;
    }
    println!("  {:<66}  Fee (sat/vB)", "TXID");
    println!("  {}", "-".repeat(78));
    for tx in txs.iter().take(25) {
        let txid = tx["txid"].as_str().unwrap_or("?");
        let fee = tx["fee_rate"].as_u64().unwrap_or(0);
        println!("  {txid:<66}  {fee}");
    }
    if txs.len() > 25 {
        println!("  ... ({} thêm)", txs.len() - 25);
    }
}

fn print_sync_status(v: &Value) {
    println!("Sync Status");
    println!("  Phase      : {}", v["phase"].as_str().unwrap_or("?"));
    println!("  Sync height: {}", v["sync_height"].as_u64().unwrap_or(0));
    println!("  UTXO height: {}", v["utxo_height"].as_u64().unwrap_or(0));
    if let Some(p) = v["overall_progress"].as_f64() {
        println!("  Progress   : {:.1}%", p * 100.0);
    }
}

// ── Output helper ──────────────────────────────────────────────────────────────

fn output(json_mode: bool, v: Value, pretty: fn(&Value)) {
    if json_mode {
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        pretty(&v);
    }
}

// ── main ───────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    let cfg = load_config();
    let base = cli
        .node
        .as_deref()
        .unwrap_or(&cfg.node_url)
        .trim_end_matches('/')
        .to_string();

    match cli.cmd {
        Cmd::Config { action } => match action {
            ConfigAction::Show => {
                println!("node_url = \"{}\"", cfg.node_url);
                println!("config   = {}", config::config_path().display());
            }
            ConfigAction::SetNode { url } => {
                let new_cfg = CliConfig { node_url: url.clone() };
                match save_config(&new_cfg) {
                    Ok(_) => println!("Đã lưu: node_url = \"{url}\""),
                    Err(e) => { eprintln!("Lỗi lưu config: {e}"); process::exit(1); }
                }
            }
        },

        Cmd::Block { height } => {
            match api_get(&base, &format!("/api/testnet/block/{height}")) {
                Ok(v)  => output(cli.json, v, print_block),
                Err(e) => { eprintln!("{e}"); process::exit(1); }
            }
        }

        Cmd::Tx { txid } => {
            match api_get(&base, &format!("/api/testnet/tx/{txid}")) {
                Ok(v)  => output(cli.json, v, print_tx),
                Err(e) => { eprintln!("{e}"); process::exit(1); }
            }
        }

        Cmd::Address { address } => {
            match api_get(&base, &format!("/api/testnet/balance/{address}")) {
                Ok(v)  => output(cli.json, v, print_address),
                Err(e) => { eprintln!("{e}"); process::exit(1); }
            }
        }

        Cmd::Mempool => {
            match api_get(&base, "/api/testnet/mempool") {
                Ok(v)  => output(cli.json, v, print_mempool),
                Err(e) => { eprintln!("{e}"); process::exit(1); }
            }
        }

        Cmd::SyncStatus => {
            match api_get(&base, "/api/testnet/sync-status") {
                Ok(v)  => output(cli.json, v, print_sync_status),
                Err(e) => { eprintln!("{e}"); process::exit(1); }
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- CLI parsing ---

    #[test]
    fn parse_block_command() {
        let cli = Cli::try_parse_from(["pkt", "block", "100"]).unwrap();
        assert!(matches!(cli.cmd, Cmd::Block { height: 100 }));
        assert!(!cli.json);
    }

    #[test]
    fn parse_tx_command() {
        let cli = Cli::try_parse_from(["pkt", "tx", "abc123"]).unwrap();
        assert!(matches!(cli.cmd, Cmd::Tx { txid } if txid == "abc123"));
    }

    #[test]
    fn parse_json_flag() {
        let cli = Cli::try_parse_from(["pkt", "--json", "sync-status"]).unwrap();
        assert!(cli.json);
        assert!(matches!(cli.cmd, Cmd::SyncStatus));
    }

    #[test]
    fn parse_node_override() {
        let cli = Cli::try_parse_from(["pkt", "--node", "http://localhost:3000", "mempool"]).unwrap();
        assert_eq!(cli.node.as_deref(), Some("http://localhost:3000"));
    }

    #[test]
    fn parse_config_set_node() {
        let cli = Cli::try_parse_from(["pkt", "config", "set-node", "http://x.com"]).unwrap();
        assert!(matches!(
            cli.cmd,
            Cmd::Config { action: ConfigAction::SetNode { url } } if url == "http://x.com"
        ));
    }

    // --- Conversion ---

    #[test]
    fn paklets_zero() {
        assert_eq!(paklets_to_pkt(0), 0.0);
    }

    #[test]
    fn paklets_one_pkt() {
        let pkt = paklets_to_pkt(1_073_741_824);
        assert!((pkt - 1.0).abs() < 1e-9);
    }

    // --- Pretty-print (smoke tests — không panic) ---

    #[test]
    fn print_block_smoke() {
        let v = json!({ "height": 42, "hash": "abc", "prev_hash": "def",
                         "timestamp": 1700000000u64, "bits": 486604799u64, "nonce": 7 });
        print_block(&v); // không panic
    }

    #[test]
    fn print_address_smoke() {
        let v = json!({ "address": "pkt1abc", "balance_sat": 2_147_483_648u64,
                         "balance_pkt": 2.0, "tx_count": 5 });
        print_address(&v);
    }

    #[test]
    fn print_mempool_empty() {
        let v = json!([]);
        print_mempool(&v); // "Mempool trống." không panic
    }
}
