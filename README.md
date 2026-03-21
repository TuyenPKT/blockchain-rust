# 🦀 Blockchain Rust — 2009 → 2037+

> Xây dựng một blockchain hoàn chỉnh từ Bitcoin 0.1 đến PKT Native Chain bằng Rust thuần — không dùng bất kỳ blockchain framework nào.

**v15.8 ✅ · 130+ versions · 22 eras · 2023 tests · 0 warnings**

---

## Tổng quan

Mỗi version build trực tiếp trên version trước, không viết lại từ đầu. Đọc code theo thứ tự là đọc lịch sử blockchain từ 2009 đến 2037+.

```
Era 1  (2009)       — Bitcoin Genesis: Block, SHA-256, PoW, UTXO                          ✅
Era 2  (2010–2013)  — Security & Wallets: ECDSA, P2P Node, Mempool, HD Wallet             ✅
Era 3  (2014–2021)  — Scale & Script: SegWit, Lightning, Taproot, Multisig                ✅
Era 4  (2018–2020)  — Layer 2 & Privacy: CTV, Confidential TX, CoinJoin, Atomic Swap      ✅
Era 5  (2021)       — Advanced Consensus: ZK-SNARK, GHOST, BFT, Sharding                  ✅
Era 6  (2022–2023)  — ZK & Rollup: ZK-Rollup, Optimistic, Recursive ZK, zkEVM            ✅
Era 7  (2023–2025)  — AI & Programmable: WASM Contracts, Oracle, Governance, AI Agent    ✅
Era 8  (2025–2027)  — Post-Quantum: Dilithium, SPHINCS+, ML-KEM, Hybrid Sigs             ✅
Era 9  (2027–2030)  — Autonomous Chain: IBC, DID, FHE, Sovereign Rollup                  ✅
Era 10 (2031+)      — PKT Native Chain: PacketCrypt PoW, RocksDB, REST API, Metrics       ✅
Era 11 (2032–2035)  — Optimization & Security: fee market, WAL, fuzz, monitoring          ✅
Era 12 (2036+)      — Multi-threading & GPU: BLAKE3, rayon, OpenCL, CUDA, SIMD            ✅
Era 13 (2037+)      — Token Economy: ERC-20, EVM-lite, DeFi AMM, Staking, Economics       ✅
Era 14              — PKTScan & API Integration: Block Explorer, WS, Search, Analytics    ✅
Era 15              — Read-Only APIs + Zero-Trust: OpenAPI, GraphQL, SDK, CORS            ✅
Era 16              — Auth Layer: API Key, Audit Log, EVM fix, Webhooks, GraphQL          ✅
Era 17              — Write APIs + Production: TX/Token/Contract write, Deploy Config     ✅
Era 18              — HD Wallet & UX: BIP39 restore, Ed25519 SLIP-0010                   ✅
Era 19              — PKT Core: PacketCrypt chuẩn, Network Steward, PKT Address, Genesis  ✅
Era 21              — UX & Frontend: TUI Dashboard, Wallet TUI, Web Frontend, QR Code     ✅
Era 22              — PKT Testnet Integration: Wire Protocol, Sync, Explorer, PKT Node    ✅ v15.8
Era 23              — Developer Experience: Devnet, Docs, E2E Tests, Hot Reload           ✅
```

---

## Cài đặt

**Yêu cầu:** Rust 1.75+

```bash
git clone https://github.com/TuyenPKT/blockchain-rust.git
cd blockchain-rust
cargo build
cargo test        # 2023 tests, 0 warnings
```

**GPU mining (tùy chọn):**
```bash
cargo build --features opencl   # OpenCL (AMD/Intel/NVIDIA)
cargo build --features cuda     # CUDA (NVIDIA — yêu cầu nvcc)
```

---

## Sử dụng

```bash
# Tạo ví PKT
cargo run -- wallet new                       # tạo ví mới (BIP39 12 từ)
cargo run -- wallet show                      # xem ví + seed phrase
cargo run -- wallet restore <word1>...<word12> # khôi phục từ mnemonic

# QR Code (v14.3)
cargo run -- qr <pkt-address>                 # QR address trong terminal
cargo run -- qr <address> 10.5               # QR payment URI (BIP21)
cargo run -- qr <address> 10.5 "donation"    # QR + label

# PKT Node (v15.7–v15.8)
cargo run -- pkt-node                         # PKT wire node (64512) + template server (64513)
cargo run -- pkt-node 8333                    # PKT wire (8333) + template server (8334)
cargo run -- pkt-node 8333 --mainnet          # mainnet magic

# Mining
cargo run -- mine                             # mine, kết nối local node 127.0.0.1:8334
                                              # fallback: seed.testnet.oceif.com:8334 → standalone
cargo run -- mine <addr_hex> <n>              # mine n blocks
cargo run -- mine <addr_hex> <n> <node:port>  # mine + kết nối node cụ thể
cargo run -- cpumine                          # CPU multi-thread (rayon, cores/3)
cargo run -- gpumine                          # GPU/software backend
cargo run -- gpumine <addr> <diff> <n> opencl # OpenCL GPU (--features opencl)
cargo run -- gpumine <addr> <diff> <n> cuda   # CUDA GPU (--features cuda)
cargo run -- hw-info                          # auto-detect hardware + miner config

# Token CLI (v11.6)
cargo run -- token create <name> <symbol> <supply>
cargo run -- token list
cargo run -- token mint <id> <to> <amount>
cargo run -- token transfer <id> <from> <to> <amount>
cargo run -- token balance <id> <addr>

# Contract CLI (v11.7)
cargo run -- contract deploy <template> [args...]   # template: counter/token/voting
cargo run -- contract list
cargo run -- contract call <addr> <method> [args...]
cargo run -- contract state <addr>
cargo run -- contract estimate <addr> <method> [args...]

# Staking CLI (v11.8)
cargo run -- staking validators
cargo run -- staking register <addr> <commission>
cargo run -- staking delegate <validator> <delegator> <amount>
cargo run -- staking rewards <addr>
cargo run -- staking claim <addr>

# Deploy Config (v11.9)
cargo run -- deploy init [mainnet|testnet|regtest]   # sinh 7 config files
cargo run -- deploy dockerfile
cargo run -- deploy compose
cargo run -- deploy systemd
cargo run -- deploy nginx

# API key (v10.0)
cargo run -- apikey new                       # tạo API key
cargo run -- apikey list

# P2P Node
cargo run -- node 8333                        # chạy node + auto-discover peers
cargo run -- node 8334 127.0.0.1:8333         # chạy node + kết nối peer cụ thể

# PKTScan Explorer + API Server (Era 14+)
cargo run -- pktscan                          # khởi động tại port 3000
cargo run -- pktscan 4000                     # tại port 4000

# Block Explorer CLI
cargo run -- explorer chain
cargo run -- explorer block <height>
cargo run -- explorer tx <tx_id>
cargo run -- explorer balance <addr>
cargo run -- explorer utxo <addr>

# Testnet
cargo run -- testnet                          # 3 nodes local testnet
cargo run -- testnet 5 18444                  # 5 nodes, base port 18444
cargo run -- genesis testnet                  # xem testnet config

# Metrics & Monitoring
cargo run -- metrics                          # đọc từ local RocksDB
cargo run -- monitor                          # health server tại port 3001

# Benchmarks
cargo run -- bench all
cargo run -- bench hash|tps|mining|merkle|utxo|mempool

# BLAKE3
cargo run -- blake3                           # BLAKE3 vs SHA-256 throughput
```

---

## PKTScan API (port 3000)

### Chain & Blocks
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/api/chain` | Danh sách blocks (cursor pagination) |
| GET | `/api/chain/:height` | Block tại height |
| GET | `/api/blocks?from=<h>&limit=<n>` | Blocks theo cursor |
| GET | `/api/blocks.csv` | Export CSV |
| GET | `/api/analytics/:metric?window=N` | Time-series: block_time/hashrate/fee/difficulty/tps |
| GET | `/api/search?q=<hash\|height\|addr>&limit=N` | Tìm kiếm block/tx/addr |

### Transactions
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/api/txs?from=<h>&min_amount=X&since=T` | Danh sách TXs (filter) |
| GET | `/api/txs.csv` | Export CSV |
| GET | `/api/tx/:txid` | TX detail + status (confirmed/pending) + confirmations |
| GET | `/api/mempool` | Mempool + fee distribution + percentiles |

### Address & Balance
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/api/balance/:addr` | Số dư |
| GET | `/api/address/:addr` | Balance + tx_history + tx_count |
| GET | `/api/labels` | Danh sách address labels |
| GET | `/api/label/:addr` | Label của địa chỉ |
| GET | `/api/risk/:addr` | Risk score (scam/phishing/mixer...) |

### Token & DeFi
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/api/tokens` | Danh sách tokens |
| GET | `/api/token/:id` | Token info |
| GET | `/api/token/:id/holders` | Danh sách holders |
| GET | `/api/token/:id/balance/:addr` | Token balance |
| GET | `/api/defi/feeds` | Oracle price feeds |
| GET | `/api/defi/loans` | Loan positions |
| GET | `/api/staking/stats` | Tổng stake, APY |
| GET | `/api/staking/validators` | Validator list |

### Smart Contracts
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/api/contracts` | Danh sách contracts |
| GET | `/api/contract/:addr` | Contract info |
| GET | `/api/contract/:addr/state` | Full state |
| GET | `/api/contract/:addr/state/:key` | Một key |

### Multi-chain (IBC)
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/api/chains` | Các chain được register |
| GET | `/api/chains/:id/channels` | IBC channels |
| GET | `/api/chains/:id/packets/pending` | Pending packets |

### Write API (yêu cầu API Key — role: write/admin)
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| POST | `/api/write/tx` | Submit TX vào mempool |
| POST | `/api/write/token/mint` | Mint token (owner sig) |
| POST | `/api/write/token/transfer` | Transfer token (sender sig) |
| POST | `/api/write/contract/deploy` | Deploy contract |
| POST | `/api/write/contract/call` | Call contract |
| POST | `/api/webhooks` | Đăng ký webhook |
| DELETE | `/api/webhooks/:id` | Xoá webhook |

### Admin (yêu cầu API Key — role: admin)
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/api/admin/logs?date=&limit=&offset=` | Audit log |
| POST | `/api/risk/:addr` | Thêm risk entry |
| DELETE | `/api/risk/:addr` | Xoá risk entry |

### Developer
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/api/openapi.json` | OpenAPI 3.0.3 spec (35 paths) |
| GET/POST | `/graphql` | GraphQL (read-only) |
| GET | `/api/sdk/js` | Generated JS client SDK |
| GET | `/api/sdk/ts` | Generated TypeScript client SDK |

### Health (port 3001)
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/health` | HealthStatus JSON |
| GET | `/ready` | 200 OK hoặc 503 |
| GET | `/version` | `{"version": "v14.3"}` |

### Pool & Mining
| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/api/pool/stats` | Pool hashrate, shares, blocks found |
| GET | `/api/pool/miners` | Per-miner stats |

### WebSocket
| Endpoint | Mô tả |
|----------|-------|
| `/ws` | Real-time NewBlock/NewTx events |
| `/ws?watch=<addr>&token=<tok>` | Subscribe events cho địa chỉ cụ thể |

---

## PKT Coin Params

```
PAKLETS_PER_PKT     = 2^30  = 1,073,741,824
INITIAL_BLOCK_REWARD = 4096 PKT
HALVING_INTERVAL    = 2^20  = 1,048,576 blocks
MAX_SUPPLY          = 6,000,000,000 PKT
Steward reward      = 20% mỗi block

Mainnet port  = 64764  (magic: d9 b4 be f9)
Testnet port  = 64765  (magic: 0b 11 09 07)
Regtest port  = 18444
```

---

## Dependencies

```toml
sha2 = "0.10"           # SHA-256 (ECDSA messages, address)
hex = "0.4"
chrono = "0.4"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
secp256k1 = { version = "0.27", features = ["rand-std", "global-context"] }
ripemd = "0.1"          # RIPEMD-160 (Bitcoin address)
bs58 = "0.5"            # Base58Check
tokio = { version = "1", features = ["full"] }
hmac = "0.12"           # BIP32 HD wallet
pbkdf2 = { version = "0.12", features = ["hmac"] }  # BIP39
rocksdb = "0.21"        # v4.2: persistent storage
axum = { version = "0.7", features = ["ws"] }  # v4.4: REST API + WebSocket
async-graphql = "7"     # v10.8: GraphQL read-only API
reqwest = "0.12"        # v10.9: outbound webhook delivery
tracing = "0.1"         # v5.7: structured logging
tracing-subscriber = "0.3"
blake3 = "1.5"          # v6.0: BLAKE3 (3–4x faster than SHA-256 for PoW)
rayon = "1.10"          # v6.1: CPU parallel mining
num_cpus = "1.16"
ed25519-dalek = "2"     # v9.0.1: Ed25519 HD Wallet (SLIP-0010)
zeroize = "1"
rand_core = "0.6"
ratatui = "0.26"        # v14.0: Terminal UI dashboard
crossterm = "0.27"      # v14.0: Terminal input/output
qrcode = "0.14"         # v14.3: QR code render (pure Rust)

# Optional GPU backends:
ocl  = { version = "0.19", optional = true }  # v6.5: OpenCL
cust = { version = "0.3",  optional = true }  # v6.6: CUDA
```

---

## Testnet

Bootstrap peer: `seed.testnet.oceif.com:8333` (PKT wire) / `seed.testnet.oceif.com:8334` (template)

```bash
# Chạy pkt-node local (wire port 8333, template port 8334)
cargo run -- pkt-node 8333

# Mine về VPS testnet node (tự động fallback nếu local không có)
cargo run -- mine
# hoặc chỉ định node cụ thể:
cargo run -- mine <addr> 0 seed.testnet.oceif.com:8334

# Block explorer CLI (kết nối template server)
cargo run -- explorer chain

# PKTScan web explorer
cargo run -- pktscan 3000
# Browser: http://localhost:3000

# Check health
curl http://seed.testnet.oceif.com:3001/health
```

**Web explorer:** [oceif.com/blockchain-rust](https://oceif.com/blockchain-rust/)

---

## License

GPL-3.0 license
