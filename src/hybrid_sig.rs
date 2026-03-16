#![allow(dead_code)]

/// v3.3 — Hybrid Signature: ECDSA + Dilithium (Migration Path)
///
/// Bridges classical and post-quantum cryptography during transition period.
///
/// ─── Migration Path ────────────────────────────────────────────────────────
///
///   Phase 1 — Classical (today):
///     Sign with ECDSA only. Validators verify ECDSA.
///
///   Phase 2 — Hybrid (transition, 2026–2030):
///     Sign with ECDSA + Dilithium. Both sigs embedded in TX.
///     Old validators: verify ECDSA only (backward compat).
///     New validators: verify BOTH (max security).
///
///   Phase 3 — Post-Quantum (future):
///     Sign with Dilithium only. ECDSA key dropped.
///     All validators verify Dilithium.
///
/// ─── Why Hybrid? ─────────────────────────────────────────────────────────
///
///   Defense in depth:
///     - If quantum computer breaks ECDSA → Dilithium still holds
///     - If Dilithium has a flaw → ECDSA still holds (unlikely for known attacks)
///
///   No "harvest now, decrypt later" risk:
///     - Hybrid signature requires breaking BOTH simultaneously
///     - Attacker cannot forge past hybrid txs even with future quantum computer
///
/// ─── Verification Policies ───────────────────────────────────────────────
///
///   AND  (strict):   Both ECDSA + Dilithium must verify — maximum security
///   OR   (lenient):  Either one sufficient — backward compat during rollout
///   CLASSICAL_ONLY:  Only ECDSA — for old nodes that don't know PQ
///   PQ_ONLY:         Only Dilithium — for upgraded full-PQ nodes
///
/// References: NIST SP 800-208, ETSI TS 119 312, Draft RFC 9691

use secp256k1::{Secp256k1, SecretKey as EcdsaSk, PublicKey as EcdsaPk, Message};
use crate::dilithium::{DilithiumKeypair, Signature as DilSig};

// ─── Modes ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum SigMode {
    Classical,  // ECDSA only (pre-quantum)
    Hybrid,     // ECDSA + Dilithium (transition)
    PQ,         // Dilithium only (post-quantum)
}

#[derive(Clone, Debug, PartialEq)]
pub enum VerifyPolicy {
    And,           // Both must verify — maximum security
    Or,            // Either suffices — backward compatible
    ClassicalOnly, // Only ECDSA — old node
    PqOnly,        // Only Dilithium — full-PQ node
}

// ─── Signature ────────────────────────────────────────────────────────────────

pub struct HybridSignature {
    pub mode:      SigMode,
    pub ecdsa_sig: Option<[u8; 64]>,   // compact ECDSA signature
    pub dil_sig:   Option<DilSig>,     // Dilithium signature
}

impl HybridSignature {
    /// Total byte cost (approximate, for comparison)
    pub fn size_bytes(&self) -> usize {
        let ecdsa = if self.ecdsa_sig.is_some() { 64 } else { 0 };
        // Dilithium2 sig ≈ 2420 bytes (z vector: L=4 polys × N=256 × 8 bytes)
        let dil = if self.dil_sig.is_some() { 2420 } else { 0 };
        ecdsa + dil
    }
}

// ─── Keypair ──────────────────────────────────────────────────────────────────

pub struct HybridKeypair {
    pub mode:      SigMode,
    pub ecdsa_sk:  Option<EcdsaSk>,
    pub ecdsa_pk:  Option<EcdsaPk>,
    pub dil_kp:    Option<DilithiumKeypair>,
    pub address:   String,
}

impl HybridKeypair {
    pub fn generate(seed: &[u8], mode: SigMode) -> Self {
        let (ecdsa_sk, ecdsa_pk) = match mode {
            SigMode::Classical | SigMode::Hybrid => {
                let sk_bytes = h256(b"hybrid_ecdsa_sk", seed);
                let sk = EcdsaSk::from_slice(&sk_bytes).expect("valid 32-byte key");
                let secp = Secp256k1::new();
                let pk = EcdsaPk::from_secret_key(&secp, &sk);
                (Some(sk), Some(pk))
            }
            SigMode::PQ => (None, None),
        };

        let dil_kp = match mode {
            SigMode::Hybrid | SigMode::PQ => {
                let dil_seed = h256(b"hybrid_dilithium_sk", seed);
                Some(DilithiumKeypair::generate(&dil_seed))
            }
            SigMode::Classical => None,
        };

        // Address = H(ecdsa_pk_bytes ‖ dil_pk_seed)
        let mut h = blake3::Hasher::new();
        h.update(b"hybrid_address");
        if let Some(pk) = &ecdsa_pk {
            h.update(&pk.serialize());
        }
        if let Some(kp) = &dil_kp {
            h.update(&kp.pk.seed_a);
        }
        let addr_hash = *h.finalize().as_bytes();
        let prefix = match mode {
            SigMode::Classical => "cls1",
            SigMode::Hybrid    => "hyb1",
            SigMode::PQ        => "pq1_",
        };
        let address = format!("{}{}", prefix, &hex::encode(&addr_hash)[..40]);

        HybridKeypair { mode, ecdsa_sk, ecdsa_pk, dil_kp, address }
    }

    /// Sign message with whichever keys this keypair holds
    pub fn sign(&self, msg: &[u8]) -> HybridSignature {
        let ecdsa_sig = self.ecdsa_sk.as_ref().map(|sk| ecdsa_sign(sk, msg));
        let dil_sig   = self.dil_kp.as_ref().map(|kp| kp.sign(msg));

        HybridSignature {
            mode: self.mode.clone(),
            ecdsa_sig,
            dil_sig,
        }
    }

    /// Verify with default policy: AND for Hybrid, ECDSA-only for Classical, PQ-only for PQ
    pub fn verify_sig(&self, msg: &[u8], sig: &HybridSignature) -> bool {
        let policy = match self.mode {
            SigMode::Classical => VerifyPolicy::ClassicalOnly,
            SigMode::PQ        => VerifyPolicy::PqOnly,
            SigMode::Hybrid    => VerifyPolicy::And,
        };
        self.verify_with_policy(msg, sig, policy)
    }

    /// Verify with explicit policy
    pub fn verify_with_policy(&self, msg: &[u8], sig: &HybridSignature, policy: VerifyPolicy) -> bool {
        let ecdsa_ok = verify_ecdsa(self.ecdsa_pk.as_ref(), msg, sig.ecdsa_sig.as_ref());
        let dil_ok   = verify_dil(self.dil_kp.as_ref(), msg, sig.dil_sig.as_ref());

        match policy {
            VerifyPolicy::And          => ecdsa_ok && dil_ok,
            VerifyPolicy::Or           => ecdsa_ok || dil_ok,
            VerifyPolicy::ClassicalOnly => ecdsa_ok,
            VerifyPolicy::PqOnly        => dil_ok,
        }
    }
}

// ─── ECDSA primitives ─────────────────────────────────────────────────────────

fn ecdsa_sign(sk: &EcdsaSk, msg: &[u8]) -> [u8; 64] {
    let secp  = Secp256k1::new();
    let hash  = h256(b"hybrid_msg_hash", msg);
    let m     = Message::from_slice(&hash).expect("32 bytes");
    secp.sign_ecdsa(&m, sk).serialize_compact()
}

fn verify_ecdsa(pk: Option<&EcdsaPk>, msg: &[u8], sig_bytes: Option<&[u8; 64]>) -> bool {
    match (pk, sig_bytes) {
        (Some(pk), Some(sig_bytes)) => {
            let secp = Secp256k1::new();
            let hash = h256(b"hybrid_msg_hash", msg);
            let m = match Message::from_slice(&hash) { Ok(m) => m, Err(_) => return false };
            let sig = match secp256k1::ecdsa::Signature::from_compact(sig_bytes) {
                Ok(s) => s, Err(_) => return false
            };
            secp.verify_ecdsa(&m, &sig, pk).is_ok()
        }
        (None, None) => true,   // not applicable to this mode — pass
        _            => false,  // key/sig mismatch
    }
}

fn verify_dil(kp: Option<&DilithiumKeypair>, msg: &[u8], sig: Option<&DilSig>) -> bool {
    match (kp, sig) {
        (Some(kp), Some(sig)) => kp.verify(msg, sig),
        (None, None)          => true,
        _                     => false,
    }
}

// ─── Migration state machine ──────────────────────────────────────────────────

/// Represents a wallet at a specific migration phase
pub struct WalletState {
    pub phase:   &'static str,
    pub keypair: HybridKeypair,
}

impl WalletState {
    /// Phase 1: Classical ECDSA wallet
    pub fn phase1_classical(seed: &[u8]) -> Self {
        WalletState {
            phase:   "Phase 1 — Classical (ECDSA only)",
            keypair: HybridKeypair::generate(seed, SigMode::Classical),
        }
    }

    /// Phase 2: Hybrid — add Dilithium key alongside ECDSA
    pub fn phase2_hybrid(seed: &[u8]) -> Self {
        WalletState {
            phase:   "Phase 2 — Hybrid (ECDSA + Dilithium)",
            keypair: HybridKeypair::generate(seed, SigMode::Hybrid),
        }
    }

    /// Phase 3: Full PQ — drop ECDSA
    pub fn phase3_pq(seed: &[u8]) -> Self {
        WalletState {
            phase:   "Phase 3 — Post-Quantum (Dilithium only)",
            keypair: HybridKeypair::generate(seed, SigMode::PQ),
        }
    }
}

// ─── Hash helper ─────────────────────────────────────────────────────────────

fn h256(label: &[u8], data: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(label);
    h.update(data);
    *h.finalize().as_bytes()
}

// ─── Quantum threat model ─────────────────────────────────────────────────────

pub struct ThreatAnalysis {
    pub scheme:              &'static str,
    pub classical_security:  &'static str,
    pub quantum_security:    &'static str,
    pub harvest_attack:      &'static str,  // "harvest now, decrypt later"
    pub migration_urgency:   &'static str,
}

pub const THREAT_TABLE: [ThreatAnalysis; 4] = [
    ThreatAnalysis {
        scheme:             "ECDSA-only",
        classical_security: "128-bit",
        quantum_security:   "0-bit (Shor breaks in poly time)",
        harvest_attack:     "VULNERABLE — past sigs can be forged retroactively",
        migration_urgency:  "CRITICAL — migrate now",
    },
    ThreatAnalysis {
        scheme:             "Hybrid (ECDSA + Dilithium)",
        classical_security: "128-bit (ECDSA)",
        quantum_security:   "128-bit (Dilithium holds even if ECDSA broken)",
        harvest_attack:     "SAFE — both must break simultaneously",
        migration_urgency:  "LOW — protected during transition",
    },
    ThreatAnalysis {
        scheme:             "Dilithium-only",
        classical_security: "128-bit (Module-LWE hard classically)",
        quantum_security:   "128-bit (BKZ lattice reduction is best known)",
        migration_urgency:  "NONE — fully post-quantum",
        harvest_attack:     "SAFE — Shor does not apply to LWE",
    },
    ThreatAnalysis {
        scheme:             "SPHINCS+-only",
        classical_security: "128-bit (hash collision)",
        quantum_security:   "64-bit (Grover halves collision resistance)",
        harvest_attack:     "SAFE — hash-only, no algebraic structure",
        migration_urgency:  "NONE — use n=256 for full PQ security",
    },
];
