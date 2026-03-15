# TODO — Blockchain Rust

**Version hiện tại: v5.0 ✅**
**Tiến độ: 49/49+ versions — Era 11 bắt đầu**

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

### Era 5 — Consensus nâng cao (2021–2022)
- [x] v1.8 — ZK-SNARK (Schnorr ZK, R1CS, Groth16) → `zk_proof.rs`
- [x] v1.9 — Advanced PoW: GHOST Protocol + Uncle Blocks → `pow_ghost.rs`
- [x] v2.0 — BFT Consensus (Tendermint-style) → `bft.rs`
- [x] v2.1 — Sharding (Beacon chain, shard chains, cross-shard receipts) → `sharding.rs`
- [x] v2.2 — ZK-Rollup (Batch TXs, validity proof, L1Verifier, WithdrawalProof) → `zk_rollup.rs`
- [x] v2.3 — Optimistic Rollup (assume valid, 7-day window, fraud proof, slashing) → `optimistic_rollup.rs`
- [x] v2.4 — Recursive ZK / IVC (constant-size proof, fold, aggregation, light client) → `recursive_zk.rs`
- [x] v2.5 — zkEVM (EVM executor, trace, constraint per opcode, ZK proof) → `zkevm.rs`
- [x] v2.6 — Smart Contract engine (WASM interpreter, gas meter, Counter/Token/Voting) → `smart_contract.rs`
- [x] v2.7 — Oracle (OracleFeed, median, outlier filter, TWAP, heartbeat, circuit breaker, DeFi consumer) → `oracle.rs`
- [x] v2.8 — On-chain Governance (Governor, Proposal lifecycle, quorum, timelock, delegation, veto, treasury) → `governance.rs`
- [x] v2.9 — AI Agent (DCA/StopLoss/TakeProfit/Rebalance, safety limits, AgentEngine, audit log) → `ai_agent.rs`
- [x] v3.0 — CRYSTALS-Dilithium (R_q poly ring, Module-LWE keygen, Fiat-Shamir sign/verify, NIST FIPS 204) → `dilithium.rs`
- [x] v3.1 — Hash-based signature SPHINCS+ (WOTS+ chains, XMSS tree, FORS k-trees, HyperTree, stateless, NIST FIPS 205) → `sphincs.rs`
- [x] v3.2 — Quantum-safe key exchange KYBER/ML-KEM (Module-LWE KEM, keygen/encap/decap, FO transform, NIST FIPS 203) → `kyber.rs`
- [x] v3.3 — Hybrid signature (ECDSA + Dilithium, 3-phase migration, AND/OR policies, backward compat, quantum threat model) → `hybrid_sig.rs`
- [x] v3.4 — Self-amending chain (on-chain protocol upgrade vote, amendment lifecycle, quorum+supermajority, ParameterChange/ProtocolUpgrade/EmergencyFix) → `self_amend.rs`
- [x] v3.5 — Cross-chain messaging IBC-style (light client, connection/channel 4-way handshake, packet lifecycle, timeout, ICS-20 transfer) → `ibc.rs`
- [x] v3.6 — Decentralized Identity DID (W3C DID Core, DID Document, VC issuance/verify, DID Auth challenge-response, key rotation, deactivation) → `did.rs`
- [x] v3.7 — Privacy smart contract FHE (LWE encryption, HE-ADD/MUL-PLAIN, noise budget, encrypted voting/payroll/auction) → `fhe_contract.rs`
- [x] v3.8 — Sovereign Rollup (DA layer, namespace blobs, erasure coding, DAS, full node sync, sovereign upgrade) → `sovereign_rollup.rs`
- [x] v3.9 — Full Stack Integration (version timeline, 9 eras, DID+VC, IBC, FHE vote, hybrid sig, ML-KEM, sovereign rollup) → `full_stack.rs`

---

## ✅ Era 10 — PKT Native Chain (2031+)

- [x] v4.0 — PKT Wallet CLI: `wallet new/show/address`, auto-load → `wallet_cli.rs`
- [x] v4.1 — PacketCrypt PoW: announcement mining + block mining → `packetcrypt.rs`
- [x] v4.2 — Persistent Storage RocksDB: save/load chain+UTXO → `storage.rs`
- [x] v4.3 — P2P Sync: longest-chain, dedup, GetHeight, mempool broadcast → `node.rs`
- [x] v4.4 — REST API: GET/POST endpoints → `api.rs` (axum 0.7)
- [x] v4.5 — Miner ↔ Node: sync + GetMempool + submit block → `miner.rs`
- [x] v4.6 — Block Explorer CLI: chain/block/tx/balance/utxo → `explorer.rs`
- [x] v4.7 — Testnet Config: NetworkParams, build_genesis(), run_local_testnet() → `genesis.rs`
- [x] **Hotfix** — Miner persist chain qua restart (load_or_new + save_blockchain)
- [x] **Hotfix** — node_rpc timeout 5s — tránh hang khi VPS slow
- [x] v4.8 — Metrics: hashrate, peer count, mempool depth, block time, sync status → `metrics.rs`
- [ ] v4.9 — PKT Mainnet _(beta)_

---

## ✅ Era 11 — Optimization & Security (2032–2035)

- [x] v5.0 — Performance: UTXO secondary index, block cache, fast Merkle → `performance.rs`
- [ ] v5.1 — Security hardening: input validation, DoS protection, rate limiting
- [ ] v5.2 — P2P improvements: peer scoring, ban list
- [ ] v5.3 — Coinbase maturity (100 blocks), replay protection
- [ ] v5.4 — Fee market: dynamic estimation, RBF
- [ ] v5.5 — Storage v2: WAL, atomic writes, crash recovery
- [ ] v5.6 — Fuzz testing + proptest
- [ ] v5.7 — Monitoring: metrics endpoint, structured logging (tracing)
- [ ] v5.8 — Code audit: fix Known Gaps
- [ ] v5.9 — Benchmark suite: tps, latency, memory

---

## 🔜 Tiếp theo

**v5.1 — Security hardening**: input validation, DoS protection, rate limiting P2P

---

## 📊 Thống kê

| Era | Versions | Hoàn thành | Còn lại |
|-----|----------|-----------|---------|
| Era 1 (2009)       | 4  | 4  | 0  |
| Era 2 (2010–2013)  | 4  | 4  | 0  |
| Era 3 (2014–2017)  | 5  | 5  | 0  |
| Era 4 (2018–2020)  | 4  | 4  | 0  |
| Era 5 (2021–2022)  | 4  | 4  | 0  |
| Era 6 (2022–2023)  | 4  | 4  | 0  |
| Era 7 (2023–2025)  | 4  | 4  | 0  |
| Era 8 (2025–2027)  | 4  | 4  | 0  |
| Era 9 (2027–2030)  | 6  | 6  | 0  |
| Era 10 (2031+)     | 9  | 9  | 0  |
| Era 11 (2032–2035) | 10 | 1  | 9  |
| **Tổng**           | **49** | **49** | **9** |

> Cập nhật lần cuối: v5.0 ✅ — Era 11 bắt đầu
