#![allow(dead_code)]
//! v24.4 — Public Mining Pool
//!
//! Proxy pool giữa PKT node template server và nhiều miners.
//!
//! ## Architecture
//!
//! ```text
//! Miners ──TCP──► Pool server (miner_port=8337)
//!                      │  cache template
//!                      │  forward NewBlock
//!                 Upstream PKT node (node_addr=127.0.0.1:8334)
//!
//! Browser ──HTTP──► Stats API (stats_port=8338)
//!                   GET /api/pool/stats
//!                   GET /api/pool/workers
//! ```
//!
//! ## Protocol
//! Cùng `crate::message::Message` JSON-lines như `pkt_node.rs` template server.
//! Miners kết nối pool thay vì node trực tiếp — không cần thay đổi miner code.
//!
//! ## CLI
//! ```bash
//! cargo run -- pool [miner_port=8337] [node_addr=127.0.0.1:8334] [stats_port=8338]
//! ```

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, RwLock},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::message::Message;

// ── Constants ─────────────────────────────────────────────────────────────────

pub const DEFAULT_MINER_PORT: u16  = 8337;
pub const DEFAULT_STATS_PORT: u16  = 8338;
pub const DEFAULT_NODE_ADDR:  &str = "127.0.0.1:8334";
/// Refresh template cache nếu quá N giây
const TEMPLATE_TTL_SECS: u64 = 30;

// ── Shared state ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WorkerStats {
    pub worker_id:    String,   // ip:port
    pub blocks_found: u64,
    pub last_seen:    u64,      // unix timestamp
    pub connected:    bool,
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total_blocks:   u64,
    pub total_connects: u64,
    pub start_time:     u64,
    pub node_addr:      String,
    pub template_height: u64,
}

struct CachedTemplate {
    msg:      Message,
    fetched:  Instant,
    height:   u64,
}

pub struct PoolShared {
    pub stats:    RwLock<PoolStats>,
    pub workers:  RwLock<HashMap<String, WorkerStats>>,
    template:     RwLock<Option<CachedTemplate>>,
    node_addr:    String,
}

impl PoolShared {
    pub fn new(node_addr: &str) -> Self {
        let now = unix_now();
        Self {
            stats: RwLock::new(PoolStats {
                total_blocks:    0,
                total_connects:  0,
                start_time:      now,
                node_addr:       node_addr.to_string(),
                template_height: 0,
            }),
            workers:  RwLock::new(HashMap::new()),
            template: RwLock::new(None),
            node_addr: node_addr.to_string(),
        }
    }

    /// Lấy template từ cache hoặc fetch mới từ upstream.
    fn get_or_fetch_template(&self) -> Option<Message> {
        // Kiểm tra cache còn fresh không
        {
            let cache = self.template.read().ok()?;
            if let Some(ref c) = *cache {
                if c.fetched.elapsed().as_secs() < TEMPLATE_TTL_SECS {
                    return Some(c.msg.clone());
                }
            }
        }
        // Fetch từ upstream
        self.refresh_template()
    }

    /// Fetch template mới từ upstream node, update cache.
    pub fn refresh_template(&self) -> Option<Message> {
        let msg = node_rpc(&self.node_addr, &Message::GetTemplate)?;
        let height = if let Message::Template { height, .. } = &msg { *height } else { 0 };
        {
            if let Ok(mut cache) = self.template.write() {
                *cache = Some(CachedTemplate { msg: msg.clone(), fetched: Instant::now(), height });
            }
        }
        if let Ok(mut stats) = self.stats.write() {
            stats.template_height = height;
        }
        Some(msg)
    }

    /// Ghi nhận block được tìm bởi worker_id.
    fn record_block(&self, worker_id: &str) {
        if let Ok(mut stats) = self.stats.write() { stats.total_blocks += 1; }
        if let Ok(mut workers) = self.workers.write() {
            let w = workers.entry(worker_id.to_string()).or_insert_with(|| WorkerStats {
                worker_id:    worker_id.to_string(),
                blocks_found: 0,
                last_seen:    unix_now(),
                connected:    true,
            });
            w.blocks_found += 1;
            w.last_seen = unix_now();
        }
    }

    fn worker_connect(&self, worker_id: &str) {
        if let Ok(mut stats)   = self.stats.write() { stats.total_connects += 1; }
        if let Ok(mut workers) = self.workers.write() {
            let w = workers.entry(worker_id.to_string()).or_insert_with(|| WorkerStats {
                worker_id:    worker_id.to_string(),
                blocks_found: 0,
                last_seen:    unix_now(),
                connected:    true,
            });
            w.connected  = true;
            w.last_seen  = unix_now();
        }
    }

    fn worker_disconnect(&self, worker_id: &str) {
        if let Ok(mut workers) = self.workers.write() {
            if let Some(w) = workers.get_mut(worker_id) {
                w.connected = false;
            }
        }
    }
}

// ── Upstream RPC (same as miner.rs node_rpc) ─────────────────────────────────

fn node_rpc(addr: &str, msg: &Message) -> Option<Message> {
    let mut stream = TcpStream::connect(addr).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    stream.write_all(&msg.serialize()).ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    Message::deserialize(line.trim_end_matches('\n').as_bytes())
}

// ── Per-miner connection handler ─────────────────────────────────────────────

fn handle_miner(stream: TcpStream, shared: Arc<PoolShared>) {
    let peer = stream.peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    shared.worker_connect(&peer);
    println!("[pool] miner connected: {}", peer);

    let cloned = match stream.try_clone() {
        Ok(s)  => s,
        Err(e) => { eprintln!("[pool] clone error for {}: {}", peer, e); return; }
    };

    let mut writer = stream;
    writer.set_write_timeout(Some(Duration::from_secs(15))).ok();

    let reader = BufReader::new(cloned);
    for line in reader.lines() {
        let line = match line { Ok(l) => l, Err(_) => break };
        if line.trim().is_empty() { continue; }
        let msg = match Message::deserialize(line.trim_end_matches('\n').as_bytes()) {
            Some(m) => m,
            None    => { eprintln!("[pool] bad msg from {}: {:?}", peer, &line[..line.len().min(80)]); break; }
        };

        let reply = match msg {
            Message::GetTemplate => {
                match shared.get_or_fetch_template() {
                    Some(t) => t,
                    None    => {
                        eprintln!("[pool] cannot get template for {}", peer);
                        break;
                    }
                }
            }
            Message::NewBlock { block } => {
                println!("[pool] block submitted by {} at height={}", peer,
                    block.index);
                // Forward tới upstream node
                match node_rpc(&shared.node_addr, &Message::NewBlock { block }) {
                    Some(reply) => {
                        shared.record_block(&peer);
                        // Invalidate template cache → miners sẽ lấy template mới
                        if let Ok(mut cache) = shared.template.write() {
                            *cache = None;
                        }
                        reply
                    }
                    None => {
                        eprintln!("[pool] upstream rejected block from {}", peer);
                        Message::Height { height: 0 }
                    }
                }
            }
            Message::GetBlocks { from_index } => {
                // Proxy GetBlocks tới upstream
                match node_rpc(&shared.node_addr, &Message::GetBlocks { from_index }) {
                    Some(r) => r,
                    None    => { eprintln!("[pool] upstream GetBlocks failed"); break; }
                }
            }
            _ => break,
        };

        if writer.write_all(&reply.serialize()).is_err() { break; }
    }

    shared.worker_disconnect(&peer);
    println!("[pool] miner disconnected: {}", peer);
}

// ── Pool server loop ──────────────────────────────────────────────────────────

fn run_pool_server(miner_port: u16, shared: Arc<PoolShared>) {
    let addr = format!("0.0.0.0:{}", miner_port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l)  => l,
        Err(e) => { eprintln!("[pool] cannot bind {}: {}", addr, e); return; }
    };
    println!("[pool] miner port: {} (connect miners here)", addr);

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s)  => s,
            Err(e) => { eprintln!("[pool] accept error: {}", e); continue; }
        };
        let shared = Arc::clone(&shared);
        thread::spawn(move || handle_miner(stream, shared));
    }
}

// ── Stats HTTP API ────────────────────────────────────────────────────────────

fn run_stats_server(stats_port: u16, shared: Arc<PoolShared>) {
    use std::io::{Read, Write as _};

    let addr = format!("0.0.0.0:{}", stats_port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l)  => l,
        Err(e) => { eprintln!("[pool] cannot bind stats {}: {}", addr, e); return; }
    };
    println!("[pool] stats API: http://0.0.0.0:{}/api/pool/stats", stats_port);
    println!("[pool] stats API: http://0.0.0.0:{}/api/pool/workers", stats_port);

    for stream in listener.incoming() {
        let mut stream = match stream { Ok(s) => s, Err(_) => continue };
        let shared = Arc::clone(&shared);
        thread::spawn(move || {
            stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

            let mut req_buf = [0u8; 2048];
            let n = match stream.read(&mut req_buf) { Ok(n) => n, Err(_) => return };
            let req = std::str::from_utf8(&req_buf[..n]).unwrap_or("");

            let path = req.lines().next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");

            let (status, body) = match path {
                "/api/pool/stats" | "/api/pool/stats/" => {
                    let stats = shared.stats.read().map(|s| s.clone());
                    match stats {
                        Ok(s) => {
                            let now  = unix_now();
                            let uptime = now.saturating_sub(s.start_time);
                            let active_workers = shared.workers.read()
                                .map(|w| w.values().filter(|w| w.connected).count())
                                .unwrap_or(0);
                            let total_workers = shared.workers.read()
                                .map(|w| w.len()).unwrap_or(0);
                            let body = serde_json::json!({
                                "node_addr":       s.node_addr,
                                "total_blocks":    s.total_blocks,
                                "total_connects":  s.total_connects,
                                "active_workers":  active_workers,
                                "total_workers":   total_workers,
                                "uptime_secs":     uptime,
                                "template_height": s.template_height,
                            });
                            ("200 OK", serde_json::to_string(&body).unwrap_or_default())
                        }
                        Err(_) => ("500 Internal Server Error", r#"{"error":"lock"}"#.to_string()),
                    }
                }
                "/api/pool/workers" | "/api/pool/workers/" => {
                    match shared.workers.read() {
                        Ok(workers) => {
                            let now = unix_now();
                            let list: Vec<_> = workers.values().map(|w| serde_json::json!({
                                "worker_id":    w.worker_id,
                                "blocks_found": w.blocks_found,
                                "last_seen_secs_ago": now.saturating_sub(w.last_seen),
                                "connected":    w.connected,
                            })).collect();
                            ("200 OK", serde_json::to_vec(&list)
                                .map(|b| String::from_utf8_lossy(&b).to_string())
                                .unwrap_or_default())
                        }
                        Err(_) => ("500 Internal Server Error", r#"{"error":"lock"}"#.to_string()),
                    }
                }
                _ => ("404 Not Found", r#"{"error":"not found"}"#.to_string()),
            };

            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status, body.len(), body
            );
            stream.write_all(response.as_bytes()).ok();
        });
    }
}

// ── Template refresh background thread ───────────────────────────────────────

fn run_template_refresh(shared: Arc<PoolShared>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(TEMPLATE_TTL_SECS));
        shared.refresh_template();
    });
}

// ── CLI entry point ───────────────────────────────────────────────────────────

pub fn cmd_pool(args: &[String]) {
    let miner_port: u16  = args.first().and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_MINER_PORT);
    let node_addr:  &str = args.get(1).map(|s| s.as_str()).unwrap_or(DEFAULT_NODE_ADDR);
    let stats_port: u16  = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_STATS_PORT);

    println!("[pool] v24.4 — PKT Mining Pool");
    println!("[pool] upstream node  : {}", node_addr);
    println!("[pool] miner port     : {}", miner_port);
    println!("[pool] stats API port : {}", stats_port);
    println!("[pool] miners should connect to 0.0.0.0:{}", miner_port);
    println!("[pool] configure miner: cargo run -- mine <addr> 0 127.0.0.1:{}", miner_port);

    let shared = Arc::new(PoolShared::new(node_addr));

    // Fetch initial template
    match shared.refresh_template() {
        Some(Message::Template { height, .. }) =>
            println!("[pool] initial template: height={}", height),
        Some(_) => println!("[pool] initial template: fetched"),
        None    => eprintln!("[pool] warning: upstream node {} not reachable (pool will retry)", node_addr),
    }

    // Background template refresh
    run_template_refresh(Arc::clone(&shared));

    // Stats HTTP server
    let stats_shared = Arc::clone(&shared);
    thread::spawn(move || run_stats_server(stats_port, stats_shared));

    // Main: pool server (blocks)
    run_pool_server(miner_port, shared);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_shared() -> Arc<PoolShared> {
        Arc::new(PoolShared::new("127.0.0.1:19999"))
    }

    #[test]
    fn test_worker_connect_disconnect() {
        let s = make_shared();
        s.worker_connect("10.0.0.1:4001");
        {
            let w = s.workers.read().unwrap();
            assert!(w.contains_key("10.0.0.1:4001"));
            assert!(w["10.0.0.1:4001"].connected);
        }
        s.worker_disconnect("10.0.0.1:4001");
        {
            let w = s.workers.read().unwrap();
            assert!(!w["10.0.0.1:4001"].connected);
        }
    }

    #[test]
    fn test_record_block_increments() {
        let s = make_shared();
        s.record_block("10.0.0.2:4002");
        s.record_block("10.0.0.2:4002");
        {
            let w = s.workers.read().unwrap();
            assert_eq!(w["10.0.0.2:4002"].blocks_found, 2);
        }
        assert_eq!(s.stats.read().unwrap().total_blocks, 2);
    }

    #[test]
    fn test_pool_stats_initial() {
        let s = make_shared();
        let stats = s.stats.read().unwrap();
        assert_eq!(stats.total_blocks, 0);
        assert_eq!(stats.total_connects, 0);
        assert_eq!(stats.node_addr, "127.0.0.1:19999");
    }

    #[test]
    fn test_worker_connect_increments_connects() {
        let s = make_shared();
        s.worker_connect("1.2.3.4:1234");
        s.worker_connect("5.6.7.8:5678");
        assert_eq!(s.stats.read().unwrap().total_connects, 2);
    }

    #[test]
    fn test_template_cache_empty_initially() {
        let s = make_shared();
        let cache = s.template.read().unwrap();
        assert!(cache.is_none());
    }

    #[test]
    fn test_unix_now_nonzero() {
        assert!(unix_now() > 0);
    }

    #[test]
    fn test_multiple_workers() {
        let s = make_shared();
        for i in 0..5 {
            s.worker_connect(&format!("10.0.0.{}:100{}", i, i));
        }
        assert_eq!(s.workers.read().unwrap().len(), 5);
        assert_eq!(s.stats.read().unwrap().total_connects, 5);
    }

    #[test]
    fn test_worker_last_seen_recent() {
        let s = make_shared();
        let before = unix_now();
        s.worker_connect("9.9.9.9:9999");
        let after = unix_now();
        let w = s.workers.read().unwrap();
        let last = w["9.9.9.9:9999"].last_seen;
        assert!(last >= before && last <= after + 1);
    }

    #[test]
    fn test_record_block_creates_worker_entry() {
        let s = make_shared();
        // Worker chưa từng kết nối, record_block vẫn tạo entry
        s.record_block("new.worker:1234");
        let w = s.workers.read().unwrap();
        assert!(w.contains_key("new.worker:1234"));
        assert_eq!(w["new.worker:1234"].blocks_found, 1);
    }
}
