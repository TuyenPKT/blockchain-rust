# Open Consensus Execution Interface Framework — CONTEXT

**Version hiện tại: v25.7 ✅ — Security Hardening (2026-04-19)**

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

### Era 32 — Storage Migration redb (v25.x) ← ĐANG LÀM
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

---

## Roadmap tiếp theo

### Era 32 — còn lại
- [ ] v25.8 — GraphQL yêu cầu API key (hiện chỉ có depth/complexity, chưa authn)
- [ ] v25.9 — Rate limit per API key (gộp với per-IP)

### Era 33 — Mainnet Prep (v26.x)
- [ ] v26.0 — Checkpoints height 50k + 100k (chờ testnet)
- [x] v26.1 — Block reward từ coinbase TX thực ✅ (done trong ps_summary)
- [ ] v26.2 — Pentest: fuzz REST, peer spam, eclipse attack

### Era 20 — Post-Singularity (v99.x)
- [ ] Quantum Random Beacon, Neural Wallet, Interplanetary Sync, AI Consensus
