#![allow(dead_code)]

/// v4.2.1 — Persistent Storage (RocksDB backend)
///
/// Thay thế JSON files bằng RocksDB embedded key-value store.
/// RocksDB: production-grade, LSM-tree, atomic batch writes, compaction.
///
/// DB path: ~/.pkt/testnet/db/ hoặc ~/.pkt/mainnet/db/ (theo pkt_paths)
///
/// Key schema:
///   block:{height:016x}  → serde_json bytes of Block   (zero-padded hex → lexicographic sort)
///   utxo:{txid}:{index}  → serde_json bytes of TxOutput
///   meta:height          → current tip height (decimal string)
///
/// Public API không thay đổi so với v4.2 (JSON) — caller không cần sửa.

use std::collections::HashMap;
use std::path::PathBuf;

use rocksdb::{DB, Options, IteratorMode};

use crate::block::Block;
use crate::transaction::TxOutput;

// ─── DB path ──────────────────────────────────────────────────────────────────

fn db_path() -> PathBuf { crate::pkt_paths::data_dir().join("db") }

fn open_db() -> Result<DB, String> {
    let path = db_path();
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let mut opts = Options::default();
    opts.create_if_missing(true);
    DB::open(&opts, &path).map_err(|e| e.to_string())
}

// ─── Key helpers ──────────────────────────────────────────────────────────────

fn block_key(height: u64) -> String { format!("block:{:016x}", height) }
fn utxo_key(k: &str)       -> String { format!("utxo:{}", k) }
const META_HEIGHT:     &[u8] = b"meta:height";
const META_DIFFICULTY: &[u8] = b"meta:difficulty";

// ─── Chain storage ────────────────────────────────────────────────────────────

/// Lưu tất cả blocks vào RocksDB
pub fn save_chain(blocks: &[Block]) -> Result<(), String> {
    let db = open_db()?;
    for block in blocks {
        let key = block_key(block.index);
        let val = serde_json::to_vec(block)
            .map_err(|e| format!("serialize block {}: {e}", block.index))?;
        db.put(key.as_bytes(), &val).map_err(|e| e.to_string())?;
    }
    // Ghi tip height vào meta
    if let Some(last) = blocks.last() {
        db.put(META_HEIGHT, last.index.to_string().as_bytes())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Load tất cả blocks từ RocksDB, sắp xếp theo height
pub fn load_chain() -> Result<Option<Vec<Block>>, String> {
    if !db_path().exists() { return Ok(None); }
    let db = open_db()?;
    let mut blocks: Vec<Block> = Vec::new();

    for item in db.iterator(IteratorMode::Start) {
        let (key, val) = item.map_err(|e| e.to_string())?;
        let key_str = std::str::from_utf8(&key).unwrap_or("");
        if key_str.starts_with("block:") {
            let block: Block = serde_json::from_slice(&val)
                .map_err(|e| format!("deserialize block: {e}"))?;
            blocks.push(block);
        }
    }

    if blocks.is_empty() { return Ok(None); }
    blocks.sort_by_key(|b| b.index);
    Ok(Some(blocks))
}

// ─── UTXO storage ─────────────────────────────────────────────────────────────

/// Lưu toàn bộ UTXO set vào RocksDB
pub fn save_utxo(utxos: &HashMap<String, TxOutput>) -> Result<(), String> {
    let db = open_db()?;

    // Xóa toàn bộ utxo cũ trước khi ghi mới (clean write)
    let old_keys: Vec<Vec<u8>> = db.iterator(IteratorMode::Start)
        .filter_map(|item| {
            item.ok().and_then(|(k, _)| {
                if std::str::from_utf8(&k).unwrap_or("").starts_with("utxo:") {
                    Some(k.to_vec())
                } else { None }
            })
        })
        .collect();
    for k in old_keys {
        db.delete(&k).map_err(|e| e.to_string())?;
    }

    // Ghi UTXO mới
    for (k, output) in utxos {
        let key = utxo_key(k);
        let val = serde_json::to_vec(output)
            .map_err(|e| format!("serialize utxo {k}: {e}"))?;
        db.put(key.as_bytes(), &val).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Load toàn bộ UTXO set từ RocksDB
pub fn load_utxo() -> Result<Option<HashMap<String, TxOutput>>, String> {
    if !db_path().exists() { return Ok(None); }
    let db = open_db()?;
    let mut utxos: HashMap<String, TxOutput> = HashMap::new();

    for item in db.iterator(IteratorMode::Start) {
        let (key, val) = item.map_err(|e| e.to_string())?;
        let key_str = std::str::from_utf8(&key).unwrap_or("");
        if let Some(utxo_key) = key_str.strip_prefix("utxo:") {
            let output: TxOutput = serde_json::from_slice(&val)
                .map_err(|e| format!("deserialize utxo: {e}"))?;
            utxos.insert(utxo_key.to_string(), output);
        }
    }

    if utxos.is_empty() { return Ok(None); }
    Ok(Some(utxos))
}

// ─── Snapshot ────────────────────────────────────────────────────────────────

/// Lưu chain + UTXO vào DB (gọi sau mỗi block mới)
pub fn save_snapshot(
    blocks: &[Block],
    utxos:  &HashMap<String, TxOutput>,
) -> Result<(), String> {
    save_chain(blocks)?;
    save_utxo(utxos)?;
    Ok(())
}

/// Thông tin về DB hiện tại
pub struct SnapshotInfo {
    pub chain_height: usize,
    pub utxo_count:   usize,
    pub db_path:      PathBuf,
}

pub fn snapshot_info() -> Result<Option<SnapshotInfo>, String> {
    if !db_path().exists() { return Ok(None); }
    let blocks = load_chain()?.unwrap_or_default();
    let utxos  = load_utxo()?.unwrap_or_default();
    Ok(Some(SnapshotInfo {
        chain_height: blocks.len().saturating_sub(1),
        utxo_count:   utxos.len(),
        db_path:      db_path(),
    }))
}

// ─── Integration ─────────────────────────────────────────────────────────────

use crate::chain::Blockchain;
use crate::utxo::UtxoSet;

/// Load snapshot vào Blockchain struct. Nếu không có → genesis.
pub fn load_or_new() -> Blockchain {
    match try_load_blockchain() {
        Ok(Some(mut bc)) => {
            // v5.5: Kiểm tra và repair nếu phát hiện crash mid-write
            crate::wal::check_and_recover(&mut bc);
            println!(
                "  📦 Loaded from RocksDB: height={}, utxos={}",
                bc.chain.len() - 1,
                bc.utxo_set.utxos.len()
            );
            bc
        }
        Ok(None) => {
            println!("  🌱 No DB found — starting fresh (genesis)");
            Blockchain::new()
        }
        Err(e) => {
            eprintln!("  ⚠️  DB load failed: {} — starting fresh", e);
            Blockchain::new()
        }
    }
}

fn try_load_blockchain() -> Result<Option<Blockchain>, String> {
    let blocks = match load_chain()? {
        Some(b) => b,
        None    => return Ok(None),
    };
    let utxo_map = load_utxo()?.unwrap_or_default();
    let mut utxo_set = UtxoSet::new();
    utxo_set.utxos = utxo_map;

    // Load difficulty từ DB; nếu chưa có (DB cũ) → tính lại từ chain
    let difficulty = {
        let db = open_db()?;
        match db.get(META_DIFFICULTY).map_err(|e| e.to_string())? {
            Some(v) => std::str::from_utf8(&v).ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(3),
            None => recalculate_difficulty(&blocks),
        }
    };

    let fee_estimator = crate::fee_market::FeeEstimator::rebuild_from_blocks(&blocks);
    Ok(Some(Blockchain {
        chain: blocks,
        difficulty,
        utxo_set,
        mempool:        crate::mempool::Mempool::new(),
        fee_estimator,
        token_registry: crate::token::TokenRegistry::new(),
        staking_pool:   crate::staking::StakingPool::new(),
    }))
}

/// Tính lại difficulty từ lịch sử chain (migration từ DB cũ không lưu difficulty)
fn recalculate_difficulty(blocks: &[crate::block::Block]) -> usize {
    // Đếm số leading zeros trong hash của các block gần nhất
    let recent = blocks.iter().rev().take(10);
    let avg_zeros = recent
        .map(|b| b.hash.chars().take_while(|&c| c == '0').count())
        .max()
        .unwrap_or(3);
    avg_zeros.max(3)
}

/// Lưu Blockchain snapshot — v5.5: dùng atomic WriteBatch qua wal::atomic_save
pub fn save_blockchain(bc: &Blockchain) -> Result<(), String> {
    crate::wal::atomic_save(bc)
}

// ─── Balance tracker ──────────────────────────────────────────────────────────

/// Key schema: balance:{address_hex}
fn balance_key(address: &str) -> String { format!("balance:{}", address) }

/// Cộng thêm số paklets đã earn vào balance của address (cumulative)
pub fn add_mined_earnings(address: &str, earned: u64) -> Result<(), String> {
    let db  = open_db()?;
    let key = balance_key(address);
    let current = db.get(key.as_bytes())
        .map_err(|e| e.to_string())?
        .and_then(|v| std::str::from_utf8(&v).ok().and_then(|s| s.parse::<u64>().ok()))
        .unwrap_or(0);
    let new_balance = current.saturating_add(earned);
    db.put(key.as_bytes(), new_balance.to_string().as_bytes())
        .map_err(|e| e.to_string())
}

/// Đọc balance tích luỹ từ DB (trả về 0 nếu chưa có)
pub fn load_mined_balance(address: &str) -> u64 {
    let Ok(db) = open_db() else { return 0; };
    let key = balance_key(address);
    db.get(key.as_bytes()).ok()
        .flatten()
        .and_then(|v| std::str::from_utf8(&v).ok().and_then(|s| s.parse::<u64>().ok()))
        .unwrap_or(0)
}

// ─── Utility ─────────────────────────────────────────────────────────────────

/// Xóa toàn bộ DB (dùng cho tests và hard reset)
pub fn reset_storage() -> Result<(), String> {
    let path = db_path();
    if path.exists() {
        // DB::destroy xóa đúng cách (bao gồm manifest, WAL, SST files)
        DB::destroy(&Options::default(), &path)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Kích thước toàn bộ DB directory (bytes)
pub fn storage_size_bytes() -> u64 {
    dir_size(&db_path())
}

fn dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0; };
    entries.flatten().map(|e| {
        let p = e.path();
        if p.is_dir() { dir_size(&p) } else {
            std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0)
        }
    }).sum()
}

// ─── Contract Persistence (v10.3) ────────────────────────────────────────────
//
// Key schema: `contract:{address}` → serde_json of ContractStateData
//
// ContractState.storage は HashMap<[u8;32],[u8;32]> — không dùng trực tiếp làm
// JSON key (array không phải string).  Dùng ContractStateData với storage là
// Vec<(hex_key, hex_val)> để serialize/deserialize an toàn.

fn contract_key(address: &str) -> String {
    format!("contract:{}", address)
}

/// JSON-friendly representation của ContractState.
/// storage map được encode thành Vec<(hex_str, hex_str)> để tránh [u8;32] key issue.
#[derive(serde::Serialize, serde::Deserialize)]
struct ContractStateData {
    address:   String,
    code_hash: String,
    storage:   Vec<(String, String)>,
    balance:   u64,
    nonce:     u64,
}

impl ContractStateData {
    fn from_state(s: &crate::contract_state::ContractState) -> Self {
        let mut storage: Vec<(String, String)> = s.storage
            .iter()
            .map(|(k, v)| (hex::encode(k), hex::encode(v)))
            .collect();
        storage.sort_by_key(|(k, _)| k.clone()); // deterministic order
        ContractStateData {
            address:   s.address.clone(),
            code_hash: s.code_hash.clone(),
            storage,
            balance:   s.balance,
            nonce:     s.nonce,
        }
    }

    fn into_state(self) -> crate::contract_state::ContractState {
        let mut storage = HashMap::new();
        for (k_hex, v_hex) in self.storage {
            let kb = hex::decode(&k_hex).unwrap_or_default();
            let vb = hex::decode(&v_hex).unwrap_or_default();
            let mut k = [0u8; 32];
            let mut v = [0u8; 32];
            let klen = kb.len().min(32);
            let vlen = vb.len().min(32);
            k[32 - klen..].copy_from_slice(&kb[..klen]);
            v[32 - vlen..].copy_from_slice(&vb[..vlen]);
            storage.insert(k, v);
        }
        crate::contract_state::ContractState {
            address:   self.address,
            code_hash: self.code_hash,
            storage,
            balance:   self.balance,
            nonce:     self.nonce,
        }
    }
}

/// Lưu toàn bộ ContractStore vào RocksDB.
/// Xóa các entry cũ trước khi ghi để đảm bảo clean write.
pub fn save_contract_store(
    store: &crate::contract_state::ContractStore,
) -> Result<(), String> {
    let db = open_db()?;

    // Xóa entries cũ
    let old_keys: Vec<Vec<u8>> = db
        .iterator(IteratorMode::Start)
        .filter_map(|item| {
            item.ok().and_then(|(k, _)| {
                if std::str::from_utf8(&k).unwrap_or("").starts_with("contract:") {
                    Some(k.to_vec())
                } else {
                    None
                }
            })
        })
        .collect();
    for k in old_keys {
        db.delete(&k).map_err(|e| e.to_string())?;
    }

    // Ghi contract mới
    for (addr, state) in &store.contracts {
        let key  = contract_key(addr);
        let data = ContractStateData::from_state(state);
        let val  = serde_json::to_vec(&data)
            .map_err(|e| format!("serialize contract {addr}: {e}"))?;
        db.put(key.as_bytes(), &val).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Load ContractStore từ RocksDB.  Trả về `None` nếu không có entry nào.
pub fn load_contract_store(
) -> Result<Option<crate::contract_state::ContractStore>, String> {
    if !db_path().exists() {
        return Ok(None);
    }
    let db = open_db()?;
    let mut store = crate::contract_state::ContractStore::new();
    let mut found = false;

    for item in db.iterator(IteratorMode::Start) {
        let (key, val) = item.map_err(|e| e.to_string())?;
        let key_str = std::str::from_utf8(&key).unwrap_or("");
        if key_str.starts_with("contract:") {
            let data: ContractStateData = serde_json::from_slice(&val)
                .map_err(|e| format!("deserialize contract: {e}"))?;
            let state = data.into_state();
            store.contracts.insert(state.address.clone(), state);
            found = true;
        }
    }

    if !found {
        return Ok(None);
    }
    Ok(Some(store))
}

// ─── Governance persistence (v10.6) ──────────────────────────────────────────

const GOV_SNAPSHOT_KEY: &[u8] = b"governance:snapshot";

/// Lưu toàn bộ Governor state vào RocksDB dưới một key duy nhất.
pub fn save_governor(governor: &crate::governance::Governor) -> Result<(), String> {
    let db  = open_db()?;
    let snap = governor.snapshot();
    let json = serde_json::to_vec(&snap).map_err(|e| e.to_string())?;
    db.put(GOV_SNAPSHOT_KEY, json).map_err(|e| e.to_string())
}

/// Load Governor từ RocksDB. Trả về `None` nếu chưa có dữ liệu.
pub fn load_governor() -> Result<Option<crate::governance::Governor>, String> {
    if !db_path().exists() {
        return Ok(None);
    }
    let db = open_db()?;
    match db.get(GOV_SNAPSHOT_KEY).map_err(|e| e.to_string())? {
        None => Ok(None),
        Some(v) => {
            let snap: crate::governance::GovernanceSnapshot =
                serde_json::from_slice(&v).map_err(|e| e.to_string())?;
            Ok(Some(crate::governance::Governor::from_snapshot(snap)))
        }
    }
}

// ─── ContractRegistry persistence (v11.7) ────────────────────────────────────

const CONTRACT_REGISTRY_KEY: &[u8] = b"contract_registry:snapshot";
/// Companion map: address → template name (stored alongside registry snapshot).
const CONTRACT_TMAP_KEY:     &[u8] = b"contract_registry:tmap";

/// Lưu ContractRegistry + template map vào RocksDB.
pub fn save_contract_registry(
    reg:  &crate::smart_contract::ContractRegistry,
    tmap: &HashMap<String, String>,
) -> Result<(), String> {
    let db   = open_db()?;
    let snap = reg.snapshot(tmap);
    let json = serde_json::to_vec(&snap).map_err(|e| e.to_string())?;
    db.put(CONTRACT_REGISTRY_KEY, json).map_err(|e| e.to_string())
}

/// Load ContractRegistry từ RocksDB. Trả về registry rỗng nếu chưa có dữ liệu.
pub fn load_contract_registry(
) -> (crate::smart_contract::ContractRegistry, HashMap<String, String>) {
    let Ok(db) = open_db() else {
        return (crate::smart_contract::ContractRegistry::new(), HashMap::new());
    };
    match db.get(CONTRACT_REGISTRY_KEY) {
        Ok(Some(v)) => {
            match serde_json::from_slice::<crate::smart_contract::ContractRegistrySnapshot>(&v) {
                Ok(snap) => crate::smart_contract::ContractRegistry::from_snapshot(snap),
                Err(_)   => (crate::smart_contract::ContractRegistry::new(), HashMap::new()),
            }
        }
        _ => (crate::smart_contract::ContractRegistry::new(), HashMap::new()),
    }
}

// ─── StakingPool persistence (v11.8) ─────────────────────────────────────────

const STAKING_POOL_KEY: &[u8] = b"staking:pool";

/// Lưu StakingPool vào RocksDB.
pub fn save_staking_pool(pool: &crate::staking::StakingPool) -> Result<(), String> {
    let db   = open_db()?;
    let json = serde_json::to_vec(pool).map_err(|e| e.to_string())?;
    db.put(STAKING_POOL_KEY, json).map_err(|e| e.to_string())
}

/// Load StakingPool từ RocksDB. Trả về pool rỗng nếu chưa có dữ liệu.
pub fn load_staking_pool() -> crate::staking::StakingPool {
    let Ok(db) = open_db() else {
        return crate::staking::StakingPool::new();
    };
    match db.get(STAKING_POOL_KEY) {
        Ok(Some(v)) => serde_json::from_slice(&v)
            .unwrap_or_else(|_| crate::staking::StakingPool::new()),
        _ => crate::staking::StakingPool::new(),
    }
}

// ─── Token Registry persistence (v11.6) ──────────────────────────────────────

const TOKEN_REGISTRY_KEY: &[u8] = b"token:registry";

/// Lưu TokenRegistry vào RocksDB.
pub fn save_token_registry(reg: &crate::token::TokenRegistry) -> Result<(), String> {
    let db   = open_db()?;
    let snap = reg.snapshot();
    let json = serde_json::to_vec(&snap).map_err(|e| e.to_string())?;
    db.put(TOKEN_REGISTRY_KEY, json).map_err(|e| e.to_string())
}

/// Load TokenRegistry từ RocksDB. Trả về registry rỗng nếu chưa có dữ liệu.
pub fn load_token_registry() -> crate::token::TokenRegistry {
    let Ok(db) = open_db() else {
        return crate::token::TokenRegistry::new();
    };
    match db.get(TOKEN_REGISTRY_KEY) {
        Ok(Some(v)) => {
            match serde_json::from_slice::<crate::token::TokenRegistrySnapshot>(&v) {
                Ok(snap) => crate::token::TokenRegistry::from_snapshot(snap),
                Err(_)   => crate::token::TokenRegistry::new(),
            }
        }
        _ => crate::token::TokenRegistry::new(),
    }
}
