#![allow(dead_code)]
//! Taproot — v1.3 (BIP340/341/342)
//!
//! Taproot = Schnorr + MAST (Merkelized Alternative Script Trees)
//!
//! Core ideas:
//!   1. Schnorr signatures — 64 bytes, linearly aggregatable, batch verifiable
//!   2. Key path spend: Q = P + H(P||merkle_root)·G
//!      → spend bằng tweaked key, không reveal scripts (privacy)
//!   3. Script path spend: reveal 1 leaf script + merkle proof
//!      → chỉ lộ script đang dùng, các scripts khác ẩn (efficiency + privacy)
//!
//! P2TR output:
//!   scriptPubKey: OP_1 <32-byte x-only pubkey Q>
//!   witness (key path):    [<schnorr_sig>]
//!   witness (script path): [<script_inputs...>, <leaf_script>, <control_block>]
//!
//! TapLeaf: tagged_hash("TapLeaf", version || script_bytes)
//! TapBranch: tagged_hash("TapBranch", sorted(left, right))
//! TapTweak: Q = P + tagged_hash("TapTweak", P||merkle_root) * G

use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};

// ── Tagged Hash (BIP340) ─────────────────────────────────────
//
// tagged_hash(tag, data) = SHA256(SHA256(tag) || SHA256(tag) || data)
// Ngăn hash collision giữa các domain khác nhau

pub fn tagged_hash(tag: &str, data: &[u8]) -> [u8; 32] {
    let tag_hash = Sha256::digest(tag.as_bytes());
    let mut h    = Sha256::new();
    h.update(&tag_hash);
    h.update(&tag_hash);
    h.update(data);
    h.finalize().into()
}

// ── Schnorr Signature (BIP340) ───────────────────────────────

/// Ký Schnorr với secp256k1
/// BIP340: sign(seckey, msg) → 64-byte sig [R_x (32) || s (32)]
pub fn schnorr_sign(secret_key: &secp256k1::SecretKey, msg: &[u8]) -> [u8; 64] {
    use secp256k1::{Secp256k1, KeyPair, Message};
    let secp    = Secp256k1::new();
    let keypair = KeyPair::from_secret_key(&secp, secret_key);
    let sighash = tagged_hash("TapSighash", msg);
    let msg     = Message::from_slice(&sighash).expect("32 bytes");
    let sig     = secp.sign_schnorr(&msg, &keypair);
    // sig.as_ref() → &[u8], copy vào [u8;64]
    let bytes: &[u8] = sig.as_ref();
    let mut out = [0u8; 64];
    out.copy_from_slice(bytes);
    out
}

/// Xác thực Schnorr signature
pub fn schnorr_verify(pubkey_bytes: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    use secp256k1::{Secp256k1, XOnlyPublicKey, Message, schnorr::Signature};
    let secp    = Secp256k1::new();
    let xonly   = match XOnlyPublicKey::from_slice(pubkey_bytes) { Ok(k) => k, Err(_) => return false };
    let sighash = tagged_hash("TapSighash", msg);
    let msg     = match Message::from_slice(&sighash)            { Ok(m) => m, Err(_) => return false };
    let sig     = match Signature::from_slice(sig)               { Ok(s) => s, Err(_) => return false };
    secp.verify_schnorr(&sig, &msg, &xonly).is_ok()
}

/// Lấy x-only pubkey (32 bytes) từ secp256k1::PublicKey
pub fn x_only(pubkey: &secp256k1::PublicKey) -> [u8; 32] {
    let serialized = pubkey.serialize(); // 33 bytes compressed
    serialized[1..33].try_into().expect("32 bytes")
}

// ── TapLeaf ──────────────────────────────────────────────────

/// Một lá trong Taproot script tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapLeaf {
    pub version: u8,        // 0xc0 = Tapscript version
    pub script:  Vec<u8>,   // serialized script
}

impl TapLeaf {
    pub fn new(script: Vec<u8>) -> Self {
        TapLeaf { version: 0xc0, script }
    }

    /// tagged_hash("TapLeaf", version || compact_size || script)
    pub fn hash(&self) -> [u8; 32] {
        let mut data = vec![self.version];
        // compact_size encoding của script length
        let len = self.script.len();
        if len < 0xfd {
            data.push(len as u8);
        } else {
            data.push(0xfd);
            data.push((len & 0xff) as u8);
            data.push((len >> 8) as u8);
        }
        data.extend_from_slice(&self.script);
        tagged_hash("TapLeaf", &data)
    }
}

// ── TapBranch / Merkle Tree ──────────────────────────────────

/// Node trong Taproot Merkle tree
#[derive(Debug, Clone)]
pub enum TapNode {
    Leaf(TapLeaf),
    Branch(Box<TapNode>, Box<TapNode>),
}

impl TapNode {
    /// Hash của node
    pub fn hash(&self) -> [u8; 32] {
        match self {
            TapNode::Leaf(l) => l.hash(),
            TapNode::Branch(l, r) => {
                let lh = l.hash();
                let rh = r.hash();
                // Sort để đảm bảo thứ tự deterministic
                let mut data = Vec::with_capacity(64);
                if lh <= rh {
                    data.extend_from_slice(&lh);
                    data.extend_from_slice(&rh);
                } else {
                    data.extend_from_slice(&rh);
                    data.extend_from_slice(&lh);
                }
                tagged_hash("TapBranch", &data)
            }
        }
    }

    /// Tìm merkle proof cho một leaf (DFS)
    /// Returns: Vec<([u8;32], bool)> — (sibling_hash, is_left)
    pub fn proof_for(&self, leaf_hash: &[u8; 32]) -> Option<Vec<[u8; 32]>> {
        match self {
            TapNode::Leaf(l) => {
                if &l.hash() == leaf_hash { Some(vec![]) } else { None }
            }
            TapNode::Branch(left, right) => {
                if let Some(mut proof) = left.proof_for(leaf_hash) {
                    proof.push(right.hash());
                    return Some(proof);
                }
                if let Some(mut proof) = right.proof_for(leaf_hash) {
                    proof.push(left.hash());
                    return Some(proof);
                }
                None
            }
        }
    }
}

// ── TapTweak ─────────────────────────────────────────────────

/// Tính tweaked public key Q từ internal key P và merkle root
/// Q = P + tagged_hash("TapTweak", P_x || merkle_root) * G
///
/// Simplified: chúng ta compute tweak hash và combine deterministically
/// (không thực sự dùng EC point addition vì cần low-level secp256k1 ops)
pub fn tap_tweak_hash(internal_key_xonly: &[u8; 32], merkle_root: Option<&[u8; 32]>) -> [u8; 32] {
    let mut data = internal_key_xonly.to_vec();
    if let Some(root) = merkle_root {
        data.extend_from_slice(root);
    }
    tagged_hash("TapTweak", &data)
}

/// Tính tweaked x-only pubkey Q = P + tweak*G
/// Dùng secp256k1 library để thực hiện EC addition đúng chuẩn BIP341
pub fn tap_tweak_pubkey(
    internal_key: &secp256k1::PublicKey,
    merkle_root:  Option<&[u8; 32]>,
) -> (secp256k1::PublicKey, [u8; 32]) {
    use secp256k1::{Secp256k1, SecretKey, PublicKey};
    let secp  = Secp256k1::new();
    let xonly = x_only(internal_key);
    let tweak = tap_tweak_hash(&xonly, merkle_root);

    // Q = P + tweak*G
    // secp256k1 0.27: dùng combine() — tạo tweak*G rồi cộng vào P
    let tweak_sk  = SecretKey::from_slice(&tweak).expect("valid tweak");
    let tweak_pub = PublicKey::from_secret_key(&secp, &tweak_sk); // tweak*G
    let tweaked   = internal_key.combine(&tweak_pub).expect("tweak combine");
    (tweaked, tweak)
}

// ── P2TR (Pay-to-Taproot) ────────────────────────────────────

/// Taproot output descriptor
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TaprootOutput {
    pub internal_key:    secp256k1::PublicKey,
    pub script_tree:     Option<TapNode>,
    pub tweaked_key:     secp256k1::PublicKey,
    pub tweaked_secret:  Option<secp256k1::SecretKey>, // tweaked sk để sign key path
    pub merkle_root:     Option<[u8; 32]>,
}

impl TaprootOutput {
    /// Key-path only (không có script tree)
    /// Dùng khi chỉ cần 1 người ký, không cần script conditions
    pub fn key_path_only(internal_key: secp256k1::PublicKey) -> Self {
        let (tweaked, _) = tap_tweak_pubkey(&internal_key, None);
        TaprootOutput {
            internal_key,
            script_tree:    None,
            tweaked_key:    tweaked,
            tweaked_secret: None, // set via with_secret_key()
            merkle_root:    None,
        }
    }

    /// Attach tweaked secret key để dùng cho key path spend
    /// tweaked_sk = internal_sk + tweak (mod n)
    pub fn with_secret_key(mut self, internal_sk: &secp256k1::SecretKey) -> Self {
        let xonly = x_only(&self.internal_key);
        let tweak = tap_tweak_hash(&xonly, self.merkle_root.as_ref());
        let tweak_sk = secp256k1::SecretKey::from_slice(&tweak).expect("valid tweak");
        let tweaked  = internal_sk.add_tweak(&secp256k1::Scalar::from(tweak_sk)).expect("add tweak");
        self.tweaked_secret = Some(tweaked);
        self
    }

    /// Script tree — có thể spend bằng key path HOẶC script path
    pub fn with_scripts(internal_key: secp256k1::PublicKey, tree: TapNode) -> Self {
        let merkle_root = tree.hash();
        let (tweaked, _) = tap_tweak_pubkey(&internal_key, Some(&merkle_root));
        TaprootOutput {
            internal_key,
            script_tree:    Some(tree),
            tweaked_key:    tweaked,
            tweaked_secret: None,
            merkle_root:    Some(merkle_root),
        }
    }

    /// x-only bytes của tweaked output key Q (32 bytes)
    /// Đây là giá trị đưa vào scriptPubKey
    pub fn output_key_xonly(&self) -> [u8; 32] {
        x_only(&self.tweaked_key)
    }

    /// scriptPubKey: OP_1 <32-byte x-only tweaked pubkey>
    pub fn script_pubkey_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0x51]; // OP_1 (witness version 1)
        bytes.push(0x20);           // push 32 bytes
        bytes.extend_from_slice(&self.output_key_xonly());
        bytes
    }

    /// Verify key path spend:
    ///   witness = [schnorr_sig]
    ///   sig phải được tạo bởi tweaked key Q
    pub fn verify_key_path(&self, msg: &[u8], sig: &[u8; 64]) -> bool {
        let qx = self.output_key_xonly();
        schnorr_verify(&qx, msg, sig)
    }

    /// Verify script path spend:
    ///   witness = [...script_inputs, leaf_script_bytes, control_block]
    ///   control_block = version || internal_key_xonly || merkle_proof
    pub fn verify_script_path(
        &self,
        leaf_script: &TapLeaf,
        merkle_proof: &[[u8; 32]],
    ) -> bool {
        // 1. Recompute merkle root từ leaf + proof
        let mut current = leaf_script.hash();
        for sibling in merkle_proof {
            let mut data = Vec::with_capacity(64);
            if current <= *sibling {
                data.extend_from_slice(&current);
                data.extend_from_slice(sibling);
            } else {
                data.extend_from_slice(sibling);
                data.extend_from_slice(&current);
            }
            current = tagged_hash("TapBranch", &data);
        }
        let recomputed_root = current;

        // 2. Recompute tweaked key Q từ internal key + recomputed root
        let (expected_tweaked, _) = tap_tweak_pubkey(&self.internal_key, Some(&recomputed_root));
        let expected_xonly = x_only(&expected_tweaked);
        let actual_xonly   = self.output_key_xonly();

        expected_xonly == actual_xonly
    }
}

// ── Key Aggregation (MuSig2 simplified) ─────────────────────
//
// MuSig2: nhiều bên tạo 1 Schnorr signature duy nhất
// aggregate_key = H(L || P1) * P1 + H(L || P2) * P2 + ...
// Bên ngoài nhìn vào không biết có bao nhiêu người ký
// Simplified version: dùng sum of tweaked keys

pub struct KeyAggContext {
    pub pubkeys: Vec<secp256k1::PublicKey>,
}

impl KeyAggContext {
    pub fn new(mut pubkeys: Vec<secp256k1::PublicKey>) -> Self {
        // Sort pubkeys để deterministic
        pubkeys.sort_by_key(|k| k.serialize());
        KeyAggContext { pubkeys }
    }

    /// Tạo aggregated key tag — dùng để tính coefficient từng key
    pub fn key_list_hash(&self) -> [u8; 32] {
        let mut data = Vec::new();
        for pk in &self.pubkeys {
            data.extend_from_slice(&pk.serialize());
        }
        tagged_hash("KeyAgg list", &data)
    }

    /// Coefficient cho key i: H(L || P_i)
    pub fn key_coefficient(&self, index: usize) -> [u8; 32] {
        let l_hash = self.key_list_hash();
        let pk     = &self.pubkeys[index].serialize();
        let mut data = l_hash.to_vec();
        data.extend_from_slice(pk);
        tagged_hash("KeyAgg coefficient", &data)
    }

    /// Aggregate public key (simplified — concatenate + hash for demo)
    pub fn aggregate_xonly(&self) -> [u8; 32] {
        let mut data = Vec::new();
        for (i, pk) in self.pubkeys.iter().enumerate() {
            let coeff = self.key_coefficient(i);
            data.extend_from_slice(&coeff);
            data.extend_from_slice(&pk.serialize());
        }
        tagged_hash("KeyAgg aggregate", &data)
    }

    pub fn describe(&self) -> String {
        format!("{}-key MuSig2 aggregate", self.pubkeys.len())
    }
}
