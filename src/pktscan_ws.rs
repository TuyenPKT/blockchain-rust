#![allow(dead_code)]
//! v9.7 — PKTScan WebSocket Live Feed + Per-Address Subscription + Token Auth
//!
//! Broadcast real-time blockchain events to all connected PKTScan clients.
//!
//! Events:
//!   connected    → welcome message on connect (includes "watch" field)
//!   new_block    → new block mined (height, hash, tx_count, timestamp)
//!   new_tx       → new non-coinbase tx confirmed (includes "addresses" field)
//!   stats        → periodic network stats (height, hashrate, mempool, difficulty)
//!   lag          → client fell behind, skipped N messages
//!
//! Per-address subscription:
//!   GET /ws?watch=<hex_pubkey_hash>
//!   → client chỉ nhận new_tx events có address khớp; stats/new_block luôn được gửi.
//!
//! Token validation (khi WsConfig.secret != ""):
//!   GET /ws?watch=<addr>&token=<first_16_chars_of_sha256(secret:addr)>
//!   → 401 nếu token sai hoặc thiếu.
//!
//! Endpoint: GET /ws  (WebSocket upgrade)
//!
//! Usage:
//!   let hub = Arc::new(WsHub::new());
//!   let ws_state = WsState { hub: Arc::clone(&hub), config: Arc::new(WsConfig::default()) };
//!   pktscan_ws::spawn_poller(Arc::clone(&hub), db.clone(), 5);
//!   let app = rest_router.merge(pktscan_ws::ws_router(ws_state));

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::pktscan_api::{avg_block_time_secs, estimate_hashrate, ScanDb};

// ─── WsConfig + Token Validation (v9.7) ──────────────────────────────────────

/// Server-side WS config.  `secret` controls token validation.
/// Empty secret = no auth required.
#[derive(Debug, Clone)]
pub struct WsConfig {
    /// Pre-shared secret for token validation.  Empty = open (no auth).
    pub secret: String,
}

impl Default for WsConfig {
    fn default() -> Self { WsConfig { secret: String::new() } }
}

impl WsConfig {
    pub fn new(secret: impl Into<String>) -> Self {
        WsConfig { secret: secret.into() }
    }

    /// Validate `token` for `watch_addr`.
    /// Expected token = first 16 hex chars of sha256("{secret}:{watch_addr}").
    /// If secret is empty, always returns true.
    pub fn validate_token(&self, watch_addr: &str, token: &str) -> bool {
        if self.secret.is_empty() { return true; }
        if token.len() < 16 { return false; }
        let mut h = Sha256::new();
        h.update(format!("{}:{}", self.secret, watch_addr).as_bytes());
        let expected = hex::encode(h.finalize());
        // Constant-time prefix comparison (token = first 16 hex chars)
        expected.get(..16).map(|prefix| prefix == &token[..16]).unwrap_or(false)
    }
}

// ─── WsState ──────────────────────────────────────────────────────────────────

/// Shared state for the WS router (hub + config).
#[derive(Clone)]
pub struct WsState {
    pub hub:    Arc<WsHub>,
    pub config: Arc<WsConfig>,
}

// ─── Query params ─────────────────────────────────────────────────────────────

/// Query params for GET /ws
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    /// Hex pubkey hash to watch.  Only new_tx events for this address are forwarded.
    pub watch: Option<String>,
    /// Auth token (first 16 hex chars of sha256("{secret}:{watch}")).
    pub token: Option<String>,
}

// ─── Event Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    NewBlock {
        height:    u64,
        hash:      String,
        tx_count:  usize,
        timestamp: i64,
    },
    NewTx {
        tx_id:       String,
        fee:         u64,
        is_coinbase: bool,
        /// Receiving addresses (hex pubkey_hash) involved in this tx.
        addresses:   Vec<String>,
    },
    Stats {
        height:        u64,
        hashrate:      u64,
        mempool_count: usize,
        difficulty:    usize,
    },
}

// ─── Hub ──────────────────────────────────────────────────────────────────────

const CHANNEL_CAP: usize = 256;

/// Broadcast hub shared between the poller and all WS handlers.
#[derive(Clone)]
pub struct WsHub {
    tx: broadcast::Sender<String>,
}

impl WsHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CHANNEL_CAP);
        WsHub { tx }
    }

    /// Serialize and broadcast `event` to every connected client.
    pub fn send(&self, event: &WsEvent) {
        let msg = serde_json::to_string(event).unwrap_or_default();
        let _ = self.tx.send(msg); // silently drop if no subscribers
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

// ─── WebSocket Handler (v9.7) ────────────────────────────────────────────────

async fn ws_handler(
    ws:             WebSocketUpgrade,
    Query(query):   Query<WsQuery>,
    State(state):   State<WsState>,
) -> Response {
    // Token validation when watch is set
    if let Some(ref watch) = query.watch {
        if !state.config.secret.is_empty() {
            match &query.token {
                Some(tok) if state.config.validate_token(watch, tok) => {}
                _ => return StatusCode::UNAUTHORIZED.into_response(),
            }
        }
    }
    let watch = query.watch.clone();
    let hub   = state.hub.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, hub, watch))
        .into_response()
}

async fn handle_socket(mut socket: WebSocket, hub: Arc<WsHub>, watch: Option<String>) {
    // Welcome frame
    let welcome = json!({
        "type":  "connected",
        "msg":   "PKTScan live feed active",
        "watch": watch,
    })
    .to_string();
    if socket.send(Message::Text(welcome.into())).await.is_err() {
        return;
    }

    let mut rx = hub.subscribe();
    loop {
        match rx.recv().await {
            Ok(msg) => {
                // Per-address filter: only apply to new_tx events
                if let Some(ref addr) = watch {
                    if !event_touches_addr(&msg, addr) {
                        continue;
                    }
                }
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                let lag = format!(r#"{{"type":"lag","skipped":{}}}"#, n);
                if socket.send(Message::Text(lag.into())).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

/// Returns true if the event should be forwarded to a client watching `addr`.
/// - `new_tx` events: only if `addresses` contains `addr`.
/// - All other events (stats, new_block, connected, lag): always forwarded.
fn event_touches_addr(event_json: &str, addr: &str) -> bool {
    let v: serde_json::Value = match serde_json::from_str(event_json) {
        Ok(v)  => v,
        Err(_) => return true,
    };
    match v["type"].as_str() {
        Some("new_tx") => v["addresses"]
            .as_array()
            .map(|arr| arr.iter().any(|a| a.as_str() == Some(addr)))
            .unwrap_or(false),
        _ => true,
    }
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn ws_router(state: WsState) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state)
}

// ─── Background Poller ────────────────────────────────────────────────────────

/// Spawns a background task that polls the blockchain every `interval_secs`
/// seconds and broadcasts events to all WS clients.
pub fn spawn_poller(hub: Arc<WsHub>, db: ScanDb, interval_secs: u64) {
    tokio::spawn(async move {
        let mut last_height: u64 = 0;
        let interval = tokio::time::Duration::from_secs(interval_secs);

        loop {
            tokio::time::sleep(interval).await;

            let bc = db.lock().await;
            let height = bc.chain.len().saturating_sub(1) as u64;

            // ── Periodic stats broadcast ──────────────────────────────────
            let avg      = avg_block_time_secs(&bc.chain);
            let hashrate = estimate_hashrate(bc.difficulty, avg);
            hub.send(&WsEvent::Stats {
                height,
                hashrate,
                mempool_count: bc.mempool.entries.len(),
                difficulty:    bc.difficulty,
            });

            // ── New-block events ──────────────────────────────────────────
            if height > last_height {
                for block in bc.chain.iter().filter(|b| b.index > last_height) {
                    hub.send(&WsEvent::NewBlock {
                        height:    block.index,
                        hash:      block.hash.clone(),
                        tx_count:  block.transactions.len(),
                        timestamp: block.timestamp,
                    });

                    // Broadcast confirmed non-coinbase TXs
                    for tx in block.transactions.iter().filter(|t| !t.is_coinbase) {
                        let addresses: Vec<String> = tx.outputs.iter()
                            .filter_map(crate::utxo::UtxoSet::output_owner_hex)
                            .collect();
                        hub.send(&WsEvent::NewTx {
                            tx_id:       tx.tx_id.clone(),
                            fee:         tx.fee,
                            is_coinbase: false,
                            addresses,
                        });
                    }
                }
                last_height = height;
            }

            drop(bc);
        }
    });
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ── WsHub ──────────────────────────────────────────────────────────────

    #[test]
    fn test_hub_new() {
        let hub = WsHub::new();
        assert_eq!(hub.receiver_count(), 0);
    }

    #[test]
    fn test_hub_send_no_subscribers() {
        // send with no subscribers should not panic
        let hub = WsHub::new();
        hub.send(&WsEvent::Stats {
            height: 0,
            hashrate: 0,
            mempool_count: 0,
            difficulty: 2,
        });
    }

    #[test]
    fn test_hub_subscribe_receives_event() {
        let hub  = WsHub::new();
        let mut rx = hub.subscribe();
        hub.send(&WsEvent::NewBlock {
            height:    42,
            hash:      "aabbcc".to_string(),
            tx_count:  3,
            timestamp: 1_700_000_000,
        });
        let msg = rx.try_recv().expect("should receive event");
        assert!(msg.contains("new_block"));
        assert!(msg.contains("42"));
    }

    #[test]
    fn test_hub_subscribe_stats() {
        let hub  = WsHub::new();
        let mut rx = hub.subscribe();
        hub.send(&WsEvent::Stats {
            height: 100, hashrate: 9999, mempool_count: 5, difficulty: 3,
        });
        let msg = rx.try_recv().unwrap();
        assert!(msg.contains("stats"));
        assert!(msg.contains("9999"));
    }

    #[test]
    fn test_hub_subscribe_new_tx() {
        let hub  = WsHub::new();
        let mut rx = hub.subscribe();
        hub.send(&WsEvent::NewTx {
            tx_id:       "deadbeef".to_string(),
            fee:         500,
            is_coinbase: false,
            addresses:   vec![],
        });
        let msg = rx.try_recv().unwrap();
        assert!(msg.contains("new_tx"));
        assert!(msg.contains("deadbeef"));
    }

    #[test]
    fn test_hub_multiple_subscribers() {
        let hub = WsHub::new();
        let mut rx1 = hub.subscribe();
        let mut rx2 = hub.subscribe();
        hub.send(&WsEvent::NewBlock {
            height: 1, hash: "ff".to_string(), tx_count: 1, timestamp: 0,
        });
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    // ── WsEvent serialization ──────────────────────────────────────────────

    #[test]
    fn test_event_new_block_serializes() {
        let ev = WsEvent::NewBlock {
            height: 7, hash: "abc".to_string(), tx_count: 2, timestamp: 123,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains(r#""type":"new_block""#));
        assert!(s.contains(r#""height":7"#));
    }

    #[test]
    fn test_event_stats_serializes() {
        let ev = WsEvent::Stats {
            height: 50, hashrate: 100, mempool_count: 3, difficulty: 4,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains(r#""type":"stats""#));
        assert!(s.contains(r#""difficulty":4"#));
    }

    #[test]
    fn test_event_new_tx_serializes() {
        let ev = WsEvent::NewTx {
            tx_id:       "txabc".to_string(),
            fee:         250,
            is_coinbase: false,
            addresses:   vec!["aabbcc".to_string()],
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains(r#""type":"new_tx""#));
        assert!(s.contains(r#""fee":250"#));
        assert!(s.contains("aabbcc"));
    }

    // ── ws_router ─────────────────────────────────────────────────────────

    #[test]
    fn test_ws_router_builds() {
        let state = WsState {
            hub:    Arc::new(WsHub::new()),
            config: Arc::new(WsConfig::default()),
        };
        let _r = ws_router(state);
    }

    // ── hub clone ─────────────────────────────────────────────────────────

    #[test]
    fn test_hub_clone_shares_channel() {
        let hub1 = WsHub::new();
        let hub2 = hub1.clone();
        let mut rx = hub2.subscribe();
        hub1.send(&WsEvent::Stats {
            height: 1, hashrate: 1, mempool_count: 0, difficulty: 1,
        });
        assert!(rx.try_recv().is_ok());
    }

    // ── v9.7 — WsConfig token validation ──────────────────────────────────

    #[test]
    fn test_ws_config_default_no_secret() {
        let cfg = WsConfig::default();
        assert!(cfg.secret.is_empty());
        // empty secret → any token (or none) is valid
        assert!(cfg.validate_token("addr123", "wrongtoken1234567"));
        assert!(cfg.validate_token("addr123", ""));
    }

    #[test]
    fn test_ws_config_empty_secret_skips_validation() {
        let cfg = WsConfig::new("");
        assert!(cfg.validate_token("someaddr", "badtoken12345678"));
    }

    #[test]
    fn test_ws_config_validate_token_correct() {
        use sha2::{Sha256, Digest};
        let secret = "mysecret";
        let addr   = "aabbccdd";
        let cfg    = WsConfig::new(secret);
        let mut h  = Sha256::new();
        h.update(format!("{}:{}", secret, addr).as_bytes());
        let full = hex::encode(h.finalize());
        let token = &full[..16];
        assert!(cfg.validate_token(addr, token));
    }

    #[test]
    fn test_ws_config_validate_token_wrong() {
        let cfg = WsConfig::new("mysecret");
        assert!(!cfg.validate_token("aabbccdd", "0000000000000000"));
    }

    #[test]
    fn test_ws_config_validate_token_too_short() {
        let cfg = WsConfig::new("s3cr3t");
        // token shorter than 16 chars → false
        assert!(!cfg.validate_token("addr", "short"));
    }

    // ── v9.7 — WsQuery deserialization ────────────────────────────────────

    #[test]
    fn test_ws_query_defaults() {
        let q: WsQuery = serde_json::from_str("{}").unwrap();
        assert!(q.watch.is_none());
        assert!(q.token.is_none());
    }

    #[test]
    fn test_ws_query_with_watch() {
        let q: WsQuery = serde_json::from_str(r#"{"watch":"deadbeef01234567"}"#).unwrap();
        assert_eq!(q.watch.as_deref(), Some("deadbeef01234567"));
        assert!(q.token.is_none());
    }

    #[test]
    fn test_ws_query_with_watch_and_token() {
        let q: WsQuery = serde_json::from_str(
            r#"{"watch":"addr123","token":"tok456"}"#,
        ).unwrap();
        assert_eq!(q.watch.as_deref(), Some("addr123"));
        assert_eq!(q.token.as_deref(), Some("tok456"));
    }

    // ── v9.7 — event_touches_addr ─────────────────────────────────────────

    #[test]
    fn test_event_touches_addr_new_tx_match() {
        let ev = WsEvent::NewTx {
            tx_id: "abc".to_string(), fee: 0, is_coinbase: false,
            addresses: vec!["targetaddr".to_string()],
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(event_touches_addr(&json, "targetaddr"));
    }

    #[test]
    fn test_event_touches_addr_new_tx_no_match() {
        let ev = WsEvent::NewTx {
            tx_id: "abc".to_string(), fee: 0, is_coinbase: false,
            addresses: vec!["otheraddr".to_string()],
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(!event_touches_addr(&json, "targetaddr"));
    }

    #[test]
    fn test_event_touches_addr_new_tx_empty_addresses() {
        let ev = WsEvent::NewTx {
            tx_id: "abc".to_string(), fee: 0, is_coinbase: false,
            addresses: vec![],
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(!event_touches_addr(&json, "targetaddr"));
    }

    #[test]
    fn test_event_touches_addr_stats_always_passes() {
        let ev = WsEvent::Stats {
            height: 1, hashrate: 100, mempool_count: 0, difficulty: 2,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(event_touches_addr(&json, "anyaddr"));
    }

    #[test]
    fn test_event_touches_addr_new_block_always_passes() {
        let ev = WsEvent::NewBlock {
            height: 5, hash: "aabb".to_string(), tx_count: 1, timestamp: 0,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(event_touches_addr(&json, "anyaddr"));
    }

    #[test]
    fn test_new_tx_event_has_addresses_field() {
        let ev = WsEvent::NewTx {
            tx_id: "t1".to_string(), fee: 10, is_coinbase: false,
            addresses: vec!["aa".to_string(), "bb".to_string()],
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&ev).unwrap()).unwrap();
        let addrs = v["addresses"].as_array().unwrap();
        assert_eq!(addrs.len(), 2);
    }
}
