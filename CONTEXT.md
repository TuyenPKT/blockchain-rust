# Open Consensus Execution Interface Framework — CONTEXT

**Version hiện tại: v27.1 ✅ — Bitcoin Script Parity (2026-04-27)**

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

### Era 27–29 (v20.x–v22.x) ✅
- [x] v20.x — PKTScan Desktop (Tauri v2, React, Charts, Search, CI Release)
- [x] v21.x — Desktop Nâng Cao (Miner IPC, i18n, Wallet + Peer Scan)
- [x] v22.x — Backend Fix (Address Index, UTXO Height, Block TX Count, Broadcast TX, Wallet Send)

### Era 30 — PKT Full Node (v23.x) ✅
- [x] v23.0 — TX Validation
- [x] v23.1 — P2PKH Script Verification
- [x] v23.2 — Block + TX Relay
- [x] v23.3 — Multi-peer Manager
- [x] v23.4 — Mempool Full
- [x] v23.5 — IBD Checkpoints
- [x] v23.6 — Wire Mempool Bridge
- [x] v23.7 — UTXO Snapshot
- [x] v23.8 — Full Node Mode
- [x] v23.8.1 — Security Patch (15 issues)

#### Checklist trước Mainnet
- [ ] Checkpoints thực tế tại height 50k, 100k — chờ testnet đủ blocks
- [x] Địa chỉ coinbase testnet genesis thật vào pkt_labels.rs ✅
- [x] Verify DNS seed.oceif.com:64764 + seed.testnet.oceif.com:8333 ✅
- [x] Tokenomics: 20 PKT/block, 525,000 halving, 21M supply ✅
- [x] Block reward thực từ coinbase TX ✅ (ps_summary đọc outputs của coinbase TX đầu tiên mỗi block)
- [x] RocksDB removed — redb backend duy nhất ✅
- [x] Security audit full + 9 patches (v25.7) ✅

### Era 31 — Public Testnet (v24.x) ✅
- [x] v24.0 — Node Onboarding (install scripts Linux/macOS/Windows)
- [x] v24.1 — EVM Address Format
- [x] v24.2 — Network-aware Data Paths
- [x] v24.3 — Nav Toggle Redesign
- [x] v24.4 — Public Mining Pool
- [x] v24.5 — LZ4 Compression
- [x] v24.6 — Tokenomics 21M PKT
- [x] v24.6.1 — Network Config (pkt_config.rs)
- [x] v24.8 — Developer Docs (OpenAPI + Swagger UI)
- [x] v24.10 — Testnet Audit (tokenomics tests + real checkpoints)

### Era 32 — Storage Migration redb (v25.x) ✅
- [x] v25.0 — RocksKv Abstraction
- [x] v25.1 — RedbKv + feature flag
- [x] v25.2 — redb Default
- [x] v25.3 — VPS Migration + Re-sync
- [x] v25.4 — In-Process Sync (spawn_blocking thay subprocess)
- [x] v25.5 — Remove RocksDB (không còn C++ dep)
- [x] v25.6 — EVM Fix + Template Server + P2P shared_chain + Difficulty Fix + API refresh + Pool auto-start
- [x] **v25.7 — Security Hardening** (2026-04-19)

---

## 🔒 v25.7 — Security Hardening (2026-04-19)

### 9 lỗ hổng đã vá

| # | Severity | Issue | Fix |
|---|----------|-------|-----|
| 1 | CRITICAL | SSRF — webhook/watch callback URL | `url_guard::validate_callback_url()`: block loopback/RFC1918/link-local/IPv4-mapped |
| 2 | CRITICAL | install.sh — không verify binary integrity | Download + verify SHA256SUMS; `curl -sSfL` (fail on error) |
| 3 | HIGH | `POST /rpc` unauthenticated | Mount `auth_middleware` + kiểm tra ApiRole, 401 nếu thiếu key |
| 4 | MEDIUM | Default bind `0.0.0.0` expose toàn bộ API | Default `127.0.0.1`; `PKT_LISTEN=0.0.0.0` để opt-in |
| 5 | MEDIUM | API key hash so sánh không constant-time | `subtle::ConstantTimeEq` thay `==` trên String |
| 6 | MEDIUM | Audit `date` param — path traversal | `is_valid_date_format()`: chỉ chấp nhận `YYYY-MM-DD` |
| 7 | MEDIUM | GraphQL không giới hạn depth/complexity | `.limit_depth(5)` + `.limit_complexity(100)` |
| 8 | MEDIUM | X-Forwarded-For spoofable — bypass rate limit | `trust_proxy: bool` từ `PKT_TRUSTED_PROXY=1`; default off |
| 9 | MEDIUM | Rate limit map tăng trưởng không giới hạn | `max_tracked_ips: 10_000`; purge expired; fail-closed khi full |

### Files
- `src/url_guard.rs` — mới: SSRF guard + 14 tests
- `src/api_auth.rs` — constant-time validate()
- `src/webhook.rs` — validate_callback_url trước khi register
- `src/address_watch.rs` — validate_callback_url thay starts_with("http")
- `src/pkt_rpc.rs` — auth_middleware + ApiRole check
- `src/pktscan_api.rs` — bind 127.0.0.1 default + pass auth vào rpc_router
- `src/audit_log.rs` — is_valid_date_format()
- `src/graphql.rs` — limit_depth + limit_complexity
- `src/zt_middleware.rs` — trust_proxy, max_tracked_ips, extract_ip_with_trust
- `src/lib.rs` — pub mod url_guard
- `src/main.rs` — mod url_guard
- `install.sh` — SHA256SUMS verify + PKT_LISTEN=0.0.0.0 trong systemd unit
- `Cargo.toml` — subtle = "2"

### Tests
- 2473 passed (0 failed)

---

### Era 33 — EVM Compatible Layer (v26.x) ✅

#### v26.0 — Full EVM Stack
- `gas_model.rs`: EIP-1559 base fee, next_base_fee, burn, intrinsic_gas, GasHeader, GAS_CODEDEPOSIT
- `pkt_evm.rs`: Full EVM executor — U256, 140+ opcodes, gas metering
- `eth_rpc.rs`: eth_* JSON-RPC 2.0 (POST /eth) — 13 methods
- `eth_wire.rs`: ETH/68 P2P wire — 13 msg types, FrameCodec
- **75 tests**

#### v26.1 — Ethereum PoW Parity
- `rlp.rs`: RLP encoder/decoder (Bytes/List)
- `uncle.rs`: Uncle/Ommer rewards, validation, UnclePool
- `evm_precompiles.rs`: Precompiles 0x01–0x09 (ecRecover, SHA256, RIPEMD160, Identity, ModExp)
- `abi.rs`: Solidity ABI encode/decode, function_selector, ERC-20 selectors
- `receipts.rs`: Receipt storage + bloom filter (redb)
- EIP-155 replay protection, nonce u64
- **843 tests**

### Era 34 — Bitcoin Script Parity (v27.x) ← ĐANG LÀM

#### v27.0 — Bitcoin Script Complete
- `script.rs`: CLTV (OP_CHECKLOCKTIMEVERIFY), CSV (OP_CHECKSEQUENCEVERIFY), OP_IF/OP_NOTIF/OP_ELSE/OP_ENDIF, HTLC scripts
- `taproot.rs`: Schnorr (BIP340), MAST (BIP341), key path + script path spend
- `lightning.rs`: Payment channels, commitment TX, revocation + penalty, HTLC settlement
- **892 tests**

#### v27.1 — CALL/CREATE Sub-execution EVM
- `pkt_evm.rs`: CALL/STATICCALL/DELEGATECALL/CALLCODE — sub-EVM execution with depth guard (max 1024)
- `pkt_evm.rs`: CREATE/CREATE2 — contract deployment via sub-EVM, address derivation
- `evm_state.rs`: WorldState + snapshot/restore for REVERT semantics
- `pkt_evm.rs`: Rc<RefCell<WorldState>> shared between parent/child EVM contexts
- **909 tests**

---

## 🛠 Các fix quan trọng (session 2026-04-27)

### Bug fixes
| # | Vấn đề | Fix |
|---|--------|-----|
| 1 | `colSpan` mismatch trong Address.tsx | `colSpan={8}` → `colSpan={9}` (loading + empty row) |
| 2 | `txin_temp` storage leak trong pkt_addr_index.rs | Delete key sau khi đọc trong `index_tx_outputs` |
| 3 | Difficulty dùng block timestamp giả mạo được | `LAST_BLOCK_WALL_SECS` AtomicU64 — wall-clock thay header timestamp |
| 4 | Single-block ±1 difficulty oscillation | Dead zone ±20% (48–72s) — no-change nếu trong dải |
| 5 | Miner counter nhảy về 2 khi restart | Emit `mine_stats` ngay sau accept block (không đợi 800ms reporter) |

### XSS fixes (address-page.js)
- `escHtml()` helper added — applied to `addr`, `from`, `to`, `txid` trong tất cả innerHTML template literals
- API `count` → `total` alias thêm vào cả hai endpoint `ps_addr_txs` + `ps_addr_by_base58`

### UI improvements
- **TxChips**: compact clickable txid pills trong block list (≤3 chips + "+N more"), cả Explorer.tsx và web block-list
- **timeAgo format**: "just now" (<10s), "X secs ago", "X mins ago", "X hrs ago", "X days ago" (<30 ngày), date string (>30 ngày — tránh "8352 days ago" với timestamp lỗi)
- **ps_headers enrichment**: mỗi block header bổ sung `tx_count` + `txids` (5 txid đầu)
- `BlockHeader` interface thêm `txids?: string[]`

### File cleanup
- Xoá `TODO.md` (stale v24.0.9.11)
- Xoá `docs/architecture.md` (auto-generated, 5 tuần lỗi thời)
- Xoá `docs/cli.md` (auto-generated, 5 tuần lỗi thời)

---

## 🗺 Attack surface map (sau v25.7)

| Entry | Role | FS effect | Status |
|-------|------|-----------|--------|
| `GET /web/**` | ANON | read `web/` | ✅ ServeDir + zt ../blocked |
| `GET /api/*` | ANON | redb reads | ✅ no write |
| `POST /rpc` | READ+ | redb reads | ✅ 401 nếu anon |
| `POST /graphql` | ANON | redb reads | ⚠ rate limited, depth/complexity capped |
| `POST /api/write/*` | WRITE | mempool KV | ✅ rate limit + script verify |
| `POST /api/webhooks`, `/api/watch` | WRITE | outbound HTTP | ✅ SSRF guard |
| `POST /api/testnet/sync/start` | WRITE | spawn current_exe | ✅ regex peer, no shell |
| `POST /api/keys` | ADMIN | write api_keys.json | ✅ fixed path, chmod 600 |
| `GET /api/admin/logs?date=` | ADMIN | read audit-*.log | ✅ date format validated |

---

## Thông tin kỹ thuật

### Biến môi trường
| Biến | Mặc định | Mục đích |
|------|----------|----------|
| `PKT_LISTEN` | `127.0.0.1` | Set `0.0.0.0` khi expose public (systemd tự set) |
| `PKT_TRUSTED_PROXY` | `0` | Set `1` khi sau nginx để rate limit theo IP thực |

### Ports (testnet)
| Service | Port |
|---------|------|
| PKTScan API + Explorer | 8080 |
| P2P Node | 8333 |
| Mining Pool Proxy | 8337 |
| Mining Pool Stats | 8338 |

### Data dirs
| Mục | Path |
|-----|------|
| Testnet DB | `~/.pkt/testnet/` |
| Mainnet DB | `~/.pkt/mainnet/` |
| API Keys | `~/.pkt/api_keys.json` (chmod 600) |
| Audit log | `~/.pkt/audit/audit-YYYY-MM-DD.log` |

### Lưu ý kỹ thuật mới (v25.7)
- `subtle = "2"`: `ConstantTimeEq` — cả hai chuỗi phải cùng độ dài, nếu không sẽ luôn trả `false` → đảm bảo hash blake3 64 chars khớp đúng
- `url_guard::validate_callback_url`: không resolve DNS — chỉ chặn IP literal và hostname biết trước. DNS rebinding vẫn có thể xảy ra nếu server có outbound DNS tùy biến
- `max_tracked_ips` khi `trust_proxy=false`: tất cả anon traffic vào bucket "unknown" → 1 bucket chung. Rate limit vẫn hoạt động nhưng không phân biệt IP
- GraphQL `limit_complexity(100)`: mỗi field mặc định cost=1; alias có thể nhân cost. Tăng nếu query hợp lệ bị từ chối

### Lưu ý kỹ thuật mới (v27.x)
- `LAST_BLOCK_WALL_SECS: AtomicU64` — static trong pkt_node.rs; `.swap()` trả last value atomically; init=0 → first block dùng TARGET_SECS=60 làm dt
- Difficulty dead zone `DEAD_ZONE=12` (±20%): 48–72s → no change; <48s → +1; >72s → -1. Ngăn oscillation single-block
- `Rc<RefCell<WorldState>>`: snapshot = `world.borrow().clone()` trước sub-call; restore bằng `*world.borrow_mut() = snapshot` khi REVERT
- EVM CREATE stack: pops `val` (top), `off`, `len` → push `len` trước, `off`, `val` cuối (val on top)
- `txin_temp` lifecycle: write trong `index_tx_inputs` → read+delete trong `index_tx_outputs`. Nếu thiếu delete → storage leak tích lũy

---

## Roadmap tiếp theo

### Era 34 — còn lại (v27.x)
- [ ] v27.2 — eth_sendRawTransaction RLP decode + EIP-155 signature verify
- [ ] v27.3 — ETH/68 P2P handshake (Status message exchange với geth peer)
- [ ] v27.4 — Lightning routing (multi-hop HTLC, onion routing stub)
- [ ] v27.5 — Taproot key aggregation on-chain validation

### Era 35 — Mainnet Prep (v28.x)
- [ ] v28.0 — Checkpoints height 50k + 100k (chờ testnet)
- [ ] v28.1 — Pentest: fuzz REST, peer spam, eclipse attack
- [ ] v28.2 — GraphQL yêu cầu API key (hiện chỉ depth/complexity, chưa authn)
- [ ] v28.3 — Rate limit per API key (gộp với per-IP)

### Era 20 — Post-Singularity (v99.x)
- [ ] Quantum Random Beacon, Neural Wallet, Interplanetary Sync, AI Consensus
