# CHANGELOG — Open Consensus Execution Interface Framework

Ghi lại thay đổi theo từng version. Format: Added / Files / Tests / Gotcha.

---

## v22.5 — Wallet Send TX (2026-03-29)

### Added
- `wallet_tx_build(privkey_hex, inputs, to_addr, amount_sat, fee_sat, change_addr)` Tauri command:
  - BIP143 segwit v0 P2WPKH transaction signing (ECDSA secp256k1)
  - Tự tính hashPrevouts, hashSequence, hashOutputs, scriptCode
  - Serialize raw segwit tx (version + marker 0x00 + flag 0x01 + inputs + outputs + witnesses + locktime)
  - Tính txid (non-witness double-sha256)
  - Helpers: `double_sha256`, `write_varint`, `write_tx_output`, `addr_to_p2wpkh_script`
- `tx_broadcast(node_url, raw_hex)` Tauri command — POST `/api/testnet/tx/broadcast`
- `Wallet.tsx` — Send panel: to address, amount (PKT), fee (PKT), greedy UTXO auto-select, sign & broadcast
- `AddressUtxo.script_pubkey` field thêm vào interface

### Files
- `desktop/src-tauri/src/lib.rs` — `wallet_tx_build`, `tx_broadcast`, helpers
- `desktop/src/pages/Wallet.tsx` — Send panel, `handleSend()`
- `desktop/src/api.ts` — `AddressUtxo.script_pubkey` field
- `desktop/src/i18n.ts` — send strings (en + vi)

### Tests
- +0 (build verification only)

### Breaking / Gotcha
- UTXO `script_pubkey` từ backend là hex P2WPKH (22 bytes `0014{hash160}`) — wallet chỉ hỗ trợ P2WPKH input
- txid display = reverse của double_sha256(legacy serialization) — khác với wtxid

---

## v22.4 — Broadcast TX Endpoint (2026-03-29)

### Added
- `POST /api/testnet/tx/broadcast` — nhận `{raw_hex}`, parse WireTx, relay lên testnet peer qua P2P
- Connect → handshake → inv + tx wire message
- 502 + txid nếu relay thất bại (txid vẫn trả về để client retry)

### Files
- `src/pkt_testnet_web.rs` — `ps_tx_broadcast` handler, route `post`

### Tests
- +0

---

## v22.3 — Rich List Alias (2026-03-29)

### Added
- Route `/api/testnet/richlist` alias cho `/api/testnet/rich-list` (đã thêm trong v22.0)

---

## v22.2 — Block TX Count (2026-03-29)

### Added
- `SyncDb::save_block_tx_count(height, count)` + `get_block_tx_count(height)` — key `txcount:{height:016x}`
- `sync_blocks` loop gọi `save_block_tx_count` sau mỗi block apply
- `ps_block_detail` dùng stored tx_count, fallback = `txids.len()`

### Files
- `src/pkt_sync.rs` — 2 methods mới
- `src/pkt_block_sync.rs` — save tx_count trong sync loop
- `src/pkt_testnet_web.rs` — dùng stored count

### Tests
- +0

---

## v22.1 — UTXO Height Field (2026-03-29)

### Added
- `#[serde(default)] pub height: u64` vào `UtxoEntry` — backward-compatible với data cũ (height=0)
- `insert_utxo(txid, vout, out, height)` signature update
- `apply_wire_tx(db, tx, txid, height)` signature update
- `apply_block_txns` truyền `height` xuống

### Files
- `src/pkt_utxo_sync.rs` — UtxoEntry, insert_utxo, apply_wire_tx, apply_block_txns
- `src/pkt_block_sync.rs` — sync_blocks caller
- `src/pkt_reorg.rs` — callers
- `src/pkt_addr_index.rs` — test callers

### Tests
- Existing tests updated (+0 new)

---

## v22.0 — Address Index Fix (2026-03-29)

### Added
- `any_addr_to_script_hex(addr)` — unified bech32/Base58Check/script_hex → script_hex conversion
- `hash160_to_script_hex(hash160)` helper
- `ps_balance` + `ps_addr_txs` dùng `any_addr_to_script_hex`; balance response thêm `balance_pkt`
- `ps_addr_utxos` handler mới + route `/api/testnet/address/:s/utxos`
- `/api/testnet/richlist` alias
- `AddrTxsParams.page` field

### Files
- `src/pkt_testnet_web.rs`

---

## v21.2 — Node Manager + Wallet Tab (2026-03-29)

### Added
- Terminal tab → Wallet tab (create/import/remove wallet, balance, keys reveal)
- Node tab: real P2P peer scan via `peer_scan(seed_addr)` IPC
- `peer_scan`: TCP GetAddr → probe ≤20 peers parallel (latency + height) → sort online-first
- `wallet_generate(network)`: secp256k1 keypair + bech32 address
- `wallet_from_privkey(privkey_hex, network)`: restore pubkey + address
- localStorage wallet persistence (`pktscan_wallet`)
- Private key hidden by default, reveal toggle

### Files
- `desktop/src/pages/Wallet.tsx` (new)
- `desktop/src/pages/Node.tsx` — peer scan
- `desktop/src/App.tsx` — wallet tab
- `desktop/src/components/Nav.tsx` — wallet tab
- `desktop/src-tauri/src/lib.rs` — peer_scan, wallet commands, Base58Check decoder
- `desktop/src-tauri/Cargo.toml` — secp256k1, sha2, ripemd
- `desktop/src/i18n.ts` — wallet + peers strings

---

## v21.1 — Light/Dark Mode Fix + i18n Complete (2026-03-29)

### Added
- i18n hoàn chỉnh Miner.tsx + Node.tsx
- Fix light mode: `SearchBar.tsx` module-level `TYPE_COLOR` → `typeColor()` function
- Fix dark mode: `RichList.tsx` FeeHistogram canvas deps thêm colors
- Fix `MiniChart.tsx` canvas deps thêm `colors.border`
- Fix Tauri build: xóa `"theme": "auto"` (Tauri v2 không hỗ trợ)
- Fix Base58 address validation trong miner (decode_base58check)
- Mine log persist khi chuyển tab (Miner always mounted via CSS display:none)

### Files
- `desktop/src/pages/Miner.tsx`
- `desktop/src/pages/Node.tsx`
- `desktop/src/components/SearchBar.tsx`
- `desktop/src/components/MiniChart.tsx`
- `desktop/src/pages/RichList.tsx`
- `desktop/src-tauri/tauri.conf.json`
- `desktop/src-tauri/src/lib.rs`
- `desktop/src/App.tsx`

---

## v21.0 — Real Miner IPC (2026-03-28)

### Added
- `start_mine(address, node_addr, threads)` IPC command — spawn Rust background thread với real blake3 PoW miner:
  - Decode địa chỉ PKT (bech32 `tpkt1...`/`pkt1...` hoặc hex pubkey_hash)
  - TCP `GetTemplate` → node trả `prev_hash`, `height`, `difficulty`, `txs`
  - Build coinbase TX JSON (re-implement `calculate_txid`/`wtxid` + Merkle từ main crate)
  - Rayon parallel mining: check MINER_STOP flag mỗi 50k hashes
  - TCP `NewBlock` submit khi block found
  - Emit `mine_log` (string) + `mine_stats` (hashrate, blocks_mined, uptime_secs) mỗi 800ms
- `stop_mine()` IPC command — set MINER_STOP AtomicBool, rayon workers thoát ≤50k hashes
- `mine_status()` IPC command — trả về bool
- `Miner.tsx` rewrite — địa chỉ input, node addr input, threads slider; `invoke("start_mine")`; `listen("mine_log")` + `listen("mine_stats")`; real hashrate + blocks mined; auto-scroll log; sync status on mount via `mine_status()`
- Dependencies desktop: `blake3 = "1.5"`, `rayon = "1.10"`, `hex = "0.4"`

### Files
- `desktop/src-tauri/src/lib.rs` — added miner commands + helpers (bech32 decode, coinbase txid, merkle, submit TCP)
- `desktop/src-tauri/Cargo.toml` — added blake3, rayon, hex
- `desktop/src/pages/Miner.tsx` — rewrite (real IPC, không còn fake simulation)

### Tests
- +0 tests (IPC miner cần node thật, không thể unit test)

### Breaking / Gotcha
- `use tauri::Emitter` cần import rõ ràng trong Tauri v2 để dùng `app.emit()`
- Coinbase txid tái tạo format Debug của Rust: `"txid|[(\"<zeros64>\", height, 4294967295)]|[amount]|true"` — phải match chính xác với `Transaction::calculate_txid()` trong main crate
- `MINER_STOP` là `static AtomicBool` → tất cả rayon threads đọc trực tiếp, không cần Arc
- Bech32 decode: strip witness version (byte 0) + 6 checksum bytes trước convertbits

---

## v20.9 — Build & Release (2026-03-28)

### Added
- `tauri.conf.json` — version bump 0.8.0; identifier `com.oceif.pktscan`; bundle metadata (category, description); macOS minimumSystemVersion 10.15; Windows digest sha256; Linux deb + AppImage config
- `.github/workflows/release.yml` — multi-platform CI triggered on tag `v*.*.*`:
  - `macos-latest` → `universal-apple-darwin` (.dmg, signed nếu có Apple secrets)
  - `windows-latest` → `x86_64-pc-windows-msvc` (.msi + .exe)
  - `ubuntu-22.04` → `x86_64-unknown-linux-gnu` (.AppImage + .deb)
  - Dùng `tauri-apps/tauri-action@v0`, `swatinem/rust-cache@v2`, `dtolnay/rust-toolchain@stable`
  - GitHub Release tự động với release body table (platform/file/notes)
  - Optional Apple code signing + notarization qua secrets
- `.github/workflows/build-check.yml` — PR/push check 3 jobs:
  - `typescript` — `tsc --noEmit` trên ubuntu
  - `rust` — `cargo build + test -p pktscan-desktop` với frontend build trước
  - `rust-main` — `cargo test --workspace --exclude pktscan-desktop`
  - Chỉ chạy khi path `desktop/**` hoặc `src/**` thay đổi
- `desktop/RELEASE.md` — hướng dẫn đầy đủ: yêu cầu hệ thống, build thủ công, CI secrets, version bump checklist, dev mode

### Files
- `desktop/src-tauri/tauri.conf.json` — updated
- `.github/workflows/release.yml` — mới
- `.github/workflows/build-check.yml` — mới
- `desktop/RELEASE.md` — mới

### Tests
- TypeScript strict 0 errors · Rust build 0 warnings

---

## v20.8 — Settings & Preferences (2026-03-28)

### Added
- `useSettings` hook — `AppSettings` type (nodeUrlTestnet/Mainnet, currency, language, theme, pollInterval); load/save localStorage; `update(patch)` + `reset()`
- `Settings` page — overlay khi click ⚙ icon trong Nav:
  - **Network section** — Testnet URL + Mainnet URL với `UrlInput` + Test button (AbortController 5s timeout, trạng thái ✓ Online / ✗ Offline)
  - **Appearance section** — Theme segmented (Dark/Light) + Currency segmented (PKT/USD)
  - **Language section** — EN / Tiếng Việt segmented
  - **Poll interval** — 5 options: 4s/8s/15s/30s/60s button group
  - **About section** — version, stack badges, source URL
  - **Danger Zone** — Reset với confirm 2 bước
  - Auto-save indicator "✓ Saved" toast sau mỗi thay đổi
- `theme.ts` — export `dark` + `light` token sets; `applyTheme(mode)` function; `colors` mutable export
- `Nav.tsx` — ⚙ Settings icon button (highlight khi settingsOpen); thêm `onSettings` + `settingsOpen` props
- `App.tsx` — `useSettings()` tích hợp; `nodeUrl` lấy từ `settings.nodeUrl{Testnet|Mainnet}`; `applyTheme()` gọi trong `useEffect`; Settings overlay thay thế main content khi `settingsOpen`

### Files
- `src/hooks/useSettings.ts` — mới
- `src/pages/Settings.tsx` — mới
- `src/theme.ts` — thêm light tokens + `applyTheme`
- `src/components/Nav.tsx` — ⚙ button + props
- `src/App.tsx` — settings integration

### Tests
- TypeScript strict 0 errors · Rust build 0 warnings

---

## v20.7 — Rich List & Mempool UI (2026-03-28)

### Added
- `RichList` page (tab mới "Rich List" trong Nav):
  - **Summary bar** — 4 stat cards: Top Holders, Total Supply, Mempool Count, Mempool Fees
  - **Leaderboard** — rank/avatar/address/balance/pct/share-bar; avatar: 🥇🥈🥉 top 3, deterministic HSL color avatar cho rank khác; click → Address detail; animated balance bar gradient
  - **Fee Histogram** — canvas thuần: bucket fee rate (sat/byte) <1/1-5/5-10/10-50/50-100/100+, gradient fill, grid lines, count label above bar
  - **Mempool TX table** — TXID/Fee/Rate badge (green/amber/red)/Size/In⁻Out/Age; click row → TxDetail
  - Tab switcher: Rich List / Mempool với count badge, fadeIn animation
- 2 IPC commands mới: `get_rich_list(limit)`, `get_mempool(limit)`
- `RichHolder`, `MempoolTx` interfaces + `fetchRichList`, `fetchMempool` trong `api.ts`
- Nav — tab "Rich List" thêm giữa Charts và Miner

### Files
- `src/pages/RichList.tsx` — mới
- `src-tauri/src/lib.rs` — +2 IPC commands
- `src/api.ts` — +2 functions, +2 interfaces
- `src/components/Nav.tsx` — thêm tab richlist
- `src/App.tsx` — routing richlist + import RichList

### Tests
- TypeScript strict 0 errors · Rust build 0 warnings

---

## v20.6 — Block & TX Detail UI (2026-03-28)

### Added
- `BlockDetail` page:
  - Hero: block height, hash, confirmation badge (green ≥6, amber <6, red=0), chips (txs/time/fee)
  - Metadata panel: height/hash/prev_hash/timestamp/txs/size/difficulty/miner/total_fees
  - TX list: click row → TxDetail; hiển thị TXID rút gọn, inputs/outputs count, fee badge, total PKT output
  - Fallback: nếu server chỉ trả txids[], render danh sách clickable
- `TxDetail` page:
  - Hero: full txid, confirmation badge, fee rate badge (Low/Medium/High sat/byte), block height, timestamp, size
  - **Flow bar**: Total Input → Total Output + Fee (3 cột)
  - **Inputs panel**: coinbase tag, txid:vout, address button → navigate Address; amount
  - **Outputs panel**: address button → navigate Address; type label; amount màu green
- `Blocks.tsx` — click row → `onBlock(height)` navigate BlockDetail
- `App.tsx` — tab ẩn `block-detail` + `tx-detail`; `goBlock/goTx/goAddr` helpers; back-tab tracking
- `SearchBar.tsx` — Tab type thêm `block-detail | tx-detail`; tx type → `explorer` (App handle)
- 2 IPC commands mới: `get_block_detail(height)`, `get_tx_detail(txid)`

### Files
- `src/pages/BlockDetail.tsx` — mới
- `src/pages/TxDetail.tsx` — mới
- `src/pages/Blocks.tsx` — thêm `onBlock` prop + click handler
- `src/App.tsx` — routing block-detail/tx-detail
- `src/components/SearchBar.tsx` — Tab type expanded
- `src-tauri/src/lib.rs` — +2 IPC commands
- `src/api.ts` — +2 functions, +4 interfaces (TxInput/TxOutput/TxDetail/BlockDetail)

### Tests
- TypeScript strict 0 errors · Rust build 0 warnings

---

## v20.5 — Address Detail UI (2026-03-28)

### Added
- `Address` page (rewrite hoàn toàn):
  - **Balance Hero** — animated PKT balance (ease-out cubic), accent glow bar, address truncated
  - **Copy button** — clipboard API, feedback "✓ Copied" 1.8s
  - **QR modal** — blur backdrop, deterministic pixel pattern từ address chars, full address monospace, Copy button
  - **TX History table** — cột TXID/Height/Amount/Type/Time; amount màu green/red theo dấu; pagination cursor-based (Prev/Next, tổng số txs)
  - **UTXO list** — cột TXID/Vout/Amount/Height, tổng UTXO PKT ở header
  - **Tab switcher** — Transactions / UTXOs với count badge
  - **Back button** — quay về Explorer
- 2 IPC commands mới: `get_address_txs(page, limit)`, `get_address_utxos`
- `fetchAddressTxs`, `fetchAddressUtxos`, `AddressTx`, `AddressUtxo` trong `api.ts`
- `App.tsx` — tab ẩn "address"; `handleNavigate` phân biệt type=address → `setSelectedAddress` + tab
- `SearchBar.tsx` — `tabForType("address")` → `"address"` (thay vì explorer)

### Files
- `src/pages/Address.tsx` — rewrite
- `src-tauri/src/lib.rs` — +2 IPC commands
- `src/api.ts` — +2 functions, +2 interfaces
- `src/App.tsx` — +address tab, selectedAddress state
- `src/components/SearchBar.tsx` — address routing

### Tests
- TypeScript strict 0 errors · Rust build 0 warnings

---

## v20.4 — Search & Navigation (2026-03-28)

### Added
- `useSearch` hook — debounced search (320ms), localStorage recent history (max 8), auto type detection: block/tx/address/unknown
- `SearchBar` component — Cmd+K / Ctrl+K global modal overlay:
  - Blur backdrop, centered modal
  - Input với SVG icon, loading indicator, ESC hint
  - Dropdown: Recent searches (với nút Clear) hoặc Live results với TypeBadge (Block/TX/Addr)
  - Keyboard nav: ↑↓ cursor, Enter select, Escape đóng
  - Tự navigate sang tab phù hợp (block → Blocks, address/tx → Explorer)
- `SearchTrigger` button trong Nav — compact với ⌘K / Ctrl+K hint
- `detectType(q)` util — regex detect: number→block, 64-hex→tx, p[a-z0-9]→address
- Xóa SearchBar inline cũ trong Explorer.tsx — chuyển sang global instance trong App.tsx

### Files
- `src/hooks/useSearch.ts` — hook mới
- `src/components/SearchBar.tsx` — rewrite hoàn toàn
- `src/components/Nav.tsx` — thêm `onSearchOpen` prop + `SearchTrigger`
- `src/App.tsx` — mount global `<SearchBar>`, truyền `handleNavigate`
- `src/pages/Explorer.tsx` — xóa SearchBar cục bộ

### Tests
- TypeScript strict 0 errors · Rust build 0 warnings

---

## v20.3 — Charts & Analytics (2026-03-28)

### Added
- `get_analytics` Tauri IPC command — fetch `hashrate | block_time | difficulty` từ `/api/testnet/analytics`
- `MiniChart` component — canvas sparkline thuần (không thư viện): filled area gradient, grid lines, last-point dot, devicePixelRatio aware
- `Charts` page — 3 charts: Hashrate (blue), Block Time (green), Difficulty (purple full-width)
  - Window selector: 50 / 100 / 200 / 500 blocks
  - Min / Avg / Max stats per chart
  - X-axis height labels cho difficulty chart
- Tab "Charts" thêm vào Nav (giữa Blocks và Miner)
- `fetchAnalytics()` typed wrapper trong `api.ts`

### Files
- `src/components/MiniChart.tsx`
- `src/pages/Charts.tsx`
- `src-tauri/src/lib.rs` — thêm `get_analytics` command
- `src/api.ts` — thêm `fetchAnalytics`, `AnalyticsPoint`, `AnalyticsSeries`

### Tests
- TypeScript strict 0 errors · Rust build sạch

---

## v20.2 — Live Dashboard (2026-03-28)

### Added
- `hooks/useLiveDashboard.ts` — poll 8s, detect block mới qua height diff, emit `LiveEvent[]`, connected/error state
- `hooks/useAnimatedNumber.ts` — animate số tăng/giảm với ease-out cubic, requestAnimationFrame
- `Explorer.tsx` rewrite:
  - `LiveStat` card — animated counter + glow line top + pulse dot khi connected
  - `ConnBadge` — Live/Offline indicator với pulse animation
  - `BlockRow` — slide-in animation khi block mới xuất hiện
  - `EventRow` — live event feed, slide-in animation
  - Status bar: connection, node URL, poll interval, difficulty, UTXO count
- Poll 8s (thay 15s), auto-detect block mới, không duplicate event

### Files
- `src/hooks/useLiveDashboard.ts`
- `src/hooks/useAnimatedNumber.ts`
- `src/pages/Explorer.tsx` — rewrite

### Tests
- TypeScript strict, 0 errors

---

## v20.1 — React UI Foundation (2026-03-28)

### Added
- **Nav** — fixed top bar, 4 tabs (Explorer/Blocks/Address/Terminal), Mainnet/Testnet toggle
- **Explorer** — stats cards (Height/Hashrate/BlockTime/Mempool), Latest Blocks panel, Event Log panel, Network info bar, auto-refresh 15s
- **Blocks** — block table: height, hash, tx count, time ago; hover highlight; Refresh button
- **Address** — address lookup form, balance hero card, raw JSON
- **Terminal** — interactive CLI: summary/blocks/balance/search commands, history (↑↓), color-coded output
- **SearchBar** — global search, Cmd+K shortcut
- `theme.ts` — color tokens, font tokens
- `api.ts` — typed IPC wrappers + fmtHashrate/shortHash/timeAgo utils
- TypeScript strict, 0 errors

### Files
- `src/theme.ts`, `src/api.ts`
- `src/components/Nav.tsx`, `StatCard.tsx`, `Panel.tsx`, `SearchBar.tsx`
- `src/pages/Explorer.tsx`, `Blocks.tsx`, `Address.tsx`, `Terminal.tsx`
- `src/App.tsx` — routing qua tab state

### Dev
```bash
cd desktop && npm run tauri dev
```

---

## v20.0 — Tauri Desktop Scaffold (2026-03-28)

### Added
- `desktop/src-tauri/` — Rust Tauri v2 backend
  - `src/lib.rs` — 4 IPC commands: `get_summary`, `get_blocks`, `get_balance`, `search`
  - `src/main.rs` — entry point (windows_subsystem = "windows" cho release)
  - `build.rs` — tauri-build
  - `tauri.conf.json` — window 1280×800, bundle targets all
  - `capabilities/default.json` — core:default permissions
  - `icons/` — 32x32, 128x128 (RGBA PNG), icon.icns, icon.ico
- `desktop/src/` — React + TypeScript frontend stub
  - `main.tsx` — ReactDOM.createRoot entry
  - `App.tsx` — Fetch Network Summary button, gọi IPC `get_summary`, hiển thị JSON
- `desktop/index.html`, `package.json`, `vite.config.ts`, `tsconfig.json`
- Root `Cargo.toml` workspace: thêm `desktop/src-tauri`
- IPC bridge: React `invoke("get_summary", { nodeUrl })` → Rust `reqwest` → PKTScan API

### Files
- `desktop/src-tauri/src/lib.rs` — Rust IPC commands
- `desktop/src-tauri/src/main.rs`
- `desktop/src/App.tsx`, `desktop/src/main.tsx`

### Tests
- +3 tests (`test_base_trims_trailing_slash`, `test_base_no_slash`, `test_client_builds`)
- Build sạch (0 warnings)

### Dev
```bash
cd desktop && npm install && npm run tauri dev
```

---

## v19.9 — Developer Portal (2026-03-28)

### Added
- `web/dev/index.html` — landing page cho developers: hero, quickstart 4 bước, API endpoint table, rate limits, API key roles, webhook verify guide
- Quickstart: curl examples, SDK install + TypeScript snippet, WebSocket subscribe, webhook đăng ký
- Endpoint table: 15 endpoints với method badge, path, auth requirement
- Rate limit cards: Anonymous (60/min) · read key (600/min) · write key (120/min)
- API key section: 3 roles (read/write/admin) + hướng dẫn lấy key
- Node.js webhook verify snippet với HMAC-SHA256
- Route `GET /dev` trong `src/web_serve.rs`
- Link **Developers** thêm vào nav toàn bộ 9 pages (8 sub-pages + index.html)

### Files
- `web/dev/index.html` — ~250 lines HTML + inline CSS + inline JS (copyCode)
- `src/web_serve.rs` — thêm `serve_dev_page()` + route `/dev`

### Tests
- Build sạch (0 warnings)

---

## v19.9.1 — REST API Key Management (2026-03-28)

### Added
- `src/key_api.rs` — REST endpoints quản lý API key (yêu cầu admin role):
  - `GET /api/keys` — liệt kê tất cả keys (id, role, label, created_at) — không trả raw key
  - `POST /api/keys` — tạo key mới (body: label?, role?), trả raw key 1 lần
  - `DELETE /api/keys/:key_id` — thu hồi key theo key_id
- `AuthDb` clone truyền vào `key_router()` trước khi bị move vào auth middleware
- Dev Portal endpoint table cập nhật 3 dòng mới (GET/POST/DELETE /api/keys)

### Files
- `src/key_api.rs` — module mới

### Tests
- +6 tests (key_api, tất cả pass)
- Build sạch (0 warnings)

---

## v19.8 — Webhook UI (2026-03-28)

### Added
- `web/webhooks/index.html` — trang quản lý webhook: đăng ký, xem danh sách, xoá
- `web/js/webhooks.js` — logic đầy đủ: POST/GET/DELETE `/api/webhooks` với auth header
- API key input (show/hide) + badge xác nhận khi key hợp lệ
- Form đăng ký: URL, checkboxes events (new_block / new_tx / address_activity), address filter tuỳ chọn
- Secret reveal box — hiển thị 1 lần sau khi register, có nút Copy + Dismiss
- Webhook list: card mỗi webhook, event tags, address filter badge, timestamp, nút Xoá
- Toast notifications (ok/err) cho mọi thao tác
- Signature verify guide + Node.js code snippet inline
- Route `GET /webhooks` trong `src/web_serve.rs`

### Files
- `web/webhooks/index.html` — HTML + inline CSS
- `web/js/webhooks.js` — ~190 lines
- `src/web_serve.rs` — thêm `serve_webhooks_page()` + route `/webhooks`

### Tests
- Build sạch (0 warnings)

---

## v19.7 — API Playground (2026-03-27)

### Added
- `web/playground/index.html` — interactive API playground page
- `web/js/playground.js` — 15 endpoints, param inputs, Run button, JSON highlight
- JSON syntax highlight: key (blue), string (green), number (orange), bool (purple), null (muted)
- Badge: HTTP status + latency ms sau mỗi request
- Copy URL (absolute) + Copy Response buttons
- Share / bookmark qua URL hash: `#ep=block&height=100`
- Keyboard shortcut: `Ctrl+Enter` = Run
- Route `GET /playground` trong `src/web_serve.rs`

### Files
- `web/playground/index.html` — HTML shell, inline playground CSS
- `web/js/playground.js` — logic hoàn chỉnh (~200 lines)
- `src/web_serve.rs` — thêm `serve_playground()` + route

### Tests
- Build sạch (0 warnings)

---

## v19.6 — PKT CLI (2026-03-27)

### Added
- Binary `pkt` — CLI tool query PKTScan từ terminal
- Commands: `block <height>`, `tx <txid>`, `address <addr>`, `mempool`, `sync-status`
- Command `config show` / `config set-node <url>`
- Flag `--json` output raw JSON; mặc định pretty table
- Flag `--node <url>` override node URL per-invocation
- Config `~/.pkt/cli.toml` — lưu `node_url`, tự tạo nếu chưa có

### Files
- `crates/pkt-cli/Cargo.toml` — clap 4, reqwest 0.12 blocking, toml 0.8
- `crates/pkt-cli/src/main.rs` — CLI dispatch, pretty-print, tests
- `crates/pkt-cli/src/config.rs` — CliConfig, load/save, config_path
- `Cargo.toml` — thêm `crates/pkt-cli` vào workspace

### Tests
- +14 tests (config × 4, CLI parse × 5, conversion × 2, print smoke × 3)

---

## v19.5.1 — JS/TS SDK (2026-03-27)

### Added
- `sdk-js/` — npm package `@pktcore/sdk v0.1.0` (TypeScript strict):
  - `PktClient(baseUrl)` — REST + RPC + WebSocket client:
    - Block: `getBlock(height)`, `getBlocks(page, limit)`
    - TX: `getTx(txid)`, `getRecentTxs()`
    - Address: `getAddress()`, `getUtxos()`, `getAddressTxs()`, `exportAddressCsvUrl()`
    - Network: `getSummary()`, `getSyncStatus()`, `getMempool()`, `getFeeHistogram()`, `getAnalytics()`, `getRichList()`, `getLabel()`, `getHealth()`, `search()`
    - JSON-RPC: `rpc(method, params)`, `getBlockCount()`, `getBlockHash()`, `getMiningInfo()`
    - WebSocket: `subscribe(event, cb)` → `Unsubscribe`; auto-reconnect 3s
  - `types.ts` — Block, Tx, TxInput, TxOutput, AddressInfo, Utxo, NetworkSummary, SyncStatus, MempoolTx, HealthStatus, RpcRequest/Response, WsEvent, ...
  - `utils.ts` — `pakletsToPkt`, `pktToPaklets`, `fmtPkt`, `fmtHashrate`, `shortHash`, `shortAddr`, `timeAgo`
  - `PktApiError(status, message)` — typed error class
  - Build: `npm run build` → `dist/` (JS + `.d.ts` + sourcemaps)

### Files
- `sdk-js/src/index.ts` — re-export all
- `sdk-js/src/client.ts` — PktClient
- `sdk-js/src/types.ts` — TypeScript interfaces
- `sdk-js/src/utils.ts` — utility functions
- `sdk-js/package.json` + `tsconfig.json`

---

## v19.5 — libp2p Transport (2026-03-27)

### Added
- `src/pkt_libp2p.rs` — libp2p P2P transport layer (song song với pkt_node/pkt_sync):
  - `PktP2pNode` — Swarm với TCP + Noise (X25519) + Yamux + mDNS + Identify + Ping
  - `PeerManager` — score-based reputation: auto ban khi score < −100 (1h)
  - `ScoreEvent` — +10 ValidBlock, +5 Connected, −10 Timeout, −20 Spam, −50 InvalidData
  - `PeerInfo { addr, score, banned_until, first_seen }`
- `Cargo.toml` — thêm `libp2p = "0.53"` + `futures = "0.3"`

### Files
- `src/pkt_libp2p.rs` — file mới
- `src/main.rs` — thêm `mod pkt_libp2p;`
- `Cargo.toml` — thêm libp2p + futures

### Tests
- +15 tests: score deltas, add/remove/count, auto-ban, active_peers, cumulative events

---

## v19.4 — Cross-Compile Workflow (2026-03-27)

### Added
- `scripts/build-linux.sh` — cross-compile static Linux x86_64 binary (musl):
  - Ưu tiên `cross` (Docker-based) → fallback `native musl target + musl-cross linker`
  - Hướng dẫn cài đặt rõ ràng khi thiếu toolchain
  - In SHA256 + size sau khi build thành công
- `scripts/deploy.sh` — deploy lên VPS oceif.com:
  - `--source` (default): rsync source → `cargo build --release` trên server → restart services
  - `--binary`: gọi `build-linux.sh` → `scp` binary → restart services
  - Config qua env: `PKT_SERVER`, `PKT_USER`, `PKT_REMOTE`
  - Auto-detect và restart `pkt-sync`, `blockchain-api` nếu enabled
- `Makefile` — 16 targets:
  - `build` / `release` / `build-linux` / `test`
  - `deploy` / `deploy-binary`
  - `logs` / `logs-api` / `logs-node`
  - `status` / `sync-start` / `sync-stop` / `sync-restart`
  - `api-start` / `api-stop` / `api-restart`
  - `help` (default target — màu, auto-generated từ `## comments`)
- `deploy.sh` (root) giữ nguyên để backward-compat

### Files
- `scripts/build-linux.sh` — file mới (chmod +x)
- `scripts/deploy.sh` — file mới (chmod +x)
- `Makefile` — file mới

### Gotcha
- Tránh dùng `USER` làm Makefile variable (bị override bởi shell env) — dùng `PKT_USER`

---

## v19.3 — P2P Peer Discovery (GetAddr/Addr) (2026-03-27)

### Added
- **`pkt_wire.rs`** — `NetAddr`, `GetAddr`, `Addr` types:
  - `NetAddr { timestamp, services, ip: [u8;16], port }` — IPv4-mapped IPv6 format
  - `NetAddr::from_addr_str("1.2.3.4:8333")` / `to_addr_string()`
  - `PktMsg::GetAddr` (empty payload) + `PktMsg::Addr { peers: Vec<NetAddr> }`
  - Encode: `[varint count][timestamp 4 LE][services 8 LE][ip 16][port 2 BE]` × N = 30 bytes/entry
  - Decode: validate min 30 bytes/entry, cap tối đa `MAX_ADDR_PER_MSG = 1000`
  - `save_peers(path, peers)` — ghi "ip:port\n" vào file, tạo parent dir nếu chưa có
  - `default_peers_path()` → `~/.pkt/peers.txt`
  - `USER_AGENT` cập nhật: `/blockchain-rust:19.3/`
- **`pkt_node.rs`** — respond `GetAddr`:
  - `handle_peer()` xử lý `PktMsg::GetAddr`: collect tất cả `ConnectedPeer.addr` → convert → gửi `Addr`
  - Log `[pkt-node] → Addr(N) to <addr>`
  - Nhận `Addr` từ peer: log entry count
- **`pkt_sync.rs`** — discover peers sau handshake:
  - Sau handshake thành công, gửi `GetAddr` ngay lập tức
  - Set read timeout 5 giây, đợi `Addr` response
  - Nếu nhận được: `save_peers(~/.pkt/peers.txt, peers)`, log count
  - Restore timeout về `cfg.recv_timeout_secs` sau đó

### Files
- `src/pkt_wire.rs` — thêm NetAddr, GetAddr/Addr encode/decode, save_peers, default_peers_path
- `src/pkt_node.rs` — handle GetAddr trong message loop
- `src/pkt_sync.rs` — send GetAddr + save peers sau handshake

### Tests
- +10 tests trong pkt_wire: netaddr_from_addr_str, netaddr_to_addr_string, roundtrip_getaddr, roundtrip_addr_empty, roundtrip_addr_two_peers, addr_port_is_big_endian, save_peers_writes_correct_lines, pkt_msg_getaddr_command_str, netaddr_invalid, netaddr_non_mapped

---

## v19.2 — JSON-RPC 2.0 Bitcoin-compatible (2026-03-27)

### Added
- `src/pkt_rpc.rs` — JSON-RPC 2.0 endpoint `POST /rpc`:
  - `getblockcount` → tip height
  - `getblockhash [height]` → block hash hex
  - `getblock [hash, verbosity?]` — accept hash string hoặc height number; verbosity=0 trả hex, verbosity=1/2 trả JSON
  - `getrawtransaction [txid]` — tìm trong mempool, trả JSON
  - `getmininginfo` → `{ blocks, difficulty, networkhashps, chain }`
  - `getnetworkinfo` → `{ version, subversion, protocolversion, connections, relayfee }`
  - `sendrawtransaction` → ERR_UNSUPPORTED stub (−32)
  - Error codes Bitcoin-compatible: `−32700` parse, `−32601` method not found, `−32602` invalid params, `−32603` internal, `−5` block not found, `−32` unsupported
  - `RpcState { sync_path, mempool_path }` — dùng default paths từ `pkt_testnet_web` + `pkt_mempool_sync`
- Route `POST /rpc` đăng ký trong `pktscan_api.rs`

### Files
- `src/pkt_rpc.rs` — file mới
- `src/main.rs` — thêm `mod pkt_rpc;`
- `src/pktscan_api.rs` — thêm `.merge(crate::pkt_rpc::rpc_router())`

### Tests
- +19 tests: error codes, getblockcount (empty + data), getblockhash, getblock (height/hash/hex/verbosity/not_found), getrawtransaction, getmininginfo, getnetworkinfo, sendrawtransaction, unknown method

---

## v19.1 — Flat File Block Storage (2026-03-27)

### Added
- `src/block_storage.rs` — append-only flat file storage cho `Block`:
  - Format: `[magic:"PKT!" 4B][block_size 4B LE][block_json]` mỗi record
  - Files: `blk00000.dat`, `blk00001.dat`... tạo file mới khi đạt `MAX_FILE_SIZE` (128 MB)
  - `BlockStorage::open(dir)` / `open_with_max(dir, max)` — mở hoặc tạo storage
  - `append(block)` → `BlockLocation` — ghi block, cập nhật index + tip
  - `get(height)` → `Option<Block>` — đọc qua index
  - `read_at(loc)` — đọc trực tiếp theo vị trí, validate magic
  - `get_location(height)` → `Option<BlockLocation>`
  - `get_tip_height()`, `count()`
  - `BlockLocation { file_num, offset, size }` — serialize 16 bytes
  - `BlockStorageError` — Io/Db/Json/Corrupt/NotFound
- RocksDB index (`{data_dir}/index`):
  - `blk:{height:016x}` → 16 bytes location
  - `meta:tip`, `meta:cur_file`, `meta:cur_offset`

### Files
- `src/block_storage.rs` — file mới
- `src/main.rs` — thêm `mod block_storage;`

### Tests
- +17 tests: location roundtrip, empty storage, append/read, file split, magic validation, error display

### Gotcha
- `append()` dùng `Mutex<()>` nội bộ — thread-safe cho concurrent writes
- `open_with_max(dir, 50)` để test file split với kích thước nhỏ
- Chưa tích hợp vào `chain.rs` (giữ `Vec<Block>` in-memory như cũ) — migration là bước riêng

---

## v19.0 — Cargo Workspace (2026-03-27)

### Added
- `[workspace]` trong root `Cargo.toml` — resolver = "2", members: `.`, `crates/pkt-sdk`, `crates/pkt-api`
- `crates/pkt-sdk/` — library crate cho third-party developers:
  - `types.rs`: `BlockHeader`, `BlockPage`, `TxRef`, `TxPage`, `AddressInfo`, `AddressBalance`, `Utxo`, `SyncStatus`, `NetworkSummary`
  - `convert.rs`: `paklets_to_pkt`, `pkt_to_paklets`, `short_hash`, `short_addr`, `ago`, `secs_ago`, `fmt_hashrate`
  - `error.rs`: `PktError`, `PktResult<T>`
  - Constants: `PAKLETS_PER_PKT`, `TESTNET_PORT`, `MAINNET_PORT`, `API_PORT`, `SDK_VERSION`
- `crates/pkt-api/` — binary stub, sẽ chứa standalone REST server từ v19.2+
- Toàn bộ code cũ giữ nguyên, chỉ thêm workspace layer

### Files
- `Cargo.toml` — thêm `[workspace]` section
- `crates/pkt-sdk/Cargo.toml` + `src/{lib,types,convert,error}.rs` — SDK library (file mới)
- `crates/pkt-api/Cargo.toml` + `src/main.rs` — API binary stub (file mới)

### Tests
- +15 unit tests + 4 doc-tests (pkt-sdk: convert + error)

### Gotcha
- `cargo build -p blockchain-rust` vẫn hoạt động như trước
- `cargo run -p pkt-api` in roadmap, chưa chạy server thật
- `'…'` là 3 bytes UTF-8 → dùng `.chars().count()` thay `.len()` khi đếm ký tự

---

## v18.9 — Data Export (2026-03-27)

### Added
- `GET /api/testnet/address/:s/export.csv` — TX history của address dưới dạng CSV (`height,txid`)
- `GET /api/testnet/blocks/export.csv?from=H&to=H` — Block range dưới dạng CSV (`height,hash,prev_hash,timestamp,bits,nonce,version`)
- Giới hạn: tối đa `MAX_ADDR_EXPORT_ROWS = 100_000` rows cho address; `MAX_EXPORT_BLOCKS = 10_000` cho blocks
- `from > to` tự động swap; `from`/`to` thiếu → dùng `0` / tip height
- Response header: `Content-Disposition: attachment; filename="*.csv"` để browser tự download

### Files
- `src/pkt_export.rs` — logic generate CSV (file mới)
- `src/pkt_testnet_web.rs` — thêm handlers `ps_export_address`, `ps_export_blocks` + 2 routes
- `src/main.rs` — thêm `mod pkt_export;`

### Tests
- +9 tests (pkt_export: header row, empty DB, single block, from>to swap, cap at max, columns count, address empty, address header, max_rows=0)

### Gotcha
- Dùng `tokio::task::spawn_blocking` cho RocksDB read để không block async runtime

---

## v15.8 — Single Chain Architecture + PKTScan Live Data (2026-03-21)

### Added
- `pkt_node.rs`: template server trên port+1 (default 8334)
  - `handle_template_client()` — GetTemplate / NewBlock / GetBlocks JSON-lines protocol
  - `run_template_server(port, chain)` — bind 0.0.0.0:{port} nhận miner + explorer
  - `cmd_pkt_node()` — load chain từ RocksDB, spawn template thread, rồi run PKT wire server
- `chain.rs`: `commit_mined_block(block)` — push block đã mine mà không re-mine
- `miner.rs`: fallback chain `127.0.0.1:8334` → `seed.testnet.oceif.com:8334` → standalone
  - `DEFAULT_NODE = "127.0.0.1:8334"`, `FALLBACK_NODE = "seed.testnet.oceif.com:8334"`
  - 3 lần liên tiếp thất bại → tự chuyển sang standalone
  - `run_standalone()` load chain từ RocksDB (thay vì reset)
  - `try_mine_one() -> bool` — trả false nếu node không phản hồi
- `pktscan_api.rs`: selective reload từ RocksDB mỗi 5s
  - Chỉ sync khi `fresh.chain.len() > bc.chain.len()` (giữ nguyên mempool/staking/tokens)
  - `miner_from_block()` — Base58Check P2PKH address từ coinbase tx
  - `block_summary()` — thêm `miner`, `difficulty`, `reward`
  - `tx_summary()` — thêm `from`, `to`
- `main.rs`: `mine` mặc định kết nối node (không còn standalone)
- `index.html`: fix hiển thị reward (paklets→PKT), tx timestamp (`block_timestamp`), tx amount (`output_total`), path `/static/testnet.js`

### Files
- `src/pkt_node.rs` — mở rộng: template server port+1
- `src/chain.rs` — thêm `commit_mined_block()`
- `src/miner.rs` — mở rộng: fallback logic, load DB khi standalone
- `src/pktscan_api.rs` — mở rộng: live reload, address/difficulty/reward fields
- `index.html` — fix display bugs

### Tests
- Không có tests mới (infrastructure + hotfix)

### Gotcha
- Template server port = PKT wire port + 1 (node chạy 8333 → template 8334; node chạy 64512 → template 64513)
- Explorer CLI (`cargo run -- explorer chain`) kết nối `DEFAULT_NODE = 127.0.0.1:8334` để GetBlocks — cần pkt-node đang chạy
- `commit_mined_block()` không mine lại — dùng khi block đã có hash; dùng `add_block()` khi muốn chain tự mine
- pktscan selective reload giữ nguyên `mempool`/`staking_pool`/`token_registry` trong memory — chỉ update chain/utxo_set/difficulty
- `try_clone().unwrap()` trong template client đã fix thành graceful error return

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
