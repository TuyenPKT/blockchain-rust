# 🦀 Blockchain Rust — CONTEXT

**Version hiện tại: v9.0 ✅ — 502 tests pass, 0 errors, 0 warnings**

---

## ✅ Tiến độ

### Era 1–9 (v0.1 – v3.9) ✅ Hoàn thành
| Era | Versions | Nội dung |
|-----|----------|---------|
| Era 1 | v0.1–v0.4 | Block, Chain, SHA-256, Genesis, PoW, UTXO |
| Era 2 | v0.5–v0.8 | ECDSA Wallet, P2P Node, Mempool, HD Wallet |
| Era 3 | v0.9–v1.3 | Script Engine, Multisig P2SH, SegWit, Lightning, Taproot |
| Era 4 | v1.4–v1.7 | Covenants/CTV, Confidential TX, CoinJoin, Atomic Swap |
| Era 5 | v1.8–v2.1 | ZK-SNARK, GHOST PoW, BFT Consensus, Sharding |
| Era 6 | v2.2–v2.5 | ZK-Rollup, Optimistic Rollup, Recursive ZK, zkEVM |
| Era 7 | v2.6–v2.9 | Smart Contract (WASM), Oracle, Governance, AI Agent |
| Era 8 | v3.0–v3.3 | Dilithium, SPHINCS+, KYBER, Hybrid Sig (Post-Quantum) |
| Era 9 | v3.4–v3.9 | Self-amend, IBC, DID, FHE, Sovereign Rollup, Full Stack |

### Era 10 — PKT Native Chain (2031+)
- [x] v4.0 — PKT Wallet CLI: `wallet new/show/address`, miner auto-load
- [x] v4.1 — **PacketCrypt PoW**: announcement mining + block mining (`src/packetcrypt.rs`) 🔴
- [x] v4.2 — **Persistent Storage** RocksDB: save/load chain+UTXO, load_or_new() (`src/storage.rs`) 🔴
- [x] v4.3 — **P2P Sync**: longest-chain rule, block/TX dedup, GetHeight, mempool broadcast 🟡
- [x] v4.4 — **REST API**: `GET /chain`, `GET /chain/:height`, `GET /balance/:addr`, `GET /mempool`, `POST /tx`, `GET /status` (`src/api.rs`) 🟡
- [x] v4.5 — **Miner ↔ Node**: sync chain + fetch mempool TXs từ node, submit block qua P2P 🟡
- [x] v4.6 — **Block Explorer CLI**: `explorer chain/block/tx/balance/utxo` (`src/explorer.rs`) 🟢
- [x] v4.7 — **Testnet Config**: `NetworkParams`, `build_genesis()`, `run_local_testnet()` (`src/genesis.rs`) 🟡
- [x] v4.8 — **Metrics**: hashrate, peer count, mempool size, block time, sync status (`src/metrics.rs`) 🟢
- [ ] v4.9 — PKT Mainnet _(beta — chưa lên kế hoạch)_

### Era 11 — Optimization & Security (v5.x)
- [x] v5.0 — **Performance**: UTXO secondary index O(1), block cache O(1), fast Merkle (`src/performance.rs`) 🟢
- [x] v5.1 — **Security hardening**: RateLimiter, BanList, PeerGuard, InputValidator (`src/security.rs`) 🟢
- [x] v5.2 — **P2P improvements**: PeerRegistry, ScoreEvent, MessageDedup bounded cache (`src/p2p.rs`) 🟢
- [x] v5.3 — **Coinbase maturity**: 100-block lockup, replay protection, BIP-style locktime/sequence (`src/maturity.rs`) 🟢
- [x] v5.4 — **Fee market**: FeeEstimator (sliding window 20 blocks, 3 targets), RBF (10% min bump) (`src/fee_market.rs`) 🟢
- [x] v5.5 — **Storage v2**: atomic WriteBatch, WAL epoch (crash detection), UTXO rebuild on recovery (`src/wal.rs`) 🟢
- [x] v5.6 — **Fuzz + proptest**: corpus no-panic, hash determinism, Message roundtrip, fee bounds, RBF consistency (`src/fuzz.rs`) 🟢
- [x] v5.7 — **Monitoring**: `tracing` structured logs, `HealthStatus`, `GET /health /ready /version`, `cmd_monitor` CLI (`src/monitoring.rs`) 🟢
- [x] v5.8 — **Peer discovery**: `PeerStore` (~/.pkt/peers.txt), `DnsSeedResolver` (stdlib DNS), `PeerDiscovery::bootstrap/record/pex_query`, auto-discover khi `run_node` (`src/peer_discovery.rs`) 🟢
- [x] v5.9 — **Benchmark suite**: hash throughput, mining latency, TPS, merkle_std vs fast, UTXO scan vs index, mempool select; `BenchResult`/`BenchSuite` serde; `cargo run -- bench [target]` (`src/bench.rs`) 🟢

### Era 12 — Multi-threading & GPU Acceleration (v6.x)
- [x] v6.0 — **BLAKE3 Hash Engine**: thay SHA-256 cho PoW, `hash_version` field, benchmark BLAKE3 vs SHA-256 (`src/blake3_hash.rs`) 🔴
- [x] v6.1 — **CPU Multi-thread Miner**: `rayon` work-stealing, nonce splitting N threads, `AtomicBool` stop flag, default = `cores/3` (`src/cpu_miner.rs`) 🔴
- [x] v6.2 — **Thread-safe Chain**: `Arc<RwLock<Blockchain>>`, multiple readers + single writer, `ConcurrentChain` (`src/chain_concurrent.rs`) 🟡
- [x] v6.3 — **Parallel Block Validation**: `rayon::par_iter()` validate N blocks đồng thời khi sync, `ValidationResult` (`src/validator.rs`) 🟡
- [x] v6.4 — **GPU Miner Abstraction**: `GpuBackend { Software, OpenCL, Cuda }`, 1/3 compute units, software fallback (`src/gpu_miner.rs`) 🟡
- [x] v6.5 — **OpenCL BLAKE3 Kernel**: full 7-round BLAKE3 OCL C kernel, `opencl_mine()`, feature-gated `--features opencl`, CPU rayon fallback (`src/opencl_kernel.rs`) ✅
- [x] v6.6 — **CUDA BLAKE3 Kernel**: `__global__ blake3_mine`, `atomicCAS`, `CudaConfig`, feature-gated `--features cuda`, CPU rayon fallback (`src/cuda_kernel.rs`) ✅
- [x] v6.7 — **Mining Pool**: Stratum-like, `PoolServer/Client`, `WorkTemplate`, `Share`, per-miner difficulty retarget, proportional payout (`src/mining_pool.rs`) 🟡
- [x] v6.8 — **SIMD Hash**: BLAKE3 batch 4x AVX2 lanes, `cfg(target_feature = "avx2")`, scalar fallback, `mine_simd`, `benchmark_simd_vs_scalar` (`src/simd_hash.rs`) 🟢
- [x] v6.9 — **Hardware Auto-config**: `HardwareProfile`, detect cores/GPU, `CpuTier`, `MinerStrategy`, `OptimalMinerConfig::from_hardware()`, `cargo run -- hw-info` (`src/hw_config.rs`) 🟢

### Era 13 — Token Economy (v7.x) ✅ Hoàn thành
- [x] v7.0 — **Block Reward Engine**: `INITIAL_SUBSIDY`, halving schedule, `subsidy_at()`, `estimated_supply()` (`src/reward.rs`) ✅
- [x] v7.1 — **Fee Calculator**: `FeePolicy`, vsize P2PKH estimation, coinbase validation, fee_rate_from_tx (`src/fee_calculator.rs`) ✅
- [x] v7.2 — **Token Standard**: ERC-20-like `TokenRegistry`, mint/transfer/burn/approve/transfer_from (`src/token.rs`) ✅
- [x] v7.3 — **Token Transfer TX**: `TokenTx`, OP_RETURN encoding, BLAKE3 txid, `TokenTxBuilder`, extract_token_txs (`src/token_tx.rs`) ✅
- [x] v7.4 — **Smart Contract State**: `ContractStore`, `storage_root`, `state_root`, snapshot/restore (`src/contract_state.rs`) ✅
- [x] v7.5 — **EVM-lite Executor**: stack VM, `EvmLiteOp`, SLoad/SStore, Log(n), gas metering, out-of-gas (`src/evm_lite.rs`) ✅
- [x] v7.6 — **Contract Deployment**: CREATE/CREATE2 address, `AbiEncoder`, `ContractDeployer` (`src/contract_deploy.rs`) ✅
- [x] v7.7 — **DeFi AMM**: `LiquidityPool` x\*y=k, fee, add/remove liquidity, swap, `DEX` (`src/defi.rs`) ✅
- [x] v7.8 — **Staking & Delegation**: `Validator`, `Stake`, distribute_rewards, slash, APY (`src/staking.rs`) ✅
- [x] v7.9 — **Economic Model**: `EraParams`, `TokenEconomics`, `Simulator`, project N blocks (`src/economics.rs`) ✅

### Era 14 — PKTScan & API Integration (v8.x)
- [x] v8.0 — **PKTScan REST Backend**: `src/pktscan_api.rs` — axum server, CORS middleware, `/api/stats`, `/api/blocks`, `/api/block/:height`, `/api/txs`, `/api/tx/:txid`, `/api/address/:addr`, `/api/mempool`, `cargo run -- pktscan [port]`
- [x] v8.1 — **Live Feed (WebSocket)**: `src/pktscan_ws.rs` — `WsHub` broadcast channel, `WsEvent` (new_block/new_tx/stats), `/ws` endpoint, `spawn_poller()` 5s interval, merged vào pktscan_api router
- [x] v8.2 — **Address Page**: `src/address_index.rs` — `TxRecord`, `history_for_addr()`, `AddressIndex::build/history_of`, `output_owner_hex()` helper in utxo.rs; `/api/address/:addr` trả thêm `tx_history` + `tx_count`
- [x] v8.3 — **Search Engine**: `src/search_index.rs` — `SearchIndex::build/search`, `BlockRef/TxRef/AddrRef`, prefix-match hash/txid, exact height + address lookup; `/api/search?q=&limit=` endpoint
- [x] v8.4 — **Mempool Explorer**: `src/mempool_stats.rs` — `MempoolStats::compute`, `FeeBucket` (0-1/1-5/5-10/10-50/50+ sat/byte), `FeePercentiles` (p25/p50/p75/p90), suggested_fast/economy_fee; `/api/mempool` trả thêm fee_distribution + percentiles + sorted txs
- [x] v8.5 — **Mining Pool Dashboard**: `src/pool_api.rs` — `PoolDb = Arc<Mutex<PoolServer>>`, `pool_router()`, `GET /api/pool/stats` (blocks_found, hashrate, shares, miners count), `GET /api/pool/miners` (per-miner shares/hashrate/payout_est sorted desc); merged vào serve()
- [x] v8.6 — **Chain Analytics**: `src/chain_analytics.rs` — `Metric` enum (block_time/hashrate/fee_market/difficulty/tx_throughput), `DataPoint{height,timestamp,value,value2}`, `AnalyticsSeries`, `analytics(metric,chain,diff,window)`; `/api/analytics/:metric?window=N` endpoint, window clamped 2–500
- [x] v8.7 — **Export / Pagination**: `src/pagination.rs` — `paginate_blocks(from,limit)`, `paginate_txs(from,limit)`, `blocks_to_csv`, `tx_rows_to_csv`; `/api/blocks?from=<height>`, `/api/txs?from=<height>` cursor support; `GET /api/blocks.csv`, `GET /api/txs.csv` download endpoints
- [x] v8.8 — **Response Cache**: `src/response_cache.rs` — `ResponseCache{ttl_secs, entries}`, TTL expiry, ETag (BLAKE3 first 16 hex, quoted), `get/set/invalidate/evict_expired/live_count`; `api_cache_middleware` in pktscan_api (GET /api/* only, 304 on ETag match, X-Cache: HIT/MISS header)
- [x] v8.9 — **Static File Serving**: `GET /` → serve `index.html` từ working dir hoặc built-in fallback HTML với links; `serve()` updated với cache layer + full endpoint list in println; `CacheDb` type exported

### Era 15 — Read-Only APIs + Zero-Trust Foundation (v9.x)
_Mô hình: Zero-Trust + Read-Only-First. Mọi GET endpoint có rate limit + request ID + audit log từ v9.0._
_Nguyên tắc: GET = public (rate-limited) | POST/PUT/DELETE = chỉ mở sau khi auth layer hoàn chỉnh (Era 16)._
- [x] v9.0 — **ZT Middleware**: `src/zt_middleware.rs` — Zero-Trust layer áp dụng cho TẤT CẢ endpoints: Request-ID header, IP rate limiter, input validator (param length/chars), audit logger (append-only: timestamp/IP/method/path/status)
- [ ] v9.1 — **Token API** _(GET only)_: `src/token_api.rs` — `GET /api/tokens`, `/api/token/:id`, `/api/token/:id/holders` — expose `TokenRegistry`; tích hợp ZT middleware
- [ ] v9.2 — **Contract API** _(GET only)_: `src/contract_api.rs` — `GET /api/contracts`, `/api/contract/:addr`, `/api/contract/:addr/state` — expose `ContractStore` + `EvmLite`
- [ ] v9.3 — **Staking API** _(GET only)_: `src/staking_api.rs` — `GET /api/validators`, `/api/validator/:addr`, `/api/staking/:addr` — expose `StakingPool`
- [ ] v9.4 — **DeFi API** _(GET only)_: `src/defi_api.rs` — `GET /api/pools`, `/api/pool/:id`, `/api/pool/:id/price` — expose `LiquidityPool` / `DEX`
- [ ] v9.5 — **Tx Status + Labels**: sửa `pktscan_api.rs` — thêm `status: confirmed/pending`, `confirmations: N` vào `/api/tx/:txid`; `src/address_labels.rs` — `GET /api/labels`, `/api/label/:addr`
- [ ] v9.6 — **Tx Filter + CORS fix**: sửa `pktscan_api.rs` — filter `/api/txs?min_amount=&max_amount=&since=&until=`; CORS đổi từ `*` → allowlist origin (config-driven)
- [ ] v9.7 — **WS Subscriptions**: sửa `pktscan_ws.rs` — per-address `/ws?watch=<addr>`; WS token validation (signed query param)
- [ ] v9.8 — **OpenAPI Spec**: `src/openapi.rs` — `GET /api/openapi.json` — OpenAPI 3.0 spec tự động cho tất cả endpoints
- [ ] v9.9 — **SDK Generation**: `src/sdk_gen.rs` — `GET /api/sdk/js`, `/api/sdk/ts` — generated client SDK từ OpenAPI spec

### Era 16 — Auth Layer + Fix Core Logic (v10.x)
_Auth và Audit Log được kéo lên ĐẦU era — write endpoint chỉ mở sau khi v10.0–v10.1 hoàn chỉnh._
- [ ] v10.0 — **API Auth**: `src/api_auth.rs` — API key system: keygen (`cargo run -- apikey new`), hash lưu file `~/.pkt/api_keys.toml`, `X-API-Key` header validation, role: `read/write/admin`; tích hợp vào ZT middleware
- [ ] v10.1 — **Audit Log**: `src/audit_log.rs` — append-only structured log mọi request: timestamp/IP/method/path/status/api_key_id/latency_ms; rotate daily; `GET /api/admin/logs` (admin role only)
- [ ] v10.2 — **EVM Complete**: sửa `evm_lite.rs` — thêm đầy đủ opcodes: Add/Sub/Mul/Div/LT/GT/EQ/Jump/JumpI/CallValue/Caller/Return (hiện chỉ có Push/Stop)
- [ ] v10.3 — **Contract Persistence**: sửa `contract_state.rs` + `storage.rs` — lưu contract state vào RocksDB, restart không mất
- [ ] v10.4 — **Token ↔ Chain**: sửa `chain.rs` + `token_tx.rs` — validate token TX khi `add_block`, token balance thực sự thay đổi
- [ ] v10.5 — **Staking Rewards**: sửa `staking.rs` + `miner.rs` — distribute staking rewards trong coinbase TX mỗi block
- [ ] v10.6 — **Governance Persistence**: sửa `governance.rs` + `storage.rs` — persist proposals vào RocksDB, implement `execute()` thực sự
- [ ] v10.7 — **Oracle Verification**: sửa `oracle.rs` — verify ECDSA signature trong `OracleReport`, enforce staleness check
- [ ] v10.8 — **GraphQL** _(read-only)_: `src/graphql.rs` — endpoint `/graphql` — query linh hoạt; không có mutation cho đến Era 17
- [ ] v10.9 — **Webhook**: `src/webhook.rs` — outbound HTTP webhook: `new_block/new_tx/address_activity`; yêu cầu `write` role API key

### Era 17 — Write APIs + Production (v11.x)
_Write endpoint chỉ được thêm sau khi api_auth (v10.0) + audit_log (v10.1) hoàn chỉnh._
_Read path: `pktscan_api.rs` | Write path: `write_api.rs` — tách biệt kiến trúc._
- [ ] v11.0 — **Write API** _(POST /tx)_: `src/write_api.rs` — migrate `POST /tx` từ `api.rs` sang authenticated write path: validate input + verify signature + rate limit per key + audit log; yêu cầu `write` role
- [ ] v11.1 — **Token Write**: thêm vào `write_api.rs` — `POST /api/token/mint`, `POST /api/token/transfer`; yêu cầu `write` role + owner signature
- [ ] v11.2 — **Contract Write**: thêm vào `write_api.rs` — `POST /api/contract/deploy`, `POST /api/contract/call`; yêu cầu `write` role + gas estimate
- [ ] v11.3 — **Scam Registry**: `src/scam_registry.rs` — `GET /api/risk/:addr` (public read) + `POST /api/risk/:addr` (admin role only)
- [ ] v11.4 — **Address Watch**: `src/address_watch.rs` — watch địa chỉ → trigger webhook khi có TX mới; yêu cầu `write` role API key
- [ ] v11.5 — **Multi-chain**: `src/multi_chain.rs` — multi-chain read-only queries: PKT + ETH/BTC state qua IBC
- [ ] v11.6 — **CLI Token**: `src/cli_token.rs` — `cargo run -- token create/mint/transfer/balance`; dùng local API key
- [ ] v11.7 — **CLI Contract**: `src/cli_contract.rs` — `cargo run -- contract deploy/call/state`
- [ ] v11.8 — **CLI Staking**: `src/cli_staking.rs` — `cargo run -- staking delegate/claim/validators`
- [ ] v11.9 — **Deploy Config**: `src/deploy_config.rs` — Docker/systemd config generator + frontend embed (`include_bytes!`) + `cargo run -- deploy init`

### Era 20 — Post-Singularity (v12.x) — hardware-dependent
- [ ] v12.0–v12.5 — Quantum Random Beacon, Neural Wallet, Interplanetary Sync, Self-Evolving Contracts, AI Consensus, Singularity Chain

---

## 🧱 Quyết định thiết kế quan trọng

Chỉ ghi các quyết định **không hiển nhiên** hoặc có thể gây lỗi khi sửa code.

| Version | Quyết định |
|---------|-----------|
| v0.4 | UtxoSet key = `"tx_id:index"` (HashMap<String, TxOutput>) |
| v0.5 | `secp256k1 = 0.27`: dùng `Message::from_slice()`, KHÔNG có `from_digest_slice()` |
| v0.6 | TCP messages phân tách bằng `\n` — mỗi Message serialize thêm `\n` cuối |
| v0.8 | `pbkdf2 = { features = ["hmac"] }` — bắt buộc, thiếu là lỗi compile |
| v0.9 | `TxOutput.script_pubkey: Script`, `TxInput.script_sig: Script` |
| v1.3 | `secp256k1 = 0.27`: KHÔNG có `mul_assign` / `add_exp_assign` trên PublicKey — dùng `PublicKey::combine()` |
| v1.3 | Key path spend phải sign bằng `tweaked_sk`, KHÔNG phải `internal_sk` |
| v1.3 | `utxo.rs::owner_bytes_of()` hỗ trợ cả 20-byte (P2PKH) và 32-byte (P2TR) |
| v1.3 | `schnorr_sign()`: dùng `copy_from_slice` thay `try_into()` trên `&[u8;64]` |
| v1.5 | `mul_assign` không có trên `PublicKey` → dùng hash-based ECDH: `H(own_sk ‖ other_pk ‖ index)` |
| v1.5 | `ConfidentialInput` field là `utxo_tx_id`, không phải `tx_id` |
| v1.8 | `SchnorrZkProof.signature: Vec<u8>` (không dùng `[u8;64]` vì serde không impl cho fixed array >32) |
| v2.3 | `Sequencer::create_fraudulent_batch()` KHÔNG apply TXs — state diverges để demo fraud |
| v2.7 | Circuit breaker price cần fixed-point: `(price * 100_000_000.0) as u64` |
| v3.2 | `decapsulate(sk, ct)` không nhận `pk` param — `h_pk` store trong sk |
| v3.5 | `Relayer` pre-extract `chain_id` clone trước khi gọi `&mut self.chain_x` — tránh borrow conflict |
| v3.6 | VC `signing_bytes()` sort claims keys — canonical form để verify đúng dù HashMap order khác |
| v3.9 | `dilithium::Signature` có `c_hash`, `z: PolyVec { polys }`, KHÔNG có `.sig` field |
| v3.9 | `kyber::PolyVec` là newtype: `.0.len()` — khác với `dilithium::PolyVec`: `.polys.len()` |
| v4.0 | Wallet file: 2 dòng — `secret_key_hex\naddress` tại `~/.pkt/wallet.key` |
| v4.1 | `effective_bits = base_bits - floor(log2(ann_count + 1))` |
| v4.1 | `AnnouncementMiner.mine()` nonce bắt đầu từ `total_mined * 1_000_000` — tránh collision |
| v4.2 | RocksDB key schema: `block:{height:016x}`, `utxo:{txid}:{index}`, `meta:height` |
| v4.2 | `reset_storage()` dùng `DB::destroy()` — KHÔNG dùng `fs::remove_dir_all()` (bỏ sót WAL/manifest) |
| v4.3 | `apply_longest_chain()` + `can_append()` là `pub fn` standalone → testable không cần TCP |
| v4.3 | Storage tests dùng `static STORAGE_LOCK: Mutex<()>` — RocksDB không cho 2 threads cùng mở 1 DB |
| v4.4 | `api::Db = Arc<Mutex<Blockchain>>` — shared state giữa axum handlers |
| v4.4 | `POST /tx`: nhận `fee` từ `tx.fee`, tính `input_total = output_total + fee`, gọi `mempool.add()` |
| v4.4 | `GET /balance/:addr` gọi `utxo_set.balance_of(addr)` — hỗ trợ P2PKH (20-byte) và P2TR (32-byte) |
| v4.5 | `node_rpc()` dùng sync `TcpStream` + `BufReader::read_line()` — không cần tokio trong miner |
| v4.5 | `node_rpc()` set `read_timeout(5s)` — tránh hang vô hạn khi VPS không respond |
| v4.5 | Miner flow khi có node: sync chain → GetMempool → mine → submit NewBlock |
| v4.5 | `MinerConfig::with_node(addr)` builder pattern — không break existing callers |
| v4.5 | `Miner::new()` dùng `storage::load_or_new()` — load chain từ RocksDB thay vì reset mỗi lần |
| v4.5 | Miner gọi `storage::save_blockchain()` sau mỗi block — chain persist qua restart |
| v4.7 | `build_genesis()` encode genesis message vào `prev_hash` (SHA256 hex) — KHÔNG dùng `witness_root` (bị validate bởi `is_valid()`) |
| v4.7 | Testnet bootstrap peer: `seed.testnet.oceif.com:18333` (Cloudflare DNS only, A record 180.93.1.235) |
| v4.8 | `estimated_hashrate = 2^difficulty / avg_block_time_s` — rough estimate từ chain timestamps |
| v4.8 | `GetPeerCount` / `PeerCount` thêm vào `message.rs` — node xử lý trong `handle_message()` |
| v4.8 | `GET /metrics` trong `api.rs` gọi `metrics::collect(&bc, None)` — không query remote node từ REST |
| v5.0 | `UtxoIndex::apply_block`: nếu UTXO key đã tồn tại (coinbase collision), skip re-add vào addr_idx |
| v5.0 | `fast_merkle` dùng raw byte concat (Bitcoin standard) — KHÔNG tương thích với `Block::merkle` (hex-string concat) |
| v5.0 | `BlockCache::build_from_chain(chain)` → O(n) một lần; sau đó `contains_hash` / `height_of` là O(1) |
| v5.1 | `security.rs` là standalone utils — node.rs KHÔNG bị sửa, guard phải được wire vào Node khi cần |
| v5.1 | `RateLimiter::check()` increment trước rồi compare — count=1 sau lần gọi đầu |
| v5.1 | `BanList`: TTL = unix_now() + duration_secs; expired nếu unix_now() >= expiry |
| v5.1 | `InputValidator::validate_tx`: coinbase flag = Err — coinbase không được relay qua P2P |
| v5.2 | `PeerScore` latency: EMA = (new*3 + old*7) / 10 — integer-only, không dùng f64 |
| v5.2 | `should_ban(ip)` khi score ≤ `BAN_SCORE_THRESHOLD = -50`; cần đủ 3 InvalidBlock từ score=0 |
| v5.2 | `MessageDedup` dùng `VecDeque` + `HashSet` — FIFO eviction; O(1) insert/lookup |
| v5.3 | `CoinbaseGuard`: unknown tx_id → always mature (không restricted); known → `current >= mined + 100` |
| v5.3 | `TxReplayGuard`: bounded FIFO — evict oldest khi full; `confirm_block` skip coinbase (no replay risk) |
| v5.3 | `LockTimeValidator::is_final`: coinbase always final; `SEQUENCE_FINAL=0xFFFFFFFF` opt-out của locktime |
| v5.7 | `HealthStatus.version: String` (không dùng `&'static str`) — serde `from_str` cần owned type |
| v5.7 | `START_TIME: OnceLock<u64>` — process start được khởi tạo lần đầu gọi `health_check()` |
| v5.7 | `serve_health` dùng axum inline handlers (closure-like) thay route handlers riêng — tránh import conflict với api.rs |
| v6.0 | `pow_hash(data, hash_version)`: 0=SHA-256d, 1=BLAKE3 — SHA-256 giữ nguyên cho ECDSA/address |
| v6.0 | `Blake3Block.hash_version: u8` — phải include vào `header_bytes()` để hash khác SHA-256 blocks |
| v6.0 | Era 12 `count` trong `ERAS` phải bằng `VERSIONS.len()` — `test_full_stack_registry` enforce |
| v6.1 | `mine_parallel` dùng `find_map_any` — rayon dừng tất cả threads khi 1 thread trả `Some(...)` |
| v6.1 | Prefix `"index|ts|txid_root|witness_root|prev_hash|"` precomputed — chỉ nonce thay đổi mỗi iter |
| v6.1 | `rayon::ThreadPoolBuilder::new().num_threads(n)` — tách pool riêng, không dùng global pool |
| v6.2 | `Mempool` field là `entries` không phải `transactions` — xem `src/mempool.rs` |
| v6.2 | `ConcurrentChain` impl `Clone` (derive) — clone = `Arc::clone`, không copy dữ liệu |
| v6.2 | `with_read/write(closure)` escape hatch — dùng khi API chuẩn không đủ |

---

## ⚠️ Known Gaps (còn thiếu)

| Mức | Thiếu | Trạng thái |
|-----|-------|-----------|
| 🔴 | Persistent storage | ✅ v4.2 (RocksDB) |
| 🔴 | Chain sync / longest-chain rule | ✅ v4.3 |
| 🔴 | Block validation khi nhận từ peer | ✅ v4.3 |
| 🔴 | Mempool broadcast | ✅ v4.3 |
| 🟡 | REST/RPC API | ✅ v4.4 (axum 0.7) |
| 🟡 | Miner ↔ Node kết nối | ✅ v4.5 |
| 🟡 | Peer discovery tự động | ✅ v5.8 (PeerStore + DNS + PEX) |
| 🟡 | Coinbase maturity (100 blocks) | ✅ v5.3 |
| 🟢 | Block Explorer | ✅ v4.6 |
| 🟢 | Metrics | ✅ v4.8 |

---

## 📦 Dependencies

```toml
# Era 1–11 (hiện tại)
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
rocksdb = "0.21"          # v4.2: persistent storage backend
axum = "0.7"              # v4.4: REST API
tracing = "0.1"           # v5.7: structured logging
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
proptest = { version = "1.4", optional = true }  # v5.6: fuzz

# Era 12
blake3 = "1.5"            # v6.0: BLAKE3 hash (pure Rust)
rayon = "1.10"            # v6.1: CPU parallel mining
num_cpus = "1.16"         # v6.1: detect core count
ocl = { version = "0.19", optional = true }     # v6.5: OpenCL GPU mining
cust = { version = "0.3", optional = true }     # v6.6: CUDA GPU mining
```

---

## 🐛 Lỗi đáng nhớ

| Version | Lỗi | Fix |
|---------|-----|-----|
| v0.5 | `Message::from_digest_slice()` not found | `Message::from_slice()` |
| v0.8 | `pbkdf2_hmac` gated | `features = ["hmac"]` |
| v1.3 | `add_exp_assign` not found trên PublicKey | `PublicKey::combine()` |
| v1.3 | Key path verify fail | sign bằng `tweaked_sk` không phải `internal_sk` |
| v1.3 | P2TR UTXO balance = 0 | `owner_bytes_of()` hỗ trợ 32-byte key |
| v1.5 | `mul_assign` E0599 trên PublicKey | hash-based ECDH |
| v4.3 | RocksDB lock contention trong parallel tests | `static STORAGE_LOCK: Mutex<()>` |
| v4.5 | Miner hang vô hạn khi VPS không respond | `stream.set_read_timeout(Some(Duration::from_secs(5)))` |
| v4.7 | `build_genesis()` fail `is_valid()` | encode genesis msg vào `prev_hash`, không phải `witness_root` |
| v5.0 | `UtxoIndex` balance double-count | kiểm tra `utxos.contains_key(&k)` trước khi push addr_idx |

---

## 🔖 Checkpoint

```
Version:   v6.3 ✅
Tests:     179/179 pass
Warnings:  0
Errors:    0

Stack đã có:
  v0.1–v3.9  40 modules, all eras complete
  v4.0–v4.8  PKT Native Chain
  v5.0       Performance: UtxoIndex O(1), BlockCache O(1), fast_merkle
  v5.1       Security: RateLimiter, BanList, PeerGuard, InputValidator
  v5.2       P2P: PeerRegistry, ScoreEvent EMA latency, MessageDedup FIFO bounded
  v5.3       Maturity: CoinbaseGuard (100-block), TxReplayGuard, LockTimeValidator
  v5.4       Fee market (src/fee_market.rs):
               FeeEstimator: sliding window 20 blocks, fast/medium/slow estimate
               RBF: RBF_MIN_BUMP=1.10, find_rbf_conflict, is_valid_rbf_bump, try_rbf_replace
  v5.5       Storage v2 (src/wal.rs):
               atomic_save: WriteBatch all keys in one db.write()
               WAL epoch: odd=write-in-progress, even=committed
               check_and_recover: rebuild UTXO from chain if inconsistent
  v5.6       Fuzz + proptest (src/fuzz.rs):
               FuzzTarget: message_deserialize, block_hash, block_serialization
               corpus: 12 entries (valid + malformed + unicode + 100KB)
               proptest: hash_64_hex, no_panic, fee_ordering, rbf_consistency
  v5.7       Monitoring (src/monitoring.rs):
               init_tracing(level): tracing-subscriber, EnvFilter, RUST_LOG support
               Typed log helpers: log_block_mined, log_tx_received, log_peer_event,
                 log_sync_event, log_rbf_replace, log_error
               HealthStatus: version, uptime_secs, height, difficulty, utxo_count,
                 mempool_depth, is_synced, fee_fast/medium/slow, timestamp
               serve_health(port): GET /health /ready /version (axum, standalone)
               cmd_monitor: cargo run -- monitor [port] (default 3001)
  v5.8       Peer discovery (src/peer_discovery.rs):
               PeerStore: load/save/add(idempotent)/remove, ~/.pkt/peers.txt
               DnsSeedResolver: ToSocketAddrs (stdlib), multi-IP round-robin DNS
               PeerDiscovery: bootstrap() = store+dns deduped, record_peer, remove_peer
               pex_query(addr): TCP GetPeers → Peers → Vec<String>
               deep_bootstrap(): initial + PEX query each peer
               run_node() tự động discover peers khi không chỉ định --peer
  v5.9       Benchmark suite (src/bench.rs):
               BenchResult: iterations, avg/min/max ns, ops_per_sec, serde
               BenchSuite: Vec<BenchResult> + timestamp, print() table
               bench_hash_throughput, bench_block_mining, bench_tps,
               bench_merkle_compare, bench_utxo_lookup, bench_mempool_select
               CLI: cargo run -- bench [hash|mining|tps|merkle|utxo|mempool|all]

  v6.0       BLAKE3 Hash Engine (src/blake3_hash.rs):
               blake3_hash(data): BLAKE3 single-pass
               sha256d(data): double-SHA256 (legacy)
               pow_hash(data, version): route by hash_version (0=SHA256d, 1=BLAKE3)
               Blake3Block: index, timestamp, txid_root, witness_root, prev_hash,
                 nonce, hash_version, hash — mine/is_valid/calculate_hash
               HashComparison: benchmark_compare(iters) → speedup_x
               CLI: cargo run -- blake3

  v6.1       CPU Multi-thread Miner (src/cpu_miner.rs):
               default_threads(): max(1, logical_cores / 3)
               mine_parallel(block, threads, difficulty) → MineResult
               rayon find_map_any: nonce space split N chunks, AtomicBool stop flag
               CpuMinerConfig: with_threads/difficulty/max_blocks builder
               CpuMiner: mine_block(&Blake3Block) → MineResult + stats
               CLI: cargo run -- cpumine [addr] [diff] [blocks]

  v6.2       Thread-safe Chain (src/chain_concurrent.rs):
               ConcurrentChain: Arc<RwLock<Blockchain>>, #[derive(Clone)]
               Read: height/chain_len/tip_hash/difficulty/block_hash/is_valid/balance_of/mempool_size
               Write: add_block(txs, addr)/mine_and_add(addr)
               Escape hatch: with_read/with_write closures
               clone_handle(): Arc clone, same chain, ref_count()

  v6.3       Parallel Block Validation (src/validator.rs):
               ValidationError: HashMismatch/DifficultyNotMet/WitnessRootMismatch/
                 MissingCoinbase/InvalidTransaction/BrokenChainLink
               ValidationResult { block_index, is_valid, errors }
               validate_block(block, diff): single block individual check
               validate_blocks_individual_parallel(blocks, diff): par_iter
               validate_chain_links_parallel(blocks): prev_hash checks par
               validate_blocks_parallel(blocks, diff): rayon::join both passes
               ParallelValidator { difficulty }: validate_one/validate_many

  v6.4       GPU Miner Abstraction (src/gpu_miner.rs):
               GpuBackend { Software, OpenCL, Cuda } — is_available(), from_str()
               GpuDeviceInfo { backend, name, compute_units, memory_mb, available }
               detect_devices() -> Vec<GpuDeviceInfo>  (Software=real, OCL/CUDA=stub)
               default_compute_units() = max(1, cores/3)
               GpuMinerConfig builder: with_backend/difficulty/max_blocks/compute_units
               GpuMineResult { nonce, hash, hashes_tried, elapsed_ms, backend_used }
               GpuMiner::mine_block(&Blake3Block) -> GpuMineResult (fallback to Software)
               CLI: cargo run -- gpumine [addr] [diff] [blocks] [software|opencl|cuda]

  v6.5       OpenCL BLAKE3 Kernel (src/opencl_kernel.rs):
               BLAKE3_OCL_KERNEL: full 7-round compress, G mixing, MSG_SCHEDULE
               OpenClConfig { compute_units, work_group_size, batch_size } + Default
               opencl_available() -> bool  (const fn, feature-gated)
               opencl_mine(block, difficulty, config) -> GpuMineResult
               _mine_ocl_impl() — OCL setup: Platform/Device/Context/Queue/Program/Kernel
               _mine_cpu_fallback() — rayon find_map_any, same as cpu_miner
               GpuBackend::OpenCL wired via gpu_miner::mine_opencl()
               Build with GPU: cargo build --features opencl
               CLI: cargo run --features opencl -- gpumine [addr] [diff] [n] opencl

  v6.6       CUDA BLAKE3 Kernel (src/cuda_kernel.rs):
               BLAKE3_CUDA_SRC: full 7-round compress, G mixing, MSG_SCHEDULE (CUDA C)
               CudaConfig { block_size, grid_size, batch_size } + Default + new()
               cuda_available() -> bool  (const fn, feature-gated)
               list_cuda_devices() -> Vec<CudaDeviceInfo>  (feature-gated, cust)
               cuda_mine(block, difficulty, config) -> GpuMineResult
               _mine_cuda_impl() — cust: init/Device/Context/Stream/Module/launch!
               PTX: compile blake3_mine.cu with nvcc → target/blake3_mine.ptx
               _mine_cpu_fallback() — rayon find_map_any, same as opencl_kernel
               GpuBackend::Cuda wired via gpu_miner::mine_cuda()
               Build with GPU: cargo build --features cuda
               CLI: cargo run --features cuda -- gpumine [addr] [diff] [n] cuda
               cust = { version = "0.3", optional = true }  in Cargo.toml

Next: v6.7 Mining Pool
```
