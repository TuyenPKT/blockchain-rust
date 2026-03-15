#![allow(dead_code)]

/// v5.7 — Monitoring: structured logging (tracing) + health check endpoint
///
/// Hai thành phần chính:
///
/// 1. Structured logging với `tracing` crate:
///    - `init_tracing(level)` — khởi tạo global subscriber (pretty hoặc JSON)
///    - `LogLevel` enum: Error, Warn, Info, Debug
///    - Typed log helpers: log_block_mined, log_tx_received, log_peer_event,
///      log_sync_event, log_rbf_replace, log_error
///
/// 2. Health check endpoint (axum, GET /health):
///    - `HealthStatus` struct: uptime_secs, version, height, difficulty,
///      utxo_count, mempool_depth, is_synced, fee_fast/medium/slow
///    - `health_check(bc) -> HealthStatus`
///    - `serve_health(bc_arc, port)` — standalone axum server
///
/// CLI:  cargo run -- monitor [port]  → health endpoint (mặc định 3001)
/// REST: GET http://localhost:3001/health  → HealthStatus JSON
///       GET http://localhost:3001/ready   → {"ok": true} khi node synced

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::chain::Blockchain;

// ─── LogLevel ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    /// Parse từ string ("error" | "warn" | "info" | "debug")
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "error" => Self::Error,
            "warn"  => Self::Warn,
            "debug" => Self::Debug,
            _       => Self::Info,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn  => "warn",
            Self::Info  => "info",
            Self::Debug => "debug",
        }
    }
}

// ─── Tracing init ─────────────────────────────────────────────────────────────

/// Khởi tạo global tracing subscriber.
///
/// - Đọc `RUST_LOG` env var nếu có, ngược lại dùng `level` param.
/// - Format: pretty (human-readable, màu nếu terminal hỗ trợ).
/// - Gọi một lần khi khởi động node / API.
///
/// Returns `true` nếu init thành công, `false` nếu đã được init trước đó.
pub fn init_tracing(level: LogLevel) -> bool {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| format!("blockchain_rust={}", level.as_str()));

    fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_target(false)
        .with_thread_ids(false)
        .compact()
        .try_init()
        .is_ok()
}

// ─── Typed log helpers ────────────────────────────────────────────────────────

/// Log sự kiện block được mine thành công.
pub fn log_block_mined(height: u64, hash: &str, difficulty: usize, txs: usize, miner: &str) {
    tracing::info!(
        event = "block_mined",
        height,
        hash = &hash[..8.min(hash.len())],
        difficulty,
        txs,
        miner = &miner[..8.min(miner.len())],
        "Block #{height} mined (diff={difficulty}, txs={txs})"
    );
}

/// Log transaction nhận vào mempool.
pub fn log_tx_received(tx_id: &str, fee: u64, fee_rate: f64) {
    tracing::info!(
        event = "tx_received",
        tx_id = &tx_id[..8.min(tx_id.len())],
        fee,
        fee_rate,
        "TX received fee={fee} ({fee_rate:.1} sat/B)"
    );
}

/// Log peer connection event.
pub fn log_peer_event(addr: &str, action: &str) {
    tracing::info!(
        event = "peer_event",
        peer = addr,
        action,
        "Peer {action}: {addr}"
    );
}

/// Log chain sync event.
pub fn log_sync_event(local_height: u64, remote_height: u64, peer: &str) {
    if remote_height > local_height {
        tracing::warn!(
            event = "sync_behind",
            local_height,
            remote_height,
            behind = remote_height - local_height,
            peer,
            "Behind by {} blocks (peer={peer})", remote_height - local_height
        );
    } else {
        tracing::info!(
            event = "sync_ok",
            height = local_height,
            peer,
            "Synced at height {local_height}"
        );
    }
}

/// Log RBF replace event.
pub fn log_rbf_replace(old_tx_id: &str, new_tx_id: &str, old_fee: u64, new_fee: u64) {
    tracing::info!(
        event = "rbf_replace",
        old_tx = &old_tx_id[..8.min(old_tx_id.len())],
        new_tx = &new_tx_id[..8.min(new_tx_id.len())],
        old_fee,
        new_fee,
        bump_pct = ((new_fee as f64 / old_fee as f64 - 1.0) * 100.0) as u64,
        "RBF replace: fee {old_fee} → {new_fee}"
    );
}

/// Log lỗi có context.
pub fn log_error(context: &str, msg: &str) {
    tracing::error!(
        event = "error",
        context,
        "Error in {context}: {msg}"
    );
}

// ─── HealthStatus ─────────────────────────────────────────────────────────────

const VERSION: &str = "v5.7";
static START_TIME: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

fn start_time() -> u64 {
    *START_TIME.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Phiên bản node
    pub version: String,
    /// Số giây kể từ khi process khởi động
    pub uptime_secs: u64,
    /// Chain tip height
    pub height: u64,
    /// Difficulty hiện tại
    pub difficulty: usize,
    /// Số UTXOs
    pub utxo_count: usize,
    /// Số TX đang chờ trong mempool
    pub mempool_depth: usize,
    /// true nếu chain có ít nhất 1 block sau genesis
    pub is_synced: bool,
    /// Fee estimate fast (sat/byte)
    pub fee_fast: f64,
    /// Fee estimate medium (sat/byte)
    pub fee_medium: f64,
    /// Fee estimate slow (sat/byte)
    pub fee_slow: f64,
    /// Unix timestamp
    pub timestamp: u64,
}

/// Collect health status từ Blockchain.
pub fn health_check(bc: &Blockchain) -> HealthStatus {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let uptime_secs = now.saturating_sub(start_time());
    let height      = bc.chain.len().saturating_sub(1) as u64;
    let fee_est     = bc.fee_estimator.estimate();

    HealthStatus {
        version:      VERSION.to_string(),
        uptime_secs,
        height,
        difficulty:    bc.difficulty,
        utxo_count:    bc.utxo_set.utxos.len(),
        mempool_depth: bc.mempool.entries.len(),
        is_synced:     height > 0,
        fee_fast:      fee_est.fast_sat_per_byte,
        fee_medium:    fee_est.medium_sat_per_byte,
        fee_slow:      fee_est.slow_sat_per_byte,
        timestamp:     now,
    }
}

// ─── HTTP health server ───────────────────────────────────────────────────────

pub type Db = Arc<Mutex<Blockchain>>;

/// Chạy standalone health HTTP server tại `port`.
///
/// Endpoints:
///   GET /health  → HealthStatus JSON (200)
///   GET /ready   → {"ok": true} nếu height > 0, {"ok": false} + 503 ngược lại
///   GET /version → {"version": "v5.7"}
pub async fn serve_health(state: Db, port: u16) {
    use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
    use serde_json::json;

    async fn handle_health(State(db): State<Db>) -> Json<HealthStatus> {
        let bc = db.lock().await;
        Json(health_check(&bc))
    }

    async fn handle_ready(State(db): State<Db>) -> (StatusCode, Json<serde_json::Value>) {
        let bc = db.lock().await;
        let height = bc.chain.len().saturating_sub(1) as u64;
        if height > 0 {
            (StatusCode::OK, Json(json!({"ok": true, "height": height})))
        } else {
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"ok": false, "height": 0})))
        }
    }

    async fn handle_version() -> Json<serde_json::Value> {
        Json(json!({"version": VERSION}))
    }

    let app = Router::new()
        .route("/health",  get(handle_health))
        .route("/ready",   get(handle_ready))
        .route("/version", get(handle_version))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Health endpoint: http://{}/health", addr);
    println!("  Health endpoint: http://0.0.0.0:{}/health", port);

    let listener = tokio::net::TcpListener::bind(&addr).await
        .unwrap_or_else(|e| {
            eprintln!("Cannot bind to {addr}: {e}");
            std::process::exit(1);
        });
    axum::serve(listener, app).await
        .unwrap_or_else(|e| eprintln!("Health server error: {e}"));
}

/// CLI entry point: `cargo run -- monitor [port]`
pub fn cmd_monitor(port: u16) {
    init_tracing(LogLevel::Info);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let bc    = crate::storage::load_or_new();
        let state = Arc::new(Mutex::new(bc));

        println!();
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║              📡  Monitoring Server  v5.7                    ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();

        let snap = {
            let bc = state.lock().await;
            health_check(&bc)
        };
        println!("  Height     : {}", snap.height);
        println!("  Difficulty : {}", snap.difficulty);
        println!("  UTXOs      : {}", snap.utxo_count);
        println!("  Mempool    : {} pending", snap.mempool_depth);
        println!("  Fee fast   : {:.1} sat/B", snap.fee_fast);
        println!();

        serve_health(state, port).await;
    });
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Blockchain;

    #[test]
    fn test_health_check_genesis() {
        let bc = Blockchain::new();
        let h  = health_check(&bc);
        assert_eq!(h.height, 0);
        assert_eq!(h.version, VERSION);
        assert!(!h.is_synced, "genesis-only chain chưa synced");
        assert_eq!(h.mempool_depth, 0);
    }

    #[test]
    fn test_health_check_after_mining() {
        let mut bc = Blockchain::new();
        bc.difficulty = 1;
        bc.add_block(vec![], "aabbccddaabbccddaabbccddaabbccddaabbccdd");
        bc.add_block(vec![], "aabbccddaabbccddaabbccddaabbccddaabbccdd");

        let h = health_check(&bc);
        assert_eq!(h.height, 2);
        assert!(h.is_synced);
        assert!(h.utxo_count > 0);
    }

    #[test]
    fn test_health_status_serializable() {
        let bc   = Blockchain::new();
        let h    = health_check(&bc);
        let json = serde_json::to_string(&h).expect("serialize ok");
        let back: HealthStatus = serde_json::from_str(&json).expect("deserialize ok");
        assert_eq!(back.height, h.height);
        assert_eq!(back.difficulty, h.difficulty);
        assert_eq!(back.version, VERSION.to_string());
    }

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("info"),  LogLevel::Info);
        assert_eq!(LogLevel::from_str("DEBUG"), LogLevel::Debug);
        assert_eq!(LogLevel::from_str("warn"),  LogLevel::Warn);
        assert_eq!(LogLevel::from_str("ERROR"), LogLevel::Error);
        assert_eq!(LogLevel::from_str("bogus"), LogLevel::Info);
    }

    #[test]
    fn test_uptime_increases() {
        // start_time() được khởi tạo một lần; uptime phải >= 0
        let bc = Blockchain::new();
        let h  = health_check(&bc);
        assert!(h.uptime_secs < 86400, "uptime không hợp lệ (> 1 ngày trong test)");
    }

    #[test]
    fn test_fee_fields_in_health() {
        let mut bc = Blockchain::new();
        bc.difficulty = 1;
        // Mine đủ blocks để fee estimator có data
        for _ in 0..5 {
            bc.add_block(vec![], "aabbccddaabbccddaabbccddaabbccddaabbccdd");
        }
        let h = health_check(&bc);
        assert!(h.fee_fast >= h.fee_medium, "fast >= medium");
        assert!(h.fee_medium >= h.fee_slow, "medium >= slow");
    }
}
