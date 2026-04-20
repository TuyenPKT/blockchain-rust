#![allow(dead_code)]
//! Lightning Network — v1.2 Payment Channels
//!
//! Simplified Lightning channel lifecycle:
//!
//!   1. OPEN:    Alice + Bob fund 2-of-2 multisig on-chain (funding TX)
//!   2. UPDATE:  Off-chain commitment TXs — không broadcast, chỉ ký
//!               Mỗi bên giữ 1 commitment TX của mình, có timelock CSV
//!               Khi update, revoke commitment cũ bằng revocation secret
//!   3. CLOSE (cooperative):  Broadcast closing TX — settlement ngay
//!   4. CLOSE (force):        Broadcast commitment TX cũ
//!                            Counterparty có thể claim full penalty
//!                            nếu dùng revocation key trong timelock
//!
//! Cấu trúc commitment TX output:
//!   - "to_self":        CSV(144 blocks) || revocation_key  (có thể bị penalty)
//!   - "to_counterparty": P2WPKH ngay lập tức
//!
//! Đây là simplified model — Lightning thực tế dùng HTLC cho multi-hop

use serde::{Serialize, Deserialize};
use crate::wallet::Wallet;

// ── Trạng thái channel ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ChannelState {
    PendingOpen,  // chờ funding TX confirm
    Open,         // active, có thể send/receive
    Closing,      // đang đóng cooperative
    Closed,       // đã đóng
    ForceClosing, // một bên broadcast commitment TX
}

// ── Commitment Transaction (off-chain) ──────────────────────

/// Một commitment TX — đại diện cho state kênh tại một thời điểm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitmentTx {
    pub sequence:          u64,          // số thứ tự (tăng dần mỗi lần update)
    pub balance_local:     u64,          // sat phía mình
    pub balance_remote:    u64,          // sat phía đối phương
    pub local_sig:         Option<String>, // sig của mình
    pub remote_sig:        Option<String>, // sig của đối phương
    pub revocation_hash:   String,       // hash của revocation secret cũ
    pub csv_delay:         u32,          // blocks phải đợi nếu force close (144)
}

impl CommitmentTx {
    /// Dữ liệu ký — bao gồm sequence để ngăn replay attack
    pub fn signing_data(&self) -> Vec<u8> {
        let data = format!(
            "commitment|{}|{}|{}|{}",
            self.sequence, self.balance_local, self.balance_remote, self.csv_delay
        );
        blake3::hash(data.as_bytes()).as_bytes().to_vec()
    }

    pub fn is_fully_signed(&self) -> bool {
        self.local_sig.is_some() && self.remote_sig.is_some()
    }

    /// "txid" của commitment TX (để tham chiếu)
    pub fn txid(&self) -> String {
        let data = format!("cmttx|{}|{}|{}", self.sequence, self.balance_local, self.balance_remote);
        hex::encode(blake3::hash(data.as_bytes()).as_bytes())
    }
}

// ── Revocation Secret ────────────────────────────────────────

/// Mỗi commitment TX cũ được revoke bằng cách reveal secret này
/// Nếu counterparty broadcast TX cũ, bên kia dùng secret để lấy hết tiền (penalty)
#[derive(Debug, Clone)]
pub struct RevocationSecret {
    pub secret:       Vec<u8>, // 32 bytes ngẫu nhiên
    pub hash:         String,  // SHA256(secret) — commit trước khi reveal
    pub for_sequence: u64,     // commitment TX nào bị revoke
}

impl RevocationSecret {
    pub fn new(sequence: u64) -> Self {
        // Dùng CSPRNG (secp256k1::rand) thay vì subsec_nanos() (entropy quá yếu)
        use secp256k1::rand::RngCore;
        let mut secret_bytes = [0u8; 32];
        secp256k1::rand::thread_rng().fill_bytes(&mut secret_bytes);
        // Mix với sequence để đảm bảo uniqueness giữa các lần gọi liên tiếp
        secret_bytes[0] ^= (sequence & 0xff) as u8;
        secret_bytes[1] ^= ((sequence >> 8) & 0xff) as u8;
        let secret = secret_bytes.to_vec();
        let hash   = hex::encode(blake3::hash(&secret).as_bytes());
        RevocationSecret { secret, hash, for_sequence: sequence }
    }

    pub fn reveal(&self) -> String {
        hex::encode(&self.secret)
    }
}

// ── HTLC (Hash Time-Locked Contract) ────────────────────────

/// Payment đang in-flight qua channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Htlc {
    pub id:           u64,
    pub amount_msat:  u64,    // millisatoshi (Lightning dùng msat)
    pub payment_hash: String, // SHA256(payment_preimage)
    pub expiry:       u64,    // block height timeout
    pub direction:    HtlcDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HtlcDirection {
    Offered,  // mình đang offer payment
    Received, // mình đang receive payment
}

impl Htlc {
    pub fn new_outgoing(amount_msat: u64, payment_hash: &str, expiry: u64, id: u64) -> Self {
        Htlc { id, amount_msat, payment_hash: payment_hash.to_string(), expiry, direction: HtlcDirection::Offered }
    }

    /// Settle HTLC khi reveal preimage: SHA256(preimage) == payment_hash
    pub fn can_settle(&self, preimage_hex: &str) -> bool {
        let preimage = match hex::decode(preimage_hex) { Ok(b) => b, Err(_) => return false };
        hex::encode(blake3::hash(&preimage).as_bytes()) == self.payment_hash
    }
}

// ── Payment Channel ──────────────────────────────────────────

pub struct Channel {
    pub channel_id:        String,
    pub state:             ChannelState,

    // Funding
    pub funding_tx_id:     Option<String>, // on-chain funding TX
    pub funding_index:     usize,          // output index trong funding TX
    pub capacity:          u64,            // tổng sat trong channel

    // Keys
    pub local_pubkey:      String,  // hex
    #[allow(dead_code)]
    pub remote_pubkey:     String,  // hex

    // State
    pub commitment_number: u64,
    pub local_balance:     u64,    // sat phía mình
    pub remote_balance:    u64,    // sat phía counterparty

    // Commitments
    pub current_commitment: Option<CommitmentTx>,
    pub pending_htlcs:      Vec<Htlc>,

    // Revocation
    pub local_revocations:  Vec<RevocationSecret>, // secrets mình đã reveal
    #[allow(dead_code)]
    pub remote_rev_hashes:  Vec<String>,           // hashes counterparty đã commit
}

impl Channel {
    /// Tạo channel mới — chưa mở
    pub fn new(local_wallet: &Wallet, remote_pubkey_hex: &str, capacity: u64) -> Self {
        let local_pubkey = local_wallet.public_key_hex();
        let channel_id   = hex::encode(blake3::hash(
            format!("channel|{}|{}|{}", local_pubkey, remote_pubkey_hex, capacity).as_bytes()
        ).as_bytes());
        Channel {
            channel_id,
            state:             ChannelState::PendingOpen,
            funding_tx_id:     None,
            funding_index:     0,
            capacity,
            local_pubkey,
            remote_pubkey:     remote_pubkey_hex.to_string(),
            commitment_number: 0,
            local_balance:     capacity,  // mở channel, mình fund toàn bộ
            remote_balance:    0,
            current_commitment: None,
            pending_htlcs:     vec![],
            local_revocations: vec![],
            remote_rev_hashes: vec![],
        }
    }

    /// Confirm funding TX on-chain → channel OPEN
    pub fn confirm_funding(&mut self, funding_tx_id: &str, index: usize) {
        self.funding_tx_id = Some(funding_tx_id.to_string());
        self.funding_index = index;
        self.state         = ChannelState::Open;
        println!("  🔓 Channel {} OPEN | capacity={} sat", &self.channel_id[..8], self.capacity);
        println!("     local={} sat | remote={} sat", self.local_balance, self.remote_balance);

        // Tạo commitment TX đầu tiên
        self.current_commitment = Some(self.make_commitment());
    }

    /// Tạo CommitmentTx từ state hiện tại
    fn make_commitment(&self) -> CommitmentTx {
        let rev = RevocationSecret::new(self.commitment_number);
        CommitmentTx {
            sequence:        self.commitment_number,
            balance_local:   self.local_balance,
            balance_remote:  self.remote_balance,
            local_sig:       None,
            remote_sig:      None,
            revocation_hash: rev.hash,
            csv_delay:       144,
        }
    }

    /// Ký commitment TX hiện tại
    pub fn sign_commitment(&mut self, wallet: &Wallet) -> String {
        let cmt = match &mut self.current_commitment {
            Some(c) => c,
            None    => panic!("Không có commitment TX"),
        };
        let sig = wallet.sign(&cmt.signing_data());
        if wallet.public_key_hex() == self.local_pubkey {
            cmt.local_sig = Some(sig.clone());
        } else {
            cmt.remote_sig = Some(sig.clone());
        }
        sig
    }

    /// Apply sig từ counterparty
    #[allow(dead_code)]
    pub fn apply_remote_sig(&mut self, sig: &str) {
        if let Some(cmt) = &mut self.current_commitment {
            if cmt.remote_sig.is_none() {
                cmt.remote_sig = Some(sig.to_string());
            }
        }
    }

    /// Gửi payment off-chain: Alice → Bob
    /// Tạo commitment TX mới với balance cập nhật
    /// Returns: (new commitment txid, revocation secret cho commitment cũ)
    pub fn send_payment(
        &mut self,
        wallet:     &Wallet,
        amount_sat: u64,
    ) -> Result<(String, String), String> {
        if self.state != ChannelState::Open {
            return Err("❌ Channel chưa mở".to_string());
        }
        if amount_sat > self.local_balance {
            return Err(format!("❌ Không đủ: local={} sat, muốn gửi={} sat",
                self.local_balance, amount_sat));
        }

        // Revoke commitment cũ — tạo revocation secret để counterparty có thể penalty nếu dùng TX cũ
        let old_rev = RevocationSecret::new(self.commitment_number);
        let rev_secret = old_rev.reveal();
        self.local_revocations.push(old_rev);

        // Cập nhật balance
        self.local_balance  -= amount_sat;
        self.remote_balance += amount_sat;
        self.commitment_number += 1;

        // Tạo + ký commitment TX mới
        let mut new_cmt = self.make_commitment();
        let sig         = wallet.sign(&new_cmt.signing_data());
        new_cmt.local_sig = Some(sig);
        let txid = new_cmt.txid();
        self.current_commitment = Some(new_cmt);

        println!("  💸 Payment {}: {} sat off-chain", self.commitment_number, amount_sat);
        println!("     local={} sat | remote={} sat", self.local_balance, self.remote_balance);

        Ok((txid, rev_secret))
    }

    /// Gửi payment qua HTLC (multi-hop Lightning)
    pub fn send_htlc(
        &mut self,
        amount_msat:  u64,
        payment_hash: &str,
        expiry:       u64,
    ) -> Result<u64, String> {
        if self.state != ChannelState::Open { return Err("❌ Channel chưa mở".to_string()); }
        let amount_sat = (amount_msat + 999) / 1000;
        if amount_sat > self.local_balance {
            return Err(format!("❌ Không đủ sat cho HTLC"));
        }
        let htlc_id = self.pending_htlcs.len() as u64;
        let htlc    = Htlc::new_outgoing(amount_msat, payment_hash, expiry, htlc_id);
        self.local_balance -= amount_sat; // lock amount trong HTLC
        self.pending_htlcs.push(htlc);
        println!("  🔒 HTLC #{}: {} msat locked (hash={}...)", htlc_id, amount_msat, &payment_hash[..8]);
        Ok(htlc_id)
    }

    /// Settle HTLC khi nhận được preimage
    pub fn settle_htlc(&mut self, htlc_id: u64, preimage_hex: &str) -> Result<(), String> {
        let htlc = self.pending_htlcs.iter()
            .find(|h| h.id == htlc_id)
            .ok_or("❌ HTLC không tồn tại")?
            .clone();

        if !htlc.can_settle(preimage_hex) {
            return Err("❌ Preimage không khớp payment_hash".to_string());
        }

        let amount_sat = (htlc.amount_msat + 999) / 1000;
        self.remote_balance += amount_sat; // tiền unlock cho remote
        self.pending_htlcs.retain(|h| h.id != htlc_id);
        println!("  ✅ HTLC #{} settled: {} msat → remote", htlc_id, htlc.amount_msat);
        Ok(())
    }

    /// Cooperative close — trả về (local_amount, remote_amount)
    /// Tự trừ on-chain fee từ local balance
    pub fn cooperative_close(&mut self, onchain_fee: u64) -> (u64, u64) {
        self.state = ChannelState::Closed;
        let local_after_fee = self.local_balance.saturating_sub(onchain_fee);
        println!("  🤝 Cooperative close: local={} sat | remote={} sat (fee={} sat)",
            local_after_fee, self.remote_balance, onchain_fee);
        (local_after_fee, self.remote_balance)
    }

    /// Force close — broadcast commitment TX mới nhất
    /// Counterparty có thể dùng revocation key nếu TX này đã bị revoke
    #[allow(dead_code)]
    pub fn force_close(&mut self) -> Option<String> {
        self.state = ChannelState::ForceClosing;
        if let Some(cmt) = &self.current_commitment {
            let txid = cmt.txid();
            println!("  ⚠️  Force close: broadcast commitment TX {}", &txid[..12]);
            println!("     CSV delay={} blocks trước khi rút local funds", cmt.csv_delay);
            Some(txid)
        } else {
            None
        }
    }

    /// Kiểm tra xem 1 broadcast TX có bị revoke không
    /// Nếu counterparty broadcast TX cũ → mình có thể lấy hết bằng revocation secret
    pub fn check_penalty(&self, broadcast_sequence: u64) -> Option<&RevocationSecret> {
        self.local_revocations.iter()
            .find(|r| r.for_sequence == broadcast_sequence)
    }

    pub fn status(&self) {
        println!("  Channel {}: {:?}", &self.channel_id[..8], self.state);
        println!("    local={} sat | remote={} sat | pending_htlcs={}",
            self.local_balance, self.remote_balance, self.pending_htlcs.len());
        if let Some(cmt) = &self.current_commitment {
            println!("    commitment #{} | signed={}", cmt.sequence, cmt.is_fully_signed());
        }
    }
}

// ── Lightning Node ───────────────────────────────────────────

/// Simplified Lightning node — quản lý nhiều channels
pub struct LightningNode {
    pub wallet:   Wallet,
    #[allow(dead_code)]
    pub channels: Vec<Channel>,
}

impl LightningNode {
    pub fn new(wallet: Wallet) -> Self {
        LightningNode { wallet, channels: vec![] }
    }

    /// Mở channel đến peer
    #[allow(dead_code)]
    pub fn open_channel(&mut self, remote_pubkey: &str, capacity: u64) -> usize {
        let ch = Channel::new(&self.wallet, remote_pubkey, capacity);
        println!("  📡 Opening channel to {}... | capacity={} sat",
            &remote_pubkey[..8], capacity);
        self.channels.push(ch);
        self.channels.len() - 1
    }

    /// Gửi payment qua channel
    #[allow(dead_code)]
    pub fn pay(&mut self, channel_idx: usize, amount_sat: u64) -> Result<String, String> {
        let ch = &mut self.channels[channel_idx];
        let (txid, _rev) = ch.send_payment(&self.wallet, amount_sat)?;
        Ok(txid)
    }

    pub fn node_id(&self) -> String { self.wallet.public_key_hex() }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::Wallet;

    fn alice() -> Wallet { Wallet::new() }
    fn bob()   -> Wallet { Wallet::new() }

    #[test]
    fn test_channel_new_is_pending() {
        let a = alice();
        let b = bob();
        let ch = Channel::new(&a, &b.public_key_hex(), 1_000_000);
        assert_eq!(ch.state, ChannelState::PendingOpen);
        assert_eq!(ch.capacity, 1_000_000);
        assert_eq!(ch.local_balance, 1_000_000);
        assert_eq!(ch.remote_balance, 0);
    }

    #[test]
    fn test_channel_open_on_funding() {
        let a = alice();
        let b = bob();
        let mut ch = Channel::new(&a, &b.public_key_hex(), 500_000);
        ch.confirm_funding("faketxid", 0);
        assert_eq!(ch.state, ChannelState::Open);
        assert!(ch.current_commitment.is_some());
    }

    #[test]
    fn test_send_payment_updates_balances() {
        let a = alice();
        let b = bob();
        let mut ch = Channel::new(&a, &b.public_key_hex(), 1_000_000);
        ch.confirm_funding("txid1", 0);
        let result = ch.send_payment(&a, 300_000);
        assert!(result.is_ok());
        assert_eq!(ch.local_balance, 700_000);
        assert_eq!(ch.remote_balance, 300_000);
    }

    #[test]
    fn test_send_payment_insufficient_funds() {
        let a = alice();
        let b = bob();
        let mut ch = Channel::new(&a, &b.public_key_hex(), 100_000);
        ch.confirm_funding("txid2", 0);
        let result = ch.send_payment(&a, 200_000);
        assert!(result.is_err());
        assert_eq!(ch.local_balance, 100_000); // unchanged
    }

    #[test]
    fn test_send_payment_increments_commitment_number() {
        let a = alice();
        let b = bob();
        let mut ch = Channel::new(&a, &b.public_key_hex(), 1_000_000);
        ch.confirm_funding("txid3", 0);
        assert_eq!(ch.commitment_number, 0);
        ch.send_payment(&a, 1_000).unwrap();
        assert_eq!(ch.commitment_number, 1);
        ch.send_payment(&a, 1_000).unwrap();
        assert_eq!(ch.commitment_number, 2);
    }

    #[test]
    fn test_send_htlc_locks_balance() {
        let a = alice();
        let b = bob();
        let mut ch = Channel::new(&a, &b.public_key_hex(), 1_000_000);
        ch.confirm_funding("txid4", 0);
        let htlc_id = ch.send_htlc(100_000, "aabbccddeeff", 500).unwrap();
        assert_eq!(htlc_id, 0);
        assert_eq!(ch.local_balance, 1_000_000 - 100); // 100 sat locked
        assert_eq!(ch.pending_htlcs.len(), 1);
    }

    #[test]
    fn test_settle_htlc_correct_preimage() {
        let a = alice();
        let b = bob();
        let mut ch = Channel::new(&a, &b.public_key_hex(), 1_000_000);
        ch.confirm_funding("txid5", 0);
        let preimage = b"test_preimage_32bytes_here______";
        let hash = hex::encode(blake3::hash(preimage).as_bytes());
        ch.send_htlc(1000, &hash, 500).unwrap();
        let preimage_hex = hex::encode(preimage);
        assert!(ch.settle_htlc(0, &preimage_hex).is_ok());
        assert_eq!(ch.pending_htlcs.len(), 0);
    }

    #[test]
    fn test_settle_htlc_wrong_preimage() {
        let a = alice();
        let b = bob();
        let mut ch = Channel::new(&a, &b.public_key_hex(), 1_000_000);
        ch.confirm_funding("txid6", 0);
        let real = b"real_preimage_32bytes_here______";
        let hash = hex::encode(blake3::hash(real).as_bytes());
        ch.send_htlc(1000, &hash, 500).unwrap();
        let result = ch.settle_htlc(0, &hex::encode(b"wrong_preimage__________________"));
        assert!(result.is_err());
        assert_eq!(ch.pending_htlcs.len(), 1); // still pending
    }

    #[test]
    fn test_cooperative_close() {
        let a = alice();
        let b = bob();
        let mut ch = Channel::new(&a, &b.public_key_hex(), 1_000_000);
        ch.confirm_funding("txid7", 0);
        ch.send_payment(&a, 300_000).unwrap();
        let (local, remote) = ch.cooperative_close(1000);
        assert_eq!(ch.state, ChannelState::Closed);
        assert_eq!(local,  700_000 - 1000);
        assert_eq!(remote, 300_000);
    }

    #[test]
    fn test_check_penalty_finds_revocation() {
        let a = alice();
        let b = bob();
        let mut ch = Channel::new(&a, &b.public_key_hex(), 1_000_000);
        ch.confirm_funding("txid8", 0);
        ch.send_payment(&a, 1_000).unwrap(); // creates revocation for seq 0
        let rev = ch.check_penalty(0);
        assert!(rev.is_some());
        assert_eq!(rev.unwrap().for_sequence, 0);
    }

    #[test]
    fn test_check_penalty_none_for_unknown_sequence() {
        let a = alice();
        let b = bob();
        let mut ch = Channel::new(&a, &b.public_key_hex(), 1_000_000);
        ch.confirm_funding("txid9", 0);
        let rev = ch.check_penalty(99);
        assert!(rev.is_none());
    }

    #[test]
    fn test_revocation_secret_unique() {
        let r1 = RevocationSecret::new(1);
        let r2 = RevocationSecret::new(1);
        // Different CSPRNG → different secrets (with overwhelming probability)
        assert_ne!(r1.secret, r2.secret);
    }

    #[test]
    fn test_htlc_can_settle_correct() {
        let preimage = b"preimage_data_exactly_32_bytes__";
        let hash  = hex::encode(blake3::hash(preimage).as_bytes());
        let htlc  = Htlc::new_outgoing(1_000_000, &hash, 100, 0);
        assert!(htlc.can_settle(&hex::encode(preimage)));
    }

    #[test]
    fn test_htlc_can_settle_wrong() {
        let hash = hex::encode(blake3::hash(b"correct").as_bytes());
        let htlc = Htlc::new_outgoing(1_000_000, &hash, 100, 0);
        assert!(!htlc.can_settle(&hex::encode(b"wrong")));
    }

    #[test]
    fn test_lightning_node_id() {
        let w    = alice();
        let node = LightningNode::new(w);
        assert!(!node.node_id().is_empty());
    }
}
