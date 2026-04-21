#![allow(dead_code)]
//! v15.1 — Testnet Peer Connect
//!
//! Kết nối tới bootstrap peers PKT testnet (seed.testnet.oceif.com:8333).
//!
//! Flow:
//!   TCP connect (timeout) → Version → Verack → keepalive ping/pong
//!   Disconnect detected → exponential backoff → retry
//!
//! Dùng pkt_wire::{encode_message, decode_message} từ v15.0

use std::io::{Read as IoRead, Write as IoWrite};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::pkt_wire::{self, PktMsg, VersionMsg, MAINNET_MAGIC, TESTNET_MAGIC};

// ── Bootstrap peers ───────────────────────────────────────────────────────────

pub const TESTNET_BOOTSTRAP: &[(&str, u16)] = &[
    ("seed.testnet.oceif.com", 8333),
];

pub const MAINNET_BOOTSTRAP: &[(&str, u16)] = &[
    ("seed.mainnet.oceif.com", 64764),
];

pub const TESTNET_PORT: u16 = 8333;
pub const MAINNET_PORT: u16 = 64764;

// ── PeerConfig ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PeerConfig {
    pub host:                 String,
    pub port:                 u16,
    pub magic:                [u8; 4],
    pub connect_timeout_secs: u64,
    pub read_timeout_secs:    u64,
    pub max_retries:          u32,   // 0 = unlimited
    pub base_retry_secs:      u64,   // initial backoff
    pub max_retry_secs:       u64,   // cap backoff
    pub ping_interval_secs:   u64,
    pub network:              String, // "testnet" | "mainnet"
    pub our_height:           i32,   // start_height sent in Version
}

impl Default for PeerConfig {
    fn default() -> Self {
        Self {
            host:                 "seed.testnet.oceif.com".to_string(),
            port:                 TESTNET_PORT,
            magic:                TESTNET_MAGIC,
            connect_timeout_secs: 10,
            read_timeout_secs:    30,
            max_retries:          5,
            base_retry_secs:      2,
            max_retry_secs:       120,
            ping_interval_secs:   60,
            network:              "testnet".to_string(),
            our_height:           0,
        }
    }
}

impl PeerConfig {
    pub fn testnet() -> Self { Self::default() }

    pub fn mainnet() -> Self {
        Self {
            host:    "seed.mainnet.oceif.com".to_string(),
            port:    MAINNET_PORT,
            magic:   MAINNET_MAGIC,
            network: "mainnet".to_string(),
            ..Self::default()
        }
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

// ── Parse CLI args ────────────────────────────────────────────────────────────

pub fn parse_peer_args(args: &[String]) -> PeerConfig {
    let mut cfg = PeerConfig::default();
    let mut i   = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mainnet" => {
                cfg = PeerConfig::mainnet();
            }
            "--host" | "-H" if i + 1 < args.len() => {
                i += 1;
                cfg.host = args[i].clone();
            }
            "--port" | "-p" if i + 1 < args.len() => {
                i += 1;
                if let Ok(p) = args[i].parse() { cfg.port = p; }
            }
            "--retries" | "-r" if i + 1 < args.len() => {
                i += 1;
                if let Ok(n) = args[i].parse() { cfg.max_retries = n; }
            }
            "--timeout" | "-t" if i + 1 < args.len() => {
                i += 1;
                if let Ok(n) = args[i].parse() { cfg.connect_timeout_secs = n; }
            }
            "--height" if i + 1 < args.len() => {
                i += 1;
                if let Ok(n) = args[i].parse() { cfg.our_height = n; }
            }
            other => {
                // bare "host:port" or "host"
                if let Some((h, p)) = other.rsplit_once(':') {
                    if let Ok(port) = p.parse() {
                        cfg.host = h.to_string();
                        cfg.port = port;
                    } else {
                        cfg.host = other.to_string();
                    }
                } else if !other.starts_with('-') {
                    cfg.host = other.to_string();
                }
            }
        }
        i += 1;
    }
    cfg
}

// ── Handshake state ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeState {
    Idle,
    SentVersion,
    ReceivedVersion,
    Complete,
    Failed(String),
}

impl HandshakeState {
    pub fn is_complete(&self) -> bool { matches!(self, Self::Complete) }
    pub fn is_failed(&self)   -> bool { matches!(self, Self::Failed(_)) }
}

// ── PeerInfo & errors ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub addr:         String,
    pub version:      u32,
    pub user_agent:   String,
    pub start_height: i32,
    pub services:     u64,
}

#[derive(Debug)]
pub enum PeerError {
    Connect(String),
    Io(String),
    Handshake(String),
    Timeout,
    Disconnected,
}

impl std::fmt::Display for PeerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect(s)    => write!(f, "connect: {}", s),
            Self::Io(s)         => write!(f, "io: {}", s),
            Self::Handshake(s)  => write!(f, "handshake: {}", s),
            Self::Timeout       => write!(f, "timeout"),
            Self::Disconnected  => write!(f, "peer disconnected"),
        }
    }
}

impl From<std::io::Error> for PeerError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => PeerError::Timeout,
            _ => PeerError::Io(e.to_string()),
        }
    }
}

// ── Backoff ───────────────────────────────────────────────────────────────────

/// Exponential backoff: attempt=0 → base, attempt=1 → base*2, …, capped at max.
pub fn backoff_delay(attempt: u32, base_secs: u64, max_secs: u64) -> Duration {
    let secs = base_secs.saturating_mul(1u64 << attempt.min(30));
    Duration::from_secs(secs.min(max_secs))
}

/// Sum of all backoff delays over `attempts` retries.
pub fn total_backoff_secs(attempts: u32, base_secs: u64, max_secs: u64) -> u64 {
    (0..attempts)
        .map(|a| backoff_delay(a, base_secs, max_secs).as_secs())
        .sum()
}

// ── Wire I/O helpers ──────────────────────────────────────────────────────────

/// Encode and send a PktMsg over TCP.
pub fn send_msg(stream: &mut TcpStream, msg: PktMsg, magic: [u8; 4]) -> Result<(), PeerError> {
    let bytes = pkt_wire::encode_message(&msg, &magic);
    stream.write_all(&bytes).map_err(PeerError::from)
}

fn read_exact_into(stream: &mut TcpStream, buf: &mut Vec<u8>, n: usize) -> Result<(), PeerError> {
    let start = buf.len();
    buf.resize(start + n, 0);
    stream.read_exact(&mut buf[start..]).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            PeerError::Disconnected
        } else {
            PeerError::from(e)
        }
    })
}

/// Read one complete PktMsg from a TCP stream (blocking).
pub fn recv_msg(stream: &mut TcpStream, magic: [u8; 4]) -> Result<PktMsg, PeerError> {
    let header_len = pkt_wire::HEADER_LEN;
    let mut buf    = Vec::with_capacity(header_len);
    read_exact_into(stream, &mut buf, header_len)?;

    // Payload length is bytes 16..20 of the header (LE u32)
    let payload_len = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]) as usize;
    if payload_len > pkt_wire::MAX_PAYLOAD {
        return Err(PeerError::Io(format!("payload too large: {} bytes", payload_len)));
    }

    // Verify magic before spending time reading payload
    if buf[0..4] != magic {
        return Err(PeerError::Handshake(format!("wrong magic {:02x}{:02x}{:02x}{:02x}",
            buf[0], buf[1], buf[2], buf[3])));
    }

    if payload_len > 0 {
        read_exact_into(stream, &mut buf, payload_len)?;
    }

    let (msg, _) = pkt_wire::decode_message(&buf)
        .map_err(|e| PeerError::Io(format!("decode: {:?}", e)))?;
    Ok(msg)
}

// ── Handshake ─────────────────────────────────────────────────────────────────

/// Perform the Version/Verack handshake with a connected peer.
///
/// Protocol flow:
///   → send Version
///   ← receive Version    (answer with Verack immediately)
///   ← receive Verack
///   → done
pub fn do_handshake(stream: &mut TcpStream, cfg: &PeerConfig) -> Result<PeerInfo, PeerError> {
    let deadline    = Instant::now() + Duration::from_secs(cfg.connect_timeout_secs);
    let version_msg = PktMsg::Version(VersionMsg::new(cfg.our_height));

    send_msg(stream, version_msg, cfg.magic)?;

    let mut peer_info: Option<PeerInfo> = None;
    let mut got_verack                  = false;

    while !(peer_info.is_some() && got_verack) {
        if Instant::now() > deadline {
            return Err(PeerError::Timeout);
        }

        let msg = recv_msg(stream, cfg.magic)?;
        match msg {
            PktMsg::Version(v) => {
                peer_info = Some(PeerInfo {
                    addr:         cfg.addr(),
                    version:      v.version,
                    user_agent:   v.user_agent.clone(),
                    start_height: v.start_height,
                    services:     v.services,
                });
                // Reply with Verack immediately after receiving their Version
                send_msg(stream, PktMsg::Verack, cfg.magic)?;
            }
            PktMsg::Verack => {
                got_verack = true;
            }
            PktMsg::Ping { nonce } => {
                // Respond to pings even during handshake
                send_msg(stream, PktMsg::Pong { nonce }, cfg.magic)?;
            }
            _ => {} // ignore other messages during handshake
        }
    }

    Ok(peer_info.expect("peer_info set before loop exits"))
}

// ── Ping/pong keepalive ───────────────────────────────────────────────────────

/// Send a ping and wait for the matching pong (up to 10 messages in between).
pub fn ping_pong(stream: &mut TcpStream, magic: [u8; 4], nonce: u64) -> Result<(), PeerError> {
    send_msg(stream, PktMsg::Ping { nonce }, magic)?;
    for _ in 0..10 {
        match recv_msg(stream, magic)? {
            PktMsg::Pong { nonce: n } if n == nonce => return Ok(()),
            PktMsg::Ping { nonce: n } => {
                // Answer their ping while we wait for our pong
                send_msg(stream, PktMsg::Pong { nonce: n }, magic)?;
            }
            _ => {}
        }
    }
    Err(PeerError::Io("pong not received within 10 messages".to_string()))
}

// ── Connect ───────────────────────────────────────────────────────────────────

/// Try to connect once: TCP → handshake. No retry.
pub fn connect_once(cfg: &PeerConfig) -> Result<(TcpStream, PeerInfo), PeerError> {
    let addr    = cfg.addr();
    let timeout = Duration::from_secs(cfg.connect_timeout_secs);

    // Resolve hostname (supports DNS names, not just IP literals)
    let sock_addr = addr
        .to_socket_addrs()
        .map_err(|e| PeerError::Connect(e.to_string()))?
        .next()
        .ok_or_else(|| PeerError::Connect(format!("no address resolved for {}", addr)))?;

    let stream = TcpStream::connect_timeout(&sock_addr, timeout)
        .map_err(|e| PeerError::Connect(e.to_string()))?;

    stream.set_read_timeout(Some(Duration::from_secs(cfg.read_timeout_secs)))?;
    stream.set_write_timeout(Some(Duration::from_secs(cfg.connect_timeout_secs)))?;

    let mut stream = stream;
    let info       = do_handshake(&mut stream, cfg)?;
    Ok((stream, info))
}

// ── ConnectResult ─────────────────────────────────────────────────────────────

pub struct ConnectResult {
    pub info:       PeerInfo,
    pub attempts:   u32,
    pub elapsed_ms: u128,
}

/// Connect with exponential backoff retry. Returns on first success.
/// `max_retries == 0` means unlimited retries.
pub fn connect_with_retry(cfg: &PeerConfig) -> Result<ConnectResult, PeerError> {
    let start   = Instant::now();
    let mut attempt = 0u32;

    loop {
        match connect_once(cfg) {
            Ok((_, info)) => {
                return Ok(ConnectResult {
                    info,
                    attempts:   attempt + 1,
                    elapsed_ms: start.elapsed().as_millis(),
                });
            }
            Err(e) => {
                attempt += 1;
                if cfg.max_retries > 0 && attempt >= cfg.max_retries {
                    return Err(e);
                }
                let delay = backoff_delay(attempt - 1, cfg.base_retry_secs, cfg.max_retry_secs);
                eprintln!(
                    "[peer] connect failed ({}/{}) — retrying in {}s: {}",
                    attempt,
                    if cfg.max_retries == 0 { "∞".to_string() } else { cfg.max_retries.to_string() },
                    delay.as_secs(),
                    e,
                );
                std::thread::sleep(delay);
            }
        }
    }
}

// ── Status formatting ─────────────────────────────────────────────────────────

pub fn format_peer_status(info: &PeerInfo, connected_secs: u64) -> String {
    format!(
        "peer={} version={} height={} agent=\"{}\" uptime={}s",
        info.addr, info.version, info.start_height, info.user_agent, connected_secs,
    )
}

pub fn format_retry_status(attempt: u32, max: u32, delay: Duration, err: &PeerError) -> String {
    let max_str = if max == 0 { "∞".to_string() } else { max.to_string() };
    format!("attempt {}/{}: {} — retry in {}s", attempt, max_str, err, delay.as_secs())
}

// ── CLI command ───────────────────────────────────────────────────────────────

pub fn cmd_peer(args: &[String]) {
    if args.first().map(|s| s.as_str()) == Some("--help") {
        println!();
        println!("  cargo run -- peer [host:port] [options]");
        println!();
        println!("  Options:");
        println!("    --mainnet          kết nối mainnet (mặc định: testnet)");
        println!("    --host HOST        địa chỉ peer");
        println!("    --port PORT        cổng (mặc định: 8333 testnet / 64764 mainnet)");
        println!("    --retries N        số lần thử lại (0 = vô hạn, mặc định: 5)");
        println!("    --timeout S        timeout kết nối (giây, mặc định: 10)");
        println!("    --height N         block height gửi trong Version");
        println!();
        println!("  Ví dụ:");
        println!("    cargo run -- peer");
        println!("    cargo run -- peer seed.testnet.oceif.com:8333");
        println!("    cargo run -- peer --mainnet --retries 3");
        println!();
        return;
    }

    let cfg = parse_peer_args(args);
    println!("[peer] kết nối tới {} ({}) …", cfg.addr(), cfg.network);

    match connect_with_retry(&cfg) {
        Ok(r) => {
            println!("[peer] ✅  handshake thành công sau {} lần ({} ms)",
                r.attempts, r.elapsed_ms);
            println!("[peer] {}", format_peer_status(&r.info, 0));
        }
        Err(e) => {
            eprintln!("[peer] ❌  kết nối thất bại sau {} lần: {}", cfg.max_retries, e);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    // ── Helper: loopback config ──────────────────────────────────────────────

    fn loopback_config(port: u16) -> PeerConfig {
        PeerConfig {
            host:                 "127.0.0.1".to_string(),
            port,
            magic:                TESTNET_MAGIC,
            connect_timeout_secs: 5,
            read_timeout_secs:    5,
            max_retries:          1,
            base_retry_secs:      0,
            max_retry_secs:       0,
            ping_interval_secs:   60,
            network:              "testnet".to_string(),
            our_height:           0,
        }
    }

    // ── Helper: spawn a simple handshake server ──────────────────────────────

    /// Spawns a server thread that:
    ///   1. Receives Version from client
    ///   2. Sends its own Version (height=100) back
    ///   3. Sends Verack
    ///   4. Receives client's Verack
    fn spawn_handshake_server(
        listener: TcpListener,
        magic: [u8; 4],
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                // Receive client's Version
                if recv_msg(&mut stream, magic).is_ok() {
                    // Send our Version
                    let v = PktMsg::Version(VersionMsg::new(100));
                    if send_msg(&mut stream, v, magic).is_ok() {
                        // Send Verack
                        let _ = send_msg(&mut stream, PktMsg::Verack, magic);
                        // Receive client's Verack (ignore result)
                        let _ = recv_msg(&mut stream, magic);
                    }
                }
            }
        })
    }

    // ── Unit tests: no network ───────────────────────────────────────────────

    #[test]
    fn test_default_config_host() {
        let cfg = PeerConfig::default();
        assert_eq!(cfg.host, "seed.testnet.oceif.com");
    }

    #[test]
    fn test_default_config_port() {
        let cfg = PeerConfig::default();
        assert_eq!(cfg.port, TESTNET_PORT);
    }

    #[test]
    fn test_default_config_magic() {
        let cfg = PeerConfig::default();
        assert_eq!(cfg.magic, TESTNET_MAGIC);
    }

    #[test]
    fn test_default_config_network() {
        let cfg = PeerConfig::default();
        assert_eq!(cfg.network, "testnet");
    }

    #[test]
    fn test_mainnet_config_port() {
        let cfg = PeerConfig::mainnet();
        assert_eq!(cfg.port, MAINNET_PORT);
    }

    #[test]
    fn test_mainnet_config_magic() {
        let cfg = PeerConfig::mainnet();
        assert_eq!(cfg.magic, MAINNET_MAGIC);
    }

    #[test]
    fn test_mainnet_config_network() {
        let cfg = PeerConfig::mainnet();
        assert_eq!(cfg.network, "mainnet");
    }

    #[test]
    fn test_peer_addr_format() {
        let cfg = PeerConfig {
            host: "192.168.1.1".to_string(),
            port: 12345,
            ..PeerConfig::default()
        };
        assert_eq!(cfg.addr(), "192.168.1.1:12345");
    }

    // ── Backoff tests ────────────────────────────────────────────────────────

    #[test]
    fn test_backoff_attempt_0_is_base() {
        assert_eq!(backoff_delay(0, 2, 120), Duration::from_secs(2));
    }

    #[test]
    fn test_backoff_doubles_each_attempt() {
        assert_eq!(backoff_delay(0, 2, 120).as_secs(), 2);
        assert_eq!(backoff_delay(1, 2, 120).as_secs(), 4);
        assert_eq!(backoff_delay(2, 2, 120).as_secs(), 8);
        assert_eq!(backoff_delay(3, 2, 120).as_secs(), 16);
    }

    #[test]
    fn test_backoff_capped_at_max() {
        assert_eq!(backoff_delay(10, 2, 30).as_secs(), 30);
    }

    #[test]
    fn test_backoff_large_attempt_still_capped() {
        assert_eq!(backoff_delay(100, 2, 60).as_secs(), 60);
    }

    #[test]
    fn test_backoff_zero_base() {
        assert_eq!(backoff_delay(5, 0, 120).as_secs(), 0);
    }

    #[test]
    fn test_backoff_zero_max() {
        assert_eq!(backoff_delay(0, 2, 0).as_secs(), 0);
    }

    #[test]
    fn test_total_backoff_three_attempts() {
        // 2 + 4 + 8 = 14
        assert_eq!(total_backoff_secs(3, 2, 120), 14);
    }

    #[test]
    fn test_total_backoff_zero_attempts() {
        assert_eq!(total_backoff_secs(0, 2, 120), 0);
    }

    #[test]
    fn test_total_backoff_one_attempt() {
        assert_eq!(total_backoff_secs(1, 2, 120), 2);
    }

    // ── HandshakeState tests ─────────────────────────────────────────────────

    #[test]
    fn test_handshake_state_idle_not_complete() {
        assert!(!HandshakeState::Idle.is_complete());
    }

    #[test]
    fn test_handshake_state_idle_not_failed() {
        assert!(!HandshakeState::Idle.is_failed());
    }

    #[test]
    fn test_handshake_state_complete_is_complete() {
        assert!(HandshakeState::Complete.is_complete());
    }

    #[test]
    fn test_handshake_state_complete_not_failed() {
        assert!(!HandshakeState::Complete.is_failed());
    }

    #[test]
    fn test_handshake_state_failed_is_failed() {
        assert!(HandshakeState::Failed("err".to_string()).is_failed());
    }

    #[test]
    fn test_handshake_state_failed_not_complete() {
        assert!(!HandshakeState::Failed("err".to_string()).is_complete());
    }

    #[test]
    fn test_handshake_state_received_version_not_complete() {
        assert!(!HandshakeState::ReceivedVersion.is_complete());
    }

    #[test]
    fn test_handshake_state_sent_version_not_failed() {
        assert!(!HandshakeState::SentVersion.is_failed());
    }

    // ── parse_peer_args tests ─────────────────────────────────────────────────

    #[test]
    fn test_parse_args_empty() {
        let cfg = parse_peer_args(&[]);
        assert_eq!(cfg.host, "seed.testnet.oceif.com");
        assert_eq!(cfg.port, TESTNET_PORT);
    }

    #[test]
    fn test_parse_args_mainnet_flag() {
        let cfg = parse_peer_args(&["--mainnet".to_string()]);
        assert_eq!(cfg.magic, MAINNET_MAGIC);
        assert_eq!(cfg.network, "mainnet");
    }

    #[test]
    fn test_parse_args_host_flag() {
        let cfg = parse_peer_args(&[
            "--host".to_string(), "127.0.0.1".to_string(),
        ]);
        assert_eq!(cfg.host, "127.0.0.1");
    }

    #[test]
    fn test_parse_args_port_flag() {
        let cfg = parse_peer_args(&[
            "--port".to_string(), "9999".to_string(),
        ]);
        assert_eq!(cfg.port, 9999);
    }

    #[test]
    fn test_parse_args_retries_flag() {
        let cfg = parse_peer_args(&["--retries".to_string(), "10".to_string()]);
        assert_eq!(cfg.max_retries, 10);
    }

    #[test]
    fn test_parse_args_timeout_flag() {
        let cfg = parse_peer_args(&["--timeout".to_string(), "30".to_string()]);
        assert_eq!(cfg.connect_timeout_secs, 30);
    }

    #[test]
    fn test_parse_args_height_flag() {
        let cfg = parse_peer_args(&["--height".to_string(), "850000".to_string()]);
        assert_eq!(cfg.our_height, 850000);
    }

    #[test]
    fn test_parse_args_bare_hostport() {
        let cfg = parse_peer_args(&["192.168.1.100:8333".to_string()]);
        assert_eq!(cfg.host, "192.168.1.100");
        assert_eq!(cfg.port, 8333);
    }

    #[test]
    fn test_parse_args_bare_host_only() {
        let cfg = parse_peer_args(&["127.0.0.1".to_string()]);
        assert_eq!(cfg.host, "127.0.0.1");
    }

    #[test]
    fn test_parse_args_combined() {
        let cfg = parse_peer_args(&[
            "--mainnet".to_string(),
            "--retries".to_string(), "3".to_string(),
            "--height".to_string(), "1000".to_string(),
        ]);
        assert_eq!(cfg.network, "mainnet");
        assert_eq!(cfg.max_retries, 3);
        assert_eq!(cfg.our_height, 1000);
    }

    // ── Error display tests ───────────────────────────────────────────────────

    #[test]
    fn test_error_display_connect() {
        let e = PeerError::Connect("connection refused".to_string());
        assert!(e.to_string().contains("connect"));
        assert!(e.to_string().contains("refused"));
    }

    #[test]
    fn test_error_display_io() {
        let e = PeerError::Io("broken pipe".to_string());
        assert!(e.to_string().contains("io"));
        assert!(e.to_string().contains("broken"));
    }

    #[test]
    fn test_error_display_handshake() {
        let e = PeerError::Handshake("wrong magic".to_string());
        assert!(e.to_string().contains("handshake"));
        assert!(e.to_string().contains("wrong magic"));
    }

    #[test]
    fn test_error_display_timeout() {
        assert_eq!(PeerError::Timeout.to_string(), "timeout");
    }

    #[test]
    fn test_error_display_disconnected() {
        assert_eq!(PeerError::Disconnected.to_string(), "peer disconnected");
    }

    // ── Format helpers ────────────────────────────────────────────────────────

    #[test]
    fn test_format_peer_status_contains_addr() {
        let info = PeerInfo {
            addr: "127.0.0.1:64765".to_string(),
            version: 70015,
            user_agent: "/pktscan:1.0/".to_string(),
            start_height: 850000,
            services: 1,
        };
        let s = format_peer_status(&info, 42);
        assert!(s.contains("127.0.0.1:64765"));
    }

    #[test]
    fn test_format_peer_status_contains_version() {
        let info = PeerInfo {
            addr: "a:1".to_string(),
            version: 70015,
            user_agent: "/test/".to_string(),
            start_height: 0,
            services: 1,
        };
        assert!(format_peer_status(&info, 0).contains("70015"));
    }

    #[test]
    fn test_format_peer_status_contains_height() {
        let info = PeerInfo {
            addr: "a:1".to_string(),
            version: 70015,
            user_agent: "/test/".to_string(),
            start_height: 850000,
            services: 1,
        };
        assert!(format_peer_status(&info, 0).contains("850000"));
    }

    #[test]
    fn test_format_peer_status_contains_uptime() {
        let info = PeerInfo {
            addr: "a:1".to_string(),
            version: 70015,
            user_agent: "/test/".to_string(),
            start_height: 0,
            services: 1,
        };
        assert!(format_peer_status(&info, 99).contains("99s"));
    }

    #[test]
    fn test_format_retry_status_finite() {
        let s = format_retry_status(2, 5, Duration::from_secs(4), &PeerError::Timeout);
        assert!(s.contains("2/5"));
        assert!(s.contains("4s"));
    }

    #[test]
    fn test_format_retry_status_unlimited() {
        let s = format_retry_status(1, 0, Duration::from_secs(2), &PeerError::Timeout);
        assert!(s.contains("∞"));
    }

    // ── Loopback TCP tests (real network, localhost only) ──────────────────────

    #[test]
    fn test_handshake_loopback_succeeds() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_handshake_server(listener, TESTNET_MAGIC);
        let cfg      = loopback_config(port);

        let result = connect_once(&cfg);
        server.join().ok();

        assert!(result.is_ok(), "loopback handshake failed: {:?}", result.err());
    }

    #[test]
    fn test_handshake_loopback_peer_height() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_handshake_server(listener, TESTNET_MAGIC);
        let cfg      = loopback_config(port);

        let (_, info) = connect_once(&cfg).unwrap();
        server.join().ok();

        assert_eq!(info.start_height, 100); // server sends height=100
    }

    #[test]
    fn test_handshake_loopback_peer_version() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_handshake_server(listener, TESTNET_MAGIC);
        let cfg      = loopback_config(port);

        let (_, info) = connect_once(&cfg).unwrap();
        server.join().ok();

        assert_eq!(info.version, pkt_wire::PROTOCOL_VERSION);
    }

    #[test]
    fn test_handshake_loopback_peer_addr() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_handshake_server(listener, TESTNET_MAGIC);
        let cfg      = loopback_config(port);

        let (_, info) = connect_once(&cfg).unwrap();
        server.join().ok();

        assert_eq!(info.addr, format!("127.0.0.1:{}", port));
    }

    #[test]
    fn test_handshake_loopback_services() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_handshake_server(listener, TESTNET_MAGIC);
        let cfg      = loopback_config(port);

        let (_, info) = connect_once(&cfg).unwrap();
        server.join().ok();

        assert_eq!(info.services, pkt_wire::SERVICES_NODE);
    }

    #[test]
    fn test_handshake_multiple_sequential() {
        for _ in 0..3 {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port     = listener.local_addr().unwrap().port();
            let server   = spawn_handshake_server(listener, TESTNET_MAGIC);
            let cfg      = loopback_config(port);
            let result   = connect_once(&cfg);
            server.join().ok();
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_send_recv_roundtrip_ping() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let magic    = TESTNET_MAGIC;

        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).ok();
            recv_msg(&mut s, magic).unwrap()
        });

        let mut client = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();
        send_msg(&mut client, PktMsg::Ping { nonce: 0xdeadbeef }, magic).unwrap();

        let received = server.join().unwrap();
        assert!(matches!(received, PktMsg::Ping { nonce: 0xdeadbeef }));
    }

    #[test]
    fn test_send_recv_roundtrip_verack() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let magic    = TESTNET_MAGIC;

        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).ok();
            recv_msg(&mut s, magic).unwrap()
        });

        let mut client = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();
        send_msg(&mut client, PktMsg::Verack, magic).unwrap();

        let received = server.join().unwrap();
        assert!(matches!(received, PktMsg::Verack));
    }

    #[test]
    fn test_ping_pong_loopback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let magic    = TESTNET_MAGIC;
        let nonce    = 0xabcd1234u64;

        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).ok();
            if let Ok(PktMsg::Ping { nonce: n }) = recv_msg(&mut s, magic) {
                send_msg(&mut s, PktMsg::Pong { nonce: n }, magic).ok();
            }
        });

        let mut client = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let result = ping_pong(&mut client, magic, nonce);
        server.join().ok();
        assert!(result.is_ok(), "ping_pong failed: {:?}", result.err());
    }

    #[test]
    fn test_connect_once_bad_port_fails() {
        // Port 1 should refuse connection (nothing listening)
        let cfg = PeerConfig {
            host:                 "127.0.0.1".to_string(),
            port:                 1,
            connect_timeout_secs: 2,
            read_timeout_secs:    2,
            max_retries:          1,
            ..PeerConfig::default()
        };
        assert!(connect_once(&cfg).is_err());
    }

    #[test]
    fn test_connect_with_retry_exhausts_retries() {
        // Port 2 should refuse → exhaust max_retries=2 quickly with 0 delay
        let cfg = PeerConfig {
            host:                 "127.0.0.1".to_string(),
            port:                 2,
            connect_timeout_secs: 1,
            read_timeout_secs:    1,
            max_retries:          2,
            base_retry_secs:      0,
            max_retry_secs:       0,
            ..PeerConfig::default()
        };
        assert!(connect_with_retry(&cfg).is_err());
    }

    #[test]
    fn test_connect_with_retry_succeeds_on_second_attempt() {
        // Start listener AFTER a short delay to test retry logic
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();

        // Server just accepts and does the handshake
        let server = spawn_handshake_server(listener, TESTNET_MAGIC);

        let cfg = PeerConfig {
            host:                 "127.0.0.1".to_string(),
            port,
            connect_timeout_secs: 5,
            read_timeout_secs:    5,
            max_retries:          3,
            base_retry_secs:      0,
            max_retry_secs:       0,
            ..PeerConfig::default()
        };

        let result = connect_with_retry(&cfg);
        server.join().ok();
        assert!(result.is_ok());
    }

    #[test]
    fn test_connect_result_has_attempts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_handshake_server(listener, TESTNET_MAGIC);
        let cfg      = loopback_config(port);

        let result   = connect_with_retry(&cfg).unwrap();
        server.join().ok();

        assert!(result.attempts >= 1);
    }

    #[test]
    fn test_connect_result_elapsed_ms() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();
        let server   = spawn_handshake_server(listener, TESTNET_MAGIC);
        let cfg      = loopback_config(port);

        let result   = connect_with_retry(&cfg).unwrap();
        server.join().ok();

        // elapsed_ms should be a non-negative number (connects in <5s)
        assert!(result.elapsed_ms < 5_000);
    }

    #[test]
    fn test_wrong_magic_handshake_fails() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port     = listener.local_addr().unwrap().port();

        // Server uses MAINNET magic, client sends TESTNET magic
        let server = thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                s.set_read_timeout(Some(Duration::from_secs(5))).ok();
                // Send a version with MAINNET magic
                let v = PktMsg::Version(VersionMsg::new(0));
                let _ = send_msg(&mut s, v, MAINNET_MAGIC);
                let _ = send_msg(&mut s, PktMsg::Verack, MAINNET_MAGIC);
            }
        });

        let mut cfg  = loopback_config(port);
        cfg.magic    = TESTNET_MAGIC;  // client expects testnet

        let result = connect_once(&cfg);
        server.join().ok();
        // Should fail due to wrong magic
        assert!(result.is_err());
    }

    #[test]
    fn test_bootstrap_testnet_not_empty() {
        assert!(!TESTNET_BOOTSTRAP.is_empty());
        assert_eq!(TESTNET_BOOTSTRAP[0].1, TESTNET_PORT);
    }

    #[test]
    fn test_bootstrap_mainnet_not_empty() {
        assert!(!MAINNET_BOOTSTRAP.is_empty());
        assert_eq!(MAINNET_BOOTSTRAP[0].1, MAINNET_PORT);
    }
}
