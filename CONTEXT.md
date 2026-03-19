# 🦀 Blockchain Rust — CONTEXT

**Version hiện tại: v14.5 ✅ — 1421 tests pass, 0 errors, 0 warnings**

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
- [x] v9.0 — **ZT Middleware**: `src/zt_middleware.rs` — Zero-Trust layer áp dụng cho TẤT CẢ endpoints: Request-ID header, IP rate limiter (100 req/60s per IP), input validator (path ≤256, query ≤512, no null byte, no `../`), audit logger append-only `~/.pkt/audit.log`; tích hợp vào `pktscan_api::serve()` qua `from_fn_with_state`
- [x] v9.0.1 — **Ed25519 HD Wallet** _(SLIP-0010)_: `src/ed25519_wallet.rs` — BIP39 mnemonic → PBKDF2-SHA512 seed → SLIP-0010 master key → hardened derivation (tất cả path components phải hardened) → Ed25519 keypair → PKT address (base58, version=0x50, SHA256+checksum); `trait Signer` + `HotSigner` (ZeroizeOnDrop) + `ColdSigner` (callback) + `MockSigner`; `WalletTx` hash/sign/verify; `HdWallet::derive_addresses(chain_id, account, count)`; deps: `ed25519-dalek=2`, `zeroize=1`, `rand_core=0.6`; **chưa có CLI/API** — dùng qua Rust API, CLI planned Era 17 v11.6
- [x] v9.1 — **Token API** _(GET only)_: `src/token_api.rs` — `GET /api/tokens`, `/api/token/:id`, `/api/token/:id/holders`, `/api/token/:id/balance/:addr` — expose `TokenRegistry`; tích hợp ZT middleware — **21 tests**
- [x] v9.2 — **Contract API** _(GET only)_: `src/contract_api.rs` — `GET /api/contracts`, `/api/contract/:addr`, `/api/contract/:addr/state`, `/api/contract/:addr/state/:key` — expose `ContractRegistry` (`WasmRuntime`); tích hợp ZT middleware — **23 tests**
- [x] v9.3 — **Staking API** _(GET only)_: `src/staking_api.rs` — `GET /api/staking/stats`, `/api/staking/validators`, `/api/staking/validator/:addr`, `/api/staking/delegator/:addr` — expose `StakingPool`; tích hợp ZT middleware — **25 tests**
- [x] v9.4 — **DeFi API** _(GET only)_: `src/defi_api.rs` — `GET /api/defi/feeds`, `/api/defi/feed/:id`, `/api/defi/feed/:id/history`, `/api/defi/loans`, `/api/defi/loans/liquidatable` — expose `OracleRegistry` + `LendingProtocol`; tích hợp ZT middleware — **27 tests**
- [x] v9.5 — **Tx Status + Labels**: sửa `pktscan_api.rs` — thêm `status: confirmed/pending`, `confirmations: N` vào `/api/tx/:txid` + mempool lookup; `src/address_labels.rs` — `GET /api/labels`, `/api/label/:addr`, `/api/labels/category/:cat` — **22 tests**
- [x] v9.6 — **Tx Filter + CORS Allowlist**: sửa `pktscan_api.rs` — `TxFilterParams` với `min_amount/max_amount/since/until`, `total_filtered` trong response; `CorsConfig` allowlist thay `*`, `is_allowed()` check per-request Origin header; CORS layer move từ `router()` vào `serve()` qua closure — **20 tests** (+10 mới)
- [x] v9.7 — **WS Subscriptions**: sửa `pktscan_ws.rs` — `WsConfig` (secret + `validate_token`), `WsQuery` (watch/token), `WsState` (hub+config); `WsEvent::NewTx` thêm `addresses: Vec<String>`; `event_touches_addr()` filter; `/ws?watch=<addr>&token=<tok>` → 401 nếu auth fail; `spawn_poller` extract output addresses qua `output_owner_hex` — **25 tests** (+14 mới)
- [x] v9.8 — **OpenAPI Spec**: `src/openapi.rs` — `GET /api/openapi.json` — OpenAPI 3.0.3 JSON, `build_spec()` + `build_paths()` (Map insertion tránh recursion limit), `build_schemas()`, helpers `qparam/pparam/ok_json/err_resp`; 35 paths documented; tích hợp `openapi_router()` vào `serve()` — **29 tests**
- [x] v9.9 — **SDK Generation**: `src/sdk_gen.rs` — `GET /api/sdk/js`, `/api/sdk/ts` — generated client SDK từ OpenAPI spec — **30 tests**

### Era 16 — Auth Layer + Fix Core Logic (v10.x)
_Auth và Audit Log được kéo lên ĐẦU era — write endpoint chỉ mở sau khi v10.0–v10.1 hoàn chỉnh._
- [x] v10.0 — **API Auth**: `src/api_auth.rs` — API key system: keygen (`cargo run -- apikey new`), hash lưu file `~/.pkt/api_keys.json`, `X-API-Key` header validation, role: `read/write/admin`; `auth_middleware` tích hợp vào pktscan_api::serve() — **28 tests**
- [x] v10.1 — **Audit Log**: `src/audit_log.rs` — append-only structured log mọi request: timestamp/IP/method/path/status/api_key_id/latency_ms; rotate daily (`~/.pkt/audit/audit-YYYY-MM-DD.log`); `GET /api/admin/logs?date=&limit=&offset=` (admin role only) — **19 tests**
- [x] v10.2 — **EVM Complete**: sửa `evm_lite.rs` — thêm `CallValue`, `Caller`, `JumpDest`, `IsZero`, `GasLeft`; refactor `execute()` → PC-based loop (Jump/JumpIf thật sự hoạt động); `test_loop_with_jump` verify counter loop — **+13 tests** (22 total)
- [x] v10.3 — **Contract Persistence**: sửa `storage.rs` — `save_contract_store()` + `load_contract_store()`; key schema `contract:{address}`; `ContractStateData` helper (Vec<hex_key,hex_val> tránh [u8;32] JSON key issue); dùng `STORAGE_LOCK` trong tests — **+5 tests**
- [x] v10.4 — **Token ↔ Chain**: sửa `chain.rs` + `token_tx.rs` — `Blockchain.token_registry: TokenRegistry`; `apply_token_txs()` gọi trong `mine_block_to_hash()` + `add_block()`; `validate_token_tx()` trong token_tx.rs — **+9 tests**
- [x] v10.5 — **Staking Rewards**: sửa `staking.rs` + `miner.rs` + `chain.rs` — `collect_block_rewards()` distribute + claim per block; staking outputs thêm vào coinbase TX; `Blockchain.staking_pool` field; `Miner.staking_pool` field — **+5 tests** (829 total)
- [x] v10.6 — **Governance Persistence**: sửa `governance.rs` + `storage.rs` — thêm serde cho `ProposalState/Action/Vote/Proposal`; `GovernanceSnapshot` + `snapshot()`/`from_snapshot()`; `save_governor()`/`load_governor()` với key `governance:snapshot` — **+5 tests** (834 total)
- [x] v10.7 — **Oracle Verification**: sửa `oracle.rs` — `OracleReport::signed()` dùng secp256k1 ECDSA thật; `verify()` unified (ECDSA nếu pubkey_hex set, blake3 legacy nếu không); `OracleNode::new(stake)` tự generate keypair; `submit(report, current_time)` enforce staleness; sửa `defi_api.rs` — **+7 tests** (841 total)
- [x] v10.8 — **GraphQL** _(read-only)_: `src/graphql.rs` — endpoint `GET/POST /graphql`; `async-graphql = "7"` (không dùng axum-crate để tránh conflict); handler thủ công qua `axum::extract::Json`; schema: `chainInfo`, `block`, `blocks`, `balance`, `mempoolCount`, `mempoolTxs`; `EmptyMutation`; merge vào `pktscan_api::serve()` — **+13 tests** (854 total)
- [x] v10.9 — **Webhook**: `src/webhook.rs` — `WebhookRegistry` register/remove/matching; `sign_payload()` HMAC-SHA256; `deliver()` async HTTP POST với `X-PKT-Signature`; `broadcast()` fire-and-forget; REST: `POST/GET /api/webhooks`, `DELETE /api/webhooks/:id` (write role); `reqwest = "0.12"` — **+18 tests** (872 total)

### Era 17 — Write APIs + Production (v11.x)
_Write endpoint chỉ được thêm sau khi api_auth (v10.0) + audit_log (v10.1) hoàn chỉnh._
_Read path: `pktscan_api.rs` | Write path: `write_api.rs` — tách biệt kiến trúc._
- [x] v11.0 — **Write API** _(POST /tx)_: `src/write_api.rs` — `WriteRateLimiter` (per-key, 60 req/min); `validate_tx_basic()` (no coinbase, fee>0, non-empty I/O, valid tx_id); `verify_tx_scripts()` public wrapper trong chain.rs; `POST /api/write/tx` (write role required → rate check → validate → script verify → mempool); `WriteState{chain,rate}` — **+15 tests** (887 total)
- [x] v11.1 — **Token Write**: sửa `write_api.rs` — `MintRequest`/`TransferRequest` + `mint_payload()`/`transfer_payload()` signing domains; `pubkey_hex_to_address()` = RIPEMD160(blake3(pk)); `verify_sig()` ECDSA blake3; `POST /api/write/token/mint` (write role + owner sig → `mint_as_owner()`); `POST /api/write/token/transfer` (write role + sender sig → `transfer()`); webhook.rs unused import fix — **+14 tests** (901 total)
- [x] v11.2 — **Contract Write**: sửa `write_api.rs` — `DeployRequest`/`CallRequest` + `deploy_payload()`/`call_payload()` signing domains; `template_to_module()` map "counter"/"token"/"voting" → WasmModule; `estimate_gas_for()` tính tổng `WasmInstr::gas_cost()` (no execution); `POST /api/write/contract/deploy` (write role + ECDSA); `POST /api/write/contract/call` (dry_run → gas estimate; live → commit); `WriteState.contract: ContractDb`; pktscan_api.rs clone contract_db — **+14 tests** (915 total)
- [x] v11.3 — **Scam Registry**: `src/scam_registry.rs` — `RiskLevel` (unknown/safe/low/medium/high/critical, score 0–100); `RiskCategory` (scam/phishing/mixer/exchange/sanctions/ransomware); `ScamRegistry` upsert/get/remove/by_min_level/by_category; `validate_address()` 32–66 hex; `GET /api/risk/:addr` (public → entry or {"level":"unknown"}); `POST /api/risk/:addr` (admin → upsert, 201 if new, 200 if update); `DELETE /api/risk/:addr` (admin); reported_at giữ nguyên khi update — **+22 tests** (937 total)
- [x] v11.4 — **Address Watch**: `src/address_watch.rs` — `WatchEntry` (id=blake3[8], address, callback_url, api_key_id, last_seen_height); `WatchRegistry` add/remove(owner-only)/by_key/snapshot/update_height; giới hạn 20/key, 500 total, no duplicate; `check_new_activity()` filter height > last_seen; `build_callback_payload()` + `deliver_callback()` reqwest 10s; `spawn_watcher()` tick 30s; `POST /api/watch`, `GET /api/watch`, `DELETE /api/watch/:id` (write role) — **+16 tests** (953 total)
- [x] v11.5 — **Multi-chain**: `src/multi_chain.rs` — `ChainType` (PKT/ETH/BTC/Custom) + `ChainMeta`; `MultiChainRegistry` register/get/open_connections/open_channels/pending_packets; `default_registry()` pre-seed PKT+ETH+BTC với IBC relayer handshake (connection+channel PKT↔ETH); `ibc.rs` thêm `Relayer::into_chains()`; GET `/api/chains`, `/api/chains/:id`, `/api/chains/:id/clients`, `/api/chains/:id/client/:cid`, `/api/chains/:id/connections`, `/api/chains/:id/channels`, `/api/chains/:id/packets/pending` — **+19 tests** (972 total)
- [x] v11.6 — **CLI Token**: `src/cli_token.rs` — `cmd_token()` dispatch: create/list/info/mint/transfer/balance; `Token`+`TokenAccount`+`TokenRegistrySnapshot` thêm serde; `TokenRegistry::snapshot()`+`from_snapshot()` tránh tuple-key JSON; `storage::save_token_registry()`+`load_token_registry()` RocksDB key `token:registry`; curl hints REST API; `cargo run -- token ...` — **+15 tests** (987 total)
- [x] v11.7 — **CLI Contract**: `src/cli_contract.rs` — `cmd_contract()` dispatch: deploy/list/info/call/state/estimate; `ContractInstanceSnapshot`+`ContractRegistrySnapshot` serde trong `smart_contract.rs`; `ContractRegistry::snapshot(tmap)`+`from_snapshot()` rebuild WasmModule từ template name; unknown template skipped khi load; `storage::save_contract_registry()`+`load_contract_registry()` RocksDB key `contract_registry:snapshot`; `call` hỗ trợ args + `--gas N` flag; curl hints REST API; `cargo run -- contract ...` — **+16 tests** (1003 total)
- [x] v11.8 — **CLI Staking**: `src/cli_staking.rs` — `cmd_staking()` dispatch: validators/register/delegate/undelegate/rewards/claim/info/slash; `Validator`+`Stake`+`StakingPool` thêm serde; `storage::save_staking_pool()`+`load_staking_pool()` RocksDB key `staking:pool`; `delegate()` 5 args (lock_blocks+current_height), `undelegate()` 3 args returns `Result<u64>`; curl hints REST API; `cargo run -- staking ...` — **+18 tests** (1021 total)
- [x] v11.9 — **Deploy Config**: `src/deploy_config.rs` — `DeployConfig` (app_name/network/ports/data_dir/log_level); `include_bytes!("../index.html")` frontend embed; generators: `dockerfile()`/`docker_compose()`/`systemd_service()`/`env_file()`/`nginx_conf()`; `DeployConfig::for_network()` tự chỉnh ports; JSON roundtrip serde; `cmd_deploy()` dispatch: init/dockerfile/compose/systemd/env/nginx/frontend/config; `deploy init [net]` viết 7 file vào current dir — **+21 tests** (1042 total)

### Era 18 — HD Wallet & UX (v12.x)
- [x] v12.0 — **HD Wallet CLI**: mở rộng `wallet_cli.rs` — `wallet new` dùng `HdWallet::new()` (BIP39/44) thay keypair thô; file format 3 dòng: `mnemonic\nsk_hex\naddress`; `wallet show` hiển thị 12 từ seed phrase có số thứ tự; `wallet restore <word1>...<word12>` khôi phục ví từ seed phrase; `load_wallet_full()` tự detect format v4.0 (2 dòng) vs v12.0 (3 dòng, dòng 1 chứa spaces); backward compat với wallet cũ — **+13 tests** (1055 total)

### Era 19 — PKT Core (v13.x)
- [x] v13.0 — **PacketCrypt PoW chuẩn**: `src/pkt_core.rs` — `CompactTarget` (nBits như Bitcoin, exponent+mantissa, `meets_target`, `with_ann_count`); `PKT_ANN_EXPIRY=3`/`PKT_SEED_DEPTH=3`/`PKT_ANN_ITEM_COUNT=4096`; `PktAnn` (version+content_hash+parent_block_height+seed_hash+nonce+ann_target, `header_hash` đúng layout, `is_valid_for_block` expiry check); `compute_content_hash` memory-hard items; `PktAnnMiner::mine`; `PktBlockHeader` (`ann_merkle`/`ann_count`/`ann_target`/`effective_target`); `PktBlockMiner::add_ann` (verify+expiry+target), `mine`; `PktChain` với `seed_hash_for(height)` — **+25 tests** (1080 total)
- [x] v13.1 — **Network Steward**: `src/pkt_steward.rs` — `STEWARD_REWARD_PCT=20%`/`VOTE_WINDOW=2048`/`BURN_ADDRESS`; `StewardVote` (for_candidate/abstain); `StewardState` (current/elected_at/balance/burn); `VoteRegistry` sliding window, `tally()`/`winner()`(>50% threshold)/`vote_fraction()`; `StewardEngine::process_block()` → (steward_amt, miner_amt, Option<new_steward>); `burn()` validates balance; `voting_stats()` snapshot — **+21 tests** (1101 total)
- [x] v13.2 — **Bandwidth Incentive Layer**: `src/pkt_bandwidth.rs` — `ANNOUNCER_REWARD_PCT=50%`/`EPOCH_BLOCKS=256`/`MIN_PROOF_BYTES=512`/`MAX_ROUTES_PER_NODE=32`; `BandwidthProof` (BLAKE3 hash, verify); `RouteAnnouncement` (expiry window 128 blocks); `NodeStats` (apply_proof, throughput_mbps); `AnnouncerPool` (fund, proportional epoch distribute); `BandwidthLedger` (process_block → fund pool + collect proofs; distribute at epoch boundary); `LedgerSummary` — **+22 tests** (1123 total)
- [x] v13.3 — **PKT Address Format**: `src/pkt_address.rs` — bech32/bech32m tự implement (không cần crate ngoài); HRP: "pkt"(mainnet)/"tpkt"(testnet)/"rpkt"(regtest); `encode_p2wpkh`/`encode_p2wsh`/`encode_p2tr`; `decode_address` → `PktAddress{network,addr_type,witness_program}`; `pubkey_to_pkt_address` (RIPEMD160(SHA256(pk))); `hash160()`/`taproot_key()` accessors; `convertbits` 8↔5; polymod checksum BIP-173/BIP-350 — **+23 tests** (1146 total)
- [x] v13.4 — **PKT Testnet Genesis Params**: `src/pkt_genesis.rs` — `PAKLETS_PER_PKT=2^30`/`INITIAL_BLOCK_REWARD=4096 PKT`/`HALVING_INTERVAL=2^20`/`MAX_SUPPLY=6B PKT`; magic bytes + ports (mainnet/testnet/regtest); bootstrap peers; `PktNetworkParams` (mainnet/testnet/regtest với halving_interval=150 + block_time=1s cho regtest); `block_reward_at(height)`/`total_issued_to(height)`/`next_halving_height()`; `PktGenesisBlock::build(params)`/`validate()` — **+35 tests** (1181 total)

### Era 21 — UX & Frontend (v14.x)
- [x] v14.0 — **Terminal UI (TUI)**: `src/tui_dashboard.rs` — `ratatui = "0.26"` + `crossterm = "0.27"`; `DashboardState` (block_height/hashrate/peers/mempool/uptime/logs); `SyncStatus` (Syncing{current,target}/Synced/NoPeers + progress()/color()/label()); `hashrate_display` (kH/s→MH/s→GH/s), `mempool_bytes_display`, `uptime_display`; `build_layout/build_body_columns/build_left_rows`; render: header/block_info/mining/mempool/sync_bar(Gauge)/peers(List)/log(Paragraph); `parse_event` → `DashboardEvent`; `cmd_dashboard()` live loop; TestBackend cho tests không cần terminal — **+32 tests** (1213 total)
- [x] v14.1 — **Wallet TUI**: `src/tui_wallet.rs` — `WalletTab` (Balance/Send/Receive/History + next/prev wrap); `SendStep` state machine (Input→Confirm→Done/Cancelled); `SendError` (EmptyRecipient/InvalidRecipient/EmptyAmount/InvalidAmount/ZeroAmount/InsufficientFunds); `WalletTuiState` (balance/address/send form/history/scroll); `send_proceed()` validate → Confirm; `send_confirm()` deduct + history; render: tabs bar/balance/send form/confirm popup(Clear overlay)/receive/history list; `handle_wallet_event` dispatch; TestBackend smoke tests — **+46 tests** (1259 total)
- [x] v14.2 — **Web Frontend**: `src/web_frontend.rs` + `frontend/app.js` + `frontend/style.css` — nhúng compile-time bằng `include_bytes!` (không cần crate thêm); `EmbeddedAsset{path,content_type,bytes}`; `ASSETS` registry 3 files (index.html/app.js/style.css); `find_asset(path)`/`total_size()`; `mime_for_ext(ext)`/`file_ext(name)`; `FrontendManifest::build()`; `frontend_router()` axum Router (GET / /index.html /static/app.js /static/style.css + fallback 404); `app.js`: fetch /api/status + /api/chain, balance lookup, search, theme toggle, 30s auto-refresh — **+36 tests** (1295 total)
- [x] v14.3 — **QR Code**: `src/qr_code.rs` — `qrcode = "0.14"` (pure Rust); `QrLevel` (Low/Medium/High → EcLevel); `render_compact()` half-block Unicode (Dense1x2); `render_full()` `██`/`  ` full-block (tương thích mọi terminal); `payment_uri(addr, amount, label)` → BIP21 `pkt:ADDR?amount=X&label=Y`; `percent_encode` (space/&/=/?)'; `address_qr()`/`payment_qr()` → `QrResult{data,qr_str,width,uri}`; `cmd_qr(args)` CLI; QR width = 17+4×N verified — **+29 tests** (1324 total)
- [x] v14.4 — **Shell Completions**: `src/shell_completions.rs` — `Shell` enum (Bash/Zsh/Fish); `generate_bash()` (compgen/COMP_WORDS), `generate_zsh()` (#compdef + _arguments), `generate_fish()` (complete -c pkt -f); `TOP_COMMANDS` 22 lệnh, sub-completions cho wallet/explorer/genesis/bench/gpu/token/contract/staking/deploy/apikey; `install_hint()` per-shell; `cmd_completions(args)` CLI; `cargo run -- completions bash|zsh|fish` — **+53 tests** (1377 total)
- [x] v14.5 — **Web Charts**: `src/web_charts.rs` — sparkline engine (▁▂▃▄▅▆▇█, resample, detect_trend); `Trend` (Up/Down/Flat + symbol/label); `MetricChart::build()` + `print()`; `ChartDashboard`; `mock_data(n, base, amp, phase)`; `format_hashrate`; `charts_router()` → `GET /static/charts.js`; nhúng `frontend/charts.js` compile-time; `cmd_charts(args)` CLI; merge vào `pktscan_api::serve()`; `frontend/charts.js`: fetch `/api/analytics/:metric`, render ASCII sparkline + Chart.js CDN line charts, auto-refresh 30s — **+44 tests** (1421 total)
- [ ] v14.6 — **Block Detail Page [UI]**: `/block/:height` — full block info, danh sách TXs, merkle path; `/tx/:txid` — inputs/outputs/fee/confirmations; `app.js` hash-router (`#block/N`, `#tx/id`)
- [ ] v14.7 — **Address Detail Page [UI]**: `/address/:addr` — balance + UTXO list + tx history; phân biệt incoming/outgoing; link đến block/tx
- [ ] v14.8 — **WebSocket Live Feed [UI]**: `app.js` kết nối `/ws` — nhận `NewBlock`/`NewTx` events real-time; cập nhật stats không cần refresh; toast notification khi block mới

### Era 23 — Developer Experience (v16.x)

_Mục tiêu: giảm friction khi dev/test/debug local._

- [ ] v16.0 — **Devnet One-Command [DX]**: `cargo run -- devnet` → spawn node + miner + API trong một process; auto-mine 1 block/giây; in địa chỉ + balance; `--blocks N` tự dừng sau N blocks
- [ ] v16.1 — **Dev Docs Generator [DX]**: `cargo run -- docs` → sinh `docs/api.md` từ OpenAPI spec + `docs/cli.md` từ help strings; `docs/architecture.md` từ module list
- [ ] v16.2 — **Integration Test Harness [DX]**: `src/integration_test.rs` — E2E tests: start local node → mine → send tx → verify balance → check explorer API; chạy bằng `cargo test --features integration`
- [ ] v16.3 — **Hot Reload Dev Mode [DX]**: `cargo run -- dev` — watch `src/` với `notify` crate; rebuild + restart tự động khi file thay đổi; print diff thời gian build

### Era 22 — PKT Testnet Integration (v15.x)

_Mục tiêu: explorer hiển thị data thật từ PKT testnet, không phải chain local giả._

- [ ] v15.0 — **PKT Wire Protocol**: `src/pkt_wire.rs` — implement pktd P2P message format; `PktMsg` (Version/Verack/GetHeaders/Headers/GetData/Block/Inv/Ping/Pong); handshake với testnet peers; magic bytes `TESTNET_MAGIC = [0x0b,0x11,0x09,0x07]`
- [ ] v15.1 — **Testnet Peer Connect**: `src/pkt_peer.rs` — kết nối bootstrap peers testnet (`testnet-seed.pkt.cash:64765`); peer handshake; keepalive ping/pong; disconnect + retry
- [ ] v15.2 — **Block Download**: `src/pkt_sync.rs` — GetHeaders → Headers → GetData → Block download pipeline; validate block header (CompactTarget, prev_hash chain); lưu vào RocksDB
- [ ] v15.3 — **UTXO Sync**: apply downloaded blocks vào UtxoSet; track sync height; resume từ last known height sau restart
- [ ] v15.4 — **Explorer Live Data**: nối `pktscan_api` với testnet chain thay vì chain local; `GET /api/chain` trả block thật; `GET /api/balance/:addr` tra UTXO testnet thật
- [ ] v15.5 — **Sync Status UI**: progress bar sync trong TUI dashboard + web frontend hiển thị "syncing X/Y blocks"

### Era 20 — Post-Singularity (v99.x) — hardware-dependent
- [ ] v99.0–v99.5 — Quantum Random Beacon, Neural Wallet, Interplanetary Sync, Self-Evolving Contracts, AI Consensus, Singularity Chain

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
| v13.0 | `CompactTarget::max()` = `0x207fffff` (target[0]=0x7f) — KHÔNG phải `0x20000001`; `[0xff;32]` hash KHÔNG pass vì sign-bit constraint |
| v13.3 | Bech32/bech32m tự implement (không cần crate) — polymod BIP-173/BIP-350; v0 dùng constant=1, v1 dùng constant=0x2bc830a3 |
| v14.0 | `TestBackend` từ ratatui cho phép render đầy đủ trong tests không cần terminal thật |
| v14.1 | `SendStep::Input { active_field }` dùng Tab để chuyển field (không phải Enter) — Enter ở Amount field mới gọi `send_proceed()` |
| v14.2 | `static_router()` chỉ có `/static/*` (không có `/`) — merge vào pktscan_api không conflict với route `/` đã có; `serve_index` dùng `embedded_index_handler()` thay `fs::read_to_string` |

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
| 🟢 | Frontend + API cùng port | ✅ v14.2 patch: static_router() merged vào pktscan_api::serve(); serve_index dùng embedded bytes |
| 🟡 | TUI dashboard dữ liệu thật | ⚠ v14.0: đang dùng mock data, chưa nối Arc<Blockchain> |
| 🟡 | QR Code địa chỉ ví | ⏳ v14.3 |
| 🟡 | Shell completions | ⏳ v14.4 |

---

## 📦 Dependencies

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
rocksdb = "0.21"          # v4.2: persistent storage backend
axum = { version = "0.7", features = ["ws"] }   # v4.4: REST API + WebSocket
async-graphql = { version = "7", features = ["tracing"] }  # v10.8: GraphQL
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }  # v10.9: webhook
tracing = "0.1"           # v5.7: structured logging
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
proptest = { version = "1.4", optional = true }  # v5.6: fuzz
blake3 = "1.5"            # v6.0: BLAKE3 hash
rayon = "1.10"            # v6.1: CPU parallel mining
num_cpus = "1.16"
ocl = { version = "0.19", optional = true }     # v6.5: OpenCL (optional)
cust = { version = "0.3", optional = true }     # v6.6: CUDA (optional)
ed25519-dalek = { version = "2", features = ["rand_core"] }  # v9.0.1: Ed25519
zeroize = { version = "1", features = ["derive"] }
rand_core = { version = "0.6", features = ["std"] }
ratatui = "0.26"          # v14.0: Terminal UI
crossterm = "0.27"        # v14.0: terminal input/output
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
Version:   v14.2 ✅
Tests:     1295/1295 pass
Warnings:  0
Errors:    0
```

## 🏗 Kiến trúc data flow (v14.2)

```
cargo run -- node 8333
  └── load_or_new() → RocksDB (storage.rs)
      └── Arc<Mutex<Blockchain>> ─────────────────┐
                                                  │
cargo run -- mine [addr]                          │
  └── node_rpc() sync chain ←──────── P2P (node.rs)
      └── mine block → submit NewBlock            │
                                                  ▼
cargo run -- dashboard        pktscan_api::serve() (axum 0.7)
  └── tui_dashboard.rs          ├── GET /api/status   → height/peers/mempool
      (ratatui, mock data)       ├── GET /api/chain    → blocks JSON
                                 ├── GET /api/balance/:addr
                                 ├── POST /api/write/tx (write role)
                                 └── GET /  (web_frontend.rs → index.html embedded)
                                     GET /static/app.js
                                     GET /static/style.css

Browser (PKTScan)
  └── fetch('/api/chain')   → JSON blocks
  └── fetch('/api/status')  → node stats
  └── app.js auto-refresh 30s

✅ Đã merge (v14.2 patch):
  pktscan_api::serve_index → web_frontend::embedded_index_handler() (compile-time bytes)
  pktscan_api::serve() + .merge(web_frontend::static_router()) → /static/app.js + /static/style.css
```
