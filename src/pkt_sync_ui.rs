#![allow(dead_code)]
//! v15.5 — Sync Status UI
//!
//! Progress bar cho quá trình sync testnet headers + UTXO:
//!   - TUI panel (ratatui Gauge) hiển thị trong dashboard
//!   - GET /api/testnet/sync-status → JSON cho web frontend
//!   - ASCII progress bar cho CLI / log output
//!
//! Không sửa file cũ — extend qua:
//!   - `render_sync_progress_panel(frame, area, progress)` cho tui_dashboard
//!   - `sync_status_router(state)` merge vào pktscan_api serve
//!   - `SyncProgress` được populate từ SyncDb + UtxoSyncDb

use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame,
};
use serde_json::{json, Value};

use crate::pkt_sync::SyncDb;
use crate::pkt_utxo_sync::UtxoSyncDb;

// ── SyncProgressPhase ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncProgressPhase {
    Idle,
    ConnectingPeer,
    DownloadingHeaders,
    ApplyingUtxo,
    Complete,
}

impl SyncProgressPhase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle               => "Idle",
            Self::ConnectingPeer     => "Connecting",
            Self::DownloadingHeaders => "Downloading Headers",
            Self::ApplyingUtxo      => "Applying UTXO",
            Self::Complete           => "Synced",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Idle | Self::ConnectingPeer => Color::DarkGray,
            Self::DownloadingHeaders          => Color::Yellow,
            Self::ApplyingUtxo               => Color::Cyan,
            Self::Complete                    => Color::Green,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::DownloadingHeaders | Self::ApplyingUtxo | Self::ConnectingPeer)
    }

    pub fn is_complete(&self) -> bool {
        *self == Self::Complete
    }
}

// ── SyncProgress ─────────────────────────────────────────────────────────────

/// Snapshot of current sync progress — read from SyncDb + UtxoSyncDb.
#[derive(Debug, Clone)]
pub struct SyncProgress {
    pub phase:               SyncProgressPhase,
    pub headers_downloaded:  u64,
    pub headers_target:      u64,  // peer's reported height
    pub utxo_height:         u64,
    pub utxo_target:         u64,  // = headers_downloaded (UTXO must catch up to headers)
    pub elapsed_secs:        u64,
    pub blocks_per_sec:      f64,
    pub peer_addr:           Option<String>,
    pub event_log:           Vec<String>,  // recent events (max 10)
}

impl SyncProgress {
    pub fn idle() -> Self {
        Self {
            phase:              SyncProgressPhase::Idle,
            headers_downloaded: 0,
            headers_target:     0,
            utxo_height:        0,
            utxo_target:        0,
            elapsed_secs:       0,
            blocks_per_sec:     0.0,
            peer_addr:          None,
            event_log:          vec!["Waiting for peer connection…".to_string()],
        }
    }

    /// Load current progress from the sync databases.
    pub fn from_dbs(sync_db: &SyncDb, utxo_db: &UtxoSyncDb) -> Self {
        let headers_downloaded = sync_db.get_sync_height().ok().flatten().unwrap_or(0);
        let utxo_height        = utxo_db.get_utxo_height().ok().flatten().unwrap_or(0);

        let phase = if headers_downloaded == 0 {
            SyncProgressPhase::Idle
        } else if utxo_height < headers_downloaded {
            SyncProgressPhase::ApplyingUtxo
        } else {
            SyncProgressPhase::Complete
        };

        Self {
            phase,
            headers_downloaded,
            headers_target:  headers_downloaded, // unknown until peer responds; use what we have
            utxo_height,
            utxo_target:     headers_downloaded,
            elapsed_secs:    0,
            blocks_per_sec:  0.0,
            peer_addr:       None,
            event_log:       vec![],
        }
    }

    // ── Progress calculations ──────────────────────────────────────────────

    /// 0.0–1.0 header download progress.
    pub fn header_progress(&self) -> f64 {
        if self.headers_target == 0 { return 0.0; }
        (self.headers_downloaded as f64 / self.headers_target as f64).min(1.0)
    }

    /// 0.0–1.0 UTXO application progress.
    pub fn utxo_progress(&self) -> f64 {
        if self.utxo_target == 0 { return 0.0; }
        (self.utxo_height as f64 / self.utxo_target as f64).min(1.0)
    }

    /// Overall progress: headers (60%) + UTXO (40%) weighted.
    pub fn overall_progress(&self) -> f64 {
        (self.header_progress() * 0.6 + self.utxo_progress() * 0.4).min(1.0)
    }

    /// Estimated seconds remaining for current phase.
    pub fn eta_secs(&self) -> Option<u64> {
        if self.blocks_per_sec <= 0.0 { return None; }
        let remaining = match &self.phase {
            SyncProgressPhase::DownloadingHeaders =>
                self.headers_target.saturating_sub(self.headers_downloaded),
            SyncProgressPhase::ApplyingUtxo =>
                self.utxo_target.saturating_sub(self.utxo_height),
            _ => return None,
        };
        Some((remaining as f64 / self.blocks_per_sec) as u64)
    }

    // ── Format helpers ─────────────────────────────────────────────────────

    pub fn format_eta(&self) -> String {
        if self.phase.is_complete() { return "synced".to_string(); }
        match self.eta_secs() {
            None    => "calculating…".to_string(),
            Some(0) => "almost done".to_string(),
            Some(s) if s < 60   => format!("{}s", s),
            Some(s) if s < 3600 => format!("{}m {}s", s / 60, s % 60),
            Some(s)             => format!("{}h {}m", s / 3600, (s % 3600) / 60),
        }
    }

    pub fn blocks_per_sec_display(&self) -> String {
        if self.blocks_per_sec <= 0.0 { return "—".to_string(); }
        if self.blocks_per_sec >= 1000.0 {
            format!("{:.1}k blk/s", self.blocks_per_sec / 1000.0)
        } else {
            format!("{:.1} blk/s", self.blocks_per_sec)
        }
    }

    pub fn header_progress_display(&self) -> String {
        format!(
            "{}/{} ({:.1}%)",
            self.headers_downloaded,
            self.headers_target,
            self.header_progress() * 100.0,
        )
    }

    pub fn utxo_progress_display(&self) -> String {
        format!(
            "{}/{} ({:.1}%)",
            self.utxo_height,
            self.utxo_target,
            self.utxo_progress() * 100.0,
        )
    }

    pub fn elapsed_display(&self) -> String {
        let s = self.elapsed_secs;
        if s < 60   { format!("{}s",  s) }
        else if s < 3600 { format!("{}m {}s", s / 60, s % 60) }
        else        { format!("{}h {}m", s / 3600, (s % 3600) / 60) }
    }
}

// ── ASCII progress bar ────────────────────────────────────────────────────────

/// Render an ASCII progress bar of given width.
///
/// Example (width=20, pct=0.45): `"████████░░░░░░░░░░░░ 45%"`
pub fn format_progress_bar(progress: f64, width: usize) -> String {
    let pct     = (progress * 100.0).round() as usize;
    let filled  = ((progress * width as f64).round() as usize).min(width);
    let empty   = width.saturating_sub(filled);
    format!(
        "{}{} {}%",
        "█".repeat(filled),
        "░".repeat(empty),
        pct,
    )
}

/// One-line CLI status string.
pub fn format_sync_oneline(p: &SyncProgress) -> String {
    format!(
        "[{}] headers={}/{} utxo={} speed={} eta={}",
        p.phase.label(),
        p.headers_downloaded,
        p.headers_target,
        p.utxo_height,
        p.blocks_per_sec_display(),
        p.format_eta(),
    )
}

// ── JSON response ─────────────────────────────────────────────────────────────

/// Build JSON payload for GET /api/testnet/sync-status.
pub fn sync_status_json(p: &SyncProgress) -> Value {
    json!({
        "phase":                p.phase.label(),
        "phase_active":         p.phase.is_active(),
        "phase_complete":       p.phase.is_complete(),
        "headers_downloaded":   p.headers_downloaded,
        "headers_target":       p.headers_target,
        "header_progress_pct":  (p.header_progress() * 100.0).round() as u64,
        "utxo_height":          p.utxo_height,
        "utxo_target":          p.utxo_target,
        "utxo_progress_pct":    (p.utxo_progress() * 100.0).round() as u64,
        "overall_progress_pct": (p.overall_progress() * 100.0).round() as u64,
        "blocks_per_sec":       p.blocks_per_sec,
        "elapsed_secs":         p.elapsed_secs,
        "eta":                  p.format_eta(),
        "peer":                 p.peer_addr,
        "progress_bar":         format_progress_bar(p.overall_progress(), 20),
    })
}

// ── Axum handler ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SyncUiState {
    pub sync_db: Arc<SyncDb>,
    pub utxo_db: Arc<UtxoSyncDb>,
}

async fn handle_sync_status(State(s): State<SyncUiState>) -> Json<Value> {
    let p = SyncProgress::from_dbs(&s.sync_db, &s.utxo_db);
    Json(sync_status_json(&p))
}

pub fn sync_status_router(state: SyncUiState) -> Router {
    Router::new()
        .route("/api/testnet/sync-status", get(handle_sync_status))
        .with_state(state)
}

// ── TUI rendering ─────────────────────────────────────────────────────────────

/// Chia area thành 3 rows: phase bar / detail / event log.
fn build_sync_panel_rows(area: Rect) -> [Rect; 3] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // overall gauge
            Constraint::Length(4), // detail lines
            Constraint::Min(3),    // event log
        ])
        .split(area);
    [chunks[0], chunks[1], chunks[2]]
}

/// Render the overall progress gauge.
fn render_overall_gauge(frame: &mut Frame, area: Rect, p: &SyncProgress) {
    let pct   = (p.overall_progress() * 100.0) as u16;
    let label = format!(
        "{} — {} — ETA {}",
        p.phase.label(),
        format_progress_bar(p.overall_progress(), 16),
        p.format_eta(),
    );
    let gauge = Gauge::default()
        .block(Block::default().title(" Testnet Sync ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(p.phase.color()).add_modifier(Modifier::BOLD))
        .percent(pct)
        .label(label);
    frame.render_widget(gauge, area);
}

/// Render header + UTXO detail lines.
fn render_sync_detail(frame: &mut Frame, area: Rect, p: &SyncProgress) {
    let h_bar  = format_progress_bar(p.header_progress(), 12);
    let u_bar  = format_progress_bar(p.utxo_progress(),  12);
    let lines  = vec![
        Line::from(vec![
            Span::styled("Headers : ", Style::default().fg(Color::DarkGray)),
            Span::styled(h_bar,        Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled(p.header_progress_display(), Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("UTXO    : ", Style::default().fg(Color::DarkGray)),
            Span::styled(u_bar,        Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled(p.utxo_progress_display(),   Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("Speed   : ", Style::default().fg(Color::DarkGray)),
            Span::styled(p.blocks_per_sec_display(),  Style::default().fg(Color::White)),
            Span::raw("   "),
            Span::styled("Elapsed : ", Style::default().fg(Color::DarkGray)),
            Span::styled(p.elapsed_display(),         Style::default().fg(Color::White)),
        ]),
    ];
    let block = Block::default().title(" Progress ").borders(Borders::ALL);
    let para  = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

/// Render recent sync events list.
fn render_event_log(frame: &mut Frame, area: Rect, p: &SyncProgress) {
    let items: Vec<ListItem> = p.event_log.iter().rev().take(8).map(|line| {
        ListItem::new(Span::styled(line.as_str(), Style::default().fg(Color::DarkGray)))
    }).collect();
    let block = Block::default().title(" Sync Log ").borders(Borders::ALL);
    let list  = List::new(items).block(block);
    frame.render_widget(list, area);
}

/// Render the full sync progress panel into `area`.
/// Call this from dashboard when testnet sync is active.
pub fn render_sync_progress_panel(frame: &mut Frame, area: Rect, progress: &SyncProgress) {
    let rows = build_sync_panel_rows(area);
    render_overall_gauge(frame, rows[0], progress);
    render_sync_detail(frame, rows[1], progress);
    render_event_log(frame, rows[2], progress);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    // ── SyncProgressPhase tests ───────────────────────────────────────────────

    #[test]
    fn test_phase_idle_label() {
        assert_eq!(SyncProgressPhase::Idle.label(), "Idle");
    }

    #[test]
    fn test_phase_downloading_label() {
        assert_eq!(SyncProgressPhase::DownloadingHeaders.label(), "Downloading Headers");
    }

    #[test]
    fn test_phase_applying_label() {
        assert_eq!(SyncProgressPhase::ApplyingUtxo.label(), "Applying UTXO");
    }

    #[test]
    fn test_phase_complete_label() {
        assert_eq!(SyncProgressPhase::Complete.label(), "Synced");
    }

    #[test]
    fn test_phase_complete_is_complete() {
        assert!(SyncProgressPhase::Complete.is_complete());
    }

    #[test]
    fn test_phase_idle_not_complete() {
        assert!(!SyncProgressPhase::Idle.is_complete());
    }

    #[test]
    fn test_phase_downloading_is_active() {
        assert!(SyncProgressPhase::DownloadingHeaders.is_active());
    }

    #[test]
    fn test_phase_idle_not_active() {
        assert!(!SyncProgressPhase::Idle.is_active());
    }

    #[test]
    fn test_phase_complete_not_active() {
        assert!(!SyncProgressPhase::Complete.is_active());
    }

    #[test]
    fn test_phase_applying_is_active() {
        assert!(SyncProgressPhase::ApplyingUtxo.is_active());
    }

    // ── SyncProgress::idle tests ──────────────────────────────────────────────

    #[test]
    fn test_idle_progress_zero_headers() {
        let p = SyncProgress::idle();
        assert_eq!(p.headers_downloaded, 0);
    }

    #[test]
    fn test_idle_header_progress_zero() {
        assert_eq!(SyncProgress::idle().header_progress(), 0.0);
    }

    #[test]
    fn test_idle_utxo_progress_zero() {
        assert_eq!(SyncProgress::idle().utxo_progress(), 0.0);
    }

    #[test]
    fn test_idle_overall_progress_zero() {
        assert_eq!(SyncProgress::idle().overall_progress(), 0.0);
    }

    // ── progress calculation tests ────────────────────────────────────────────

    #[test]
    fn test_header_progress_half() {
        let mut p = SyncProgress::idle();
        p.headers_downloaded = 500;
        p.headers_target     = 1000;
        assert!((p.header_progress() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_header_progress_full() {
        let mut p = SyncProgress::idle();
        p.headers_downloaded = 1000;
        p.headers_target     = 1000;
        assert_eq!(p.header_progress(), 1.0);
    }

    #[test]
    fn test_header_progress_capped_at_one() {
        let mut p = SyncProgress::idle();
        p.headers_downloaded = 2000;
        p.headers_target     = 1000;
        assert_eq!(p.header_progress(), 1.0);
    }

    #[test]
    fn test_utxo_progress_three_quarters() {
        let mut p = SyncProgress::idle();
        p.utxo_height = 750;
        p.utxo_target = 1000;
        assert!((p.utxo_progress() - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_overall_progress_weighted() {
        let mut p = SyncProgress::idle();
        p.headers_downloaded = 1000;
        p.headers_target     = 1000;  // header 100% → 0.6
        p.utxo_height        = 500;
        p.utxo_target        = 1000;  // utxo 50%   → 0.4 * 0.5 = 0.2
        // overall = 0.6 + 0.2 = 0.8
        assert!((p.overall_progress() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_overall_progress_capped_at_one() {
        let mut p = SyncProgress::idle();
        p.headers_downloaded = 1000;
        p.headers_target     = 1000;
        p.utxo_height        = 1000;
        p.utxo_target        = 1000;
        assert_eq!(p.overall_progress(), 1.0);
    }

    // ── eta_secs tests ────────────────────────────────────────────────────────

    #[test]
    fn test_eta_none_when_no_speed() {
        let mut p    = SyncProgress::idle();
        p.phase      = SyncProgressPhase::DownloadingHeaders;
        p.blocks_per_sec = 0.0;
        assert!(p.eta_secs().is_none());
    }

    #[test]
    fn test_eta_calculated_correctly() {
        let mut p = SyncProgress::idle();
        p.phase              = SyncProgressPhase::DownloadingHeaders;
        p.headers_downloaded = 0;
        p.headers_target     = 1000;
        p.blocks_per_sec     = 100.0; // 1000 / 100 = 10s
        assert_eq!(p.eta_secs(), Some(10));
    }

    #[test]
    fn test_eta_zero_when_done() {
        let mut p = SyncProgress::idle();
        p.phase              = SyncProgressPhase::DownloadingHeaders;
        p.headers_downloaded = 1000;
        p.headers_target     = 1000;
        p.blocks_per_sec     = 100.0;
        assert_eq!(p.eta_secs(), Some(0));
    }

    #[test]
    fn test_eta_none_for_idle_phase() {
        let mut p = SyncProgress::idle();
        p.blocks_per_sec = 100.0;
        assert!(p.eta_secs().is_none()); // Idle phase → None
    }

    // ── format_eta tests ──────────────────────────────────────────────────────

    #[test]
    fn test_format_eta_synced() {
        let mut p = SyncProgress::idle();
        p.phase = SyncProgressPhase::Complete;
        assert_eq!(p.format_eta(), "synced");
    }

    #[test]
    fn test_format_eta_calculating() {
        let p = SyncProgress::idle(); // Idle, no speed
        assert_eq!(p.format_eta(), "calculating…");
    }

    #[test]
    fn test_format_eta_seconds() {
        let mut p = SyncProgress::idle();
        p.phase              = SyncProgressPhase::DownloadingHeaders;
        p.headers_target     = 100;
        p.blocks_per_sec     = 10.0; // eta=10s
        assert_eq!(p.format_eta(), "10s");
    }

    #[test]
    fn test_format_eta_minutes() {
        let mut p = SyncProgress::idle();
        p.phase          = SyncProgressPhase::DownloadingHeaders;
        p.headers_target = 9000;
        p.blocks_per_sec = 100.0; // 90s
        assert_eq!(p.format_eta(), "1m 30s");
    }

    #[test]
    fn test_format_eta_hours() {
        let mut p = SyncProgress::idle();
        p.phase          = SyncProgressPhase::DownloadingHeaders;
        p.headers_target = 720_000;
        p.blocks_per_sec = 100.0; // 7200s = 2h
        assert_eq!(p.format_eta(), "2h 0m");
    }

    // ── blocks_per_sec_display tests ──────────────────────────────────────────

    #[test]
    fn test_bps_display_dash_when_zero() {
        let p = SyncProgress::idle();
        assert_eq!(p.blocks_per_sec_display(), "—");
    }

    #[test]
    fn test_bps_display_normal() {
        let mut p = SyncProgress::idle();
        p.blocks_per_sec = 42.5;
        assert_eq!(p.blocks_per_sec_display(), "42.5 blk/s");
    }

    #[test]
    fn test_bps_display_kilo() {
        let mut p = SyncProgress::idle();
        p.blocks_per_sec = 1500.0;
        assert_eq!(p.blocks_per_sec_display(), "1.5k blk/s");
    }

    // ── elapsed_display tests ─────────────────────────────────────────────────

    #[test]
    fn test_elapsed_seconds() {
        let mut p = SyncProgress::idle();
        p.elapsed_secs = 45;
        assert_eq!(p.elapsed_display(), "45s");
    }

    #[test]
    fn test_elapsed_minutes() {
        let mut p = SyncProgress::idle();
        p.elapsed_secs = 150; // 2m 30s
        assert_eq!(p.elapsed_display(), "2m 30s");
    }

    #[test]
    fn test_elapsed_hours() {
        let mut p = SyncProgress::idle();
        p.elapsed_secs = 7260; // 2h 1m
        assert_eq!(p.elapsed_display(), "2h 1m");
    }

    // ── format_progress_bar tests ─────────────────────────────────────────────

    #[test]
    fn test_progress_bar_zero() {
        let s = format_progress_bar(0.0, 10);
        assert!(s.contains("░░░░░░░░░░"));
        assert!(s.contains("0%"));
    }

    #[test]
    fn test_progress_bar_full() {
        let s = format_progress_bar(1.0, 10);
        assert!(s.contains("██████████"));
        assert!(s.contains("100%"));
    }

    #[test]
    fn test_progress_bar_half() {
        let s = format_progress_bar(0.5, 10);
        assert!(s.contains("50%"));
        assert!(s.contains("█████"));
    }

    #[test]
    fn test_progress_bar_width() {
        let s = format_progress_bar(0.3, 20);
        // filled(6) + empty(14) = 20 chars of block symbols
        let filled_count = s.chars().filter(|&c| c == '█').count();
        let empty_count  = s.chars().filter(|&c| c == '░').count();
        assert_eq!(filled_count + empty_count, 20);
    }

    // ── format_sync_oneline tests ─────────────────────────────────────────────

    #[test]
    fn test_oneline_contains_phase() {
        let mut p = SyncProgress::idle();
        p.phase = SyncProgressPhase::DownloadingHeaders;
        let s = format_sync_oneline(&p);
        assert!(s.contains("Downloading Headers"));
    }

    #[test]
    fn test_oneline_contains_heights() {
        let mut p = SyncProgress::idle();
        p.headers_downloaded = 500;
        p.headers_target     = 1000;
        let s = format_sync_oneline(&p);
        assert!(s.contains("500/1000"));
    }

    // ── sync_status_json tests ────────────────────────────────────────────────

    #[test]
    fn test_json_has_phase() {
        let p = SyncProgress::idle();
        let v = sync_status_json(&p);
        assert!(v["phase"].as_str().is_some());
    }

    #[test]
    fn test_json_has_progress_pct() {
        let p = SyncProgress::idle();
        let v = sync_status_json(&p);
        assert!(v["overall_progress_pct"].as_u64().is_some());
    }

    #[test]
    fn test_json_has_progress_bar() {
        let p = SyncProgress::idle();
        let v = sync_status_json(&p);
        assert!(v["progress_bar"].as_str().is_some());
    }

    #[test]
    fn test_json_complete_100_pct() {
        let mut p = SyncProgress::idle();
        p.phase              = SyncProgressPhase::Complete;
        p.headers_downloaded = 1000;
        p.headers_target     = 1000;
        p.utxo_height        = 1000;
        p.utxo_target        = 1000;
        let v = sync_status_json(&p);
        assert_eq!(v["overall_progress_pct"].as_u64(), Some(100));
        assert_eq!(v["phase_complete"].as_bool(), Some(true));
    }

    #[test]
    fn test_json_eta_string() {
        let p = SyncProgress::idle();
        let v = sync_status_json(&p);
        assert!(v["eta"].as_str().is_some());
    }

    // ── from_dbs tests ────────────────────────────────────────────────────────
    // Serialized via DB_OPEN_LOCK because SyncDb::open_temp() uses SystemTime hash
    // — two parallel threads at the same nanosecond collide on the same path.

    static DB_OPEN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_from_dbs_idle_when_empty() {
        let _g  = DB_OPEN_LOCK.lock().unwrap();
        let sdb = crate::pkt_sync::SyncDb::open_temp().unwrap();
        let udb = crate::pkt_utxo_sync::UtxoSyncDb::open_temp().unwrap();
        let p   = SyncProgress::from_dbs(&sdb, &udb);
        assert_eq!(p.phase, SyncProgressPhase::Idle);
        assert_eq!(p.headers_downloaded, 0);
    }

    #[test]
    fn test_from_dbs_complete_when_heights_match() {
        let _g  = DB_OPEN_LOCK.lock().unwrap();
        let sdb = crate::pkt_sync::SyncDb::open_temp().unwrap();
        let udb = crate::pkt_utxo_sync::UtxoSyncDb::open_temp().unwrap();
        sdb.set_sync_height(100).unwrap();
        udb.set_utxo_height(100).unwrap();
        let p = SyncProgress::from_dbs(&sdb, &udb);
        assert_eq!(p.phase, SyncProgressPhase::Complete);
    }

    #[test]
    fn test_from_dbs_applying_when_utxo_behind() {
        let _g  = DB_OPEN_LOCK.lock().unwrap();
        let sdb = crate::pkt_sync::SyncDb::open_temp().unwrap();
        let udb = crate::pkt_utxo_sync::UtxoSyncDb::open_temp().unwrap();
        sdb.set_sync_height(100).unwrap();
        udb.set_utxo_height(50).unwrap();
        let p = SyncProgress::from_dbs(&sdb, &udb);
        assert_eq!(p.phase, SyncProgressPhase::ApplyingUtxo);
    }

    #[test]
    fn test_from_dbs_headers_downloaded_populated() {
        let _g  = DB_OPEN_LOCK.lock().unwrap();
        let sdb = crate::pkt_sync::SyncDb::open_temp().unwrap();
        let udb = crate::pkt_utxo_sync::UtxoSyncDb::open_temp().unwrap();
        sdb.set_sync_height(42).unwrap();
        let p = SyncProgress::from_dbs(&sdb, &udb);
        assert_eq!(p.headers_downloaded, 42);
    }

    // ── TUI render tests (TestBackend — no terminal needed) ───────────────────

    fn test_terminal(w: u16, h: u16) -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(w, h)).unwrap()
    }

    #[test]
    fn test_render_sync_panel_idle_no_panic() {
        let mut term = test_terminal(80, 20);
        let p = SyncProgress::idle();
        term.draw(|f| {
            render_sync_progress_panel(f, f.size(), &p);
        }).unwrap();
    }

    #[test]
    fn test_render_sync_panel_downloading_no_panic() {
        let mut term = test_terminal(80, 20);
        let mut p    = SyncProgress::idle();
        p.phase              = SyncProgressPhase::DownloadingHeaders;
        p.headers_downloaded = 5000;
        p.headers_target     = 10000;
        p.blocks_per_sec     = 200.0;
        p.elapsed_secs       = 25;
        term.draw(|f| {
            render_sync_progress_panel(f, f.size(), &p);
        }).unwrap();
    }

    #[test]
    fn test_render_sync_panel_complete_no_panic() {
        let mut term = test_terminal(80, 20);
        let mut p    = SyncProgress::idle();
        p.phase              = SyncProgressPhase::Complete;
        p.headers_downloaded = 1000;
        p.headers_target     = 1000;
        p.utxo_height        = 1000;
        p.utxo_target        = 1000;
        term.draw(|f| {
            render_sync_progress_panel(f, f.size(), &p);
        }).unwrap();
    }

    #[test]
    fn test_render_sync_panel_with_log_no_panic() {
        let mut term = test_terminal(100, 30);
        let mut p    = SyncProgress::idle();
        p.event_log  = (0..10).map(|i| format!("event {}", i)).collect();
        term.draw(|f| {
            render_sync_progress_panel(f, f.size(), &p);
        }).unwrap();
    }

    #[test]
    fn test_render_sync_panel_tiny_terminal_no_panic() {
        let mut term = test_terminal(40, 12);
        let p = SyncProgress::idle();
        term.draw(|f| {
            render_sync_progress_panel(f, f.size(), &p);
        }).unwrap();
    }
}
