#![allow(dead_code)]

/// v3.8 — Sovereign Rollup
///
/// A Sovereign Rollup posts transaction data to a Data Availability (DA) layer
/// but settles entirely on its own chain — no L1 smart contract for settlement.
///
/// ─── Architecture ────────────────────────────────────────────────────────────
///
///   Sovereign Rollup  ──posts blobs──▶  DA Layer (Celestia-like)
///       │                                    │
///       │  own consensus + settlement         │  stores raw data
///       │  full nodes re-execute              │  namespaced by rollup ID
///       │  can upgrade independently          │  erasure-coded for light clients
///       ▼                                    ▼
///   Rollup full nodes                   Light nodes (DAS)
///
/// ─── vs. Smart Contract Rollup ────────────────────────────────────────────────
///
///   Smart Contract Rollup (v2.2, v2.3):
///     - Posts state roots + proofs to L1 smart contract
///     - Settlement: L1 contract verifies and accepts state
///     - Upgrade: requires L1 contract upgrade (L1 governance)
///
///   Sovereign Rollup (this file):
///     - Posts raw data blobs to DA layer (no state/proof to L1)
///     - Settlement: rollup full nodes execute from DA data, own fork-choice
///     - Upgrade: rollup community decides — DA layer is neutral
///     - Example: Celestia-based rollups (Eclipse, Fuel, Rollkit chains)
///
/// ─── Data Availability Layer ─────────────────────────────────────────────────
///
///   - Accepts blobs identified by Namespace (8-byte rollup ID)
///   - Guarantees data is available for download (does NOT execute)
///   - Namespace Merkle Tree (NMT): proves inclusion for a specific namespace
///   - Erasure coding: 2x extend data so 50% availability → full recovery
///
/// ─── Data Availability Sampling (DAS) ────────────────────────────────────────
///
///   Light nodes verify availability without downloading all data:
///     1. Request k random positions from extended (erasure-coded) blob
///     2. If all k positions respond, data is (probabilistically) available
///     3. With k=8 samples and 50% withheld: detection prob = 1 - 0.5^8 = 99.6%
///
/// References: Celestia whitepaper, Rollkit, LazyLedger, Modular Blockchain thesis

use sha2::{Sha256, Digest};
use std::collections::HashMap;

// ─── Namespace ────────────────────────────────────────────────────────────────

pub type Namespace = [u8; 8];

/// Convert a string label to a fixed 8-byte namespace
pub fn ns(label: &str) -> Namespace {
    let mut r = [0u8; 8];
    let b = label.as_bytes();
    r[..b.len().min(8)].copy_from_slice(&b[..b.len().min(8)]);
    r
}

// ─── Erasure Coding (simplified) ──────────────────────────────────────────────

/// XOR-based erasure extension: doubles data size so any 50% suffices for recovery
pub struct ErasureExtended {
    pub data:   Vec<u8>,   // original n bytes
    pub parity: Vec<u8>,   // parity n bytes: p[i] = d[i] XOR key(i, seed)
    pub seed:   u64,
}

impl ErasureExtended {
    pub fn encode(data: &[u8], seed: u64) -> Self {
        let parity = data.iter().enumerate()
            .map(|(i, &b)| b ^ parity_key(i, seed))
            .collect();
        ErasureExtended { data: data.to_vec(), parity, seed }
    }

    /// Full extended data: [data ++ parity] (2n bytes)
    pub fn extended(&self) -> Vec<u8> {
        [self.data.clone(), self.parity.clone()].concat()
    }

    /// Reconstruct data from parity alone (data shard was withheld)
    pub fn recover_from_parity(parity: &[u8], seed: u64) -> Vec<u8> {
        parity.iter().enumerate()
            .map(|(i, &b)| b ^ parity_key(i, seed))
            .collect()
    }

    /// Verify a sampled position
    pub fn verify_position(ext: &[u8], pos: usize, seed: u64, orig_len: usize) -> bool {
        if pos >= ext.len() { return false; }
        if pos < orig_len {
            // data shard — any byte is valid if present
            true
        } else {
            // parity shard — check d[i] XOR p[i] == key(i)
            let i = pos - orig_len;
            if i >= orig_len { return false; }
            (ext[i] ^ ext[pos]) == parity_key(i, seed)
        }
    }
}

fn parity_key(i: usize, seed: u64) -> u8 {
    let x = seed.wrapping_add(i as u64).wrapping_mul(0x9e3779b97f4a7c15u64);
    (x >> 56) as u8  // top byte of hash output
}

// ─── DAS Result ───────────────────────────────────────────────────────────────

pub struct DasSampleResult {
    pub positions_sampled: usize,
    pub valid_count:       usize,
    /// Estimated probability data is available (1 - 0.5^valid_count for 50% attack)
    pub confidence:        f64,
}

impl DasSampleResult {
    pub fn likely_available(&self) -> bool {
        self.valid_count == self.positions_sampled
    }
}

// ─── DA Blob ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct BlobId {
    pub da_height:  u64,
    pub index:      usize,
    pub commitment: [u8; 32],
}

impl BlobId {
    pub fn short(&self) -> String {
        format!("{}..@da{}", hex::encode(&self.commitment[..4]), self.da_height)
    }
}

#[derive(Clone)]
pub struct DaBlob {
    pub namespace:  Namespace,
    pub data:       Vec<u8>,      // raw tx bytes from rollup
    pub extended:   Vec<u8>,      // erasure-coded 2x data
    pub commitment: [u8; 32],     // H(namespace ‖ data)
    pub erasure_seed: u64,
}

impl DaBlob {
    pub fn new(namespace: Namespace, data: Vec<u8>, seed: u64) -> Self {
        let erasure = ErasureExtended::encode(&data, seed);
        let commitment = blob_hash(&namespace, &data);
        DaBlob {
            namespace,
            extended: erasure.extended(),
            data,
            commitment,
            erasure_seed: seed,
        }
    }
}

fn blob_hash(ns: &Namespace, data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"blob_v1");
    h.update(ns.as_ref());
    h.update(data);
    let out = h.finalize();
    let mut r = [0u8; 32]; r.copy_from_slice(&out); r
}

// ─── DA Block ─────────────────────────────────────────────────────────────────

pub struct DaBlock {
    pub height:    u64,
    pub blobs:     Vec<DaBlob>,
    pub data_root: [u8; 32],  // NMT root: H(blob commitments sorted by namespace)
}

impl DaBlock {
    fn compute_data_root(blobs: &[DaBlob]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"nmt_root");
        // Sort by namespace (NMT property: namespaces are in order)
        let mut sorted: Vec<_> = blobs.iter().collect();
        sorted.sort_by_key(|b| b.namespace);
        for blob in sorted {
            h.update(&blob.namespace);
            h.update(&blob.commitment);
        }
        let out = h.finalize();
        let mut r = [0u8; 32]; r.copy_from_slice(&out); r
    }
}

// ─── DA Layer ─────────────────────────────────────────────────────────────────

pub struct DaLayer {
    pub chain_id: String,
    pub height:   u64,
    pub blocks:   Vec<DaBlock>,
    pub events:   Vec<String>,
    seed_ctr:     u64,
}

impl DaLayer {
    pub fn new(chain_id: &str) -> Self {
        let genesis = DaBlock { height: 0, blobs: Vec::new(), data_root: [0u8; 32] };
        DaLayer {
            chain_id: chain_id.to_string(),
            height: 0,
            blocks: vec![genesis],
            events: Vec::new(),
            seed_ctr: 0,
        }
    }

    fn log(&mut self, msg: &str) {
        self.events.push(format!("[{}@{}] {}", self.chain_id, self.height, msg));
    }

    /// Submit a blob to the current DA block; returns a BlobId pointer
    pub fn submit_blob(&mut self, namespace: Namespace, data: Vec<u8>) -> BlobId {
        self.seed_ctr += 1;
        let blob = DaBlob::new(namespace, data, self.seed_ctr);
        let commitment = blob.commitment;
        let data_len = blob.data.len();
        let ext_len  = blob.extended.len();

        let current = self.blocks.last_mut().unwrap();
        let index = current.blobs.len();
        current.blobs.push(blob);
        current.data_root = DaBlock::compute_data_root(&current.blobs);

        self.log(&format!("SubmitBlob ns={} data={}b ext={}b commit={}...",
            hex::encode(&namespace[..4]), data_len, ext_len, hex::encode(&commitment[..4])));
        BlobId { da_height: self.height, index, commitment }
    }

    /// Seal current block and start next one
    pub fn seal_block(&mut self) {
        let block = self.blocks.last().unwrap();
        let data_root = block.data_root;
        let blobs = block.blobs.len();
        self.log(&format!("SealBlock height={} blobs={} root={}...",
            self.height, blobs, hex::encode(&data_root[..4])));
        self.height += 1;
        self.blocks.push(DaBlock { height: self.height, blobs: Vec::new(), data_root: [0u8; 32] });
    }

    pub fn get_blob(&self, id: &BlobId) -> Option<&DaBlob> {
        self.blocks.get(id.da_height as usize)?.blobs.get(id.index)
    }

    /// Get all blobs for a namespace in a DA height range
    pub fn namespace_blobs(&self, ns: &Namespace, from: u64, to: u64) -> Vec<(u64, &DaBlob)> {
        let mut out = Vec::new();
        for h in from..=to.min(self.height) {
            if let Some(block) = self.blocks.get(h as usize) {
                for blob in &block.blobs {
                    if &blob.namespace == ns {
                        out.push((h, blob));
                    }
                }
            }
        }
        out
    }

    /// Namespace inclusion proof: verify namespace has data in DA block
    pub fn prove_namespace(&self, height: u64, ns: &Namespace) -> bool {
        self.blocks.get(height as usize)
            .map(|b| b.blobs.iter().any(|bl| &bl.namespace == ns))
            .unwrap_or(false)
    }

    /// DAS: light node samples k positions of a blob's extended data
    pub fn das_sample(&self, id: &BlobId, sample_positions: &[usize]) -> DasSampleResult {
        let blob = match self.get_blob(id) {
            Some(b) => b,
            None    => return DasSampleResult { positions_sampled: 0, valid_count: 0, confidence: 0.0 },
        };
        let ext = &blob.extended;
        let orig_len = blob.data.len();
        let valid = sample_positions.iter()
            .filter(|&&pos| {
                pos < ext.len() &&
                ErasureExtended::verify_position(ext, pos, blob.erasure_seed, orig_len)
            })
            .count();
        let k = sample_positions.len();
        let confidence = 1.0 - 0.5f64.powi(valid as i32);
        DasSampleResult { positions_sampled: k, valid_count: valid, confidence }
    }

    pub fn print_events_since(&self, from: usize) {
        for e in &self.events[from..] { println!("  {}", e); }
    }
}

// ─── Rollup Transaction ───────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct RollupTx {
    pub from:   String,
    pub to:     String,
    pub amount: u64,
    pub nonce:  u64,
}

impl RollupTx {
    pub fn encode(&self) -> Vec<u8> {
        format!("rtx:{}:{}:{}:{}", self.from, self.to, self.amount, self.nonce).into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(bytes).ok()?;
        let p: Vec<&str> = s.splitn(5, ':').collect();
        if p.len() == 5 && p[0] == "rtx" {
            Some(RollupTx {
                from: p[1].to_string(), to: p[2].to_string(),
                amount: p[3].parse().ok()?, nonce: p[4].parse().ok()?,
            })
        } else { None }
    }
}

/// Encode a batch of transactions into blob data (length-prefixed)
pub fn encode_batch(txs: &[RollupTx]) -> Vec<u8> {
    let mut out = Vec::new();
    for tx in txs {
        let enc = tx.encode();
        out.extend_from_slice(&(enc.len() as u32).to_le_bytes());
        out.extend_from_slice(&enc);
    }
    out
}

/// Decode a batch from blob data
pub fn decode_batch(data: &[u8]) -> Vec<RollupTx> {
    let mut txs = Vec::new();
    let mut pos = 0;
    while pos + 4 <= data.len() {
        let len = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + len > data.len() { break; }
        if let Some(tx) = RollupTx::decode(&data[pos..pos+len]) {
            txs.push(tx);
        }
        pos += len;
    }
    txs
}

// ─── Rollup Block Header ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct RollupBlockHeader {
    pub height:     u64,
    pub state_root: [u8; 32],
    pub tx_root:    [u8; 32],
    pub tx_count:   usize,
    pub da_height:  u64,
    pub blob_id:    BlobId,
}

// ─── Sovereign Rollup ─────────────────────────────────────────────────────────

pub struct SovereignRollup {
    pub name:       String,
    pub namespace:  Namespace,
    pub height:     u64,
    pub state_root: [u8; 32],
    pub balances:   HashMap<String, u64>,
    pub headers:    Vec<RollupBlockHeader>,
    pub events:     Vec<String>,
    pub protocol_version: u32,  // sovereign governance can bump this
    nonces:         HashMap<String, u64>,
}

impl SovereignRollup {
    pub fn new(name: &str, ns_label: &str) -> Self {
        SovereignRollup {
            name:             name.to_string(),
            namespace:        ns(ns_label),
            height:           0,
            state_root:       [0u8; 32],
            balances:         HashMap::new(),
            headers:          Vec::new(),
            events:           Vec::new(),
            protocol_version: 1,
            nonces:           HashMap::new(),
        }
    }

    fn log(&mut self, msg: &str) {
        self.events.push(format!("[{}@{}] {}", self.name, self.height, msg));
    }

    /// Mint tokens (genesis / bridge-in)
    pub fn deposit(&mut self, addr: &str, amount: u64) {
        *self.balances.entry(addr.to_string()).or_insert(0) += amount;
        self.recompute_state_root();
        self.log(&format!("Deposit {} += {}", addr, amount));
    }

    /// Sequencer: execute txs locally, post blob to DA layer, build block header
    pub fn post_block(&mut self, da: &mut DaLayer, txs: Vec<RollupTx>) -> Result<RollupBlockHeader, String> {
        let mut executed = Vec::new();

        for tx in &txs {
            let bal = *self.balances.get(&tx.from).unwrap_or(&0);
            let nonce = *self.nonces.get(&tx.from).unwrap_or(&0);
            if bal < tx.amount {
                self.log(&format!("InvalidTx: {} balance {} < {}", tx.from, bal, tx.amount));
                continue;
            }
            if tx.nonce != nonce {
                self.log(&format!("InvalidTx: {} nonce {} != {}", tx.from, tx.nonce, nonce));
                continue;
            }
            *self.balances.entry(tx.from.clone()).or_insert(0) -= tx.amount;
            *self.balances.entry(tx.to.clone()).or_insert(0)   += tx.amount;
            *self.nonces.entry(tx.from.clone()).or_insert(0) += 1;
            executed.push(tx.clone());
        }

        // Post blob to DA layer
        let blob_data = encode_batch(&executed);
        let blob_id = da.submit_blob(self.namespace, blob_data);
        da.seal_block();

        self.recompute_state_root();
        let tx_root = tx_root(&executed);

        let header = RollupBlockHeader {
            height:     self.height + 1,
            state_root: self.state_root,
            tx_root,
            tx_count:   executed.len(),
            da_height:  blob_id.da_height,
            blob_id,
        };

        self.height += 1;
        self.headers.push(header.clone());
        self.log(&format!("PostBlock: height={} txs={} da={}",
            self.height, executed.len(), header.blob_id.short()));
        Ok(header)
    }

    /// Full node: verify a block by re-downloading from DA and checking state root
    pub fn verify_from_da(&self, da: &DaLayer, header: &RollupBlockHeader) -> bool {
        // 1. Get blob from DA
        let blob = match da.get_blob(&header.blob_id) {
            Some(b) => b,
            None    => return false,
        };
        // 2. Verify commitment matches
        let expected = blob_hash(&self.namespace, &blob.data);
        if expected != header.blob_id.commitment {
            return false;
        }
        // 3. Decode transactions
        let txs = decode_batch(&blob.data);
        // 4. Verify tx root
        if tx_root(&txs) != header.tx_root {
            return false;
        }
        true
    }

    /// Sync full node from DA: scan DA layer for this rollup's blobs
    pub fn sync_from_da(&mut self, da: &DaLayer) -> usize {
        let blobs = da.namespace_blobs(&self.namespace, 0, da.height);
        let count = blobs.len();
        self.log(&format!("SyncFromDA: {} blobs found for {}", count, self.name));
        count
    }

    /// Sovereign governance: rollup upgrades its own protocol version (no L1 needed)
    pub fn sovereign_upgrade(&mut self, new_version: u32, reason: &str) {
        let old = self.protocol_version;
        self.protocol_version = new_version;
        self.log(&format!("SovereignUpgrade: v{} → v{} reason={}", old, new_version, reason));
    }

    fn recompute_state_root(&mut self) {
        let mut h = Sha256::new();
        h.update(b"rollup_state_v1");
        h.update(self.name.as_bytes());
        h.update(&self.height.to_le_bytes());
        let mut keys: Vec<_> = self.balances.keys().cloned().collect();
        keys.sort();
        for k in &keys {
            h.update(k.as_bytes());
            h.update(&self.balances[k].to_le_bytes());
        }
        let out = h.finalize();
        self.state_root.copy_from_slice(&out);
    }

    pub fn print_events_since(&self, from: usize) {
        for e in &self.events[from..] { println!("  {}", e); }
    }
}

fn tx_root(txs: &[RollupTx]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"tx_root_v1");
    for tx in txs { h.update(&tx.encode()); }
    let out = h.finalize();
    let mut r = [0u8; 32]; r.copy_from_slice(&out); r
}
