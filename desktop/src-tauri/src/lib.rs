//! pktscan-desktop — Tauri backend (v25.0)
//!
//! Embedded node: pkt_core chạy axum API server local tại 127.0.0.1:21019
//! Frontend fetch() trực tiếp → không proxy, không fake.
//!
//! IPC commands (real only):
//!   start_mine / stop_mine / mine_status    → blake3 PoW miner
//!   peer_scan                               → scan PKT peers
//!   wallet_generate / wallet_from_privkey / wallet_from_mnemonic
//!   wallet_tx_build / tx_broadcast
//!   start_sync / stop_sync / is_sync_running → embedded node sync

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use rayon::prelude::*;
use tauri::Emitter;
use hmac::{Hmac, Mac};
use sha2::{Sha512, Digest as Sha2Digest};
use axum;

// ── Global state ──────────────────────────────────────────────────────────────

static MINER_RUNNING: AtomicBool = AtomicBool::new(false);
static MINER_STOP:    AtomicBool = AtomicBool::new(false);
static SYNC_RUNNING:  AtomicBool = AtomicBool::new(false);
static SYNC_STOP:     AtomicBool = AtomicBool::new(false);

pub const EMBEDDED_API_PORT: u16 = 21019;

// ── Embedded node API server ──────────────────────────────────────────────────

/// Spawn embedded axum API server tại 127.0.0.1:21019.
/// Đọc dữ liệu trực tiếp từ local RocksDB (~/.pkt/testnet/).
fn spawn_embedded_server(mainnet: bool) {
    pkt_core::pkt_paths::set_mainnet(mainnet);
    std::thread::spawn(move || {
        // Đảm bảo data dir tồn tại
        let data_dir = pkt_core::pkt_paths::data_dir();
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            eprintln!("[PKTScan] Cannot create data dir {:?}: {}", data_dir, e);
            return;
        }

        let rt = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(r)  => r,
            Err(e) => { eprintln!("[PKTScan] Tokio runtime error: {}", e); return; }
        };

        rt.block_on(async move {
            let addr = format!("127.0.0.1:{}", EMBEDDED_API_PORT);

            // Port đã bị dùng? Không panic, log và thoát thread.
            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l)  => l,
                Err(e) => {
                    eprintln!("[PKTScan] Cannot bind {} (port busy?): {}", addr, e);
                    return;
                }
            };

            // Mở DB — catch panic để không crash cả app
            let router = match std::panic::catch_unwind(|| {
                pkt_core::pkt_testnet_web::testnet_web_router()
            }) {
                Ok(r)  => r,
                Err(_) => {
                    eprintln!("[PKTScan] API router init failed (DB corrupt?)");
                    return;
                }
            };

            eprintln!("[PKTScan] Embedded API server → http://{}", addr);
            if let Err(e) = axum::serve(listener, router).await {
                eprintln!("[PKTScan] API server stopped: {}", e);
            }
        });
    });
}

// ── Sync control commands ─────────────────────────────────────────────────────

/// Bắt đầu sync blockchain từ peer vào local DB.
#[tauri::command]
async fn start_sync(peer: Option<String>) -> Result<String, String> {
    if SYNC_RUNNING.load(Ordering::SeqCst) {
        return Err("Sync đang chạy".into());
    }
    let peer = peer.unwrap_or_else(|| pkt_core::pkt_config::get().seed_p2p());
    SYNC_STOP.store(false, Ordering::SeqCst);
    SYNC_RUNNING.store(true, Ordering::SeqCst);
    let peer_display = peer.clone();
    std::thread::spawn(move || {
        pkt_core::pkt_sync::cmd_sync(&[peer]);
        SYNC_RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(format!("Sync started → {}", peer_display))
}

/// Trả về true nếu sync đang chạy.
#[tauri::command]
fn is_sync_running() -> bool {
    SYNC_RUNNING.load(Ordering::SeqCst)
}

// ── PKT wire broadcast (direct TCP — không qua HTTP server) ───────────────────
//
// Kết nối trực tiếp đến PKT peer, thực hiện Version/Verack handshake,
// gửi raw "tx" message. Không cần /api/testnet/tx/broadcast endpoint nữa.

const PKT_TESTNET_MAGIC: [u8; 4] = [0xfc, 0x11, 0x09, 0x07];
const PKT_HEADER_LEN:    usize   = 24;

fn sha256d_checksum(payload: &[u8]) -> [u8; 4] {
    use sha2::Sha256;
    let h1 = Sha256::digest(payload);
    let h2 = Sha256::digest(&h1);
    [h2[0], h2[1], h2[2], h2[3]]
}

fn pkt_frame(cmd: &str, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(PKT_HEADER_LEN + payload.len());
    frame.extend_from_slice(&PKT_TESTNET_MAGIC);
    let mut cmd_bytes = [0u8; 12];
    let b = cmd.as_bytes();
    cmd_bytes[..b.len().min(12)].copy_from_slice(&b[..b.len().min(12)]);
    frame.extend_from_slice(&cmd_bytes);
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&sha256d_checksum(payload));
    frame.extend_from_slice(payload);
    frame
}

fn pkt_version_payload() -> Vec<u8> {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let ua = b"/pktscan-desktop:0.1/";
    let mut buf = Vec::new();
    buf.extend_from_slice(&70015u32.to_le_bytes()); // protocol version
    buf.extend_from_slice(&0u64.to_le_bytes());     // services
    buf.extend_from_slice(&ts.to_le_bytes());       // timestamp
    buf.extend_from_slice(&[0u8; 26]);              // addr_recv
    buf.extend_from_slice(&[0u8; 26]);              // addr_from
    buf.extend_from_slice(&0u64.to_le_bytes());     // nonce
    buf.push(ua.len() as u8);                       // user_agent varint
    buf.extend_from_slice(ua);
    buf.extend_from_slice(&0i32.to_le_bytes());     // start_height
    buf.push(1u8);                                  // relay
    buf
}

/// Read one PKT message header; return command string and skip payload bytes.
fn pkt_recv_cmd(stream: &mut TcpStream) -> Result<String, String> {
    use std::io::Read;
    let mut hdr = [0u8; PKT_HEADER_LEN];
    stream.read_exact(&mut hdr).map_err(|e| format!("read header: {}", e))?;
    if hdr[0..4] != PKT_TESTNET_MAGIC {
        return Err("wrong magic".into());
    }
    let cmd = std::str::from_utf8(&hdr[4..16])
        .unwrap_or("").trim_matches('\0').to_string();
    let payload_len = u32::from_le_bytes([hdr[16], hdr[17], hdr[18], hdr[19]]) as usize;
    if payload_len > 0 {
        let mut payload = vec![0u8; payload_len.min(4_000_000)];
        stream.read_exact(&mut payload).map_err(|e| format!("read payload: {}", e))?;
    }
    Ok(cmd)
}

/// Broadcast raw TX bytes trực tiếp đến một PKT testnet peer.
fn pkt_broadcast_sync(raw: &[u8], peer: &str) -> Result<(), String> {
    use std::io::Write;
    use std::net::ToSocketAddrs;

    let addr = peer.to_socket_addrs()
        .map_err(|e| format!("resolve {}: {}", peer, e))?
        .next()
        .ok_or_else(|| format!("cannot resolve {}", peer))?;

    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))
        .map_err(|e| format!("connect: {}", e))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();

    // → Version
    stream.write_all(&pkt_frame("version", &pkt_version_payload()))
        .map_err(|e| format!("send version: {}", e))?;

    // ← Version + Verack  (send Verack on receiving their Version)
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut got_version = false;
    let mut got_verack  = false;
    while !(got_version && got_verack) {
        if std::time::Instant::now() > deadline { return Err("handshake timeout".into()); }
        match pkt_recv_cmd(&mut stream)?.as_str() {
            "version" => {
                got_version = true;
                stream.write_all(&pkt_frame("verack", &[]))
                    .map_err(|e| format!("send verack: {}", e))?;
            }
            "verack" => { got_verack = true; }
            _ => {}
        }
    }

    // → TX
    stream.write_all(&pkt_frame("tx", raw))
        .map_err(|e| format!("send tx: {}", e))?;
    stream.flush().map_err(|e| format!("flush: {}", e))?;

    // ← wait 3s for "reject"
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
    loop {
        match pkt_recv_cmd(&mut stream) {
            Ok(cmd) if cmd == "reject" => return Err("rejected by peer".into()),
            Ok(_)  => {}
            Err(_) => break, // timeout = no reject = OK
        }
    }
    Ok(())
}

/// Broadcast raw hex TX trực tiếp đến PKT testnet peer (không qua HTTP server).
#[tauri::command]
async fn tx_broadcast(_node_url: String, raw_hex: String) -> Result<serde_json::Value, String> {
    let raw = hex::decode(raw_hex.trim()).map_err(|e| format!("hex decode: {}", e))?;

    // Compute SHA256d TXID (display: reversed)
    let txid = {
        use sha2::Sha256;
        let h1 = Sha256::digest(&raw);
        let h2 = Sha256::digest(&h1);
        let mut b: [u8; 32] = h2.into();
        b.reverse();
        hex::encode(b)
    };

    let default_peer = pkt_core::pkt_config::get().seed_p2p();
    tokio::task::spawn_blocking(move || pkt_broadcast_sync(&raw, &default_peer))
        .await
        .map_err(|e| e.to_string())?
        .map(|_| serde_json::json!({"txid": txid, "status": "broadcast"}))
}


// ── Miner commands ────────────────────────────────────────────────────────────

/// Bắt đầu mine: spawn background thread, emit "mine_log" + "mine_stats" events.
#[tauri::command]
async fn start_mine(
    app: tauri::AppHandle,
    address: String,
    node_addr: String,
    threads: usize,
) -> Result<(), String> {
    if MINER_RUNNING.load(Ordering::SeqCst) {
        return Err("Miner đang chạy".to_string());
    }
    let pubkey_hash_bytes = parse_miner_address(&address)?;
    let pubkey_hash_hex   = hex::encode(&pubkey_hash_bytes);
    MINER_STOP.store(false, Ordering::SeqCst);
    MINER_RUNNING.store(true, Ordering::SeqCst);
    std::thread::spawn(move || {
        miner_loop(app, pubkey_hash_hex, node_addr, threads.max(1));
        MINER_RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(())
}

/// Dừng miner (set MINER_STOP flag, rayon workers thoát ≤50k hashes).
#[tauri::command]
fn stop_mine() {
    MINER_STOP.store(true, Ordering::SeqCst);
}

/// Trả về true nếu miner đang chạy.
#[tauri::command]
fn mine_status() -> bool {
    MINER_RUNNING.load(Ordering::SeqCst)
}

// ── Miner loop (sync, chạy trong std::thread) ─────────────────────────────────

fn emit_log(app: &tauri::AppHandle, msg: &str) {
    let _ = app.emit("mine_log", msg.to_string());
}

fn miner_loop(app: tauri::AppHandle, pubkey_hash_hex: String, node_addr: String, threads: usize) {
    let total_hashes  = Arc::new(AtomicU64::new(0));
    let blocks_mined  = Arc::new(AtomicU64::new(0));
    let session_start = std::time::Instant::now();

    // Progress reporter: emit mine_stats every 800ms
    {
        let app2    = app.clone();
        let th      = Arc::clone(&total_hashes);
        let bm      = Arc::clone(&blocks_mined);
        std::thread::spawn(move || {
            let mut last_h = 0u64;
            let mut last_t = std::time::Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(800));
                let h   = th.load(Ordering::Relaxed);
                let dt  = last_t.elapsed().as_secs_f64().max(0.001);
                let rate = ((h.saturating_sub(last_h)) as f64 / dt) as u64;
                last_h  = h;
                last_t  = std::time::Instant::now();
                let _ = app2.emit("mine_stats", serde_json::json!({
                    "hashrate":     rate,
                    "total_hashes": h,
                    "blocks_mined": bm.load(Ordering::Relaxed),
                    "uptime_secs":  session_start.elapsed().as_secs(),
                }));
                if MINER_STOP.load(Ordering::Relaxed) && !MINER_RUNNING.load(Ordering::Relaxed) {
                    break;
                }
            }
        });
    }

    emit_log(&app, &format!("⛏ PKTScan Miner bắt đầu — node: {}", node_addr));
    emit_log(&app, &format!("  Threads: {}  |  Reward: {}", threads, &pubkey_hash_hex[..8]));

    loop {
        if MINER_STOP.load(Ordering::Relaxed) { break; }

        // ── 1. Lấy block template từ node ────────────────────────────────────
        emit_log(&app, &format!("→ GetTemplate từ {}", node_addr));
        let tmpl = match get_template_tcp(&node_addr) {
            Ok(t)  => t,
            Err(e) => {
                emit_log(&app, &format!("⚠️ Node error: {} — thử lại 5s", e));
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        };
        emit_log(&app, &format!("  Template #{} diff={} txs={}",
            tmpl.height, tmpl.difficulty, tmpl.txs.len()));

        // ── 2. Build coinbase TX ──────────────────────────────────────────────
        let fee: u64 = tmpl.txs.iter()
            .map(|t| t["fee"].as_u64().unwrap_or(0))
            .sum();
        let era     = (tmpl.height / pkt_core::pkt_genesis::HALVING_INTERVAL).min(63);
        let subsidy = pkt_core::pkt_genesis::INITIAL_BLOCK_REWARD >> era;
        let amount  = subsidy + fee;

        let cb_txid  = compute_coinbase_txid(tmpl.height, amount);
        let cb_wtxid = compute_coinbase_wtxid(tmpl.height, amount);

        // ── 3. Merkle roots ───────────────────────────────────────────────────
        let mut tx_ids:  Vec<String> = vec![cb_txid.clone()];
        let mut wtx_ids: Vec<String> = vec![cb_wtxid.clone()];
        for tx in &tmpl.txs {
            tx_ids.push(tx["tx_id"].as_str().unwrap_or("").to_string());
            wtx_ids.push(tx["wtx_id"].as_str().unwrap_or("").to_string());
        }
        let txid_root    = merkle_root(tx_ids);
        let witness_root = merkle_root(wtx_ids);

        // ── 4. Mine PoW ───────────────────────────────────────────────────────
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let prefix = format!("{}|{}|{}|{}|{}|",
            tmpl.height, timestamp, txid_root, witness_root, tmpl.prev_hash);
        let target = "0".repeat(tmpl.difficulty);
        let n      = threads;
        let chunk  = u64::MAX / n as u64;

        let round_hashes = Arc::new(AtomicU64::new(0));
        let stop_flag    = Arc::new(AtomicBool::new(false));

        // Watch MINER_STOP → stop_flag
        {
            let sf = Arc::clone(&stop_flag);
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(50));
                    if MINER_STOP.load(Ordering::Relaxed) {
                        sf.store(true, Ordering::Relaxed);
                        break;
                    }
                    if sf.load(Ordering::Relaxed) { break; }
                }
            });
        }

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build();
        let pool = match pool {
            Ok(p)  => p,
            Err(e) => { emit_log(&app, &format!("Pool error: {}", e)); break; }
        };

        let prefix2 = prefix.clone();
        let target2 = target.clone();
        let rh      = Arc::clone(&round_hashes);
        let sf2     = Arc::clone(&stop_flag);

        let found = pool.install(|| {
            (0..n).into_par_iter().find_map_any(|tid| {
                let start  = (tid as u64).saturating_mul(chunk);
                let end    = if tid == n - 1 { u64::MAX } else { start.saturating_add(chunk) };
                let mut local = 0u64;
                for nonce in start..end {
                    if sf2.load(Ordering::Relaxed) {
                        rh.fetch_add(local, Ordering::Relaxed);
                        return None;
                    }
                    let header = format!("{}{}", prefix2, nonce);
                    let hash   = hex::encode(blake3::hash(header.as_bytes()).as_bytes());
                    local += 1;
                    if local % 50_000 == 0 {
                        rh.fetch_add(50_000, Ordering::Relaxed);
                        local = 0;
                    }
                    if hash.starts_with(&target2) {
                        sf2.store(true, Ordering::Relaxed);
                        rh.fetch_add(local, Ordering::Relaxed);
                        return Some((nonce, hash));
                    }
                }
                rh.fetch_add(local, Ordering::Relaxed);
                None
            })
        });

        // Accumulate hashes từ round này vào session total
        let round_h = round_hashes.load(Ordering::Relaxed);
        total_hashes.fetch_add(round_h, Ordering::Relaxed);

        if MINER_STOP.load(Ordering::Relaxed) { break; }

        // ── 5. Submit block ───────────────────────────────────────────────────
        if let Some((nonce, hash)) = found {
            emit_log(&app, &format!("🎉 Block #{} found! nonce={} hash={}...{}",
                tmpl.height, nonce, &hash[..8], &hash[56..]));

            let coinbase_json = build_coinbase_json(
                &pubkey_hash_hex, tmpl.height, amount, &cb_txid, &cb_wtxid);
            let mut all_txs: Vec<serde_json::Value> = vec![coinbase_json];
            all_txs.extend(tmpl.txs.clone());

            let block_json = serde_json::json!({
                "index":        tmpl.height,
                "timestamp":    timestamp,
                "transactions": all_txs,
                "prev_hash":    tmpl.prev_hash,
                "nonce":        nonce,
                "hash":         hash,
                "witness_root": witness_root,
            });

            match submit_block_tcp(&node_addr, block_json) {
                Ok(tip) => {
                    emit_log(&app, &format!("✅ Block accepted! Node height={}", tip));
                    blocks_mined.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    emit_log(&app, &format!("⚠️ Submit error: {}", e));
                }
            }
        }
    }

    emit_log(&app, &format!("⏹ Mining stopped. Blocks: {}  Hashes: {}",
        blocks_mined.load(Ordering::Relaxed),
        fmt_big(total_hashes.load(Ordering::Relaxed))));
}

// ── Miner helpers ─────────────────────────────────────────────────────────────

struct MineTemplate {
    prev_hash:  String,
    height:     u64,
    difficulty: usize,
    txs:        Vec<serde_json::Value>,
}

fn get_template_tcp(node_addr: &str) -> Result<MineTemplate, String> {
    let msg = b"{\"GetTemplate\":null}\n";
    let mut stream = TcpStream::connect(node_addr)
        .map_err(|e| format!("connect: {}", e))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.write_all(msg).map_err(|e| format!("write: {}", e))?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| format!("read: {}", e))?;
    let v: serde_json::Value = serde_json::from_str(line.trim())
        .map_err(|e| format!("parse: {}", e))?;
    let t = &v["Template"];
    if t.is_null() { return Err("Node trả về lỗi (không có Template)".to_string()); }
    Ok(MineTemplate {
        prev_hash:  t["prev_hash"].as_str().unwrap_or("").to_string(),
        height:     t["height"].as_u64().unwrap_or(0),
        difficulty: t["difficulty"].as_u64().unwrap_or(3) as usize,
        txs:        t["txs"].as_array().cloned().unwrap_or_default(),
    })
}

fn submit_block_tcp(node_addr: &str, block: serde_json::Value) -> Result<u64, String> {
    let msg = serde_json::json!({"NewBlock": {"block": block}});
    let mut msg_str = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
    msg_str.push('\n');
    let mut stream = TcpStream::connect(node_addr)
        .map_err(|e| format!("connect: {}", e))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.write_all(msg_str.as_bytes()).map_err(|e| format!("write: {}", e))?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| format!("read: {}", e))?;
    let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap_or_default();
    Ok(v["Height"]["height"].as_u64().unwrap_or(0))
}

/// Giải mã địa chỉ PKT: EVM (0x...), hex 40-char, hoặc bech32 legacy.
fn parse_miner_address(addr: &str) -> Result<Vec<u8>, String> {
    let a = addr.trim();
    // EVM address: 0x + 40 hex chars
    if a.starts_with("0x") || a.starts_with("0X") {
        return parse_evm_address(a).map(|b| b.to_vec());
    }
    // Hex pubkey_hash (40 hex chars = 20 bytes, no prefix)
    if a.len() == 40 && a.chars().all(|c| c.is_ascii_hexdigit()) {
        return hex::decode(a).map_err(|e| format!("địa chỉ không hợp lệ: {}", e));
    }
    // bech32 legacy: tpkt1/pkt1/rpkt1
    if a.starts_with("pkt1") || a.starts_with("tpkt1") || a.starts_with("rpkt1") {
        return decode_bech32_witprog(a);
    }
    Err(format!("địa chỉ không hợp lệ: {}", a))
}


fn decode_bech32_witprog(addr: &str) -> Result<Vec<u8>, String> {
    const CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    let lower = addr.to_ascii_lowercase();
    let sep   = lower.rfind('1').ok_or("thiếu '1' separator")?;
    let data_str = &lower[sep + 1..];
    if data_str.len() < 7 { return Err("địa chỉ quá ngắn".to_string()); }
    let mut data = Vec::with_capacity(data_str.len());
    for c in data_str.chars() {
        let idx = CHARSET.iter().position(|&x| x == c as u8)
            .ok_or_else(|| format!("ký tự không hợp lệ: '{}'", c))?;
        data.push(idx as u8);
    }
    // data[0] = witness version, last 6 = checksum → strip both
    let payload = &data[1..data.len() - 6];
    convertbits5to8(payload).ok_or("convertbits thất bại".to_string())
}

fn convertbits5to8(data: &[u8]) -> Option<Vec<u8>> {
    let mut acc: u32  = 0;
    let mut bits: u32 = 0;
    let mut ret = Vec::new();
    for &b in data {
        if (b as u32) >> 5 != 0 { return None; }
        acc   = (acc << 5) | b as u32;
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            ret.push(((acc >> bits) & 0xff) as u8);
        }
    }
    if bits >= 5 || ((acc << (8 - bits)) & 0xff) != 0 { return None; }
    Some(ret)
}

/// Coinbase txid = blake3("txid|[("<zeros64>", height, 4294967295)]|[amount]|true")
fn compute_coinbase_txid(height: u64, amount: u64) -> String {
    let data = format!(
        "txid|[(\"{}\", {}, {})]|[{}]|true",
        "0".repeat(64), height as usize, 0xFFFF_FFFFu32, amount,
    );
    hex::encode(blake3::hash(data.as_bytes()).as_bytes())
}

/// Coinbase wtxid — witness rỗng, thêm "[[]]" ở cuối
fn compute_coinbase_wtxid(height: u64, amount: u64) -> String {
    let data = format!(
        "wtxid|[(\"{}\", {}, {})]|[{}]|true|[[]]",
        "0".repeat(64), height as usize, 0xFFFF_FFFFu32, amount,
    );
    hex::encode(blake3::hash(data.as_bytes()).as_bytes())
}

/// Binary Merkle tree (blake3) — giống Block::merkle_root trong main crate.
fn merkle_root(mut hashes: Vec<String>) -> String {
    if hashes.is_empty() { return "0".repeat(64); }
    while hashes.len() > 1 {
        if hashes.len() % 2 == 1 {
            let last = hashes.last().unwrap().clone();
            hashes.push(last);
        }
        hashes = hashes.chunks(2).map(|p| {
            let combined = format!("{}{}", p[0], p[1]);
            hex::encode(blake3::hash(combined.as_bytes()).as_bytes())
        }).collect();
    }
    hashes.into_iter().next().unwrap_or_else(|| "0".repeat(64))
}

/// Build coinbase TX JSON theo format serde của Transaction trong main crate.
fn build_coinbase_json(
    pubkey_hash_hex: &str,
    height: u64,
    amount: u64,
    tx_id: &str,
    wtx_id: &str,
) -> serde_json::Value {
    let hash_bytes: Vec<serde_json::Value> = hex::decode(pubkey_hash_hex)
        .unwrap_or_default()
        .into_iter()
        .map(|b| serde_json::Value::Number(b.into()))
        .collect();
    serde_json::json!({
        "tx_id":  tx_id,
        "wtx_id": wtx_id,
        "inputs": [{
            "tx_id":        "0".repeat(64),
            "output_index": height as u64,
            "script_sig":   { "ops": [] },
            "sequence":     0xFFFF_FFFFu32,
            "witness":      []
        }],
        "outputs": [{
            "amount": amount,
            "script_pubkey": {
                "ops": [
                    "OpDup",
                    "OpHash160",
                    { "OpPushData": hash_bytes },
                    "OpEqualVerify",
                    "OpCheckSig"
                ]
            }
        }],
        "is_coinbase": true,
        "fee": 0u64
    })
}

fn fmt_big(n: u64) -> String {
    if n >= 1_000_000_000 { format!("{:.2}G", n as f64 / 1e9) }
    else if n >= 1_000_000 { format!("{:.2}M", n as f64 / 1e6) }
    else if n >= 1_000     { format!("{:.1}K", n as f64 / 1e3) }
    else                   { format!("{}", n) }
}

// ── Peer scan ─────────────────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
struct PeerInfo {
    addr:       String,
    latency_ms: Option<u64>,
    height:     Option<u64>,
    status:     String, // "online" | "timeout" | "refused"
}

/// Probe một peer qua PKT wire protocol: TCP connect → Version/Verack → lấy height.
fn probe_peer(addr: &str) -> PeerInfo {
    let sock_addr: std::net::SocketAddr = match addr.parse() {
        Ok(a) => a,
        Err(_) => return PeerInfo {
            addr: addr.to_string(), latency_ms: None, height: None, status: "invalid".into(),
        },
    };
    let t0 = std::time::Instant::now();
    match TcpStream::connect_timeout(&sock_addr, Duration::from_secs(5)) {
        Ok(mut stream) => {
            let latency_ms = t0.elapsed().as_millis() as u64;
            stream.set_read_timeout(Some(Duration::from_secs(4))).ok();
            let magic = pkt_core::pkt_wire::TESTNET_MAGIC;
            let cfg = pkt_core::pkt_peer::PeerConfig {
                host: sock_addr.ip().to_string(),
                port: sock_addr.port(),
                magic,
                ..Default::default()
            };
            let height = pkt_core::pkt_peer::do_handshake(&mut stream, &cfg)
                .ok()
                .map(|info| info.start_height.max(0) as u64);
            PeerInfo { addr: addr.to_string(), latency_ms: Some(latency_ms), height, status: "online".into() }
        }
        Err(e) => {
            let status = if e.kind() == std::io::ErrorKind::ConnectionRefused { "refused" } else { "timeout" };
            PeerInfo { addr: addr.to_string(), latency_ms: None, height: None, status: status.into() }
        }
    }
}

/// Kết nối seed qua PKT wire protocol → GetAddr → probe từng peer song song.
#[tauri::command]
async fn peer_scan(seed_addr: String) -> Result<Vec<PeerInfo>, String> {
    let seed = seed_addr.trim().to_string();
    let seed_sock: std::net::SocketAddr = seed.parse()
        .map_err(|_| format!("seed address không hợp lệ: {}", seed))?;

    // Bước 1: PKT handshake + GetAddr → lấy danh sách peers từ seed
    let mut addrs: Vec<String> = vec![seed.clone()];
    if let Ok(mut stream) = TcpStream::connect_timeout(&seed_sock, Duration::from_secs(5)) {
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let magic = pkt_core::pkt_wire::TESTNET_MAGIC;
        let cfg = pkt_core::pkt_peer::PeerConfig {
            host: seed_sock.ip().to_string(),
            port: seed_sock.port(),
            magic,
            ..Default::default()
        };
        if pkt_core::pkt_peer::do_handshake(&mut stream, &cfg).is_ok() {
            let _ = pkt_core::pkt_peer::send_msg(&mut stream, pkt_core::pkt_wire::PktMsg::GetAddr, magic);
            stream.set_read_timeout(Some(Duration::from_secs(4))).ok();
            for _ in 0..10 {
                match pkt_core::pkt_peer::recv_msg(&mut stream, magic) {
                    Ok(pkt_core::pkt_wire::PktMsg::Addr { peers }) => {
                        for p in peers {
                            if let Some(s) = p.to_addr_string() {
                                if s != seed { addrs.push(s); }
                            }
                        }
                        break;
                    }
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
        }
    }

    // Bước 2: Probe song song (tối đa 20 peers, timeout 5s mỗi cái)
    let addrs: Vec<String> = addrs.into_iter().take(20).collect();
    let handles: Vec<_> = addrs.into_iter()
        .map(|a| tokio::task::spawn_blocking(move || probe_peer(&a)))
        .collect();

    let mut results = Vec::new();
    for h in handles {
        if let Ok(info) = h.await {
            results.push(info);
        }
    }
    // Seed trước, online trước, latency tăng dần
    results.sort_by(|a, b| {
        let ord = |s: &str| match s { "online" => 0, "refused" => 1, _ => 2 };
        ord(&a.status).cmp(&ord(&b.status))
            .then(a.latency_ms.unwrap_or(u64::MAX).cmp(&b.latency_ms.unwrap_or(u64::MAX)))
    });
    Ok(results)
}

// ── Wallet ────────────────────────────────────────────────────────────────────

/// PKT hash160: RIPEMD160(BLAKE3(compressed_pubkey)) — PKT dùng blake3 thay SHA256.
/// Address format: "0x" + hex(hash160) — 42 ký tự.
/// Consistent với pkt_script::verify_p2pkh_input.
fn evm_address_from_pubkey(pk: &secp256k1::PublicKey) -> String {
    use ripemd::{Ripemd160, Digest as _};
    let compressed = pk.serialize(); // 33 bytes
    let b3   = blake3::hash(&compressed);
    let hash160: [u8; 20] = Ripemd160::digest(b3.as_bytes()).into();
    format!("0x{}", hex::encode(hash160))
}


/// Parse EVM address `0x...` → [u8; 20].
fn parse_evm_address(addr: &str) -> Result<[u8; 20], String> {
    let hex = addr.strip_prefix("0x").or_else(|| addr.strip_prefix("0X"))
        .ok_or_else(|| format!("địa chỉ EVM phải bắt đầu bằng 0x: {addr}"))?;
    if hex.len() != 40 {
        return Err(format!("địa chỉ EVM phải có 40 ký tự hex, got {}", hex.len()));
    }
    let bytes = hex::decode(hex).map_err(|e| format!("hex decode lỗi: {e}"))?;
    bytes.try_into().map_err(|_| "slice error".to_string())
}

/// Tạo ví PKT mới: secp256k1 keypair → EVM address (RIPEMD160(BLAKE3)).
#[tauri::command]
fn wallet_generate(_network: String) -> Result<serde_json::Value, String> {
    let secp = secp256k1::Secp256k1::new();
    let (secret_key, public_key) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
    let address = evm_address_from_pubkey(&public_key);
    let privkey = hex::encode(secret_key.secret_bytes());
    let pubkey  = hex::encode(public_key.serialize());
    Ok(serde_json::json!({ "address": address, "privkey_hex": privkey, "pubkey_hex": pubkey }))
}

/// Khôi phục ví từ private key hex → EVM address.
#[tauri::command]
fn wallet_from_privkey(privkey_hex: String, _network: String) -> Result<serde_json::Value, String> {
    let bytes = hex::decode(privkey_hex.trim())
        .map_err(|_| "private key hex không hợp lệ".to_string())?;
    let secp = secp256k1::Secp256k1::new();
    let sk   = secp256k1::SecretKey::from_slice(&bytes)
        .map_err(|e| format!("private key lỗi: {}", e))?;
    let pk      = secp256k1::PublicKey::from_secret_key(&secp, &sk);
    let address = evm_address_from_pubkey(&pk);
    let pubkey  = hex::encode(pk.serialize());
    Ok(serde_json::json!({ "address": address, "pubkey_hex": pubkey }))
}

/// Khôi phục ví từ BIP39 seed phrase → EVM address (BIP44 m/44'/60'/0'/0/0).
/// Tương thích MetaMask, Trust Wallet, Ledger (Ethereum coin type 60).
#[tauri::command]
fn wallet_from_mnemonic(mnemonic: String, passphrase: String) -> Result<serde_json::Value, String> {
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    if words.len() != 12 && words.len() != 24 {
        return Err(format!("Seed phrase phải có 12 hoặc 24 từ (bạn nhập {} từ)", words.len()));
    }

    // BIP39: mnemonic → seed via PBKDF2-HMAC-SHA512
    let mnemonic_bytes = mnemonic.trim().as_bytes();
    let salt           = format!("mnemonic{}", passphrase.trim());
    let mut seed       = [0u8; 64];
    pbkdf2::pbkdf2_hmac::<Sha512>(mnemonic_bytes, salt.as_bytes(), 2048, &mut seed);

    // BIP32: seed → master key via HMAC-SHA512("Bitcoin seed", seed)
    let (mut sk_bytes, mut chain_code) = bip32_master(&seed)?;

    // Derive m/44'/60'/0'/0/0 — coin type 60 = Ethereum/EVM
    for (index, hardened) in [(44u32, true), (60, true), (0, true), (0, false), (0, false)] {
        let idx = if hardened { index + 0x8000_0000 } else { index };
        (sk_bytes, chain_code) = bip32_child_private(&sk_bytes, &chain_code, idx)?;
    }

    let secp = secp256k1::Secp256k1::new();
    let sk   = secp256k1::SecretKey::from_slice(&sk_bytes)
        .map_err(|e| format!("derived key lỗi: {}", e))?;
    let pk      = secp256k1::PublicKey::from_secret_key(&secp, &sk);
    let address = evm_address_from_pubkey(&pk);
    let privkey = hex::encode(sk.secret_bytes());
    let pubkey  = hex::encode(pk.serialize());
    Ok(serde_json::json!({ "address": address, "privkey_hex": privkey, "pubkey_hex": pubkey }))
}

/// BIP32 master key từ seed: HMAC-SHA512("Bitcoin seed", seed).
fn bip32_master(seed: &[u8]) -> Result<([u8; 32], [u8; 32]), String> {
    type HmacSha512 = Hmac<Sha512>;
    let mut mac = HmacSha512::new_from_slice(b"Bitcoin seed")
        .map_err(|e| format!("HMAC init lỗi: {}", e))?;
    mac.update(seed);
    let result = mac.finalize().into_bytes();
    let mut sk = [0u8; 32];
    let mut cc = [0u8; 32];
    sk.copy_from_slice(&result[..32]);
    cc.copy_from_slice(&result[32..]);
    Ok((sk, cc))
}

/// BIP32 child private key derivation.
/// Hardened (index ≥ 0x80000000): data = 0x00 || parent_sk || index_be
/// Normal:                         data = compressed_pubkey || index_be
fn bip32_child_private(sk: &[u8; 32], cc: &[u8; 32], index: u32) -> Result<([u8; 32], [u8; 32]), String> {
    use secp256k1::{Secp256k1, SecretKey, PublicKey};
    type HmacSha512 = Hmac<Sha512>;

    let mut mac = HmacSha512::new_from_slice(cc)
        .map_err(|e| format!("HMAC init lỗi: {}", e))?;

    if index >= 0x8000_0000 {
        // Hardened
        mac.update(&[0x00]);
        mac.update(sk);
    } else {
        // Normal: dùng compressed pubkey
        let secp = Secp256k1::new();
        let parent_sk = SecretKey::from_slice(sk).map_err(|e| e.to_string())?;
        let parent_pk = PublicKey::from_secret_key(&secp, &parent_sk);
        mac.update(&parent_pk.serialize());
    }
    mac.update(&index.to_be_bytes());

    let result = mac.finalize().into_bytes();
    let il = &result[..32];
    let ir = &result[32..];

    // child_key = (IL + parent_key) mod n
    let parent = secp256k1::SecretKey::from_slice(sk).map_err(|e| e.to_string())?;
    let il_scalar = secp256k1::SecretKey::from_slice(il)
        .map_err(|_| "IL không hợp lệ (≥ curve order)".to_string())?;
    let child = parent.add_tweak(&secp256k1::Scalar::from(il_scalar))
        .map_err(|e| format!("child key tweak lỗi: {}", e))?;

    let mut child_bytes = [0u8; 32];
    let mut child_cc    = [0u8; 32];
    child_bytes.copy_from_slice(&child.secret_bytes());
    child_cc.copy_from_slice(ir);
    Ok((child_bytes, child_cc))
}

// ── Wallet: build + sign tx ────────────────────────────────────────────────────

/// Chuẩn hoá script_pubkey hex về dạng wire P2PKH (76a914{20 bytes}88ac).
///
/// UTXO indexed trước v22.x lưu script dưới dạng JSON hex:
///   hex( {"ops":["OpDup","OpHash160",{"OpPushData":[b0..b19]},"OpEqualVerify","OpCheckSig"]} )
/// Hàm này detect format đó và rebuild wire bytes.
/// Wire format (76a914…88ac) được trả về nguyên.
fn normalize_script_pubkey(hex_str: &str) -> Result<Vec<u8>, String> {
    let bytes = hex::decode(hex_str).map_err(|_| "script_pubkey hex lỗi".to_string())?;
    // Wire P2PKH: 25 bytes, bắt đầu 76 a9 14, kết thúc 88 ac
    if bytes.len() == 25 && bytes[0] == 0x76 && bytes[1] == 0xa9 && bytes[2] == 0x14
        && bytes[23] == 0x88 && bytes[24] == 0xac {
        return Ok(bytes);
    }
    // Legacy JSON format
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        if let Some(ops) = json["ops"].as_array() {
            for op in ops {
                if let Some(arr) = op["OpPushData"].as_array() {
                    let hash160: Vec<u8> = arr.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as u8))
                        .collect();
                    if hash160.len() == 20 {
                        let mut script = vec![0x76u8, 0xa9, 0x14];
                        script.extend_from_slice(&hash160);
                        script.push(0x88);
                        script.push(0xac);
                        return Ok(script);
                    }
                }
            }
        }
    }
    Err(format!("không nhận dạng được script_pubkey format ({} bytes)", bytes.len()))
}

/// Input UTXO từ frontend.
#[derive(serde::Deserialize)]
struct UtxoInput {
    txid:          String, // txid hex (big-endian display order)
    vout:          u32,
    value:         u64,    // paklets
    script_pubkey: String, // script_pubkey hex (P2PKH wire hoặc legacy JSON hex)
}

/// Build + sign P2PKH legacy transaction (PKT dùng P2PKH, không phải segwit).
/// Signing: BLAKE3(BLAKE3(preimage)) — PKT dùng blake3-double-hash thay SHA256d.
#[tauri::command]
fn wallet_tx_build(
    privkey_hex: String,
    inputs:      Vec<UtxoInput>,
    to_addr:     String,
    amount_sat:  u64,
    fee_sat:     u64,
    change_addr: String,
    _network:    String,
) -> Result<serde_json::Value, String> {
    use secp256k1::{Secp256k1, SecretKey, Message};

    let secp     = Secp256k1::new();
    let sk       = SecretKey::from_slice(
        &hex::decode(privkey_hex.trim()).map_err(|_| "privkey hex lỗi".to_string())?
    ).map_err(|e| format!("privkey lỗi: {}", e))?;
    let pk       = secp256k1::PublicKey::from_secret_key(&secp, &sk);
    let pk_bytes = pk.serialize(); // 33 bytes compressed

    let to_script  = addr_to_p2pkh_script(&to_addr)?;
    let chg_script = addr_to_p2pkh_script(&change_addr)?;

    let total_in: u64 = inputs.iter().map(|u| u.value).sum();
    if total_in < amount_sat + fee_sat {
        return Err(format!("số dư không đủ: {} < {} + {}", total_in, amount_sat, fee_sat));
    }
    let change = total_in - amount_sat - fee_sat;
    let n_in   = inputs.len() as u64;
    let n_out  = if change > 0 { 2u64 } else { 1u64 };

    // P2PKH legacy signing: cho mỗi input, serialize tx với scriptSig = scriptPubKey của input đó,
    // các input còn lại scriptSig = empty, append SIGHASH_ALL(4LE), hash = blake3(blake3(preimage)).
    let mut sigs: Vec<Vec<u8>> = Vec::new();
    for (i, inp) in inputs.iter().enumerate() {
        let sp_bytes = normalize_script_pubkey(&inp.script_pubkey)?;

        let mut preimage: Vec<u8> = Vec::new();
        preimage.extend_from_slice(&1u32.to_le_bytes()); // version
        write_varint(&mut preimage, n_in);
        for (j, inp2) in inputs.iter().enumerate() {
            let mut txid_bytes = hex::decode(&inp2.txid).map_err(|_| "txid hex lỗi")?;
            txid_bytes.reverse(); // display → wire LE
            preimage.extend_from_slice(&txid_bytes);
            preimage.extend_from_slice(&inp2.vout.to_le_bytes());
            if j == i {
                write_varint(&mut preimage, sp_bytes.len() as u64);
                preimage.extend_from_slice(&sp_bytes);
            } else {
                preimage.push(0x00); // empty scriptSig
            }
            preimage.extend_from_slice(&0xffff_ffffu32.to_le_bytes()); // sequence
        }
        write_varint(&mut preimage, n_out);
        write_tx_output(&mut preimage, amount_sat, &to_script);
        if change > 0 { write_tx_output(&mut preimage, change, &chg_script); }
        preimage.extend_from_slice(&0u32.to_le_bytes());  // locktime
        preimage.extend_from_slice(&1u32.to_le_bytes());  // SIGHASH_ALL

        // PKT sighash: BLAKE3(BLAKE3(preimage)) — xem pkt_script::pkt_double_hash
        let h1  = blake3::hash(&preimage);
        let h2  = blake3::hash(h1.as_bytes());
        let msg = Message::from_slice(h2.as_bytes()).map_err(|e| format!("msg error: {}", e))?;
        let sig = secp.sign_ecdsa(&msg, &sk);
        let mut der = sig.serialize_der().to_vec();
        der.push(0x01); // SIGHASH_ALL
        sigs.push(der);
    }

    // Serialize final tx: version + inputs(với scriptSig) + outputs + locktime
    let mut raw: Vec<u8> = Vec::new();
    raw.extend_from_slice(&1u32.to_le_bytes()); // version
    write_varint(&mut raw, n_in);
    for (i, inp) in inputs.iter().enumerate() {
        let mut txid_bytes = hex::decode(&inp.txid).map_err(|_| "txid hex lỗi")?;
        txid_bytes.reverse();
        raw.extend_from_slice(&txid_bytes);
        raw.extend_from_slice(&inp.vout.to_le_bytes());
        // scriptSig = <sig_len><sig><pk_len><pk>
        let sig = &sigs[i];
        let script_sig_len = 1 + sig.len() + 1 + pk_bytes.len();
        write_varint(&mut raw, script_sig_len as u64);
        raw.push(sig.len() as u8);
        raw.extend_from_slice(sig);
        raw.push(pk_bytes.len() as u8);
        raw.extend_from_slice(&pk_bytes);
        raw.extend_from_slice(&0xffff_ffffu32.to_le_bytes()); // sequence
    }
    write_varint(&mut raw, n_out);
    write_tx_output(&mut raw, amount_sat, &to_script);
    if change > 0 { write_tx_output(&mut raw, change, &chg_script); }
    raw.extend_from_slice(&0u32.to_le_bytes()); // locktime

    // txid = SHA256d(raw) reversed (PKT wire protocol = Bitcoin wire)
    let h1   = sha2::Sha256::digest(&raw);
    let h2   = sha2::Sha256::digest(&h1);
    let mut txid_bytes = [0u8; 32];
    txid_bytes.copy_from_slice(&h2);
    txid_bytes.reverse();
    let txid = hex::encode(txid_bytes);

    Ok(serde_json::json!({
        "raw_hex":    hex::encode(&raw),
        "txid":       txid,
        "fee_sat":    fee_sat,
        "amount_sat": amount_sat,
        "change_sat": change,
    }))
}

fn write_varint(buf: &mut Vec<u8>, n: u64) {
    if n < 0xfd {
        buf.push(n as u8);
    } else if n <= 0xffff {
        buf.push(0xfd);
        buf.extend_from_slice(&(n as u16).to_le_bytes());
    } else if n <= 0xffff_ffff {
        buf.push(0xfe);
        buf.extend_from_slice(&(n as u32).to_le_bytes());
    } else {
        buf.push(0xff);
        buf.extend_from_slice(&n.to_le_bytes());
    }
}

fn write_tx_output(buf: &mut Vec<u8>, value: u64, script: &[u8]) {
    buf.extend_from_slice(&value.to_le_bytes());
    write_varint(buf, script.len() as u64);
    buf.extend_from_slice(script);
}

/// Decode EVM address → 25-byte scriptPubKey (OP_DUP OP_HASH160 <20> OP_EQUALVERIFY OP_CHECKSIG).
fn addr_to_p2pkh_script(addr: &str) -> Result<Vec<u8>, String> {
    let hash20 = parse_evm_address(addr)?;
    let mut script = Vec::with_capacity(25);
    script.push(0x76); // OP_DUP
    script.push(0xa9); // OP_HASH160
    script.push(0x14); // push 20 bytes
    script.extend_from_slice(&hash20);
    script.push(0x88); // OP_EQUALVERIFY
    script.push(0xac); // OP_CHECKSIG
    Ok(script)
}

// ── App entry ─────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Khởi động embedded API server (testnet mặc định)
    spawn_embedded_server(false);

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            // Sync
            start_sync,
            is_sync_running,
            // Miner
            start_mine,
            stop_mine,
            mine_status,
            // Network
            peer_scan,
            // Wallet
            wallet_generate,
            wallet_from_privkey,
            wallet_from_mnemonic,
            wallet_tx_build,
            tx_broadcast,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PKTScan desktop");
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evm_address_format() {
        use secp256k1::rand::{rngs::StdRng, SeedableRng};
        let secp = secp256k1::Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut StdRng::seed_from_u64(0));
        let addr = evm_address_from_pubkey(&pk);
        assert!(addr.starts_with("0x"), "EVM address phải bắt đầu bằng 0x");
        assert_eq!(addr.len(), 42, "0x + 40 hex chars");
    }

    #[test]
    fn test_parse_evm_address_roundtrip() {
        use secp256k1::rand::{rngs::StdRng, SeedableRng};
        let secp = secp256k1::Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut StdRng::seed_from_u64(1));
        let addr = evm_address_from_pubkey(&pk);
        let raw  = parse_evm_address(&addr).unwrap();
        let back = format!("0x{}", hex::encode(raw));
        assert_eq!(addr, back, "parse → encode phải idempotent");
    }
}
