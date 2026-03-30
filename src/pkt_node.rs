#![allow(dead_code)]
//! v15.7 — PKT Node Server
//!
//! Listen cho incoming PKT P2P connections, thực hiện handshake, giữ kết nối.
//!
//! Handshake flow (server side — ngược với client):
//!   ← receive Version   (peer gửi trước)
//!   → send Version       (ta gửi Version của ta)
//!   → send Verack        (xác nhận Version của peer)
//!   ← receive Verack     (peer xác nhận Version của ta)
//!   → keepalive Ping/Pong + respond GetHeaders với empty Headers
//!
//! CLI: cargo run -- pkt-node [port] [--mainnet] [--max-peers N]
//!
//! Dùng pkt_wire + pkt_peer từ v15.0–v15.1

use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

use crate::pkt_wire::{PktMsg, VersionMsg, TESTNET_MAGIC, MAINNET_MAGIC, PROTOCOL_VERSION};
use crate::pkt_peer::{send_msg, recv_msg, PeerError};
use crate::pkt_relay::{RelayHub, SeenHashes, wire_block_hash};

// ── Constants ─────────────────────────────────────────────────────────────────

pub const DEFAULT_PORT:       u16 = 64512;   // PKT testnet official port
pub const READ_TIMEOUT_SECS:  u64 = 60;      // inactivity → send ping
pub const HANDSHAKE_TIMEOUT:  u64 = 20;      // max time để hoàn thành handshake
pub const MAX_PEERS_DEFAULT:  usize = 50;

// ── NodeConfig ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub port:       u16,
    pub magic:      [u8; 4],
    pub network:    String,
    pub our_height: i32,
    pub max_peers:  usize,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            port:       DEFAULT_PORT,
            magic:      TESTNET_MAGIC,
            network:    "testnet".to_string(),
            our_height: 0,
            max_peers:  MAX_PEERS_DEFAULT,
        }
    }
}

impl NodeConfig {
    pub fn testnet(port: u16) -> Self {
        Self { port, ..Self::default() }
    }

    pub fn mainnet(port: u16) -> Self {
        Self {
            port,
            magic:   MAINNET_MAGIC,
            network: "mainnet".to_string(),
            ..Self::default()
        }
    }
}

// ── ConnectedPeer ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConnectedPeer {
    pub addr:       String,
    pub user_agent: String,
    pub height:     i32,
    pub version:    u32,
}

// ── Server-side handshake ─────────────────────────────────────────────────────
//
// Server waits for peer's Version first, then replies Version + Verack,
// then waits for peer's Verack.

pub fn server_handshake(
    stream:     &mut TcpStream,
    magic:      [u8; 4],
    our_height: i32,
) -> Result<ConnectedPeer, PeerError> {
    stream.set_read_timeout(Some(Duration::from_secs(HANDSHAKE_TIMEOUT)))?;

    let addr = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // ── Step 1: receive peer's Version ────────────────────────────────────────
    let peer = loop {
        let msg = recv_msg(stream, magic)?;
        match msg {
            PktMsg::Version(v) => {
                break ConnectedPeer {
                    addr:       addr.clone(),
                    user_agent: v.user_agent.clone(),
                    height:     v.start_height,
                    version:    v.version,
                };
            }
            PktMsg::Ping { nonce } => {
                // peer might ping before sending Version — respond and keep waiting
                send_msg(stream, PktMsg::Pong { nonce }, magic)?;
            }
            _ => {
                return Err(PeerError::Handshake(
                    "expected Version as first message".to_string(),
                ));
            }
        }
    };

    // ── Step 2: send our Version ──────────────────────────────────────────────
    send_msg(stream, PktMsg::Version(VersionMsg::new(our_height)), magic)?;

    // ── Step 3: send Verack (acknowledge peer's Version) ─────────────────────
    send_msg(stream, PktMsg::Verack, magic)?;

    // ── Step 4: wait for peer's Verack ────────────────────────────────────────
    loop {
        let msg = recv_msg(stream, magic)?;
        match msg {
            PktMsg::Verack => break,
            PktMsg::Ping { nonce } => {
                send_msg(stream, PktMsg::Pong { nonce }, magic)?;
            }
            _ => {} // ignore other messages while waiting
        }
    }

    Ok(peer)
}

// ── Per-peer session loop ─────────────────────────────────────────────────────

/// Build WireBlockHeader slice từ chain blocks, đảm bảo wire-chain linkage:
/// header[i].prev_block == SHA256d(wire_bytes_of_header[i-1]).
/// Sync client validate điều này trong validate_chain_links().
fn blocks_to_wire_headers(
    blocks:    &[crate::block::Block],
    prev_hash: [u8; 32],   // SHA256d hash của block ngay trước slice (hoặc zeros nếu genesis)
) -> Vec<crate::pkt_wire::WireBlockHeader> {
    use crate::pkt_wire::WireBlockHeader;
    use crate::block::Block;

    let mut headers = Vec::with_capacity(blocks.len());
    let mut prev = prev_hash;

    for block in blocks {
        // merkle_root từ txids
        let mr_hex = Block::merkle_root_txid(&block.transactions);
        let mut merkle_root = [0u8; 32];
        if let Ok(b) = hex::decode(&mr_hex) {
            let len = b.len().min(32);
            merkle_root[..len].copy_from_slice(&b[..len]);
        }

        let wh = WireBlockHeader {
            version:     1,
            prev_block:  prev,
            merkle_root,
            timestamp:   block.timestamp as u32,
            bits:        0x207f_ffff,
            nonce:       (block.nonce & 0xFFFF_FFFF) as u32,
        };
        prev = wh.block_hash(); // SHA256d của wire bytes → dùng làm prev_block cho header tiếp
        headers.push(wh);
    }
    headers
}

/// Serialize one chain Block to Bitcoin wire format:
///   [80B: wire header] [varint: tx_count] [tx_0] ... [tx_n]
/// `prev_wire_hash` = SHA256d of the preceding block's wire header bytes.
fn block_to_wire_payload(
    block:          &crate::block::Block,
    prev_wire_hash: [u8; 32],
) -> Vec<u8> {
    use crate::pkt_utxo_sync::{WireTx, WireTxIn, WireTxOut, encode_wire_tx};

    // 80-byte wire header
    let headers = blocks_to_wire_headers(std::slice::from_ref(block), prev_wire_hash);
    let hdr_bytes = headers[0].to_bytes();

    // varint helper (inline, no import needed)
    let write_varint = |buf: &mut Vec<u8>, n: u64| {
        if n < 0xfd { buf.push(n as u8); }
        else if n <= 0xffff { buf.push(0xfd); buf.extend_from_slice(&(n as u16).to_le_bytes()); }
        else if n <= 0xffff_ffff { buf.push(0xfe); buf.extend_from_slice(&(n as u32).to_le_bytes()); }
        else { buf.push(0xff); buf.extend_from_slice(&n.to_le_bytes()); }
    };

    // Convert chain Transaction → WireTx
    let wire_txs: Vec<WireTx> = block.transactions.iter().map(|tx| {
        let inputs = tx.inputs.iter().map(|inp| {
            let mut prev_txid = [0u8; 32];
            if let Ok(b) = hex::decode(&inp.tx_id) {
                let n = b.len().min(32);
                prev_txid[..n].copy_from_slice(&b[..n]);
            }
            WireTxIn {
                prev_txid,
                prev_vout:  inp.output_index as u32,
                script_sig: inp.script_sig.to_bytes(),
                sequence:   inp.sequence,
            }
        }).collect();
        let outputs = tx.outputs.iter().map(|out| WireTxOut {
            value:        out.amount,
            script_pubkey: out.script_pubkey.to_bytes(),
        }).collect();
        WireTx { version: 1, inputs, outputs, locktime: 0 }
    }).collect();

    // Assemble payload
    let mut payload = Vec::with_capacity(80 + 9 + wire_txs.len() * 256);
    payload.extend_from_slice(&hdr_bytes);
    write_varint(&mut payload, wire_txs.len() as u64);
    for tx in &wire_txs {
        payload.extend_from_slice(&encode_wire_tx(tx));
    }
    payload
}

fn handle_peer(
    mut stream: TcpStream,
    magic:      [u8; 4],
    our_height: i32,
    peers:      Arc<Mutex<Vec<ConnectedPeer>>>,
    chain:      SharedChain,
    relay_hub:  Arc<RelayHub>,
) {
    let addr = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    println!("[pkt-node] incoming: {}", addr);

    let peer = match server_handshake(&mut stream, magic, our_height) {
        Ok(p) => p,
        Err(e) => {
            println!("[pkt-node] handshake failed {}: {}", addr, e);
            return;
        }
    };

    println!(
        "[pkt-node] connected: {} agent=\"{}\" height={}",
        peer.addr, peer.user_agent, peer.height
    );

    {
        let mut locked = peers.lock().unwrap();
        locked.push(peer.clone());
        println!("[pkt-node] total peers: {}", locked.len());
    }

    // Longer read timeout for established session
    let _ = stream.set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECS)));

    // ── Relay setup ───────────────────────────────────────────────────────────
    // Đăng ký peer với RelayHub, nhận channel để write-thread gửi Inv.
    let relay_rx = relay_hub.register(&addr);

    // Clone stream cho write-thread (relay outbound Inv messages).
    let write_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            println!("[pkt-node] stream clone failed {}: {}", addr, e);
            relay_hub.deregister(&addr);
            return;
        }
    };
    let relay_magic = magic;
    let relay_addr  = addr.clone();
    thread::spawn(move || {
        let mut ws = write_stream;
        for event in relay_rx {
            let item = event.to_inv_item();
            let msg  = PktMsg::Inv { items: vec![item] };
            if let Err(e) = send_msg(&mut ws, msg, relay_magic) {
                println!("[pkt-node] relay write failed {}: {}", relay_addr, e);
                break;
            }
        }
    });

    // SeenHashes: seed từ BLAKE3 hashes hiện tại của chain (dạng bytes).
    let mut seen: SeenHashes = SeenHashes::new(8192);
    {
        let bc = chain.lock().unwrap();
        for blk in &bc.chain {
            if let Ok(bytes) = hex::decode(&blk.hash) {
                if bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    seen.insert(arr);
                }
            }
        }
    }

    // ── Message loop ──────────────────────────────────────────────────────────
    loop {
        match recv_msg(&mut stream, magic) {
            Ok(msg) => match msg {
                PktMsg::Ping { nonce } => {
                    if let Err(e) = send_msg(&mut stream, PktMsg::Pong { nonce }, magic) {
                        println!("[pkt-node] send pong failed {}: {}", addr, e);
                        break;
                    }
                }
                PktMsg::Pong { .. } => {}
                PktMsg::GetHeaders { locator_hashes, .. } => {
                    // Locator hashes are SHA256d wire hashes (stored by sync client),
                    // NOT our BLAKE3 block hashes. Must walk chain computing wire hashes
                    // to find the correct resume point.
                    let headers = {
                        let bc = chain.lock().unwrap();
                        let locator_set: std::collections::HashSet<[u8; 32]> =
                            locator_hashes.iter().copied().collect();

                        // Walk all blocks, compute wire hashes in sequence (deterministic).
                        // Find the highest block whose wire hash appears in the locator.
                        let mut wire_prev    = [0u8; 32];
                        let mut start_after  = 0usize;
                        let mut start_prev_w = [0u8; 32];

                        for (i, block) in bc.chain.iter().enumerate() {
                            let wh = blocks_to_wire_headers(
                                std::slice::from_ref(block), wire_prev
                            ).into_iter().next().unwrap();
                            wire_prev = wh.block_hash();
                            if locator_set.contains(&wire_prev) {
                                start_after  = i + 1;
                                start_prev_w = wire_prev;
                            }
                        }

                        let slice = &bc.chain[start_after..bc.chain.len().min(start_after + 2000)];
                        blocks_to_wire_headers(slice, start_prev_w)
                    };
                    let count = headers.len();
                    if let Err(e) = send_msg(&mut stream, PktMsg::Headers { headers }, magic) {
                        println!("[pkt-node] send headers failed {}: {}", addr, e);
                        break;
                    }
                    println!("[pkt-node] → Headers({}) to {}", count, addr);
                }
                PktMsg::GetData { items } => {
                    let bc = chain.lock().unwrap();
                    for item in &items {
                        if item.inv_type != crate::pkt_wire::INV_MSG_BLOCK { continue; }
                        // Find block by wire hash: walk chain computing SHA256d wire hashes
                        let mut wire_prev = [0u8; 32];
                        let mut found_idx: Option<usize> = None;
                        for (i, blk) in bc.chain.iter().enumerate() {
                            let hdrs = blocks_to_wire_headers(std::slice::from_ref(blk), wire_prev);
                            let h = hdrs[0].block_hash();
                            if h == item.hash { found_idx = Some(i); break; }
                            wire_prev = h;
                        }
                        if let Some(idx) = found_idx {
                            // Recompute prev_wire_hash for this block
                            let mut prev_w = [0u8; 32];
                            for blk in bc.chain[..idx].iter() {
                                let hdrs = blocks_to_wire_headers(std::slice::from_ref(blk), prev_w);
                                prev_w = hdrs[0].block_hash();
                            }
                            let payload = block_to_wire_payload(&bc.chain[idx], prev_w);
                            let mut cmd = [0u8; crate::pkt_wire::COMMAND_LEN];
                            cmd[..5].copy_from_slice(b"block");
                            let msg = PktMsg::Unknown { command: cmd, payload };
                            if let Err(e) = send_msg(&mut stream, msg, magic) {
                                println!("[pkt-node] send block failed {}: {}", addr, e);
                                break;
                            }
                            println!("[pkt-node] → Block(h={}) to {}", bc.chain[idx].index, addr);
                        } else {
                            println!("[pkt-node] GetData: block not found for {}", hex::encode(&item.hash[..8]));
                        }
                    }
                }
                PktMsg::GetAddr => {
                    // Respond with Addr containing currently connected peers
                    let peer_addrs: Vec<crate::pkt_wire::NetAddr> = {
                        let locked = peers.lock().unwrap();
                        locked.iter()
                            .filter_map(|p| crate::pkt_wire::NetAddr::from_addr_str(&p.addr))
                            .collect()
                    };
                    let count = peer_addrs.len();
                    if let Err(e) = send_msg(&mut stream, PktMsg::Addr { peers: peer_addrs }, magic) {
                        println!("[pkt-node] send addr failed {}: {}", addr, e);
                        break;
                    }
                    println!("[pkt-node] → Addr({}) to {}", count, addr);
                }
                PktMsg::Addr { peers: received } => {
                    println!("[pkt-node] addr from {}: {} entries", addr, received.len());
                }
                PktMsg::Inv { items } => {
                    // Lọc những hash chưa thấy → gửi GetData để request
                    let unknown: Vec<_> = items.into_iter()
                        .filter(|item| {
                            let already_seen = seen.contains(&item.hash);
                            if !already_seen { seen.insert(item.hash); }
                            !already_seen
                        })
                        .collect();
                    if !unknown.is_empty() {
                        println!("[pkt-node] inv from {}: {} unknown items → GetData", addr, unknown.len());
                        let getdata = PktMsg::GetData { items: unknown };
                        if let Err(e) = send_msg(&mut stream, getdata, magic) {
                            println!("[pkt-node] send getdata failed {}: {}", addr, e);
                            break;
                        }
                    }
                }
                PktMsg::Version(_) | PktMsg::Verack => {} // already done
                other => {
                    // Nhận block payload từ peer → relay Inv tới các peers khác
                    if let PktMsg::Unknown { ref command, ref payload } = other {
                        let cmd_str = String::from_utf8_lossy(command);
                        let cmd_trim = cmd_str.trim_end_matches('\0');
                        if cmd_trim == "block" {
                            if let Some(wire_hash) = wire_block_hash(payload) {
                                let is_new = !seen.insert(wire_hash); // insert trả false nếu mới
                                if is_new {
                                    println!("[pkt-node] block from {}: {} → relay to {} peers",
                                        addr, hex::encode(&wire_hash[..8]), relay_hub.peer_count().saturating_sub(1));
                                    relay_hub.broadcast_block(wire_hash, Some(&addr));
                                }
                            }
                        } else if cmd_trim == "tx" {
                            // Raw tx relay
                            use crate::pkt_relay::wire_tx_hash;
                            let txid = wire_tx_hash(payload);
                            let is_new = !seen.insert(txid);
                            if is_new {
                                println!("[pkt-node] tx from {}: {} → relay to {} peers",
                                    addr, hex::encode(&txid[..8]), relay_hub.peer_count().saturating_sub(1));
                                relay_hub.broadcast_tx(txid, Some(&addr));
                            }
                        } else {
                            let dbg = format!("{:?}", other);
                            let name = &dbg[..dbg.find('(').unwrap_or(dbg.len())];
                            println!("[pkt-node] {} → {}", addr, name);
                        }
                    } else {
                        let dbg = format!("{:?}", other);
                        let cmd = &dbg[..dbg.find('(').unwrap_or(dbg.len())];
                        println!("[pkt-node] {} → {}", addr, cmd);
                    }
                }
            },
            Err(PeerError::Timeout) => {
                // Send keepalive ping
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(1);
                if let Err(e) = send_msg(&mut stream, PktMsg::Ping { nonce }, magic) {
                    println!("[pkt-node] keepalive failed {}: {}", addr, e);
                    break;
                }
            }
            Err(PeerError::Disconnected) => {
                println!("[pkt-node] disconnected: {}", addr);
                break;
            }
            Err(e) => {
                println!("[pkt-node] error {}: {}", addr, e);
                break;
            }
        }
    }

    // Remove peer from active list + relay hub
    {
        let mut locked = peers.lock().unwrap();
        locked.retain(|p| p.addr != addr);
        println!("[pkt-node] total peers: {}", locked.len());
    }
    relay_hub.deregister(&addr);
}

// ── Main server loop ──────────────────────────────────────────────────────────

/// Khởi động PKT node server.
///
/// Trả về `Arc<RelayHub>` để caller (miner, API) có thể broadcast
/// block/tx mới sau khi validate.
pub fn run_pkt_node(cfg: NodeConfig, chain: SharedChain) -> Arc<RelayHub> {
    let bind_addr = format!("0.0.0.0:{}", cfg.port);
    let listener  = TcpListener::bind(&bind_addr).unwrap_or_else(|e| {
        eprintln!("[pkt-node] cannot bind {}: {}", bind_addr, e);
        std::process::exit(1);
    });

    println!("[pkt-node] listening on {} (network={})", bind_addr, cfg.network);
    println!(
        "[pkt-node] magic: {:02x}{:02x}{:02x}{:02x}  protocol_version: {}",
        cfg.magic[0], cfg.magic[1], cfg.magic[2], cfg.magic[3],
        PROTOCOL_VERSION,
    );

    let peers:     Arc<Mutex<Vec<ConnectedPeer>>> = Arc::new(Mutex::new(Vec::new()));
    let relay_hub: Arc<RelayHub> = Arc::new(RelayHub::new());
    let hub_ret   = Arc::clone(&relay_hub);

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let current = peers.lock().map(|l| l.len()).unwrap_or(0);
                    if current >= cfg.max_peers {
                        println!(
                            "[pkt-node] max peers ({}) reached, rejecting connection",
                            cfg.max_peers
                        );
                        continue;
                    }

                    let peers_clone = Arc::clone(&peers);
                    let chain_clone = Arc::clone(&chain);
                    let hub_clone   = Arc::clone(&relay_hub);
                    let magic       = cfg.magic;
                    let our_height  = cfg.our_height;

                    thread::spawn(move || {
                        handle_peer(s, magic, our_height, peers_clone, chain_clone, hub_clone);
                    });
                }
                Err(e) => {
                    eprintln!("[pkt-node] accept error: {}", e);
                }
            }
        }
    });

    hub_ret
}

// ── CLI arg parsing ───────────────────────────────────────────────────────────

pub fn parse_node_args(args: &[String]) -> NodeConfig {
    let mut cfg = NodeConfig::default();
    let mut i   = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mainnet" => {
                cfg.magic   = MAINNET_MAGIC;
                cfg.network = "mainnet".to_string();
            }
            "--port" | "-p" if i + 1 < args.len() => {
                i += 1;
                if let Ok(p) = args[i].parse() { cfg.port = p; }
            }
            "--height" if i + 1 < args.len() => {
                i += 1;
                if let Ok(h) = args[i].parse() { cfg.our_height = h; }
            }
            "--max-peers" if i + 1 < args.len() => {
                i += 1;
                if let Ok(n) = args[i].parse() { cfg.max_peers = n; }
            }
            other if !other.starts_with('-') => {
                // bare port number
                if let Ok(p) = other.parse::<u16>() { cfg.port = p; }
            }
            _ => {}
        }
        i += 1;
    }
    cfg
}

// ── Template server (port+1) ──────────────────────────────────────────────────
//
// Speaks crate::message::Message protocol (JSON lines).
// GetTemplate → Template { prev_hash, height, difficulty, txs }
// NewBlock    → validate + commit + save RocksDB
//
// Miner kết nối 127.0.0.1:(p2p_port+1) thay vì testnet node.

type SharedChain = Arc<Mutex<crate::chain::Blockchain>>;

fn handle_template_client(
    mut stream: std::net::TcpStream,
    chain:      SharedChain,
    relay_hub:  Arc<RelayHub>,
) {
    use std::io::{BufRead, BufReader, Write};
    use crate::message::Message;

    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    let cloned = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => { eprintln!("[template] stream clone failed: {}", e); return; }
    };
    let reader = BufReader::new(cloned);
    for line in reader.lines() {
        let line = match line { Ok(l) => l, Err(_) => break };
        let msg  = match Message::deserialize(line.as_bytes()) { Some(m) => m, None => break };
        let reply = match msg {
            Message::GetTemplate => {
                let bc       = chain.lock().unwrap();
                let height   = bc.chain.len() as u64;
                let prev     = bc.chain.last().map(|b| b.hash.clone())
                                .unwrap_or_else(|| "0".repeat(64));
                let diff     = bc.difficulty;
                let mut txs  = bc.mempool.select_transactions(100);
                drop(bc);

                // v23.6: merge wire mempool TXs (from pkt_sync) into template
                let remaining = 100usize.saturating_sub(txs.len());
                if remaining > 0 {
                    let mempool_path = crate::pkt_mempool_sync::default_mempool_db_path();
                    let wire_txs = crate::pkt_mempool_bridge::load_wire_mempool_txs(
                        &mempool_path, remaining,
                    );
                    let existing: std::collections::HashSet<String> =
                        txs.iter().map(|t| t.tx_id.clone()).collect();
                    for wt in wire_txs {
                        if !existing.contains(&wt.tx_id) {
                            txs.push(wt);
                        }
                    }
                }

                Message::Template { prev_hash: prev, height, difficulty: diff, txs }
            }
            Message::NewBlock { block } => {
                // Lấy BLAKE3 hash trước khi commit để relay
                let block_hash_hex = block.hash.clone();
                let mut bc = chain.lock().unwrap();
                bc.commit_mined_block(block);
                let h = bc.chain.len() as u64;
                drop(bc);
                if let Err(e) = crate::storage::save_blockchain(&chain.lock().unwrap()) {
                    eprintln!("[template] save error: {}", e);
                }
                // Relay block hash tới tất cả connected peers
                if let Ok(bytes) = hex::decode(&block_hash_hex) {
                    if bytes.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&bytes);
                        if relay_hub.peer_count() > 0 {
                            println!("[template] relay block {} to {} peers",
                                &block_hash_hex[..16], relay_hub.peer_count());
                            relay_hub.broadcast_block(arr, None);
                        }
                    }
                }
                Message::Height { height: h }
            }
            Message::GetBlocks { from_index } => {
                let bc = chain.lock().unwrap();
                let blocks = bc.chain.iter()
                    .skip(from_index as usize)
                    .cloned()
                    .collect();
                drop(bc);
                Message::Blocks { blocks }
            }
            _ => break,
        };
        if stream.write_all(&reply.serialize()).is_err() { break; }
    }
}

fn run_template_server(port: u16, chain: SharedChain, relay_hub: Arc<RelayHub>) {
    let addr = format!("0.0.0.0:{}", port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => { eprintln!("[template] cannot bind {}: {}", addr, e); return; }
    };
    println!("[template] listening on {} (miner endpoint)", addr);
    for stream in listener.incoming() {
        if let Ok(s) = stream {
            let c   = Arc::clone(&chain);
            let hub = Arc::clone(&relay_hub);
            thread::spawn(move || handle_template_client(s, c, hub));
        }
    }
}

pub fn cmd_pkt_node(args: &[String]) {
    let cfg   = parse_node_args(args);
    let chain = Arc::new(Mutex::new(crate::storage::load_or_new()));
    {
        let bc = chain.lock().unwrap();
        println!("[pkt-node] starting on port {} ({})", cfg.port, cfg.network);
        println!("[pkt-node] chain loaded: height={}", bc.chain.len().saturating_sub(1));
    }

    // Khởi động P2P node server — nhận RelayHub để share với template server
    let relay_hub = run_pkt_node(cfg.clone(), Arc::clone(&chain));

    // Spawn template server on port+1 (local miner endpoint) — share relay_hub
    let template_port  = cfg.port + 1;
    let chain_template = Arc::clone(&chain);
    let hub_template   = Arc::clone(&relay_hub);
    thread::spawn(move || run_template_server(template_port, chain_template, hub_template));

    // Block main thread
    loop { thread::sleep(Duration::from_secs(60)); }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use crate::pkt_wire::{TESTNET_MAGIC, MAINNET_MAGIC, PROTOCOL_VERSION as WIRE_PROTOCOL_VERSION};
    use crate::pkt_peer::{send_msg, recv_msg};

    // ── Config tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_node_config_default() {
        let cfg = NodeConfig::default();
        assert_eq!(cfg.port, DEFAULT_PORT);
        assert_eq!(cfg.magic, TESTNET_MAGIC);
        assert_eq!(cfg.network, "testnet");
        assert_eq!(cfg.our_height, 0);
        assert_eq!(cfg.max_peers, MAX_PEERS_DEFAULT);
    }

    #[test]
    fn test_node_config_testnet() {
        let cfg = NodeConfig::testnet(9000);
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.magic, TESTNET_MAGIC);
        assert_eq!(cfg.network, "testnet");
    }

    #[test]
    fn test_node_config_mainnet() {
        let cfg = NodeConfig::mainnet(64764);
        assert_eq!(cfg.port, 64764);
        assert_eq!(cfg.magic, MAINNET_MAGIC);
        assert_eq!(cfg.network, "mainnet");
    }

    // ── parse_node_args tests ─────────────────────────────────────────────────

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_parse_node_args_empty() {
        let cfg = parse_node_args(&args(&[]));
        assert_eq!(cfg.port, DEFAULT_PORT);
    }

    #[test]
    fn test_parse_node_args_bare_port() {
        let cfg = parse_node_args(&args(&["8333"]));
        assert_eq!(cfg.port, 8333);
    }

    #[test]
    fn test_parse_node_args_port_flag() {
        let cfg = parse_node_args(&args(&["--port", "9999"]));
        assert_eq!(cfg.port, 9999);
    }

    #[test]
    fn test_parse_node_args_short_port() {
        let cfg = parse_node_args(&args(&["-p", "7777"]));
        assert_eq!(cfg.port, 7777);
    }

    #[test]
    fn test_parse_node_args_mainnet() {
        let cfg = parse_node_args(&args(&["--mainnet"]));
        assert_eq!(cfg.magic, MAINNET_MAGIC);
        assert_eq!(cfg.network, "mainnet");
    }

    #[test]
    fn test_parse_node_args_height() {
        let cfg = parse_node_args(&args(&["--height", "1234"]));
        assert_eq!(cfg.our_height, 1234);
    }

    #[test]
    fn test_parse_node_args_max_peers() {
        let cfg = parse_node_args(&args(&["--max-peers", "10"]));
        assert_eq!(cfg.max_peers, 10);
    }

    #[test]
    fn test_parse_node_args_combined() {
        let cfg = parse_node_args(&args(&["8333", "--mainnet", "--height", "500", "--max-peers", "20"]));
        assert_eq!(cfg.port, 8333);
        assert_eq!(cfg.magic, MAINNET_MAGIC);
        assert_eq!(cfg.our_height, 500);
        assert_eq!(cfg.max_peers, 20);
    }

    #[test]
    fn test_parse_node_args_unknown_ignored() {
        let cfg = parse_node_args(&args(&["--unknown-flag"]));
        assert_eq!(cfg.port, DEFAULT_PORT); // unchanged
    }

    // ── Handshake tests (loopback TCP) ────────────────────────────────────────
    //
    // client thread: connect → send Version → recv Version → recv Verack → send Verack
    // server side:   server_handshake() called on accepted stream

    fn run_client_handshake(addr: std::net::SocketAddr, magic: [u8; 4], height: i32) {
        let mut stream = TcpStream::connect(addr).expect("client connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        // Send Version first (client initiates in standard Bitcoin P2P)
        send_msg(&mut stream, PktMsg::Version(VersionMsg::new(height)), magic).unwrap();

        // Expect server's Version
        let msg = recv_msg(&mut stream, magic).expect("recv server version");
        assert!(matches!(msg, PktMsg::Version(_)), "expected Version, got {:?}", msg);

        // Expect Verack
        let msg = recv_msg(&mut stream, magic).expect("recv verack");
        assert!(matches!(msg, PktMsg::Verack), "expected Verack");

        // Send our Verack
        send_msg(&mut stream, PktMsg::Verack, magic).unwrap();
    }

    #[test]
    fn test_server_handshake_ok() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr     = listener.local_addr().unwrap();
        let magic    = TESTNET_MAGIC;

        let client = thread::spawn(move || {
            run_client_handshake(addr, magic, 42);
        });

        let (mut stream, _) = listener.accept().unwrap();
        let peer = server_handshake(&mut stream, magic, 0).expect("server handshake");

        assert_eq!(peer.height, 42);
        assert!(!peer.addr.is_empty());
        client.join().unwrap();
    }

    #[test]
    fn test_server_handshake_wrong_magic() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr     = listener.local_addr().unwrap();
        let correct  = TESTNET_MAGIC;
        let wrong    = MAINNET_MAGIC;

        let client = thread::spawn(move || {
            // Client sends with WRONG magic
            let mut stream = TcpStream::connect(addr).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            send_msg(&mut stream, PktMsg::Version(VersionMsg::new(0)), wrong).unwrap();
            // server will reject — client may see broken pipe, that's OK
        });

        let (mut stream, _) = listener.accept().unwrap();
        // Server expects correct magic — should fail
        let result = server_handshake(&mut stream, correct, 0);
        assert!(result.is_err(), "expected error for wrong magic");
        client.join().unwrap();
    }

    #[test]
    fn test_server_handshake_captures_peer_height() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr     = listener.local_addr().unwrap();
        let magic    = TESTNET_MAGIC;

        thread::spawn(move || {
            run_client_handshake(addr, magic, 9999);
        });

        let (mut stream, _) = listener.accept().unwrap();
        let peer = server_handshake(&mut stream, magic, 0).unwrap();
        assert_eq!(peer.height, 9999);
    }

    #[test]
    fn test_server_handshake_captures_user_agent() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr     = listener.local_addr().unwrap();
        let magic    = TESTNET_MAGIC;

        thread::spawn(move || {
            run_client_handshake(addr, magic, 0);
        });

        let (mut stream, _) = listener.accept().unwrap();
        let peer = server_handshake(&mut stream, magic, 0).unwrap();
        // VersionMsg::new() sets USER_AGENT
        assert!(!peer.user_agent.is_empty());
        assert!(peer.user_agent.contains("blockchain-rust"));
    }

    #[test]
    fn test_server_handshake_version_number() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr     = listener.local_addr().unwrap();
        let magic    = TESTNET_MAGIC;

        thread::spawn(move || {
            run_client_handshake(addr, magic, 0);
        });

        let (mut stream, _) = listener.accept().unwrap();
        let peer = server_handshake(&mut stream, magic, 0).unwrap();
        assert_eq!(peer.version, WIRE_PROTOCOL_VERSION);
    }

    // ── ConnectedPeer tests ───────────────────────────────────────────────────

    #[test]
    fn test_connected_peer_clone() {
        let peer = ConnectedPeer {
            addr:       "127.0.0.1:1234".to_string(),
            user_agent: "/test:1.0/".to_string(),
            height:     100,
            version:    70013,
        };
        let cloned = peer.clone();
        assert_eq!(cloned.addr, peer.addr);
        assert_eq!(cloned.height, peer.height);
    }

    // ── DEFAULT_PORT ──────────────────────────────────────────────────────────

    #[test]
    fn test_default_port_is_pkt_testnet() {
        // PKT testnet official port
        assert_eq!(DEFAULT_PORT, 64512);
    }

    #[test]
    fn test_handshake_timeout_reasonable() {
        assert!(HANDSHAKE_TIMEOUT >= 10);
        assert!(HANDSHAKE_TIMEOUT <= 60);
    }

    #[test]
    fn test_read_timeout_longer_than_handshake() {
        assert!(READ_TIMEOUT_SECS > HANDSHAKE_TIMEOUT);
    }
}
