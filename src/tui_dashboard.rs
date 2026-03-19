#![allow(dead_code)]
//! v14.0 — Terminal UI Dashboard
//!
//! Dashboard real-time cho PKT node dùng ratatui:
//!   - Block height + sync status
//!   - Hashrate (PacketCrypt PoW)
//!   - Peer count
//!   - Mempool depth + bytes
//!   - Uptime + network name
//!
//! Dùng TestBackend trong tests — không cần terminal thật.
//!
//! Chạy: `cargo run -- dashboard`

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};

// ── Dashboard state ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Syncing { current: u64, target: u64 },
    Synced,
    NoPeers,
}

impl SyncStatus {
    pub fn label(&self) -> &'static str {
        match self {
            SyncStatus::Syncing { .. } => "SYNCING",
            SyncStatus::Synced        => "SYNCED",
            SyncStatus::NoPeers       => "NO PEERS",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            SyncStatus::Syncing { .. } => Color::Yellow,
            SyncStatus::Synced        => Color::Green,
            SyncStatus::NoPeers       => Color::Red,
        }
    }

    /// 0.0–1.0 cho progress bar (1.0 = synced)
    pub fn progress(&self) -> f64 {
        match self {
            SyncStatus::Synced => 1.0,
            SyncStatus::NoPeers => 0.0,
            SyncStatus::Syncing { current, target } => {
                if *target == 0 { return 0.0; }
                (*current as f64 / *target as f64).min(1.0)
            }
        }
    }
}

/// Toàn bộ state hiển thị trên dashboard
#[derive(Debug, Clone)]
pub struct DashboardState {
    pub network:        String,
    pub block_height:   u64,
    pub best_hash:      String,     // hex prefix 16 chars
    pub sync_status:    SyncStatus,
    /// Hashrate tính bằng kH/s (PacketCrypt announcement mining)
    pub hashrate_khs:   f64,
    pub ann_count:      u64,        // announcements trong mempool
    pub peer_count:     usize,
    pub peer_list:      Vec<String>,
    pub mempool_count:  usize,
    pub mempool_bytes:  u64,
    pub uptime_secs:    u64,
    pub steward_addr:   String,
    pub log_lines:      Vec<String>,
}

impl DashboardState {
    pub fn new(network: &str) -> Self {
        DashboardState {
            network:       network.to_string(),
            block_height:  0,
            best_hash:     "0".repeat(16),
            sync_status:   SyncStatus::NoPeers,
            hashrate_khs:  0.0,
            ann_count:     0,
            peer_count:    0,
            peer_list:     Vec::new(),
            mempool_count: 0,
            mempool_bytes: 0,
            uptime_secs:   0,
            steward_addr:  "pkt1steward0000000000000000000000000".to_string(),
            log_lines:     Vec::new(),
        }
    }

    /// Cập nhật hashrate
    pub fn set_hashrate(&mut self, khs: f64) {
        self.hashrate_khs = khs.max(0.0);
    }

    /// Thêm peer
    pub fn add_peer(&mut self, addr: String) {
        if !self.peer_list.contains(&addr) {
            self.peer_list.push(addr);
            self.peer_count = self.peer_list.len();
        }
        self.update_sync_status();
    }

    /// Xóa peer
    pub fn remove_peer(&mut self, addr: &str) {
        self.peer_list.retain(|p| p != addr);
        self.peer_count = self.peer_list.len();
        self.update_sync_status();
    }

    fn update_sync_status(&mut self) {
        if self.peer_count == 0 {
            self.sync_status = SyncStatus::NoPeers;
        }
    }

    /// Thêm dòng log (giữ tối đa 100 dòng)
    pub fn push_log(&mut self, line: String) {
        self.log_lines.push(line);
        if self.log_lines.len() > 100 {
            self.log_lines.remove(0);
        }
    }

    /// Hashrate string dễ đọc
    pub fn hashrate_display(&self) -> String {
        let khs = self.hashrate_khs;
        if khs >= 1_000_000.0 {
            format!("{:.2} GH/s", khs / 1_000_000.0)
        } else if khs >= 1_000.0 {
            format!("{:.2} MH/s", khs / 1_000.0)
        } else {
            format!("{:.2} kH/s", khs)
        }
    }

    /// Mempool size string dễ đọc
    pub fn mempool_bytes_display(&self) -> String {
        let b = self.mempool_bytes;
        if b >= 1_048_576 {
            format!("{:.1} MB", b as f64 / 1_048_576.0)
        } else if b >= 1_024 {
            format!("{:.1} KB", b as f64 / 1_024.0)
        } else {
            format!("{} B", b)
        }
    }

    /// Uptime string
    pub fn uptime_display(&self) -> String {
        let s = self.uptime_secs;
        let h = s / 3600;
        let m = (s % 3600) / 60;
        let sc = s % 60;
        format!("{:02}:{:02}:{:02}", h, m, sc)
    }

    /// Sync progress percent string
    pub fn sync_pct_display(&self) -> String {
        format!("{:.1}%", self.sync_status.progress() * 100.0)
    }
}

// ── Layout helpers ──────────────────────────────────────────────────────────

/// Chia terminal thành 3 phần dọc: header / body / log
pub fn build_layout(area: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // header
            Constraint::Min(10),     // body
            Constraint::Length(6),   // log panel
        ])
        .split(area);
    (chunks[0], chunks[1], chunks[2])
}

/// Chia body thành 2 cột: left (stats) / right (peers)
pub fn build_body_columns(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(area);
    (chunks[0], chunks[1])
}

/// Chia cột trái thành: block info / mining / mempool / sync bar
pub fn build_left_rows(area: Rect) -> [Rect; 4] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);
    [chunks[0], chunks[1], chunks[2], chunks[3]]
}

// ── Render functions ────────────────────────────────────────────────────────

/// Render toàn bộ dashboard vào frame
pub fn render_dashboard(frame: &mut Frame, state: &DashboardState) {
    let area = frame.size();
    let (header_area, body_area, log_area) = build_layout(area);
    let (left_area, right_area) = build_body_columns(body_area);
    let left_rows = build_left_rows(left_area);

    render_header(frame, header_area, state);
    render_block_info(frame, left_rows[0], state);
    render_mining(frame, left_rows[1], state);
    render_mempool(frame, left_rows[2], state);
    render_sync_bar(frame, left_rows[3], state);
    render_peers(frame, right_area, state);
    render_log(frame, log_area, state);
}

fn render_header(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let title = format!(
        " PKT Node Dashboard — {} | uptime {} ",
        state.network.to_uppercase(),
        state.uptime_display()
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    let status_span = Span::styled(
        format!("  {}  ", state.sync_status.label()),
        Style::default()
            .fg(Color::Black)
            .bg(state.sync_status.color())
            .add_modifier(Modifier::BOLD),
    );
    let para = Paragraph::new(Line::from(vec![status_span]))
        .block(block)
        .alignment(Alignment::Left);
    frame.render_widget(para, area);
}

fn render_block_info(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let lines = vec![
        Line::from(vec![
            Span::styled("Height : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", state.block_height),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Hash   : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}…", &state.best_hash[..state.best_hash.len().min(16)]),
                Style::default().fg(Color::Gray),
            ),
        ]),
    ];
    let block = Block::default().title(" Block ").borders(Borders::ALL);
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

fn render_mining(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let line = Line::from(vec![
        Span::styled("Hashrate : ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            state.hashrate_display(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("Anns : ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", state.ann_count),
            Style::default().fg(Color::Yellow),
        ),
    ]);
    let block = Block::default().title(" Mining ").borders(Borders::ALL);
    let para = Paragraph::new(line).block(block);
    frame.render_widget(para, area);
}

fn render_mempool(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let line = Line::from(vec![
        Span::styled("TXs : ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", state.mempool_count),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("Size : ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            state.mempool_bytes_display(),
            Style::default().fg(Color::Magenta),
        ),
    ]);
    let block = Block::default().title(" Mempool ").borders(Borders::ALL);
    let para = Paragraph::new(line).block(block);
    frame.render_widget(para, area);
}

fn render_sync_bar(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let pct = (state.sync_status.progress() * 100.0) as u16;
    let label = match &state.sync_status {
        SyncStatus::Syncing { current, target } =>
            format!("Syncing {}/{} ({})", current, target, state.sync_pct_display()),
        SyncStatus::Synced  => "Synced".to_string(),
        SyncStatus::NoPeers => "No Peers".to_string(),
    };
    let gauge = Gauge::default()
        .block(Block::default().title(" Sync ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(state.sync_status.color()))
        .percent(pct)
        .label(label);
    frame.render_widget(gauge, area);
}

fn render_peers(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let title = format!(" Peers ({}) ", state.peer_count);
    let items: Vec<ListItem> = state
        .peer_list
        .iter()
        .map(|p| ListItem::new(Span::styled(p.as_str(), Style::default().fg(Color::Cyan))))
        .collect();
    let block = Block::default().title(title).borders(Borders::ALL);
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_log(frame: &mut Frame, area: Rect, state: &DashboardState) {
    // Chỉ hiển thị 4 dòng cuối
    let max_lines = (area.height as usize).saturating_sub(2);
    let start = state.log_lines.len().saturating_sub(max_lines);
    let lines: Vec<Line> = state.log_lines[start..]
        .iter()
        .map(|l| Line::from(Span::styled(l.as_str(), Style::default().fg(Color::Gray))))
        .collect();
    let block = Block::default().title(" Log ").borders(Borders::ALL);
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

// ── Dashboard events ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DashboardEvent {
    Tick,
    Quit,
    Resize(u16, u16),
}

/// Parse crossterm event → DashboardEvent (None = bỏ qua)
pub fn parse_event(ev: crossterm::event::Event) -> Option<DashboardEvent> {
    use crossterm::event::{Event, KeyCode, KeyEvent};
    match ev {
        Event::Key(KeyEvent { code: KeyCode::Char('q'), .. })
        | Event::Key(KeyEvent { code: KeyCode::Char('Q'), .. })
        | Event::Key(KeyEvent { code: KeyCode::Esc,     .. }) => Some(DashboardEvent::Quit),
        Event::Resize(w, h) => Some(DashboardEvent::Resize(w, h)),
        _ => None,
    }
}

// ── cmd_dashboard (entrypoint) ──────────────────────────────────────────────

/// `cargo run -- dashboard` — chạy TUI live.
/// Dùng mock state để demo; node thật sẽ truyền state qua channel.
pub fn cmd_dashboard() {
    use crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};
    use std::time::{Duration, Instant};

    enable_raw_mode().expect("enable raw mode");
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("enter alternate screen");
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("create terminal");

    let mut state = DashboardState::new("testnet");
    state.sync_status = SyncStatus::Syncing { current: 0, target: 100_000 };
    state.add_peer("192.168.1.10:64765".to_string());
    state.add_peer("10.0.0.5:64765".to_string());
    state.push_log("[INFO] PKT node started".to_string());
    state.push_log("[INFO] Connecting to testnet peers...".to_string());

    let tick = Duration::from_millis(500);
    let mut last_tick = Instant::now();
    let mut tick_count: u64 = 0;

    loop {
        terminal.draw(|f| render_dashboard(f, &state)).expect("draw");

        let timeout = tick.checked_sub(last_tick.elapsed()).unwrap_or_default();
        if crossterm::event::poll(timeout).unwrap_or(false) {
            if let Ok(ev) = crossterm::event::read() {
                if let Some(DashboardEvent::Quit) = parse_event(ev) {
                    break;
                }
            }
        }
        if last_tick.elapsed() >= tick {
            // Simulate live updates
            tick_count += 1;
            state.uptime_secs += 1;
            state.set_hashrate(1_234.5 + (tick_count % 100) as f64 * 10.0);
            if tick_count % 5 == 0 && state.block_height < 100_000 {
                state.block_height += 1;
                if let SyncStatus::Syncing { ref mut current, .. } = state.sync_status {
                    *current = state.block_height;
                }
                if state.block_height >= 100_000 {
                    state.sync_status = SyncStatus::Synced;
                }
            }
            state.ann_count = tick_count * 3 % 500;
            state.mempool_count = (tick_count % 50) as usize;
            state.mempool_bytes = state.mempool_count as u64 * 250;
            last_tick = Instant::now();
        }
    }

    disable_raw_mode().expect("disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen).expect("leave alternate screen");
    terminal.show_cursor().expect("show cursor");
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn test_terminal(w: u16, h: u16) -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(w, h)).unwrap()
    }

    // ── DashboardState ────────────────────────────────────────────────────

    #[test]
    fn test_new_state_defaults() {
        let s = DashboardState::new("testnet");
        assert_eq!(s.network, "testnet");
        assert_eq!(s.block_height, 0);
        assert_eq!(s.peer_count, 0);
        assert_eq!(s.sync_status, SyncStatus::NoPeers);
    }

    #[test]
    fn test_add_peer_increments_count() {
        let mut s = DashboardState::new("testnet");
        s.add_peer("1.2.3.4:64765".to_string());
        assert_eq!(s.peer_count, 1);
        s.add_peer("5.6.7.8:64765".to_string());
        assert_eq!(s.peer_count, 2);
    }

    #[test]
    fn test_add_peer_no_duplicate() {
        let mut s = DashboardState::new("testnet");
        s.add_peer("1.2.3.4:64765".to_string());
        s.add_peer("1.2.3.4:64765".to_string());
        assert_eq!(s.peer_count, 1);
    }

    #[test]
    fn test_remove_peer_decrements_count() {
        let mut s = DashboardState::new("testnet");
        s.add_peer("1.2.3.4:64765".to_string());
        s.remove_peer("1.2.3.4:64765");
        assert_eq!(s.peer_count, 0);
    }

    #[test]
    fn test_no_peers_sets_no_peers_status() {
        let mut s = DashboardState::new("testnet");
        s.add_peer("1.2.3.4:64765".to_string());
        s.remove_peer("1.2.3.4:64765");
        assert_eq!(s.sync_status, SyncStatus::NoPeers);
    }

    #[test]
    fn test_push_log_appends() {
        let mut s = DashboardState::new("testnet");
        s.push_log("line1".to_string());
        s.push_log("line2".to_string());
        assert_eq!(s.log_lines.len(), 2);
        assert_eq!(s.log_lines[1], "line2");
    }

    #[test]
    fn test_push_log_capped_at_100() {
        let mut s = DashboardState::new("testnet");
        for i in 0..110 {
            s.push_log(format!("line {}", i));
        }
        assert_eq!(s.log_lines.len(), 100);
        assert_eq!(s.log_lines[0], "line 10");
    }

    #[test]
    fn test_set_hashrate_negative_clamped() {
        let mut s = DashboardState::new("testnet");
        s.set_hashrate(-100.0);
        assert_eq!(s.hashrate_khs, 0.0);
    }

    // ── Hashrate display ──────────────────────────────────────────────────

    #[test]
    fn test_hashrate_display_khs() {
        let mut s = DashboardState::new("testnet");
        s.set_hashrate(500.0);
        assert_eq!(s.hashrate_display(), "500.00 kH/s");
    }

    #[test]
    fn test_hashrate_display_mhs() {
        let mut s = DashboardState::new("testnet");
        s.set_hashrate(2_500.0);
        assert_eq!(s.hashrate_display(), "2.50 MH/s");
    }

    #[test]
    fn test_hashrate_display_ghs() {
        let mut s = DashboardState::new("testnet");
        s.set_hashrate(3_000_000.0);
        assert_eq!(s.hashrate_display(), "3.00 GH/s");
    }

    // ── Mempool display ───────────────────────────────────────────────────

    #[test]
    fn test_mempool_display_bytes() {
        let mut s = DashboardState::new("testnet");
        s.mempool_bytes = 512;
        assert_eq!(s.mempool_bytes_display(), "512 B");
    }

    #[test]
    fn test_mempool_display_kb() {
        let mut s = DashboardState::new("testnet");
        s.mempool_bytes = 2_048;
        assert_eq!(s.mempool_bytes_display(), "2.0 KB");
    }

    #[test]
    fn test_mempool_display_mb() {
        let mut s = DashboardState::new("testnet");
        s.mempool_bytes = 2_097_152;
        assert_eq!(s.mempool_bytes_display(), "2.0 MB");
    }

    // ── Uptime display ────────────────────────────────────────────────────

    #[test]
    fn test_uptime_display_zero() {
        let s = DashboardState::new("testnet");
        assert_eq!(s.uptime_display(), "00:00:00");
    }

    #[test]
    fn test_uptime_display_hms() {
        let mut s = DashboardState::new("testnet");
        s.uptime_secs = 3_661; // 1h 1m 1s
        assert_eq!(s.uptime_display(), "01:01:01");
    }

    // ── SyncStatus ────────────────────────────────────────────────────────

    #[test]
    fn test_sync_progress_synced_is_1() {
        assert!((SyncStatus::Synced.progress() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_sync_progress_no_peers_is_0() {
        assert_eq!(SyncStatus::NoPeers.progress(), 0.0);
    }

    #[test]
    fn test_sync_progress_half() {
        let s = SyncStatus::Syncing { current: 50_000, target: 100_000 };
        assert!((s.progress() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_sync_progress_zero_target() {
        let s = SyncStatus::Syncing { current: 0, target: 0 };
        assert_eq!(s.progress(), 0.0);
    }

    #[test]
    fn test_sync_pct_display() {
        let mut s = DashboardState::new("testnet");
        s.sync_status = SyncStatus::Syncing { current: 75_000, target: 100_000 };
        assert_eq!(s.sync_pct_display(), "75.0%");
    }

    #[test]
    fn test_sync_status_labels() {
        assert_eq!(SyncStatus::Synced.label(), "SYNCED");
        assert_eq!(SyncStatus::NoPeers.label(), "NO PEERS");
        assert_eq!(SyncStatus::Syncing { current: 0, target: 1 }.label(), "SYNCING");
    }

    // ── Layout ───────────────────────────────────────────────────────────

    #[test]
    fn test_build_layout_areas_non_zero() {
        let area = Rect::new(0, 0, 120, 40);
        let (header, body, log) = build_layout(area);
        assert!(header.height > 0);
        assert!(body.height > 0);
        assert!(log.height > 0);
        assert_eq!(header.height + body.height + log.height, area.height);
    }

    #[test]
    fn test_build_body_columns_widths_sum() {
        let area = Rect::new(0, 0, 120, 30);
        let (left, right) = build_body_columns(area);
        assert_eq!(left.width + right.width, area.width);
    }

    #[test]
    fn test_build_left_rows_heights() {
        let area = Rect::new(0, 0, 80, 20);
        let rows = build_left_rows(area);
        let total: u16 = rows.iter().map(|r| r.height).sum();
        assert_eq!(total, rows[0].height + rows[1].height + rows[2].height + rows[3].height);
    }

    // ── Render smoke tests (TestBackend — không cần terminal thật) ─────────

    #[test]
    fn test_render_dashboard_does_not_panic() {
        let mut terminal = test_terminal(120, 40);
        let state = DashboardState::new("testnet");
        terminal.draw(|f| render_dashboard(f, &state)).unwrap();
    }

    #[test]
    fn test_render_with_peers_and_logs() {
        let mut terminal = test_terminal(120, 40);
        let mut state = DashboardState::new("mainnet");
        state.add_peer("192.168.1.1:64764".to_string());
        state.block_height = 500_000;
        state.set_hashrate(12_345.0);
        state.mempool_count = 42;
        state.mempool_bytes = 10_240;
        state.sync_status = SyncStatus::Synced;
        state.push_log("[INFO] block 500000 accepted".to_string());
        terminal.draw(|f| render_dashboard(f, &state)).unwrap();
    }

    #[test]
    fn test_render_syncing_state() {
        let mut terminal = test_terminal(120, 40);
        let mut state = DashboardState::new("testnet");
        state.sync_status = SyncStatus::Syncing { current: 30_000, target: 100_000 };
        terminal.draw(|f| render_dashboard(f, &state)).unwrap();
    }

    #[test]
    fn test_render_small_terminal() {
        let mut terminal = test_terminal(40, 20);
        let state = DashboardState::new("regtest");
        terminal.draw(|f| render_dashboard(f, &state)).unwrap();
    }

    // ── parse_event ───────────────────────────────────────────────────────

    #[test]
    fn test_parse_quit_key_q() {
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let ev = Event::Key(KeyEvent {
            code:      KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind:      KeyEventKind::Press,
            state:     KeyEventState::NONE,
        });
        assert_eq!(parse_event(ev), Some(DashboardEvent::Quit));
    }

    #[test]
    fn test_parse_resize_event() {
        use crossterm::event::Event;
        let ev = Event::Resize(100, 50);
        assert_eq!(parse_event(ev), Some(DashboardEvent::Resize(100, 50)));
    }

    #[test]
    fn test_parse_unknown_event_returns_none() {
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let ev = Event::Key(KeyEvent {
            code:      KeyCode::Char('x'),
            modifiers: KeyModifiers::NONE,
            kind:      KeyEventKind::Press,
            state:     KeyEventState::NONE,
        });
        assert_eq!(parse_event(ev), None);
    }
}
