# CLAUDE.md — Open Consensus Execution Interface Framework

**Version hiện tại: v21.0 ✅**

## Quy tắc cốt lõi
- Bắt buộc dùng Tiếng Việt
- Không sửa code cũ, chỉ extend. Mỗi version = 1 file mới + update `main.rs`
- Build sạch: `cargo build/test` không warnings, là sửa lỗi, không được phép xoá warnings
- Cập nhật `CONTEXT.md` + `CHANGELOG.md` sau mỗi version
- Không `unwrap()` / `panic` trong production
- Không hardcode secret, không raw SQL, validate tại API boundary
* không fake/demo

## Làm việc với AI

AI bị đóng băng tại training cutoff — API signature thay đổi sau đó có thể sai.

**Khi thêm dependency mới:**
1. Thêm vào `Cargo.toml` với version cố định
2. Đọc CHANGELOG của dep
3. Ghi gotcha vào mục **Lưu ý kỹ thuật** bên dưới
4. Hỏi AI viết code → `cargo build` → paste lỗi cho AI nếu có (tối đa 2–3 lần)

**Nguyên tắc:** AI giỏi structure — compiler giỏi correctness — docs giỏi truth. `CLAUDE.md` là "correction file": ghi một lần, AI đọc mãi.

## Testing

- Không dùng `mock_data()` để test business logic — dùng chain/DB thật
- `mock_data()` chỉ hợp lệ cho thuật toán render thuần túy (format string, sparkline)
- Test xanh với data ảo = test vô nghĩa

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
rocksdb = "0.21"
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
├── main.rs                 ← mod declarations + CLI dispatch + integration tests
├── block.rs / chain.rs / transaction.rs / utxo.rs / wallet.rs / mempool.rs
├── message.rs / node.rs / hd_wallet.rs / script.rs
├── lightning.rs / taproot.rs / covenant.rs / confidential.rs / coinjoin.rs
├── atomic_swap.rs / zk_proof.rs / pow_ghost.rs / bft.rs / sharding.rs
├── zk_rollup.rs / optimistic_rollup.rs / recursive_zk.rs / zkevm.rs
├── smart_contract.rs / oracle.rs / governance.rs / ai_agent.rs
├── dilithium.rs / sphincs.rs / kyber.rs / hybrid_sig.rs
├── self_amend.rs / ibc.rs / did.rs / fhe_contract.rs / sovereign_rollup.rs
├── sdk_gen.rs / full_stack.rs / miner.rs / wallet_cli.rs
├── pktscan_api.rs          ← REST API: /chain /balance /tx /status
├── pkt_bandwidth.rs / pkt_address.rs / pkt_genesis.rs
├── tui_dashboard.rs / tui_wallet.rs / web_frontend.rs
├── qr_code.rs / shell_completions.rs / web_charts.rs
├── block_detail.rs / address_detail.rs / ws_live.rs
├── web_serve.rs            ← ServeDir + page routes (/address/:a /block/:h /rx/:id)
├── pkt_sync.rs / pkt_utxo_sync.rs / pkt_wire.rs / pkt_address_index.rs
├── pkt_mempool_sync.rs / pkt_testnet_web.rs
└── pkt_analytics.rs        ← v18.0: hashrate/block_time/difficulty time-series

web/
├── css/style.css           ← theme, panels — ServeDir (no rebuild)
├── js/app.js               ← Home SPA
├── js/testnet.js           ← Testnet page
├── js/charts-live.js       ← v18.0: Chart.js analytics
├── js/address.js / address-page.js / block-list.js / block-detail.js
├── js/rx-list.js / rx-detail.js / shared.js
├── address/index.html / block/index.html / block/detail.html
└── rx/index.html / rx/detail.html

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

### Era 25 — Analytics & Polish (v18.x) ← ĐANG LÀM

~~v18.0 — Analytics Charts Web~~ ✅
~~v18.1 — v18.6, v18.8~~ ✅
v18.1 — Address Labels: LabelDb, preset miners/exchanges
v18.2 — Search Pro: detect type, fuzzy label
v18.3 — TX Detail Page: inputs/outputs, fee rate, confirmations
v18.4 — Block Detail Enhanced: TX list, fee total, miner breakdown
v18.5 — Pagination Cursor: cursor-based thay offset
v18.6 — Mobile API: /api/testnet/summary
v18.8 — Health & Uptime: /api/health/detailed
v18.9 — Data Export: CSV streaming
~~v18.7 — Mainnet Switch~~ — hoãn vô thời hạn
~~v18.9 — Data Export~~ ✅

### Era 26 — PKTCore Production + Dev Layer (v19.x)

v19.0 Cargo Workspace · v19.1 Flat File Storage · v19.2 JSON-RPC · v19.3 GetAddr/Addr
v19.4 libp2p · v19.5 JS/TS SDK · v19.6 PKT CLI · v19.7 API Playground
v19.8 Webhook UI · v19.9 Developer Portal

### Era 27 — PKTScan Desktop App (v20.x) ← HOÀN THÀNH ✅

~~v20.0–v20.9~~ ✅ · ~~v20.9 Build & Release~~ ✅

### Era 28 — PKTScan Desktop Nâng Cao (v21.x)

~~v21.0 Real Miner IPC~~ ✅ · v21.1 Wallet Integration · v21.2 Node Manager
v21.3 Offline Mode · v21.4 Notifications · v21.5 Auto-update
