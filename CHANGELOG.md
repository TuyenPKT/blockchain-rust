# CHANGELOG — Blockchain Rust

Ghi lại thay đổi theo từng version. Format: Added / Files / Tests / Gotcha.

---

## v15.7 — PKT Node Server (2026-03-20)

### Added
- `NodeConfig` — port/magic/network/our_height/max_peers, default: 0.0.0.0:64512 (testnet)
- `server_handshake(stream, magic, height)` — server-side PKT handshake: receive Version → send Version+Verack → receive Verack
- `run_pkt_node(cfg)` — TCP listener, spawn thread per peer, max_peers cap
- `handle_peer` — message loop: Ping→Pong, GetHeaders→empty Headers, Inv log, keepalive ping on timeout
- `parse_node_args(args)` — bare port / --port / --mainnet / --height / --max-peers
- CLI: `cargo run -- pkt-node [port] [--mainnet]` → PKT-compatible P2P node

### Files
- `src/pkt_node.rs` — module mới

### Tests
- +21 tests (2023 total)
- Loopback TCP: server_handshake OK, wrong magic → error, height/user_agent/version captured
- Config tests, parse_node_args, ConnectedPeer clone, constant sanity

### Gotcha
- `PktMsg::Inv` và `PktMsg::Headers` là struct variants (named fields `{ items }` / `{ headers }`), không phải tuple variants — dùng `PktMsg::Headers { headers: vec![] }`, không phải `PktMsg::Headers(vec![])`
- Server handshake ngược với client: peer gửi Version trước, server reply Version+Verack+chờ Verack
- `cargo run -- node` vẫn là old custom-protocol node; `cargo run -- pkt-node` mới là PKT wire protocol

---

## v15.6 — Testnet Web Integration (2026-03-20)

### Added
- `testnet_web_router()` — wires `/api/testnet/*` + `/api/testnet/sync-status` vào `pktscan_api::serve()`
- `testnet_web_router_with_dbs(sdb, udb)` — testable variant nhận DB handles từ ngoài
- `home_path(suffix)`, `default_sync_db_path()`, `default_utxo_db_path()` — path helpers
- Graceful degradation: nếu DB chưa có, server trả về JS-only (không crash)
- `/static/testnet.js` — embedded JS: fetchSyncStatus / fetchTestnetStats / fetchTestnetHeaders
- `window.showTestnet()` — page nav function, auto-refresh mỗi 15s
- `renderProgressBar(pct, width)` — ASCII bar trong JS
- `index.html` — nav link "Testnet" + `#testnet-page` div + `<script src="/static/testnet.js">`
- CLI: `cargo run -- testnet-web` → in DB paths + sync status

### Files
- `src/pkt_testnet_web.rs` — module mới
- `frontend/testnet.js` — JS panel mới

### Tests
- +23 tests (2002 total)
- JS content tests (endpoints, functions, IIFE, auto-refresh, DOM IDs)
- Path helper tests (suffix, .pkt component, syncdb vs utxodb)
- Router construction tests với temp DBs (serialized via ROUTER_LOCK)

### Gotcha
- `testnet_web_router()` opens real DBs at `~/.pkt/syncdb` + `~/.pkt/utxodb` — chạy `cargo run -- sync` trước khi start server để có dữ liệu
- Nếu server start trước khi sync: routes `/api/testnet/*` vắng mặt (404), frontend hiện "Not connected"
- `pkt_sync::open_temp()` race condition vẫn còn trong pkt_sync.rs tests — flaky nhưng pass khi rerun

---

## v15.5 — Sync Status UI (2026-03-20)

### Added
- `SyncProgressPhase` — Idle / ConnectingPeer / DownloadingHeaders / ApplyingUtxo / Complete
- `SyncProgress` — snapshot: headers_downloaded, utxo_height, elapsed_secs, blocks_per_sec, event_log
- `SyncProgress::from_dbs(sync_db, utxo_db)` — populate từ RocksDB thật
- `header_progress()`, `utxo_progress()`, `overall_progress()` — weighted (60/40)
- `eta_secs()`, `format_eta()` — ETA string: "10s" / "1m 30s" / "2h 0m" / "synced"
- `format_progress_bar(progress, width)` — ASCII bar dùng █/░
- `format_sync_oneline(p)` — one-liner cho CLI / log
- `sync_status_json(p)` — JSON cho web frontend
- `SyncUiState` + `sync_status_router()` — Axum: GET /api/testnet/sync-status
- `render_sync_progress_panel(frame, area, progress)` — ratatui 3-panel: Gauge + Paragraph + List
- CLI: `cargo run -- sync-status`

### Files
- `src/pkt_sync_ui.rs` — module mới

### Tests
- +55 tests (1979 total)
- TUI tests dùng `TestBackend` — không cần real terminal
- `DB_OPEN_LOCK` mutex serialize 4 from_dbs tests (tránh collision khi mở temp RocksDB song song)

### Gotcha
- `SyncDb::open_temp()` dùng `SystemTime::now()` hash làm path suffix — 2 test chạy cùng nanosecond → cùng path → RocksDB lock conflict
- Fix: `static DB_OPEN_LOCK: Mutex<()>` trong test module để serialize các test gọi `open_temp()`

---

## v15.4 — Explorer Live Data (2026-03-20)

### Added
- Adapter layer: SyncDb (headers) + UtxoSyncDb (UTXOs) → JSON API
- 5 routes mới: `GET /api/testnet/{stats,headers,header/:h,balance/:s,utxos/:s}`
- `query_headers` — list wire headers mới nhất, có pagination (limit/offset)
- `query_header` — single header by height, trả None nếu chưa sync
- `query_utxos` — filter UTXOs theo script_pubkey prefix (hex)
- `query_balance` — tổng balance của một script
- `query_sync_stats` — combined status: height, UTXO count, total value, synced flag
- `UtxoSyncDb::raw_db()` — accessor để iterator qua toàn bộ UTXO entries
- CLI: `cargo run -- explorer-testnet` → in trạng thái sync

### Files
- `src/pkt_explorer_api.rs` — module mới

### Tests
- +36 tests (1924 total)
- Test toàn bộ data layer với temp RocksDB thật, không mock

### Gotcha
- `UtxoSyncDb.db` là private — cần `raw_db()` accessor để iterate từ module ngoài
- `Result<Option<T>>.ok()??` (double `?`) thay vì `.ok()?.flatten()` trong closure `and_then`

---

## v15.3 — UTXO Sync (2026-03-20)

### Added
- Bitcoin wire transaction types: `WireTxIn`, `WireTxOut`, `WireTx`
- `is_coinbase()` — detect null prev_txid (all zeros) + vout=0xffffffff
- `encode_wire_tx / decode_wire_tx` — Bitcoin standard format + segwit marker detection
- `decode_block_txns(payload)` — skip 80-byte header, decode tất cả txns
- `wire_txid(tx)` — SHA256(SHA256(encoded)) → [u8;32]
- `UtxoSyncDb` — RocksDB: insert/remove/get UTXO, height tracking, total_value
- `apply_wire_tx` — coinbase: skip inputs, tạo outputs; normal: spend + tạo
- `apply_block_txns` — apply toàn bộ block, persist height + tip_hash
- `sync_utxos(blocks, resume_from)` — skip đã apply, resume sau restart
- CLI: `cargo run -- utxosync`

### Files
- `src/pkt_utxo_sync.rs` — module mới

### Tests
- +39 tests (1888 total)
- Test decode roundtrip (coinbase/spend/multi-output), resume logic, UtxoSyncDb CRUD

### Gotcha
- Segwit tx có marker byte `0x00` sau version → `in_count == 0` → đọc thêm flag byte, rồi decode như thường
- Coinbase input: prev_txid=[0;32] AND prev_vout=0xffffffff — phải check cả hai

---

## v15.2 — Block Download (2026-03-20)

### Added
- `compact_target_to_bytes(bits)` — decode Bitcoin nBits → 32-byte big-endian target
- `hash_meets_target(hash, target)` — so sánh big-endian (reverse hash trước)
- `validate_chain_links(headers, prev_hash)` — kiểm tra prev_block linkage
- `validate_header_batch` — links + PoW (skip với `skip_pow_check=true`)
- `build_locator(known_hashes)` — Bitcoin block locator: dense ở tip, log2 về genesis
- `SyncDb` — RocksDB lưu raw 80-byte headers: `wireheader:{h:016x}`
- `send_getheaders / recv_headers / send_getdata_blocks` — wire I/O helpers
- `sync_headers()` — loop GetHeaders → validate → save, resume được
- `SyncConfig.regtest(path)` — skip PoW + short timeout cho tests
- CLI: `cargo run -- sync`

### Files
- `src/pkt_sync.rs` — module mới

### Tests
- +59 tests (1849 total)
- Loopback TCP: send GetHeaders → server trả Headers → verify saved to DB

### Gotcha
- Bitcoin hash comparison: hash phải `.reverse()` trước khi so với target (Bitcoin lưu LE)
- `SyncDb` auto-cleanup temp dirs trong `Drop` (path chứa `pkt_syncdb_test_`)
- `SyncConfig` không implement `Copy` — clone khi cần pass vào thread

---

## v15.1 — Testnet Peer Connect (2026-03-20)

### Added
- `PeerConfig` — host/port/magic/timeout/retries/backoff, default: testnet-seed.pkt.cash:64765
- `HandshakeState` — Idle → SentVersion → ReceivedVersion → Complete / Failed
- `backoff_delay(attempt, base, max)` — exponential backoff với cap
- `do_handshake` — Version → Verack exchange theo Bitcoin protocol
- `ping_pong` — gửi Ping, chờ Pong, trả lời Ping của peer trong khi chờ
- `connect_once` — TCP connect + DNS resolve + handshake
- `connect_with_retry` — retry vô hạn (max=0) hoặc N lần
- `PeerError` — Connect/Io/Handshake/Timeout/Disconnected
- CLI: `cargo run -- peer [host:port] [--mainnet] [--retries N]`

### Files
- `src/pkt_peer.rs` — module mới

### Tests
- +63 tests (1790 total)
- Loopback TCP (127.0.0.1:0): handshake, ping/pong, wrong magic → fails, retry exhausted

### Gotcha
- `encode_message` nhận `&PktMsg` và `&[u8; 4]` (không phải owned) — cần borrow cả hai
- DNS resolve dùng `ToSocketAddrs` — hỗ trợ cả hostname lẫn IP literal
- `TcpStream::connect_timeout` cần `SocketAddr` (resolved), không nhận `&str`

---

## v15.0 — PKT Wire Protocol (2026-03-19)

### Added
- Bitcoin P2P wire format cho PKT network
- `TESTNET_MAGIC = [0x0b, 0x11, 0x09, 0x07]`, `MAINNET_MAGIC = [0xcb, 0xf2, 0xc0, 0xef]`
- `PROTOCOL_VERSION = 70015`
- `encode_varint / decode_varint` — Bitcoin compact int (1/3/5/9 bytes)
- `encode_varstr / decode_varstr` — VarInt-prefixed UTF-8
- `checksum(payload)` — SHA256(SHA256(payload))[0..4], `EMPTY_CHECKSUM`
- `MsgHeader` — magic/command(12-byte)/length/checksum
- `VersionMsg` — handshake message với user_agent, start_height, relay
- `InvItem` — inventory item (type + 32-byte hash)
- `WireBlockHeader` — 80-byte wire format: to_bytes()/from_bytes()/block_hash()
- `PktMsg` enum — Version/Verack/Ping/Pong/Inv/GetData/GetHeaders/Headers/Unknown
- `encode_message / decode_message` — full roundtrip

### Files
- `src/pkt_wire.rs` — module mới

### Tests
- +47 tests (1727 total)
- Roundtrip tất cả message types, checksum verify, varint mọi range

### Gotcha
- `Unknown` variant dùng `Box::leak` để convert runtime string → `&'static str` cho `encode_payload`
- `EMPTY_CHECKSUM = [0x5d, 0xf6, 0xe0, 0xe2]` — SHA256d của empty payload (pre-computed)
- `command_bytes(name)` — null-pad đến 12 bytes, truncate nếu dài hơn

---

## v16.3 — Hot Reload Dev Mode (2026-03-19)

### Added
- `run_dev(config)` — watch `src/`, debounce 300ms, rebuild + restart tự động
- `DevConfig` — watch_dir/port/cmd/debounce_ms
- `run_cargo_build()` — spawn subprocess, capture stderr, parse error count
- `list_watch_files(dir)` — recursive .rs scan, sorted
- `spawn_server / kill_server` — manage child process
- `ReloadEvent` — FileChanged/BuildSuccess/BuildFailure/ServerRestarted
- CLI: `cargo run -- dev [--watch DIR] [--port PORT] [--cmd CMD]`

### Files
- `src/hot_reload.rs` — module mới

### Tests
- +39 tests (1680 total)
- Dùng real filesystem: list_watch_files("src") verify ≥50 .rs files, sorted

### Gotcha
- `notify = "6"` crate cho filesystem watching — thêm vào `Cargo.toml`
- CPU-bound rebuild không block async executor: dùng `tokio::task::block_in_place`

---

## v16.2 — Integration Test Harness (2026-03-19)

### Added
- `TestNode` — mine/balance/height/send/start_api qua real chain + API
- `static PORT_SEQ: AtomicU16 = AtomicU16::new(47000)` — unique port per test
- E2E tests: mine → balance → send → confirm → API verify
- Feature-gated: `--features integration`

### Files
- `src/integration_test.rs` — module mới

### Tests
- +24 integration tests (chạy với `cargo test --features integration`)

### Gotcha
- `.send(...).expect(...)` sai — phải `.send(...).await.expect(...)`
- API trả `"index"` không phải `"height"` cho block detail
- `/api/address/:addr` dùng `pubkey_hash_hex`, không phải wallet address string

---

## v16.1 — Dev Docs Generator (2026-03-19)

### Added
- `generate_api_md()`, `generate_cli_md()`, `generate_arch_md()` → markdown strings
- 41 API endpoints, 26 CLI commands, 55 modules documented
- `write_docs(out_dir)` → tạo dir, ghi 3 files
- CLI: `cargo run -- docs [--out DIR]`

### Files
- `src/docs_gen.rs` — module mới

### Tests
- +43 tests (1641 total)

---

## v16.0 — Devnet One-Command (2026-03-19)

### Added
- `cargo run -- devnet` → node + miner + API một process
- `DevnetConfig` — api_port/blocks/difficulty/mine_interval_ms
- `fresh_devnet_db(difficulty)` — ScanDb sạch, không load từ disk
- `run_devnet_async()` — mine real blocks + spawn pktscan API
- CLI flags: `--port/-p`, `--blocks/-n`, `--difficulty/-d`, `--interval`

### Files
- `src/devnet.rs` — module mới

### Tests
- +36 tests (1598 total)
- Mine real blocks difficulty=1, assert height/balance/chain.is_valid()

### Gotcha
- `block_in_place` cần `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`
- Không dùng `unwrap()` / `panic` trong prod code
