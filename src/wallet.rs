#![allow(dead_code)]
use secp256k1::{Secp256k1, SecretKey, PublicKey, rand};
use sha2::{Sha256, Digest};
use ripemd::Ripemd160;

/// Wallet = cặp khóa ECDSA + địa chỉ Bitcoin
#[allow(dead_code)]
pub struct Wallet {
    pub secret_key:  SecretKey,
    pub public_key:  PublicKey,
    pub address:     String,   // Base58Check address (như 1BvBMSE...)
}

impl Wallet {
    /// Tạo wallet mới với keypair ngẫu nhiên
    pub fn new() -> Self {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let address = Self::pubkey_to_address(&public_key);
        Wallet { secret_key, public_key, address }
    }

    /// Bitcoin address = Base58Check( 0x00 + RIPEMD160( SHA256( pubkey ) ) )
    /// Đây là chuỗi tạo địa chỉ thực sự của Bitcoin
    pub fn pubkey_to_address(public_key: &PublicKey) -> String {
        // Bước 1: SHA-256 của public key
        let pub_bytes = public_key.serialize(); // 33 bytes compressed
        let sha256_hash = Sha256::digest(&pub_bytes);

        // Bước 2: RIPEMD-160 của kết quả trên → 20 bytes
        let ripemd_hash = Ripemd160::digest(&sha256_hash);

        // Bước 3: thêm version byte 0x00 (mainnet)
        let mut payload = vec![0x00u8];
        payload.extend_from_slice(&ripemd_hash);

        // Bước 4: checksum = SHA256(SHA256(payload))[0..4]
        let checksum_full = Sha256::digest(&Sha256::digest(&payload));
        payload.extend_from_slice(&checksum_full[..4]);

        // Bước 5: Base58 encode
        bs58::encode(payload).into_string()
    }

    /// Ký dữ liệu bằng private key → trả về signature dạng hex
    pub fn sign(&self, data: &[u8]) -> String {
        let secp    = Secp256k1::new();
        let hash    = Sha256::digest(data);
        let msg     = secp256k1::Message::from_slice(&hash).unwrap();
        let sig     = secp.sign_ecdsa(&msg, &self.secret_key);
        hex::encode(sig.serialize_compact())
    }

    /// Xác minh chữ ký với public key
    #[allow(dead_code)]
    pub fn verify(public_key: &PublicKey, data: &[u8], sig_hex: &str) -> bool {
        let secp = Secp256k1::new();
        let hash = Sha256::digest(data);
        let msg  = match secp256k1::Message::from_slice(&hash) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let sig_bytes = match hex::decode(sig_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig = match secp256k1::ecdsa::Signature::from_compact(&sig_bytes) {
            Ok(s) => s,
            Err(_) => return false,
        };
        secp.verify_ecdsa(&msg, &sig, public_key).is_ok()
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key.serialize())
    }
}
