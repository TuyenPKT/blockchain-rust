#![allow(dead_code)]

/// v2.8 — On-chain Governance
///
/// Kiến trúc:
///
///   Token Holder               Governor Contract          Timelock
///   ─────────────              ─────────────────          ────────
///   propose(action) ─────────► ProposalCreated            Queue
///   vote(id, For)   ─────────► VoteCast                   Execute (after delay)
///   execute(id)     ─────────► ProposalExecuted ─────────► Action applied
///
/// Lifecycle của 1 proposal:
///   Pending → Active → (Succeeded | Defeated | Canceled) → Queued → Executed
///
/// Trust model:
///   - Voting power = token balance (1 token = 1 vote)
///   - Quorum: tổng votes cast ≥ X% tổng supply
///   - Supermajority: For/(For+Against) ≥ threshold (e.g. 60%)
///   - Timelock: delay trước khi execute (phòng governance attack)
///   - Veto: Guardian có thể cancel proposal bất kỳ lúc nào
///
/// Tham khảo: Compound Governor Bravo, OpenZeppelin Governor, Uniswap governance

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

// ─── Constants ────────────────────────────────────────────────────────────────

pub const VOTING_DELAY:    u64 = 1;       // blocks sau propose trước khi vote bắt đầu
pub const VOTING_PERIOD:   u64 = 100;     // blocks vote window
pub const TIMELOCK_DELAY:  u64 = 48;      // blocks delay trước execute (simulated hours)
pub const QUORUM_BPS:      u64 = 400;     // 4% tổng supply (basis points)
pub const THRESHOLD_BPS:   u64 = 6000;    // 60% For votes để thắng

// ─── ProposalState ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProposalState {
    Pending,    // chưa đến voting_start
    Active,     // đang vote
    Succeeded,  // vote passed, chưa queue
    Defeated,   // vote failed
    Canceled,   // bị cancel
    Queued,     // trong timelock
    Executed,   // đã execute
    Expired,    // queued nhưng không execute kịp
}

// ─── ProposalAction ───────────────────────────────────────────────────────────

/// Hành động sẽ được thực thi nếu proposal pass
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalAction {
    /// Thay đổi 1 parameter của protocol
    SetParameter { key: String, value: u64 },
    /// Transfer token từ treasury
    TreasuryTransfer { to: String, amount: u64 },
    /// Thêm/remove guardian
    UpdateGuardian { address: String, add: bool },
    /// Upgrade contract (giả lập)
    UpgradeContract { contract: String, new_impl: String },
    /// Text proposal (signaling only)
    Text { description: String },
}

impl ProposalAction {
    pub fn description(&self) -> String {
        match self {
            ProposalAction::SetParameter { key, value } =>
                format!("SetParameter({} = {})", key, value),
            ProposalAction::TreasuryTransfer { to, amount } =>
                format!("Transfer {} tokens to {}", amount, to),
            ProposalAction::UpdateGuardian { address, add } =>
                format!("{} guardian {}", if *add { "Add" } else { "Remove" }, address),
            ProposalAction::UpgradeContract { contract, new_impl } =>
                format!("Upgrade {} → {}", contract, new_impl),
            ProposalAction::Text { description } =>
                format!("Text: {}", description),
        }
    }
}

// ─── Vote ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VoteChoice { For, Against, Abstain }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub voter:  String,
    pub choice: VoteChoice,
    pub weight: u64,   // voting power tại thời điểm snapshot
    pub reason: String,
}

// ─── Proposal ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id:           u64,
    pub proposer:     String,
    pub title:        String,
    pub description:  String,
    pub actions:      Vec<ProposalAction>,

    pub snapshot_block:  u64,   // block để tính voting power
    pub voting_start:    u64,   // block bắt đầu vote
    pub voting_end:      u64,   // block kết thúc vote
    pub eta:             u64,   // block có thể execute (sau timelock)

    pub votes_for:       u64,
    pub votes_against:   u64,
    pub votes_abstain:   u64,
    pub votes:           Vec<Vote>,

    pub state:           ProposalState,
    pub execution_log:   Vec<String>,
}

impl Proposal {
    pub fn new(
        id: u64,
        proposer: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        actions: Vec<ProposalAction>,
        current_block: u64,
    ) -> Self {
        let voting_start = current_block + VOTING_DELAY;
        let voting_end   = voting_start + VOTING_PERIOD;
        Proposal {
            id,
            proposer:      proposer.into(),
            title:         title.into(),
            description:   description.into(),
            actions,
            snapshot_block:  current_block,
            voting_start,
            voting_end,
            eta:             0,
            votes_for:       0,
            votes_against:   0,
            votes_abstain:   0,
            votes:           vec![],
            state:           ProposalState::Pending,
            execution_log:   vec![],
        }
    }

    pub fn total_votes(&self) -> u64 {
        self.votes_for + self.votes_against + self.votes_abstain
    }

    /// Kiểm tra quorum đạt không (total_cast ≥ quorum_bps% of supply)
    pub fn meets_quorum(&self, total_supply: u64) -> bool {
        let quorum = total_supply * QUORUM_BPS / 10_000;
        self.total_votes() >= quorum
    }

    /// Kiểm tra supermajority: For/(For+Against) ≥ threshold
    pub fn is_supermajority(&self) -> bool {
        let contested = self.votes_for + self.votes_against;
        if contested == 0 { return false; }
        self.votes_for * 10_000 / contested >= THRESHOLD_BPS
    }

    pub fn proposal_hash(&self) -> String {
        let mut h = blake3::Hasher::new();
        h.update(b"proposal_v28");
        h.update(&self.id.to_le_bytes());
        h.update(self.proposer.as_bytes());
        h.update(self.title.as_bytes());
        hex::encode(h.finalize().as_bytes())
    }
}

// ─── TokenLedger ──────────────────────────────────────────────────────────────

/// Sổ cái token đơn giản — balances + delegation
pub struct TokenLedger {
    pub balances:    HashMap<String, u64>,
    pub delegations: HashMap<String, String>,  // delegator → delegate
    pub total_supply: u64,
}

impl TokenLedger {
    pub fn new() -> Self {
        TokenLedger { balances: HashMap::new(), delegations: HashMap::new(), total_supply: 0 }
    }

    pub fn mint(&mut self, to: &str, amount: u64) {
        *self.balances.entry(to.to_string()).or_insert(0) += amount;
        self.total_supply += amount;
    }

    pub fn transfer(&mut self, from: &str, to: &str, amount: u64) -> Result<(), String> {
        let bal = self.balances.get(from).copied().unwrap_or(0);
        if bal < amount {
            return Err(format!("{} has {} < {}", from, bal, amount));
        }
        *self.balances.entry(from.to_string()).or_insert(0) -= amount;
        *self.balances.entry(to.to_string()).or_insert(0) += amount;
        Ok(())
    }

    /// Voting power = sum of balances of all delegators who delegate to `addr` + own balance
    pub fn voting_power(&self, addr: &str) -> u64 {
        let own = self.balances.get(addr).copied().unwrap_or(0);
        let delegated: u64 = self.delegations.iter()
            .filter(|(_, delegate)| delegate.as_str() == addr)
            .map(|(delegator, _)| self.balances.get(delegator).copied().unwrap_or(0))
            .sum();
        own + delegated
    }

    pub fn delegate(&mut self, from: impl Into<String>, to: impl Into<String>) {
        self.delegations.insert(from.into(), to.into());
    }
}

// ─── Timelock ─────────────────────────────────────────────────────────────────

pub struct TimelockQueue {
    /// proposal_id → execute_after_block
    pub queue: HashMap<u64, u64>,
    pub grace_period: u64,  // blocks sau eta trước khi expire
}

impl TimelockQueue {
    pub fn new() -> Self {
        TimelockQueue { queue: HashMap::new(), grace_period: 200 }
    }

    pub fn enqueue(&mut self, proposal_id: u64, current_block: u64) -> u64 {
        let eta = current_block + TIMELOCK_DELAY;
        self.queue.insert(proposal_id, eta);
        eta
    }

    pub fn is_ready(&self, proposal_id: u64, current_block: u64) -> bool {
        match self.queue.get(&proposal_id) {
            Some(&eta) => current_block >= eta && current_block <= eta + self.grace_period,
            None => false,
        }
    }

    pub fn dequeue(&mut self, proposal_id: u64) {
        self.queue.remove(&proposal_id);
    }
}

// ─── ProtocolState ────────────────────────────────────────────────────────────

/// Trạng thái protocol có thể thay đổi qua governance
pub struct ProtocolState {
    pub parameters:   HashMap<String, u64>,
    pub treasury:     HashMap<String, u64>,
    pub guardians:    Vec<String>,
    pub contracts:    HashMap<String, String>,   // name → implementation
    pub change_log:   Vec<String>,
}

impl ProtocolState {
    pub fn new() -> Self {
        let mut params = HashMap::new();
        params.insert("min_stake".to_string(),     1_000);
        params.insert("max_validators".to_string(), 100);
        params.insert("block_reward".to_string(),  2_000_000_000);
        params.insert("tx_fee_base".to_string(),   1_000);

        let mut treasury = HashMap::new();
        treasury.insert("treasury".to_string(), 10_000_000);

        ProtocolState {
            parameters: params,
            treasury,
            guardians: vec!["dao_guardian".to_string()],
            contracts: HashMap::new(),
            change_log: vec![],
        }
    }

    pub fn apply(&mut self, action: &ProposalAction) -> String {
        match action {
            ProposalAction::SetParameter { key, value } => {
                let old = self.parameters.insert(key.clone(), *value).unwrap_or(0);
                let msg = format!("SetParam: {} {} → {}", key, old, value);
                self.change_log.push(msg.clone());
                msg
            }
            ProposalAction::TreasuryTransfer { to, amount } => {
                let balance = self.treasury.entry("treasury".to_string()).or_insert(0);
                if *balance >= *amount {
                    *balance -= amount;
                    *self.treasury.entry(to.clone()).or_insert(0) += amount;
                    let msg = format!("Treasury: {} tokens → {}", amount, to);
                    self.change_log.push(msg.clone());
                    msg
                } else {
                    "Treasury: insufficient funds".to_string()
                }
            }
            ProposalAction::UpdateGuardian { address, add } => {
                if *add {
                    if !self.guardians.contains(address) {
                        self.guardians.push(address.clone());
                    }
                    let msg = format!("Guardian added: {}", address);
                    self.change_log.push(msg.clone());
                    msg
                } else {
                    self.guardians.retain(|g| g != address);
                    let msg = format!("Guardian removed: {}", address);
                    self.change_log.push(msg.clone());
                    msg
                }
            }
            ProposalAction::UpgradeContract { contract, new_impl } => {
                self.contracts.insert(contract.clone(), new_impl.clone());
                let msg = format!("Upgraded {} → {}", contract, new_impl);
                self.change_log.push(msg.clone());
                msg
            }
            ProposalAction::Text { description } => {
                let msg = format!("Text proposal recorded: {}", description);
                self.change_log.push(msg.clone());
                msg
            }
        }
    }
}

// ─── Governor ─────────────────────────────────────────────────────────────────

pub struct Governor {
    pub proposals:      HashMap<u64, Proposal>,
    pub token:          TokenLedger,
    pub timelock:       TimelockQueue,
    pub protocol:       ProtocolState,
    pub next_id:        u64,
    pub current_block:  u64,
    pub proposal_threshold: u64,  // min voting power để propose
}

impl Governor {
    pub fn new() -> Self {
        Governor {
            proposals: HashMap::new(),
            token:     TokenLedger::new(),
            timelock:  TimelockQueue::new(),
            protocol:  ProtocolState::new(),
            next_id:   1,
            current_block: 0,
            proposal_threshold: 10_000,  // cần 10,000 tokens để propose
        }
    }

    pub fn advance_block(&mut self, n: u64) {
        self.current_block += n;
        // Cập nhật state của tất cả proposals theo block mới
        let block = self.current_block;
        let supply = self.token.total_supply;
        for p in self.proposals.values_mut() {
            p.state = Self::compute_state(p, block, supply);
        }
    }

    fn compute_state(p: &Proposal, block: u64, total_supply: u64) -> ProposalState {
        match p.state {
            ProposalState::Executed | ProposalState::Canceled => p.state.clone(),
            ProposalState::Queued => {
                // Giữ Queued — expire check nằm trong execute()
                ProposalState::Queued
            }
            _ => {
                if block < p.voting_start {
                    ProposalState::Pending
                } else if block <= p.voting_end {
                    ProposalState::Active
                } else {
                    // Voting ended — tính kết quả
                    if p.meets_quorum(total_supply) && p.is_supermajority() {
                        ProposalState::Succeeded
                    } else {
                        ProposalState::Defeated
                    }
                }
            }
        }
    }

    /// Tạo proposal mới
    pub fn propose(
        &mut self,
        proposer: &str,
        title: &str,
        description: &str,
        actions: Vec<ProposalAction>,
    ) -> Result<u64, String> {
        let power = self.token.voting_power(proposer);
        if power < self.proposal_threshold {
            return Err(format!(
                "Voting power {} < threshold {}", power, self.proposal_threshold
            ));
        }
        let id = self.next_id;
        self.next_id += 1;
        let proposal = Proposal::new(id, proposer, title, description, actions, self.current_block);
        self.proposals.insert(id, proposal);
        Ok(id)
    }

    /// Cast vote
    pub fn cast_vote(
        &mut self,
        proposal_id: u64,
        voter: &str,
        choice: VoteChoice,
        reason: &str,
    ) -> Result<u64, String> {
        let block   = self.current_block;
        let supply  = self.token.total_supply;
        let power   = self.token.voting_power(voter);

        let p = self.proposals.get_mut(&proposal_id)
            .ok_or_else(|| format!("Proposal {} not found", proposal_id))?;

        // Re-compute state
        p.state = Self::compute_state(p, block, supply);

        if p.state != ProposalState::Active {
            return Err(format!("Proposal not active (state: {:?})", p.state));
        }
        if p.votes.iter().any(|v| v.voter == voter) {
            return Err(format!("{} already voted", voter));
        }

        match choice {
            VoteChoice::For     => p.votes_for     += power,
            VoteChoice::Against => p.votes_against += power,
            VoteChoice::Abstain => p.votes_abstain += power,
        }
        p.votes.push(Vote { voter: voter.to_string(), choice, weight: power, reason: reason.to_string() });
        Ok(power)
    }

    /// Queue proposal vào timelock (sau khi Succeeded)
    pub fn queue(&mut self, proposal_id: u64) -> Result<u64, String> {
        let block  = self.current_block;
        let supply = self.token.total_supply;

        let p = self.proposals.get_mut(&proposal_id)
            .ok_or_else(|| format!("Proposal {} not found", proposal_id))?;

        p.state = Self::compute_state(p, block, supply);
        if p.state != ProposalState::Succeeded {
            return Err(format!("Cannot queue — state: {:?}", p.state));
        }

        let eta = self.timelock.enqueue(proposal_id, block);
        let p = self.proposals.get_mut(&proposal_id).unwrap();
        p.eta   = eta;
        p.state = ProposalState::Queued;
        Ok(eta)
    }

    /// Execute proposal (sau timelock delay)
    pub fn execute(&mut self, proposal_id: u64) -> Result<Vec<String>, String> {
        let block = self.current_block;

        let p = self.proposals.get(&proposal_id)
            .ok_or_else(|| format!("Proposal {} not found", proposal_id))?;

        if p.state != ProposalState::Queued {
            return Err(format!("Not queued — state: {:?}", p.state));
        }

        if !self.timelock.is_ready(proposal_id, block) {
            let eta = p.eta;
            if block < eta {
                return Err(format!("Timelock not expired — {} blocks remaining", eta - block));
            } else {
                // Grace period expired
                self.proposals.get_mut(&proposal_id).unwrap().state = ProposalState::Expired;
                return Err("Grace period expired".to_string());
            }
        }

        // Execute all actions
        let actions = self.proposals[&proposal_id].actions.clone();
        let mut logs = vec![];
        for action in &actions {
            let log = self.protocol.apply(action);
            logs.push(log);
        }

        self.timelock.dequeue(proposal_id);
        let p = self.proposals.get_mut(&proposal_id).unwrap();
        p.state = ProposalState::Executed;
        p.execution_log = logs.clone();
        Ok(logs)
    }

    /// Guardian cancel proposal
    pub fn cancel(&mut self, proposal_id: u64, caller: &str) -> Result<(), String> {
        if !self.protocol.guardians.contains(&caller.to_string()) {
            return Err(format!("{} is not a guardian", caller));
        }
        let p = self.proposals.get_mut(&proposal_id)
            .ok_or_else(|| format!("Proposal {} not found", proposal_id))?;
        if p.state == ProposalState::Executed {
            return Err("Cannot cancel executed proposal".to_string());
        }
        p.state = ProposalState::Canceled;
        Ok(())
    }

    pub fn proposal_state(&mut self, proposal_id: u64) -> Option<ProposalState> {
        let block  = self.current_block;
        let supply = self.token.total_supply;
        let p = self.proposals.get_mut(&proposal_id)?;
        p.state = Self::compute_state(p, block, supply);
        Some(p.state.clone())
    }

    // ── v10.6 — Persistence helpers ──────────────────────────────────────────

    /// Capture mutable state into a serializable snapshot.
    pub fn snapshot(&self) -> GovernanceSnapshot {
        GovernanceSnapshot {
            proposals:          self.proposals.clone(),
            token_balances:     self.token.balances.clone(),
            token_delegations:  self.token.delegations.clone(),
            token_total_supply: self.token.total_supply,
            timelock_queue:     self.timelock.queue
                .iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            timelock_grace_period: self.timelock.grace_period,
            protocol_params:    self.protocol.parameters.clone(),
            protocol_treasury:  self.protocol.treasury.clone(),
            protocol_guardians: self.protocol.guardians.clone(),
            protocol_contracts: self.protocol.contracts.clone(),
            protocol_change_log: self.protocol.change_log.clone(),
            next_id:            self.next_id,
            current_block:      self.current_block,
            proposal_threshold: self.proposal_threshold,
        }
    }

    /// Restore a `Governor` from a persisted snapshot.
    pub fn from_snapshot(s: GovernanceSnapshot) -> Self {
        let timelock_queue: HashMap<u64, u64> = s.timelock_queue
            .into_iter()
            .filter_map(|(k, v)| k.parse::<u64>().ok().map(|id| (id, v)))
            .collect();

        let mut token = TokenLedger::new();
        token.balances     = s.token_balances;
        token.delegations  = s.token_delegations;
        token.total_supply = s.token_total_supply;

        let mut timelock = TimelockQueue::new();
        timelock.queue        = timelock_queue;
        timelock.grace_period = s.timelock_grace_period;

        let mut protocol = ProtocolState::new();
        protocol.parameters = s.protocol_params;
        protocol.treasury   = s.protocol_treasury;
        protocol.guardians  = s.protocol_guardians;
        protocol.contracts  = s.protocol_contracts;
        protocol.change_log = s.protocol_change_log;

        Governor {
            proposals:          s.proposals,
            token,
            timelock,
            protocol,
            next_id:            s.next_id,
            current_block:      s.current_block,
            proposal_threshold: s.proposal_threshold,
        }
    }
}

// ─── GovernanceSnapshot ───────────────────────────────────────────────────────

/// v10.6 — Serializable snapshot of all mutable Governor state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceSnapshot {
    pub proposals:           HashMap<u64, Proposal>,
    pub token_balances:      HashMap<String, u64>,
    pub token_delegations:   HashMap<String, String>,
    pub token_total_supply:  u64,
    /// Timelock queue as String keys (JSON requires string keys for maps).
    pub timelock_queue:      HashMap<String, u64>,
    pub timelock_grace_period: u64,
    pub protocol_params:     HashMap<String, u64>,
    pub protocol_treasury:   HashMap<String, u64>,
    pub protocol_guardians:  Vec<String>,
    pub protocol_contracts:  HashMap<String, String>,
    pub protocol_change_log: Vec<String>,
    pub next_id:             u64,
    pub current_block:       u64,
    pub proposal_threshold:  u64,
}
