#![allow(dead_code)]
//! v6.7 — Mining Pool (Stratum-like)
//!
//! PoolServer phân phối WorkTemplate cho miners.
//! Miners submit Share (partial PoW thấp hơn block difficulty).
//! Pool track contributions → payout proportional khi tìm block.

use std::collections::HashMap;
use chrono::Utc;

// ─── Work Template ────────────────────────────────────────────────────────────

/// Pool gửi WorkTemplate cho mỗi miner khi có job mới.
#[derive(Debug, Clone)]
pub struct WorkTemplate {
    pub job_id:           u64,
    pub block_index:      u64,
    pub prev_hash:        String,
    pub timestamp:        i64,
    pub txid_root:        String,
    pub witness_root:     String,
    pub block_difficulty: usize, // target để tìm block thật
    pub share_difficulty: usize, // target thấp hơn để submit share
}

impl WorkTemplate {
    /// Hash một nonce theo cùng schema với Block::calculate_hash
    pub fn hash_nonce(&self, nonce: u64) -> String {
        let input = format!(
            "{}|{}|{}|{}|{}|{}",
            self.block_index, self.timestamp,
            self.txid_root, self.witness_root,
            self.prev_hash, nonce
        );
        hex::encode(blake3::hash(input.as_bytes()).as_bytes())
    }

    pub fn meets_share(&self, hash: &str) -> bool {
        hash.starts_with(&"0".repeat(self.share_difficulty))
    }

    pub fn meets_block(&self, hash: &str) -> bool {
        hash.starts_with(&"0".repeat(self.block_difficulty))
    }
}

// ─── Share ────────────────────────────────────────────────────────────────────

/// Miner gửi Share khi tìm được nonce đáp ứng share_difficulty.
#[derive(Debug, Clone)]
pub struct Share {
    pub job_id:             u64,
    pub miner_id:           String,
    pub nonce:              u64,
    pub hash:               String,
    pub is_block_solution:  bool, // true nếu đồng thời đáp ứng block_difficulty
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShareResult {
    /// Share hợp lệ, cộng vào contribution
    Accepted,
    /// Share đáp ứng cả block_difficulty — block được tìm!
    BlockFound { nonce: u64, hash: String },
    /// Hash không đáp ứng share_difficulty
    InvalidHash,
    /// job_id đã cũ
    StaleJob,
}

// ─── Pool Miner (server-side state) ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PoolMiner {
    pub id:               String,
    pub address:          String,
    pub shares:           u64,    // shares hợp lệ trong window hiện tại
    pub total_shares:     u64,    // tổng tất cả thời gian
    pub last_share_ts:    i64,    // unix timestamp share gần nhất
    pub share_difficulty: usize,  // per-miner adjusted difficulty
}

impl PoolMiner {
    fn new(id: &str, address: &str, share_difficulty: usize) -> Self {
        PoolMiner {
            id: id.to_string(),
            address: address.to_string(),
            shares: 0,
            total_shares: 0,
            last_share_ts: 0,
            share_difficulty,
        }
    }

    /// Ước tính hashrate (H/s) dựa trên thời gian giữa các shares
    pub fn estimated_hashrate(&self) -> f64 {
        if self.last_share_ts == 0 || self.shares == 0 {
            return 0.0;
        }
        // Mỗi share cần trung bình 16^share_difficulty hashes
        let hashes_per_share = 16_u64.pow(self.share_difficulty as u32) as f64;
        let elapsed = Utc::now().timestamp() - self.last_share_ts;
        if elapsed <= 0 {
            return 0.0;
        }
        (self.shares as f64 * hashes_per_share) / elapsed as f64
    }
}

// ─── Pool Server ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct PoolServer {
    pub block_difficulty:      usize,
    pub default_share_diff:    usize,
    pub miners:                HashMap<String, PoolMiner>,
    pub current_job:           Option<WorkTemplate>,
    pub job_counter:           u64,
    pub total_shares_in_round: u64,
    pub blocks_found:          u64,
    /// Target: 1 share / RETARGET_INTERVAL giây mỗi miner
    pub retarget_interval:     i64,
}

impl PoolServer {
    pub fn new(block_difficulty: usize) -> Self {
        // share_difficulty mặc định = block_difficulty - 2 (nếu >= 1)
        let default_share_diff = if block_difficulty > 2 { block_difficulty - 2 } else { 1 };
        PoolServer {
            block_difficulty,
            default_share_diff,
            miners: HashMap::new(),
            current_job: None,
            job_counter: 0,
            total_shares_in_round: 0,
            blocks_found: 0,
            retarget_interval: 10, // seconds
        }
    }

    /// Đăng ký miner mới, trả về per-miner share difficulty
    pub fn register_miner(&mut self, miner_id: &str, address: &str) -> usize {
        let diff = self.default_share_diff;
        self.miners.entry(miner_id.to_string())
            .or_insert_with(|| PoolMiner::new(miner_id, address, diff));
        diff
    }

    /// Tạo job mới từ block header fields, gửi cho tất cả miners.
    pub fn new_job(
        &mut self,
        block_index: u64,
        prev_hash: &str,
        txid_root: &str,
        witness_root: &str,
    ) -> WorkTemplate {
        self.job_counter += 1;
        let job = WorkTemplate {
            job_id:           self.job_counter,
            block_index,
            prev_hash:        prev_hash.to_string(),
            timestamp:        Utc::now().timestamp(),
            txid_root:        txid_root.to_string(),
            witness_root:     witness_root.to_string(),
            block_difficulty: self.block_difficulty,
            share_difficulty: self.default_share_diff,
        };
        self.current_job = Some(job.clone());
        job
    }

    /// Miner submit share. Trả về ShareResult.
    pub fn submit_share(&mut self, share: Share) -> ShareResult {
        // Kiểm tra job còn hiện tại không
        let job = match &self.current_job {
            Some(j) if j.job_id == share.job_id => j.clone(),
            _ => return ShareResult::StaleJob,
        };

        let miner_diff = self.miners.get(&share.miner_id)
            .map(|m| m.share_difficulty)
            .unwrap_or(self.default_share_diff);

        // Validate hash thực sự
        let expected_hash = job.hash_nonce(share.nonce);
        if expected_hash != share.hash {
            return ShareResult::InvalidHash;
        }
        if !expected_hash.starts_with(&"0".repeat(miner_diff)) {
            return ShareResult::InvalidHash;
        }

        // Cộng contribution
        let now = Utc::now().timestamp();
        if let Some(miner) = self.miners.get_mut(&share.miner_id) {
            miner.shares += 1;
            miner.total_shares += 1;
            miner.last_share_ts = now;
        }
        self.total_shares_in_round += 1;

        // Auto-retarget per-miner difficulty after each accepted share
        self.retarget_miner(&share.miner_id.clone());

        // Block solution?
        if expected_hash.starts_with(&"0".repeat(self.block_difficulty)) {
            self.blocks_found += 1;
            return ShareResult::BlockFound {
                nonce: share.nonce,
                hash:  expected_hash,
            };
        }

        ShareResult::Accepted
    }

    /// Tính payout tỷ lệ thuận với số shares trong round.
    /// Gọi sau khi BlockFound — reset counter sau đó.
    /// Satoshi dư do float truncation được cộng vào miner có nhiều shares nhất.
    pub fn payout(&self, total_reward: u64) -> HashMap<String, u64> {
        if self.total_shares_in_round == 0 {
            return HashMap::new();
        }
        let mut result: HashMap<String, u64> = HashMap::new();
        let mut distributed: u64 = 0;
        let mut max_shares:  u64 = 0;
        let mut max_miner         = String::new();

        for (id, m) in self.miners.iter().filter(|(_, m)| m.shares > 0) {
            let share  = m.shares as f64 / self.total_shares_in_round as f64;
            let reward = (total_reward as f64 * share) as u64;
            result.insert(id.clone(), reward);
            distributed += reward;
            if m.shares > max_shares {
                max_shares = m.shares;
                max_miner  = id.clone();
            }
        }

        // Distribute leftover satoshis (float truncation) to highest-share miner
        let leftover = total_reward.saturating_sub(distributed);
        if leftover > 0 && !max_miner.is_empty() {
            *result.entry(max_miner).or_insert(0) += leftover;
        }

        result
    }

    /// Reset shares sau khi tìm block và phát payout.
    pub fn reset_round(&mut self) {
        self.total_shares_in_round = 0;
        for miner in self.miners.values_mut() {
            miner.shares = 0;
        }
    }

    /// Điều chỉnh share difficulty cho một miner dựa trên tốc độ submit.
    /// Mục tiêu: 1 share / retarget_interval giây.
    pub fn retarget_miner(&mut self, miner_id: &str) {
        let miner = match self.miners.get_mut(miner_id) {
            Some(m) => m,
            None => return,
        };
        if miner.shares < 2 { return; } // chưa đủ dữ liệu

        let hashrate = miner.estimated_hashrate();
        if hashrate == 0.0 { return; }

        // hashes_needed = hashrate * target_interval
        let hashes_needed = hashrate * self.retarget_interval as f64;
        // new_diff = log16(hashes_needed)
        let new_diff = (hashes_needed.log2() / 4.0).round() as usize;
        let new_diff = new_diff.max(1).min(self.block_difficulty);

        if new_diff != miner.share_difficulty {
            miner.share_difficulty = new_diff;
        }
    }

    /// Trả về WorkTemplate với per-miner share difficulty.
    pub fn get_work_for(&self, miner_id: &str) -> Option<WorkTemplate> {
        let job = self.current_job.as_ref()?;
        let miner_diff = self.miners.get(miner_id)
            .map(|m| m.share_difficulty)
            .unwrap_or(self.default_share_diff);
        let mut work = job.clone();
        work.share_difficulty = miner_diff;
        Some(work)
    }

    pub fn miner_count(&self) -> usize { self.miners.len() }
}

// ─── Pool Client ──────────────────────────────────────────────────────────────

/// PoolClient chạy phía miner — nhận WorkTemplate, mine shares, submit.
pub struct PoolClient {
    pub miner_id: String,
    pub address:  String,
}

impl PoolClient {
    pub fn new(miner_id: &str, address: &str) -> Self {
        PoolClient { miner_id: miner_id.to_string(), address: address.to_string() }
    }

    /// Mine cho đến khi tìm share (đáp ứng share_difficulty).
    /// Trả về None nếu max_nonce exhausted (không xảy ra trong thực tế).
    pub fn mine_share(&self, work: &WorkTemplate) -> Option<Share> {
        self.mine_share_from(work, 0)
    }

    /// Mine từ nonce_start — để test với nonce xác định.
    pub fn mine_share_from(&self, work: &WorkTemplate, nonce_start: u64) -> Option<Share> {
        let share_target = "0".repeat(work.share_difficulty);
        let block_target = "0".repeat(work.block_difficulty);
        let mut nonce = nonce_start;
        loop {
            let hash = work.hash_nonce(nonce);
            if hash.starts_with(&share_target) {
                return Some(Share {
                    job_id:            work.job_id,
                    miner_id:          self.miner_id.clone(),
                    nonce,
                    is_block_solution: hash.starts_with(&block_target),
                    hash,
                });
            }
            nonce = nonce.wrapping_add(1);
            if nonce == nonce_start { return None; } // wrapped around
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_server(block_diff: usize) -> PoolServer {
        PoolServer::new(block_diff)
    }

    fn make_job(server: &mut PoolServer) -> WorkTemplate {
        server.new_job(1, "0".repeat(64).as_str(), &"0".repeat(64), &"0".repeat(64))
    }

    #[test]
    fn test_register_miner() {
        let mut srv = make_server(3);
        let diff = srv.register_miner("0000000000000001", "0000000000000000000000000000000000000001");
        assert_eq!(diff, 1); // 3-2=1
        assert_eq!(srv.miner_count(), 1);
    }

    #[test]
    fn test_new_job_increments_counter() {
        let mut srv = make_server(3);
        let j1 = make_job(&mut srv);
        let j2 = make_job(&mut srv);
        assert_eq!(j1.job_id, 1);
        assert_eq!(j2.job_id, 2);
    }

    #[test]
    fn test_client_mine_share_valid() {
        let mut srv = make_server(4);
        srv.register_miner("0000000000000002", "0000000000000000000000000000000000000002");
        let work = make_job(&mut srv);
        let client = PoolClient::new("0000000000000002", "0000000000000000000000000000000000000002");
        let share = client.mine_share(&work).expect("should find share");
        assert!(share.hash.starts_with(&"0".repeat(work.share_difficulty)));
        assert_eq!(share.miner_id, "0000000000000002");
        assert_eq!(share.job_id, work.job_id);
    }

    #[test]
    fn test_submit_share_accepted() {
        let mut srv = make_server(4);
        srv.register_miner("0000000000000002", "0000000000000000000000000000000000000002");
        let work = make_job(&mut srv);
        let client = PoolClient::new("0000000000000002", "0000000000000000000000000000000000000002");
        let share = client.mine_share(&work).unwrap();
        let result = srv.submit_share(share);
        // either Accepted or BlockFound
        assert!(matches!(result, ShareResult::Accepted | ShareResult::BlockFound { .. }));
        assert_eq!(srv.total_shares_in_round, 1);
    }

    #[test]
    fn test_submit_stale_job() {
        let mut srv = make_server(3);
        srv.register_miner("0000000000000002", "0000000000000000000000000000000000000002");
        let work = make_job(&mut srv);
        let client = PoolClient::new("0000000000000002", "0000000000000000000000000000000000000002");
        let mut share = client.mine_share(&work).unwrap();
        share.job_id = 999; // stale
        let result = srv.submit_share(share);
        assert_eq!(result, ShareResult::StaleJob);
    }

    #[test]
    fn test_submit_invalid_hash() {
        let mut srv = make_server(3);
        srv.register_miner("0000000000000002", "0000000000000000000000000000000000000002");
        let work = make_job(&mut srv);
        let client = PoolClient::new("0000000000000002", "0000000000000000000000000000000000000002");
        let mut share = client.mine_share(&work).unwrap();
        share.hash = "ffffffffffffffffffffffffffffffff".to_string(); // wrong
        let result = srv.submit_share(share);
        assert_eq!(result, ShareResult::InvalidHash);
    }

    #[test]
    fn test_payout_proportional() {
        let mut srv = make_server(4);
        srv.register_miner("0000000000000001", "0000000000000000000000000000000000000001");
        srv.register_miner("0000000000000002",   "0000000000000000000000000000000000000002");
        let work = make_job(&mut srv);
        let c1 = PoolClient::new("0000000000000001", "0000000000000000000000000000000000000001");
        let c2 = PoolClient::new("0000000000000002",   "0000000000000000000000000000000000000002");

        // alice submits 3 shares, bob 1 share
        for _ in 0..3 {
            let work2 = work.clone();
            let s = c1.mine_share(&work2).unwrap();
            let _ = srv.submit_share(s);
        }
        let s = c2.mine_share(&work).unwrap();
        let _ = srv.submit_share(s);

        let payouts = srv.payout(1000);
        let alice_pay = *payouts.get("0000000000000001").unwrap_or(&0);
        let bob_pay   = *payouts.get("0000000000000002").unwrap_or(&0);
        // alice = 75%, bob = 25%
        assert!(alice_pay > bob_pay, "alice should earn more: {} vs {}", alice_pay, bob_pay);
        assert!(alice_pay + bob_pay <= 1000);
    }

    #[test]
    fn test_reset_round() {
        let mut srv = make_server(4);
        srv.register_miner("0000000000000001", "0000000000000000000000000000000000000001");
        let work = make_job(&mut srv);
        let c = PoolClient::new("0000000000000001", "0000000000000000000000000000000000000001");
        let s = c.mine_share(&work).unwrap();
        let _ = srv.submit_share(s);
        assert_eq!(srv.total_shares_in_round, 1);
        srv.reset_round();
        assert_eq!(srv.total_shares_in_round, 0);
        assert_eq!(srv.miners["0000000000000001"].shares, 0);
    }

    #[test]
    fn test_get_work_for_miner() {
        let mut srv = make_server(4);
        srv.register_miner("0000000000000001", "0000000000000000000000000000000000000001");
        make_job(&mut srv);
        let work = srv.get_work_for("0000000000000001").unwrap();
        assert_eq!(work.block_difficulty, 4);
        assert_eq!(work.share_difficulty, 2); // 4-2=2
    }

    #[test]
    fn test_default_share_diff_floor() {
        let mut srv = make_server(2);
        let diff = srv.register_miner("0000000000000001", "0000000000000000000000000000000000000001");
        assert_eq!(diff, 1); // floor at 1
    }

    #[test]
    fn test_block_solution_detected() {
        // Use difficulty=1 so shares can also be block solutions
        let mut srv = make_server(1);
        srv.register_miner("0000000000000001", "0000000000000000000000000000000000000001");
        let work = make_job(&mut srv);
        let client = PoolClient::new("0000000000000001", "0000000000000000000000000000000000000001");
        // mine until we find something that meets diff=1 (share_diff=1 too)
        let share = client.mine_share(&work).unwrap();
        if share.is_block_solution {
            let result = srv.submit_share(share.clone());
            assert!(matches!(result, ShareResult::BlockFound { .. }));
            assert_eq!(srv.blocks_found, 1);
        }
    }
}
