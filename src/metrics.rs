#![allow(dead_code)]

/// v4.8 — Metrics
///
/// Collects runtime stats from chain, mempool, and (optionally) P2P node:
///   - Chain height & difficulty
///   - UTXO count
///   - Mempool depth + total pending fees
///   - Average block time (over last N blocks)
///   - Estimated hashrate (H/s) from difficulty + block time
///   - Peer count (queried from live node via RPC)
///   - Sync status: local height vs remote height
///
/// CLI:  cargo run -- metrics [node:port]
/// REST: GET /metrics  (added to api.rs)

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::chain::Blockchain;
use crate::message::Message;

// ─── Snapshot ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Current chain tip height
    pub height: u64,
    /// Current mining difficulty
    pub difficulty: usize,
    /// Number of UTXOs in UTXO set
    pub utxo_count: usize,
    /// Number of pending transactions in mempool
    pub mempool_depth: usize,
    /// Total fees of mempool transactions (satoshi)
    pub mempool_fees: u64,
    /// Average block time over last up-to-20 blocks (seconds)
    pub avg_block_time_s: f64,
    /// Estimated hashrate from difficulty + block time (H/s)
    pub estimated_hashrate: f64,
    /// Number of connected peers (0 if no live node queried)
    pub peer_count: usize,
    /// Local chain height (same as `height`)
    pub sync_height_local: u64,
    /// Remote node height (None if no node queried or unreachable)
    pub sync_height_remote: Option<u64>,
    /// Unix timestamp when snapshot was collected
    pub collected_at: i64,
}

// ─── Collect ──────────────────────────────────────────────────────────────────

/// Collect metrics from a local Blockchain instance.
/// Pass `node_addr` to also query peer count and remote height.
pub fn collect(bc: &Blockchain, node_addr: Option<&str>) -> MetricsSnapshot {
    let height = bc.chain.len().saturating_sub(1) as u64;
    let difficulty = bc.difficulty;
    let utxo_count = bc.utxo_set.utxos.len();
    let mempool_depth = bc.mempool.entries.len();
    let mempool_fees = bc.mempool.total_pending_fees();

    let avg_block_time_s = avg_block_time(&bc.chain);
    let estimated_hashrate = estimate_hashrate(difficulty, avg_block_time_s);

    // Query live node for peer count and remote height
    let (peer_count, sync_height_remote) = match node_addr {
        Some(addr) => query_node(addr),
        None => (0, None),
    };

    MetricsSnapshot {
        height,
        difficulty,
        utxo_count,
        mempool_depth,
        mempool_fees,
        avg_block_time_s,
        estimated_hashrate,
        peer_count,
        sync_height_local: height,
        sync_height_remote,
        collected_at: chrono::Utc::now().timestamp(),
    }
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

fn avg_block_time(chain: &[crate::block::Block]) -> f64 {
    let window = 20usize;
    if chain.len() < 2 {
        return 0.0;
    }
    let n = chain.len().min(window + 1);
    let start = chain.len() - n;
    let blocks = &chain[start..];
    let first_ts = blocks[0].timestamp;
    let last_ts  = blocks[blocks.len() - 1].timestamp;
    let intervals = (blocks.len() - 1) as f64;
    if intervals < 1.0 {
        return 0.0;
    }
    let total_secs = (last_ts - first_ts).max(0) as f64;
    total_secs / intervals
}

/// Estimate H/s: on average need 2^difficulty hashes per block, so
/// hashrate = 2^difficulty / block_time_s
fn estimate_hashrate(difficulty: usize, avg_block_time_s: f64) -> f64 {
    if avg_block_time_s < 0.001 {
        return 0.0;
    }
    let expected_hashes = (1u64 << difficulty.min(62)) as f64;
    expected_hashes / avg_block_time_s
}

/// Query a live node via TCP RPC to get peer count and chain height.
fn query_node(addr: &str) -> (usize, Option<u64>) {
    let height = rpc_height(addr);
    let peer_count = rpc_peer_count(addr);
    (peer_count, height)
}

fn rpc_height(addr: &str) -> Option<u64> {
    let msg = Message::GetHeight;
    match node_rpc(addr, &msg) {
        Some(Message::Height { height }) => Some(height),
        _ => None,
    }
}

fn rpc_peer_count(addr: &str) -> usize {
    // Use GetStatus to get peer count from node — not all nodes expose this,
    // so fall back to 0 on any failure.
    let msg = Message::GetPeerCount;
    match node_rpc(addr, &msg) {
        Some(Message::PeerCount { count }) => count,
        _ => 0,
    }
}

fn node_rpc(addr: &str, msg: &Message) -> Option<Message> {
    let mut stream = TcpStream::connect(addr).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    stream.write_all(&msg.serialize()).ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    Message::deserialize(line.trim_end_matches('\n').as_bytes())
}

// ─── CLI display ──────────────────────────────────────────────────────────────

pub fn print_metrics(m: &MetricsSnapshot) {
    use chrono::{DateTime, Utc};
    let dt = DateTime::<Utc>::from_timestamp(m.collected_at, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| m.collected_at.to_string());

    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                   📊  Node Metrics  v4.8                    ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  Collected at     : {}", dt);
    println!();
    println!("  ── Chain ────────────────────────────────────────────────────");
    println!("  Height           : {}", m.height);
    println!("  Difficulty       : {}", m.difficulty);
    println!("  UTXO count       : {}", m.utxo_count);
    println!();
    println!("  ── Mempool ──────────────────────────────────────────────────");
    println!("  Depth            : {} tx", m.mempool_depth);
    println!("  Total fees       : {} sat  ({:.8} PKT)",
        m.mempool_fees, m.mempool_fees as f64 / 1e8);
    println!();
    println!("  ── Performance ──────────────────────────────────────────────");
    println!("  Avg block time   : {}", fmt_duration(m.avg_block_time_s));
    println!("  Est. hashrate    : {}", hashrate_str(m.estimated_hashrate));
    println!();
    println!("  ── Network ──────────────────────────────────────────────────");
    println!("  Peers connected  : {}", m.peer_count);
    print!(  "  Sync status      : local={}", m.sync_height_local);
    match m.sync_height_remote {
        Some(remote) => {
            if m.sync_height_local >= remote {
                println!("  remote={}  ✅ synced", remote);
            } else {
                println!("  remote={}  ⚠️  behind by {} blocks",
                    remote, remote - m.sync_height_local);
            }
        }
        None => println!("  (no remote node queried)"),
    }
    println!();
}

// ─── CLI entry point ──────────────────────────────────────────────────────────

/// `cargo run -- metrics [node:port]`
pub fn cmd_metrics(node_addr: Option<&str>) {
    let bc = crate::storage::load_or_new();
    let m = collect(&bc, node_addr);
    print_metrics(&m);
}

// ─── Formatting helpers ───────────────────────────────────────────────────────

fn hashrate_str(h: f64) -> String {
    if h >= 1_000_000.0 {
        format!("{:.2} MH/s", h / 1_000_000.0)
    } else if h >= 1_000.0 {
        format!("{:.1} KH/s", h / 1_000.0)
    } else {
        format!("{:.0} H/s", h)
    }
}

fn fmt_duration(secs: f64) -> String {
    if secs < 1.0 {
        format!("{:.0}ms", secs * 1000.0)
    } else if secs < 60.0 {
        format!("{:.1}s", secs)
    } else {
        let m = (secs / 60.0) as u64;
        let s = secs as u64 % 60;
        format!("{}m{}s", m, s)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Blockchain;

    #[test]
    fn test_metrics_genesis_chain() {
        let bc = Blockchain::new();
        let m = collect(&bc, None);
        assert_eq!(m.height, 0);
        assert_eq!(m.mempool_depth, 0);
        assert_eq!(m.peer_count, 0);
        assert!(m.sync_height_remote.is_none());
    }

    #[test]
    fn test_metrics_after_mining() {
        let mut bc = Blockchain::new();
        bc.difficulty = 1;
        bc.add_block(vec![], "aabbccddaabbccddaabbccddaabbccddaabbccdd");
        bc.add_block(vec![], "aabbccddaabbccddaabbccddaabbccddaabbccdd");
        let m = collect(&bc, None);
        assert_eq!(m.height, 2);
        assert!(m.utxo_count > 0, "coinbase UTXOs phải tồn tại");
        assert!(m.avg_block_time_s >= 0.0);
    }

    #[test]
    fn test_avg_block_time_single_block() {
        let bc = Blockchain::new();
        let t = avg_block_time(&bc.chain);
        assert_eq!(t, 0.0, "chain 1 block → avg_block_time = 0");
    }

    #[test]
    fn test_estimate_hashrate_zero_time() {
        let h = estimate_hashrate(3, 0.0);
        assert_eq!(h, 0.0);
    }

    #[test]
    fn test_estimate_hashrate_nonzero() {
        let h = estimate_hashrate(3, 10.0); // diff=3, 10s/block → 2^3/10 = 0.8 H/s
        assert!((h - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_metrics_snapshot_serializable() {
        let bc = Blockchain::new();
        let m = collect(&bc, None);
        let json = serde_json::to_string(&m).expect("serialize phải thành công");
        let back: MetricsSnapshot = serde_json::from_str(&json).expect("deserialize phải thành công");
        assert_eq!(back.height, m.height);
    }
}
