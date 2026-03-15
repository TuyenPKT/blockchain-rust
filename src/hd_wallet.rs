#![allow(dead_code)]
/// BIP39: Mnemonic phrase (12/24 từ) → Seed (512-bit)
/// BIP32: Seed → Master Key → child keys theo path
/// BIP44: path chuẩn m/44'/0'/account'/change/index
///
/// Pipeline:
///   entropy (128 bit) → mnemonic 12 từ → seed (PBKDF2) → master key (HMAC-SHA512)
///   → derive theo path → keypair → address

use sha2::{Sha256, Sha512, Digest};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use secp256k1::{Secp256k1, SecretKey, PublicKey};
use crate::wallet::Wallet;

type HmacSha512 = Hmac<Sha512>;

// ── BIP39 Wordlist (rút gọn 128 từ cho demo — production cần đủ 2048 từ) ──
// Trong Bitcoin thật dùng wordlist chuẩn BIP39: https://github.com/trezor/python-mnemonic
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
    "awake","aware","away","awesome","awful","awkward","axis","baby",
];

/// Tạo entropy ngẫu nhiên 16 bytes (128 bit) → mnemonic 12 từ
pub fn generate_entropy() -> [u8; 16] {
    use secp256k1::rand::RngCore;
    let mut entropy = [0u8; 16];
    secp256k1::rand::thread_rng().fill_bytes(&mut entropy);
    entropy
}

/// BIP39: entropy → mnemonic
/// 128 bit entropy → 12 words (mỗi từ = 11 bit index vào wordlist 2048 từ)
pub fn entropy_to_mnemonic(entropy: &[u8]) -> Vec<String> {
    // Checksum = SHA256(entropy)[0..len/32 bits]
    let hash     = Sha256::digest(entropy);
    let cs_bits  = entropy.len() * 8 / 32; // 128bit → 4 bits checksum

    // Ghép entropy + checksum thành bit array
    let mut bits: Vec<bool> = vec![];
    for byte in entropy {
        for i in (0..8).rev() {
            bits.push((byte >> i) & 1 == 1);
        }
    }
    for i in (8 - cs_bits..8).rev() {
        bits.push((hash[0] >> i) & 1 == 1);
    }

    // Mỗi 11 bit → 1 chỉ số trong wordlist
    bits.chunks(11)
        .map(|chunk| {
            let idx = chunk.iter().fold(0usize, |acc, &b| (acc << 1) | b as usize);
            WORDLIST[idx % WORDLIST.len()].to_string() // mod để tránh out-of-bounds với wordlist rút gọn
        })
        .collect()
}

/// BIP39: mnemonic + passphrase → seed 64 bytes (PBKDF2-HMAC-SHA512, 2048 rounds)
pub fn mnemonic_to_seed(mnemonic: &[String], passphrase: &str) -> [u8; 64] {
    let mnemonic_str = mnemonic.join(" ");
    let salt         = format!("mnemonic{}", passphrase);

    let mut seed = [0u8; 64];
    pbkdf2_hmac::<Sha512>(
        mnemonic_str.as_bytes(),
        salt.as_bytes(),
        2048,
        &mut seed,
    );
    seed
}

/// BIP32 Extended Key — đại diện cho 1 node trong cây key
#[derive(Debug, Clone)]
pub struct ExtendedKey {
    pub key:       [u8; 32], // private key (32 bytes)
    pub chain_code: [u8; 32], // chain code (32 bytes)
    pub depth:     u8,
    pub index:     u32,
}

impl ExtendedKey {
    /// BIP32: Seed → Master Extended Key
    /// Master key = HMAC-SHA512( key="Bitcoin seed", data=seed )
    pub fn from_seed(seed: &[u8]) -> Self {
        let mut mac = HmacSha512::new_from_slice(b"Bitcoin seed").unwrap();
        mac.update(seed);
        let result = mac.finalize().into_bytes();

        let mut key        = [0u8; 32];
        let mut chain_code = [0u8; 32];
        key.copy_from_slice(&result[..32]);
        chain_code.copy_from_slice(&result[32..]);

        ExtendedKey { key, chain_code, depth: 0, index: 0 }
    }

    /// BIP32: Child key derivation
    /// index >= 0x80000000 → hardened (dùng private key)
    /// index <  0x80000000 → normal  (dùng public key)
    pub fn derive_child(&self, index: u32) -> Self {
        let secp   = Secp256k1::new();
        let secret = SecretKey::from_slice(&self.key).unwrap();
        let pubkey = PublicKey::from_secret_key(&secp, &secret);

        let mut data = vec![];
        if index >= 0x80000000 {
            // Hardened: 0x00 + private_key + index
            data.push(0x00);
            data.extend_from_slice(&self.key);
        } else {
            // Normal: compressed_pubkey + index
            data.extend_from_slice(&pubkey.serialize());
        }
        data.extend_from_slice(&index.to_be_bytes());

        let mut mac = HmacSha512::new_from_slice(&self.chain_code).unwrap();
        mac.update(&data);
        let result = mac.finalize().into_bytes();

        // child_key = (IL + parent_key) mod n  — dùng tweak_add_assign
        let mut il = [0u8; 32];
        il.copy_from_slice(&result[..32]);
        let mut child_secret = SecretKey::from_slice(&self.key).unwrap();
        child_secret = child_secret.add_tweak(&secp256k1::Scalar::from_be_bytes(il).unwrap()).unwrap();

        let mut chain_code = [0u8; 32];
        chain_code.copy_from_slice(&result[32..]);

        ExtendedKey {
            key: child_secret.secret_bytes(),
            chain_code,
            depth: self.depth + 1,
            index,
        }
    }

    /// Derive theo BIP44 path string như "m/44'/0'/0'/0/0"
    /// ' sau số = hardened (index + 0x80000000)
    pub fn derive_path(&self, path: &str) -> Self {
        let mut current = self.clone();
        for part in path.split('/').skip(1) { // bỏ "m"
            let (index_str, hardened) = if part.ends_with('\'') {
                (&part[..part.len()-1], true)
            } else {
                (part, false)
            };
            let mut index: u32 = index_str.parse().unwrap_or(0);
            if hardened { index += 0x80000000; }
            current = current.derive_child(index);
        }
        current
    }

    /// Chuyển thành Wallet (lấy keypair từ derived key)
    pub fn to_wallet(&self) -> Wallet {
        let secp       = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&self.key).unwrap();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let address    = Wallet::pubkey_to_address(&public_key);
        Wallet { secret_key, public_key, address }
    }
}

/// HD Wallet — quản lý toàn bộ cây key từ 1 seed phrase
pub struct HdWallet {
    pub mnemonic:   Vec<String>,
    pub master_key: ExtendedKey,
}

impl HdWallet {
    /// Tạo HD Wallet mới với entropy ngẫu nhiên
    pub fn new(passphrase: &str) -> Self {
        let entropy  = generate_entropy();
        let mnemonic = entropy_to_mnemonic(&entropy);
        let seed     = mnemonic_to_seed(&mnemonic, passphrase);
        let master   = ExtendedKey::from_seed(&seed);
        HdWallet { mnemonic, master_key: master }
    }

    /// Khôi phục HD Wallet từ mnemonic đã có
    pub fn from_mnemonic(mnemonic: Vec<String>, passphrase: &str) -> Self {
        let seed   = mnemonic_to_seed(&mnemonic, passphrase);
        let master = ExtendedKey::from_seed(&seed);
        HdWallet { mnemonic, master_key: master }
    }

    /// BIP44: m/44'/0'/account'/0/index → external address
    /// 44'  = BIP44 purpose
    /// 0'   = Bitcoin mainnet (coin type)
    /// account' = account index (hardened)
    /// 0    = external chain (receiving addresses)
    /// index = address index
    pub fn get_address(&self, account: u32, index: u32) -> Wallet {
        let path = format!("m/44'/0'/{}'/0/{}", account, index);
        self.master_key.derive_path(&path).to_wallet()
    }

    /// BIP44: m/44'/0'/account'/1/index → internal address (change)
    pub fn get_change_address(&self, account: u32, index: u32) -> Wallet {
        let path = format!("m/44'/0'/{}'/1/{}", account, index);
        self.master_key.derive_path(&path).to_wallet()
    }

    pub fn mnemonic_string(&self) -> String {
        self.mnemonic.join(" ")
    }
}
