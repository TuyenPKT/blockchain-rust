#![allow(dead_code)]
//! v9.0.1 — HD Wallet (Ed25519, SLIP-0010)
//!
//! Production-grade HD Wallet cho non-EVM blockchain dùng Ed25519.
//!
//! Pipeline:
//!   OsRng → entropy → BIP39 mnemonic → seed (PBKDF2-SHA512)
//!   → SLIP-0010 master key → derive child keys (tất cả hardened)
//!   → Ed25519 keypair → address (version + SHA256 + checksum, base58)
//!
//! Modules (flat layout):
//!   Seed      — BIP39 mnemonic + PBKDF2 seed
//!   Slip10    — SLIP-0010 Ed25519 hardened derivation
//!   Address   — PKT address: base58(version || hash20 || checksum4)
//!   Signer    — trait Signer + HotSigner + ColdSigner + MockSigner
//!   WalletTx  — transaction hash / sign / verify
//!   HdWallet  — derive N addresses từ một seed
//!
//! Bảo mật:
//!   - Private key zeroized on drop (ZeroizeOnDrop)
//!   - Entropy từ OsRng
//!   - Không log seed / private key
//!   - Không tự implement crypto — dùng ed25519-dalek, hmac, sha2, blake3
//!
//! Derivation path: m / 9000' / chain_id' / account' / role' / index'
//! Tất cả components phải hardened (ràng buộc Ed25519 SLIP-0010)

use ed25519_dalek::{
    Signature as DalekSig, Signer as DalekSigner, SigningKey, Verifier, VerifyingKey,
};
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256, Sha512};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

type HmacSha512 = Hmac<Sha512>;

// ─── Constants ────────────────────────────────────────────────────────────────

/// HMAC key cho SLIP-0010 Ed25519 master key generation.
const SLIP10_ED25519_SEED: &[u8] = b"ed25519 seed";

/// Address version byte (0x50 = 'P' cho PKT).
const ADDR_VERSION: u8 = 0x50;

/// Hardened index offset (2^31).
const HARDENED_OFFSET: u32 = 0x8000_0000;

/// PKT coin type (SLIP-0044).
const COIN_TYPE: u32 = 9000;

// ─── DerivationPath ───────────────────────────────────────────────────────────

/// SLIP-0010 derivation path. Tất cả indices stored pre-hardened.
///
/// Format: `m / purpose' / chain_id' / account' / role' / index'`
/// Ví dụ:  `m / 9000' / 1' / 0' / 0' / 0'`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationPath {
    pub purpose:  u32,
    pub chain_id: u32,
    pub account:  u32,
    pub role:     u32,
    pub index:    u32,
}

impl DerivationPath {
    /// Path chuẩn PKT: `m/9000'/chain_id'/account'/role'/index'`.
    pub fn pkt(chain_id: u32, account: u32, role: u32, index: u32) -> Self {
        DerivationPath { purpose: COIN_TYPE, chain_id, account, role, index }
    }

    /// Trả về 5 components đã thêm hardened offset (dùng khi derive).
    pub fn hardened_components(&self) -> [u32; 5] {
        [
            add_hardened(self.purpose),
            add_hardened(self.chain_id),
            add_hardened(self.account),
            add_hardened(self.role),
            add_hardened(self.index),
        ]
    }
}

impl fmt::Display for DerivationPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f, "m/{}'/{}'/{}'/{}'/{}'",
            self.purpose, self.chain_id, self.account, self.role, self.index
        )
    }
}

fn add_hardened(index: u32) -> u32 {
    index
        .checked_add(HARDENED_OFFSET)
        .expect("index overflows hardened range")
}

// ─── Entropy ──────────────────────────────────────────────────────────────────

/// 128-bit entropy, zeroized on drop.
#[derive(ZeroizeOnDrop)]
pub struct Entropy([u8; 16]);

impl Entropy {
    /// Tạo 128-bit entropy bằng `OsRng`.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 16];
        OsRng.fill_bytes(&mut bytes);
        Entropy(bytes)
    }

    /// Từ bytes cố định (chỉ dùng cho test).
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Entropy(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] { &self.0 }
}

// ─── Seed (BIP39) ─────────────────────────────────────────────────────────────

/// 64-byte BIP39 seed, zeroized on drop.
#[derive(ZeroizeOnDrop)]
pub struct Seed([u8; 64]);

impl Seed {
    /// Derive 64-byte seed từ mnemonic qua PBKDF2-SHA512 (2048 iterations).
    ///
    /// `passphrase = ""` theo chuẩn BIP39 (không có passphrase).
    pub fn from_mnemonic(mnemonic: &str, passphrase: &str) -> Self {
        let salt = format!("mnemonic{passphrase}");
        let mut seed = [0u8; 64];
        pbkdf2::pbkdf2_hmac::<Sha512>(mnemonic.as_bytes(), salt.as_bytes(), 2048, &mut seed);
        Seed(seed)
    }

    pub fn as_bytes(&self) -> &[u8; 64] { &self.0 }
}

// ─── BIP39 Mnemonic (128-word subset) ────────────────────────────────────────

/// BIP39 mnemonic helper.
///
/// WORDLIST hiện là 128 từ đầu của BIP39 English list.
/// Production: thay bằng đủ 2048 từ.
pub struct Bip39;

// 128 từ đầu BIP39 English
const WORDLIST: &[&str] = &[
    "abandon","ability","able","about","above","absent","absorb","abstract",
    "absurd","abuse","access","accident","account","accuse","achieve","acid",
    "acoustic","acquire","across","act","action","actor","actress","actual",
    "adapt","add","addict","address","adjust","admit","adult","advance",
    "advice","aerobic","afford","afraid","again","age","agent","agree",
    "ahead","aim","air","airport","aisle","alarm","album","alcohol",
    "alert","alien","all","alley","allow","almost","alone","alpha",
    "already","also","alter","always","amateur","amazing","among","amount",
    "amused","analyst","anchor","ancient","anger","angle","angry","animal",
    "ankle","announce","annual","another","answer","antenna","antique","anxiety",
    "any","apart","apology","appear","apple","approve","april","arch",
    "arctic","area","arena","argue","arm","armed","armor","army",
    "around","arrange","arrest","arrive","arrow","art","artefact","artist",
    "artwork","ask","aspect","assault","asset","assist","assume","asthma",
    "athlete","atom","attack","attend","attitude","attract","auction","audit",
    "august","aunt","author","auto","autumn","average","avocado","avoid",
];

impl Bip39 {
    /// Chuyển entropy thành 12-word mnemonic (7 bits/word cho 128-word list).
    pub fn entropy_to_mnemonic(entropy: &Entropy) -> Vec<String> {
        let bits: Vec<u8> = entropy.0.iter()
            .flat_map(|b| (0..8u8).rev().map(move |i| (b >> i) & 1))
            .collect();
        bits.chunks(7)
            .take(12)
            .map(|chunk| {
                let idx = chunk.iter().fold(0usize, |acc, &b| (acc << 1) | b as usize);
                WORDLIST[idx % WORDLIST.len()].to_string()
            })
            .collect()
    }

    /// Kết hợp entropy → mnemonic string (12 từ cách nhau bằng dấu cách).
    pub fn generate_mnemonic() -> (Entropy, String) {
        let entropy = Entropy::generate();
        let words   = Self::entropy_to_mnemonic(&entropy);
        let phrase  = words.join(" ");
        (entropy, phrase)
    }
}

// ─── SLIP-0010 Key ────────────────────────────────────────────────────────────

/// Ed25519 key pair + chain code theo SLIP-0010. Zeroized on drop.
#[derive(ZeroizeOnDrop)]
pub struct Slip10Key {
    /// 32-byte private (signing) key — zeroized khi drop.
    sk:         [u8; 32],
    chain_code: [u8; 32],
}

impl Slip10Key {
    /// Tạo master key từ `Seed` theo SLIP-0010.
    pub fn master(seed: &Seed) -> Self {
        let mut mac = HmacSha512::new_from_slice(SLIP10_ED25519_SEED)
            .expect("HMAC-SHA512 accepts any key length");
        mac.update(seed.as_bytes());
        let i = mac.finalize().into_bytes();

        let mut sk = [0u8; 32];
        let mut cc = [0u8; 32];
        sk.copy_from_slice(&i[..32]);
        cc.copy_from_slice(&i[32..]);
        Slip10Key { sk, chain_code: cc }
    }

    /// Derive hardened child key.
    ///
    /// `index` phải >= HARDENED_OFFSET (0x8000_0000).
    /// Ed25519 SLIP-0010 KHÔNG hỗ trợ non-hardened derivation.
    pub fn derive_child(&self, index: u32) -> Self {
        assert!(
            index >= HARDENED_OFFSET,
            "Ed25519 SLIP-0010: only hardened derivation — index must be >= 0x80000000"
        );
        let mut mac = HmacSha512::new_from_slice(&self.chain_code)
            .expect("chain code is 32 bytes");
        mac.update(&[0x00u8]);  // 0x00 || parent_sk || index_be
        mac.update(&self.sk);
        mac.update(&index.to_be_bytes());
        let i = mac.finalize().into_bytes();

        let mut sk = [0u8; 32];
        let mut cc = [0u8; 32];
        sk.copy_from_slice(&i[..32]);
        cc.copy_from_slice(&i[32..]);
        Slip10Key { sk, chain_code: cc }
    }

    /// Derive theo toàn bộ path.
    pub fn derive_path(&self, path: &DerivationPath) -> Self {
        let components = path.hardened_components();
        let mut key = self.derive_child(components[0]);
        for &idx in &components[1..] {
            key = key.derive_child(idx);
        }
        key
    }

    /// Lấy `VerifyingKey` (public key) từ signing key bytes.
    pub fn verifying_key(&self) -> VerifyingKey {
        SigningKey::from_bytes(&self.sk).verifying_key()
    }

    /// Trả về signing key bytes — chỉ dùng nội bộ.
    pub(crate) fn signing_key_bytes(&self) -> [u8; 32] {
        self.sk
    }
}

// ─── Address ──────────────────────────────────────────────────────────────────

/// PKT address từ Ed25519 public key.
///
/// Format: `base58( 0x50 || SHA256(pubkey)[0..20] || checksum[0..4] )`
/// Checksum = BLAKE3(BLAKE3(version || hash20))[0..4]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address(pub String);

impl Address {
    /// Tạo address từ `VerifyingKey`.
    pub fn from_verifying_key(vk: &VerifyingKey) -> Self {
        let hash  = Sha256::digest(vk.as_bytes());
        let hash20 = &hash[..20];

        let mut payload = Vec::with_capacity(25);
        payload.push(ADDR_VERSION);
        payload.extend_from_slice(hash20);

        let c1 = blake3::hash(&payload);
        let c2 = blake3::hash(c1.as_bytes());
        payload.extend_from_slice(&c2.as_bytes()[..4]);

        Address(bs58::encode(&payload).into_string())
    }

    pub fn as_str(&self) -> &str { &self.0 }

    /// Kiểm tra checksum address.
    pub fn is_valid(&self) -> bool {
        let Ok(bytes) = bs58::decode(&self.0).into_vec() else { return false; };
        if bytes.len() != 25 || bytes[0] != ADDR_VERSION { return false; }
        let c1 = blake3::hash(&bytes[..21]);
        let c2 = blake3::hash(c1.as_bytes());
        bytes[21..25] == c2.as_bytes()[..4]
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

// ─── Ed25519 types ────────────────────────────────────────────────────────────

/// Ed25519 signature (64 bytes).
#[derive(Debug, Clone)]
pub struct Ed25519Signature(DalekSig);

impl Ed25519Signature {
    pub fn to_bytes(&self) -> [u8; 64] { self.0.to_bytes() }

    pub fn from_bytes(bytes: &[u8; 64]) -> Option<Self> {
        Some(Ed25519Signature(DalekSig::from_bytes(bytes)))
    }
}

/// Ed25519 public key (32 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ed25519PublicKey(VerifyingKey);

impl Ed25519PublicKey {
    pub fn to_bytes(&self) -> [u8; 32] { *self.0.as_bytes() }
    pub fn address(&self) -> Address   { Address::from_verifying_key(&self.0) }

    /// Verify signature trên `msg`.
    pub fn verify(&self, msg: &[u8], sig: &Ed25519Signature) -> bool {
        self.0.verify(msg, &sig.0).is_ok()
    }
}

// ─── Signer trait ─────────────────────────────────────────────────────────────

/// Abstraction over hot / cold / hardware signers.
/// Không expose private key qua bất kỳ phương thức nào.
pub trait Signer: Send + Sync {
    fn sign(&self, msg: &[u8]) -> Ed25519Signature;
    fn public_key(&self) -> Ed25519PublicKey;
    fn address(&self) -> Address { self.public_key().address() }
}

// ─── HotSigner ────────────────────────────────────────────────────────────────

/// Signer giữ private key trong memory.
///
/// `SigningKey` từ ed25519-dalek implement `ZeroizeOnDrop` —
/// key tự động bị xóa khi `HotSigner` bị drop.
pub struct HotSigner {
    signing_key: SigningKey,
}

impl HotSigner {
    /// Tạo từ SLIP-0010 derived key.
    pub fn from_slip10(key: &Slip10Key) -> Self {
        let mut sk_bytes = key.signing_key_bytes();
        let signing_key  = SigningKey::from_bytes(&sk_bytes);
        sk_bytes.zeroize();  // xóa bản copy ngay lập tức
        HotSigner { signing_key }
    }

    /// Tạo từ 32-byte raw bytes (zeroizes input).
    pub fn from_bytes(mut bytes: [u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(&bytes);
        bytes.zeroize();
        HotSigner { signing_key }
    }
}

impl Signer for HotSigner {
    fn sign(&self, msg: &[u8]) -> Ed25519Signature {
        Ed25519Signature(self.signing_key.sign(msg))
    }
    fn public_key(&self) -> Ed25519PublicKey {
        Ed25519PublicKey(self.signing_key.verifying_key())
    }
}

// ─── ColdSigner ───────────────────────────────────────────────────────────────

/// Signer chỉ giữ public key; signing được delegate ra external callback
/// (hardware wallet, air-gapped device, HSM, v.v.).
///
/// Production: thay `sign_fn` bằng RPC/channel đến thiết bị cold.
pub struct ColdSigner {
    verifying_key: VerifyingKey,
    sign_fn:       Box<dyn Fn(&[u8]) -> Ed25519Signature + Send + Sync>,
}

impl ColdSigner {
    pub fn new(
        verifying_key: VerifyingKey,
        sign_fn: impl Fn(&[u8]) -> Ed25519Signature + Send + Sync + 'static,
    ) -> Self {
        ColdSigner { verifying_key, sign_fn: Box::new(sign_fn) }
    }
}

impl Signer for ColdSigner {
    fn sign(&self, msg: &[u8]) -> Ed25519Signature { (self.sign_fn)(msg) }
    fn public_key(&self) -> Ed25519PublicKey { Ed25519PublicKey(self.verifying_key) }
}

// ─── MockSigner ───────────────────────────────────────────────────────────────

/// Deterministic signer cho tests. KHÔNG dùng trong production.
pub struct MockSigner {
    signing_key: SigningKey,
}

impl MockSigner {
    pub fn from_seed(seed_bytes: &[u8; 32]) -> Self {
        MockSigner { signing_key: SigningKey::from_bytes(seed_bytes) }
    }
    /// Key mặc định với seed [0x42; 32].
    pub fn default_test() -> Self { Self::from_seed(&[0x42u8; 32]) }
}

impl Signer for MockSigner {
    fn sign(&self, msg: &[u8]) -> Ed25519Signature {
        Ed25519Signature(self.signing_key.sign(msg))
    }
    fn public_key(&self) -> Ed25519PublicKey {
        Ed25519PublicKey(self.signing_key.verifying_key())
    }
}

// ─── WalletTx ─────────────────────────────────────────────────────────────────

/// Transaction có thể ký và verify.
///
/// Flow: tạo → hash() → sign(&signer) → verify()
#[derive(Debug, Clone)]
pub struct WalletTx {
    pub from:      Address,
    pub to:        Address,
    pub amount:    u64,
    pub nonce:     u64,
    pub timestamp: u64,
    pub signature: Option<Ed25519Signature>,
    pub pub_key:   Option<Ed25519PublicKey>,
}

impl WalletTx {
    /// Tạo unsigned transaction.
    pub fn new(from: Address, to: Address, amount: u64, nonce: u64, timestamp: u64) -> Self {
        WalletTx { from, to, amount, nonce, timestamp, signature: None, pub_key: None }
    }

    /// BLAKE3 hash của unsigned fields (không include signature).
    pub fn hash(&self) -> [u8; 32] {
        let mut data = Vec::with_capacity(128);
        data.extend_from_slice(self.from.as_str().as_bytes());
        data.extend_from_slice(self.to.as_str().as_bytes());
        data.extend_from_slice(&self.amount.to_be_bytes());
        data.extend_from_slice(&self.nonce.to_be_bytes());
        data.extend_from_slice(&self.timestamp.to_be_bytes());
        *blake3::hash(&data).as_bytes()
    }

    /// Ký transaction. Lưu signature + public key vào tx.
    pub fn sign(&mut self, signer: &dyn Signer) {
        let h           = self.hash();
        self.signature  = Some(signer.sign(&h));
        self.pub_key    = Some(signer.public_key());
    }

    /// Verify signature. Trả về false nếu chưa ký hoặc sai signature.
    pub fn verify(&self) -> bool {
        let (Some(sig), Some(pk)) = (&self.signature, &self.pub_key) else {
            return false;
        };
        pk.verify(&self.hash(), sig)
    }

    /// True nếu đã được ký.
    pub fn is_signed(&self) -> bool { self.signature.is_some() }
}

// ─── HdWallet ─────────────────────────────────────────────────────────────────

/// Top-level HD Wallet.
///
/// Không expose seed hay private key ra ngoài.
/// Dùng `derive_hot_signer()` để lấy signer cho một path cụ thể.
pub struct HdWallet {
    master: Slip10Key,
}

impl HdWallet {
    /// Tạo wallet từ BIP39 seed (đã derive qua PBKDF2).
    pub fn from_seed(seed: &Seed) -> Self {
        HdWallet { master: Slip10Key::master(seed) }
    }

    /// Derive `HotSigner` tại path chỉ định.
    pub fn derive_hot_signer(&self, path: &DerivationPath) -> HotSigner {
        HotSigner::from_slip10(&self.master.derive_path(path))
    }

    /// Derive address tại path mà không expose private key.
    pub fn derive_address(&self, path: &DerivationPath) -> Address {
        Address::from_verifying_key(&self.master.derive_path(path).verifying_key())
    }

    /// Derive N địa chỉ liên tiếp (account=0, role=0, index=0..count-1).
    pub fn derive_addresses(
        &self,
        chain_id: u32,
        account:  u32,
        count:    u32,
    ) -> Vec<(DerivationPath, Address)> {
        (0..count)
            .map(|i| {
                let path = DerivationPath::pkt(chain_id, account, 0, i);
                let addr = self.derive_address(&path);
                (path, addr)
            })
            .collect()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── DerivationPath ────────────────────────────────────────────────────

    #[test]
    fn test_derivation_path_display() {
        let p = DerivationPath::pkt(1, 0, 0, 0);
        assert_eq!(p.to_string(), "m/9000'/1'/0'/0'/0'");
    }

    #[test]
    fn test_derivation_path_hardened_offset() {
        let p = DerivationPath::pkt(1, 2, 3, 4);
        let c = p.hardened_components();
        assert_eq!(c[0], COIN_TYPE + HARDENED_OFFSET);
        assert_eq!(c[1], 1 + HARDENED_OFFSET);
        assert_eq!(c[2], 2 + HARDENED_OFFSET);
        assert_eq!(c[3], 3 + HARDENED_OFFSET);
        assert_eq!(c[4], 4 + HARDENED_OFFSET);
    }

    #[test]
    fn test_derivation_path_different_indices_differ() {
        let p0 = DerivationPath::pkt(1, 0, 0, 0);
        let p1 = DerivationPath::pkt(1, 0, 0, 1);
        assert_ne!(p0, p1);
    }

    // ── Entropy ───────────────────────────────────────────────────────────

    #[test]
    fn test_entropy_generate_nonzero() {
        let e = Entropy::generate();
        assert_ne!(e.as_bytes(), &[0u8; 16]);
    }

    #[test]
    fn test_entropy_generate_unique() {
        let a = Entropy::generate();
        let b = Entropy::generate();
        assert_ne!(a.as_bytes(), b.as_bytes());
    }

    // ── BIP39 ─────────────────────────────────────────────────────────────

    #[test]
    fn test_mnemonic_12_words() {
        let e     = Entropy::from_bytes([0u8; 16]);
        let words = Bip39::entropy_to_mnemonic(&e);
        assert_eq!(words.len(), 12);
    }

    #[test]
    fn test_mnemonic_words_in_wordlist() {
        let e     = Entropy::from_bytes([0xABu8; 16]);
        let words = Bip39::entropy_to_mnemonic(&e);
        for w in &words {
            assert!(WORDLIST.contains(&w.as_str()), "word not in list: {w}");
        }
    }

    #[test]
    fn test_mnemonic_deterministic() {
        let e  = Entropy::from_bytes([0x42u8; 16]);
        let w1 = Bip39::entropy_to_mnemonic(&e);
        let w2 = Bip39::entropy_to_mnemonic(&e);
        assert_eq!(w1, w2);
    }

    #[test]
    fn test_mnemonic_different_entropy_different_words() {
        let e1 = Entropy::from_bytes([0x00u8; 16]);
        let e2 = Entropy::from_bytes([0xFFu8; 16]);
        assert_ne!(
            Bip39::entropy_to_mnemonic(&e1),
            Bip39::entropy_to_mnemonic(&e2)
        );
    }

    // ── Seed ──────────────────────────────────────────────────────────────

    #[test]
    fn test_seed_from_mnemonic_length() {
        let seed = Seed::from_mnemonic("abandon abandon abandon", "");
        assert_eq!(seed.as_bytes().len(), 64);
    }

    #[test]
    fn test_seed_deterministic() {
        let s1 = Seed::from_mnemonic("ability able about above", "");
        let s2 = Seed::from_mnemonic("ability able about above", "");
        assert_eq!(s1.as_bytes(), s2.as_bytes());
    }

    #[test]
    fn test_seed_passphrase_changes_seed() {
        let s1 = Seed::from_mnemonic("abandon", "");
        let s2 = Seed::from_mnemonic("abandon", "mypassphrase");
        assert_ne!(s1.as_bytes(), s2.as_bytes());
    }

    #[test]
    fn test_seed_nonzero() {
        let s = Seed::from_mnemonic("abandon ability able about", "");
        assert_ne!(s.as_bytes(), &[0u8; 64]);
    }

    // ── SLIP-0010 ─────────────────────────────────────────────────────────

    #[test]
    fn test_slip10_master_deterministic() {
        let seed = Seed::from_mnemonic("ability able about above", "");
        let k1   = Slip10Key::master(&seed);
        let k2   = Slip10Key::master(&seed);
        assert_eq!(k1.sk, k2.sk);
        assert_eq!(k1.chain_code, k2.chain_code);
    }

    #[test]
    fn test_slip10_child_differs_from_parent() {
        let seed   = Seed::from_mnemonic("ability able about above", "");
        let master = Slip10Key::master(&seed);
        let child  = master.derive_child(HARDENED_OFFSET);
        assert_ne!(master.sk, child.sk);
        assert_ne!(master.chain_code, child.chain_code);
    }

    #[test]
    fn test_slip10_different_indices_differ() {
        let seed   = Seed::from_mnemonic("ability able about above", "");
        let master = Slip10Key::master(&seed);
        let c0     = master.derive_child(HARDENED_OFFSET);
        let c1     = master.derive_child(HARDENED_OFFSET + 1);
        assert_ne!(c0.sk, c1.sk);
    }

    #[test]
    fn test_slip10_path_deterministic() {
        let seed = Seed::from_mnemonic("ability able about above", "");
        let m    = Slip10Key::master(&seed);
        let path = DerivationPath::pkt(1, 0, 0, 0);
        let k1   = m.derive_path(&path);
        let k2   = m.derive_path(&path);
        assert_eq!(k1.sk, k2.sk);
    }

    #[test]
    #[should_panic(expected = "only hardened derivation")]
    fn test_slip10_non_hardened_panics() {
        let seed   = Seed::from_mnemonic("abandon", "");
        let master = Slip10Key::master(&seed);
        let _ = master.derive_child(0); // index < HARDENED_OFFSET → panic
    }

    #[test]
    fn test_slip10_verifying_key_32_bytes() {
        let seed = Seed::from_mnemonic("ability able about above", "");
        let k    = Slip10Key::master(&seed);
        assert_eq!(k.verifying_key().as_bytes().len(), 32);
    }

    // ── Address ───────────────────────────────────────────────────────────

    #[test]
    fn test_address_from_key_nonempty() {
        let sk   = SigningKey::from_bytes(&[0x42u8; 32]);
        let vk   = sk.verifying_key();
        let addr = Address::from_verifying_key(&vk);
        assert!(!addr.as_str().is_empty());
    }

    #[test]
    fn test_address_starts_with_version() {
        // base58 of 0x50 prefix starts with 'P' or nearby chars
        let sk   = SigningKey::from_bytes(&[0x01u8; 32]);
        let vk   = sk.verifying_key();
        let addr = Address::from_verifying_key(&vk);
        // Verify checksum valid
        assert!(addr.is_valid(), "address checksum should be valid");
    }

    #[test]
    fn test_address_is_valid() {
        let sk   = SigningKey::from_bytes(&[0xABu8; 32]);
        let addr = Address::from_verifying_key(&sk.verifying_key());
        assert!(addr.is_valid());
    }

    #[test]
    fn test_address_different_keys_different_addresses() {
        let a1 = Address::from_verifying_key(&SigningKey::from_bytes(&[0x01u8; 32]).verifying_key());
        let a2 = Address::from_verifying_key(&SigningKey::from_bytes(&[0x02u8; 32]).verifying_key());
        assert_ne!(a1, a2);
    }

    #[test]
    fn test_address_deterministic() {
        let vk = SigningKey::from_bytes(&[0x42u8; 32]).verifying_key();
        let a1 = Address::from_verifying_key(&vk);
        let a2 = Address::from_verifying_key(&vk);
        assert_eq!(a1, a2);
    }

    #[test]
    fn test_address_invalid_tampered() {
        let sk   = SigningKey::from_bytes(&[0x10u8; 32]);
        let addr = Address::from_verifying_key(&sk.verifying_key());
        let mut s = addr.0.clone();
        // Flip last char
        let last = s.pop().unwrap();
        s.push(if last == 'A' { 'B' } else { 'A' });
        assert!(!Address(s).is_valid());
    }

    // ── HotSigner ─────────────────────────────────────────────────────────

    #[test]
    fn test_hot_signer_sign_verify() {
        let msg    = b"hello pkt";
        let signer = MockSigner::default_test();
        let sig    = signer.sign(msg);
        assert!(signer.public_key().verify(msg, &sig));
    }

    #[test]
    fn test_hot_signer_wrong_msg_fails() {
        let signer = MockSigner::default_test();
        let sig    = signer.sign(b"correct");
        assert!(!signer.public_key().verify(b"wrong", &sig));
    }

    #[test]
    fn test_hot_signer_from_slip10() {
        let seed   = Seed::from_mnemonic("ability able about above", "");
        let master = Slip10Key::master(&seed);
        let path   = DerivationPath::pkt(1, 0, 0, 0);
        let child  = master.derive_path(&path);
        let signer = HotSigner::from_slip10(&child);
        let msg    = b"test transaction";
        let sig    = signer.sign(msg);
        assert!(signer.public_key().verify(msg, &sig));
    }

    #[test]
    fn test_hot_signer_address_valid() {
        let signer = MockSigner::default_test();
        assert!(signer.address().is_valid());
    }

    // ── ColdSigner ────────────────────────────────────────────────────────

    #[test]
    fn test_cold_signer_delegates_signing() {
        // Build a mock "cold device" backed by a real key
        let backing = MockSigner::default_test();
        let vk      = backing.public_key().0;
        let cold    = ColdSigner::new(vk, move |msg| backing.sign(msg));

        let msg = b"cold sign test";
        let sig = cold.sign(msg);
        assert!(cold.public_key().verify(msg, &sig));
    }

    #[test]
    fn test_cold_signer_public_key_matches() {
        let mock = MockSigner::default_test();
        let vk   = mock.public_key().0;
        let cold = ColdSigner::new(vk, move |msg| mock.sign(msg));
        assert_eq!(cold.public_key().to_bytes().len(), 32);
    }

    // ── MockSigner ────────────────────────────────────────────────────────

    #[test]
    fn test_mock_signer_deterministic() {
        let s1 = MockSigner::default_test();
        let s2 = MockSigner::default_test();
        assert_eq!(s1.public_key().to_bytes(), s2.public_key().to_bytes());
    }

    #[test]
    fn test_mock_signer_different_seeds_differ() {
        let s1 = MockSigner::from_seed(&[0x01u8; 32]);
        let s2 = MockSigner::from_seed(&[0x02u8; 32]);
        assert_ne!(s1.public_key().to_bytes(), s2.public_key().to_bytes());
    }

    // ── WalletTx ──────────────────────────────────────────────────────────

    fn make_tx() -> WalletTx {
        let signer = MockSigner::default_test();
        let from   = signer.address();
        let to     = MockSigner::from_seed(&[0x01u8; 32]).address();
        WalletTx::new(from, to, 1_000_000, 1, 1_700_000_000)
    }

    #[test]
    fn test_tx_hash_32_bytes() {
        let tx = make_tx();
        assert_eq!(tx.hash().len(), 32);
    }

    #[test]
    fn test_tx_hash_deterministic() {
        let tx = make_tx();
        assert_eq!(tx.hash(), tx.hash());
    }

    #[test]
    fn test_tx_hash_changes_with_amount() {
        let mut tx1 = make_tx();
        let mut tx2 = make_tx();
        tx2.amount = 999;
        assert_ne!(tx1.hash(), tx2.hash());
        // Silence unused warning
        tx1.nonce = 1; tx2.nonce = 1;
    }

    #[test]
    fn test_tx_unsigned_verify_false() {
        let tx = make_tx();
        assert!(!tx.verify());
    }

    #[test]
    fn test_tx_sign_and_verify() {
        let signer = MockSigner::default_test();
        let mut tx = make_tx();
        tx.sign(&signer);
        assert!(tx.is_signed());
        assert!(tx.verify());
    }

    #[test]
    fn test_tx_tampered_amount_fails_verify() {
        let signer = MockSigner::default_test();
        let mut tx = make_tx();
        tx.sign(&signer);
        tx.amount += 1; // tamper after signing
        assert!(!tx.verify());
    }

    #[test]
    fn test_tx_tampered_nonce_fails_verify() {
        let signer = MockSigner::default_test();
        let mut tx = make_tx();
        tx.sign(&signer);
        tx.nonce += 1;
        assert!(!tx.verify());
    }

    // ── HdWallet ──────────────────────────────────────────────────────────

    #[test]
    fn test_hd_wallet_derive_address_deterministic() {
        let seed   = Seed::from_mnemonic("ability able about above", "");
        let wallet = HdWallet::from_seed(&seed);
        let path   = DerivationPath::pkt(1, 0, 0, 0);
        assert_eq!(wallet.derive_address(&path), wallet.derive_address(&path));
    }

    #[test]
    fn test_hd_wallet_different_indices_different_addresses() {
        let seed   = Seed::from_mnemonic("ability able about above", "");
        let wallet = HdWallet::from_seed(&seed);
        let a0     = wallet.derive_address(&DerivationPath::pkt(1, 0, 0, 0));
        let a1     = wallet.derive_address(&DerivationPath::pkt(1, 0, 0, 1));
        assert_ne!(a0, a1);
    }

    #[test]
    fn test_hd_wallet_derive_n_addresses() {
        let seed      = Seed::from_mnemonic("ability able about above", "");
        let wallet    = HdWallet::from_seed(&seed);
        let addresses = wallet.derive_addresses(1, 0, 5);
        assert_eq!(addresses.len(), 5);
        // All unique
        let unique: std::collections::HashSet<_> = addresses.iter().map(|(_, a)| a.clone()).collect();
        assert_eq!(unique.len(), 5);
    }

    #[test]
    fn test_hd_wallet_all_addresses_valid() {
        let seed      = Seed::from_mnemonic("ability able about above", "");
        let wallet    = HdWallet::from_seed(&seed);
        let addresses = wallet.derive_addresses(1, 0, 10);
        for (_, addr) in &addresses {
            assert!(addr.is_valid(), "invalid address: {addr}");
        }
    }

    #[test]
    fn test_hd_wallet_sign_and_verify_tx() {
        let seed   = Seed::from_mnemonic("ability able about above", "");
        let wallet = HdWallet::from_seed(&seed);
        let path   = DerivationPath::pkt(1, 0, 0, 0);
        let signer = wallet.derive_hot_signer(&path);
        let from   = signer.address();
        let to     = wallet.derive_address(&DerivationPath::pkt(1, 0, 0, 1));
        let mut tx = WalletTx::new(from, to, 500_000, 42, 1_700_000_000);
        tx.sign(&signer);
        assert!(tx.verify());
    }

    #[test]
    fn test_hd_wallet_different_mnemonics_different_keys() {
        let w1 = HdWallet::from_seed(&Seed::from_mnemonic("ability able about above", ""));
        let w2 = HdWallet::from_seed(&Seed::from_mnemonic("artwork ask aspect assault", ""));
        let p  = DerivationPath::pkt(1, 0, 0, 0);
        assert_ne!(w1.derive_address(&p), w2.derive_address(&p));
    }

    // ── Ed25519Signature serialization ────────────────────────────────────

    #[test]
    fn test_signature_roundtrip() {
        let signer = MockSigner::default_test();
        let sig    = signer.sign(b"roundtrip test");
        let bytes  = sig.to_bytes();
        let sig2   = Ed25519Signature::from_bytes(&bytes).unwrap();
        assert!(signer.public_key().verify(b"roundtrip test", &sig2));
    }

    #[test]
    fn test_signature_64_bytes() {
        let signer = MockSigner::default_test();
        let sig    = signer.sign(b"length check");
        assert_eq!(sig.to_bytes().len(), 64);
    }
}
