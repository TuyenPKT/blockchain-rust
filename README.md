# 🦀 Blockchain Rust — 2009 → 2031+

> Xây dựng một blockchain hoàn chỉnh từ Bitcoin 0.1 đến PKT Native Chain bằng Rust thuần — không dùng bất kỳ blockchain framework nào.

**v4.8 ✅ · 48+ versions · 10+ eras · 57/57 tests · 0 warnings**

---

## Tổng quan

Mỗi version build trực tiếp trên version trước, không viết lại từ đầu. Đọc code theo thứ tự là đọc lịch sử blockchain từ 2009 đến 2031+.

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
Era 10 (2031+)      — PKT Native Chain: PacketCrypt PoW, RocksDB, REST API, Testnet, Metrics
```

---

## Cài đặt

**Yêu cầu:** Rust 1.75+

```bash
git clone https://github.com/your-username/blockchain-rust
cd blockchain-rust
cargo build
cargo test
```

---

## Sử dụng

```bash
# Tạo ví PKT mới
cargo run -- wallet new
cargo run -- wallet show

# Mining
cargo run -- mine                              # mine dùng ví đã tạo
cargo run -- mine <addr_hex> <n>               # mine n blocks
cargo run -- mine <addr_hex> <n> <node:port>   # mine + kết nối P2P node

# P2P Node
cargo run -- node 8333                         # chạy node
cargo run -- node 8334 127.0.0.1:8333          # chạy node + kết nối peer

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

# Metrics (v4.8)
cargo run -- metrics                           # đọc từ local RocksDB
cargo run -- metrics 127.0.0.1:8333            # + query peer count và remote height

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

---

## Metrics (v4.8)

```bash
$ cargo run -- metrics

╔══════════════════════════════════════════════════════════════╗
║                   📊  Node Metrics  v4.8                    ║
╚══════════════════════════════════════════════════════════════╝

  Collected at     : 2026-03-15 10:00:00 UTC

  ── Chain ────────────────────────────────────────────────────
  Height           : 42
  Difficulty       : 3
  UTXO count       : 85

  ── Mempool ──────────────────────────────────────────────────
  Depth            : 7 tx
  Total fees       : 35000 sat  (0.00035000 PKT)

  ── Performance ──────────────────────────────────────────────
  Avg block time   : 8.3s
  Est. hashrate    : 963 H/s

  ── Network ──────────────────────────────────────────────────
  Peers connected  : 2
  Sync status      : local=42  remote=42  ✅ synced
```

---

## Cấu trúc source

```
src/
├── main.rs              CLI dispatch + integration tests (57 tests)
├── block.rs             Block, SHA-256, Merkle root, PoW
├── chain.rs             Blockchain, validation, UTXO management
├── transaction.rs       TxInput/TxOutput, txid/wtxid, SegWit
├── utxo.rs              UTXO set, P2PKH + P2TR balance lookup
├── wallet.rs            ECDSA keypair, Bitcoin address Base58Check
├── mempool.rs           Mempool, fee-rate selection
├── message.rs           P2P message protocol
├── node.rs              TCP node, peer discovery, chain sync
├── hd_wallet.rs         BIP32/39/44 HD Wallet
├── script.rs            Script engine, P2PK/P2PKH/P2SH/OP_RETURN
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
├── packetcrypt.rs       PacketCrypt PoW (announcement + block mining)
├── storage.rs           RocksDB persistent chain + UTXO storage
├── api.rs               REST API (axum 0.7)
├── explorer.rs          Block Explorer CLI
├── genesis.rs           NetworkParams, testnet genesis config
└── metrics.rs           Runtime metrics: hashrate, peers, mempool, sync
```

---

## Dependencies

```toml
sha2       = "0.10"    # SHA-256
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
```

---

## Testnet

Bootstrap peer: `seed.testnet.oceif.com:18333`

```bash
# Kết nối node PC đến VPS testnet
cargo run -- node 18334 seed.testnet.oceif.com:18333

# Mine về VPS seed
cargo run -- mine <addr> 0 seed.testnet.oceif.com:18333
```

---

## License

MIT
