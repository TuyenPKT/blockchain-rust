#![allow(dead_code)]
//! v14.1 — Wallet TUI
//!
//! Interactive wallet trong terminal dùng ratatui:
//!   - Tab Balance: địa chỉ + số dư PKT/paklets
//!   - Tab Send: nhập địa chỉ nhận + amount → màn hình xác nhận trước khi ký
//!   - Tab Receive: hiển thị địa chỉ nhận
//!   - Tab History: danh sách giao dịch gần nhất
//!
//! State machine: screen tab + send flow (Input → Confirm → Done/Cancel)

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};

// ── Constants ───────────────────────────────────────────────────────────────

pub const PAKLETS_PER_PKT: u64 = 1_073_741_824; // 2^30 — giống pkt_genesis

// ── Screen / tab ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletTab { Balance, Send, Receive, History }

impl WalletTab {
    pub fn all() -> &'static [WalletTab] {
        &[WalletTab::Balance, WalletTab::Send, WalletTab::Receive, WalletTab::History]
    }

    pub fn label(&self) -> &'static str {
        match self {
            WalletTab::Balance  => "Balance",
            WalletTab::Send     => "Send",
            WalletTab::Receive  => "Receive",
            WalletTab::History  => "History",
        }
    }

    pub fn index(&self) -> usize {
        WalletTab::all().iter().position(|t| t == self).unwrap_or(0)
    }

    pub fn next(&self) -> WalletTab {
        let tabs = WalletTab::all();
        let i = (self.index() + 1) % tabs.len();
        tabs[i].clone()
    }

    pub fn prev(&self) -> WalletTab {
        let tabs = WalletTab::all();
        let i = (self.index() + tabs.len() - 1) % tabs.len();
        tabs[i].clone()
    }
}

// ── Send flow ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendField { Recipient, Amount }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendStep {
    /// Đang nhập thông tin
    Input { active_field: SendField },
    /// Màn hình xác nhận — chờ Enter hoặc Esc
    Confirm,
    /// Đã ký và broadcast thành công
    Done { txid: String },
    /// Người dùng hủy
    Cancelled,
}

/// Lỗi validate khi send
#[derive(Debug, Clone, PartialEq)]
pub enum SendError {
    EmptyRecipient,
    InvalidRecipient,
    EmptyAmount,
    InvalidAmount,
    InsufficientFunds { have: u64, need: u64 },
    ZeroAmount,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::EmptyRecipient      => write!(f, "Địa chỉ nhận không được trống"),
            SendError::InvalidRecipient    => write!(f, "Địa chỉ không hợp lệ (cần bắt đầu bằng pkt1/tpkt1/rpkt1)"),
            SendError::EmptyAmount         => write!(f, "Số lượng không được trống"),
            SendError::InvalidAmount       => write!(f, "Số lượng không hợp lệ"),
            SendError::ZeroAmount          => write!(f, "Số lượng phải > 0"),
            SendError::InsufficientFunds { have, need } =>
                write!(f, "Không đủ funds: có {} PKT, cần {} PKT",
                    paklets_to_pkt_display(*have), paklets_to_pkt_display(*need)),
        }
    }
}

// ── Transaction history entry ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TxHistoryEntry {
    pub txid:      String,
    pub direction: TxDirection,
    pub amount_paklets: u64,
    pub counterpart: String,   // địa chỉ gửi hoặc nhận
    pub confirmations: u32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TxDirection { Incoming, Outgoing }

impl TxHistoryEntry {
    pub fn amount_display(&self) -> String {
        paklets_to_pkt_display(self.amount_paklets)
    }

    pub fn direction_symbol(&self) -> &'static str {
        match self.direction {
            TxDirection::Incoming => "↓",
            TxDirection::Outgoing => "↑",
        }
    }

    pub fn status_label(&self) -> String {
        if self.confirmations == 0 {
            "pending".to_string()
        } else {
            format!("{} conf", self.confirmations)
        }
    }
}

// ── Wallet TUI state ─────────────────────────────────────────────────────────

/// Toàn bộ state của Wallet TUI
#[derive(Debug)]
pub struct WalletTuiState {
    pub active_tab:     WalletTab,
    pub address:        String,
    pub balance_paklets: u64,
    pub network:        String,

    // Send tab
    pub recipient_input: String,
    pub amount_input:    String,
    pub send_step:       SendStep,
    pub send_error:      Option<SendError>,

    // History
    pub history:         Vec<TxHistoryEntry>,
    pub history_offset:  usize,   // scroll offset

    pub status_msg:      Option<String>,
}

impl WalletTuiState {
    pub fn new(address: &str, balance_paklets: u64, network: &str) -> Self {
        WalletTuiState {
            active_tab:      WalletTab::Balance,
            address:         address.to_string(),
            balance_paklets,
            network:         network.to_string(),
            recipient_input: String::new(),
            amount_input:    String::new(),
            send_step:       SendStep::Input { active_field: SendField::Recipient },
            send_error:      None,
            history:         Vec::new(),
            history_offset:  0,
            status_msg:      None,
        }
    }

    pub fn balance_pkt_display(&self) -> String {
        paklets_to_pkt_display(self.balance_paklets)
    }

    /// Chuyển tab
    pub fn next_tab(&mut self) { self.active_tab = self.active_tab.next(); }
    pub fn prev_tab(&mut self) { self.active_tab = self.active_tab.prev(); }

    /// Reset send form về trạng thái ban đầu
    pub fn reset_send(&mut self) {
        self.recipient_input = String::new();
        self.amount_input    = String::new();
        self.send_step       = SendStep::Input { active_field: SendField::Recipient };
        self.send_error      = None;
    }

    /// Toggle active field trong Send Input step
    pub fn toggle_send_field(&mut self) {
        if let SendStep::Input { ref mut active_field } = self.send_step {
            *active_field = match active_field {
                SendField::Recipient => SendField::Amount,
                SendField::Amount    => SendField::Recipient,
            };
        }
    }

    /// Nhập ký tự vào field đang active
    pub fn send_push_char(&mut self, c: char) {
        if let SendStep::Input { ref active_field } = self.send_step.clone() {
            match active_field {
                SendField::Recipient => self.recipient_input.push(c),
                SendField::Amount    => {
                    // Chỉ nhận digit và dấu chấm
                    if c.is_ascii_digit() || (c == '.' && !self.amount_input.contains('.')) {
                        self.amount_input.push(c);
                    }
                }
            }
            self.send_error = None;
        }
    }

    /// Backspace trên field đang active
    pub fn send_pop_char(&mut self) {
        if let SendStep::Input { ref active_field } = self.send_step.clone() {
            match active_field {
                SendField::Recipient => { self.recipient_input.pop(); }
                SendField::Amount    => { self.amount_input.pop(); }
            }
        }
    }

    /// Validate và chuyển sang Confirm step
    pub fn send_proceed(&mut self) -> Result<(), SendError> {
        // Validate recipient
        if self.recipient_input.is_empty() {
            let e = SendError::EmptyRecipient;
            self.send_error = Some(e.clone());
            return Err(e);
        }
        if !is_valid_pkt_address(&self.recipient_input) {
            let e = SendError::InvalidRecipient;
            self.send_error = Some(e.clone());
            return Err(e);
        }
        // Validate amount
        if self.amount_input.is_empty() {
            let e = SendError::EmptyAmount;
            self.send_error = Some(e.clone());
            return Err(e);
        }
        let pkt: f64 = self.amount_input.parse().map_err(|_| {
            let e = SendError::InvalidAmount;
            self.send_error = Some(e.clone());
            e
        })?;
        if pkt <= 0.0 {
            let e = SendError::ZeroAmount;
            self.send_error = Some(e.clone());
            return Err(e);
        }
        let need_paklets = (pkt * PAKLETS_PER_PKT as f64) as u64;
        if need_paklets > self.balance_paklets {
            let e = SendError::InsufficientFunds {
                have: self.balance_paklets,
                need: need_paklets,
            };
            self.send_error = Some(e.clone());
            return Err(e);
        }
        self.send_error = None;
        self.send_step = SendStep::Confirm;
        Ok(())
    }

    /// Xác nhận giao dịch (giả lập ký + broadcast)
    pub fn send_confirm(&mut self) {
        // Trừ balance
        let pkt: f64 = self.amount_input.parse().unwrap_or(0.0);
        let paklets = (pkt * PAKLETS_PER_PKT as f64) as u64;
        self.balance_paklets = self.balance_paklets.saturating_sub(paklets);

        // Thêm vào history
        let txid = format!("tx{:016x}", paklets ^ 0xdeadbeef_cafebabe);
        self.history.push(TxHistoryEntry {
            txid: txid.clone(),
            direction: TxDirection::Outgoing,
            amount_paklets: paklets,
            counterpart: self.recipient_input.clone(),
            confirmations: 0,
            timestamp: 0,
        });

        self.send_step = SendStep::Done { txid };
        self.status_msg = Some("Giao dịch đã được broadcast!".to_string());
    }

    /// Hủy giao dịch
    pub fn send_cancel(&mut self) {
        self.send_step = SendStep::Cancelled;
        self.reset_send();
    }

    /// Thêm tx vào history (dùng khi load từ node)
    pub fn push_history(&mut self, entry: TxHistoryEntry) {
        self.history.push(entry);
    }

    /// Scroll history
    pub fn history_scroll_down(&mut self) {
        if self.history_offset + 1 < self.history.len() {
            self.history_offset += 1;
        }
    }
    pub fn history_scroll_up(&mut self) {
        self.history_offset = self.history_offset.saturating_sub(1);
    }

    /// Parse amount_input thành paklets (0 nếu invalid)
    pub fn amount_paklets(&self) -> u64 {
        self.amount_input.parse::<f64>()
            .map(|f| (f * PAKLETS_PER_PKT as f64) as u64)
            .unwrap_or(0)
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

pub fn paklets_to_pkt_display(paklets: u64) -> String {
    let pkt = paklets as f64 / PAKLETS_PER_PKT as f64;
    format!("{:.8}", pkt)
}

/// Kiểm tra bech32 PKT address (đơn giản — chỉ check prefix và độ dài)
pub fn is_valid_pkt_address(addr: &str) -> bool {
    let valid_prefixes = ["pkt1", "tpkt1", "rpkt1"];
    let has_valid_prefix = valid_prefixes.iter().any(|p| addr.starts_with(p));
    has_valid_prefix && addr.len() >= 14 && addr.len() <= 90
        && addr.chars().all(|c| c.is_ascii_alphanumeric())
}

// ── Render ──────────────────────────────────────────────────────────────────

pub fn render_wallet(frame: &mut Frame, state: &WalletTuiState) {
    let area = frame.size();

    // Outer layout: title bar (1) / tabs (3) / body (fill) / status (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(area);

    render_title(frame, chunks[0], state);
    render_tabs(frame, chunks[1], state);
    match state.active_tab {
        WalletTab::Balance  => render_balance(frame, chunks[2], state),
        WalletTab::Send     => render_send(frame, chunks[2], state),
        WalletTab::Receive  => render_receive(frame, chunks[2], state),
        WalletTab::History  => render_history(frame, chunks[2], state),
    }
    render_status_bar(frame, chunks[3], state);

    // Popup xác nhận (overlay)
    if state.send_step == SendStep::Confirm {
        render_confirm_popup(frame, area, state);
    }
}

fn render_title(frame: &mut Frame, area: Rect, state: &WalletTuiState) {
    let title = format!(
        " PKT Wallet — {} | {} PKT ",
        state.network.to_uppercase(),
        state.balance_pkt_display()
    );
    let para = Paragraph::new(Span::styled(
        title,
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(para, area);
}

fn render_tabs(frame: &mut Frame, area: Rect, state: &WalletTuiState) {
    let titles: Vec<Line> = WalletTab::all()
        .iter()
        .map(|t| Line::from(Span::raw(t.label())))
        .collect();
    let selected = state.active_tab.index();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Navigation "))
        .select(selected)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, area);
}

fn render_balance(frame: &mut Frame, area: Rect, state: &WalletTuiState) {
    let lines = vec![
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  Address : ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.address, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  Balance : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} PKT", state.balance_pkt_display()),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("          : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} paklets", state.balance_paklets),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(vec![]),
        Line::from(vec![
            Span::styled(
                "  Tab → next tab   Shift+Tab → prev tab   q → quit",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];
    let block = Block::default().title(" Balance ").borders(Borders::ALL);
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn render_send(frame: &mut Frame, area: Rect, state: &WalletTuiState) {
    let active_field = if let SendStep::Input { ref active_field } = state.send_step {
        Some(active_field.clone())
    } else { None };

    let recipient_style = if active_field == Some(SendField::Recipient) {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let amount_style = if active_field == Some(SendField::Amount) {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let mut lines = vec![
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  Địa chỉ nhận : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if state.recipient_input.is_empty() { "<nhập pkt1...>" } else { &state.recipient_input },
                recipient_style,
            ),
        ]),
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  Số lượng PKT : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if state.amount_input.is_empty() { "<ví dụ: 10.5>" } else { &state.amount_input },
                amount_style,
            ),
        ]),
        Line::from(vec![]),
    ];

    if let Some(ref e) = state.send_error {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  ⚠ {}", e),
                Style::default().fg(Color::Red),
            ),
        ]));
        lines.push(Line::from(vec![]));
    }

    // Hướng dẫn
    let hint = match &state.send_step {
        SendStep::Input { .. } =>
            "  Tab → chuyển field   Enter → xác nhận   Esc → hủy",
        SendStep::Done { txid } =>
            return render_send_done(frame, area, txid),
        SendStep::Cancelled =>
            "  Giao dịch đã hủy.   Enter → gửi mới",
        _ => "",
    };
    lines.push(Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray))));

    let block = Block::default().title(" Send PKT ").borders(Borders::ALL);
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn render_send_done(frame: &mut Frame, area: Rect, txid: &str) {
    let lines = vec![
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  ✓ Giao dịch đã broadcast!", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  TxID : ", Style::default().fg(Color::DarkGray)),
            Span::styled(txid, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  Enter → gửi mới", Style::default().fg(Color::DarkGray)),
        ]),
    ];
    let block = Block::default().title(" Send PKT ").borders(Borders::ALL);
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn render_receive(frame: &mut Frame, area: Rect, state: &WalletTuiState) {
    let lines = vec![
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  Gửi PKT đến địa chỉ này:", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![]),
        Line::from(vec![
            Span::styled(
                format!("  {}", state.address),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![]),
        Line::from(vec![
            Span::styled(
                "  (QR code sẽ có ở v14.3)",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];
    let block = Block::default().title(" Receive PKT ").borders(Borders::ALL);
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn render_history(frame: &mut Frame, area: Rect, state: &WalletTuiState) {
    let items: Vec<ListItem> = state.history
        .iter()
        .skip(state.history_offset)
        .map(|tx| {
            let color = match tx.direction {
                TxDirection::Incoming => Color::Green,
                TxDirection::Outgoing => Color::Red,
            };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", tx.direction_symbol()),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:>14} PKT", tx.amount_display()),
                    Style::default().fg(color),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{:.12}…", &tx.counterpart[..tx.counterpart.len().min(12)]),
                    Style::default().fg(Color::Gray),
                ),
                Span::raw("  "),
                Span::styled(
                    tx.status_label(),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = format!(" History ({} txs) ", state.history.len());
    let block = Block::default().title(title).borders(Borders::ALL);
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_confirm_popup(frame: &mut Frame, area: Rect, state: &WalletTuiState) {
    // Popup 60%×40% ở giữa màn hình
    let popup_area = centered_rect(60, 40, area);

    let pkt: f64 = state.amount_input.parse().unwrap_or(0.0);
    let lines = vec![
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  Xác nhận giao dịch:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  Gửi đến : ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.recipient_input, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("  Số lượng : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.8} PKT", pkt),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Số dư còn lại : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} PKT",
                    paklets_to_pkt_display(state.balance_paklets.saturating_sub(state.amount_paklets()))),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(vec![]),
        Line::from(vec![
            Span::styled(
                "  [ Enter ] Ký và gửi     [ Esc ] Hủy",
                Style::default().fg(Color::Yellow),
            ),
        ]),
    ];

    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(" ⚠  Xác nhận ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(para, popup_area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, state: &WalletTuiState) {
    let msg = state.status_msg.as_deref().unwrap_or("q=quit  Tab=next  Shift+Tab=prev  ↑↓=scroll");
    let para = Paragraph::new(Span::styled(
        msg,
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(para, area);
}

/// Tính rect ở giữa với tỉ lệ phần trăm
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

// ── Events ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum WalletEvent {
    NextTab,
    PrevTab,
    Confirm,
    Cancel,
    Char(char),
    Backspace,
    ScrollDown,
    ScrollUp,
    Quit,
}

pub fn parse_wallet_event(ev: crossterm::event::Event) -> Option<WalletEvent> {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    match ev {
        Event::Key(KeyEvent { code: KeyCode::Char('q'), modifiers: KeyModifiers::NONE, .. })
        | Event::Key(KeyEvent { code: KeyCode::Esc, .. }) => Some(WalletEvent::Quit),
        Event::Key(KeyEvent { code: KeyCode::Tab, modifiers: KeyModifiers::NONE, .. })  => Some(WalletEvent::NextTab),
        Event::Key(KeyEvent { code: KeyCode::BackTab, .. })                              => Some(WalletEvent::PrevTab),
        Event::Key(KeyEvent { code: KeyCode::Enter, .. })                                => Some(WalletEvent::Confirm),
        Event::Key(KeyEvent { code: KeyCode::Backspace, .. })                            => Some(WalletEvent::Backspace),
        Event::Key(KeyEvent { code: KeyCode::Down, .. })                                 => Some(WalletEvent::ScrollDown),
        Event::Key(KeyEvent { code: KeyCode::Up, .. })                                   => Some(WalletEvent::ScrollUp),
        Event::Key(KeyEvent { code: KeyCode::Char(c), modifiers: KeyModifiers::NONE, .. }) => Some(WalletEvent::Char(c)),
        _ => None,
    }
}

/// Xử lý event → cập nhật state, trả về true nếu cần thoát
pub fn handle_wallet_event(state: &mut WalletTuiState, ev: WalletEvent) -> bool {
    match ev {
        WalletEvent::Quit => return true,

        WalletEvent::NextTab => state.next_tab(),
        WalletEvent::PrevTab => state.prev_tab(),

        WalletEvent::ScrollDown => state.history_scroll_down(),
        WalletEvent::ScrollUp   => state.history_scroll_up(),

        WalletEvent::Char(c) => {
            if state.active_tab == WalletTab::Send {
                match &state.send_step {
                    SendStep::Input { .. } => state.send_push_char(c),
                    SendStep::Done { .. } | SendStep::Cancelled => {}
                    _ => {}
                }
            }
        }

        WalletEvent::Backspace => {
            if state.active_tab == WalletTab::Send {
                state.send_pop_char();
            }
        }

        WalletEvent::Confirm => {
            if state.active_tab == WalletTab::Send {
                match state.send_step.clone() {
                    SendStep::Input { active_field: SendField::Recipient } => {
                        state.toggle_send_field();
                    }
                    SendStep::Input { active_field: SendField::Amount } => {
                        let _ = state.send_proceed();
                    }
                    SendStep::Confirm => {
                        state.send_confirm();
                    }
                    SendStep::Done { .. } | SendStep::Cancelled => {
                        state.reset_send();
                    }
                }
            }
        }

        WalletEvent::Cancel => {
            if state.active_tab == WalletTab::Send {
                state.send_cancel();
            }
        }
    }
    false
}

// ── cmd_wallet_tui ───────────────────────────────────────────────────────────

pub fn cmd_wallet_tui(address: &str, balance_paklets: u64, network: &str) {
    use crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};
    use std::time::Duration;

    enable_raw_mode().expect("enable raw mode");
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("enter alternate screen");
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("create terminal");

    let mut state = WalletTuiState::new(address, balance_paklets, network);

    loop {
        terminal.draw(|f| render_wallet(f, &state)).expect("draw");

        if crossterm::event::poll(Duration::from_millis(200)).unwrap_or(false) {
            if let Ok(ev) = crossterm::event::read() {
                // Esc trong Send/Confirm → cancel thay vì quit
                if state.active_tab == WalletTab::Send
                    && state.send_step == SendStep::Confirm
                {
                    use crossterm::event::{Event, KeyCode, KeyEvent};
                    if matches!(ev, Event::Key(KeyEvent { code: KeyCode::Esc, .. })) {
                        state.send_cancel();
                        continue;
                    }
                }
                if let Some(we) = parse_wallet_event(ev) {
                    if handle_wallet_event(&mut state, we) { break; }
                }
            }
        }
    }

    disable_raw_mode().expect("disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen).expect("leave alternate screen");
    terminal.show_cursor().expect("show cursor");
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    const ADDR: &str = "pkt1qtest000000000000000000000000000000000";
    const BAL:  u64  = 10 * PAKLETS_PER_PKT;  // 10 PKT

    fn state() -> WalletTuiState {
        WalletTuiState::new(ADDR, BAL, "testnet")
    }

    fn terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(120, 40)).unwrap()
    }

    // ── WalletTab ─────────────────────────────────────────────────────────

    #[test]
    fn test_tab_next_wraps() {
        let mut t = WalletTab::History;
        t = t.next();
        assert_eq!(t, WalletTab::Balance);
    }

    #[test]
    fn test_tab_prev_wraps() {
        let mut t = WalletTab::Balance;
        t = t.prev();
        assert_eq!(t, WalletTab::History);
    }

    #[test]
    fn test_tab_index_balance() {
        assert_eq!(WalletTab::Balance.index(), 0);
    }

    #[test]
    fn test_tab_labels() {
        assert_eq!(WalletTab::Send.label(), "Send");
        assert_eq!(WalletTab::Receive.label(), "Receive");
        assert_eq!(WalletTab::History.label(), "History");
    }

    // ── WalletTuiState ────────────────────────────────────────────────────

    #[test]
    fn test_initial_state_balance_tab() {
        let s = state();
        assert_eq!(s.active_tab, WalletTab::Balance);
    }

    #[test]
    fn test_balance_pkt_display() {
        let s = state();
        assert!(s.balance_pkt_display().starts_with("10.000"));
    }

    #[test]
    fn test_next_prev_tab() {
        let mut s = state();
        s.next_tab();
        assert_eq!(s.active_tab, WalletTab::Send);
        s.prev_tab();
        assert_eq!(s.active_tab, WalletTab::Balance);
    }

    #[test]
    fn test_push_char_recipient() {
        let mut s = state();
        s.active_tab = WalletTab::Send;
        "pkt1qtest".chars().for_each(|c| s.send_push_char(c));
        assert_eq!(s.recipient_input, "pkt1qtest");
    }

    #[test]
    fn test_push_char_amount_only_digits_and_dot() {
        let mut s = state();
        s.active_tab = WalletTab::Send;
        s.toggle_send_field(); // → Amount field
        "1.5x".chars().for_each(|c| s.send_push_char(c));
        assert_eq!(s.amount_input, "1.5"); // 'x' bị lọc
    }

    #[test]
    fn test_push_char_amount_no_double_dot() {
        let mut s = state();
        s.toggle_send_field();
        "1..5".chars().for_each(|c| s.send_push_char(c));
        assert_eq!(s.amount_input, "1.5"); // dấu chấm thứ hai bị bỏ qua
    }

    #[test]
    fn test_pop_char() {
        let mut s = state();
        "pkt1".chars().for_each(|c| s.send_push_char(c));
        s.send_pop_char(); // "pkt1" → pop → "pkt"
        assert_eq!(s.recipient_input, "pkt");
    }

    #[test]
    fn test_pop_char_removes_last() {
        let mut s = state();
        s.recipient_input = "pkt1abc".to_string();
        s.send_pop_char();
        assert_eq!(s.recipient_input, "pkt1ab");
    }

    #[test]
    fn test_reset_send_clears_fields() {
        let mut s = state();
        s.recipient_input = "pkt1abc".to_string();
        s.amount_input = "5".to_string();
        s.reset_send();
        assert!(s.recipient_input.is_empty());
        assert!(s.amount_input.is_empty());
    }

    #[test]
    fn test_toggle_send_field() {
        let mut s = state();
        assert!(matches!(s.send_step, SendStep::Input { active_field: SendField::Recipient }));
        s.toggle_send_field();
        assert!(matches!(s.send_step, SendStep::Input { active_field: SendField::Amount }));
    }

    // ── send_proceed validation ────────────────────────────────────────────

    #[test]
    fn test_proceed_empty_recipient_error() {
        let mut s = state();
        s.active_tab = WalletTab::Send;
        // Move to amount field
        s.send_step = SendStep::Input { active_field: SendField::Amount };
        let r = s.send_proceed();
        assert_eq!(r, Err(SendError::EmptyRecipient));
    }

    #[test]
    fn test_proceed_invalid_recipient_error() {
        let mut s = state();
        s.recipient_input = "notanaddress".to_string();
        s.amount_input = "1".to_string();
        s.send_step = SendStep::Input { active_field: SendField::Amount };
        let r = s.send_proceed();
        assert_eq!(r, Err(SendError::InvalidRecipient));
    }

    #[test]
    fn test_proceed_empty_amount_error() {
        let mut s = state();
        s.recipient_input = "pkt1qtest000000000000000000000000000000000".to_string();
        s.send_step = SendStep::Input { active_field: SendField::Amount };
        let r = s.send_proceed();
        assert_eq!(r, Err(SendError::EmptyAmount));
    }

    #[test]
    fn test_proceed_zero_amount_error() {
        let mut s = state();
        s.recipient_input = "pkt1qtest000000000000000000000000000000000".to_string();
        s.amount_input = "0".to_string();
        s.send_step = SendStep::Input { active_field: SendField::Amount };
        let r = s.send_proceed();
        assert_eq!(r, Err(SendError::ZeroAmount));
    }

    #[test]
    fn test_proceed_insufficient_funds_error() {
        let mut s = state(); // 10 PKT
        s.recipient_input = "pkt1qtest000000000000000000000000000000000".to_string();
        s.amount_input = "999".to_string();
        s.send_step = SendStep::Input { active_field: SendField::Amount };
        let r = s.send_proceed();
        assert!(matches!(r, Err(SendError::InsufficientFunds { .. })));
    }

    #[test]
    fn test_proceed_valid_moves_to_confirm() {
        let mut s = state(); // 10 PKT
        s.recipient_input = "pkt1qtest000000000000000000000000000000000".to_string();
        s.amount_input = "1".to_string();
        s.send_step = SendStep::Input { active_field: SendField::Amount };
        assert!(s.send_proceed().is_ok());
        assert_eq!(s.send_step, SendStep::Confirm);
    }

    #[test]
    fn test_send_confirm_deducts_balance() {
        let mut s = state(); // 10 PKT
        s.recipient_input = "pkt1qtest000000000000000000000000000000000".to_string();
        s.amount_input = "5".to_string();
        s.send_step = SendStep::Confirm;
        s.send_confirm();
        let expected = 5 * PAKLETS_PER_PKT;
        assert_eq!(s.balance_paklets, expected);
    }

    #[test]
    fn test_send_confirm_adds_to_history() {
        let mut s = state();
        s.recipient_input = "pkt1qtest000000000000000000000000000000000".to_string();
        s.amount_input = "1".to_string();
        s.send_step = SendStep::Confirm;
        s.send_confirm();
        assert_eq!(s.history.len(), 1);
        assert_eq!(s.history[0].direction, TxDirection::Outgoing);
    }

    #[test]
    fn test_send_confirm_sets_done_step() {
        let mut s = state();
        s.recipient_input = "pkt1qtest000000000000000000000000000000000".to_string();
        s.amount_input = "1".to_string();
        s.send_step = SendStep::Confirm;
        s.send_confirm();
        assert!(matches!(s.send_step, SendStep::Done { .. }));
    }

    #[test]
    fn test_send_cancel_resets_form() {
        let mut s = state();
        s.recipient_input = "pkt1qtest000000000000000000000000000000000".to_string();
        s.send_cancel();
        assert!(s.recipient_input.is_empty());
    }

    // ── History ───────────────────────────────────────────────────────────

    #[test]
    fn test_push_history() {
        let mut s = state();
        s.push_history(TxHistoryEntry {
            txid: "txabc".to_string(),
            direction: TxDirection::Incoming,
            amount_paklets: PAKLETS_PER_PKT,
            counterpart: "pkt1qsender0000".to_string(),
            confirmations: 6,
            timestamp: 0,
        });
        assert_eq!(s.history.len(), 1);
        assert_eq!(s.history[0].direction, TxDirection::Incoming);
    }

    #[test]
    fn test_history_scroll() {
        let mut s = state();
        for i in 0..5 {
            s.push_history(TxHistoryEntry {
                txid: format!("tx{}", i),
                direction: TxDirection::Outgoing,
                amount_paklets: PAKLETS_PER_PKT,
                counterpart: "pkt1q0000".to_string(),
                confirmations: 0,
                timestamp: 0,
            });
        }
        s.history_scroll_down();
        assert_eq!(s.history_offset, 1);
        s.history_scroll_up();
        assert_eq!(s.history_offset, 0);
    }

    #[test]
    fn test_history_scroll_up_at_zero_stays() {
        let mut s = state();
        s.history_scroll_up();
        assert_eq!(s.history_offset, 0);
    }

    #[test]
    fn test_tx_history_status_label_pending() {
        let tx = TxHistoryEntry {
            txid: "x".to_string(), direction: TxDirection::Outgoing,
            amount_paklets: 0, counterpart: "a".to_string(),
            confirmations: 0, timestamp: 0,
        };
        assert_eq!(tx.status_label(), "pending");
    }

    #[test]
    fn test_tx_history_status_label_confirmed() {
        let tx = TxHistoryEntry {
            txid: "x".to_string(), direction: TxDirection::Outgoing,
            amount_paklets: 0, counterpart: "a".to_string(),
            confirmations: 3, timestamp: 0,
        };
        assert_eq!(tx.status_label(), "3 conf");
    }

    // ── Helpers ───────────────────────────────────────────────────────────

    #[test]
    fn test_paklets_to_pkt_display_one_pkt() {
        assert_eq!(paklets_to_pkt_display(PAKLETS_PER_PKT), "1.00000000");
    }

    #[test]
    fn test_paklets_to_pkt_display_zero() {
        assert_eq!(paklets_to_pkt_display(0), "0.00000000");
    }

    #[test]
    fn test_is_valid_pkt_address_mainnet() {
        assert!(is_valid_pkt_address("pkt1qtest000000000000000000000000000000000"));
    }

    #[test]
    fn test_is_valid_pkt_address_testnet() {
        assert!(is_valid_pkt_address("tpkt1qtest0000000000000000000000000000000"));
    }

    #[test]
    fn test_is_valid_pkt_address_invalid() {
        assert!(!is_valid_pkt_address("notanaddress"));
        assert!(!is_valid_pkt_address("btc1qtest000000000000000000000000000000000"));
        assert!(!is_valid_pkt_address("pkt1")); // too short
    }

    #[test]
    fn test_amount_paklets_from_input() {
        let mut s = state();
        s.amount_input = "2".to_string();
        assert_eq!(s.amount_paklets(), 2 * PAKLETS_PER_PKT);
    }

    #[test]
    fn test_send_error_display_insufficient_funds() {
        let e = SendError::InsufficientFunds { have: PAKLETS_PER_PKT, need: 2 * PAKLETS_PER_PKT };
        let msg = format!("{}", e);
        assert!(msg.contains("PKT"));
    }

    // ── handle_wallet_event ───────────────────────────────────────────────

    #[test]
    fn test_event_quit_returns_true() {
        let mut s = state();
        assert!(handle_wallet_event(&mut s, WalletEvent::Quit));
    }

    #[test]
    fn test_event_next_tab_changes_tab() {
        let mut s = state();
        handle_wallet_event(&mut s, WalletEvent::NextTab);
        assert_eq!(s.active_tab, WalletTab::Send);
    }

    #[test]
    fn test_event_char_appends_to_send() {
        let mut s = state();
        s.active_tab = WalletTab::Send;
        handle_wallet_event(&mut s, WalletEvent::Char('p'));
        handle_wallet_event(&mut s, WalletEvent::Char('k'));
        handle_wallet_event(&mut s, WalletEvent::Char('t'));
        assert_eq!(s.recipient_input, "pkt");
    }

    #[test]
    fn test_event_backspace_removes_char() {
        let mut s = state();
        s.active_tab = WalletTab::Send;
        s.recipient_input = "pkt1abc".to_string();
        handle_wallet_event(&mut s, WalletEvent::Backspace);
        assert_eq!(s.recipient_input, "pkt1ab");
    }

    // ── Render smoke tests ────────────────────────────────────────────────

    #[test]
    fn test_render_balance_tab() {
        let mut t = terminal();
        let s = state();
        t.draw(|f| render_wallet(f, &s)).unwrap();
    }

    #[test]
    fn test_render_send_tab() {
        let mut t = terminal();
        let mut s = state();
        s.active_tab = WalletTab::Send;
        t.draw(|f| render_wallet(f, &s)).unwrap();
    }

    #[test]
    fn test_render_confirm_popup() {
        let mut t = terminal();
        let mut s = state();
        s.active_tab = WalletTab::Send;
        s.recipient_input = "pkt1qtest000000000000000000000000000000000".to_string();
        s.amount_input = "1".to_string();
        s.send_step = SendStep::Confirm;
        t.draw(|f| render_wallet(f, &s)).unwrap();
    }

    #[test]
    fn test_render_receive_tab() {
        let mut t = terminal();
        let mut s = state();
        s.active_tab = WalletTab::Receive;
        t.draw(|f| render_wallet(f, &s)).unwrap();
    }

    #[test]
    fn test_render_history_tab_with_entries() {
        let mut t = terminal();
        let mut s = state();
        s.active_tab = WalletTab::History;
        s.push_history(TxHistoryEntry {
            txid: "txabc".to_string(), direction: TxDirection::Incoming,
            amount_paklets: PAKLETS_PER_PKT,
            counterpart: "pkt1qsender0000000000000000000000000000000".to_string(),
            confirmations: 3, timestamp: 0,
        });
        t.draw(|f| render_wallet(f, &s)).unwrap();
    }

    #[test]
    fn test_render_small_terminal() {
        let mut t = Terminal::new(TestBackend::new(40, 20)).unwrap();
        let s = state();
        t.draw(|f| render_wallet(f, &s)).unwrap();
    }
}
