#![allow(dead_code)]

/// v2.9 — AI Agent: Condition-based Autonomous TX Signing
///
/// Kiến trúc:
///
///   Market Data          Agent Engine             Blockchain
///   ───────────          ────────────             ──────────
///   Price feed ─────────► Evaluate rules          Sign TX
///   Balance feed ───────► Match conditions ──────► Broadcast TX
///   Time tick ──────────► Select action            Record log
///                        Check safety limits
///
/// Agent lifecycle:
///   Idle → Evaluating → Triggered → Executing → Cooldown → Idle
///                     ↘ NoMatch  →────────────────────────────↗
///
/// Rule types:
///   PriceBelow/Above: kích hoạt khi giá vượt ngưỡng
///   BalanceBelow: tự động top-up khi số dư thấp
///   PriceChangePct: phản ứng với biến động giá lớn
///   TimeInterval: định kỳ (DCA, rebalance)
///   PortfolioRatio: giữ tỷ lệ tài sản mục tiêu
///
/// Safety model:
///   - max_tx_amount: giới hạn mỗi TX
///   - daily_limit: tổng chi tiêu trong 24h
///   - cooldown: min blocks giữa 2 lần execute
///   - pause: guardian có thể pause agent
///   - whitelist: chỉ giao dịch với địa chỉ được phép
///
/// Tham khảo: Gelato Network, Chainlink Automation, MEV bots, DeFi strategies

use std::collections::HashMap;

// ─── MarketData ────────────────────────────────────────────────────────────────

/// Snapshot thị trường tại 1 thời điểm
#[derive(Debug, Clone)]
pub struct MarketData {
    pub block:      u64,
    pub timestamp:  u64,
    pub prices:     HashMap<String, f64>,   // "PKT/USD", "ETH/USD", ...
    pub volumes:    HashMap<String, f64>,
}

impl MarketData {
    pub fn new(block: u64, timestamp: u64) -> Self {
        MarketData { block, timestamp, prices: HashMap::new(), volumes: HashMap::new() }
    }

    pub fn set_price(&mut self, pair: &str, price: f64) {
        self.prices.insert(pair.to_string(), price);
    }

    pub fn price(&self, pair: &str) -> f64 {
        *self.prices.get(pair).unwrap_or(&0.0)
    }
}

// ─── AgentCondition ───────────────────────────────────────────────────────────

/// Điều kiện để kích hoạt rule
#[derive(Debug, Clone)]
pub enum AgentCondition {
    /// Giá cặp pair thấp hơn threshold → mua
    PriceBelow { pair: String, threshold: f64 },
    /// Giá cặp pair cao hơn threshold → bán
    PriceAbove { pair: String, threshold: f64 },
    /// Số dư asset thấp hơn min → top-up
    BalanceBelow { asset: String, min_balance: f64 },
    /// Giá thay đổi > pct% so với lần check trước
    PriceChangePct { pair: String, pct: f64 },
    /// Chạy định kỳ mỗi interval blocks (DCA)
    TimeInterval { interval_blocks: u64 },
    /// Tỷ lệ tài sản lệch khỏi target > tolerance
    PortfolioRatio { asset: String, target_pct: f64, tolerance_pct: f64 },
    /// Luôn true (manual trigger)
    Always,
}

impl AgentCondition {
    pub fn evaluate(&self, market: &MarketData, agent: &AgentWallet, last_trigger_block: u64) -> bool {
        match self {
            AgentCondition::PriceBelow { pair, threshold } =>
                market.price(pair) > 0.0 && market.price(pair) < *threshold,

            AgentCondition::PriceAbove { pair, threshold } =>
                market.price(pair) > *threshold,

            AgentCondition::BalanceBelow { asset, min_balance } =>
                agent.balance(asset) < *min_balance,

            AgentCondition::PriceChangePct { pair, pct } => {
                if let Some(&prev) = agent.price_snapshots.get(pair) {
                    if prev == 0.0 { return false; }
                    let change = (market.price(pair) - prev).abs() / prev * 100.0;
                    change >= *pct
                } else {
                    false
                }
            }

            AgentCondition::TimeInterval { interval_blocks } =>
                market.block >= last_trigger_block + interval_blocks,

            AgentCondition::PortfolioRatio { asset, target_pct, tolerance_pct } => {
                let total = agent.total_value_usd(market);
                if total == 0.0 { return false; }
                let asset_val = agent.balance(asset) * market.price(&format!("{}/USD", asset));
                let actual_pct = asset_val / total * 100.0;
                (actual_pct - target_pct).abs() > *tolerance_pct
            }

            AgentCondition::Always => true,
        }
    }

    pub fn description(&self) -> String {
        match self {
            AgentCondition::PriceBelow { pair, threshold } =>
                format!("{} < ${:.0}", pair, threshold),
            AgentCondition::PriceAbove { pair, threshold } =>
                format!("{} > ${:.0}", pair, threshold),
            AgentCondition::BalanceBelow { asset, min_balance } =>
                format!("{} balance < {:.0}", asset, min_balance),
            AgentCondition::PriceChangePct { pair, pct } =>
                format!("{} moved >{:.0}%", pair, pct),
            AgentCondition::TimeInterval { interval_blocks } =>
                format!("every {} blocks (DCA)", interval_blocks),
            AgentCondition::PortfolioRatio { asset, target_pct, tolerance_pct } =>
                format!("{} ratio off target {:.0}% ±{:.0}%", asset, target_pct, tolerance_pct),
            AgentCondition::Always => "always".to_string(),
        }
    }
}

// ─── AgentAction ──────────────────────────────────────────────────────────────

/// Hành động agent sẽ thực hiện khi condition met
#[derive(Debug, Clone)]
pub enum AgentAction {
    /// Chuyển tiền đến địa chỉ
    Transfer { to: String, asset: String, amount: f64 },
    /// Swap asset này lấy asset kia (simplified DEX)
    Swap { from_asset: String, to_asset: String, amount: f64 },
    /// Stake token
    Stake { asset: String, amount: f64, validator: String },
    /// Unstake token
    Unstake { asset: String, amount: f64, validator: String },
    /// Rebalance portfolio về target ratio
    Rebalance { target_btc_pct: f64, target_eth_pct: f64 },
    /// DCA: mua cố định mỗi chu kỳ
    DcaBuy { asset: String, usd_amount: f64 },
    /// Stop-loss: bán khi giá giảm quá ngưỡng
    StopLoss { asset: String, sell_pct: f64 },
    /// Take-profit: bán khi giá tăng đủ
    TakeProfit { asset: String, sell_pct: f64 },
}

impl AgentAction {
    pub fn description(&self) -> String {
        match self {
            AgentAction::Transfer { to, asset, amount } =>
                format!("Transfer {:.2} {} to {}", amount, asset, to),
            AgentAction::Swap { from_asset, to_asset, amount } =>
                format!("Swap {:.2} {} → {}", amount, from_asset, to_asset),
            AgentAction::Stake { asset, amount, validator } =>
                format!("Stake {:.2} {} with {}", amount, asset, validator),
            AgentAction::Unstake { asset, amount, validator } =>
                format!("Unstake {:.2} {} from {}", amount, asset, validator),
            AgentAction::Rebalance { target_btc_pct, target_eth_pct } =>
                format!("Rebalance PKT:{:.0}% ETH:{:.0}%", target_btc_pct, target_eth_pct),
            AgentAction::DcaBuy { asset, usd_amount } =>
                format!("DCA buy ${:.0} of {}", usd_amount, asset),
            AgentAction::StopLoss { asset, sell_pct } =>
                format!("StopLoss: sell {:.0}% of {}", sell_pct, asset),
            AgentAction::TakeProfit { asset, sell_pct } =>
                format!("TakeProfit: sell {:.0}% of {}", sell_pct, asset),
        }
    }
}

// ─── AgentRule ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgentRule {
    pub id:          u64,
    pub name:        String,
    pub condition:   AgentCondition,
    pub action:      AgentAction,
    pub cooldown:    u64,   // min blocks giữa 2 executions
    pub enabled:     bool,
    pub max_executions: Option<u64>,  // None = unlimited
    pub executions:  u64,
}

impl AgentRule {
    pub fn new(
        id: u64,
        name: impl Into<String>,
        condition: AgentCondition,
        action: AgentAction,
        cooldown: u64,
    ) -> Self {
        AgentRule {
            id,
            name: name.into(),
            condition,
            action,
            cooldown,
            enabled: true,
            max_executions: None,
            executions: 0,
        }
    }

    pub fn with_max(mut self, max: u64) -> Self {
        self.max_executions = Some(max);
        self
    }
}

// ─── AgentWallet ──────────────────────────────────────────────────────────────

/// Ví của agent — theo dõi balances và snapshots
#[derive(Debug, Clone)]
pub struct AgentWallet {
    pub address:         String,
    pub balances:        HashMap<String, f64>,
    pub price_snapshots: HashMap<String, f64>,  // giá lúc agent lần cuối check
    pub staked:          HashMap<String, f64>,   // staked amounts
}

impl AgentWallet {
    pub fn new(address: impl Into<String>) -> Self {
        AgentWallet {
            address: address.into(),
            balances: HashMap::new(),
            price_snapshots: HashMap::new(),
            staked: HashMap::new(),
        }
    }

    pub fn deposit(&mut self, asset: &str, amount: f64) {
        *self.balances.entry(asset.to_string()).or_insert(0.0) += amount;
    }

    pub fn balance(&self, asset: &str) -> f64 {
        *self.balances.get(asset).unwrap_or(&0.0)
    }

    pub fn total_value_usd(&self, market: &MarketData) -> f64 {
        self.balances.iter().map(|(asset, &bal)| {
            if asset == "USD" { bal }
            else { bal * market.price(&format!("{}/USD", asset)) }
        }).sum()
    }

    pub fn snapshot_prices(&mut self, market: &MarketData) {
        for (pair, &price) in &market.prices {
            self.price_snapshots.insert(pair.clone(), price);
        }
    }
}

// ─── TxLog ────────────────────────────────────────────────────────────────────

/// Bản ghi mỗi TX agent đã thực hiện
#[derive(Debug, Clone)]
pub struct TxLog {
    pub block:       u64,
    pub rule_name:   String,
    pub action_desc: String,
    pub tx_hash:     String,
    pub success:     bool,
    pub note:        String,
}

impl TxLog {
    pub fn new(block: u64, rule_name: &str, action_desc: &str, success: bool, note: &str) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"agent_tx_v29");
        h.update(&block.to_le_bytes());
        h.update(rule_name.as_bytes());
        h.update(action_desc.as_bytes());
        let tx_hash = format!("0x{}", &hex::encode(h.finalize().as_bytes())[..16]);
        TxLog {
            block,
            rule_name: rule_name.to_string(),
            action_desc: action_desc.to_string(),
            tx_hash,
            success,
            note: note.to_string(),
        }
    }
}

// ─── SafetyConfig ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SafetyConfig {
    pub max_tx_usd:    f64,   // max USD value mỗi TX
    pub daily_limit:   f64,   // max tổng USD trong 24h
    pub whitelist:     Vec<String>,
    pub paused:        bool,
}

impl SafetyConfig {
    pub fn new(max_tx_usd: f64, daily_limit: f64) -> Self {
        SafetyConfig { max_tx_usd, daily_limit, whitelist: vec![], paused: false }
    }

    pub fn add_whitelist(&mut self, addr: impl Into<String>) {
        self.whitelist.push(addr.into());
    }
}

// ─── AgentState ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    Idle,
    Evaluating,
    Executing { rule_id: u64 },
    Cooldown { until_block: u64 },
    Paused,
    Stopped,
}

// ─── Agent ────────────────────────────────────────────────────────────────────

pub struct Agent {
    pub name:              String,
    pub wallet:            AgentWallet,
    pub rules:             Vec<AgentRule>,
    pub safety:            SafetyConfig,
    pub state:             AgentState,
    pub tx_log:            Vec<TxLog>,
    pub last_trigger:      HashMap<u64, u64>,   // rule_id → last triggered block
    pub daily_spent:       f64,
    pub daily_reset_block: u64,
}

impl Agent {
    pub fn new(name: impl Into<String>, address: impl Into<String>, safety: SafetyConfig) -> Self {
        Agent {
            name: name.into(),
            wallet: AgentWallet::new(address),
            rules: vec![],
            safety,
            state: AgentState::Idle,
            tx_log: vec![],
            last_trigger: HashMap::new(),
            daily_spent: 0.0,
            daily_reset_block: 0,
        }
    }

    pub fn add_rule(&mut self, rule: AgentRule) {
        self.rules.push(rule);
    }

    /// Chạy 1 tick — evaluate tất cả rules và execute nếu condition met
    pub fn tick(&mut self, market: &MarketData) -> Vec<TxLog> {
        if self.safety.paused {
            self.state = AgentState::Paused;
            return vec![];
        }

        // Reset daily limit mỗi ~240 blocks (~1 day giả lập)
        if market.block >= self.daily_reset_block + 240 {
            self.daily_spent = 0.0;
            self.daily_reset_block = market.block;
        }

        self.state = AgentState::Evaluating;
        let mut new_logs = vec![];

        // Clone rules để borrow checker happy
        let rules: Vec<AgentRule> = self.rules.clone();

        for rule in &rules {
            if !rule.enabled { continue; }
            if let Some(max) = rule.max_executions {
                if rule.executions >= max { continue; }
            }

            // Cooldown check
            let last = *self.last_trigger.get(&rule.id).unwrap_or(&0);
            if market.block < last + rule.cooldown { continue; }

            // Evaluate condition
            if !rule.condition.evaluate(market, &self.wallet, last) {
                continue;
            }

            // Condition met — try to execute
            self.state = AgentState::Executing { rule_id: rule.id };
            let (success, note) = self.execute_action(&rule.action, market);

            let log = TxLog::new(
                market.block,
                &rule.name,
                &rule.action.description(),
                success,
                &note,
            );
            self.tx_log.push(log.clone());
            new_logs.push(log);

            if success {
                self.last_trigger.insert(rule.id, market.block);
                // Update rule execution count
                if let Some(r) = self.rules.iter_mut().find(|r| r.id == rule.id) {
                    r.executions += 1;
                }
                // Snapshot prices after execution
                self.wallet.snapshot_prices(market);
            }
        }

        self.state = AgentState::Idle;
        new_logs
    }

    /// Execute action — trả về (success, note)
    fn execute_action(&mut self, action: &AgentAction, market: &MarketData) -> (bool, String) {
        match action {
            AgentAction::Transfer { to, asset, amount } => {
                let usd_val = if asset == "USD" { *amount }
                              else { amount * market.price(&format!("{}/USD", asset)) };

                if self.safety.whitelist.len() > 0 && !self.safety.whitelist.contains(to) {
                    return (false, format!("{} not whitelisted", to));
                }
                if usd_val > self.safety.max_tx_usd {
                    return (false, format!("${:.0} exceeds max_tx ${:.0}", usd_val, self.safety.max_tx_usd));
                }
                if self.daily_spent + usd_val > self.safety.daily_limit {
                    return (false, format!("daily limit ${:.0} would be exceeded", self.safety.daily_limit));
                }
                let bal = self.wallet.balance(asset);
                if bal < *amount {
                    return (false, format!("insufficient {}: {:.4} < {:.4}", asset, bal, amount));
                }
                *self.wallet.balances.entry(asset.clone()).or_insert(0.0) -= amount;
                self.daily_spent += usd_val;
                (true, format!("sent {:.4} {} (${:.0})", amount, asset, usd_val))
            }

            AgentAction::Swap { from_asset, to_asset, amount } => {
                let from_price = market.price(&format!("{}/USD", from_asset));
                let to_price   = market.price(&format!("{}/USD", to_asset));
                if from_price == 0.0 || to_price == 0.0 {
                    return (false, "price feed unavailable".to_string());
                }
                let usd_val = amount * from_price;
                if usd_val > self.safety.max_tx_usd {
                    return (false, format!("swap ${:.0} exceeds max_tx", usd_val));
                }
                let bal = self.wallet.balance(from_asset);
                if bal < *amount {
                    return (false, format!("insufficient {}: {:.4} < {:.4}", from_asset, bal, amount));
                }
                let received = usd_val / to_price * 0.997; // 0.3% DEX fee
                *self.wallet.balances.entry(from_asset.clone()).or_insert(0.0) -= amount;
                *self.wallet.balances.entry(to_asset.clone()).or_insert(0.0)   += received;
                self.daily_spent += usd_val;
                (true, format!("{:.4} {} → {:.4} {} (fee 0.3%)", amount, from_asset, received, to_asset))
            }

            AgentAction::Stake { asset, amount, validator } => {
                let bal = self.wallet.balance(asset);
                if bal < *amount {
                    return (false, format!("insufficient {} to stake", asset));
                }
                *self.wallet.balances.entry(asset.clone()).or_insert(0.0) -= amount;
                *self.wallet.staked.entry(validator.clone()).or_insert(0.0) += amount;
                (true, format!("staked {:.4} {} with {}", amount, asset, validator))
            }

            AgentAction::Unstake { asset, amount, validator } => {
                let staked = *self.wallet.staked.get(validator).unwrap_or(&0.0);
                if staked < *amount {
                    return (false, format!("not enough staked at {}", validator));
                }
                *self.wallet.staked.entry(validator.clone()).or_insert(0.0) -= amount;
                *self.wallet.balances.entry(asset.clone()).or_insert(0.0)   += amount;
                (true, format!("unstaked {:.4} {} from {}", amount, asset, validator))
            }

            AgentAction::DcaBuy { asset, usd_amount } => {
                let price = market.price(&format!("{}/USD", asset));
                if price == 0.0 { return (false, "no price".to_string()); }
                if *usd_amount > self.safety.max_tx_usd {
                    return (false, format!("DCA ${} exceeds max_tx", usd_amount));
                }
                if self.daily_spent + usd_amount > self.safety.daily_limit {
                    return (false, "daily limit".to_string());
                }
                let usd_bal = self.wallet.balance("USD");
                if usd_bal < *usd_amount {
                    return (false, format!("insufficient USD: {:.0} < {:.0}", usd_bal, usd_amount));
                }
                let bought = usd_amount / price * 0.997;
                *self.wallet.balances.entry("USD".to_string()).or_insert(0.0)   -= usd_amount;
                *self.wallet.balances.entry(asset.clone()).or_insert(0.0)       += bought;
                self.daily_spent += usd_amount;
                (true, format!("bought {:.6} {} @ ${:.0}", bought, asset, price))
            }

            AgentAction::StopLoss { asset, sell_pct } => {
                let bal = self.wallet.balance(asset);
                let sell_amt = bal * sell_pct / 100.0;
                let price = market.price(&format!("{}/USD", asset));
                let usd_val = sell_amt * price;
                *self.wallet.balances.entry(asset.clone()).or_insert(0.0) -= sell_amt;
                *self.wallet.balances.entry("USD".to_string()).or_insert(0.0) += usd_val * 0.997;
                (true, format!("sold {:.4} {} @ ${:.0} = ${:.0}", sell_amt, asset, price, usd_val))
            }

            AgentAction::TakeProfit { asset, sell_pct } => {
                let bal = self.wallet.balance(asset);
                let sell_amt = bal * sell_pct / 100.0;
                let price = market.price(&format!("{}/USD", asset));
                let usd_val = sell_amt * price;
                *self.wallet.balances.entry(asset.clone()).or_insert(0.0) -= sell_amt;
                *self.wallet.balances.entry("USD".to_string()).or_insert(0.0) += usd_val * 0.997;
                (true, format!("sold {:.4} {} @ ${:.0} = ${:.0}", sell_amt, asset, price, usd_val))
            }

            AgentAction::Rebalance { target_btc_pct, target_eth_pct } => {
                let total = self.wallet.total_value_usd(market);
                if total == 0.0 { return (false, "no portfolio value".to_string()); }
                let btc_price = market.price("PKT/USD");
                let eth_price = market.price("ETH/USD");
                if btc_price == 0.0 || eth_price == 0.0 {
                    return (false, "missing prices".to_string());
                }
                let target_btc_usd = total * target_btc_pct / 100.0;
                let target_eth_usd = total * target_eth_pct / 100.0;
                let current_btc = self.wallet.balance("PKT") * btc_price;
                let current_eth = self.wallet.balance("ETH") * eth_price;
                // Simplified: adjust to targets
                let new_btc = target_btc_usd / btc_price;
                let new_eth = target_eth_usd / eth_price;
                let new_usd = total - target_btc_usd - target_eth_usd;
                self.wallet.balances.insert("PKT".to_string(), new_btc);
                self.wallet.balances.insert("ETH".to_string(), new_eth);
                self.wallet.balances.insert("USD".to_string(), new_usd);
                (true, format!("PKT ${:.0}→${:.0}, ETH ${:.0}→${:.0}",
                    current_btc, target_btc_usd, current_eth, target_eth_usd))
            }
        }
    }

    pub fn pause(&mut self) { self.safety.paused = true; self.state = AgentState::Paused; }
    pub fn resume(&mut self) { self.safety.paused = false; self.state = AgentState::Idle; }

    pub fn portfolio_value(&self, market: &MarketData) -> f64 {
        self.wallet.total_value_usd(market)
    }
}

// ─── AgentEngine ──────────────────────────────────────────────────────────────

/// Engine chạy nhiều agents trên cùng 1 market feed
pub struct AgentEngine {
    pub agents:  Vec<Agent>,
    pub market:  MarketData,
    pub history: Vec<MarketData>,
}

impl AgentEngine {
    pub fn new() -> Self {
        AgentEngine {
            agents:  vec![],
            market:  MarketData::new(0, 0),
            history: vec![],
        }
    }

    pub fn add_agent(&mut self, agent: Agent) {
        self.agents.push(agent);
    }

    /// Update market và chạy tất cả agents
    pub fn update(&mut self, market: MarketData) -> Vec<(String, TxLog)> {
        self.history.push(self.market.clone());
        self.market = market.clone();

        let mut all_logs = vec![];
        for agent in &mut self.agents {
            let logs = agent.tick(&market);
            for log in logs {
                all_logs.push((agent.name.clone(), log));
            }
        }
        all_logs
    }

    pub fn total_txs(&self) -> usize {
        self.agents.iter().map(|a| a.tx_log.len()).sum()
    }
}
