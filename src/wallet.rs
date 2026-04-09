#![allow(dead_code)]
use secp256k1::{Secp256k1, SecretKey, PublicKey, rand};

/// Wallet = cặp khóa ECDSA + địa chỉ EVM
#[allow(dead_code)]
pub struct Wallet {
    pub secret_key:  SecretKey,
    pub public_key:  PublicKey,
    pub address:     String,   // EVM address "0x..." (EIP-55 checksummed)
}

impl Wallet {
    /// Tạo wallet mới với keypair ngẫu nhiên
    pub fn new() -> Self {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let address = Self::pubkey_to_address(&public_key);
        Wallet { secret_key, public_key, address }
    }

    /// EVM address = "0x" + EIP-55( last20( Keccak256( uncompressed_pubkey_64 ) ) )
    /// Tương thích hoàn toàn với Ethereum / BNB Chain.
    pub fn pubkey_to_address(public_key: &PublicKey) -> String {
        let compressed = public_key.serialize(); // 33 bytes
        crate::evm_address::pubkey_to_evm_address(&compressed)
            .unwrap_or_else(|_| "0x0000000000000000000000000000000000000000".to_string())
    }

    /// Ký dữ liệu bằng private key → trả về signature dạng hex
    pub fn sign(&self, data: &[u8]) -> String {
        let secp    = Secp256k1::new();
        let hash    = blake3::hash(data);
        let msg     = secp256k1::Message::from_slice(hash.as_bytes()).unwrap();
        let sig     = secp.sign_ecdsa(&msg, &self.secret_key);
        hex::encode(sig.serialize_compact())
    }

    /// Xác minh chữ ký với public key
    #[allow(dead_code)]
    pub fn verify(public_key: &PublicKey, data: &[u8], sig_hex: &str) -> bool {
        let secp = Secp256k1::new();
        let hash = blake3::hash(data);
        let msg  = match secp256k1::Message::from_slice(hash.as_bytes()) {
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
