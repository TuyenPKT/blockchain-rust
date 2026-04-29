# CLAUDE.md — Open Consensus Execution Interface Framework

**Version hiện tại: v27.1 ✅**

## Quy tắc cốt lõi (Refined)

- Trả lời bằng Tiếng Việt  
- Bạn là senior engineer, chịu trách nhiệm tìm và fix bug, không đổ lỗi user  
- Không bịa dữ liệu. Thiếu data → trả error typed hoặc tạo interface + TODO rõ ràng  

---

## Code & Reliability

- Không dùng unwrap() / panic() / expect() trong production và tests  
- Tất cả API trả Result<T, AppError> (error phải typed, không string tự do)  
- Error phải có context đầy đủ (dùng thiserror / anyhow chuẩn hóa)  
- Không tạo nhiều nguồn dữ liệu cho cùng một entity (single source of truth)  

---

## Dependency & Versioning

- Khi thêm dependency:
  - Pin version trong Cargo.toml  
  - Đọc CHANGELOG  
  - Ghi gotcha vào Lưu ý kỹ thuật  
- Tránh auto-upgrade gây breaking change  

---

## Data & Migration

- Nếu thay đổi format / logic → phải migrate data cũ  
- Migration phải:
  - Idempotent  
  - Có version (schema_version)  
- Sau migrate → chỉ còn 1 format duy nhất (không đọc song song)  

---

## Observability (Bắt buộc)

- Structured logging (JSON)  
- Có trace_id xuyên suốt request  
- Ghi log đầy đủ context khi error  
- Chuẩn bị sẵn hook cho metrics (latency, error rate)  

---

## Concurrency & Network

- Tránh deadlock / race condition  
- Có timeout cho mọi network call  
- Có retry strategy rõ ràng (không retry vô hạn)  

---

## Security

- SSH: ssh tuyenpkt@180.93.1.235 chỉ dùng khi cần debug production  
- Chỉ dùng SSH key, không dùng password  
- Không hardcode credential trong code / log  
- Audit command trước khi chạy  

---

## UI Consistency

- Sửa ở Tauri → phải sync Web nếu cùng feature  
- Không để lệch logic giữa các platform  

---

## Documentation

- Sau mỗi version:
  - Update CONTEXT.md  
  - Update CHANGELOG.md  

---

## Nguyên tắc xử lý bug

- Không đổ lỗi user  
- Không đoán mò  
- Ưu tiên:
  1. Reproduce  
  2. Xác định root cause  
  3. Fix triệt để (không workaround bẩn)  
  4. Thêm log + test để chặn tái diễn  

---
## Tóm tắt cốt lõi

- Single source of truth  
- No panic  
- Typed error  
- Versioned migration  
- Observability bắt buộc  
- Security-first

## DATA POLICY

Tuyệt đối **không tạo**: mock data, fake data, example values, placeholder, demo accounts, lorem ipsum, test emails, sample phone numbers, seed data giả, hard-coded literal trong tests.

Mọi dữ liệu phải đến từ: **database thật, API thật, config thật, input runtime**.

Test inputs phải:
- deterministic
- không mang semantic identity (không phải email/tên/số điện thoại nhìn như thật)
- load từ external source qua interface
- không hard-code

Nếu thiếu dữ liệu thật:
→ tạo interface/type/schema, để TODO hoặc trả error
→ **KHÔNG** tự bịa giá trị

```rust
fn init_repo(cfg: &Config) -> Result<UserRepo> {
    connect(cfg.database_url)   // ✅ từ config thật
}
```

## CHANGELOG format

```markdown
## v{X.Y} — {Tên} ({YYYY-MM-DD})
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
secp256k1 = { version = "0.27", features = ["rand-std", "global-context", "recovery"] }
ripemd = "0.1"
bs58 = "0.5"
tokio = { version = "1", features = ["full"] }
hmac = "0.12"
pbkdf2 = { version = "0.12", features = ["hmac"] }
redb = "2"                             # pure-Rust KV (v25.5, không còn RocksDB)
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
subtle = "2"                           # constant-time compare (v25.7)
```

## Cấu trúc file

```
src/
├── main.rs                 ← CLI dispatch + integration tests
├── lib.rs                  ← pub mod exports (dùng bởi desktop Tauri)
│
│  ── Base types ──
├── script.rs               ← Bitcoin Script VM: P2PKH/P2SH/P2TR/multisig/CLTV/CSV/HTLC/OP_IF
├── taproot.rs              ← Schnorr (BIP340) + MAST (BIP341) + MuSig2
├── lightning.rs            ← Payment channels: HTLC, commitment TX, revocation, penalty
├── wallet.rs / transaction.rs / reward.rs / api_auth.rs
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
│  ── EVM Layer (v26.x–v27.x) ──
├── gas_model.rs            ← EIP-1559: next_base_fee, burn, intrinsic_gas, GasHeader
├── evm_state.rs            ← WorldState: codes/balances/storage/nonces; create_address, create2_address
├── pkt_evm.rs              ← Full EVM: U256, 140+ opcodes, gas metering, CALL/CREATE sub-execution
├── evm_precompiles.rs      ← Precompiles 0x01–0x09: ecRecover, SHA256, RIPEMD160, Identity, ModExp
├── eth_rpc.rs              ← eth_* JSON-RPC 2.0 (POST /eth)
├── eth_wire.rs             ← ETH/68 P2P wire: 13 msg types, FrameCodec
├── rlp.rs                  ← RLP encoder/decoder (Bytes/List)
├── uncle.rs                ← Uncle/Ommer rewards, validation, UnclePool
├── abi.rs                  ← Solidity ABI encode/decode, function_selector, ERC-20 selectors
├── receipts.rs             ← Receipt storage + bloom filter (redb)
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
├── js/shared.js            ← API_BASE, fetchJson, ago, escHtml, theme
├── js/app.js               ← Home SPA
├── js/charts-live.js       ← Chart.js analytics (hashrate/difficulty/block_time)
├── js/address-page.js / block-list.js / block-detail.js
├── js/rx-list.js / rx-detail.js
├── js/health.js / playground.js / webhooks.js
└── address/ block/ rx/     ← detail pages (HTML stubs + JS hydrate)

desktop/
├── src/pages/              ← Explorer.tsx / Node.tsx / Miner.tsx / Wallet.tsx
├── src/api.ts / i18n.ts / theme.ts / App.tsx
└── src-tauri/src/lib.rs    ← Tauri IPC commands (start_sync, peer_scan, broadcast_tx, …)

index.html                  ← embedded via include_bytes! — rebuild khi sửa
```

## Lưu ý kỹ thuật

### Crypto / Wire
- `secp256k1 = 0.27`: dùng `Message::from_slice()` (không có `from_digest_slice()`); `PublicKey::combine()` thay `add_exp_assign`; không có `mul_assign` trên `PublicKey`. Feature `recovery` bắt buộc cho `RecoverableSignature` + `recover_ecdsa()`
- `pbkdf2`: bắt buộc `features = ["hmac"]`
- Schnorr: sign bằng `tweaked_sk`, không phải `internal_sk`
- `WireBlockHeader.nonce` là `u64` (v26.1); header = 84 bytes (`WIRE_HEADER_LEN`); load_header backward-compat với 80-byte entries (zero-pad)
- UTXO lookup: `owner_bytes_of()` hỗ trợ 20-byte (P2PKH) và 32-byte (P2TR)
- Tránh `try_into()` trên `&[u8;64]` — dùng `copy_from_slice`

### Storage / Test
- redb 2: `ReadableTable::len()` không có → đếm bằng iterator
- `SyncDb::open_temp()` race condition khi tests song song → dùng `static Mutex<()>`
- `txin_temp` lifecycle: write trong `index_tx_inputs` → **bắt buộc** read+delete trong `index_tx_outputs`. Thiếu delete → storage leak tích luỹ
- `testnet_web_router()` mở DBs tại `~/.pkt/syncdb` + `~/.pkt/utxodb` — cần `cargo run -- sync` trước
- ratatui: dùng `TestBackend` cho unit tests
- `#![allow(dead_code)]` ở đầu file khi có nhiều public API chưa dùng

### EVM
- ABI: `encode_bytes_payload()` helper cần thiết vì Rust không cho phép `Bytes(b) | String(b)` với `b` khác kiểu trong cùng arm
- `Evm::new_with_world()` arg order: `(ctx, code, storage, world)` — code sau ctx
- EVM CREATE stack: pops `val` (top), `off`, `len` → push `len` trước, `off`, `val` cuối (val on top)
- `Rc<RefCell<WorldState>>` dùng cho sub-EVM sharing; snapshot = `world.borrow().clone()` trước sub-call, restore = `*world.borrow_mut() = snapshot` khi REVERT

### Consensus / Node
- `LAST_BLOCK_WALL_SECS: AtomicU64` static — dùng `.swap()` để lấy last value atomic; init=0 → first block dùng `TARGET_SECS=60`
- Difficulty dead zone `DEAD_ZONE=12` (±20%): 48–72s → no change; <48 → +1; >72 → -1. Ngăn single-block oscillation
- Difficulty dùng wall clock (`SystemTime::now()`), **không** dùng `header.timestamp` (forgeable)

### Web / Front-end
- `tower-http = 0.5`: `ServeDir::new("web")` serve từ CWD/web/ — binary phải chạy từ project root
- QR width = `17 + 4×N` — test: `(w - 17) % 4 == 0`
- innerHTML template literals: **bắt buộc** `escHtml()` cho mọi giá trị từ URL/API/user (XSS)
- `ago(secs)`: <10s = "just now"; 30 ngày: trả date string thay vì "X days ago" để tránh hiển thị timestamp lỗi (vd "8352 days ago")

### Security (v25.7)
- `subtle::ConstantTimeEq`: cả hai chuỗi phải cùng length, nếu không sẽ luôn `false` → đảm bảo blake3 hash 64 chars khớp đúng
- `url_guard::validate_callback_url`: không resolve DNS — chỉ chặn IP literal + hostname biết trước. DNS rebinding vẫn có thể xảy ra
- Default bind = `127.0.0.1`; set `PKT_LISTEN=0.0.0.0` để expose public
- `PKT_TRUSTED_PROXY=1` khi sau nginx; default off → tránh X-Forwarded-For spoofing
- GraphQL `limit_complexity(100)` + `limit_depth(5)` — tăng nếu query hợp lệ bị từ chối

## Roadmap

| Era | Versions | Trạng thái |
|-----|----------|-----------|
| 1–28 | v0.1–v21.x | ✅ Foundation → Desktop App |
| 29 | v22.x | ✅ PKTScan Backend Fix |
| 30 | v23.x | ✅ PKT Full Node + Security Patch |
| 31 | v24.x | ✅ Public Testnet & Ecosystem |
| 32 | v25.x | ✅ Storage Migration redb + Security Hardening (9 patches) |
| 33 | v26.x | ✅ EVM Compatible Layer (gas_model, pkt_evm, eth_rpc, eth_wire, rlp, uncle, precompiles, abi, receipts) |
| 34 | v27.x | ← ĐANG LÀM (Bitcoin Script Parity + sub-EVM) |
| 35 | v28.x | Mainnet Prep (checkpoints, pentest, GraphQL authn) |

### Era 34 — đang làm (v27.x)
- ~~v27.0~~ Bitcoin Script Complete (CLTV/CSV/OP_IF/HTLC/Taproot/Schnorr/Lightning; 892 tests)
- ~~v27.1~~ CALL/CREATE sub-EVM (WorldState + snapshot/restore + depth guard 1024; 909 tests)
- v27.2 — eth_sendRawTransaction RLP decode + EIP-155 sig verify
- v27.3 — ETH/68 P2P handshake (Status message với geth peer)
- v27.4 — Lightning routing (multi-hop HTLC, onion routing stub)
- v27.5 — Taproot key aggregation on-chain validation

> Chi tiết per-version + file changes → `CONTEXT.md`. Format CHANGELOG entries → `CHANGELOG.md`.
