#![allow(dead_code)]

/// v2.0 — BFT Consensus: Tendermint-style
///
/// Tendermint consensus gồm 4 bước lặp lại mỗi height:
///
///   ┌──────────────────────────────────────────────────────────┐
///   │  HEIGHT h, ROUND r                                       │
///   │                                                          │
///   │  [PROPOSE]    Proposer broadcast Proposal{block}        │
///   │      ↓                                                   │
///   │  [PREVOTE]    Mỗi validator: Prevote(block) hoặc nil    │
///   │      ↓  (2/3+ prevotes for block)                       │
///   │  [PRECOMMIT]  Mỗi validator: Precommit(block) hoặc nil  │
///   │      ↓  (2/3+ precommits for block)                     │
///   │  [COMMIT]     Block được commit, height += 1            │
///   │                                                          │
///   │  Nếu timeout → round += 1, proposer mới                 │
///   └──────────────────────────────────────────────────────────┘
///
/// Tính chất:
///   - Safety: không bao giờ commit 2 block khác nhau cùng height (BFT)
///   - Liveness: tiến trình tiếp tục miễn < 1/3 validators faulty
///   - Instant finality: block committed = finalized (không cần xác nhận thêm)
///   - Lock mechanism: validator không vote block khác sau khi đã lock

use sha2::{Sha256, Digest};
use std::collections::HashMap;

// ─── Step / Phase ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ConsensusStep {
    Propose,
    Prevote,
    Precommit,
    Commit,
}

impl std::fmt::Display for ConsensusStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsensusStep::Propose    => write!(f, "PROPOSE"),
            ConsensusStep::Prevote    => write!(f, "PREVOTE"),
            ConsensusStep::Precommit  => write!(f, "PRECOMMIT"),
            ConsensusStep::Commit     => write!(f, "COMMIT"),
        }
    }
}

// ─── VoteType ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum VoteType { Prevote, Precommit }

// ─── Vote ────────────────────────────────────────────────────────────────────

/// Phiếu bầu của 1 validator tại 1 round
#[derive(Debug, Clone)]
pub struct Vote {
    pub vote_type:   VoteType,
    pub height:      u64,
    pub round:       u32,
    pub block_hash:  Option<String>,  // None = nil vote
    pub validator:   String,
    pub signature:   Vec<u8>,
}

impl Vote {
    pub fn new(
        vote_type: VoteType,
        height: u64, round: u32,
        block_hash: Option<String>,
        validator: &str,
    ) -> Self {
        let sig = Self::sign_vote(&vote_type, height, round, &block_hash, validator);
        Vote { vote_type, height, round, block_hash, validator: validator.to_string(), signature: sig }
    }

    fn sign_vote(vt: &VoteType, height: u64, round: u32, hash: &Option<String>, validator: &str) -> Vec<u8> {
        let mut h = Sha256::new();
        h.update(validator.as_bytes());
        h.update(height.to_le_bytes());
        h.update(round.to_le_bytes());
        match vt { VoteType::Prevote => h.update(b"prevote"), VoteType::Precommit => h.update(b"precommit") };
        h.update(hash.as_deref().unwrap_or("nil").as_bytes());
        h.finalize().to_vec()
    }

    pub fn verify(&self) -> bool {
        self.signature == Self::sign_vote(
            &self.vote_type, self.height, self.round,
            &self.block_hash, &self.validator,
        )
    }

    pub fn is_nil(&self) -> bool { self.block_hash.is_none() }
}

// ─── Proposal ────────────────────────────────────────────────────────────────

/// Block proposal do proposer broadcast ở đầu mỗi round
#[derive(Debug, Clone)]
pub struct Proposal {
    pub height:    u64,
    pub round:     u32,
    pub proposer:  String,
    pub block:     BftBlock,
    pub signature: Vec<u8>,
}

impl Proposal {
    pub fn new(height: u64, round: u32, proposer: &str, block: BftBlock) -> Self {
        let sig = Self::sign(height, round, proposer, &block.hash);
        Proposal { height, round, proposer: proposer.to_string(), block, signature: sig }
    }

    fn sign(height: u64, round: u32, proposer: &str, block_hash: &str) -> Vec<u8> {
        let mut h = Sha256::new();
        h.update(b"proposal_v20");
        h.update(proposer.as_bytes());
        h.update(height.to_le_bytes());
        h.update(round.to_le_bytes());
        h.update(block_hash.as_bytes());
        h.finalize().to_vec()
    }

    pub fn verify(&self) -> bool {
        self.signature == Self::sign(self.height, self.round, &self.proposer, &self.block.hash)
    }
}

// ─── BftBlock ────────────────────────────────────────────────────────────────

/// Block trong BFT chain — không cần PoW, được commit bởi 2/3+ votes
#[derive(Debug, Clone)]
pub struct BftBlock {
    pub height:     u64,
    pub round:      u32,      // round nào block được commit
    pub prev_hash:  String,
    pub proposer:   String,
    pub payload:    String,
    pub hash:       String,

    /// Precommits đã thu thập (commit certificate)
    pub commit_votes: Vec<Vote>,
}

impl BftBlock {
    pub fn new(height: u64, round: u32, prev_hash: &str, proposer: &str, payload: &str) -> Self {
        let mut b = BftBlock {
            height, round,
            prev_hash: prev_hash.to_string(),
            proposer:  proposer.to_string(),
            payload:   payload.to_string(),
            hash:      String::new(),
            commit_votes: vec![],
        };
        b.hash = b.compute_hash();
        b
    }

    fn compute_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(b"bft_v20");
        h.update(self.height.to_le_bytes());
        h.update(self.round.to_le_bytes());
        h.update(self.prev_hash.as_bytes());
        h.update(self.proposer.as_bytes());
        h.update(self.payload.as_bytes());
        hex::encode(h.finalize())
    }
}

// ─── ValidatorInfo ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ValidatorInfo {
    pub address:    String,
    pub power:      u64,    // voting power (stake)
    pub is_active:  bool,
    pub is_faulty:  bool,   // Byzantine / offline
}

impl ValidatorInfo {
    pub fn new(address: impl Into<String>, power: u64) -> Self {
        ValidatorInfo { address: address.into(), power, is_active: true, is_faulty: false }
    }
}

// ─── ValidatorSet ─────────────────────────────────────────────────────────────

pub struct BftValidatorSet {
    pub validators: Vec<ValidatorInfo>,
}

impl BftValidatorSet {
    pub fn new() -> Self { BftValidatorSet { validators: vec![] } }

    pub fn add(&mut self, v: ValidatorInfo) { self.validators.push(v); }

    pub fn total_power(&self) -> u64 {
        self.validators.iter().filter(|v| v.is_active).map(|v| v.power).sum()
    }

    /// Chọn proposer cho round theo round-robin (sorted by address)
    pub fn proposer_for_round(&self, height: u64, round: u32) -> Option<&ValidatorInfo> {
        let active: Vec<&ValidatorInfo> = self.validators.iter()
            .filter(|v| v.is_active && !v.is_faulty)
            .collect();
        if active.is_empty() { return None; }
        let idx = ((height + round as u64) as usize) % active.len();
        Some(active[idx])
    }

    pub fn active_honest(&self) -> Vec<&ValidatorInfo> {
        self.validators.iter().filter(|v| v.is_active && !v.is_faulty).collect()
    }

    /// Tổng power của tập validators đã vote cho 1 block hash
    pub fn power_for(&self, votes: &[Vote], block_hash: &str) -> u64 {
        votes.iter()
            .filter(|v| v.verify() && v.block_hash.as_deref() == Some(block_hash))
            .filter_map(|v| self.validators.iter().find(|val| val.address == v.validator))
            .map(|val| val.power)
            .sum()
    }

    /// Có ≥ 2/3 power vote cho block_hash không?
    pub fn has_quorum(&self, votes: &[Vote], block_hash: &str) -> bool {
        let voted = self.power_for(votes, block_hash);
        voted * 3 >= self.total_power() * 2
    }
}

// ─── RoundState ──────────────────────────────────────────────────────────────

/// Trạng thái của 1 round trong consensus
#[derive(Debug)]
pub struct RoundState {
    pub height:       u64,
    pub round:        u32,
    pub step:         ConsensusStep,
    pub proposal:     Option<Proposal>,
    pub prevotes:     Vec<Vote>,
    pub precommits:   Vec<Vote>,
    pub locked_block: Option<String>,   // hash của block đang bị locked
}

impl RoundState {
    pub fn new(height: u64, round: u32) -> Self {
        RoundState {
            height, round,
            step:         ConsensusStep::Propose,
            proposal:     None,
            prevotes:     vec![],
            precommits:   vec![],
            locked_block: None,
        }
    }
}

// ─── ConsensusResult ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ConsensusResult {
    /// Committed block + round dùng
    Committed { block: BftBlock, round: u32, precommit_count: usize },
    /// Timeout — không đủ votes, chuyển round tiếp theo
    RoundTimeout { height: u64, round: u32, reason: String },
}

// ─── TendermintEngine ────────────────────────────────────────────────────────

/// Engine chạy 1 vòng Tendermint consensus cho 1 height
pub struct TendermintEngine<'a> {
    pub validator_set: &'a BftValidatorSet,
    pub height:        u64,
    pub prev_hash:     String,
    pub payload:       String,
}

impl<'a> TendermintEngine<'a> {
    pub fn new(vset: &'a BftValidatorSet, height: u64, prev_hash: &str, payload: &str) -> Self {
        TendermintEngine {
            validator_set: vset,
            height,
            prev_hash: prev_hash.to_string(),
            payload: payload.to_string(),
        }
    }

    /// Chạy consensus cho height này — thử tối đa max_rounds rounds
    /// Trả về tất cả kết quả (committed hoặc timeouts) để demo
    pub fn run(&self, max_rounds: u32) -> Vec<ConsensusResult> {
        let mut results = vec![];

        for round in 0..max_rounds {
            let result = self.run_round(round);
            let committed = matches!(result, ConsensusResult::Committed { .. });
            results.push(result);
            if committed { break; }
        }

        results
    }

    fn run_round(&self, round: u32) -> ConsensusResult {
        let vset = self.validator_set;
        let height = self.height;

        // ── PROPOSE ──────────────────────────────────────────
        let proposer = match vset.proposer_for_round(height, round) {
            Some(p) => p,
            None    => return ConsensusResult::RoundTimeout {
                height, round, reason: "Không có proposer".to_string(),
            },
        };

        // Proposer bị faulty → không gửi proposal → timeout
        if proposer.is_faulty {
            return ConsensusResult::RoundTimeout {
                height, round,
                reason: format!("Proposer {} is faulty — no proposal", proposer.address),
            };
        }

        let block    = BftBlock::new(height, round, &self.prev_hash, &proposer.address, &self.payload);
        let _proposal = Proposal::new(height, round, &proposer.address, block.clone());

        // ── PREVOTE ──────────────────────────────────────────
        // Mỗi validator honest prevote cho block (sau khi xác nhận proposal hợp lệ)
        let prevotes: Vec<Vote> = vset.active_honest().iter().map(|v| {
            Vote::new(VoteType::Prevote, height, round, Some(block.hash.clone()), &v.address)
        }).collect();

        // Kiểm tra quorum prevote
        if !vset.has_quorum(&prevotes, &block.hash) {
            return ConsensusResult::RoundTimeout {
                height, round,
                reason: format!(
                    "Prevote quorum không đủ: {}/{} power",
                    vset.power_for(&prevotes, &block.hash),
                    vset.total_power()
                ),
            };
        }

        // ── PRECOMMIT ────────────────────────────────────────
        // Sau khi thấy 2/3+ prevotes → precommit
        let precommits: Vec<Vote> = vset.active_honest().iter().map(|v| {
            Vote::new(VoteType::Precommit, height, round, Some(block.hash.clone()), &v.address)
        }).collect();

        if !vset.has_quorum(&precommits, &block.hash) {
            return ConsensusResult::RoundTimeout {
                height, round,
                reason: "Precommit quorum không đủ".to_string(),
            };
        }

        // ── COMMIT ───────────────────────────────────────────
        let mut committed_block = block;
        committed_block.commit_votes = precommits.clone();

        ConsensusResult::Committed {
            precommit_count: precommits.len(),
            round,
            block: committed_block,
        }
    }
}

// ─── BftChain ─────────────────────────────────────────────────────────────────

/// Chain lưu các block đã được BFT commit
pub struct BftChain {
    pub blocks:        Vec<BftBlock>,
    pub validator_set: BftValidatorSet,

    /// Log tất cả rounds đã chạy (kể cả timeout)
    pub round_log:     Vec<ConsensusResult>,
}

impl BftChain {
    pub fn new() -> Self {
        let genesis = BftBlock::new(0, 0, "0000000000000000", "genesis", "Genesis Block");
        BftChain {
            blocks:        vec![genesis],
            validator_set: BftValidatorSet::new(),
            round_log:     vec![],
        }
    }

    pub fn height(&self) -> u64 { self.blocks.len() as u64 - 1 }

    pub fn tip_hash(&self) -> String {
        self.blocks.last().map(|b| b.hash.clone()).unwrap_or_default()
    }

    /// Chạy consensus cho block tiếp theo, lưu kết quả vào chain
    pub fn commit_next(&mut self, payload: &str, max_rounds: u32) -> bool {
        let height   = self.height() + 1;
        let prev_hash = self.tip_hash();

        let engine   = TendermintEngine::new(&self.validator_set, height, &prev_hash, payload);
        let results  = engine.run(max_rounds);

        let mut committed = false;
        for result in results {
            if let ConsensusResult::Committed { ref block, .. } = result {
                self.blocks.push(block.clone());
                committed = true;
            }
            self.round_log.push(result);
        }
        committed
    }
}

// ─── Safety Proof (simplified) ───────────────────────────────────────────────

/// Kiểm tra safety: không có 2 block khác nhau được commit cùng height
pub fn verify_safety(chain: &BftChain) -> bool {
    let mut by_height: HashMap<u64, &str> = HashMap::new();
    for block in &chain.blocks {
        if let Some(existing) = by_height.insert(block.height, &block.hash) {
            if existing != block.hash { return false; }
        }
    }
    true
}
