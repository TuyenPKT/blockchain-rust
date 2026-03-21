# TODO — Blockchain Rust

**Version hiện tại: v15.8 ✅**
**Tiến độ: 130+ versions — Era 1–23 hoàn thành (trừ Era 20 hardware-dependent)**

---

## ✅ Đã hoàn thành

### Era 1 — Nền tảng (2009)
- [x] v0.1 — Block, Chain, SHA-256, Genesis Block → `block.rs`, `chain.rs`
- [x] v0.2 — Transaction cơ bản, Coinbase TX → `transaction.rs`
- [x] v0.3 — Proof of Work, Mining, Difficulty Adjustment → `block.rs`
- [x] v0.4 — UTXO Model → `utxo.rs`

### Era 2 — Bảo mật & Ví (2010–2013)
- [x] v0.5 — ECDSA Signature, Wallet, Address (Base58) → `wallet.rs`
- [x] v0.6 — P2P Network, Node discovery → `node.rs`, `message.rs`
- [x] v0.7 — Mempool, Transaction Fee, Selection → `mempool.rs`
- [x] v0.8 — HD Wallet (BIP32/39/44) → `hd_wallet.rs`

### Era 3 — Scale & Script (2014–2017)
- [x] v0.9 — Script engine (P2PK, P2PKH, OP_RETURN, stack machine) → `script.rs`
- [x] v1.0 — Multisig P2SH (M-of-N, RedeemScript) → `script.rs`
- [x] v1.1 — SegWit (P2WPKH, witness, txid/wtxid, BIP143) → `transaction.rs`
- [x] v1.2 — Lightning Network (Payment Channel, HTLC, Commitment TX) → `lightning.rs`
- [x] v1.3 — Taproot (Schnorr BIP340, MAST, P2TR, MuSig2) → `taproot.rs`

### Era 4 — Layer 2 & Privacy (2018–2020)
- [x] v1.4 — Covenants & CTV (CheckTemplateVerify, Vault) → `covenant.rs`
- [x] v1.5 — Confidential Transactions (Pedersen, Range Proof, ECDH) → `confidential.rs`
- [x] v1.6 — CoinJoin, PayJoin/P2EP → `coinjoin.rs`
- [x] v1.7 — Atomic Swap (HTLC cross-chain) → `atomic_swap.rs`

### Era 5–9 — Consensus, ZK, AI, PQ, Autonomous (2021–2030)
- [x] v1.8 — ZK-SNARK (Schnorr ZK, R1CS, Groth16) → `zk_proof.rs`
- [x] v1.9 — GHOST Protocol + Uncle Blocks → `pow_ghost.rs`
- [x] v2.0 — BFT Consensus (Tendermint-style) → `bft.rs`
- [x] v2.1 — Sharding (Beacon chain, shard chains, cross-shard receipts) → `sharding.rs`
- [x] v2.2 — ZK-Rollup (Batch TXs, validity proof, L1Verifier) → `zk_rollup.rs`
- [x] v2.3 — Optimistic Rollup (7-day window, fraud proof, slashing) → `optimistic_rollup.rs`
- [x] v2.4 — Recursive ZK / IVC (constant-size proof, fold, aggregation) → `recursive_zk.rs`
- [x] v2.5 — zkEVM (EVM executor, trace, constraint per opcode, ZK proof) → `zkevm.rs`
- [x] v2.6 — WASM Smart Contract engine (gas meter, Counter/Token/Voting) → `smart_contract.rs`
- [x] v2.7 — Oracle (OracleFeed, TWAP, heartbeat, circuit breaker, DeFi) → `oracle.rs`
- [x] v2.8 — On-chain Governance (Governor, Proposal lifecycle, timelock) → `governance.rs`
- [x] v2.9 — AI Agent (DCA/StopLoss/TakeProfit/Rebalance, safety limits) → `ai_agent.rs`
- [x] v3.0 — CRYSTALS-Dilithium (Module-LWE, NIST FIPS 204) → `dilithium.rs`
- [x] v3.1 — SPHINCS+ (WOTS+, XMSS, FORS, NIST FIPS 205) → `sphincs.rs`
- [x] v3.2 — ML-KEM/KYBER (Module-LWE KEM, NIST FIPS 203) → `kyber.rs`
- [x] v3.3 — Hybrid Sig (ECDSA + Dilithium, 3-phase migration) → `hybrid_sig.rs`
- [x] v3.4 — Self-amending chain (on-chain protocol upgrade vote) → `self_amend.rs`
- [x] v3.5 — IBC Cross-chain (ICS-02/03/04/20, 4-way handshake) → `ibc.rs`
- [x] v3.6 — W3C DID + Verifiable Credentials, DID Auth → `did.rs`
- [x] v3.7 — FHE Privacy contracts (LWE, encrypted vote/payroll) → `fhe_contract.rs`
- [x] v3.8 — Sovereign Rollup (DA layer, erasure coding, DAS) → `sovereign_rollup.rs`
- [x] v3.9 — Full Stack Integration (58 versions, 11 eras) → `full_stack.rs`

---

## ✅ Era 10 — PKT Native Chain (2031+)

- [x] v4.0 — PKT Wallet CLI: `wallet new/show/address` → `wallet_cli.rs`
- [x] v4.1 — PacketCrypt PoW: announcement + block mining → `packetcrypt.rs`
- [x] v4.2 — Persistent Storage RocksDB: save/load chain+UTXO → `storage.rs`
- [x] v4.3 — P2P Sync: longest-chain, dedup, GetHeight, mempool broadcast → `node.rs`
- [x] v4.4 — REST API: GET/POST endpoints (axum 0.7) → `api.rs`
- [x] v4.5 — Miner ↔ Node: sync + GetMempool + submit block → `miner.rs`
- [x] v4.6 — Block Explorer CLI: chain/block/tx/balance/utxo → `explorer.rs`
- [x] v4.7 — Testnet Config: NetworkParams, build_genesis(), local testnet → `genesis.rs`
- [x] v4.8 — Metrics: hashrate, peer count, mempool depth, block time, sync → `metrics.rs`
- [ ] v4.9 — PKT Mainnet _(beta)_

---

## ✅ Era 11 — Optimization & Security (2032–2035)

- [x] v5.0 — Performance: UTXO O(1) index, block cache, fast Merkle → `performance.rs`
- [x] v5.1 — Security: RateLimiter, BanList, PeerGuard, InputValidator → `security.rs`
- [x] v5.2 — P2P: PeerRegistry, ScoreEvent EMA, MessageDedup FIFO → `p2p.rs`
- [x] v5.3 — Coinbase maturity (100-block), replay protection, locktime → `maturity.rs`
- [x] v5.4 — Fee market: FeeEstimator (sliding window), RBF 10% bump → `fee_market.rs`
- [x] v5.5 — Storage v2: atomic WriteBatch, WAL epoch, crash recovery → `wal.rs`
- [x] v5.6 — Fuzz + proptest: corpus no-panic, hash determinism, fee bounds → `fuzz.rs`
- [x] v5.7 — Monitoring: tracing logs, HealthStatus, GET /health /ready → `monitoring.rs`
- [x] v5.8 — Peer discovery: PeerStore, DnsSeedResolver, PEX bootstrap → `peer_discovery.rs`
- [x] v5.9 — Benchmark suite: hash/TPS/latency/merkle/UTXO/mempool → `bench.rs`

---

## ✅ Era 12–23 (v6.x – v15.x) — Hoàn thành

- [x] Era 12 (v6.x) — Multi-threading & GPU: BLAKE3, rayon, OpenCL, CUDA, SIMD, HW auto-config
- [x] Era 13 (v7.x) — Token Economy: ERC-20, EVM-lite, DeFi AMM, Staking, Economics
- [x] Era 14 (v8.x) — PKTScan: Block Explorer, WebSocket, Search, Analytics, Cache, Export
- [x] Era 15 (v9.x) — Read-Only APIs + Zero-Trust: OpenAPI, GraphQL, SDK, CORS, WS Subscriptions
- [x] Era 16 (v10.x) — Auth Layer: API Key, Audit Log, EVM fix, Oracle sig, GraphQL, Webhook
- [x] Era 17 (v11.x) — Write APIs + Production: TX/Token/Contract write, Deploy Config
- [x] Era 18 (v12.x) — HD Wallet CLI: BIP39 restore, Ed25519 SLIP-0010
- [x] Era 19 (v13.x) — PKT Core: PacketCrypt chuẩn, Network Steward, PKT Address, Genesis
- [x] Era 21 (v14.x) — UX & Frontend: TUI Dashboard, Wallet TUI, Web, QR, Shell Completions, Charts, Block/Addr Detail, WS Live
- [x] Era 22 (v15.x) — PKT Testnet Integration: Wire Protocol, Peer Connect, Block Download, UTXO Sync, Explorer Live, Sync UI, Testnet Web, PKT Node Server, **Single Chain Architecture (v15.8)**
- [x] Era 23 (v16.x) — Developer Experience: Devnet, Docs Generator, Integration Test Harness, Hot Reload

> Xem chi tiết từng version trong [CONTEXT.md](CONTEXT.md)

---

## 🔜 Era 12 — Multi-threading & GPU Acceleration (2036+)

- [ ] v6.0 — **BLAKE3 Hash Engine**: thay SHA-256 cho PoW, 3–4x nhanh hơn → `blake3_hash.rs`
  - `blake3 = "1.5"` (pure Rust)
  - `hash_version: u8` field trên Block (0=SHA256, 1=BLAKE3)
  - Backward-compatible: SHA-256 giữ nguyên cho ECDSA/address
  - Benchmark BLAKE3 vs SHA-256
- [ ] v6.1 — **CPU Multi-thread Miner**: rayon, nonce splitting, 1/3 cores mặc định → `cpu_miner.rs`
  - `rayon = "1.10"`, `num_cpus = "1.16"`
  - `CpuMinerConfig { threads }` — default `max(1, total/3)`
  - `AtomicBool` solution_found để stop tất cả threads
  - Per-thread hashrate tracking
- [ ] v6.2 — **Thread-safe Chain**: `Arc<RwLock>`, multi-reader + single-writer → `chain_concurrent.rs`
- [ ] v6.3 — **Parallel Block Validation**: `rayon::par_iter()` batch validate khi sync → `validator.rs`
- [ ] v6.4 — **GPU Miner Abstraction**: `GpuBackend { Software, OpenCL, Cuda }`, 1/3 units → `gpu_miner.rs`
- [ ] v6.5 — **OpenCL Kernel**: BLAKE3 OpenCL C kernel, `--features opencl` → `opencl_kernel.rs`
- [ ] v6.6 — **CUDA Kernel**: BLAKE3 PTX kernel, `--features cuda`, CPU fallback → `cuda_kernel.rs`
- [ ] v6.7 — **Mining Pool**: Stratum-like, PoolServer, WorkTemplate, Share → `mining_pool.rs`
- [ ] v6.8 — **SIMD Hash**: BLAKE3 4x AVX2 lanes, `cfg(avx2)`, scalar fallback → `simd_hash.rs`
- [ ] v6.9 — **Hardware Auto-config**: HardwareProfile, detect cores/GPU, `hw-info` → `hw_config.rs`

---

## 📊 Thống kê

| Era | Versions | Hoàn thành | Còn lại |
|-----|----------|-----------|---------|
| Era 1 (2009)       | 4  | 4  | 0  |
| Era 2 (2010–2013)  | 4  | 4  | 0  |
| Era 3 (2014–2017)  | 5  | 5  | 0  |
| Era 4 (2018–2020)  | 4  | 4  | 0  |
| Era 5 (2021)       | 2  | 2  | 0  |
| Era 6 (2022–2023)  | 6  | 6  | 0  |
| Era 7 (2023–2025)  | 4  | 4  | 0  |
| Era 8 (2025–2027)  | 4  | 4  | 0  |
| Era 9 (2027–2030)  | 6  | 6  | 0  |
| Era 10 (2031+)     | 9  | 9  | 0  |
| Era 11 (2032–2035) | 10 | 10 | 0  |
| Era 12 (2036+)     | 10 | 10 | 0  |
| Era 13 (2037+)     | 10 | 10 | 0  |
| Era 14             | 10 | 10 | 0  |
| Era 15             | 10 | 10 | 0  |
| Era 16             | 10 | 10 | 0  |
| Era 17             | 10 | 10 | 0  |
| Era 18             | 1  | 1  | 0  |
| Era 19             | 4  | 4  | 0  |
| Era 21 (UI)        | 9  | 9  | 0  |
| Era 22 (Testnet)   | 9  | 9  | 0  |
| Era 23 (DX)        | 4  | 4  | 0  |
| **Tổng**           | **~130** | **~130** | **0** |

> Cập nhật lần cuối: **v15.8 ✅ — 2023 tests · 0 warnings**
