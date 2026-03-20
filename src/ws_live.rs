#![allow(dead_code)]
//! v14.8 — WebSocket Live Feed (Rust module)
//!
//! Embed `frontend/live.js` và cung cấp types cho WebSocket live feed:
//! WsEventType, ToastLevel, LiveEvent, ConnectionState.
//!
//! HTTP route: GET /static/live.js → trả binary đã embed.

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

// ── Embedded asset ────────────────────────────────────────────────────────────

pub static LIVE_JS: &[u8] = include_bytes!("../frontend/live.js");

// ── WsEventType ──────────────────────────────────────────────────────────────

/// Loại sự kiện WebSocket từ server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsEventType {
    NewBlock,
    NewTx,
    Stats,
    Unknown(String),
}

impl WsEventType {
    /// Parse từ string (case-insensitive, hỗ trợ cả camelCase và snake_case).
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "new_block" | "newblock" => Self::NewBlock,
            "new_tx"    | "newtx"   => Self::NewTx,
            "stats"                  => Self::Stats,
            other                    => Self::Unknown(other.to_string()),
        }
    }

    /// Canonical string representation.
    pub fn as_str(&self) -> &str {
        match self {
            Self::NewBlock      => "new_block",
            Self::NewTx         => "new_tx",
            Self::Stats         => "stats",
            Self::Unknown(s)    => s.as_str(),
        }
    }

    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}

// ── ToastLevel ───────────────────────────────────────────────────────────────

/// Mức độ toast notification (map tới CSS class trong live.js).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Info    => "pk-toast-info",
            Self::Success => "pk-toast-success",
            Self::Warning => "pk-toast-warning",
            Self::Error   => "pk-toast-error",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Info    => "📨",
            Self::Success => "⛏",
            Self::Warning => "⚠",
            Self::Error   => "✗",
        }
    }
}

// ── LiveEvent ─────────────────────────────────────────────────────────────────

/// Sự kiện live feed (block hoặc tx).
#[derive(Debug, Clone)]
pub struct LiveEvent {
    pub event_type: WsEventType,
    pub height:     Option<u64>,
    pub hash:       Option<String>,
    pub tx_count:   Option<u32>,
    pub timestamp:  Option<i64>,
}

impl LiveEvent {
    /// Tạo NewBlock event.
    pub fn new_block(height: u64, hash: impl Into<String>, tx_count: u32) -> Self {
        Self {
            event_type: WsEventType::NewBlock,
            height:     Some(height),
            hash:       Some(hash.into()),
            tx_count:   Some(tx_count),
            timestamp:  None,
        }
    }

    /// Tạo NewTx event.
    pub fn new_tx(txid: impl Into<String>) -> Self {
        Self {
            event_type: WsEventType::NewTx,
            height:     None,
            hash:       Some(txid.into()),
            tx_count:   None,
            timestamp:  None,
        }
    }

    /// Toast message tương ứng với event này (mirrors live.js onNewBlock/onNewTx).
    pub fn toast_message(&self) -> String {
        match &self.event_type {
            WsEventType::NewBlock => {
                let h = self.height.map(|n| n.to_string()).unwrap_or_else(|| "?".to_string());
                format!("⛏ New Block #{}", h)
            }
            WsEventType::NewTx => {
                let txid = self.hash.as_deref().unwrap_or("");
                format!("📨 New TX {}", short_hash(txid))
            }
            WsEventType::Stats    => "📊 Stats update".to_string(),
            WsEventType::Unknown(t) => format!("? {}", t),
        }
    }

    /// Toast level tương ứng.
    pub fn toast_level(&self) -> ToastLevel {
        match &self.event_type {
            WsEventType::NewBlock => ToastLevel::Success,
            WsEventType::NewTx    => ToastLevel::Info,
            WsEventType::Stats    => ToastLevel::Info,
            WsEventType::Unknown(_) => ToastLevel::Warning,
        }
    }
}

// ── ConnectionState ───────────────────────────────────────────────────────────

/// Trạng thái kết nối WebSocket (mirrors setStatus trong live.js).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Reconnecting { attempt: u32 },
}

impl ConnectionState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Connecting          => "● Connecting…",
            Self::Connected           => "● Live",
            Self::Disconnected        => "○ Offline",
            Self::Reconnecting { .. } => "● Reconnecting…",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Connecting          => "pk-ws-connecting",
            Self::Connected           => "pk-ws-live",
            Self::Disconnected        => "pk-ws-offline",
            Self::Reconnecting { .. } => "pk-ws-connecting",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Connected)
    }

    pub fn attempt(&self) -> u32 {
        match self {
            Self::Reconnecting { attempt } => *attempt,
            _ => 0,
        }
    }
}

// ── Reconnect backoff ─────────────────────────────────────────────────────────

/// Exponential backoff delay (ms) cho lần reconnect thứ `attempt`.
/// Mirrors live.js: `Math.min(1000 * Math.pow(2, attempt), 30_000)`.
pub fn reconnect_delay_ms(attempt: u32) -> u64 {
    let base: u64 = 1000u64.saturating_mul(1u64 << attempt.min(30));
    base.min(30_000)
}

// ── Format helpers ────────────────────────────────────────────────────────────

/// Rút gọn hash/txid: lấy 16 ký tự đầu + "…" (mirrors live.js shortH()).
pub fn short_hash(h: &str) -> String {
    if h.len() > 16 {
        format!("{}…", &h[..16])
    } else {
        h.to_string()
    }
}

// ── HTTP route ────────────────────────────────────────────────────────────────

async fn serve_live_js() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        LIVE_JS,
    )
        .into_response()
}

/// Router: GET /static/live.js
pub fn live_router() -> Router {
    Router::new().route("/static/live.js", get(serve_live_js))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── WsEventType ──────────────────────────────────────────────────────────

    #[test]
    fn event_type_new_block_variants() {
        assert_eq!(WsEventType::from_str("new_block"), WsEventType::NewBlock);
        assert_eq!(WsEventType::from_str("newblock"),  WsEventType::NewBlock);
        assert_eq!(WsEventType::from_str("NEW_BLOCK"), WsEventType::NewBlock);
        assert_eq!(WsEventType::from_str("NewBlock"),  WsEventType::NewBlock);
    }

    #[test]
    fn event_type_new_tx_variants() {
        assert_eq!(WsEventType::from_str("new_tx"), WsEventType::NewTx);
        assert_eq!(WsEventType::from_str("newtx"),  WsEventType::NewTx);
        assert_eq!(WsEventType::from_str("NEW_TX"), WsEventType::NewTx);
    }

    #[test]
    fn event_type_stats() {
        assert_eq!(WsEventType::from_str("stats"), WsEventType::Stats);
        assert_eq!(WsEventType::from_str("STATS"), WsEventType::Stats);
    }

    #[test]
    fn event_type_unknown() {
        let t = WsEventType::from_str("foobar");
        assert_eq!(t, WsEventType::Unknown("foobar".to_string()));
        assert!(!t.is_known());
    }

    #[test]
    fn event_type_is_known() {
        assert!(WsEventType::NewBlock.is_known());
        assert!(WsEventType::NewTx.is_known());
        assert!(WsEventType::Stats.is_known());
        assert!(!WsEventType::Unknown("x".to_string()).is_known());
    }

    #[test]
    fn event_type_as_str() {
        assert_eq!(WsEventType::NewBlock.as_str(), "new_block");
        assert_eq!(WsEventType::NewTx.as_str(),    "new_tx");
        assert_eq!(WsEventType::Stats.as_str(),    "stats");
        assert_eq!(WsEventType::Unknown("ping".to_string()).as_str(), "ping");
    }

    // ── ToastLevel ───────────────────────────────────────────────────────────

    #[test]
    fn toast_level_css_class() {
        assert_eq!(ToastLevel::Info.css_class(),    "pk-toast-info");
        assert_eq!(ToastLevel::Success.css_class(), "pk-toast-success");
        assert_eq!(ToastLevel::Warning.css_class(), "pk-toast-warning");
        assert_eq!(ToastLevel::Error.css_class(),   "pk-toast-error");
    }

    #[test]
    fn toast_level_icon() {
        assert_eq!(ToastLevel::Success.icon(), "⛏");
        assert_eq!(ToastLevel::Info.icon(),    "📨");
        assert_eq!(ToastLevel::Warning.icon(), "⚠");
        assert_eq!(ToastLevel::Error.icon(),   "✗");
    }

    // ── LiveEvent ─────────────────────────────────────────────────────────────

    #[test]
    fn live_event_new_block_fields() {
        let ev = LiveEvent::new_block(42, "deadbeef1234", 3);
        assert_eq!(ev.event_type, WsEventType::NewBlock);
        assert_eq!(ev.height,   Some(42));
        assert_eq!(ev.hash,     Some("deadbeef1234".to_string()));
        assert_eq!(ev.tx_count, Some(3));
        assert!(ev.timestamp.is_none());
    }

    #[test]
    fn live_event_new_tx_fields() {
        let ev = LiveEvent::new_tx("abcdef0123456789");
        assert_eq!(ev.event_type, WsEventType::NewTx);
        assert!(ev.height.is_none());
        assert_eq!(ev.hash, Some("abcdef0123456789".to_string()));
    }

    #[test]
    fn live_event_toast_message_block() {
        let ev = LiveEvent::new_block(100, "abc", 1);
        assert!(ev.toast_message().contains("100"));
        assert!(ev.toast_message().contains("⛏"));
    }

    #[test]
    fn live_event_toast_message_tx() {
        let ev = LiveEvent::new_tx("abcdef0123456789ffff");
        let msg = ev.toast_message();
        assert!(msg.contains("📨"));
        assert!(msg.contains("abcdef01234567"));
    }

    #[test]
    fn live_event_toast_level_block() {
        assert_eq!(LiveEvent::new_block(1, "h", 0).toast_level(), ToastLevel::Success);
    }

    #[test]
    fn live_event_toast_level_tx() {
        assert_eq!(LiveEvent::new_tx("x").toast_level(), ToastLevel::Info);
    }

    #[test]
    fn live_event_stats_toast() {
        let ev = LiveEvent {
            event_type: WsEventType::Stats,
            height: None,
            hash: None,
            tx_count: None,
            timestamp: None,
        };
        assert!(ev.toast_message().contains("Stats"));
        assert_eq!(ev.toast_level(), ToastLevel::Info);
    }

    #[test]
    fn live_event_unknown_toast() {
        let ev = LiveEvent {
            event_type: WsEventType::Unknown("ping".to_string()),
            height: None,
            hash: None,
            tx_count: None,
            timestamp: None,
        };
        let msg = ev.toast_message();
        assert!(msg.contains("ping"));
        assert_eq!(ev.toast_level(), ToastLevel::Warning);
    }

    // ── ConnectionState ───────────────────────────────────────────────────────

    #[test]
    fn connection_state_label() {
        assert_eq!(ConnectionState::Connecting.label(),          "● Connecting…");
        assert_eq!(ConnectionState::Connected.label(),           "● Live");
        assert_eq!(ConnectionState::Disconnected.label(),        "○ Offline");
        assert_eq!(ConnectionState::Reconnecting { attempt: 3 }.label(), "● Reconnecting…");
    }

    #[test]
    fn connection_state_css_class() {
        assert_eq!(ConnectionState::Connecting.css_class(),          "pk-ws-connecting");
        assert_eq!(ConnectionState::Connected.css_class(),           "pk-ws-live");
        assert_eq!(ConnectionState::Disconnected.css_class(),        "pk-ws-offline");
        assert_eq!(ConnectionState::Reconnecting { attempt: 0 }.css_class(), "pk-ws-connecting");
    }

    #[test]
    fn connection_state_is_active() {
        assert!( ConnectionState::Connected.is_active());
        assert!(!ConnectionState::Connecting.is_active());
        assert!(!ConnectionState::Disconnected.is_active());
        assert!(!ConnectionState::Reconnecting { attempt: 1 }.is_active());
    }

    #[test]
    fn connection_state_attempt() {
        assert_eq!(ConnectionState::Reconnecting { attempt: 5 }.attempt(), 5);
        assert_eq!(ConnectionState::Connected.attempt(), 0);
    }

    // ── reconnect_delay_ms ────────────────────────────────────────────────────

    #[test]
    fn reconnect_delay_attempt_0() {
        assert_eq!(reconnect_delay_ms(0), 1000);
    }

    #[test]
    fn reconnect_delay_attempt_1() {
        assert_eq!(reconnect_delay_ms(1), 2000);
    }

    #[test]
    fn reconnect_delay_attempt_2() {
        assert_eq!(reconnect_delay_ms(2), 4000);
    }

    #[test]
    fn reconnect_delay_attempt_3() {
        assert_eq!(reconnect_delay_ms(3), 8000);
    }

    #[test]
    fn reconnect_delay_attempt_4() {
        assert_eq!(reconnect_delay_ms(4), 16000);
    }

    #[test]
    fn reconnect_delay_capped_at_30s() {
        // 2^5 = 32s > 30s → capped
        assert_eq!(reconnect_delay_ms(5), 30_000);
        assert_eq!(reconnect_delay_ms(10), 30_000);
        assert_eq!(reconnect_delay_ms(99), 30_000);
    }

    // ── short_hash ────────────────────────────────────────────────────────────

    #[test]
    fn short_hash_long() {
        let h = "abcdef0123456789deadbeef";
        let s = short_hash(h);
        assert!(s.ends_with('…'));
        assert_eq!(&s[..16], "abcdef0123456789");
    }

    #[test]
    fn short_hash_exact_16() {
        let h = "abcdef0123456789";
        assert_eq!(short_hash(h), h);
    }

    #[test]
    fn short_hash_short() {
        assert_eq!(short_hash("abc"), "abc");
    }

    #[test]
    fn short_hash_empty() {
        assert_eq!(short_hash(""), "");
    }

    // ── LIVE_JS embed ─────────────────────────────────────────────────────────

    #[test]
    fn live_js_not_empty() {
        assert!(!LIVE_JS.is_empty());
    }

    #[test]
    fn live_js_contains_websocket() {
        let src = std::str::from_utf8(LIVE_JS).unwrap();
        assert!(src.contains("WebSocket"));
    }

    #[test]
    fn live_js_contains_reconnect() {
        let src = std::str::from_utf8(LIVE_JS).unwrap();
        assert!(src.contains("scheduleReconnect") || src.contains("reconnect"));
    }

    #[test]
    fn live_js_contains_toast() {
        let src = std::str::from_utf8(LIVE_JS).unwrap();
        assert!(src.contains("toast"));
    }

    #[test]
    fn live_js_contains_ws_path() {
        let src = std::str::from_utf8(LIVE_JS).unwrap();
        assert!(src.contains("/ws"));
    }

    #[test]
    fn live_js_exports_pkt_live() {
        let src = std::str::from_utf8(LIVE_JS).unwrap();
        assert!(src.contains("pktLive"));
    }
}
