#![allow(dead_code)]

/// v3.9 — Full Stack: End-to-end Blockchain (2009 → 2030)
///
/// Integrates all 38 previous versions into a coherent 2030 blockchain ecosystem.
/// This module contains the version registry, era descriptions, and stack statistics
/// used in the final integration demo.
///
/// ─── Technology Journey ──────────────────────────────────────────────────────
///
///   2009  Era 1  Bitcoin Genesis — PoW, Block, UTXO, SHA-256
///   2013  Era 2  Security & Wallets — ECDSA, P2P, HD Wallet
///   2017  Era 3  Scale & Script — SegWit, Lightning, Taproot
///   2020  Era 4  Layer 2 & Privacy — Confidential, CoinJoin, Atomic Swap
///   2021  Era 5  Advanced Consensus — ZK-SNARK, GHOST Protocol
///   2023  Era 6  ZK & Rollup — BFT, Sharding, zkEVM, Recursive ZK
///   2025  Era 7  AI & Programmable — Contracts, Oracle, AI Agent
///   2027  Era 8  Post-Quantum — Dilithium, SPHINCS+, KYBER, Hybrid Sigs
///   2030  Era 9  Autonomous Chain — IBC, DID, FHE, Sovereign Rollup
///
/// ─── 2030 Blockchain Stack ───────────────────────────────────────────────────
///
///   ┌──────────────────────────────────────────────────────────────────────┐
///   │  Identity   DID + VC (W3C)      FHE Privacy Contracts               │
///   │  Application  Smart Contracts   AI Agent   Oracle   Governance       │
///   │  Cross-chain  IBC Messaging     Sovereign Rollup    Atomic Swap      │
///   │  Cryptography Taproot/Schnorr   ZK Proofs  PQ-Hybrid Sigs  ML-KEM   │
///   │  Consensus    BFT + GHOST PoW   Sharding   Self-Amending             │
///   │  Base Layer   Block + UTXO + SegWit + Lightning + P2P                │
///   └──────────────────────────────────────────────────────────────────────┘

// ─── Version Registry ─────────────────────────────────────────────────────────

pub struct VersionInfo {
    pub version:     &'static str,
    pub file:        &'static str,
    pub year:        u16,
    pub description: &'static str,
}

pub const VERSIONS: &[VersionInfo] = &[
    // Era 1 — Nền tảng (2009)
    VersionInfo { version: "v0.1", file: "block.rs",             year: 2009, description: "Block, Chain, SHA-256, Genesis Block" },
    VersionInfo { version: "v0.2", file: "transaction.rs",       year: 2009, description: "Transaction, Coinbase TX" },
    VersionInfo { version: "v0.3", file: "block.rs",             year: 2009, description: "Proof of Work, Mining, Difficulty Adjustment" },
    VersionInfo { version: "v0.4", file: "utxo.rs",              year: 2009, description: "UTXO Set" },
    // Era 2 — Bảo mật & Ví (2010–2013)
    VersionInfo { version: "v0.5", file: "wallet.rs",            year: 2010, description: "ECDSA Signature, Wallet, Base58 Address" },
    VersionInfo { version: "v0.6", file: "node.rs",              year: 2010, description: "P2P Network, TCP, Node Discovery" },
    VersionInfo { version: "v0.7", file: "mempool.rs",           year: 2011, description: "Mempool, Fee Rate Sort, Selection" },
    VersionInfo { version: "v0.8", file: "hd_wallet.rs",         year: 2013, description: "HD Wallet BIP32/39/44" },
    // Era 3 — Scale & Script (2014–2017)
    VersionInfo { version: "v0.9", file: "script.rs",            year: 2014, description: "Script Engine, P2PK, P2PKH, OP_RETURN" },
    VersionInfo { version: "v1.0", file: "script.rs",            year: 2015, description: "Multisig P2SH, M-of-N, RedeemScript" },
    VersionInfo { version: "v1.1", file: "transaction.rs",       year: 2017, description: "SegWit P2WPKH, witness, BIP143" },
    VersionInfo { version: "v1.2", file: "lightning.rs",         year: 2018, description: "Lightning Network, Payment Channel, HTLC" },
    VersionInfo { version: "v1.3", file: "taproot.rs",           year: 2021, description: "Taproot, Schnorr BIP340, MAST, MuSig2" },
    // Era 4 — Layer 2 & Privacy (2018–2020)
    VersionInfo { version: "v1.4", file: "covenant.rs",          year: 2021, description: "Covenants, CTV CheckTemplateVerify, Vault" },
    VersionInfo { version: "v1.5", file: "confidential.rs",      year: 2019, description: "Confidential TX, Pedersen, Range Proof, ECDH" },
    VersionInfo { version: "v1.6", file: "coinjoin.rs",          year: 2019, description: "CoinJoin, PayJoin/P2EP" },
    VersionInfo { version: "v1.7", file: "atomic_swap.rs",       year: 2020, description: "HTLC Atomic Swap, Cross-chain" },
    // Era 5 — Consensus nâng cao (2021)
    VersionInfo { version: "v1.8", file: "zk_proof.rs",          year: 2021, description: "ZK-SNARK, Schnorr ZK, R1CS, Groth16" },
    VersionInfo { version: "v1.9", file: "pow_ghost.rs",         year: 2021, description: "GHOST Protocol, Uncle Blocks" },
    // Era 6 — ZK & Rollup (2022–2023)
    VersionInfo { version: "v2.0", file: "bft.rs",               year: 2022, description: "BFT Consensus, Tendermint-style" },
    VersionInfo { version: "v2.1", file: "sharding.rs",          year: 2022, description: "Sharding, Beacon Chain, Cross-shard Receipts" },
    VersionInfo { version: "v2.2", file: "zk_rollup.rs",         year: 2022, description: "ZK-Rollup, Batch TX, Validity Proof, L1Verifier" },
    VersionInfo { version: "v2.3", file: "optimistic_rollup.rs", year: 2022, description: "Optimistic Rollup, Fraud Proof, 7-day Window" },
    VersionInfo { version: "v2.4", file: "recursive_zk.rs",      year: 2023, description: "Recursive ZK/IVC, constant-size proof, fold" },
    VersionInfo { version: "v2.5", file: "zkevm.rs",             year: 2023, description: "zkEVM, EVM executor, constraint per opcode" },
    // Era 7 — AI & Programmable (2023–2025)
    VersionInfo { version: "v2.6", file: "smart_contract.rs",    year: 2023, description: "WASM Smart Contract Engine, Gas Meter" },
    VersionInfo { version: "v2.7", file: "oracle.rs",            year: 2023, description: "Oracle, TWAP, Circuit Breaker, DeFi Consumer" },
    VersionInfo { version: "v2.8", file: "governance.rs",        year: 2024, description: "On-chain Governance, Proposal Lifecycle, Treasury" },
    VersionInfo { version: "v2.9", file: "ai_agent.rs",          year: 2024, description: "AI Agent, DCA/Stop-loss/Take-profit/Rebalance" },
    // Era 8 — Post-Quantum (2025–2027)
    VersionInfo { version: "v3.0", file: "dilithium.rs",         year: 2025, description: "CRYSTALS-Dilithium, Module-LWE, NIST FIPS 204" },
    VersionInfo { version: "v3.1", file: "sphincs.rs",           year: 2025, description: "SPHINCS+, WOTS+, XMSS, FORS, NIST FIPS 205" },
    VersionInfo { version: "v3.2", file: "kyber.rs",             year: 2025, description: "ML-KEM/KYBER, Module-LWE KEM, NIST FIPS 203" },
    VersionInfo { version: "v3.3", file: "hybrid_sig.rs",        year: 2026, description: "Hybrid Sig: ECDSA + Dilithium, 3-phase migration" },
    // Era 9 — Autonomous Chain (2027–2030)
    VersionInfo { version: "v3.4", file: "self_amend.rs",        year: 2027, description: "Self-Amending Chain, On-chain Protocol Upgrade Vote" },
    VersionInfo { version: "v3.5", file: "ibc.rs",               year: 2027, description: "IBC Cross-chain Messaging, Channel Handshake, Relay" },
    VersionInfo { version: "v3.6", file: "did.rs",               year: 2028, description: "DID, DID Document, Verifiable Credentials, DID Auth" },
    VersionInfo { version: "v3.7", file: "fhe_contract.rs",      year: 2028, description: "FHE Privacy Contract, LWE, Encrypted Vote/Salary" },
    VersionInfo { version: "v3.8", file: "sovereign_rollup.rs",  year: 2029, description: "Sovereign Rollup, DA Layer, Erasure Coding, DAS" },
    VersionInfo { version: "v3.9", file: "full_stack.rs",        year: 2030, description: "Full Stack Integration: End-to-end 2009 → 2030" },
    // Era 10 — PKT Native Chain (2031+)
    VersionInfo { version: "v4.0", file: "wallet_cli.rs",   year: 2031, description: "PKT Wallet CLI: keygen, address, save/load, mine integration" },
    VersionInfo { version: "v4.1", file: "packetcrypt.rs",  year: 2031, description: "PacketCrypt PoW: announcement mining, effective difficulty" },
    VersionInfo { version: "v4.2", file: "storage.rs",      year: 2031, description: "RocksDB Persistent Storage: save/load chain + UTXO" },
    VersionInfo { version: "v4.3", file: "node.rs",         year: 2031, description: "P2P Sync: longest-chain rule, dedup, mempool broadcast" },
    VersionInfo { version: "v4.4", file: "api.rs",          year: 2031, description: "REST API axum 0.7: /chain /balance /mempool /tx /status" },
    VersionInfo { version: "v4.5", file: "miner.rs",        year: 2031, description: "Miner ↔ Node: sync chain, fetch mempool TXs, submit block" },
    VersionInfo { version: "v4.6", file: "explorer.rs",     year: 2031, description: "Block Explorer CLI: chain/block/tx/balance/utxo" },
    VersionInfo { version: "v4.7", file: "genesis.rs",      year: 2031, description: "Testnet Config: NetworkParams, build_genesis(), local testnet" },
    VersionInfo { version: "v4.8", file: "metrics.rs",      year: 2031, description: "Metrics: hashrate, peer count, mempool depth, block time, sync" },
    // Era 11 — Optimization & Security (2032+)
    VersionInfo { version: "v5.0", file: "performance.rs",  year: 2032, description: "Performance: UTXO secondary index, block cache, fast Merkle" },
    VersionInfo { version: "v5.1", file: "security.rs",     year: 2032, description: "Security: RateLimiter, BanList, PeerGuard, InputValidator" },
    VersionInfo { version: "v5.2", file: "p2p.rs",          year: 2032, description: "P2P: PeerRegistry, peer scoring, MessageDedup bounded cache" },
    VersionInfo { version: "v5.3", file: "maturity.rs",     year: 2033, description: "Coinbase maturity 100-block, replay protection, locktime/sequence" },
    VersionInfo { version: "v5.4", file: "fee_market.rs",   year: 2033, description: "Fee market: dynamic estimation, RBF (Replace-By-Fee)" },
    VersionInfo { version: "v5.5", file: "wal.rs",          year: 2033, description: "Storage v2: atomic WriteBatch, WAL epoch, crash recovery" },
    VersionInfo { version: "v5.6", file: "fuzz.rs",         year: 2034, description: "Fuzz + proptest: hash determinism, message no-panic, fee bounds, RBF" },
    VersionInfo { version: "v5.7", file: "monitoring.rs",     year: 2034, description: "Monitoring: tracing structured logs, HealthStatus, /health /ready endpoint" },
    VersionInfo { version: "v5.8", file: "peer_discovery.rs", year: 2034, description: "Peer discovery: PeerStore, DnsSeedResolver, PEX bootstrap, auto-connect" },
    VersionInfo { version: "v5.9", file: "bench.rs",          year: 2035, description: "Benchmark suite: hash/TPS/latency/merkle/UTXO/mempool, BenchResult serde" },
    VersionInfo { version: "v6.0", file: "blake3_hash.rs",    year: 2036, description: "BLAKE3 hash engine: thay SHA-256 cho PoW, hash_version field, 3-4x nhanh hơn" },
    VersionInfo { version: "v6.1", file: "cpu_miner.rs",        year: 2036, description: "CPU multi-thread miner: rayon work-stealing, nonce split, AtomicBool stop flag" },
    VersionInfo { version: "v6.2", file: "chain_concurrent.rs", year: 2036, description: "Thread-safe chain: Arc<RwLock<Blockchain>>, multiple readers + single writer" },
    VersionInfo { version: "v6.3", file: "validator.rs",         year: 2036, description: "Parallel block validation: rayon par_iter, ValidationResult, chain link check" },
    VersionInfo { version: "v6.4", file: "gpu_miner.rs",         year: 2036, description: "GPU miner abstraction: GpuBackend{Software,OpenCL,Cuda}, software fallback, 1/3 CU" },
    VersionInfo { version: "v6.5", file: "opencl_kernel.rs",     year: 2036, description: "OpenCL BLAKE3 kernel: full 7-round compress, G mixing, MSG_SCHEDULE, CPU rayon fallback" },
    VersionInfo { version: "v6.6", file: "cuda_kernel.rs",       year: 2036, description: "CUDA BLAKE3 PTX kernel: __global__ blake3_mine, atomicCAS, --features cuda, CPU fallback" },
    // Era 13 — Token Economy (2037)
    VersionInfo { version: "v7.0", file: "reward.rs",           year: 2037, description: "Block Reward Engine: halving schedule, subsidy_at, estimated_supply" },
    VersionInfo { version: "v7.1", file: "fee_calculator.rs",   year: 2037, description: "Fee Calculator: FeePolicy, vsize estimation, coinbase validation" },
    VersionInfo { version: "v7.2", file: "token.rs",            year: 2037, description: "Token Standard: ERC-20-like mint/transfer/burn/approve/transfer_from" },
    VersionInfo { version: "v7.3", file: "token_tx.rs",         year: 2037, description: "Token Transfer TX: OP_RETURN encoding, BLAKE3 txid, TokenTxBuilder" },
    VersionInfo { version: "v7.4", file: "contract_state.rs",   year: 2037, description: "Smart Contract State: ContractStore, storage_root, snapshot/restore" },
    VersionInfo { version: "v7.5", file: "evm_lite.rs",         year: 2037, description: "EVM-lite Executor: stack VM, SLoad/SStore, Log, gas metering" },
    VersionInfo { version: "v7.6", file: "contract_deploy.rs",  year: 2037, description: "Contract Deployment: CREATE/CREATE2 address, ABI encode/decode" },
    VersionInfo { version: "v7.7", file: "defi.rs",             year: 2037, description: "DeFi AMM: LiquidityPool, x*y=k swap, fee, spot price, DEX" },
    VersionInfo { version: "v7.8", file: "staking.rs",          year: 2037, description: "Staking & Delegation: Validator, Stake, distribute rewards, slash, APY" },
    VersionInfo { version: "v7.9", file: "economics.rs",        year: 2037, description: "Economic Model: EraParams, TokenEconomics, Simulator, project N blocks" },
    // Era 14 — Explorer & Analytics (2038)
    VersionInfo { version: "v8.0", file: "pktscan_api.rs",      year: 2038, description: "PKTScan REST: blocks/tx/address endpoints, search, mempool stats" },
    VersionInfo { version: "v8.5", file: "pkt_analytics.rs",    year: 2038, description: "Analytics: hashrate/difficulty/block_time time-series, CSV export" },
    VersionInfo { version: "v8.9", file: "websocket.rs",        year: 2038, description: "WebSocket: live block/tx feed, address subscribe, mempool delta" },
    // Era 15 — Zero-Trust & SDK (2038)
    VersionInfo { version: "v9.0", file: "zt_middleware.rs",    year: 2038, description: "Zero-Trust middleware: rate limit, IP guard, request validation" },
    VersionInfo { version: "v9.5", file: "ed25519_hd.rs",       year: 2038, description: "Ed25519 HD Wallet (SLIP-0010), Token/Contract/Staking/DeFi APIs" },
    VersionInfo { version: "v9.9", file: "openapi.rs",          year: 2038, description: "OpenAPI 3.1 spec + SDK code generator (Rust/JS/TS/Python)" },
    // Era 16 — API Auth & GraphQL (2039)
    VersionInfo { version: "v10.0", file: "api_auth.rs",        year: 2039, description: "API Key auth: blake3 hash, role (read/write/admin), audit log" },
    VersionInfo { version: "v10.5", file: "graphql.rs",         year: 2039, description: "GraphQL endpoint: schema, query/subscription, depth/complexity limit" },
    VersionInfo { version: "v10.9", file: "webhook.rs",         year: 2039, description: "Webhooks: register, sign HMAC, retry queue, dead letter" },
    // Era 17 — Write APIs (2039)
    VersionInfo { version: "v11.0", file: "write_api.rs",       year: 2039, description: "POST /api/write/*: TX/Token/Contract write, mempool inject" },
    VersionInfo { version: "v11.5", file: "deploy_config.rs",   year: 2039, description: "Deploy config gen: Dockerfile, compose, systemd, nginx, env" },
    // Era 18 — Wallet UX (2040)
    VersionInfo { version: "v12.0", file: "bip39.rs",           year: 2040, description: "BIP39 mnemonic restore + Ed25519 SLIP-0010 derivation" },
    VersionInfo { version: "v12.5", file: "qr.rs",              year: 2040, description: "QR code: address + payment URI (pkt:addr?amount=X), terminal render" },
    VersionInfo { version: "v12.9", file: "completions.rs",     year: 2040, description: "Shell completions: bash/zsh/fish auto-generated" },
    // Era 19 — PKT Wire & Genesis (2040)
    VersionInfo { version: "v13.0", file: "packetcrypt.rs",     year: 2040, description: "PacketCrypt PKT chuẩn: 1024 announcements, effective difficulty" },
    VersionInfo { version: "v13.5", file: "pkt_address.rs",     year: 2040, description: "PKT Address Base58Check (P2PKH/P2TR), bech32 encoding" },
    VersionInfo { version: "v13.9", file: "pkt_genesis.rs",     year: 2040, description: "PKT Genesis: 20 PKT/block, 525k halving, 21M supply, network steward" },
    // Era 21 — TUI & Web Frontend (2041)
    VersionInfo { version: "v14.0", file: "tui_dashboard.rs",   year: 2041, description: "TUI Dashboard (ratatui): hashrate, peers, mempool, block history" },
    VersionInfo { version: "v14.5", file: "web_frontend.rs",    year: 2041, description: "Web Explorer: block/address detail, live WS charts (Chart.js)" },
    // Era 22 — PKT Wire Protocol (2041)
    VersionInfo { version: "v15.0", file: "pkt_wire.rs",        year: 2041, description: "PKT Wire Protocol: 84-byte header, BLAKE3 PoW, varint encoding" },
    VersionInfo { version: "v15.3", file: "pkt_utxo_sync.rs",   year: 2041, description: "UTXO Sync: addr index, height tracking, balance reconstruction" },
    VersionInfo { version: "v15.7", file: "pkt_node.rs",        year: 2041, description: "PKT Node Server: template server, P2P bridge, mempool relay" },
    VersionInfo { version: "v15.8", file: "pkt_sync.rs",        year: 2041, description: "Single chain architecture, PKTScan live data integration" },
    // Era 23 — Devnet & E2E (2041)
    VersionInfo { version: "v16.0", file: "devnet.rs",          year: 2041, description: "Devnet one-command, hot reload, integration test harness" },
    // Era 24 — Block Explorer Pro (2042)
    VersionInfo { version: "v17.0", file: "explorer_pro.rs",    year: 2042, description: "Block Explorer Pro: TX detail, multi-sort, rich list, mempool pro" },
    // Era 25 — Analytics Charts (2042)
    VersionInfo { version: "v18.0", file: "charts_live.rs",     year: 2042, description: "Chart.js analytics: hashrate/difficulty/block_time time-series, CSV export" },
    // Era 26 — JSON-RPC & Dev Portal (2043)
    VersionInfo { version: "v19.0", file: "workspace.rs",       year: 2043, description: "Cargo workspace: blockchain-rust + pkt-cli + pkt-sdk crates" },
    VersionInfo { version: "v19.2", file: "pkt_rpc.rs",         year: 2043, description: "JSON-RPC 2.0 Bitcoin-compatible: getblock, getrawtx, sendrawtx" },
    VersionInfo { version: "v19.7", file: "playground.rs",      year: 2043, description: "API Playground + Webhook UI + Dev Portal (in-browser docs)" },
    // Era 27 — PKTScan Desktop (2043)
    VersionInfo { version: "v20.0", file: "tauri/lib.rs",       year: 2043, description: "Tauri v2 Desktop: React UI, Charts, Search, CI Release pipeline" },
    // Era 28 — Desktop Advanced (2043)
    VersionInfo { version: "v21.0", file: "tauri/miner.rs",     year: 2043, description: "Real Miner IPC, i18n (vi/en), Wallet tab, Peer Scan" },
    // Era 29 — Backend Fix (2044)
    VersionInfo { version: "v22.0", file: "pkt_addr_index.rs",  year: 2044, description: "Address Index Fix, UTXO Height, Block TX Count, Broadcast TX, Wallet Send" },
    // Era 30 — PKT Full Node (2044)
    VersionInfo { version: "v23.0", file: "pkt_fullnode.rs",    year: 2044, description: "TX Validation, P2PKH script, Block+TX relay, Multi-peer, IBD checkpoints" },
    VersionInfo { version: "v23.8", file: "pkt_fullnode.rs",    year: 2044, description: "Full Node Mode + Security Patch (15 issues)" },
    // Era 31 — Public Testnet (2044)
    VersionInfo { version: "v24.0", file: "pkt_install.rs",     year: 2044, description: "Node Onboarding (Linux/macOS/Windows), EVM Address, Mining Pool" },
    VersionInfo { version: "v24.6", file: "pkt_config.rs",      year: 2044, description: "Tokenomics 21M PKT, LZ4 Compression, Network Config (single source)" },
    VersionInfo { version: "v24.10", file: "pkt_audit.rs",      year: 2044, description: "Testnet Audit: tokenomics tests, real checkpoints, Developer Docs (OpenAPI)" },
    // Era 32 — Storage Migration redb (2045)
    VersionInfo { version: "v25.0", file: "kv_store.rs",        year: 2045, description: "KV Abstraction (RocksKv/RedbKv), feature flag migration" },
    VersionInfo { version: "v25.5", file: "redb_kv.rs",         year: 2045, description: "Remove RocksDB — redb pure-Rust backend duy nhất (no C++ dep)" },
    VersionInfo { version: "v25.7", file: "url_guard.rs",       year: 2045, description: "Security Hardening: 9 patches (SSRF, timing, bind, GraphQL DoS, XFF, install checksum)" },
    // Era 33 — EVM Compatible Layer (2045)
    VersionInfo { version: "v26.0", file: "pkt_evm.rs",         year: 2045, description: "Full EVM Stack: gas_model EIP-1559, 140+ opcodes, eth_rpc, eth_wire ETH/68" },
    VersionInfo { version: "v26.1", file: "evm_precompiles.rs", year: 2045, description: "Ethereum PoW Parity: RLP, Uncle, precompiles 0x01–0x09, ABI, receipts, EIP-155" },
    // Era 34 — Bitcoin Script Parity (2045)
    VersionInfo { version: "v27.0", file: "script.rs",          year: 2045, description: "Bitcoin Script Complete: CLTV/CSV/OP_IF/HTLC/Taproot/Schnorr/Lightning" },
    VersionInfo { version: "v27.1", file: "evm_state.rs",       year: 2045, description: "CALL/CREATE sub-EVM: WorldState snapshot/restore, depth guard 1024" },
];

// ─── Era Descriptions ─────────────────────────────────────────────────────────

pub struct Era {
    pub name:     &'static str,
    pub range:    &'static str,
    pub versions: &'static str,
    pub count:    usize,
    pub theme:    &'static str,
}

pub const ERAS: &[Era] = &[
    Era { name: "Era 1", range: "2009",      versions: "v0.1–v0.4", count: 4, theme: "Bitcoin Genesis — PoW, Block, UTXO, SHA-256" },
    Era { name: "Era 2", range: "2010–2013", versions: "v0.5–v0.8", count: 4, theme: "Security & Wallets — ECDSA, P2P, HD Wallet" },
    Era { name: "Era 3", range: "2014–2021", versions: "v0.9–v1.3", count: 5, theme: "Scale & Script — SegWit, Lightning, Taproot, MAST" },
    Era { name: "Era 4", range: "2018–2020", versions: "v1.4–v1.7", count: 4, theme: "Layer 2 & Privacy — CTV, Confidential, CoinJoin, Swap" },
    Era { name: "Era 5", range: "2021",      versions: "v1.8–v1.9", count: 2, theme: "Advanced Consensus — ZK-SNARK, GHOST Protocol" },
    Era { name: "Era 6", range: "2022–2023", versions: "v2.0–v2.5", count: 6, theme: "ZK & Rollup — BFT, Sharding, zkEVM, Recursive ZK" },
    Era { name: "Era 7", range: "2023–2025", versions: "v2.6–v2.9", count: 4, theme: "AI & Programmable — Contracts, Oracle, AI Agent" },
    Era { name: "Era 8", range: "2025–2027", versions: "v3.0–v3.3", count: 4, theme: "Post-Quantum — Dilithium, SPHINCS+, ML-KEM, Hybrid" },
    Era { name: "Era 9", range: "2027–2030", versions: "v3.4–v3.9", count: 6, theme: "Autonomous Chain — IBC, DID, FHE, Sovereign Rollup" },
    Era { name: "Era 10", range: "2031+",     versions: "v4.0–v4.8", count: 9, theme: "PKT Native Chain — PacketCrypt PoW, REST API, Testnet, Metrics" },
    Era { name: "Era 11", range: "2032–2035", versions: "v5.0–v5.9", count: 10, theme: "Optimization & Security — fee market, WAL, fuzz, monitoring, peer discovery, benchmarks" },
    Era { name: "Era 12", range: "2036+",     versions: "v6.0–v6.9", count: 7,  theme: "Multi-threading & GPU — BLAKE3, rayon, Arc<RwLock>, OpenCL, CUDA, SIMD, Mining Pool" },
    Era { name: "Era 13", range: "2037+",     versions: "v7.0–v7.9", count: 10, theme: "Token Economy — Block Reward, Fees, ERC-20 Token, EVM-lite, DeFi AMM, Staking, Economics" },
    Era { name: "Era 14", range: "2038",      versions: "v8.0–v8.9",   count: 3,  theme: "Explorer & Analytics — REST API, WebSocket, address page, mempool, CSV" },
    Era { name: "Era 15", range: "2038",      versions: "v9.0–v9.9",   count: 3,  theme: "Zero-Trust & SDK — middleware, Ed25519 HD, OpenAPI, code gen" },
    Era { name: "Era 16", range: "2039",      versions: "v10.0–v10.9", count: 3,  theme: "API Auth & GraphQL — API key, audit, depth/complexity limit, webhooks" },
    Era { name: "Era 17", range: "2039",      versions: "v11.0–v11.9", count: 2,  theme: "Write APIs — TX/Token/Contract write, deploy config, webhook delivery" },
    Era { name: "Era 18", range: "2040",      versions: "v12.0–v12.9", count: 3,  theme: "Wallet UX — BIP39 restore, Ed25519 SLIP-0010, QR Code, shell completions" },
    Era { name: "Era 19", range: "2040",      versions: "v13.0–v13.9", count: 3,  theme: "PKT Wire & Genesis — PacketCrypt PKT chuẩn, Base58Check, network steward" },
    Era { name: "Era 21", range: "2041",      versions: "v14.0–v14.9", count: 2,  theme: "TUI & Web Frontend — ratatui dashboard, Chart.js live, address detail" },
    Era { name: "Era 22", range: "2041",      versions: "v15.0–v15.8", count: 4,  theme: "PKT Wire Protocol — 84-byte header, BLAKE3 PoW, sync engine, UTXO sync" },
    Era { name: "Era 23", range: "2041",      versions: "v16.0–v16.3", count: 1,  theme: "Devnet & E2E — devnet one-command, hot reload, integration test harness" },
    Era { name: "Era 24", range: "2042",      versions: "v17.0–v17.9", count: 1,  theme: "Block Explorer Pro — TX detail, multi-sort, rich list, mempool pro" },
    Era { name: "Era 25", range: "2042",      versions: "v18.0–v18.9", count: 1,  theme: "Analytics Charts — Chart.js time-series, hashrate/difficulty, CSV export" },
    Era { name: "Era 26", range: "2043",      versions: "v19.0–v19.9", count: 3,  theme: "JSON-RPC & Dev Portal — Cargo workspace, Bitcoin RPC, SDK, playground" },
    Era { name: "Era 27", range: "2043",      versions: "v20.0–v20.9", count: 1,  theme: "PKTScan Desktop — Tauri v2, React, Charts, Search, CI Release" },
    Era { name: "Era 28", range: "2043",      versions: "v21.0–v21.2", count: 1,  theme: "Desktop Advanced — Real Miner IPC, i18n, Wallet + Peer Scan" },
    Era { name: "Era 29", range: "2044",      versions: "v22.0–v22.6", count: 1,  theme: "Backend Fix — Address Index, UTXO Height, Block TX Count, Broadcast TX" },
    Era { name: "Era 30", range: "2044",      versions: "v23.0–v23.8", count: 2,  theme: "PKT Full Node — TX validation, P2PKH, Block Relay, IBD checkpoints, Security Patch" },
    Era { name: "Era 31", range: "2044",      versions: "v24.0–v24.10", count: 3, theme: "Public Testnet — Node Onboarding, EVM Address, Mining Pool, Tokenomics 21M, Audit" },
    Era { name: "Era 32", range: "2045",      versions: "v25.0–v25.7", count: 3,  theme: "Storage Migration — RedbKv (no C++ dep), Security Hardening (9 patches)" },
    Era { name: "Era 33", range: "2045",      versions: "v26.0–v26.1", count: 2,  theme: "EVM Compatible Layer — gas_model, 140+ opcodes, ETH/68 wire, RLP, precompiles" },
    Era { name: "Era 34", range: "2045",      versions: "v27.0–v27.1", count: 2,  theme: "Bitcoin Script Parity — CLTV/CSV/HTLC/Taproot/Lightning + sub-EVM CALL/CREATE" },
];

// ─── Stack Statistics ─────────────────────────────────────────────────────────

pub struct StackStats {
    pub total_versions:     usize,
    pub total_eras:         usize,
    pub total_src_files:    usize,
    pub crypto_primitives:  &'static [&'static str],
    pub consensus_mechanisms: &'static [&'static str],
    pub layer2_solutions:   &'static [&'static str],
    pub pq_algorithms:      &'static [&'static str],
    pub privacy_tech:       &'static [&'static str],
}

pub const STATS: StackStats = StackStats {
    total_versions:  VERSIONS.len(),
    total_eras:      ERAS.len(),
    total_src_files: 173,

    crypto_primitives: &[
        "SHA-256 (block hash, Merkle)",
        "ECDSA secp256k1 (Bitcoin signatures)",
        "Schnorr BIP340 (Taproot)",
        "RIPEMD-160 (address derivation)",
        "Pedersen Commitment (confidential)",
        "HMAC-SHA512 (HD wallet BIP32)",
        "CRYSTALS-Dilithium (NIST FIPS 204)",
        "SPHINCS+ (NIST FIPS 205)",
        "ML-KEM/KYBER (NIST FIPS 203)",
        "LWE encryption (FHE contract)",
    ],

    consensus_mechanisms: &[
        "Proof of Work (SHA-256, difficulty)",
        "GHOST Protocol + Uncle Blocks",
        "BFT Tendermint-style (prevote/precommit)",
        "On-chain governance voting (v2.8)",
        "Self-amending protocol vote (v3.4)",
    ],

    layer2_solutions: &[
        "Lightning Network (payment channels, HTLC)",
        "ZK-Rollup (validity proof, batch settle)",
        "Optimistic Rollup (fraud proof, 7-day)",
        "Recursive ZK / IVC (constant-size)",
        "zkEVM (EVM trace, per-opcode constraint)",
        "Sovereign Rollup (DA-layer, self-settle)",
    ],

    pq_algorithms: &[
        "CRYSTALS-Dilithium (Module-LWE sign)",
        "SPHINCS+ (hash-based stateless sign)",
        "ML-KEM (Module-LWE key encapsulation)",
        "Hybrid ECDSA+Dilithium (migration)",
    ],

    privacy_tech: &[
        "Pedersen Commitment + Range Proof",
        "CoinJoin + PayJoin/P2EP",
        "HTLC Atomic Swap (cross-chain)",
        "Schnorr ZK / Groth16 SNARK",
        "Confidential TX (ECDH blinding)",
        "FHE LWE (encrypted contracts)",
        "DID + VC (self-sovereign identity)",
    ],
};

// ─── Security Layers ──────────────────────────────────────────────────────────

pub struct SecurityLayer {
    pub layer: &'static str,
    pub mechanism: &'static str,
    pub threat_model: &'static str,
}

pub const SECURITY_STACK: &[SecurityLayer] = &[
    SecurityLayer {
        layer:        "Classical Crypto",
        mechanism:    "ECDSA, Schnorr, SHA-256",
        threat_model: "Classical computers: 128-bit security",
    },
    SecurityLayer {
        layer:        "Post-Quantum (PQ)",
        mechanism:    "Dilithium, SPHINCS+, ML-KEM",
        threat_model: "Quantum computers (Shor/Grover): 128-bit PQ security",
    },
    SecurityLayer {
        layer:        "Hybrid Transition",
        mechanism:    "ECDSA ∧ Dilithium (AND policy)",
        threat_model: "Must break BOTH simultaneously — defense in depth",
    },
    SecurityLayer {
        layer:        "Zero-Knowledge",
        mechanism:    "Groth16, Recursive ZK, zkEVM",
        threat_model: "Verifiable computation without revealing witnesses",
    },
    SecurityLayer {
        layer:        "Privacy",
        mechanism:    "FHE (LWE), Pedersen, CoinJoin",
        threat_model: "Data privacy: chain never sees plaintext",
    },
    SecurityLayer {
        layer:        "Identity",
        mechanism:    "DID + VC (W3C), DID Auth",
        threat_model: "Self-sovereign — no central authority",
    },
    SecurityLayer {
        layer:        "Availability",
        mechanism:    "Erasure coding + DAS (k=8: 99.6%)",
        threat_model: "Data withholding: probabilistic detection",
    },
];
