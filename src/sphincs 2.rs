#![allow(dead_code)]

/// v3.1 — Post-Quantum: SPHINCS+ (Hash-Based Signature)
///
/// Kiến trúc:
///
///   SPHINCS+ = FORS + HyperTree
///   ──────────────────────────
///
///   Sign(sk, msg):
///     R  = PRF(sk_prf, opt_rand, msg)         ← randomizer
///     md = H(R || pk || msg)                   ← message digest
///     Split md → (fors_indices, ht_addr)
///
///     FORS signature: few-times sig on md (k trees × t leaves)
///     HyperTree sig:  d layers of XMSS, bottom signs FORS root
///
///   Full signature = (R, FORS_sig, HT_sig)
///
///   Verification:
///     Recompute FORS root from sig → HT root → compare with PK
///
/// Building blocks:
///   WOTS+:  Winternitz OTS — chain hash w times, one-time signing
///   XMSS:   eXtended Merkle Signature Scheme — tree of WOTS+ leaves
///   FORS:   Forest Of Random Subsets — k small trees for few-times sig
///   HyperTree: d-layer XMSS stack, each layer authenticates level below
///
/// Why hash-based is quantum-safe:
///   - Security ← ONLY collision resistance + pre-image resistance of hash
///   - No algebraic structure (no DLP, no LWE) — purely combinatorial
///   - Shor/Grover worst-case: 2^(n/2) work (use n=256 for 128-bit PQ)
///
/// Parameters (educational — smaller than production for speed):
///   n  = 16    bytes (security parameter, hash output size)
///   h  =  8    (total HyperTree height)
///   d  =  2    (HyperTree layers, h/d = 4 per layer)
///   k  =  4    (FORS trees)
///   a  =  2    (FORS tree height, t = 2^a = 4 leaves)
///   w  =  4    (Winternitz chains length)
///
/// Real SPHINCS+-SHA2-128s: n=16, h=63, d=7, k=14, t=1024, w=16
///   PK=32B, SK=64B, Sig=7856B
///
/// Tham khảo: SPHINCS+ spec (v3.1), NIST FIPS 205 (2024)


// ─── Parameters ───────────────────────────────────────────────────────────────

pub const N:     usize = 16;   // hash output bytes (security param)
pub const H:     usize = 8;    // total HyperTree height
pub const D:     usize = 2;    // HyperTree layers
pub const H_PER: usize = H / D; // tree height per layer = 4
pub const K:     usize = 4;    // FORS trees
pub const A:     usize = 2;    // FORS tree height (t = 2^a = 4 leaves)
pub const T:     usize = 1 << A; // FORS leaves per tree = 4
pub const W:     usize = 4;    // Winternitz parameter
pub const LOG_W: usize = 2;    // log2(W)

/// WOTS+: len = ceil(8n / log2(w)) + floor(log2(len1 * (w-1)) / log2(w)) + 1
/// For n=16, w=4: len1=64, len2=3, len=67
pub const WOTS_LEN1: usize = (8 * N + LOG_W - 1) / LOG_W; // 64
pub const WOTS_LEN2: usize = 3;                             // checksum digits
pub const WOTS_LEN:  usize = WOTS_LEN1 + WOTS_LEN2;        // 67

// ─── Hash primitives ──────────────────────────────────────────────────────────

/// Tweakable hash: H(seed || addr || data) → n bytes
/// In real SPHINCS+: uses SHA-256 or SHAKE-256 with address encoding
pub fn thash(seed: &[u8; N], addr: &[u8; 32], data: &[&[u8]]) -> [u8; N] {
    let mut h = blake3::Hasher::new();
    h.update(b"sphincs_thash_v31");
    h.update(seed);
    h.update(addr);
    for d in data { h.update(d); }
    let out = *h.finalize().as_bytes();
    let mut r = [0u8; N];
    r.copy_from_slice(&out[..N]);
    r
}

/// PRF: deterministic pseudo-random value from seed + addr
pub fn prf(seed: &[u8; N], addr: &[u8; 32]) -> [u8; N] {
    let mut h = blake3::Hasher::new();
    h.update(b"sphincs_prf_v31");
    h.update(seed);
    h.update(addr);
    let out = *h.finalize().as_bytes();
    let mut r = [0u8; N];
    r.copy_from_slice(&out[..N]);
    r
}

/// PRF_msg: randomize message
pub fn prf_msg(sk_prf: &[u8; N], opt_rand: &[u8; N], msg: &[u8]) -> [u8; N] {
    let mut h = blake3::Hasher::new();
    h.update(b"sphincs_prf_msg");
    h.update(sk_prf);
    h.update(opt_rand);
    h.update(msg);
    let out = *h.finalize().as_bytes();
    let mut r = [0u8; N];
    r.copy_from_slice(&out[..N]);
    r
}

/// H_msg: hash message with randomizer + pk
pub fn h_msg(r: &[u8; N], pk_seed: &[u8; N], pk_root: &[u8; N], msg: &[u8]) -> Vec<u8> {
    let mut h = blake3::Hasher::new();
    h.update(b"sphincs_h_msg");
    h.update(r);
    h.update(pk_seed);
    h.update(pk_root);
    h.update(msg);
    // Returns k*a + h bits = 4*2 + 8 = 16 bits minimum → we return 32 bytes
    h.finalize().as_bytes().to_vec()
}

/// Build address bytes encoding (layer, tree, type, keypair, chain, hash)
pub fn make_addr(layer: u32, tree: u64, typ: u8, keypair: u32, chain: u32, hash_idx: u32) -> [u8; 32] {
    let mut addr = [0u8; 32];
    addr[0..4].copy_from_slice(&layer.to_le_bytes());
    addr[4..12].copy_from_slice(&tree.to_le_bytes());
    addr[12] = typ;
    addr[13..17].copy_from_slice(&keypair.to_le_bytes());
    addr[17..21].copy_from_slice(&chain.to_le_bytes());
    addr[21..25].copy_from_slice(&hash_idx.to_le_bytes());
    addr
}

// ─── WOTS+ ────────────────────────────────────────────────────────────────────

/// Winternitz OTS: chain a value c times
fn chain(x: &[u8; N], start: usize, steps: usize, seed: &[u8; N], addr: &[u8; 32]) -> [u8; N] {
    if steps == 0 { return *x; }
    let mut current = *x;
    for i in start..(start + steps) {
        let mut a = *addr;
        a[21..25].copy_from_slice(&(i as u32).to_le_bytes()); // hash_idx = i
        current = thash(seed, &a, &[&current]);
    }
    current
}

/// Convert message bytes to base-W digits + checksum digits
fn msg_to_wots_digits(msg: &[u8; N]) -> Vec<usize> {
    // Base-W digits for message
    let mut digits: Vec<usize> = Vec::with_capacity(WOTS_LEN);
    for &byte in msg.iter() {
        // 2 digits per byte (LOG_W=2, W=4)
        digits.push(((byte >> 6) & 3) as usize);
        digits.push(((byte >> 4) & 3) as usize);
        digits.push(((byte >> 2) & 3) as usize);
        digits.push((byte & 3) as usize);
    }
    digits.truncate(WOTS_LEN1);

    // Checksum
    let csum: usize = digits.iter().map(|&d| W - 1 - d).sum();
    // Encode checksum in LOG_W bits (WOTS_LEN2 = 3 digits)
    for i in (0..WOTS_LEN2).rev() {
        digits.push((csum >> (i * LOG_W)) & (W - 1));
    }
    digits
}

/// WOTS+ key generation: sk[i] = PRF(sk_seed, addr_i), pk[i] = chain(sk[i], 0, W-1)
pub struct WotsKeypair {
    pub sk: Vec<[u8; N]>,  // WOTS_LEN secret values
    pub pk: Vec<[u8; N]>,  // WOTS_LEN public values (chained W-1 times)
}

impl WotsKeypair {
    pub fn generate(sk_seed: &[u8; N], pk_seed: &[u8; N], addr_base: &[u8; 32]) -> Self {
        let mut sk = Vec::with_capacity(WOTS_LEN);
        let mut pk = Vec::with_capacity(WOTS_LEN);
        for i in 0..WOTS_LEN {
            let mut addr = *addr_base;
            addr[17..21].copy_from_slice(&(i as u32).to_le_bytes()); // chain index
            let sk_i = prf(sk_seed, &addr);
            let pk_i = chain(&sk_i, 0, W - 1, pk_seed, &addr);
            sk.push(sk_i);
            pk.push(pk_i);
        }
        WotsKeypair { sk, pk }
    }

    /// Sign: for digit d_i, chain sk_i from 0 to d_i steps
    pub fn sign(&self, msg: &[u8; N], pk_seed: &[u8; N], addr_base: &[u8; 32]) -> Vec<[u8; N]> {
        let digits = msg_to_wots_digits(msg);
        digits.iter().enumerate().map(|(i, &d)| {
            let mut addr = *addr_base;
            addr[17..21].copy_from_slice(&(i as u32).to_le_bytes());
            chain(&self.sk[i], 0, d, pk_seed, &addr)
        }).collect()
    }

    /// Verify: given sig, complete chains to reconstruct pk
    pub fn verify_sig_to_pk(
        sig: &[[u8; N]],
        msg: &[u8; N],
        pk_seed: &[u8; N],
        addr_base: &[u8; 32],
    ) -> Vec<[u8; N]> {
        let digits = msg_to_wots_digits(msg);
        sig.iter().enumerate().map(|(i, s_i)| {
            let d = digits[i];
            let mut addr = *addr_base;
            addr[17..21].copy_from_slice(&(i as u32).to_le_bytes());
            chain(s_i, d, W - 1 - d, pk_seed, &addr)
        }).collect()
    }
}

// ─── XMSS Tree ────────────────────────────────────────────────────────────────

/// Build 1 XMSS tree of height H_PER
/// Leaves = WOTS+ public keys hashed; internal nodes = thash(left || right)
pub struct XmssTree {
    pub nodes: Vec<Vec<[u8; N]>>,  // nodes[layer][index]
    pub height: usize,
}

impl XmssTree {
    pub fn build(sk_seed: &[u8; N], pk_seed: &[u8; N], layer: u32, tree_idx: u64) -> Self {
        let num_leaves = 1usize << H_PER;
        let height = H_PER;

        // Generate leaves (WOTS+ public key hashes)
        let leaves: Vec<[u8; N]> = (0..num_leaves).map(|i| {
            let addr = make_addr(layer, tree_idx, 0, i as u32, 0, 0);
            let wots = WotsKeypair::generate(sk_seed, pk_seed, &addr);
            // Compress WOTS+ pk to n bytes
            let mut h = blake3::Hasher::new();
            h.update(b"sphincs_wots_pk");
            for pk_i in &wots.pk { h.update(pk_i); }
            let out = *h.finalize().as_bytes();
            let mut r = [0u8; N];
            r.copy_from_slice(&out[..N]);
            r
        }).collect();

        let mut nodes = vec![leaves];

        // Build tree bottom-up
        for lev in 0..height {
            let prev = &nodes[lev];
            let next_len = prev.len() / 2;
            let mut next = Vec::with_capacity(next_len);
            for i in 0..next_len {
                let addr = make_addr(layer, tree_idx, 1, 0, lev as u32, i as u32);
                let node = thash(pk_seed, &addr, &[&prev[2*i], &prev[2*i+1]]);
                next.push(node);
            }
            nodes.push(next);
        }

        XmssTree { nodes, height }
    }

    pub fn root(&self) -> [u8; N] {
        self.nodes[self.height][0]
    }

    /// Authentication path for leaf idx (sibling at each level)
    pub fn auth_path(&self, leaf_idx: usize) -> Vec<[u8; N]> {
        let mut path = Vec::with_capacity(self.height);
        let mut idx = leaf_idx;
        for lev in 0..self.height {
            let sibling = idx ^ 1;
            path.push(self.nodes[lev][sibling]);
            idx >>= 1;
        }
        path
    }

    /// Verify: reconstruct root from leaf + auth_path
    pub fn verify_auth(
        leaf: &[u8; N],
        leaf_idx: usize,
        auth_path: &[[u8; N]],
        pk_seed: &[u8; N],
        layer: u32,
        tree_idx: u64,
    ) -> [u8; N] {
        let mut node = *leaf;
        let mut idx = leaf_idx;
        for (lev, sibling) in auth_path.iter().enumerate() {
            let addr = make_addr(layer, tree_idx, 1, 0, lev as u32, (idx >> 1) as u32);
            node = if idx % 2 == 0 {
                thash(pk_seed, &addr, &[&node, sibling.as_slice()])
            } else {
                thash(pk_seed, &addr, &[sibling.as_slice(), &node])
            };
            idx >>= 1;
        }
        node
    }
}

// ─── FORS ─────────────────────────────────────────────────────────────────────

/// FORS signature: K trees × T leaves
/// Each tree reveals 1 secret leaf + auth path
pub struct ForsSig {
    pub secret_vals: Vec<[u8; N]>,     // K secret values (one per tree)
    pub auth_paths:  Vec<Vec<[u8; N]>>, // K auth paths (A nodes each)
}

impl ForsSig {
    /// Generate FORS trees from sk_seed and sign with given indices
    pub fn sign(sk_seed: &[u8; N], pk_seed: &[u8; N], indices: &[usize], addr_base: u64) -> (Self, [u8; N]) {
        assert_eq!(indices.len(), K);
        let mut secret_vals = Vec::with_capacity(K);
        let mut auth_paths  = Vec::with_capacity(K);
        let mut roots = Vec::with_capacity(K);

        for (tree_idx, &leaf_idx) in indices.iter().enumerate() {
            // Build FORS tree
            let leaves: Vec<[u8; N]> = (0..T).map(|i| {
                let addr = make_addr(0, addr_base + tree_idx as u64, 2, i as u32, 0, 0);
                prf(sk_seed, &addr)  // secret leaf = PRF(sk_seed, addr)
            }).collect();

            // Hash leaves
            let leaf_hashes: Vec<[u8; N]> = leaves.iter().map(|leaf| {
                let addr = make_addr(0, addr_base + tree_idx as u64, 2, 0, 0, 0);
                thash(pk_seed, &addr, &[leaf.as_slice()])
            }).collect();

            // Build auth tree
            let tree = build_simple_tree(&leaf_hashes, pk_seed, addr_base + tree_idx as u64);
            let auth = simple_auth_path(&tree, leaf_idx);
            let root = tree[tree.len() - 1][0];

            secret_vals.push(leaves[leaf_idx]);
            auth_paths.push(auth);
            roots.push(root);
        }

        // FORS public key = hash of all roots
        let fors_pk = hash_concat(&roots, b"sphincs_fors_pk");

        (ForsSig { secret_vals, auth_paths }, fors_pk)
    }

    /// Verify FORS sig → recompute public key
    pub fn verify(&self, indices: &[usize], pk_seed: &[u8; N], addr_base: u64) -> [u8; N] {
        let mut roots = Vec::with_capacity(K);

        for (tree_idx, ((&leaf_idx, secret), auth)) in
            indices.iter().zip(&self.secret_vals).zip(&self.auth_paths).enumerate()
        {
            let addr = make_addr(0, addr_base + tree_idx as u64, 2, 0, 0, 0);
            let leaf_hash = thash(pk_seed, &addr, &[secret.as_slice()]);
            let root = simple_verify_path(&leaf_hash, leaf_idx, auth, pk_seed, addr_base + tree_idx as u64);
            roots.push(root);
        }

        hash_concat(&roots, b"sphincs_fors_pk")
    }
}

/// Build simple Merkle tree from leaves → returns all levels
fn build_simple_tree(leaves: &[[u8; N]], pk_seed: &[u8; N], tree_id: u64) -> Vec<Vec<[u8; N]>> {
    let mut levels = vec![leaves.to_vec()];
    while levels.last().unwrap().len() > 1 {
        let prev = levels.last().unwrap();
        let mut next = Vec::new();
        for i in (0..prev.len()).step_by(2) {
            let addr = make_addr(0, tree_id, 3, 0, (levels.len() - 1) as u32, (i/2) as u32);
            let node = if i + 1 < prev.len() {
                thash(pk_seed, &addr, &[&prev[i], &prev[i+1]])
            } else {
                prev[i]
            };
            next.push(node);
        }
        levels.push(next);
    }
    levels
}

fn simple_auth_path(tree: &[Vec<[u8; N]>], leaf_idx: usize) -> Vec<[u8; N]> {
    let mut path = Vec::new();
    let mut idx = leaf_idx;
    for level in &tree[..tree.len()-1] {
        let sibling = idx ^ 1;
        path.push(if sibling < level.len() { level[sibling] } else { level[idx] });
        idx >>= 1;
    }
    path
}

fn simple_verify_path(leaf: &[u8; N], leaf_idx: usize, path: &[[u8; N]], pk_seed: &[u8; N], tree_id: u64) -> [u8; N] {
    let mut node = *leaf;
    let mut idx = leaf_idx;
    for (lev, sibling) in path.iter().enumerate() {
        let addr = make_addr(0, tree_id, 3, 0, lev as u32, (idx >> 1) as u32);
        node = if idx % 2 == 0 {
            thash(pk_seed, &addr, &[&node, sibling.as_slice()])
        } else {
            thash(pk_seed, &addr, &[sibling.as_slice(), &node])
        };
        idx >>= 1;
    }
    node
}

fn hash_concat(vals: &[[u8; N]], tag: &[u8]) -> [u8; N] {
    let mut h = blake3::Hasher::new();
    h.update(tag);
    for v in vals { h.update(v); }
    let out = *h.finalize().as_bytes();
    let mut r = [0u8; N];
    r.copy_from_slice(&out[..N]);
    r
}

// ─── HyperTree Signature ──────────────────────────────────────────────────────

/// One XMSS layer signature: WOTS+ sig + auth path
pub struct XmssSig {
    pub wots_sig:  Vec<[u8; N]>,    // WOTS_LEN values
    pub auth_path: Vec<[u8; N]>,    // H_PER nodes
}

/// HyperTree signature: D XMSS signatures stacked
pub struct HtSig {
    pub layers: Vec<XmssSig>,
}

impl HtSig {
    pub fn sign(
        sk_seed: &[u8; N],
        pk_seed: &[u8; N],
        msg_to_sign: &[u8; N],  // FORS root (bottom) or tree root (higher)
        ht_idx: u64,             // index within HyperTree
    ) -> Self {
        let mut layers = Vec::with_capacity(D);
        let mut current_msg = *msg_to_sign;
        let mut current_idx = ht_idx;

        for layer in 0..D {
            let tree_idx   = current_idx >> H_PER;
            let leaf_idx   = (current_idx & ((1 << H_PER) - 1)) as usize;

            // Build XMSS tree at this layer
            let xmss = XmssTree::build(sk_seed, pk_seed, layer as u32, tree_idx);

            // WOTS+ sign current_msg as this leaf
            let addr = make_addr(layer as u32, tree_idx, 0, leaf_idx as u32, 0, 0);
            let wots = WotsKeypair::generate(sk_seed, pk_seed, &addr);
            let wots_sig = wots.sign(&current_msg, pk_seed, &addr);

            // Auth path to prove leaf is in this tree
            let auth = xmss.auth_path(leaf_idx);

            layers.push(XmssSig { wots_sig, auth_path: auth });

            // Next layer signs this tree's root
            current_msg = xmss.root();
            current_idx = tree_idx;
        }

        HtSig { layers }
    }

    /// Verify: reconstruct root of top-level tree
    pub fn verify(
        &self,
        msg: &[u8; N],
        pk_seed: &[u8; N],
        ht_idx: u64,
    ) -> [u8; N] {
        let mut current_msg = *msg;
        let mut current_idx = ht_idx;

        for (layer, xmss_sig) in self.layers.iter().enumerate() {
            let tree_idx  = current_idx >> H_PER;
            let leaf_idx  = (current_idx & ((1 << H_PER) - 1)) as usize;

            let addr = make_addr(layer as u32, tree_idx, 0, leaf_idx as u32, 0, 0);

            // Reconstruct WOTS+ PK from sig
            let pk_vals = WotsKeypair::verify_sig_to_pk(
                &xmss_sig.wots_sig, &current_msg, pk_seed, &addr
            );

            // Hash WOTS+ PK to leaf
            let mut h = blake3::Hasher::new();
            h.update(b"sphincs_wots_pk");
            for v in &pk_vals { h.update(v); }
            let out = *h.finalize().as_bytes();
            let mut leaf = [0u8; N];
            leaf.copy_from_slice(&out[..N]);

            // Walk auth path to tree root
            let root = XmssTree::verify_auth(
                &leaf, leaf_idx, &xmss_sig.auth_path,
                pk_seed, layer as u32, tree_idx
            );

            current_msg = root;
            current_idx = tree_idx;
        }

        current_msg
    }
}

// ─── Keys & Signature ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SphincsPublicKey {
    pub pk_seed: [u8; N],
    pub pk_root: [u8; N],    // root of top-level HyperTree
}

#[derive(Debug, Clone)]
pub struct SphincsSecretKey {
    pub sk_seed: [u8; N],
    pub sk_prf:  [u8; N],
    pub pk_seed: [u8; N],
    pub pk_root: [u8; N],
}

pub struct SphincsSignature {
    pub r:        [u8; N],
    pub fors_sig: ForsSig,
    pub ht_sig:   HtSig,
}

// ─── Keygen ───────────────────────────────────────────────────────────────────

pub fn keygen(seed: &[u8]) -> (SphincsPublicKey, SphincsSecretKey) {
    let mut h = blake3::Hasher::new();
    h.update(b"sphincs_keygen_v31");
    h.update(seed);
    let master = *h.finalize().as_bytes();

    let mut sk_seed = [0u8; N];
    let mut sk_prf  = [0u8; N];
    let mut pk_seed = [0u8; N];
    sk_seed.copy_from_slice(&master[0..N]);
    sk_prf .copy_from_slice(&master[N..2*N]);
    pk_seed.copy_from_slice(&master[N..2*N]); // reuse for demo

    // pk_root = root of top-level XMSS tree (layer D-1)
    let top_tree = XmssTree::build(&sk_seed, &pk_seed, (D - 1) as u32, 0);
    let pk_root = top_tree.root();

    let pk = SphincsPublicKey { pk_seed, pk_root };
    let sk = SphincsSecretKey { sk_seed, sk_prf, pk_seed, pk_root };
    (pk, sk)
}

// ─── Sign ─────────────────────────────────────────────────────────────────────

pub fn sign(sk: &SphincsSecretKey, msg: &[u8]) -> SphincsSignature {
    // 1. Randomize message
    let opt_rand = sk.pk_seed; // in real: fresh randomness
    let r = prf_msg(&sk.sk_prf, &opt_rand, msg);

    // 2. Digest
    let digest = h_msg(&r, &sk.pk_seed, &sk.pk_root, msg);

    // 3. Split digest into FORS indices + HyperTree index
    let (fors_indices, ht_idx) = split_digest(&digest);

    // 4. FORS sign
    let fors_addr_base = ht_idx & 0xFFFF;
    let (fors_sig, fors_pk) = ForsSig::sign(&sk.sk_seed, &sk.pk_seed, &fors_indices, fors_addr_base);

    // 5. HyperTree sign FORS PK
    let ht_sig = HtSig::sign(&sk.sk_seed, &sk.pk_seed, &fors_pk, ht_idx);

    SphincsSignature { r, fors_sig, ht_sig }
}

// ─── Verify ───────────────────────────────────────────────────────────────────

pub fn verify(pk: &SphincsPublicKey, msg: &[u8], sig: &SphincsSignature) -> bool {
    // 1. Recompute digest
    let digest = h_msg(&sig.r, &pk.pk_seed, &pk.pk_root, msg);

    // 2. Split digest
    let (fors_indices, ht_idx) = split_digest(&digest);

    // 3. Recompute FORS PK
    let fors_addr_base = ht_idx & 0xFFFF;
    let fors_pk = sig.fors_sig.verify(&fors_indices, &pk.pk_seed, fors_addr_base);

    // 4. Verify HyperTree → should recover pk_root
    let computed_root = sig.ht_sig.verify(&fors_pk, &pk.pk_seed, ht_idx);

    computed_root == pk.pk_root
}

/// Extract FORS indices (K values in [0, T)) and HyperTree index from digest
fn split_digest(digest: &[u8]) -> (Vec<usize>, u64) {
    // First K bits-groups for FORS (A bits each = 2 bits each for K=4)
    let mut fors_indices = Vec::with_capacity(K);
    for i in 0..K {
        let byte_idx = i / 4;
        let bit_off  = (i % 4) * 2;
        let idx = ((digest[byte_idx % digest.len()] >> bit_off) & (T as u8 - 1)) as usize;
        fors_indices.push(idx);
    }

    // HyperTree index from remaining bytes
    let ht_idx = u64::from_le_bytes(digest[4..12].try_into().unwrap_or([0u8; 8]))
        & ((1u64 << H) - 1);

    (fors_indices, ht_idx)
}

// ─── Convenience wrapper ──────────────────────────────────────────────────────

pub struct SphincsKeypair {
    pub pk:      SphincsPublicKey,
    pub sk:      SphincsSecretKey,
    pub address: String,
}

impl SphincsKeypair {
    pub fn generate(seed: &[u8]) -> Self {
        let (pk, sk) = keygen(seed);
        let mut h = blake3::Hasher::new();
        h.update(b"sphincs_address");
        h.update(&pk.pk_seed);
        h.update(&pk.pk_root);
        let addr = format!("spx1{}", &hex::encode(h.finalize().as_bytes())[..40]);
        SphincsKeypair { pk, sk, address: addr }
    }

    pub fn sign(&self, msg: &[u8]) -> SphincsSignature {
        sign(&self.sk, msg)
    }

    pub fn verify(&self, msg: &[u8], sig: &SphincsSignature) -> bool {
        verify(&self.pk, msg, sig)
    }
}

// ─── Signature sizes (real SPHINCS+ parameters) ───────────────────────────────

pub struct SphincsParams {
    pub name:      &'static str,
    pub n:         usize,
    pub h:         usize,
    pub d:         usize,
    pub k:         usize,
    pub a:         usize,
    pub w:         usize,
    pub pk_bytes:  usize,
    pub sk_bytes:  usize,
    pub sig_bytes: usize,
    pub security:  &'static str,
    pub variant:   &'static str,
}

pub const SPHINCS_PARAMS: [SphincsParams; 6] = [
    SphincsParams { name: "SPHINCS+-SHA2-128s", n:16, h:63, d:7,  k:14, a:12, w:16, pk_bytes:32,  sk_bytes:64,  sig_bytes:7856,  security:"128-bit", variant:"small sig" },
    SphincsParams { name: "SPHINCS+-SHA2-128f", n:16, h:66, d:22, k:33, a:6,  w:16, pk_bytes:32,  sk_bytes:64,  sig_bytes:17088, security:"128-bit", variant:"fast sign" },
    SphincsParams { name: "SPHINCS+-SHA2-192s", n:24, h:63, d:7,  k:17, a:14, w:16, pk_bytes:48,  sk_bytes:96,  sig_bytes:16224, security:"192-bit", variant:"small sig" },
    SphincsParams { name: "SPHINCS+-SHA2-192f", n:24, h:66, d:22, k:33, a:8,  w:16, pk_bytes:48,  sk_bytes:96,  sig_bytes:35664, security:"192-bit", variant:"fast sign" },
    SphincsParams { name: "SPHINCS+-SHA2-256s", n:32, h:64, d:8,  k:22, a:14, w:16, pk_bytes:64,  sk_bytes:128, sig_bytes:29792, security:"256-bit", variant:"small sig" },
    SphincsParams { name: "SPHINCS+-SHA2-256f", n:32, h:68, d:17, k:35, a:9,  w:16, pk_bytes:64,  sk_bytes:128, sig_bytes:49856, security:"256-bit", variant:"fast sign" },
];
