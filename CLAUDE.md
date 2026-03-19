# CLAUDE.md — Blockchain Rust Project

## Tổng quan

Dự án xây dựng blockchain từ Bitcoin 0.1 đến 2030 bằng Rust.
Mỗi version build trên nền version trước, không viết lại từ đầu.

**Version hiện tại: v14.2 ✅**

## Quy tắc làm việc

- dùng tiếng việt nếu không bắt buộc dùng English
- Không viết lại code cũ — chỉ thêm file mới hoặc mở rộng file hiện có
- Mỗi version thêm 1 file mới trong `src/` và thêm `mod <tên>;` vào `main.rs`
- KHÔNG thêm demo functions — thay vào đó thêm `#[test]` vào `mod tests` trong `main.rs`
- Cập nhật `CONTEXT.md` sau mỗi version: đánh dấu `[x]`, cập nhật version hiện tại, ghi quyết định thiết kế và lỗi gặp phải
- Không có warnings khi build xong (`cargo build` và `cargo test` đều pass)
- Khi được hỏi câu hỏi mà không có yêu cầu implement rõ ràng, 
  chỉ giải thích/thảo luận, KHÔNG tự động viết code.

Security — AI-generated Code Guidelines

Không tin code AI mặc định; luôn review thủ công các phần: auth, permission, crypto.

validate input, không raw SQL, không hardcode secret, handle error đầy đủ.

API mặc định read-only; tách node → indexer → DB → API; áp dụng rate limit + API key.

Validate input chặt (format/length), giới hạn query (pagination, range), cache chống DoS.

Dùng tool: cargo audit, clippy, dependency scan.

Không expose internal logic; log không lộ thông tin nhạy cảm.

Viết test cho case lỗi/boundary (invalid input, overflow, spam).

Nguyên tắc: API = data mirror, không phải control layer.

## Stack

- Rust edition 2021
- `cargo run` → hiển thị help + version timeline
- `cargo run -- wallet new/show` → PKT wallet CLI
- `cargo run -- mine [addr] [n]` → PoW miner
- `cargo run -- node <port> [peer]` → P2P node
- `cargo test` → chạy 27 integration tests
- `cargo build` → kiểm tra compile

## Dependencies hiện tại (Cargo.toml)

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
rocksdb = "0.21"                         # v4.2.1: persistent storage backend
```

Khi thêm dependency mới: ghi chú version và lý do vào `CONTEXT.md`.

## Cấu trúc file

```
src/
├── main.rs              ← mod declarations + CLI dispatch + #[cfg(test)] integration tests
├── block.rs             ← Block, SHA-256, Merkle root, mining
├── chain.rs             ← Blockchain, validation, send/spend
├── transaction.rs       ← TxInput (witness), TxOutput, txid/wtxid
├── utxo.rs              ← UtxoSet, owner_bytes_of()
├── wallet.rs            ← ECDSA keypair, Bitcoin address
├── mempool.rs           ← Mempool, fee_rate sort
├── message.rs           ← P2P message enum
├── node.rs              ← TCP node, peer discovery
├── hd_wallet.rs         ← BIP32/39/44
├── script.rs            ← Opcode, Script, ScriptInterpreter
├── lightning.rs         ← Channel, HTLC, CommitmentTx
├── taproot.rs           ← Schnorr, MAST, P2TR, MuSig2
├── covenant.rs          ← CTV, Vault, CongestionBatch
├── confidential.rs      ← Pedersen, RangeProof, ECDH
├── coinjoin.rs          ← CoinJoin, PayJoin
├── atomic_swap.rs       ← HTLC cross-chain, AtomicSwap
├── zk_proof.rs          ← Schnorr ZK, R1CS, Groth16
├── pow_ghost.rs         ← GHOST Protocol, Uncle Blocks
├── bft.rs               ← Tendermint, BftValidatorSet, ConsensusResult
├── sharding.rs          ← BeaconChain, ShardChain, CrossShardReceipt
├── zk_rollup.rs         ← RollupBatch, ZkRollupProof, L1Verifier
├── optimistic_rollup.rs ← Sequencer, FraudProof, L1OptimisticContract
├── recursive_zk.rs      ← IvcChain, RecursiveProof, AggregatedProof
├── zkevm.rs             ← EvmExecutor, ZkEvmProof, ZkEvmVerifier
├── smart_contract.rs    ← WasmRuntime, ContractRegistry, GasMeter
├── oracle.rs            ← OracleFeed, OracleRegistry, LendingProtocol
├── governance.rs        ← Governor, Proposal, TimelockQueue
├── ai_agent.rs          ← AgentEngine, AgentRule, safety limits
├── dilithium.rs         ← Module-LWE, CRYSTALS-Dilithium (FIPS 204)
├── sphincs.rs           ← WOTS+, XMSS, FORS, SPHINCS+ (FIPS 205)
├── kyber.rs             ← ML-KEM, KEM keygen/encap/decap (FIPS 203)
├── hybrid_sig.rs        ← ECDSA + Dilithium hybrid, migration phases
├── self_amend.rs        ← On-chain protocol upgrade vote
├── ibc.rs               ← IBC Relayer, channel/connection handshake
├── did.rs               ← DID, VerifiableCredential, AuthChallenge
├── fhe_contract.rs      ← FHE keygen, EncryptedVoteContract
├── sovereign_rollup.rs  ← DaLayer, SovereignRollup, DAS
├── sdk_gen.rs           ← SdkRouter, generate_js_sdk, generate_ts_sdk (v9.9)
├── full_stack.rs        ← VERSIONS, ERAS, STATS, SECURITY_STACK (v3.9)
├── miner.rs             ← MinerConfig, MinerStats, mine_live(), live hashrate
└── wallet_cli.rs        ← cmd_wallet_new/show/address, load_miner_address() (v4.0)
```

## Lưu ý kỹ thuật quan trọng

- `secp256k1 = 0.27`: dùng `Message::from_slice()`, không có `from_digest_slice()`
- `secp256k1 = 0.27`: không có `mul_assign` trên `PublicKey` — dùng hash-based ECDH
- `secp256k1 = 0.27`: `PublicKey::combine()` thay vì `add_exp_assign`
- `pbkdf2`: bắt buộc `features = ["hmac"]`
- Schnorr: sign bằng `tweaked_sk`, không phải `internal_sk`
- `TxOutput.script_pubkey` và `TxInput.script_sig` là type `Script`
- UTXO lookup: `owner_bytes_of()` hỗ trợ cả 20-byte (P2PKH) và 32-byte (P2TR)
- Tránh `try_into()` trên `&[u8;64]` — dùng `copy_from_slice` thay thế
- `#![allow(dead_code)]` ở đầu file khi có nhiều public API chưa dùng

## Thứ tự version tiếp theo (Era 19 — PKT Core)

v13.3 — PKT Address Format: `src/pkt_address.rs` — bech32 PKT address encoding (hrp="pkt")
v13.4 — PKT Testnet Genesis: `src/pkt_genesis.rs` — genesis block PKT testnet, coin params (paklets, halving), bootstrap peers

## Thứ tự version tiếp theo (Era 10 — đã hoàn thành)

v4.1 — PacketCrypt PoW: `src/packetcrypt.rs` — announcement mining + block mining (PKT-native PoW)
v4.2 — Persistent Storage: `src/storage.rs` — lưu chain + UTXO vào file, load khi restart
v4.3 — P2P Sync: longest-chain rule + mempool broadcast + block validation khi nhận từ peer
v4.4 — REST API: `src/api.rs` — GET /chain, GET /balance/:addr, POST /tx (axum/hyper)
v4.5 — Miner ↔ Node: miner submit block qua P2P, lấy pending TX từ mempool node
v4.6 — Block Explorer CLI: `src/explorer.rs` — query chain, TX, UTXO, balance
v4.7 — Testnet Config: `src/genesis.rs` — genesis block file, network magic bytes, coin params
v4.8 — Metrics: hashrate, peer count, mempool depth, block time, sync status
v4.9 — PKT Mainnet: _(beta — chưa lên kế hoạch)_

## Era 11 — Optimization & Security (v5.x)

v5.0 — Performance: UTXO set indexing, block cache, faster Merkle tree
v5.1 — Security hardening: input validation, DoS protection, rate limiting P2P
v5.2 — P2P improvements: peer scoring, ban list, message deduplication
v5.3 — Coinbase maturity (100 blocks), replay protection, nonce validation
v5.4 — Fee market: dynamic fee estimation, RBF (Replace-By-Fee)
v5.5 — Persistent storage v2: WAL (Write-Ahead Log), atomic writes, crash recovery
v5.6 — Fuzz testing + property-based tests (cargo-fuzz, proptest)
v5.7 — Monitoring: metrics endpoint, health check, structured logging (tracing)
v5.8 — Code audit: fix tất cả Known Gaps còn lại từ CONTEXT.md
v5.9 — Benchmark suite: tps, latency, memory — perf regression baseline

## Era 20 — Post-Singularity (v10.x) — hardware-dependent, future

v10.0 — Quantum Random Beacon (cần quantum hardware)
v10.1 — Neural Wallet (AI-driven signing heuristics)
v10.2 — Interplanetary Sync (delay-tolerant networking, Mars latency)
v10.3 — Self-Evolving Smart Contracts (genetic algorithm bytecode)
v10.4 — Decentralized AI Consensus (neural network validator voting)
v10.5 — Singularity Chain (fully autonomous, post-human governance)
