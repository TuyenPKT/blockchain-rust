# 🦀 Blockchain Rust — CONTEXT

**Version hiện tại: v5.8 ✅ — 127/127 tests pass, 0 errors, 0 warnings**

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
- [ ] v5.9 — Benchmark suite: tps, latency, memory

### Era 20 — Post-Singularity (v10.x) — hardware-dependent
- [ ] v10.0–v10.5 — Quantum Random Beacon, Neural Wallet, Interplanetary Sync, Self-Evolving Contracts, AI Consensus, Singularity Chain

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
Version:   v5.7 ✅
Tests:     116/116 pass
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

Next: v5.9 — Benchmark suite: tps, latency, memory
```
