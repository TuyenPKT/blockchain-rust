# CLAUDE.md — Open Consensus Execution Interface Framework

**Version hiện tại: v25.4 ✅**

## Quy tắc cốt lõi
- Bắt buộc dùng Tiếng Việt
- Cập nhật `CONTEXT.md` + `CHANGELOG.md` sau mỗi version
- Không `unwrap()` / `panic` trong production
Tests vẫn tồn tại
nhưng:

- không được tạo fake values
- không hard-code literal data
- chỉ test qua interface
- dữ liệu test phải load từ external source

Test inputs phải:
- deterministic
- không mang semantic identity
- không hard-code example user data

# DATA POLICY

Tuyệt đối không tạo:
- mock data
- fake data
- example values
- placeholder values
- demo accounts
- lorem ipsum
- test emails
- sample phone numbers

Không viết:
- example usage với giá trị cụ thể
- unit test chứa hard-coded values
- seed data giả

Nếu thiếu dữ liệu:
→ tạo interface / type / schema
→ để TODO hoặc error
→ KHÔNG tự bịa giá trị

Mọi dữ liệu phải đến từ:
- database thật
- API thật
- config thật
- input runtime

Không được viết:
"test data"

**Khi thêm dependency mới:**
1. Thêm vào `Cargo.toml` với version cố định
2. Đọc CHANGELOG của dep
3. Ghi gotcha vào mục **Lưu ý kỹ thuật** bên dưới
4. Hỏi AI viết code → `cargo build` → paste lỗi cho AI nếu có 

**Nguyên tắc:** AI giỏi structure — compiler giỏi correctness — docs giỏi truth. `CLAUDE.md` là "correction file": ghi một lần, AI đọc mãi.

## Testing

- Không dùng `mock_data()` để test business logic — dùng chain/DB thật
- Test xanh với data ảo = test vô nghĩa

If real data source is undefined:
    return error
Do NOT fabricate values

struct Config {
    database_url: String
}

fn init_repo(cfg: &Config) -> Result<UserRepo> {
    connect(cfg.database_url)
}


## CHANGELOG format

```markdown
## v{X.Y} — {Tên} ({ngày})
### Added
- Tính năng chính
### Files
- `src/{file}.rs` — mô tả
### Tests
- +N tests ({tổng} total)
### Breaking / Gotcha
- Ghi nếu có
```

## Stack

```
cargo run                              # help + version timeline
cargo run -- wallet new/show          # PKT wallet CLI
cargo run -- mine [addr] [n]          # PoW miner
cargo run -- node <port> [peer]       # P2P node
cargo run -- qr <address>             # QR code terminal
cargo run -- completions <bash|zsh>   # shell completions
cargo test                            # all tests
cargo build                           # compile check
```

## Dependencies (Cargo.toml)

```toml
sha2 = "0.10"
hex = "0.4"
chrono = "0.4"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
secp256k1 = { version = "0.27", features = ["rand-std", "global-context"] }
ripemd = "0.1"
bs58 = "0.5"
tokio = { version = "1", features = ["full"] }
hmac = "0.12"
pbkdf2 = { version = "0.12", features = ["hmac"] }
rocksdb = "0.21"                       # fallback (--no-default-features)
redb = { version = "2", optional = true }  # default backend (v25.2)
axum = { version = "0.7", features = ["ws"] }
async-graphql = { version = "7", features = ["tracing"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
blake3 = "1.5"
rayon = "1.10"
num_cpus = "1.16"
ed25519-dalek = { version = "2", features = ["rand_core"] }
zeroize = { version = "1", features = ["derive"] }
rand_core = { version = "0.6", features = ["std"] }
ratatui = "0.26"
crossterm = "0.27"
qrcode = "0.14"
proptest = { version = "1.4", optional = true }
tower-http = { version = "0.5", features = ["fs"] }
```

## Cấu trúc file

```
src/
├── main.rs                 ← CLI dispatch + integration tests
├── lib.rs                  ← pub mod exports (dùng bởi desktop Tauri)
│
│  ── Base types ──
├── script.rs / transaction.rs / reward.rs / api_auth.rs
├── pkt_address.rs / pkt_health.rs / pkt_export.rs
│
│  ── Network config (single source of truth) ──
├── pkt_genesis.rs          ← tokenomics: 20 PKT/block, 525k halving, 21M supply
├── pkt_config.rs           ← PktConfig OnceLock: seed, ports, data_dir (testnet/mainnet)
├── pkt_paths.rs            ← data dirs + db_opts() LZ4 compression
├── pkt_wire.rs / pkt_peer.rs / pkt_checkpoints.rs / evm_address.rs
│
│  ── Storage / sync ──
├── pkt_sync.rs / pkt_utxo_sync.rs / pkt_addr_index.rs
├── pkt_reorg.rs / pkt_mempool.rs / pkt_mempool_sync.rs
├── pkt_block_sync.rs / pkt_labels.rs / pkt_search.rs
├── pkt_analytics.rs / pkt_snapshot.rs
│
│  ── API / UI ──
├── pkt_explorer_api.rs     ← REST /api/testnet/*
├── pkt_testnet_web.rs      ← summary, block list, TX, address endpoints
├── pkt_sync_ui.rs
│
│  ── Services ──
├── pkt_pool.rs             ← mining pool proxy (8337 → 8334, stats 8338)
├── pkt_fullnode.rs         ← full node mode (spawn + watcher)
└── pkt_install.rs          ← install script generator (systemd/launchd/Windows Service)

web/
├── css/style.css           ← theme, panels — ServeDir (no rebuild)
├── js/shared.js            ← API_BASE, fetchJson, helpers
├── js/app.js               ← Home SPA
├── js/testnet.js           ← Testnet page
├── js/charts-live.js       ← Chart.js analytics (hashrate/difficulty/block_time)
├── js/address.js / address-page.js
├── js/block-list.js / block-detail.js
├── js/rx-list.js / rx-detail.js
├── js/health.js / playground.js / webhooks.js
├── address/index.html / block/index.html / block/detail.html
└── rx/index.html / rx/detail.html

desktop/
├── src/pages/              ← Explorer.tsx / Node.tsx / Miner.tsx / Wallet.tsx
├── src/api.ts / i18n.ts / theme.ts / App.tsx
└── src-tauri/src/lib.rs    ← Tauri IPC commands (start_sync, peer_scan, broadcast_tx, …)

index.html                  ← embedded via include_bytes! — rebuild khi sửa
```

## Lưu ý kỹ thuật

- `secp256k1 = 0.27`: dùng `Message::from_slice()`, không có `from_digest_slice()`; `PublicKey::combine()` thay vì `add_exp_assign`; không có `mul_assign` trên `PublicKey`
- `pbkdf2`: bắt buộc `features = ["hmac"]`
- Schnorr: sign bằng `tweaked_sk`, không phải `internal_sk`
- `TxOutput.script_pubkey` và `TxInput.script_sig` là type `Script`
- UTXO lookup: `owner_bytes_of()` hỗ trợ 20-byte (P2PKH) và 32-byte (P2TR)
- Tránh `try_into()` trên `&[u8;64]` — dùng `copy_from_slice`
- `#![allow(dead_code)]` ở đầu file khi có nhiều public API chưa dùng
- ratatui: dùng `TestBackend` cho unit tests
- `SyncDb::open_temp()` — race condition khi tests song song → dùng `static Mutex<()>`
- `testnet_web_router()` mở DBs tại `~/.pkt/syncdb` + `~/.pkt/utxodb` — cần `cargo run -- sync` trước
- `tower-http = 0.5`: `ServeDir::new("web")` serve từ CWD/web/ — binary phải chạy từ project root
- QR width = `17 + 4×N` — test: `(w - 17) % 4 == 0`

## Roadmap

### Era 25–28 (v18.x–v21.x) ✅ HOÀN THÀNH

~~Era 25 — Analytics & Polish (v18.x)~~ ✅
~~Era 26 — PKTCore Production + Dev Layer (v19.x)~~ ✅
~~Era 27 — PKTScan Desktop App (v20.x)~~ ✅
~~Era 28 — PKTScan Desktop Nâng Cao (v21.x)~~ ✅ (v21.0 Miner IPC · v21.1 i18n+Theme · v21.2 Wallet+Peer Scan)

### Era 29 — PKTScan Backend Fix (v22.x) ✅

~~v22.0~~ Address Index · ~~v22.1~~ UTXO Height · ~~v22.2~~ Block TX Count
~~v22.4~~ Broadcast TX · ~~v22.5~~ Wallet Send · ~~v22.6~~ Fix Stats Display

### Era 30 — PKT Full Node (v23.x) ✅

~~v23.0~~ TX Validation · ~~v23.1~~ P2PKH Script · ~~v23.2~~ Block Relay
~~v23.3~~ Multi-peer · ~~v23.4~~ Mempool Full · ~~v23.5~~ IBD Checkpoints
~~v23.6~~ Wire Mempool Bridge · ~~v23.7~~ UTXO Snapshot · ~~v23.8~~ Full Node Mode
~~v23.8.1~~ Security Patch (15 issues)

### Era 31 — Public Testnet & Ecosystem Bootstrap (v24.x) ✅

~~v24.0~~ Node Onboarding · ~~v24.1~~ EVM Address · ~~v24.2~~ Network-aware Paths
~~v24.3~~ Nav Toggle · ~~v24.4~~ Mining Pool · ~~v24.5~~ LZ4 Compression
~~v24.6~~ Tokenomics 21M PKT · ~~v24.6.1~~ Network Config (pkt_config.rs)
v24.7 — Testnet Faucet · v24.8 — Developer Docs · v24.9 — Multi-node Bootstrap
v24.10 — Mainnet Prep (checkpoints, genesis verify, tokenomics audit)

### Era 32 — Storage Migration redb (v25.x) ← ĐANG LÀM

~~v25.0~~ RocksKv Abstraction · ~~v25.1~~ RedbKv + feature flag · ~~v25.2~~ redb Default
~~v25.3~~ VPS Migration + Re-sync · ~~v25.4~~ In-Process Sync (redb hoạt động trên VPS)
