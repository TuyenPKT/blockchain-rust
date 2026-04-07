#![allow(dead_code)]
//! v11.5 — Multi-chain Read-Only API
//!
//! Expose trạng thái của nhiều chain (PKT, ETH, BTC, ...) qua IBC light clients.
//! Tất cả endpoints đều GET (read-only, public).
//!
//! Registry được pre-seed với 3 chains:
//!   "pkt-mainnet" — chain nội bộ (IbcChain)
//!   "eth-mainnet" — Ethereum (mock IbcChain, advance bởi relayer sim)
//!   "btc-mainnet" — Bitcoin  (mock IbcChain, advance bởi relayer sim)
//!
//! Endpoints:
//!   GET /api/chains                              — list tất cả chains
//!   GET /api/chains/:chain_id                    — chain detail (clients, conns, channels)
//!   GET /api/chains/:chain_id/clients            — danh sách light clients
//!   GET /api/chains/:chain_id/client/:client_id  — client state cụ thể
//!   GET /api/chains/:chain_id/connections        — danh sách connections
//!   GET /api/chains/:chain_id/channels           — danh sách channels
//!   GET /api/chains/:chain_id/packets/pending    — packets chưa được ack

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::ibc::{IbcChain, Ordering};

// ─── ChainMeta ────────────────────────────────────────────────────────────────

/// Metadata bổ sung cho mỗi chain trong registry.
#[derive(Debug, Clone)]
pub struct ChainMeta {
    pub chain_type:  ChainType,
    /// RPC URL tham chiếu (không dùng cho call thật — đây là read-only mirror).
    pub rpc_url:     Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainType {
    Pkt,
    Ethereum,
    Bitcoin,
    Custom(String),
}

impl ChainType {
    pub fn as_str(&self) -> &str {
        match self {
            ChainType::Pkt          => "pkt",
            ChainType::Ethereum     => "ethereum",
            ChainType::Bitcoin      => "bitcoin",
            ChainType::Custom(s)    => s.as_str(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pkt"      => ChainType::Pkt,
            "ethereum" => ChainType::Ethereum,
            "bitcoin"  => ChainType::Bitcoin,
            other      => ChainType::Custom(other.to_string()),
        }
    }
}

// ─── MultiChainRegistry ───────────────────────────────────────────────────────

pub struct MultiChainRegistry {
    pub chains: HashMap<String, (IbcChain, ChainMeta)>,
}

impl MultiChainRegistry {
    pub fn new() -> Self {
        MultiChainRegistry { chains: HashMap::new() }
    }

    /// Thêm chain vào registry.
    pub fn register(&mut self, chain: IbcChain, meta: ChainMeta) {
        self.chains.insert(chain.chain_id.clone(), (chain, meta));
    }

    pub fn get(&self, chain_id: &str) -> Option<&(IbcChain, ChainMeta)> {
        self.chains.get(chain_id)
    }

    pub fn chain_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.chains.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Số IBC connections đang mở trên chain.
    pub fn open_connections(&self, chain_id: &str) -> usize {
        self.chains.get(chain_id)
            .map(|(c, _)| c.connections.values()
                .filter(|conn| matches!(conn.state, crate::ibc::ConnState::Open))
                .count())
            .unwrap_or(0)
    }

    /// Số IBC channels đang mở trên chain.
    pub fn open_channels(&self, chain_id: &str) -> usize {
        self.chains.get(chain_id)
            .map(|(c, _)| c.channels.values()
                .filter(|ch| matches!(ch.state, crate::ibc::ChanState::Open))
                .count())
            .unwrap_or(0)
    }

    /// Packets đã send nhưng chưa được ack (pending).
    pub fn pending_packets(&self, chain_id: &str) -> Vec<(String, u64)> {
        let Some((chain, _)) = self.chains.get(chain_id) else { return vec![] };
        chain.commitments.keys()
            .filter(|(chan_id, seq)| !chain.acks.contains_key(&(chan_id.clone(), *seq)))
            .map(|(c, s)| (c.clone(), *s))
            .collect()
    }
}

/// Tạo registry pre-seeded với PKT + ETH + BTC, cùng với IBC relayer setup mẫu.
pub fn default_registry() -> MultiChainRegistry {
    let mut reg = MultiChainRegistry::new();

    // PKT chain
    let mut pkt = IbcChain::new("pkt-mainnet");
    pkt.advance(1000);
    reg.register(pkt, ChainMeta {
        chain_type:  ChainType::Pkt,
        rpc_url:     Some("https://oceif.com/blockchain-rust/api".into()),
        description: "PKT Mainnet (OCEIF)".into(),
    });

    // ETH chain (light client mirror)
    let mut eth = IbcChain::new("eth-mainnet");
    eth.advance(19_000_000);
    // ETH có client trỏ về PKT
    let pkt_hash = blake3::hash(b"pkt-genesis").into();
    eth.create_client("pkt-client-0", "pkt-mainnet", 1000, pkt_hash);
    reg.register(eth, ChainMeta {
        chain_type:  ChainType::Ethereum,
        rpc_url:     Some("https://eth-rpc.example.com".into()),
        description: "Ethereum mainnet (IBC light client mirror)".into(),
    });

    // BTC chain (light client mirror, SPV-style)
    let mut btc = IbcChain::new("btc-mainnet");
    btc.advance(840_000);
    reg.register(btc, ChainMeta {
        chain_type:  ChainType::Bitcoin,
        rpc_url:     None,
        description: "Bitcoin mainnet (SPV light client mirror)".into(),
    });

    // Tạo IBC connection PKT ↔ ETH qua Relayer để demo
    setup_pkt_eth_ibc(&mut reg);

    reg
}

/// Setup IBC connection + channel giữa PKT và ETH chains.
fn setup_pkt_eth_ibc(reg: &mut MultiChainRegistry) {
    let Some((pkt, _)) = reg.chains.get_mut("pkt-mainnet") else { return };
    let pkt_hash: [u8; 32] = blake3::hash(b"eth-genesis").into();
    pkt.create_client("eth-client-0", "eth-mainnet", 19_000_000, pkt_hash);

    // Xử lý thông qua Relayer để tránh borrow conflict
    let (pkt_chain, _) = reg.chains.remove("pkt-mainnet").unwrap();
    let (eth_chain, _) = reg.chains.remove("eth-mainnet").unwrap();

    let mut relayer = crate::ibc::Relayer::new(pkt_chain, eth_chain);
    // Connection handshake PKT → ETH
    let _ = relayer.connection_handshake("eth-client-0", "pkt-client-0");
    // Channel handshake
    let _ = relayer.channel_handshake(
        "transfer", "transfer",
        "connection-0", "connection-0",
        Ordering::Unordered,
    );

    let (pkt_back, eth_back) = relayer.into_chains();

    reg.chains.insert("pkt-mainnet".into(), (pkt_back, ChainMeta {
        chain_type:  ChainType::Pkt,
        rpc_url:     Some("https://oceif.com/blockchain-rust/api".into()),
        description: "PKT Mainnet (OCEIF)".into(),
    }));
    reg.chains.insert("eth-mainnet".into(), (eth_back, ChainMeta {
        chain_type:  ChainType::Ethereum,
        rpc_url:     Some("https://eth-rpc.example.com".into()),
        description: "Ethereum mainnet (IBC light client mirror)".into(),
    }));
}

// ─── JSON helpers ─────────────────────────────────────────────────────────────

fn chain_summary(chain: &IbcChain, meta: &ChainMeta) -> Value {
    json!({
        "chain_id":    chain.chain_id,
        "chain_type":  meta.chain_type.as_str(),
        "description": meta.description,
        "height":      chain.height,
        "state_root":  hex::encode(chain.state_root),
        "clients":     chain.clients.len(),
        "connections": chain.connections.len(),
        "channels":    chain.channels.len(),
        "rpc_url":     meta.rpc_url,
    })
}

fn client_info(id: &str, cs: &crate::ibc::ClientState) -> Value {
    json!({
        "client_id":     id,
        "chain_id":      cs.chain_id,
        "latest_height": cs.latest_height,
        "frozen":        cs.frozen,
        "latest_hash":   hex::encode(cs.latest_hash),
        "header_count":  cs.headers.len(),
    })
}

fn connection_info(conn: &crate::ibc::Connection) -> Value {
    json!({
        "id":                    conn.id,
        "client_id":             conn.client_id,
        "counterparty_chain_id": conn.counterparty_chain_id,
        "counterparty_conn_id":  conn.counterparty_conn_id,
        "state":                 conn.state.label(),
        "version":               conn.version,
    })
}

fn channel_info(ch: &crate::ibc::Channel) -> Value {
    json!({
        "id":                      ch.id,
        "port":                    ch.port,
        "connection_id":           ch.connection_id,
        "counterparty_channel_id": ch.counterparty_channel_id,
        "counterparty_port":       ch.counterparty_port,
        "ordering":                ch.ordering.label(),
        "state":                   ch.state.label(),
        "next_seq_send":           ch.next_seq_send,
        "next_seq_recv":           ch.next_seq_recv,
    })
}

// ─── State ────────────────────────────────────────────────────────────────────

pub type MultiDb = Arc<Mutex<MultiChainRegistry>>;

fn err_resp(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/chains — list tất cả chains.
async fn get_chains(State(db): State<MultiDb>) -> Json<Value> {
    let reg   = db.lock().await;
    let items: Vec<_> = reg.chain_ids().iter()
        .filter_map(|id| reg.get(id).map(|(c, m)| chain_summary(c, m)))
        .collect();
    Json(json!({ "count": items.len(), "chains": items }))
}

/// GET /api/chains/:chain_id — chain detail.
async fn get_chain(
    Path(chain_id): Path<String>,
    State(db):      State<MultiDb>,
) -> axum::response::Response {
    let reg = db.lock().await;
    match reg.get(&chain_id) {
        None => err_resp(StatusCode::NOT_FOUND, "chain not found"),
        Some((chain, meta)) => {
            let mut summary = chain_summary(chain, meta);
            // Thêm pending packet count
            let pending = reg.pending_packets(&chain_id).len();
            summary["pending_packets"] = json!(pending);
            Json(summary).into_response()
        }
    }
}

/// GET /api/chains/:chain_id/clients — danh sách light clients.
async fn get_clients(
    Path(chain_id): Path<String>,
    State(db):      State<MultiDb>,
) -> axum::response::Response {
    let reg = db.lock().await;
    match reg.get(&chain_id) {
        None => err_resp(StatusCode::NOT_FOUND, "chain not found"),
        Some((chain, _)) => {
            let items: Vec<_> = chain.clients.iter()
                .map(|(id, cs)| client_info(id, cs))
                .collect();
            Json(json!({ "chain_id": chain_id, "count": items.len(), "clients": items }))
                .into_response()
        }
    }
}

/// GET /api/chains/:chain_id/client/:client_id — client state cụ thể.
async fn get_client(
    Path((chain_id, client_id)): Path<(String, String)>,
    State(db):                   State<MultiDb>,
) -> axum::response::Response {
    let reg = db.lock().await;
    match reg.get(&chain_id) {
        None => err_resp(StatusCode::NOT_FOUND, "chain not found"),
        Some((chain, _)) => match chain.clients.get(&client_id) {
            None     => err_resp(StatusCode::NOT_FOUND, "client not found"),
            Some(cs) => Json(client_info(&client_id, cs)).into_response(),
        }
    }
}

/// GET /api/chains/:chain_id/connections — danh sách connections.
async fn get_connections(
    Path(chain_id): Path<String>,
    State(db):      State<MultiDb>,
) -> axum::response::Response {
    let reg = db.lock().await;
    match reg.get(&chain_id) {
        None => err_resp(StatusCode::NOT_FOUND, "chain not found"),
        Some((chain, _)) => {
            let items: Vec<_> = chain.connections.values()
                .map(connection_info)
                .collect();
            Json(json!({ "chain_id": chain_id, "count": items.len(), "connections": items }))
                .into_response()
        }
    }
}

/// GET /api/chains/:chain_id/channels — danh sách channels.
async fn get_channels(
    Path(chain_id): Path<String>,
    State(db):      State<MultiDb>,
) -> axum::response::Response {
    let reg = db.lock().await;
    match reg.get(&chain_id) {
        None => err_resp(StatusCode::NOT_FOUND, "chain not found"),
        Some((chain, _)) => {
            let items: Vec<_> = chain.channels.values()
                .map(channel_info)
                .collect();
            Json(json!({ "chain_id": chain_id, "count": items.len(), "channels": items }))
                .into_response()
        }
    }
}

/// GET /api/chains/:chain_id/packets/pending — packets chưa được ack.
async fn get_pending_packets(
    Path(chain_id): Path<String>,
    State(db):      State<MultiDb>,
) -> axum::response::Response {
    let reg = db.lock().await;
    if reg.get(&chain_id).is_none() {
        return err_resp(StatusCode::NOT_FOUND, "chain not found");
    }
    let pending = reg.pending_packets(&chain_id);
    let items: Vec<_> = pending.iter()
        .map(|(chan_id, seq)| json!({ "channel_id": chan_id, "sequence": seq }))
        .collect();
    Json(json!({ "chain_id": chain_id, "count": items.len(), "pending": items }))
        .into_response()
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn multi_chain_router(db: MultiDb) -> Router {
    Router::new()
        .route("/api/chains",
            get(get_chains))
        .route("/api/chains/:chain_id",
            get(get_chain))
        .route("/api/chains/:chain_id/clients",
            get(get_clients))
        .route("/api/chains/:chain_id/client/:client_id",
            get(get_client))
        .route("/api/chains/:chain_id/connections",
            get(get_connections))
        .route("/api/chains/:chain_id/channels",
            get(get_channels))
        .route("/api/chains/:chain_id/packets/pending",
            get(get_pending_packets))
        .with_state(db)
}

pub fn open_default() -> MultiDb {
    Arc::new(Mutex::new(default_registry()))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> MultiChainRegistry {
        default_registry()
    }

    // ── ChainType ─────────────────────────────────────────────────────────────

    #[test]
    fn test_chain_type_as_str() {
        assert_eq!(ChainType::Pkt.as_str(),      "pkt");
        assert_eq!(ChainType::Ethereum.as_str(),  "ethereum");
        assert_eq!(ChainType::Bitcoin.as_str(),   "bitcoin");
        assert_eq!(ChainType::Custom("cosmos".into()).as_str(), "cosmos");
    }

    #[test]
    fn test_chain_type_from_str() {
        assert_eq!(ChainType::from_str("pkt"),      ChainType::Pkt);
        assert_eq!(ChainType::from_str("ethereum"),  ChainType::Ethereum);
        assert_eq!(ChainType::from_str("bitcoin"),   ChainType::Bitcoin);
        assert_eq!(ChainType::from_str("PKT"),       ChainType::Pkt);
    }

    #[test]
    fn test_chain_type_custom() {
        assert_eq!(ChainType::from_str("cosmos"),
                   ChainType::Custom("cosmos".into()));
    }

    // ── MultiChainRegistry ────────────────────────────────────────────────────

    #[test]
    fn test_default_registry_has_three_chains() {
        let reg = make_registry();
        assert_eq!(reg.chains.len(), 3);
    }

    #[test]
    fn test_default_registry_chain_ids_sorted() {
        let reg = make_registry();
        let ids = reg.chain_ids();
        assert!(ids.contains(&"pkt-mainnet".to_string()));
        assert!(ids.contains(&"eth-mainnet".to_string()));
        assert!(ids.contains(&"btc-mainnet".to_string()));
        // Sorted
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_registry_get_pkt_chain() {
        let reg = make_registry();
        let (chain, meta) = reg.get("pkt-mainnet").unwrap();
        assert_eq!(chain.chain_id, "pkt-mainnet");
        assert_eq!(meta.chain_type, ChainType::Pkt);
        assert!(chain.height >= 1000);
    }

    #[test]
    fn test_registry_get_eth_chain() {
        let reg = make_registry();
        let (chain, meta) = reg.get("eth-mainnet").unwrap();
        assert_eq!(meta.chain_type, ChainType::Ethereum);
        assert!(chain.height >= 19_000_000);
    }

    #[test]
    fn test_registry_get_btc_chain() {
        let reg = make_registry();
        let (chain, meta) = reg.get("btc-mainnet").unwrap();
        assert_eq!(meta.chain_type, ChainType::Bitcoin);
        assert!(chain.height >= 840_000);
    }

    #[test]
    fn test_registry_get_unknown_chain() {
        let reg = make_registry();
        assert!(reg.get("unknown-chain").is_none());
    }

    #[test]
    fn test_pkt_has_eth_client() {
        let reg = make_registry();
        let (pkt, _) = reg.get("pkt-mainnet").unwrap();
        // PKT chain should have a client pointing to ETH
        assert!(!pkt.clients.is_empty());
    }

    #[test]
    fn test_eth_has_pkt_client() {
        let reg = make_registry();
        let (eth, _) = reg.get("eth-mainnet").unwrap();
        // ETH chain should have a client pointing to PKT
        assert!(!eth.clients.is_empty());
    }

    #[test]
    fn test_ibc_connection_established() {
        let reg = make_registry();
        // At least one open connection on PKT after relayer setup
        let open = reg.open_connections("pkt-mainnet");
        assert!(open >= 1);
    }

    #[test]
    fn test_ibc_channel_established() {
        let reg = make_registry();
        let open = reg.open_channels("pkt-mainnet");
        assert!(open >= 1);
    }

    #[test]
    fn test_pending_packets_empty_on_new_chain() {
        let reg = make_registry();
        // BTC has no packets sent
        let pending = reg.pending_packets("btc-mainnet");
        assert!(pending.is_empty());
    }

    #[test]
    fn test_pending_packets_unknown_chain_returns_empty() {
        let reg = make_registry();
        assert!(reg.pending_packets("nope").is_empty());
    }

    #[test]
    fn test_register_custom_chain() {
        let mut reg = MultiChainRegistry::new();
        let chain   = IbcChain::new("cosmos-hub");
        reg.register(chain, ChainMeta {
            chain_type:  ChainType::Custom("cosmos".into()),
            rpc_url:     None,
            description: "Cosmos Hub".into(),
        });
        assert_eq!(reg.chains.len(), 1);
        assert!(reg.get("cosmos-hub").is_some());
    }

    #[test]
    fn test_open_connections_unknown_chain() {
        let reg = make_registry();
        assert_eq!(reg.open_connections("nope"), 0);
    }

    #[test]
    fn test_open_channels_unknown_chain() {
        let reg = make_registry();
        assert_eq!(reg.open_channels("nope"), 0);
    }

    // ── JSON helpers ──────────────────────────────────────────────────────────

    #[test]
    fn test_chain_summary_fields() {
        let chain = IbcChain::new("test-chain");
        let meta  = ChainMeta {
            chain_type:  ChainType::Custom("test".into()),
            rpc_url:     Some("https://rpc".into()),
            description: "Test chain".into(),
        };
        let v = chain_summary(&chain, &meta);
        assert_eq!(v["chain_id"],   "test-chain");
        assert_eq!(v["chain_type"], "test");
        assert!(v["state_root"].as_str().unwrap().len() == 64);
    }
}
