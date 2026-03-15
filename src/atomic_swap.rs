#![allow(dead_code)]
//! Atomic Swap — v1.7
//!
//! Cross-chain trustless exchange dùng HTLC (Hash Time-Locked Contract):
//!   Alice muốn đổi PKT → LTC với Bob (không cần trust bên thứ ba)
//!
//! Protocol (2 chain A = PKT, B = LTC):
//!
//!   1. Alice tạo secret `s`, tính hash_lock `h = SHA256(s)`
//!   2. Alice lock PKT trên chain A:
//!        HTLC_A: Bob claim nếu biết `s` trong T1 blocks
//!                Alice refund sau T1 blocks
//!   3. Bob thấy HTLC_A trên chain A, kiểm tra hash_lock `h`
//!   4. Bob lock LTC trên chain B:
//!        HTLC_B: Alice claim nếu biết `s` trong T2 blocks (T2 < T1)
//!                Bob refund sau T2 blocks
//!   5. Alice reveal `s` để claim LTC từ HTLC_B
//!   6. Bob thấy `s` on-chain, dùng nó claim PKT từ HTLC_A
//!
//! Tính chất atomic:
//!   - Nếu Alice claim LTC → `s` lộ → Bob claim PKT ✅
//!   - Nếu Alice không claim trong T2 → Bob refund LTC ✅
//!   - Nếu Bob không lock → Alice không reveal `s` → Alice refund PKT sau T1 ✅
//!   - Không thể: Alice lấy LTC mà Bob không lấy được PKT
//!
//! Timelock: T1 >> T2 (ví dụ T1 = 48h, T2 = 24h)
//! → Alice có đủ thời gian claim sau khi thấy Bob lock
//! → Bob có đủ thời gian claim sau khi thấy Alice claim

use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use crate::wallet::Wallet;
use crate::script::Script;

// ── HTLC (Hash Time-Locked Contract) ─────────────────────────
//
// Script logic (simplified):
//   Claim: preimage s sao cho SHA256(s) == hash_lock, AND sig(claimer)
//   Refund: current_block >= locktime, AND sig(refunder)

/// HTLC output — đại diện cho locked funds trên một chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtlcOutput {
    pub hash_lock:     [u8; 32],   // SHA256(preimage) — public
    pub locktime:      u64,        // block height refund có thể claim
    pub amount:        u64,        // sat bị lock
    pub claimer_hash:  String,     // pubkey hash người được claim (reveal preimage)
    pub refunder_hash: String,     // pubkey hash người refund (sau locktime)
    pub utxo_id:       String,     // ID của output này trên chain
}

impl HtlcOutput {
    pub fn new(
        preimage_hash: [u8; 32],
        locktime:      u64,
        amount:        u64,
        claimer_hash:  &str,
        refunder_hash: &str,
    ) -> Self {
        // utxo_id = H(hash_lock || amount || locktime)
        let mut data = preimage_hash.to_vec();
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&locktime.to_le_bytes());
        let utxo_id = hex::encode(Sha256::digest(&data));

        HtlcOutput {
            hash_lock: preimage_hash,
            locktime,
            amount,
            claimer_hash:  claimer_hash.to_string(),
            refunder_hash: refunder_hash.to_string(),
            utxo_id,
        }
    }

    /// Verify claim: preimage phải hash thành hash_lock, sig phải hợp lệ
    pub fn verify_claim(&self, preimage: &[u8], claimer_wallet: &Wallet) -> bool {
        // 1. Check hash lock
        let h: [u8; 32] = Sha256::digest(preimage).into();
        if h != self.hash_lock { return false; }

        // 2. Check claimer identity
        let claimer_actual = hex::encode(Script::pubkey_hash(&claimer_wallet.public_key.serialize()));
        if claimer_actual != self.claimer_hash { return false; }

        // 3. Verify signature
        let signing_data = self.claim_signing_data(preimage);
        let sig = claimer_wallet.sign(&signing_data);
        Wallet::verify(&claimer_wallet.public_key, &signing_data, &sig)
    }

    /// Verify refund: current_block >= locktime, sig hợp lệ
    pub fn verify_refund(&self, current_block: u64, refunder_wallet: &Wallet) -> bool {
        // 1. Check timelock
        if current_block < self.locktime { return false; }

        // 2. Check refunder identity
        let refunder_actual = hex::encode(Script::pubkey_hash(&refunder_wallet.public_key.serialize()));
        if refunder_actual != self.refunder_hash { return false; }

        // 3. Verify signature
        let signing_data = self.refund_signing_data(current_block);
        let sig = refunder_wallet.sign(&signing_data);
        Wallet::verify(&refunder_wallet.public_key, &signing_data, &sig)
    }

    fn claim_signing_data(&self, preimage: &[u8]) -> Vec<u8> {
        let mut data = b"htlc_claim|".to_vec();
        data.extend_from_slice(&self.hash_lock);
        data.extend_from_slice(&self.amount.to_le_bytes());
        data.extend_from_slice(preimage);
        Sha256::digest(&data).to_vec()
    }

    fn refund_signing_data(&self, current_block: u64) -> Vec<u8> {
        let mut data = b"htlc_refund|".to_vec();
        data.extend_from_slice(&self.hash_lock);
        data.extend_from_slice(&self.amount.to_le_bytes());
        data.extend_from_slice(&current_block.to_le_bytes());
        Sha256::digest(&data).to_vec()
    }
}

// ── AtomicSwap ────────────────────────────────────────────────

/// Trạng thái của một atomic swap
#[derive(Debug, Clone, PartialEq)]
pub enum SwapState {
    Proposed,           // Alice tạo proposal, chưa lock
    AliceLocked,        // Alice đã lock trên chain A
    BobLocked,          // Bob đã lock trên chain B (swap active)
    AliceClaimed,       // Alice đã reveal secret và claim chain B
    BobClaimed,         // Bob đã claim chain A bằng secret
    Completed,          // Hoàn thành
    AliceRefunded,      // Alice refund (Bob không lock)
    BobRefunded,        // Bob refund (Alice không claim kịp T2)
}

/// Atomic Swap session giữa Alice và Bob
pub struct AtomicSwap {
    pub state:         SwapState,
    pub preimage:      Option<Vec<u8>>,   // Alice giữ bí mật cho đến khi claim
    pub hash_lock:     [u8; 32],          // SHA256(preimage) — public
    pub alice_amount:  u64,               // Alice lock bao nhiêu sat (chain A)
    pub bob_amount:    u64,               // Bob lock bao nhiêu sat (chain B)
    pub locktime_a:    u64,               // Timelock dài hơn (chain A — Alice refund)
    pub locktime_b:    u64,               // Timelock ngắn hơn (chain B — Bob refund)
    pub htlc_a:        Option<HtlcOutput>,  // HTLC trên chain A
    pub htlc_b:        Option<HtlcOutput>,  // HTLC trên chain B
    pub revealed_preimage: Option<Vec<u8>>, // Sau khi Alice claim — Bob dùng để claim A
}

impl AtomicSwap {
    /// Alice tạo swap: chọn amount, tạo secret
    pub fn propose(
        alice_amount: u64,
        bob_amount:   u64,
        locktime_a:   u64,  // block height dài hơn (ví dụ 100)
        locktime_b:   u64,  // block height ngắn hơn (ví dụ 50)
    ) -> Self {
        // Alice tạo random preimage 32 bytes
        let preimage = generate_preimage();
        let hash_lock: [u8; 32] = Sha256::digest(&preimage).into();

        AtomicSwap {
            state: SwapState::Proposed,
            preimage: Some(preimage),
            hash_lock,
            alice_amount,
            bob_amount,
            locktime_a,
            locktime_b,
            htlc_a: None,
            htlc_b: None,
            revealed_preimage: None,
        }
    }

    /// Step 2: Alice lock PKT trên chain A
    /// claimer = Bob (người reveal preimage để claim)
    /// refunder = Alice (nếu Bob không tham gia, Alice refund sau T1)
    pub fn alice_lock(
        &mut self,
        alice_wallet: &Wallet,
        bob_wallet:   &Wallet,
    ) -> Result<&HtlcOutput, String> {
        if self.state != SwapState::Proposed {
            return Err("❌ Swap không ở trạng thái Proposed".to_string());
        }

        let alice_hash = hex::encode(Script::pubkey_hash(&alice_wallet.public_key.serialize()));
        let bob_hash   = hex::encode(Script::pubkey_hash(&bob_wallet.public_key.serialize()));

        self.htlc_a = Some(HtlcOutput::new(
            self.hash_lock,
            self.locktime_a,
            self.alice_amount,
            &bob_hash,    // Bob là claimer (chain A)
            &alice_hash,  // Alice là refunder (chain A)
        ));
        self.state = SwapState::AliceLocked;

        println!("  ✅ Alice lock {} sat trên chain A", self.alice_amount);
        println!("     HTLC_A id: {}...", &self.htlc_a.as_ref().unwrap().utxo_id[..16]);
        println!("     hash_lock: {}...", hex::encode(&self.hash_lock[..8]));
        println!("     locktime:  block {}", self.locktime_a);

        Ok(self.htlc_a.as_ref().unwrap())
    }

    /// Step 4: Bob lock LTC trên chain B (sau khi verify HTLC_A)
    /// claimer = Alice (người reveal preimage để claim)
    /// refunder = Bob (nếu Alice không claim kịp T2)
    pub fn bob_lock(
        &mut self,
        alice_wallet: &Wallet,
        bob_wallet:   &Wallet,
    ) -> Result<&HtlcOutput, String> {
        if self.state != SwapState::AliceLocked {
            return Err("❌ Alice chưa lock trên chain A".to_string());
        }

        // Bob verify HTLC_A tồn tại và đúng hash_lock
        let htlc_a = self.htlc_a.as_ref().ok_or("❌ Không có HTLC_A")?;
        if htlc_a.hash_lock != self.hash_lock {
            return Err("❌ hash_lock không khớp".to_string());
        }
        if htlc_a.amount != self.alice_amount {
            return Err("❌ Amount chain A không khớp".to_string());
        }

        let alice_hash = hex::encode(Script::pubkey_hash(&alice_wallet.public_key.serialize()));
        let bob_hash   = hex::encode(Script::pubkey_hash(&bob_wallet.public_key.serialize()));

        self.htlc_b = Some(HtlcOutput::new(
            self.hash_lock,
            self.locktime_b,  // T2 < T1
            self.bob_amount,
            &alice_hash,  // Alice là claimer (chain B)
            &bob_hash,    // Bob là refunder (chain B)
        ));
        self.state = SwapState::BobLocked;

        println!("  ✅ Bob lock {} sat trên chain B", self.bob_amount);
        println!("     HTLC_B id: {}...", &self.htlc_b.as_ref().unwrap().utxo_id[..16]);
        println!("     hash_lock: {}... (CÙNG hash với HTLC_A)", hex::encode(&self.hash_lock[..8]));
        println!("     locktime:  block {} (ngắn hơn HTLC_A)", self.locktime_b);

        Ok(self.htlc_b.as_ref().unwrap())
    }

    /// Step 5: Alice claim LTC từ HTLC_B bằng cách reveal preimage
    pub fn alice_claim(&mut self, alice_wallet: &Wallet) -> Result<Vec<u8>, String> {
        if self.state != SwapState::BobLocked {
            return Err("❌ Bob chưa lock".to_string());
        }

        let preimage = self.preimage.as_ref().ok_or("❌ Không có preimage")?;
        let htlc_b   = self.htlc_b.as_ref().ok_or("❌ Không có HTLC_B")?;

        if !htlc_b.verify_claim(preimage, alice_wallet) {
            return Err("❌ Claim thất bại: preimage hoặc sig không hợp lệ".to_string());
        }

        let revealed = preimage.clone();
        self.revealed_preimage = Some(revealed.clone());
        self.state = SwapState::AliceClaimed;

        println!("  ✅ Alice claim {} sat từ HTLC_B", htlc_b.amount);
        println!("     Preimage lộ on-chain: {}...", hex::encode(&revealed[..8]));
        println!("     Bob có thể thấy preimage và claim HTLC_A");

        Ok(revealed)
    }

    /// Step 6: Bob claim PKT từ HTLC_A bằng preimage Alice đã reveal
    pub fn bob_claim(&mut self, bob_wallet: &Wallet, preimage: &[u8]) -> Result<(), String> {
        if self.state != SwapState::AliceClaimed {
            return Err("❌ Alice chưa reveal preimage".to_string());
        }

        let htlc_a = self.htlc_a.as_ref().ok_or("❌ Không có HTLC_A")?;

        if !htlc_a.verify_claim(preimage, bob_wallet) {
            return Err("❌ Bob claim thất bại: preimage hoặc sig không hợp lệ".to_string());
        }

        self.state = SwapState::Completed;

        println!("  ✅ Bob claim {} sat từ HTLC_A dùng preimage Alice đã lộ", htlc_a.amount);
        println!("  🎉 Atomic Swap HOÀN THÀNH!");

        Ok(())
    }

    /// Refund path: Alice refund sau locktime_a (nếu Bob không lock)
    pub fn alice_refund(
        &mut self,
        alice_wallet: &Wallet,
        current_block: u64,
    ) -> Result<(), String> {
        let htlc_a = self.htlc_a.as_ref().ok_or("❌ Không có HTLC_A")?;
        if !htlc_a.verify_refund(current_block, alice_wallet) {
            return Err(format!(
                "❌ Refund thất bại: block {} < locktime {}",
                current_block, htlc_a.locktime
            ));
        }
        self.state = SwapState::AliceRefunded;
        println!("  ↩ Alice refund {} sat từ HTLC_A (block {})", htlc_a.amount, current_block);
        Ok(())
    }

    /// Refund path: Bob refund sau locktime_b (nếu Alice không claim kịp)
    pub fn bob_refund(
        &mut self,
        bob_wallet:    &Wallet,
        current_block: u64,
    ) -> Result<(), String> {
        let htlc_b = self.htlc_b.as_ref().ok_or("❌ Không có HTLC_B")?;
        if !htlc_b.verify_refund(current_block, bob_wallet) {
            return Err(format!(
                "❌ Refund thất bại: block {} < locktime {}",
                current_block, htlc_b.locktime
            ));
        }
        self.state = SwapState::BobRefunded;
        println!("  ↩ Bob refund {} sat từ HTLC_B (block {})", htlc_b.amount, current_block);
        Ok(())
    }

    pub fn hash_lock_hex(&self) -> String { hex::encode(self.hash_lock) }
}

// ── Helper ────────────────────────────────────────────────────

fn generate_preimage() -> Vec<u8> {
    use secp256k1::rand::RngCore;
    let mut rng = secp256k1::rand::thread_rng();
    let mut r = [0u8; 32];
    rng.fill_bytes(&mut r);
    r.to_vec()
}

// ── SwapVerifier — kiểm tra từ góc nhìn bên ngoài ────────────

/// Observer xác minh atomic swap trên chain
pub struct SwapVerifier;

impl SwapVerifier {
    /// Verify 2 HTLC dùng cùng hash_lock — đây là dấu hiệu của atomic swap
    pub fn verify_linked(htlc_a: &HtlcOutput, htlc_b: &HtlcOutput) -> bool {
        htlc_a.hash_lock == htlc_b.hash_lock
    }

    /// Verify timelock ordering: T_A > T_B (bắt buộc để swap an toàn)
    pub fn verify_timelock_order(htlc_a: &HtlcOutput, htlc_b: &HtlcOutput) -> bool {
        htlc_a.locktime > htlc_b.locktime
    }

    /// Verify preimage hợp lệ với hash_lock
    pub fn verify_preimage(preimage: &[u8], hash_lock: &[u8; 32]) -> bool {
        let h: [u8; 32] = Sha256::digest(preimage).into();
        h == *hash_lock
    }
}
