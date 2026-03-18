#![allow(dead_code)]
//! v8.1 — PKTScan WebSocket Live Feed
//!
//! Broadcast real-time blockchain events to all connected PKTScan clients.
//!
//! Events:
//!   connected    → welcome message on connect
//!   new_block    → new block mined (height, hash, tx_count, timestamp)
//!   new_tx       → new non-coinbase transaction confirmed
//!   stats        → periodic network stats (height, hashrate, mempool, difficulty)
//!   lag          → client fell behind, skipped N messages
//!
//! Endpoint: GET /ws  (WebSocket upgrade)
//!
//! Usage:
//!   let hub = Arc::new(WsHub::new());
//!   pktscan_ws::spawn_poller(Arc::clone(&hub), db.clone(), 5);
//!   let app = rest_router.merge(pktscan_ws::ws_router(hub));

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::pktscan_api::{avg_block_time_secs, estimate_hashrate, ScanDb};

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

// ─── WebSocket Handler ────────────────────────────────────────────────────────

async fn ws_handler(
    ws:         WebSocketUpgrade,
    State(hub): State<Arc<WsHub>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, hub))
}

async fn handle_socket(mut socket: WebSocket, hub: Arc<WsHub>) {
    // Welcome frame
    let welcome = json!({
        "type": "connected",
        "msg":  "PKTScan live feed active"
    })
    .to_string();
    if socket.send(Message::Text(welcome.into())).await.is_err() {
        return;
    }

    let mut rx = hub.subscribe();
    loop {
        match rx.recv().await {
            Ok(msg) => {
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

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn ws_router(hub: Arc<WsHub>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(hub)
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
                        hub.send(&WsEvent::NewTx {
                            tx_id:       tx.tx_id.clone(),
                            fee:         tx.fee,
                            is_coinbase: false,
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
            tx_id: "deadbeef".to_string(),
            fee:   500,
            is_coinbase: false,
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
            tx_id: "txabc".to_string(),
            fee:   250,
            is_coinbase: false,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains(r#""type":"new_tx""#));
        assert!(s.contains(r#""fee":250"#));
    }

    // ── ws_router ─────────────────────────────────────────────────────────

    #[test]
    fn test_ws_router_builds() {
        let hub = Arc::new(WsHub::new());
        let _r = ws_router(hub);
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
}
