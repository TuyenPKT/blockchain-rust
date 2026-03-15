#![allow(dead_code)]

/// v3.4 — Self-Amending Chain (On-chain Protocol Upgrade)
///
/// Inspired by Tezos: the chain can upgrade its own rules through on-chain voting,
/// without hard forks. Each amendment goes through a governance cycle.
///
/// ─── Amendment Lifecycle ─────────────────────────────────────────────────────
///
///   PROPOSAL  →  VOTING  →  COOLDOWN  →  ACTIVE
///      ↓             ↓
///   Expired       Rejected
///
///   Proposal  (PROPOSAL_PERIOD  blocks): Proposer submits, others endorse
///   Voting    (VOTING_PERIOD    blocks): Stakers vote for/against
///   Cooldown  (COOLDOWN_PERIOD  blocks): Testing delay before activation
///   Active:   Protocol switch takes effect
///
/// ─── Amendment Types ──────────────────────────────────────────────────────────
///
///   ParameterChange:  Modify a single protocol param (gas limit, block size…)
///   ProtocolUpgrade:  Bump version + new code hash (full upgrade)
///   EmergencyFix:     Short cycle for critical security patches
///
/// ─── Passing Threshold ───────────────────────────────────────────────────────
///
///   Quorum:      ≥ QUORUM_BPS/10000 of total stake must participate
///   Supermajority: ≥ SUPERMAJ_BPS/10000 of participating stake in favor
///
/// ─── Self-Amendment vs. Hard Fork ────────────────────────────────────────────
///
///   Hard fork: out-of-band coordination, risk of chain split
///   Self-amendment: on-chain, automatic, coordinated, no split
///   Trade-off: slightly slower (must wait governance cycle)
///
/// Inspired by: Tezos Babylon/Athens upgrades, Cosmos governance module

use sha2::{Sha256, Digest};
use std::collections::HashMap;

// ─── Constants ────────────────────────────────────────────────────────────────

pub const PROPOSAL_PERIOD:  u64 = 5;   // blocks (real Tezos: ~2 weeks)
pub const VOTING_PERIOD:    u64 = 10;  // blocks
pub const COOLDOWN_PERIOD:  u64 = 3;   // blocks
pub const EMERGENCY_PERIOD: u64 = 3;   // blocks (emergency fast-track)

pub const QUORUM_BPS:   u32 = 2000; // 20% of total stake must vote
pub const SUPERMAJ_BPS: u32 = 6700; // 67% of participating stake in favor

// ─── Protocol Version ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Version { major, minor, patch }
    }
    pub fn bump_minor(&self) -> Self {
        Version { major: self.major, minor: self.minor + 1, patch: 0 }
    }
    pub fn bump_patch(&self) -> Self {
        Version { major: self.major, minor: self.minor, patch: self.patch + 1 }
    }
    pub fn to_string(&self) -> String {
        format!("v{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ─── Protocol Parameters ──────────────────────────────────────────────────────

/// All parameters that can be changed through self-amendment
#[derive(Clone, Debug)]
pub struct ProtocolParams {
    pub max_block_size:   u64,   // bytes
    pub max_tx_per_block: u32,
    pub block_time_secs:  u64,
    pub min_stake:        u64,   // min stake to be a validator
    pub gas_limit:        u64,
    pub base_fee:         u64,   // satoshis/gas
    pub max_validators:   u32,
}

impl ProtocolParams {
    pub fn genesis() -> Self {
        ProtocolParams {
            max_block_size:   1_000_000,
            max_tx_per_block: 2000,
            block_time_secs:  12,
            min_stake:        32_000,
            gas_limit:        15_000_000,
            base_fee:         1000,
            max_validators:   100,
        }
    }

    /// Apply a parameter change, return old value
    pub fn apply_change(&mut self, key: &str, new_val: i64) -> Result<i64, String> {
        let old = match key {
            "max_block_size"   => { let o = self.max_block_size as i64;   self.max_block_size   = new_val as u64; o }
            "max_tx_per_block" => { let o = self.max_tx_per_block as i64; self.max_tx_per_block = new_val as u32; o }
            "block_time_secs"  => { let o = self.block_time_secs as i64;  self.block_time_secs  = new_val as u64; o }
            "min_stake"        => { let o = self.min_stake as i64;        self.min_stake        = new_val as u64; o }
            "gas_limit"        => { let o = self.gas_limit as i64;        self.gas_limit        = new_val as u64; o }
            "base_fee"         => { let o = self.base_fee as i64;         self.base_fee         = new_val as u64; o }
            "max_validators"   => { let o = self.max_validators as i64;   self.max_validators   = new_val as u32; o }
            _ => return Err(format!("Unknown parameter: {}", key)),
        };
        Ok(old)
    }

    pub fn get(&self, key: &str) -> Option<i64> {
        match key {
            "max_block_size"   => Some(self.max_block_size   as i64),
            "max_tx_per_block" => Some(self.max_tx_per_block as i64),
            "block_time_secs"  => Some(self.block_time_secs  as i64),
            "min_stake"        => Some(self.min_stake        as i64),
            "gas_limit"        => Some(self.gas_limit        as i64),
            "base_fee"         => Some(self.base_fee         as i64),
            "max_validators"   => Some(self.max_validators   as i64),
            _ => None,
        }
    }
}

// ─── Amendment Kind ───────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum AmendmentKind {
    /// Change a single protocol parameter
    ParamChange {
        param:     String,
        new_value: i64,
        rationale: String,
    },
    /// Full protocol upgrade: bump version + new code
    ProtocolUpgrade {
        new_version: Version,
        code_hash:   [u8; 32],  // hash of new protocol bytecode
        notes:       String,
    },
    /// Emergency security fix: short voting cycle
    EmergencyFix {
        patch_hash: [u8; 32],
        reason:     String,
    },
}

impl AmendmentKind {
    pub fn label(&self) -> &str {
        match self {
            AmendmentKind::ParamChange { .. }     => "ParameterChange",
            AmendmentKind::ProtocolUpgrade { .. } => "ProtocolUpgrade",
            AmendmentKind::EmergencyFix { .. }    => "EmergencyFix",
        }
    }

    pub fn is_emergency(&self) -> bool {
        matches!(self, AmendmentKind::EmergencyFix { .. })
    }
}

// ─── Amendment Phase ──────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Proposal,  // accepting endorsements, not yet to vote
    Voting,    // active vote
    Cooldown,  // passed voting, waiting for activation block
    Active,    // amendment is live
    Rejected,  // failed quorum or supermajority
    Expired,   // proposal period ended without enough endorsements
}

impl Phase {
    pub fn label(&self) -> &str {
        match self {
            Phase::Proposal  => "Proposal",
            Phase::Voting    => "Voting",
            Phase::Cooldown  => "Cooldown",
            Phase::Active    => "Active",
            Phase::Rejected  => "Rejected",
            Phase::Expired   => "Expired",
        }
    }
}

// ─── Vote ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct VoteRecord {
    pub voter:    String,
    pub stake:    u64,
    pub in_favor: bool,
    pub block:    u64,
}

// ─── Amendment ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Amendment {
    pub id:           [u8; 32],
    pub proposer:     String,
    pub kind:         AmendmentKind,
    pub proposed_at:  u64,
    pub voting_start: u64,
    pub voting_end:   u64,
    pub activate_at:  u64,
    pub phase:        Phase,
    pub endorsers:    Vec<String>,    // proposal-period endorsements
    pub votes:        Vec<VoteRecord>,
}

impl Amendment {
    pub fn votes_for(&self) -> u64 {
        self.votes.iter().filter(|v| v.in_favor).map(|v| v.stake).sum()
    }
    pub fn votes_against(&self) -> u64 {
        self.votes.iter().filter(|v| !v.in_favor).map(|v| v.stake).sum()
    }
    pub fn participation(&self) -> u64 {
        self.votes_for() + self.votes_against()
    }

    /// Has enough quorum and supermajority to pass?
    pub fn passes(&self, total_stake: u64) -> bool {
        let participation = self.participation();
        if total_stake == 0 { return false; }
        let quorum_ok = participation * 10000 >= total_stake as u64 * QUORUM_BPS as u64;
        if !quorum_ok { return false; }
        let for_bps = self.votes_for() * 10000 / participation;
        for_bps >= SUPERMAJ_BPS as u64
    }

    pub fn id_hex(&self) -> String {
        hex::encode(&self.id[..4])
    }
}

// ─── Upgrade History ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct UpgradeRecord {
    pub block:    u64,
    pub old_ver:  Version,
    pub new_ver:  Version,
    pub kind:     String,
    pub proposer: String,
    pub id:       [u8; 32],
}

// ─── Self-Amending Chain ──────────────────────────────────────────────────────

pub struct SelfAmendChain {
    pub block:      u64,
    pub version:    Version,
    pub params:     ProtocolParams,
    pub code_hash:  [u8; 32],           // hash of current protocol code
    pub stakers:    HashMap<String, u64>, // address → stake
    pub amendments: Vec<Amendment>,
    pub history:    Vec<UpgradeRecord>,
    pub events:     Vec<String>,         // audit log
}

impl SelfAmendChain {
    pub fn new() -> Self {
        let code_hash: [u8; 32] = {
            let mut sha = Sha256::new();
            sha.update(b"genesis_protocol_v1.0.0");
            let out = sha.finalize();
            let mut r = [0u8; 32]; r.copy_from_slice(&out); r
        };

        let mut chain = SelfAmendChain {
            block:      0,
            version:    Version::new(1, 0, 0),
            params:     ProtocolParams::genesis(),
            code_hash,
            stakers:    HashMap::new(),
            amendments: Vec::new(),
            history:    Vec::new(),
            events:     Vec::new(),
        };
        chain.log("Chain genesis: protocol v1.0.0");
        chain
    }

    pub fn stake(&mut self, addr: &str, amount: u64) {
        *self.stakers.entry(addr.to_string()).or_insert(0) += amount;
        self.log(&format!("Stake: {} staked {} (total={})", addr, amount, self.stakers[addr]));
    }

    pub fn total_stake(&self) -> u64 {
        self.stakers.values().sum()
    }

    /// Submit a new amendment proposal
    pub fn propose(&mut self, proposer: &str, kind: AmendmentKind) -> Result<[u8; 32], String> {
        let proposer_stake = *self.stakers.get(proposer).unwrap_or(&0);
        if proposer_stake == 0 {
            return Err(format!("{} has no stake — cannot propose", proposer));
        }

        let is_emergency = kind.is_emergency();
        let voting_period = if is_emergency { EMERGENCY_PERIOD } else { VOTING_PERIOD };

        let voting_start  = self.block + PROPOSAL_PERIOD;
        let voting_end    = voting_start + voting_period;
        let activate_at   = voting_end + COOLDOWN_PERIOD;

        // Amendment ID = H(proposer || block || kind_label)
        let id = self.make_id(proposer, self.block, kind.label());

        let amendment = Amendment {
            id,
            proposer: proposer.to_string(),
            kind,
            proposed_at:  self.block,
            voting_start,
            voting_end,
            activate_at,
            phase:        Phase::Proposal,
            endorsers:    vec![proposer.to_string()],
            votes:        Vec::new(),
        };

        self.log(&format!("[block {}] {} proposed {} (id={}...)",
            self.block, proposer, amendment.kind.label(), amendment.id_hex()));

        self.amendments.push(amendment);
        Ok(id)
    }

    /// Endorse a proposal during Proposal phase
    pub fn endorse(&mut self, amendment_id: &[u8; 32], endorser: &str) -> Result<(), String> {
        let stake = *self.stakers.get(endorser).unwrap_or(&0);
        if stake == 0 {
            return Err(format!("{} has no stake", endorser));
        }
        let amendment = self.amendments.iter_mut()
            .find(|a| a.id == *amendment_id)
            .ok_or("Amendment not found")?;
        if amendment.phase != Phase::Proposal {
            return Err(format!("Amendment {} is not in Proposal phase", amendment.id_hex()));
        }
        if amendment.endorsers.contains(&endorser.to_string()) {
            return Err(format!("{} already endorsed", endorser));
        }
        amendment.endorsers.push(endorser.to_string());
        self.log(&format!("[block {}] {} endorsed amendment {}", self.block, endorser, hex::encode(&amendment_id[..4])));
        Ok(())
    }

    /// Cast a vote during Voting phase
    pub fn vote(&mut self, amendment_id: &[u8; 32], voter: &str, in_favor: bool) -> Result<(), String> {
        let stake = *self.stakers.get(voter).unwrap_or(&0);
        if stake == 0 {
            return Err(format!("{} has no stake", voter));
        }

        let block = self.block;
        let amendment = self.amendments.iter_mut()
            .find(|a| a.id == *amendment_id)
            .ok_or("Amendment not found")?;

        if amendment.phase != Phase::Voting {
            return Err(format!("Amendment {} is not in Voting phase (current: {})",
                amendment.id_hex(), amendment.phase.label()));
        }
        if amendment.votes.iter().any(|v| v.voter == voter) {
            return Err(format!("{} already voted", voter));
        }

        amendment.votes.push(VoteRecord {
            voter: voter.to_string(), stake, in_favor, block
        });
        let id_hex = amendment.id_hex();
        let dir = if in_favor { "FOR" } else { "AGAINST" };
        self.log(&format!("[block {}] {} voted {} (stake={}) on {}",
            block, voter, dir, stake, id_hex));
        Ok(())
    }

    /// Advance chain by n blocks, processing state transitions
    pub fn advance(&mut self, n: u64) {
        for _ in 0..n {
            self.block += 1;
            self.process_amendments();
        }
    }

    fn process_amendments(&mut self) {
        let block = self.block;
        let total_stake = self.total_stake();

        // Collect indices of amendments to activate (to avoid borrow issues)
        let mut to_activate: Vec<usize> = Vec::new();

        for (idx, a) in self.amendments.iter_mut().enumerate() {
            match a.phase {
                Phase::Proposal => {
                    if block >= a.voting_start {
                        // Transition to Voting
                        a.phase = Phase::Voting;
                        // Note: log after loop to avoid borrow conflict
                        to_activate.push(idx * 1000 + 1); // encoding for logging
                    }
                }
                Phase::Voting => {
                    if block >= a.voting_end {
                        if a.passes(total_stake) {
                            a.phase = Phase::Cooldown;
                            to_activate.push(idx * 1000 + 2);
                        } else {
                            a.phase = Phase::Rejected;
                            to_activate.push(idx * 1000 + 3);
                        }
                    }
                }
                Phase::Cooldown => {
                    if block >= a.activate_at {
                        a.phase = Phase::Active;
                        to_activate.push(idx * 1000 + 4);
                    }
                }
                _ => {}
            }
        }

        // Process activations
        for code in to_activate {
            let idx = code / 1000;
            let event = code % 1000;
            let a = &self.amendments[idx];
            match event {
                1 => self.events.push(format!("[block {}] Amendment {} → Voting", block, a.id_hex())),
                2 => {
                    let voted_for = a.votes_for();
                    let participation = a.participation();
                    let pct = if participation > 0 { voted_for * 100 / participation } else { 0 };
                    self.events.push(format!("[block {}] Amendment {} passed voting ({}/{} = {}%) → Cooldown",
                        block, a.id_hex(), voted_for, participation, pct));
                }
                3 => {
                    let voted_for = a.votes_for();
                    let participation = a.participation();
                    let pct = if participation > 0 { voted_for * 100 / participation } else { 0 };
                    self.events.push(format!("[block {}] Amendment {} REJECTED ({}/{} = {}%)",
                        block, a.id_hex(), voted_for, participation, pct));
                }
                4 => {
                    self.events.push(format!("[block {}] Activating amendment {}...", block, a.id_hex()));
                    // Will apply below
                    let a_clone = a.clone();
                    self.apply_amendment(&a_clone);
                }
                _ => {}
            }
        }
    }

    fn apply_amendment(&mut self, a: &Amendment) {
        let old_version = self.version.clone();

        match &a.kind {
            AmendmentKind::ParamChange { param, new_value, rationale: _ } => {
                if let Ok(old) = self.params.apply_change(param, *new_value) {
                    let new_ver = self.version.bump_patch();
                    self.history.push(UpgradeRecord {
                        block:    self.block,
                        old_ver:  old_version.clone(),
                        new_ver:  new_ver.clone(),
                        kind:     format!("param {} changed: {} → {}", param, old, new_value),
                        proposer: a.proposer.clone(),
                        id:       a.id,
                    });
                    self.version = new_ver;
                    self.log(&format!("[block {}] ACTIVATED: {} {} → {} (now {})",
                        self.block, param, old, new_value, self.version.to_string()));
                }
            }
            AmendmentKind::ProtocolUpgrade { new_version, code_hash, notes } => {
                self.version   = new_version.clone();
                self.code_hash = *code_hash;
                self.history.push(UpgradeRecord {
                    block:    self.block,
                    old_ver:  old_version.clone(),
                    new_ver:  new_version.clone(),
                    kind:     format!("upgrade: {}", notes),
                    proposer: a.proposer.clone(),
                    id:       a.id,
                });
                self.log(&format!("[block {}] ACTIVATED: protocol upgrade {} → {} (code={}...)",
                    self.block, old_version.to_string(), new_version.to_string(),
                    hex::encode(&code_hash[..4])));
            }
            AmendmentKind::EmergencyFix { patch_hash, reason } => {
                let new_ver = self.version.bump_patch();
                self.code_hash = *patch_hash;
                self.history.push(UpgradeRecord {
                    block:    self.block,
                    old_ver:  old_version.clone(),
                    new_ver:  new_ver.clone(),
                    kind:     format!("emergency: {}", reason),
                    proposer: a.proposer.clone(),
                    id:       a.id,
                });
                self.version = new_ver;
                self.log(&format!("[block {}] EMERGENCY FIX applied: {} → {} reason={}",
                    self.block, old_version.to_string(), self.version.to_string(), reason));
            }
        }
    }

    pub fn amendment_state(&self, id: &[u8; 32]) -> Option<&Amendment> {
        self.amendments.iter().find(|a| a.id == *id)
    }

    fn make_id(&self, proposer: &str, block: u64, label: &str) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"amendment_id");
        h.update(proposer.as_bytes());
        h.update(&block.to_le_bytes());
        h.update(label.as_bytes());
        let out = h.finalize();
        let mut r = [0u8; 32];
        r.copy_from_slice(&out);
        r
    }

    fn log(&mut self, msg: &str) {
        self.events.push(msg.to_string());
    }

    pub fn print_events_since(&self, from_idx: usize) {
        for e in &self.events[from_idx..] {
            println!("  {}", e);
        }
    }
}

// ─── Utility ──────────────────────────────────────────────────────────────────

pub fn make_code_hash(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    let out = h.finalize();
    let mut r = [0u8; 32];
    r.copy_from_slice(&out);
    r
}
