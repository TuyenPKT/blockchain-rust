#![allow(dead_code)]

/// v4.0 — PKT Wallet CLI
///
/// Usage:
///   cargo run -- wallet new           → tạo keypair mới, lưu ~/.pkt/wallet.key
///   cargo run -- wallet show          → hiển thị địa chỉ từ file đã lưu
///   cargo run -- wallet address       → chỉ in địa chỉ (dùng cho scripts)
///   cargo run -- wallet balance       → hiển thị số dư (TODO: cần chain sync)

use std::fs;
use std::path::PathBuf;

use secp256k1::{Secp256k1, SecretKey, PublicKey};
use ripemd::{Ripemd160, Digest as RipemdDigest};

// ─── Wallet file format ───────────────────────────────────────────────────────

/// Nơi lưu wallet key: ~/.pkt/wallet.key
fn wallet_path() -> PathBuf {
    let home = dirs_home();
    home.join(".pkt").join("wallet.key")
}

fn dirs_home() -> PathBuf {
    // $HOME hoặc fallback về thư mục hiện tại
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Format lưu file: 2 dòng
///   secret_key_hex (64 chars)
///   address
fn save_wallet(sk_hex: &str, address: &str) -> Result<(), String> {
    let path = wallet_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = format!("{}\n{}\n", sk_hex, address);
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

fn load_wallet() -> Result<(String, String), String> {
    let path = wallet_path();
    let content = fs::read_to_string(&path)
        .map_err(|_| format!("Không tìm thấy wallet tại {:?}\nChạy: cargo run -- wallet new", path))?;
    let mut lines = content.lines();
    let sk_hex  = lines.next().ok_or("File wallet bị hỏng")?.to_string();
    let address = lines.next().ok_or("File wallet bị hỏng")?.to_string();
    Ok((sk_hex, address))
}

// ─── Crypto helpers ───────────────────────────────────────────────────────────

fn pubkey_to_address(public_key: &PublicKey) -> String {
    let pub_bytes     = public_key.serialize(); // 33 bytes compressed
    let sha256_hash   = blake3::hash(&pub_bytes);
    let ripemd_hash   = Ripemd160::digest(sha256_hash.as_bytes());
    let mut payload   = vec![0x00u8];
    payload.extend_from_slice(&ripemd_hash);
    let checksum_full = blake3::hash(blake3::hash(&payload).as_bytes());
    payload.extend_from_slice(&checksum_full.as_bytes()[..4]);
    bs58::encode(payload).into_string()
}

/// Cũng trả về pubkey_hash hex (40 chars) để dùng với miner
fn pubkey_to_hash_hex(public_key: &PublicKey) -> String {
    let pub_bytes   = public_key.serialize();
    let sha256_hash = blake3::hash(&pub_bytes);
    let ripemd_hash = Ripemd160::digest(sha256_hash.as_bytes());
    hex::encode(ripemd_hash)
}

// ─── CLI commands ─────────────────────────────────────────────────────────────

/// cargo run -- wallet new
pub fn cmd_wallet_new() {
    let secp = Secp256k1::new();
    let (sk, pk) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());

    let sk_hex   = hex::encode(sk.secret_bytes());
    let address  = pubkey_to_address(&pk);
    let hash_hex = pubkey_to_hash_hex(&pk);

    match save_wallet(&sk_hex, &address) {
        Ok(_) => {
            println!();
            println!("╔══════════════════════════════════════════════════════════════╗");
            println!("║                  ✅  PKT Wallet Created                     ║");
            println!("╚══════════════════════════════════════════════════════════════╝");
            println!();
            println!("  Address      : {}", address);
            println!("  Pubkey hash  : {} (dùng cho miner)", hash_hex);
            println!("  Saved to     : {:?}", wallet_path());
            println!();
            println!("  ⛏  Mine đến ví này:");
            println!("      cargo run -- mine {}", hash_hex);
            println!();
            println!("  ⚠️  Giữ bí mật file wallet.key — ai có file này có thể dùng ví của bạn!");
            println!();
        }
        Err(e) => {
            eprintln!("❌ Không thể lưu wallet: {}", e);
        }
    }
}

/// cargo run -- wallet show
pub fn cmd_wallet_show() {
    match load_wallet() {
        Ok((sk_hex, address)) => {
            // Khôi phục pubkey từ secret key để lấy hash hex
            let sk_bytes = match hex::decode(&sk_hex) {
                Ok(b) => b,
                Err(_) => { eprintln!("❌ File wallet bị hỏng (secret key không hợp lệ)"); return; }
            };
            let secp = Secp256k1::new();
            let sk   = match SecretKey::from_slice(&sk_bytes) {
                Ok(k) => k,
                Err(_) => { eprintln!("❌ File wallet bị hỏng"); return; }
            };
            let pk       = PublicKey::from_secret_key(&secp, &sk);
            let hash_hex = pubkey_to_hash_hex(&pk);

            let balance_paklets = crate::storage::load_mined_balance(&hash_hex);
            let balance_pkt     = balance_paklets as f64 / 1_000_000_000.0;

            println!();
            println!("╔══════════════════════════════════════════════════════════════╗");
            println!("║                    PKT Wallet Info                          ║");
            println!("╚══════════════════════════════════════════════════════════════╝");
            println!();
            println!("  Address      : {}", address);
            println!("  Pubkey hash  : {}", hash_hex);
            println!("  Balance      : {} paklets  ({:.9} PKT)", balance_paklets, balance_pkt);
            println!("  Wallet file  : {:?}", wallet_path());
            println!();
            println!("  ⛏  Mine đến ví này:");
            println!("      cargo run -- mine {}", hash_hex);
            println!();
        }
        Err(e) => {
            eprintln!("❌ {}", e);
        }
    }
}

/// cargo run -- wallet address  → chỉ in pubkey_hash hex (cho scripts)
pub fn cmd_wallet_address() {
    match load_wallet() {
        Ok((sk_hex, _address)) => {
            let sk_bytes = hex::decode(&sk_hex).unwrap_or_default();
            let secp = Secp256k1::new();
            if let Ok(sk) = SecretKey::from_slice(&sk_bytes) {
                let pk = PublicKey::from_secret_key(&secp, &sk);
                println!("{}", pubkey_to_hash_hex(&pk));
            } else {
                eprintln!("❌ File wallet bị hỏng");
            }
        }
        Err(e) => eprintln!("❌ {}", e),
    }
}

/// Trả về pubkey_hash hex từ wallet file — dùng cho miner auto-detect
pub fn load_miner_address() -> Option<String> {
    let (sk_hex, _) = load_wallet().ok()?;
    let sk_bytes    = hex::decode(&sk_hex).ok()?;
    let secp        = Secp256k1::new();
    let sk          = SecretKey::from_slice(&sk_bytes).ok()?;
    let pk          = PublicKey::from_secret_key(&secp, &sk);
    Some(pubkey_to_hash_hex(&pk))
}

// ─── Entry point ──────────────────────────────────────────────────────────────

pub fn run_wallet(args: &[String]) {
    let subcmd = args.get(2).map(|s| s.as_str()).unwrap_or("show");
    match subcmd {
        "new"     => cmd_wallet_new(),
        "show"    => cmd_wallet_show(),
        "address" => cmd_wallet_address(),
        other => {
            eprintln!("❌ Lệnh wallet không hợp lệ: '{}'", other);
            eprintln!();
            eprintln!("  Cách dùng:");
            eprintln!("    cargo run -- wallet new       → tạo wallet mới");
            eprintln!("    cargo run -- wallet show      → xem thông tin ví");
            eprintln!("    cargo run -- wallet address   → chỉ in pubkey hash (cho scripts)");
        }
    }
}
