# CLAUDE.md — Blockchain Rust Project

## Tổng quan

Dự án xây dựng blockchain từ Bitcoin 0.1 đến 2030 bằng Rust.
Mỗi version build trên nền version trước, không viết lại từ đầu.

**Version hiện tại: v14.3 ✅**

## Quy tắc làm việc

Yêu cầu:
- Có core logic
- Có interface (CLI hoặc API cụ thể)
- Có integration vào system hiện tại
- trả lời bằng 100% tiếng việt nếu không bắt buộc dùng English
- Không viết lại code cũ — chỉ thêm file mới hoặc mở rộng file hiện có
- Mỗi version thêm 1 file mới trong `src/` và thêm `mod <tên>;` vào `main.rs`
- KHÔNG thêm demo functions — thay vào đó thêm `#[test]` vào `mod tests` trong `main.rs`
- Cập nhật `CONTEXT.md` sau mỗi version: đánh dấu `[x]`, cập nhật version hiện tại, ghi quyết định thiết kế và lỗi gặp phải
- Không có warnings khi build xong (`cargo build` và `cargo test` đều pass)
- Khi được hỏi câu hỏi mà không có yêu cầu implement rõ ràng,
  chỉ giải thích/thảo luận, KHÔNG tự động viết code.
- Nếu user không dùng được → feature chưa xong

Không chấp nhận:
- chỉ function
- code demo

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
- `cargo run -- qr <address> [amount] [label]` → QR code trong terminal
- `cargo test` → chạy toàn bộ unit + integration tests
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
axum = { version = "0.7", features = ["ws"] }  # v4.4: REST API + v8.1: WebSocket
async-graphql = { version = "7", features = ["tracing"] }  # v10.8: GraphQL API
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }  # v10.9: webhook
tracing = "0.1"                          # v5.7: structured logging
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
blake3 = "1.5"                           # v6.0: BLAKE3 hash engine
rayon = "1.10"                           # v6.1: multi-thread miner
num_cpus = "1.16"
ed25519-dalek = { version = "2", features = ["rand_core"] }  # v9.0.1: Ed25519 HD Wallet
zeroize = { version = "1", features = ["derive"] }
rand_core = { version = "0.6", features = ["std"] }
ratatui = "0.26"                         # v14.0: Terminal UI dashboard
crossterm = "0.27"                       # v14.0: Terminal input/output
qrcode = "0.14"                          # v14.3: QR code render (pure Rust)
proptest = { version = "1.4", optional = true }  # v5.6: fuzz/property tests
# Optional features:
# ocl = "0.19"   (--features opencl) v6.5: OpenCL GPU mining
# cust = "0.3"   (--features cuda)   v6.6: CUDA GPU mining
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
├── wallet_cli.rs        ← cmd_wallet_new/show/address, load_miner_address() (v4.0)
├── pktscan_api.rs       ← REST API + Block Explorer (axum): /chain /balance /tx /status
├── pkt_bandwidth.rs     ← PacketCrypt bandwidth scoring, announcements (v13.2)
├── pkt_address.rs       ← PKT bech32/bech32m address encode/decode (v13.3)
├── pkt_genesis.rs       ← PKT coin params, genesis block, halving schedule (v13.4)
├── tui_dashboard.rs     ← Terminal UI dashboard (ratatui): hashrate/peers/mempool (v14.0)
├── tui_wallet.rs        ← Wallet TUI: balance/send/receive/history tabs (v14.1)
├── web_frontend.rs      ← Embedded static assets (index.html/app.js/style.css) (v14.2)
└── qr_code.rs           ← QR code render: terminal half-block/full-block, BIP21 URI (v14.3)

frontend/
├── app.js               ← Vanilla JS SPA: fetch API, auto-refresh, theme toggle
└── style.css            ← Dark theme CSS, stat cards, data tables

index.html               ← Entry point (embedded via include_bytes!)
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
- ratatui: dùng `TestBackend` cho unit tests, không cần real terminal
- `web_frontend`: `static_router()` chỉ mount `/static/*`, merge vào `pktscan_api::serve()`
- QR width = `17 + 4×N` (QR spec) — test: `(w - 17) % 4 == 0`

## Thứ tự version tiếp theo (Era 21 còn lại — UI)

v14.4 — Shell Completions [CX]: bash/zsh/fish, `cargo run -- completions <shell>`
v14.5 — Web Charts [UI]: sparkline TUI + Chart.js web (hashrate/block time/tx volume)
v14.6 — Block Detail Page [UI]: /block/:height + /tx/:txid, hash-router trong app.js
v14.7 — Address Detail Page [UI]: balance + UTXO list + tx history
v14.8 — WebSocket Live Feed [UI]: /ws real-time NewBlock/NewTx, toast notification

## Era 22 — PKT Testnet Integration (v15.x)

v15.0 — PKT Wire Protocol: `src/pkt_wire.rs` — pktd P2P message format, handshake testnet
v15.1 — Testnet Peer Connect: `src/pkt_peer.rs` — kết nối bootstrap peers, ping/pong keepalive
v15.2 — Block Download: `src/pkt_sync.rs` — GetHeaders → GetData → validate → RocksDB
v15.3 — UTXO Sync: apply blocks vào UtxoSet, resume từ last height
v15.4 — Explorer Live Data: pktscan_api dùng testnet chain thật
v15.5 — Sync Status UI: progress bar trong TUI + web frontend

## Era 23 — Developer Experience (v16.x)

v16.0 — Devnet One-Command [DX]: `cargo run -- devnet` → node+miner+API một process
v16.1 — Dev Docs Generator [DX]: `cargo run -- docs` → api.md + cli.md + architecture.md
v16.2 — Integration Test Harness [DX]: E2E tests --features integration
v16.3 — Hot Reload Dev Mode [DX]: watch src/, rebuild + restart tự động

## Era 20 — Post-Singularity (v10.x) — hardware-dependent, future

v10.0 — Quantum Random Beacon (cần quantum hardware)
v10.1 — Neural Wallet (AI-driven signing heuristics)
v10.2 — Interplanetary Sync (delay-tolerant networking, Mars latency)
v10.3 — Self-Evolving Smart Contracts (genetic algorithm bytecode)
v10.4 — Decentralized AI Consensus (neural network validator voting)
v10.5 — Singularity Chain (fully autonomous, post-human governance)
