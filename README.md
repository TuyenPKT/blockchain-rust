# 🦀 Blockchain Rust — 2009 → 2037+

> Xây dựng một blockchain hoàn chỉnh từ Bitcoin 0.1 đến PKT Native Chain bằng Rust thuần — không dùng bất kỳ blockchain framework nào.

**v7.9 ✅ · 75 versions · 13 eras · 307/307 tests · 0 warnings**

---

## Tổng quan

Mỗi version build trực tiếp trên version trước, không viết lại từ đầu. Đọc code theo thứ tự là đọc lịch sử blockchain từ 2009 đến 2037+.

```
Era 1  (2009)       — Bitcoin Genesis: Block, SHA-256, PoW, UTXO
Era 2  (2010–2013)  — ECDSA Wallet, P2P Node, Mempool, HD Wallet
Era 3  (2014–2021)  — Script, Multisig P2SH, SegWit, Lightning, Taproot
Era 4  (2018–2020)  — Covenants/CTV, Confidential TX, CoinJoin, Atomic Swap
Era 5  (2021)       — ZK-SNARK Groth16, GHOST Protocol
Era 6  (2022–2023)  — BFT, Sharding, ZK-Rollup, Optimistic Rollup, zkEVM
Era 7  (2023–2025)  — WASM Contracts, Oracle, Governance, AI Agent
Era 8  (2025–2027)  — Post-Quantum: Dilithium, SPHINCS+, ML-KEM, Hybrid
Era 9  (2027–2030)  — Self-Amend, IBC, W3C DID, FHE, Sovereign Rollup
Era 10 (2031+)      — PKT Native Chain: PacketCrypt PoW, RocksDB, REST API, Testnet, Metrics  ✅
Era 11 (2032–2035)  — Optimization & Security: UTXO index, Fee market, WAL, Fuzz, Monitoring  ✅
Era 12 (2036)       — Multi-threading & GPU: BLAKE3, rayon, OpenCL, CUDA (v6.0–v6.6 ✅)
Era 13 (2037+)      — Token Economy: Block Reward, ERC-20, EVM-lite, DeFi AMM, Staking         ✅
```

---

## Cài đặt

**Yêu cầu:** Rust 1.75+

```bash
git clone https://github.com/TuyenPKT/blockchain-rust.git
cd blockchain-rust
cargo build
cargo test        # 307 tests, 0 warnings
```

**GPU mining (tùy chọn):**
```bash
cargo build --features opencl   # OpenCL (AMD/Intel/NVIDIA)
cargo build --features cuda     # CUDA (NVIDIA — yêu cầu nvcc)
```

---

## Sử dụng

```bash
# Tạo ví PKT mới
cargo run -- wallet new
cargo run -- wallet show

# Mining (mặc định: cores/3 threads)
cargo run -- mine                              # mine dùng ví đã tạo
cargo run -- mine <addr_hex> <n>               # mine n blocks
cargo run -- mine <addr_hex> <n> <node:port>   # mine + kết nối P2P node
cargo run -- mine <addr> 0 --threads 8        # mine với 8 threads

# CPU multi-thread miner (v6.1)
cargo run -- cpumine                           # mine với rayon (cores/3)
cargo run -- cpumine <addr> <diff> <n>         # mine n blocks, difficulty=diff

# GPU miner (v6.4–v6.6)
cargo run -- gpumine                                    # software backend (CPU rayon)
cargo run -- gpumine <addr> <diff> <n> software        # CPU rayon fallback
cargo run -- gpumine <addr> <diff> <n> opencl          # OpenCL GPU
cargo run --features opencl -- gpumine <addr> 3 10 opencl
cargo run --features cuda   -- gpumine <addr> 3 10 cuda

# BLAKE3 benchmark (v6.0)
cargo run -- blake3                            # BLAKE3 vs SHA-256 throughput

# P2P Node (v5.8: tự động peer discovery)
cargo run -- node 8333                         # chạy node + auto-discover peers
cargo run -- node 8334 127.0.0.1:8333          # chạy node + kết nối peer cụ thể

# REST API
cargo run -- api 3000                          # khởi động API tại port 3000

# Block Explorer
cargo run -- explorer chain
cargo run -- explorer block <height>
cargo run -- explorer tx <tx_id>
cargo run -- explorer balance <addr>
cargo run -- explorer utxo <addr>

# Testnet
cargo run -- testnet                           # 3 nodes local testnet
cargo run -- testnet 5 18444                   # 5 nodes, base port 18444
cargo run -- genesis testnet                   # xem testnet config

# Metrics & Monitoring
cargo run -- metrics                           # đọc từ local RocksDB
cargo run -- metrics 127.0.0.1:8333            # + query peer count và remote height
cargo run -- monitor                           # health server tại port 3001
cargo run -- monitor 3002                      # health server tại port 3002

# Benchmark suite (v5.9)
cargo run -- bench all                         # toàn bộ benchmarks
cargo run -- bench hash                        # hash throughput
cargo run -- bench tps                         # transactions per second
cargo run -- bench mining                      # block mining latency
cargo run -- bench merkle                      # merkle_std vs fast_merkle
cargo run -- bench utxo                        # UTXO scan O(n) vs index O(1)
cargo run -- bench mempool                     # mempool select

# Tests
cargo test
```

---

## REST API (port 3000)

| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/chain` | Toàn bộ chain |
| GET | `/chain/:height` | Block tại height |
| GET | `/balance/:addr` | Số dư địa chỉ |
| GET | `/mempool` | TX đang chờ |
| POST | `/tx` | Thêm TX vào mempool |
| GET | `/status` | Trạng thái node |
| GET | `/metrics` | Hashrate, peers, mempool, block time |

## Health API (port 3001 — v5.7)

| Method | Endpoint | Mô tả |
|--------|----------|-------|
| GET | `/health` | HealthStatus JSON (uptime, height, fees, ...) |
| GET | `/ready` | 200 OK hoặc 503 khi chưa synced |
| GET | `/version` | `{"version": "v7.9"}` |

---

## Era 11 — Optimization & Security ✅

| Version | Module | Nội dung |
|---------|--------|---------|
| v5.0 | `performance.rs` | UTXO O(1) index, BlockCache O(1), fast_merkle |
| v5.1 | `security.rs` | RateLimiter, BanList, PeerGuard, InputValidator |
| v5.2 | `p2p.rs` | PeerRegistry, ScoreEvent EMA, MessageDedup FIFO |
| v5.3 | `maturity.rs` | Coinbase 100-block lockup, replay protection, locktime |
| v5.4 | `fee_market.rs` | FeeEstimator sliding window 20 blocks, RBF 10% |
| v5.5 | `wal.rs` | Atomic WriteBatch, WAL epoch, UTXO crash recovery |
| v5.6 | `fuzz.rs` | Fuzz corpus, proptest: hash/fee/RBF/roundtrip |
| v5.7 | `monitoring.rs` | tracing logs, HealthStatus, /health /ready /version |
| v5.8 | `peer_discovery.rs` | PeerStore, DnsSeedResolver, PEX, auto-connect |
| v5.9 | `bench.rs` | BenchResult/Suite, hash/TPS/latency/merkle/UTXO/mempool |

## Era 12 — Multi-threading & GPU (v6.0–v6.6 ✅)

| Version | Module | Nội dung |
|---------|--------|---------|
| v6.0 | `blake3_hash.rs` | BLAKE3 thay SHA-256 cho PoW (3–4x nhanh hơn), `hash_version` |
| v6.1 | `cpu_miner.rs` | rayon work-stealing, nonce splitting, default=cores/3 |
| v6.2 | `chain_concurrent.rs` | `Arc<RwLock<Blockchain>>`, multi-reader + single-writer |
| v6.3 | `validator.rs` | Parallel block validation với `rayon::par_iter()` |
| v6.4 | `gpu_miner.rs` | `GpuBackend { Software, OpenCL, Cuda }`, 1/3 compute units |
| v6.5 | `opencl_kernel.rs` | BLAKE3 OpenCL C kernel, full 7-round, `--features opencl` |
| v6.6 | `cuda_kernel.rs` | BLAKE3 CUDA PTX kernel, `atomicCAS`, `--features cuda` |
| v6.7 | `mining_pool.rs` | Stratum-like pool, WorkTemplate, Share _(upcoming)_ |
| v6.8 | `simd_hash.rs` | BLAKE3 AVX2 4x lanes SIMD _(upcoming)_ |
| v6.9 | `hw_config.rs` | HardwareProfile, auto-config _(upcoming)_ |

## Era 13 — Token Economy ✅

| Version | Module | Nội dung |
|---------|--------|---------|
| v7.0 | `reward.rs` | Block Reward Engine: halving schedule, `subsidy_at()`, `estimated_supply()` |
| v7.1 | `fee_calculator.rs` | Fee Calculator: vsize P2PKH, `FeePolicy`, `validate_coinbase()` |
| v7.2 | `token.rs` | Token Standard (ERC-20): `TokenRegistry`, mint/transfer/burn/approve |
| v7.3 | `token_tx.rs` | Token Transfer TX: OP_RETURN encoding, BLAKE3 txid, `TokenTxBuilder` |
| v7.4 | `contract_state.rs` | Smart Contract State: `ContractStore`, storage_root, snapshot/restore |
| v7.5 | `evm_lite.rs` | EVM-lite Executor: 30 opcodes, gas model, SLoad/SStore, Log, Revert |
| v7.6 | `contract_deploy.rs` | Contract Deployment: CREATE/CREATE2 address, `AbiEncoder` |
| v7.7 | `defi.rs` | DeFi AMM: `LiquidityPool` x\*y=k, swap, add/remove liquidity, `DEX` |
| v7.8 | `staking.rs` | Staking & Delegation: delegate/slash/APY, distribute rewards |
| v7.9 | `economics.rs` | Economic Model: `EraParams`, fee burn, `Simulator::project(n_blocks)` |

---

## Cấu trúc source (74 files)

```
src/
├── main.rs              CLI dispatch + 307 integration tests
├── block.rs             Block, BLAKE3, Merkle root, PoW
├── chain.rs             Blockchain, validation, difficulty (target=300s)
├── transaction.rs       TxInput/TxOutput, txid/wtxid, SegWit
├── utxo.rs              UTXO set, P2PKH + P2TR balance lookup
├── wallet.rs            ECDSA keypair, Bitcoin address Base58Check
├── mempool.rs           Mempool, fee-rate selection, RBF
├── message.rs           P2P message protocol
├── node.rs              TCP node, chain sync, peer discovery
├── hd_wallet.rs         BIP32/39/44 HD Wallet
├── script.rs            Script engine, P2PK/P2PKH/P2SH/P2WPKH/P2TR
├── lightning.rs         Payment channels, HTLC, commitment TX
├── taproot.rs           Schnorr BIP340, MAST, P2TR, MuSig2
├── covenant.rs          CTV (CheckTemplateVerify), Vault
├── confidential.rs      Pedersen commitment, range proof, ECDH
├── coinjoin.rs          CoinJoin, PayJoin/P2EP
├── atomic_swap.rs       HTLC cross-chain atomic swap
├── zk_proof.rs          Schnorr ZK, R1CS, Groth16
├── pow_ghost.rs         GHOST protocol, uncle blocks
├── bft.rs               BFT Tendermint-style consensus
├── sharding.rs          Beacon chain, shard chains, cross-shard receipts
├── zk_rollup.rs         ZK-Rollup batch + validity proof
├── optimistic_rollup.rs Optimistic Rollup + fraud proof
├── recursive_zk.rs      Recursive ZK / IVC, proof aggregation
├── zkevm.rs             zkEVM opcode tracing + constraints
├── smart_contract.rs    WASM interpreter, gas meter
├── oracle.rs            OracleFeed, TWAP, circuit breaker
├── governance.rs        Governor, proposal lifecycle, timelock
├── ai_agent.rs          DCA/StopLoss/TakeProfit/Rebalance agent
├── dilithium.rs         CRYSTALS-Dilithium NIST FIPS 204
├── sphincs.rs           SPHINCS+ NIST FIPS 205
├── kyber.rs             ML-KEM NIST FIPS 203
├── hybrid_sig.rs        Hybrid ECDSA + Dilithium
├── self_amend.rs        On-chain protocol upgrade voting
├── ibc.rs               IBC (ICS-02/03/04/20) cross-chain
├── did.rs               W3C DID Core + Verifiable Credentials
├── fhe_contract.rs      FHE-LWE privacy contracts
├── sovereign_rollup.rs  DA layer, erasure coding, DAS
├── full_stack.rs        Version registry, era descriptions, stats
├── miner.rs             Live PoW miner, hashrate display
├── wallet_cli.rs        PKT Wallet CLI commands
├── packetcrypt.rs       PacketCrypt PoW (announcement + block)
├── storage.rs           RocksDB persistent chain + UTXO storage
├── api.rs               REST API (axum 0.7)
├── explorer.rs          Block Explorer CLI
├── genesis.rs           NetworkParams, testnet genesis config
├── metrics.rs           Runtime metrics: hashrate, peers, mempool, sync
├── performance.rs       UTXO O(1) index, BlockCache, fast_merkle       ← v5.0
├── security.rs          RateLimiter, BanList, PeerGuard                ← v5.1
├── p2p.rs               PeerRegistry, ScoreEvent, MessageDedup         ← v5.2
├── maturity.rs          CoinbaseGuard, TxReplayGuard, LockTime         ← v5.3
├── fee_market.rs        FeeEstimator, RBF replace-by-fee               ← v5.4
├── wal.rs               Atomic WAL, crash recovery                     ← v5.5
├── fuzz.rs              Fuzz corpus, proptest invariants               ← v5.6
├── monitoring.rs        tracing logs, HealthStatus, /health            ← v5.7
├── peer_discovery.rs    PeerStore, DnsSeedResolver, PEX                ← v5.8
├── bench.rs             BenchResult/Suite, all benchmarks              ← v5.9
├── blake3_hash.rs       BLAKE3 hash engine, Blake3Block, benchmark     ← v6.0
├── cpu_miner.rs         rayon multi-thread miner, nonce splitting      ← v6.1
├── chain_concurrent.rs  Arc<RwLock> thread-safe chain                  ← v6.2
├── validator.rs         Parallel block validation (rayon)              ← v6.3
├── gpu_miner.rs         GpuBackend abstraction, detect_devices         ← v6.4
├── opencl_kernel.rs     BLAKE3 OpenCL C kernel (--features opencl)    ← v6.5
├── cuda_kernel.rs       BLAKE3 CUDA PTX kernel (--features cuda)      ← v6.6
├── reward.rs            Block Reward Engine, halving schedule          ← v7.0
├── fee_calculator.rs    Fee Calculator, FeePolicy, coinbase validation ← v7.1
├── token.rs             ERC-20 TokenRegistry, mint/transfer/burn       ← v7.2
├── token_tx.rs          Token Transfer TX, OP_RETURN, TokenTxBuilder   ← v7.3
├── contract_state.rs    ContractStore, storage_root, snapshot/restore  ← v7.4
├── evm_lite.rs          EVM-lite stack VM, 30 opcodes, gas model       ← v7.5
├── contract_deploy.rs   CREATE/CREATE2, AbiEncoder, ContractDeployer   ← v7.6
├── defi.rs              AMM LiquidityPool x*y=k, swap, DEX             ← v7.7
├── staking.rs           Staking, delegate, slash, APY                  ← v7.8
└── economics.rs         TokenEconomics, EraParams, Simulator           ← v7.9
```

---

## Dependencies

```toml
sha2       = "0.10"    # SHA-256 (ECDSA messages, address)
hex        = "0.4"
chrono     = "0.4"
serde      = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
secp256k1  = { version = "0.27", features = ["rand-std", "global-context"] }
ripemd     = "0.1"     # RIPEMD-160 (Bitcoin address)
bs58       = "0.5"     # Base58Check
tokio      = { version = "1", features = ["full"] }
hmac       = "0.12"    # BIP32 HD wallet
pbkdf2     = { version = "0.12", features = ["hmac"] }  # BIP39
rocksdb    = "0.21"    # v4.2: persistent storage
axum       = "0.7"     # v4.4: REST API
tracing    = "0.1"     # v5.7: structured logging
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
blake3     = "1.5"     # v6.0: BLAKE3 (3–4x faster than SHA-256 for PoW)
rayon      = "1.10"    # v6.1: CPU parallel mining
num_cpus   = "1.16"    # v6.1: core count detection

# Optional GPU backends:
ocl  = { version = "0.19", optional = true }  # v6.5: OpenCL
cust = { version = "0.3",  optional = true }  # v6.6: CUDA
```

---

## Testnet

Bootstrap peer: `seed.testnet.oceif.com:18333`

```bash
# Kết nối node PC đến VPS testnet
cargo run -- node 18334 seed.testnet.oceif.com:18333

# Mine về VPS seed
cargo run -- mine <addr> 0 seed.testnet.oceif.com:18333

# Check health node
curl http://seed.testnet.oceif.com:3001/health
```

---

## License

GPL-3.0 license
