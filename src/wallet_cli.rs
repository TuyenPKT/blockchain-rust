#![allow(dead_code)]

/// v4.0  — PKT Wallet CLI: keypair, lưu ~/.pkt/wallet.key
/// v12.0 — HD Wallet: BIP39 mnemonic (12 từ) + BIP44 derive, wallet restore
///
/// File format (v12.0): 3 dòng
///   mnemonic (12 từ, cách nhau dấu cách)
///   secret_key_hex (64 chars)
///   address
///
/// Backward compat: file 2 dòng (v4.0) vẫn load được, mnemonic = None.
///
/// Usage:
///   cargo run -- wallet new                → tạo ví + hiển thị 12 từ seed phrase
///   cargo run -- wallet show               → hiển thị địa chỉ + seed phrase
///   cargo run -- wallet restore <12 từ>   → khôi phục ví từ seed phrase
///   cargo run -- wallet address            → chỉ in pubkey_hash (dùng cho scripts)

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
    // Unix: $HOME  |  Windows: %USERPROFILE%  |  fallback: current dir
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Lưu wallet v12.0: 3 dòng — mnemonic / sk_hex / address
fn save_wallet_hd(mnemonic: &str, sk_hex: &str, address: &str) -> Result<(), String> {
    let path = wallet_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = format!("{}\n{}\n{}\n", mnemonic, sk_hex, address);
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

/// Backward compat: lưu 2 dòng (không có mnemonic)
fn save_wallet(sk_hex: &str, address: &str) -> Result<(), String> {
    save_wallet_hd("", sk_hex, address)
}

/// Load full wallet — trả về (mnemonic: Option, sk_hex, address)
/// Tự phát hiện format:
///   - Dòng 1 chứa khoảng trắng (nhiều từ)  → mnemonic, dòng 2=sk, dòng 3=addr  (v12.0)
///   - Dòng 1 không có khoảng trắng (hex)    → sk, dòng 2=addr                   (v4.0)
fn load_wallet_full() -> Result<(Option<String>, String, String), String> {
    let path = wallet_path();
    let content = fs::read_to_string(&path)
        .map_err(|_| format!("Không tìm thấy wallet tại {:?}\nChạy: cargo run -- wallet new", path))?;
    let lines: Vec<&str> = content.lines().collect();

    if lines.len() >= 3 && lines[0].contains(' ') {
        // v12.0 format: mnemonic / sk_hex / address
        let mnemonic = lines[0].to_string();
        let sk_hex   = lines[1].to_string();
        let address  = lines[2].to_string();
        let mnemo    = if mnemonic.is_empty() { None } else { Some(mnemonic) };
        Ok((mnemo, sk_hex, address))
    } else if lines.len() >= 2 {
        // v4.0 format: sk_hex / address  (hoặc v12.0 mnemonic rỗng)
        let sk_hex  = lines[0].to_string();
        let address = lines[1].to_string();
        Ok((None, sk_hex, address))
    } else {
        Err("File wallet bị hỏng".to_string())
    }
}

fn load_wallet() -> Result<(String, String), String> {
    let (_, sk_hex, address) = load_wallet_full()?;
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

/// cargo run -- wallet new  (v12.0: dùng HdWallet BIP39/44)
pub fn cmd_wallet_new() {
    use crate::hd_wallet::HdWallet;

    // Kiểm tra file đã tồn tại — tránh ghi đè ví cũ
    let path = wallet_path();
    if path.exists() {
        eprintln!();
        eprintln!("❌ Đã có wallet tại {:?}", path);
        eprintln!();
        eprintln!("   Dùng lệnh sau để xem ví hiện tại:");
        eprintln!("     cargo run -- wallet show");
        eprintln!();
        eprintln!("   Nếu chắc chắn muốn tạo ví MỚI (mất ví cũ nếu không có seed phrase):");
        eprintln!("     rm {:?}", path);
        eprintln!("     cargo run -- wallet new");
        eprintln!();
        std::process::exit(1);
    }

    let hd       = HdWallet::new("");                          // entropy ngẫu nhiên, passphrase rỗng
    let wallet   = hd.get_address(0, 0);                      // m/44'/0'/0'/0/0
    let mnemonic = hd.mnemonic_string();
    let sk_hex   = hex::encode(wallet.secret_key.secret_bytes());
    let address  = wallet.address.clone();
    let hash_hex = pubkey_to_hash_hex(&wallet.public_key);

    match save_wallet_hd(&mnemonic, &sk_hex, &address) {
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
            println!("  🔑  Seed phrase (12 từ) — ghi lại và giữ bí mật:");
            println!();
            // In từng từ có số thứ tự để dễ ghi
            let words: Vec<&str> = mnemonic.split_whitespace().collect();
            for (i, w) in words.iter().enumerate() {
                if i % 4 == 0 { print!("      "); }
                print!("{:2}. {:<10}", i + 1, w);
                if i % 4 == 3 || i == words.len() - 1 { println!(); }
            }
            println!();
            println!("  ⛏  Mine đến ví này:");
            println!("      cargo run -- mine {}", hash_hex);
            println!();
            println!("  ⚠️  Mất seed phrase = mất ví vĩnh viễn. Không lưu trên máy tính!");
            println!();
        }
        Err(e) => {
            eprintln!("❌ Không thể lưu wallet: {}", e);
        }
    }
}

/// cargo run -- wallet restore <word1> <word2> ... <word12>
pub fn cmd_wallet_restore(args: &[String]) {
    use crate::hd_wallet::HdWallet;

    // args[2..] là các từ seed phrase
    let words: Vec<String> = args[2..].iter().map(|s| s.to_lowercase()).collect();
    if words.is_empty() {
        eprintln!("❌ Cú pháp: cargo run -- wallet restore <word1> <word2> ... <word12>");
        eprintln!("   Ví dụ:  cargo run -- wallet restore abandon ability able about above absent absorb abstract absurd abuse access accident");
        std::process::exit(1);
    }
    if words.len() != 12 && words.len() != 24 {
        eprintln!("❌ Seed phrase phải có 12 hoặc 24 từ (bạn nhập {} từ)", words.len());
        std::process::exit(1);
    }

    let hd       = HdWallet::from_mnemonic(words.clone(), "");
    let wallet   = hd.get_address(0, 0);
    let mnemonic = words.join(" ");
    let sk_hex   = hex::encode(wallet.secret_key.secret_bytes());
    let address  = wallet.address.clone();
    let hash_hex = pubkey_to_hash_hex(&wallet.public_key);

    match save_wallet_hd(&mnemonic, &sk_hex, &address) {
        Ok(_) => {
            println!();
            println!("╔══════════════════════════════════════════════════════════════╗");
            println!("║                ✅  Wallet Restored                          ║");
            println!("╚══════════════════════════════════════════════════════════════╝");
            println!();
            println!("  Address      : {}", address);
            println!("  Pubkey hash  : {}", hash_hex);
            println!("  Saved to     : {:?}", wallet_path());
            println!();
            println!("  ⛏  Mine đến ví này:");
            println!("      cargo run -- mine {}", hash_hex);
            println!();
        }
        Err(e) => {
            eprintln!("❌ Không thể lưu wallet: {}", e);
        }
    }
}

/// cargo run -- wallet show  (v12.0: hiển thị seed phrase nếu có)
pub fn cmd_wallet_show() {
    match load_wallet_full() {
        Ok((mnemonic_opt, sk_hex, address)) => {
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

            if let Some(mnemonic) = mnemonic_opt {
                println!("  🔑  Seed phrase (12 từ):");
                let words: Vec<&str> = mnemonic.split_whitespace().collect();
                for (i, w) in words.iter().enumerate() {
                    if i % 4 == 0 { print!("      "); }
                    print!("{:2}. {:<10}", i + 1, w);
                    if i % 4 == 3 || i == words.len() - 1 { println!(); }
                }
                println!();
                println!("  ⚠️  Không chia sẻ seed phrase với bất kỳ ai!");
            } else {
                println!("  ℹ️  Ví cũ (v4.0) — không có seed phrase.");
                println!("      Tạo ví mới: cargo run -- wallet new");
            }

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

/// In private key hex ra stdout — dùng để import vào desktop app.
pub fn cmd_wallet_privkey() {
    match load_wallet() {
        Ok((sk_hex, _)) => println!("{}", sk_hex),
        Err(e)          => eprintln!("❌ {}", e),
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
        "privkey" => cmd_wallet_privkey(),
        "restore" => cmd_wallet_restore(args),
        other => {
            eprintln!("❌ Lệnh wallet không hợp lệ: '{}'", other);
            eprintln!();
            eprintln!("  Cách dùng:");
            eprintln!("    cargo run -- wallet new                      tạo ví mới + hiển thị seed phrase");
            eprintln!("    cargo run -- wallet show                     xem thông tin ví + seed phrase");
            eprintln!("    cargo run -- wallet privkey                  in private key hex (để import vào desktop)");
            eprintln!("    cargo run -- wallet restore <word1>...<w12>  khôi phục ví từ seed phrase");
            eprintln!("    cargo run -- wallet address                  chỉ in pubkey hash (cho scripts)");
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hd_wallet::HdWallet;

    // ── HdWallet integration ─────────────────────────────────────────────────

    #[test]
    fn test_hd_wallet_new_has_12_words() {
        let hd = HdWallet::new("");
        assert_eq!(hd.mnemonic.len(), 12);
    }

    #[test]
    fn test_hd_wallet_mnemonic_string_12_words() {
        let hd = HdWallet::new("");
        let s  = hd.mnemonic_string();
        assert_eq!(s.split_whitespace().count(), 12);
    }

    #[test]
    fn test_hd_wallet_get_address_returns_wallet() {
        let hd = HdWallet::new("");
        let w  = hd.get_address(0, 0);
        assert!(!w.address.is_empty());
    }

    #[test]
    fn test_hd_wallet_same_mnemonic_same_address() {
        let hd1 = HdWallet::new("");
        let mn  = hd1.mnemonic.clone();
        let hd2 = HdWallet::from_mnemonic(mn, "");
        assert_eq!(hd1.get_address(0, 0).address, hd2.get_address(0, 0).address);
    }

    #[test]
    fn test_hd_wallet_different_index_different_address() {
        let hd = HdWallet::new("");
        let a0 = hd.get_address(0, 0).address;
        let a1 = hd.get_address(0, 1).address;
        assert_ne!(a0, a1);
    }

    #[test]
    fn test_hd_wallet_different_account_different_address() {
        let hd = HdWallet::new("");
        let a0 = hd.get_address(0, 0).address;
        let a1 = hd.get_address(1, 0).address;
        assert_ne!(a0, a1);
    }

    // ── load_wallet_full format detection ────────────────────────────────────

    #[test]
    fn test_load_wallet_full_v12_format() {
        // Mô phỏng parse nội dung v12.0
        let content = "abandon ability able about above absent absorb abstract absurd abuse access accident\ndeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n1AbcAddress\n";
        let lines: Vec<&str> = content.lines().collect();
        assert!(lines[0].contains(' '));   // dòng 1 là mnemonic
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_load_wallet_full_v4_format() {
        // Mô phỏng parse nội dung v4.0 (không có mnemonic)
        let content = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n1AbcAddress\n";
        let lines: Vec<&str> = content.lines().collect();
        assert!(!lines[0].contains(' '));  // dòng 1 không có space = sk_hex
        assert_eq!(lines.len(), 2);
    }

    // ── pubkey helpers ────────────────────────────────────────────────────────

    #[test]
    fn test_pubkey_to_address_not_empty() {
        let secp     = Secp256k1::new();
        let (_, pk)  = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        let addr     = pubkey_to_address(&pk);
        assert!(!addr.is_empty());
    }

    #[test]
    fn test_pubkey_to_hash_hex_40_chars() {
        let secp    = Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        let hash    = pubkey_to_hash_hex(&pk);
        assert_eq!(hash.len(), 40);
    }

    #[test]
    fn test_pubkey_hash_is_hex() {
        let secp    = Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        let hash    = pubkey_to_hash_hex(&pk);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ── restore determinism ───────────────────────────────────────────────────

    #[test]
    fn test_restore_same_words_same_sk() {
        let hd1 = HdWallet::new("");
        let words = hd1.mnemonic.clone();
        let hd2 = HdWallet::from_mnemonic(words, "");
        let w1  = hd1.get_address(0, 0);
        let w2  = hd2.get_address(0, 0);
        assert_eq!(
            hex::encode(w1.secret_key.secret_bytes()),
            hex::encode(w2.secret_key.secret_bytes())
        );
    }

    #[test]
    fn test_two_new_wallets_different_mnemonic() {
        let hd1 = HdWallet::new("");
        let hd2 = HdWallet::new("");
        // Xác suất trùng cực thấp (2^128 entropy)
        assert_ne!(hd1.mnemonic_string(), hd2.mnemonic_string());
    }
}
