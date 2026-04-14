2# Open Consensus Execution Interface Framework — CONTEXT

**Version hiện tại: v24.4 ✅ — 0 errors, 0 warnings**

---

## ✅ Tiến độ

### Era 1–9 (v0.1–v3.9) ✅
| Era | Versions | Nội dung |
|-----|----------|---------|
| 1 | v0.1–v0.4 | Block, Chain, SHA-256, Genesis, PoW, UTXO |
| 2 | v0.5–v0.8 | ECDSA Wallet, P2P Node, Mempool, HD Wallet |
| 3 | v0.9–v1.3 | Script Engine, Multisig P2SH, SegWit, Lightning, Taproot |
| 4 | v1.4–v1.7 | Covenants/CTV, Confidential TX, CoinJoin, Atomic Swap |
| 5 | v1.8–v2.1 | ZK-SNARK, GHOST PoW, BFT Consensus, Sharding |
| 6 | v2.2–v2.5 | ZK-Rollup, Optimistic Rollup, Recursive ZK, zkEVM |
| 7 | v2.6–v2.9 | Smart Contract (WASM), Oracle, Governance, AI Agent |
| 8 | v3.0–v3.3 | Dilithium, SPHINCS+, KYBER, Hybrid Sig (Post-Quantum) |
| 9 | v3.4–v3.9 | Self-amend, IBC, DID, FHE, Sovereign Rollup, Full Stack |

### Era 10–26 (v4.x–v19.x) ✅
| Era | Versions | Nội dung |
|-----|----------|---------|
| 10 | v4.0–v4.8 | PKT Wallet CLI, PacketCrypt PoW, RocksDB, P2P Sync, REST API, Explorer, Metrics |
| 11 | v5.0–v5.9 | Performance O(1), Security hardening, Fee market, WAL, Fuzz, Monitoring, Peer discovery, Bench |
| 12 | v6.0–v6.9 | BLAKE3, CPU rayon miner, ConcurrentChain, Parallel validation, OpenCL, CUDA, Mining Pool, SIMD, HW auto-config |
| 13 | v7.0–v7.9 | Block Reward, Fee Calculator, Token ERC-20, EVM-lite, DeFi AMM, Staking, Economics |
| 14 | v8.0–v8.9 | PKTScan REST, WebSocket, Address page, Search, Mempool, Pool Dashboard, Analytics, CSV, Cache |
| 15 | v9.0–v9.9 | Zero-Trust middleware, Ed25519 HD Wallet, Token/Contract/Staking/DeFi API, OpenAPI, SDK Gen |
| 16 | v10.0–v10.9 | API Auth (API Key), Audit Log, EVM fix, Multi-chain IBC, GraphQL, Webhooks, Risk Score |
| 17 | v11.0–v11.9 | Write APIs, TX/Token/Contract write, Deploy Config, Webhook delivery, Analytics v2 |
| 18 | v12.0–v12.9 | BIP39 mnemonic restore, Ed25519 SLIP-0010, QR Code, Shell completions |
| 19 | v13.0–v13.9 | PacketCrypt chuẩn PKT, Network Steward, PKT Address (Base58Check), PKT Genesis, Web frontend |
| 21 | v14.0–v14.9 | TUI Dashboard, TUI Wallet, Web Frontend, QR Code, Block/Address detail, Live WS charts |
| 22 | v15.0–v15.8 | PKT Wire Protocol, Sync engine, UTXO sync, Address index, PKT Node, Template server |
| 23 | v16.0–v16.9 | Devnet, Docs gen, E2E tests, CLI token, CLI key, Hot reload |
| 24 | v17.0–v17.9 | Block explorer pro, TX detail, Address detail, Multi-sort, Rich list, Mempool pro |
| 25 | v18.0–v18.9 | Analytics charts (Chart.js), Hashrate/difficulty time-series, CSV export, Health API |
| 26 | v19.0–v19.9 | Cargo workspace, JSON-RPC, GetAddr, JS/TS SDK, PKT CLI, API Playground, Webhook UI, Dev Portal |

### Era 27 — PKTScan Desktop App (v20.x) ✅
- [x] v20.0 — **Tauri Setup**: Tauri v2 backend (4 IPC commands); React stub; workspace member; icons; 3 tests
- [x] v20.1 — **React UI**: Nav (4 tabs + Mainnet/Testnet toggle); Explorer; Blocks; Address; Terminal; SearchBar
- [x] v20.2 — **Live Dashboard**: `useLiveDashboard` poll 8s; `useAnimatedNumber`; slide-in animations
- [x] v20.3 — **Charts**: `MiniChart` canvas sparkline; hashrate/block_time/difficulty; window 50–500
- [x] v20.4 — **Search**: `useSearch` debounce 320ms; localStorage recents; Cmd+K modal
- [x] v20.5 — **Address Detail**: Balance Hero + QR modal; TX history paginated; UTXO list
- [x] v20.6 — **Block & TX Detail**: BlockDetail hero; TxDetail flow bar; FeeRateBadge; ConfBadge
- [x] v20.7 — **Rich List & Mempool**: Leaderboard; Fee Histogram canvas; Mempool TX table
- [x] v20.8 — **Settings**: `useSettings` localStorage; Network/Theme/Currency/Language/Poll; light mode
- [x] v20.9 — **Build & Release**: `release.yml` CI 3-platform (macOS .dmg, Windows .msi, Linux .AppImage)

### Era 28 — PKTScan Desktop Nâng Cao (v21.x) ✅
- [x] v21.0 — **Real Miner IPC**: `start_mine/stop_mine/mine_status` Tauri commands; TCP GetTemplate/NewBlock; blake3 PoW rayon; emit `mine_log`+`mine_stats`; Miner.tsx realtime
- [x] v21.1 — **i18n + Auto Theme**: `i18n.ts` EN/VI ~60 keys; `applyTheme()` auto via Tauri `onThemeChanged`; Node.tsx bỏ fake data; white-border fix; light mode canvas fix
- [x] v21.2 — **Wallet + Peer Scan**: `wallet_generate/wallet_from_privkey`; `peer_scan` TCP; Wallet.tsx (create/import/reveal/remove); Node.tsx Peers panel; bỏ tab Terminal

### Era 29 — PKTScan Backend Fix (v22.x) ✅
- [x] v22.0 — **Address Index Fix**: `any_addr_to_script_hex()` bech32/Base58Check/script_hex; `ps_addr_utxos`; `/richlist` alias; `balance_pkt`
- [x] v22.1 — **UTXO Height Field**: `pub height: u64` vào `UtxoEntry`; backward-compatible; tất cả callers updated
- [x] v22.2 — **Block TX Count**: `save/get_block_tx_count` vào `SyncDb`; `ps_block_detail` dùng stored count
- [x] v22.3 — **Rich List**: `/api/testnet/richlist` alias (done trong v22.0)
- [x] v22.4 — **Broadcast TX**: `POST /api/testnet/tx/broadcast` → parse WireTx → relay P2P → `{txid, status}`
- [x] v22.5 — **Wallet Send TX**: `wallet_tx_build` BIP143 segwit P2WPKH; `tx_broadcast`; Wallet.tsx Send panel
- [x] v22.6 — **Fix Stats Display**: `difficulty` trong `ps_summary`; Explorer.tsx + Node.tsx fix field names

### Era 30 — PKT Full Node (v23.x) ✅
- [x] v23.0 — **TX Validation**: `validate_block()` coinbase/UTXO/double-spend/value/merkle; `ValidateError`; +15 tests
- [x] v23.1 — **P2PKH Script Verification**: `verify_p2pkh_input` ECDSA; `verify_tx_scripts`; `ScriptError`; +14 tests
- [x] v23.2 — **Block + TX Relay**: `RelayHub` mpsc fanout; `SeenHashes` bounded 8192; `Inv→GetData`; +18 tests
- [x] v23.3 — **Multi-peer Manager**: `PeerSet`; `PeerSlot`; `strike()` auto-ban; backoff 5×2^n; +21 tests
- [x] v23.4 — **Mempool Full**: `PktMempool` BTreeMap priority; RBF; `evict_lowest/expired`; `select_transactions`; +18 tests
- [x] v23.5 — **IBD Checkpoints**: `Checkpoint` const fn; `TESTNET/MAINNET_CHECKPOINTS`; `ibd_action()`; +17 tests
- [x] v23.6 — **Wire Mempool Bridge**: `load_wire_mempool_txs`; WireTx→Transaction convert; template merge; +21 tests
- [x] v23.7 — **UTXO Snapshot**: NDJSON format; `dump/load/load_snapshot`; CLI `snapshot dump/load/info`; +21 tests
- [x] v23.8 — **Full Node Mode**: `pkt_fullnode.rs`; `spawn_sync_process()`; `start_watcher` auto-restart; `cmd_fullnode`; +17 tests
- [x] v23.8.1 — **Security Patch** (15 issues): Auth `sync/start`, API key in URL, `api_keys.json` 0644, UTXO decode, Merkle SHA256d, balance=0 wire/legacy, PKT divisor 1e9→2^30, OCEIF rebranding, genesis placeholder, Mutex unwrap ×15, peer param injection, wallet.key 0644, pool dummy_reward, fake label pGTESTNE

#### Checklist trước Mainnet
- [ ] Checkpoints thực tế tại height 50k, 100k — **chờ testnet đủ blocks** (hiện tại ~77)
- [x] Xóa `src/genesis.rs` cũ ✅
- [x] Địa chỉ coinbase testnet genesis thật vào `pkt_labels.rs` PRESETS ✅
- [x] Verify DNS `seed.oceif.com:64764` + `seed.testnet.oceif.com:8333` ✅
- [ ] Verify `HALVING_INTERVAL` + `INITIAL_BLOCK_REWARD` khớp tokenomics PKT
- [ ] Block reward thực từ coinbase TX (plan v24.0.9.7 — thay formula lý thuyết)
- [ ] Pentest: fuzz REST API, peer spam / eclipse attack trên testnet
- [ ] LZ4 compact: chạy đủ lâu để RocksDB tự compact data cũ → kiểm tra disk giảm

### Era 31 — Public Testnet & Ecosystem Bootstrap (v24.x) ← ĐANG LÀM
- [x] v24.0 — **Node Onboarding**: `src/pkt_install.rs` — `generate_install_sh()` Linux/macOS (systemd+launchd) + `generate_install_ps1()` Windows (Windows Service); `generate_config_toml()`; CLI `install-node [--mainnet] [--print-sh|--print-ps1|--print-config]`; +25 tests
- [x] v24.1 — **EVM Address Format**: `src/evm_address.rs` Keccak-256 + EIP-55; `wallet.rs` + `wallet_cli.rs` dùng EVM; pkt_labels cập nhật; +14 tests
- [x] v24.2 — **Network-aware Data Paths**: `src/pkt_paths.rs` single source of truth; `testnet/` vs `mainnet/` data dirs; +6 tests
- [x] v24.3 — **Nav Toggle Redesign**: desktop pill shape; testnet first; dot indicator; amber/blue active colors
- [x] v24.0.9.5–v24.0.9.11 — **Frontend Bug Fixes**: Block Reward động từ API; TX input/output value field; Avg Block Time fallback; Address Type EVM; address-page.js API URL fix; nginx prefix strip; RocksDB LOCK cleanup
- [x] v24.4 — **Public Mining Pool**: `src/pkt_pool.rs`; proxy pool (miner_port=8337, stats_port=8338); `PoolShared` RwLock; per-miner stats; stats HTTP API `/api/pool/stats` + `/api/pool/workers`; CLI `cargo run -- pool`; +9 tests
- [x] v24.5 — **LZ4 Compression**: `db_opts()` trong `pkt_paths.rs`; bật LZ4 cho 7 RocksDB opens; tiết kiệm ~40-60% disk
- [ ] v24.6 — **Testnet Faucet**: Web UI → gửi test PKT; rate-limit 1/IP/24h
- [ ] v24.7 — **Developer Docs**: OpenAPI spec đầy đủ; quick-start guide
- [ ] v24.8 — **Multi-node Bootstrap**: 3+ bootstrap nodes độc lập; peer health monitoring
- [ ] v24.9 — **Mainnet Prep**: checkpoints thực tế, genesis verify, tokenomics audit
- [ ] v24.6 — **Developer Docs**: OpenAPI spec đầy đủ; quick-start guide
- [ ] v24.7 — **Multi-node Bootstrap**: 3+ bootstrap nodes độc lập; peer health monitoring
- [ ] v24.8 — **Mainnet Prep**: checkpoints thực tế, genesis verify, tokenomics audit

### Era 20 — Post-Singularity (v99.x) — hardware-dependent
- [ ] v99.0–v99.5 — Quantum Random Beacon, Neural Wallet, Interplanetary Sync, AI Consensus, Singularity Chain

---

## 🧱 Quyết định thiết kế quan trọng

Chỉ ghi các quyết định **không hiển nhiên** hoặc có thể gây lỗi khi sửa code.

| Version | Quyết định |
|---------|-----------|
| v0.5 | `secp256k1 = 0.27`: `Message::from_slice()`, không có `from_digest_slice()`; `PublicKey::combine()` thay `add_exp_assign`; không có `mul_assign` |
| v0.8 | `pbkdf2 = { features = ["hmac"] }` — bắt buộc, thiếu là lỗi compile |
| v1.3 | Key path spend phải sign bằng `tweaked_sk`, KHÔNG phải `internal_sk` |
| v1.3 | `schnorr_sign()`: dùng `copy_from_slice` thay `try_into()` trên `&[u8;64]` |
| v5.0 | `fast_merkle` dùng raw byte concat (Bitcoin std) — KHÔNG tương thích `Block::merkle` (hex-string concat) |
| v5.1 | `RateLimiter::check()` increment trước rồi compare — count=1 sau lần gọi đầu |
| v5.2 | `MessageDedup` dùng `VecDeque` + `HashSet` — FIFO eviction; O(1) insert/lookup |
| v5.3 | `CoinbaseGuard`: unknown tx_id → always mature; known → `current >= mined + 100` |
| v6.1 | `mine_parallel` dùng `find_map_any` — rayon dừng tất cả threads khi 1 trả `Some` |
| v6.2 | `ConcurrentChain` impl `Clone` (derive) = `Arc::clone`, không copy data |
| v13.0 | `CompactTarget::max()` = `0x207fffff` — KHÔNG phải `0x20000001` |
| v13.3 | Bech32/bech32m tự implement — polymod BIP-173/BIP-350; v0 constant=1, v1 constant=0x2bc830a3 |
| v14.2 | `static_router()` chỉ có `/static/*` — merge vào pktscan_api không conflict với `/` |
| v15.8 | Template server port = PKT wire port + 1 (8333→8334, 64512→64513) |
| v15.8 | `commit_mined_block()` KHÔNG gọi `mine_block_to_hash()` — dùng cho block đã có nonce/hash |
| v15.8 | pktscan selective reload: chỉ copy `chain/utxo_set/difficulty` — giữ `mempool/staking_pool/token_registry` |
| v15.8 | `DEFAULT_NODE = "127.0.0.1:8334"` — cần pkt-node chạy; template server `_ => break` đóng kết nối lạ |
| v22.1 | `insert_utxo(height)` — `height: u64` bắt buộc từ v22.1; data cũ backward-compat với height=0 |
| v23.0 | Merkle root dùng SHA256d (khớp `wire_txid`), KHÔNG phải BLAKE3 |
| v23.5 | `ibd_action()` skip validation nếu block height ≤ checkpoint — không verify signature cũ |
| v24.0 | `install.ps1` dùng `New-Service` (cần Admin); non-admin fallback `~/.local/bin` không có service |
| v24.1 | EVM address = Keccak256(uncompressed_pubkey_64B)[12:32] + EIP-55 checksum; KHÔNG phải RIPEMD160 |
| v24.2 | `pkt_paths::set_mainnet()` phải gọi trước mọi dispatch trong `main.rs` |
| v24.0.9.x | Backend summary trả `block_time_avg`; JS cần `avg_block_time_s ?? block_time_avg` |
| v24.0.9.x | `address-page.js` dùng `API_BASE = '/blockchain-rust'`; gọi `/api/testnet/addr/` KHÔNG phải `/api/address/` |
| v24.0.9.x | `TxInput/TxOutput` field là `value` (paklets), KHÔNG phải `amount`; dùng `Number()` cast vì `[key: string]: unknown` |

---

## 🐛 Lỗi đáng nhớ

| Version | Lỗi | Fix |
|---------|-----|-----|
| v0.5 | `Message::from_digest_slice()` not found | `Message::from_slice()` |
| v0.8 | `pbkdf2_hmac` gated | `features = ["hmac"]` |
| v1.3 | `add_exp_assign` E0599 trên PublicKey | `PublicKey::combine()` |
| v4.3 | RocksDB lock contention trong parallel tests | `static STORAGE_LOCK: Mutex<()>` |
| v4.5 | Miner hang khi VPS không respond | `stream.set_read_timeout(Some(5s))` |
| v15.8 | Double-mining: `mine_live()` rồi `chain.add_block()` mine lại | `commit_mined_block()` |
| v15.8 | pktscan reload xóa mempool (`*bc = fresh`) | selective sync chỉ chain/utxo_set/difficulty |
| v15.8 | Explorer "connection closed" sau đổi DEFAULT_NODE 8334 | template server thiếu GetBlocks handler |
| v22.0 | balance = 0 với địa chỉ bech32/Base58 | `any_addr_to_script_hex()` convert tự động |
| v23.0 | Merkle root mismatch | đổi sang SHA256d thay BLAKE3 |

---

## 🏗 Kiến trúc data flow (v23.8)

```
cargo run -- fullnode [port] [peer]
  ├── spawn subprocess: blockchain-rust sync [peer]  ← RocksDB write lock
  ├── start_watcher thread (auto-restart nếu crash)
  └── pktscan_api::serve(port)  ← REST API read-only

cargo run -- sync [peer]
  └── pkt_sync.rs: connect peer → headers → blocks → UTXO index → addr index
      ~/.pkt/syncdb / utxodb / addrdb (RocksDB)

cargo run -- pkt-node 8333
  └── PKT wire 0.0.0.0:8333 + Template server 0.0.0.0:8334
      (GetTemplate/NewBlock/GetBlocks JSON-lines)

cargo run -- mine [addr]
  ├── kết nối 127.0.0.1:8334 → GetTemplate → blake3 PoW → NewBlock
  └── fallback: seed.testnet.oceif.com:8334 → standalone

Browser (oceif.com/blockchain-rust/)
  └── GET /api/testnet/* → pkt_testnet_web.rs handlers → RocksDB read-only
```

---

## 🔑 OCEIF Network Constants

```
PAKLETS_PER_PKT      = 2^30 = 1,073,741,824
INITIAL_BLOCK_REWARD = 4,096 PKT
HALVING_INTERVAL     = 2^20 = 1,048,576 blocks
PoW domain           = OCEIF_Ann_v1 / OCEIF_Block_v1

Mainnet port  = 64764  peer: seed.oceif.com:64764
Testnet port  = 8333   peer: seed.testnet.oceif.com:8333
Template port = wire_port + 1

Genesis mainnet: hash=00000ccc...96d2  nonce=190223  ts=1775526006
Genesis testnet: hash=00da8943...c970  nonce=156     ts=1775528821
Genesis regtest: hash=357e6f92...4e14  nonce=1
```
