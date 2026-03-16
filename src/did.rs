#![allow(dead_code)]

/// v3.6 — Decentralized Identity (DID)
///
/// W3C DID Core Specification (https://www.w3.org/TR/did-core/)
///
/// ─── DID Format ──────────────────────────────────────────────────────────────
///
///   did:chain:<method-specific-id>
///   │   │       └─ hex(SHA256(compressed_pubkey)[..16]) — globally unique
///   │   └─ method name ("chain" = our blockchain)
///   └─ scheme
///
///   Example: did:chain:a3f8c2e1b7d90456
///
/// ─── DID Document ────────────────────────────────────────────────────────────
///
///   Contains:
///     verificationMethod — cryptographic keys (ECDSA, etc.)
///     authentication     — keys that can authenticate
///     assertionMethod    — keys for making claims / signing VCs
///     keyAgreement       — keys for key exchange (ECDH)
///     service            — service endpoints (DIDComm, LinkedDomains…)
///
/// ─── Verifiable Credentials (W3C VC Data Model) ──────────────────────────────
///
///   Issuer   → signs credential about Subject
///   Subject  → presents credential to Verifier
///   Verifier → resolves Issuer DID, verifies signature
///
///   Flow:
///     University issues "BachelorDegree" VC to Alice
///     Alice presents VC to Employer
///     Employer resolves did:chain:<university>, verifies signature
///
/// ─── DID Auth ────────────────────────────────────────────────────────────────
///
///   1. Verifier issues challenge (nonce + domain)
///   2. Subject signs challenge with DID authentication key
///   3. Verifier resolves DID, checks signature — proves DID control
///
/// References: W3C DID Core 1.0, W3C VC Data Model 2.0,
///             DIF DID Auth, did:key method spec

use secp256k1::{Secp256k1, SecretKey, PublicKey, Message};
use std::collections::HashMap;

// ─── DID ──────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Did {
    pub method: String,  // e.g. "chain"
    pub id:     String,  // method-specific identifier
}

impl Did {
    /// Parse "did:method:id"
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() == 3 && parts[0] == "did" {
            Some(Did { method: parts[1].to_string(), id: parts[2].to_string() })
        } else {
            None
        }
    }

    /// Derive DID from compressed public key bytes
    pub fn from_pubkey(pk_bytes: &[u8]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"did_id_v1");
        h.update(pk_bytes);
        let out = *h.finalize().as_bytes();
        Did {
            method: "chain".to_string(),
            id:     hex::encode(&out[..8]),  // 16 hex chars — compact, collision-safe for demo
        }
    }

    pub fn to_string(&self) -> String {
        format!("did:{}:{}", self.method, self.id)
    }

    /// Key fragment: "did:chain:xxx#key-1"
    pub fn key_ref(&self, n: u32) -> String {
        format!("{}#key-{}", self.to_string(), n)
    }

    /// Service fragment: "did:chain:xxx#service-1"
    pub fn service_ref(&self, n: u32) -> String {
        format!("{}#service-{}", self.to_string(), n)
    }
}

// ─── Verification Method ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct VerificationMethod {
    pub id:             String,  // did:chain:xxx#key-1
    pub type_:          String,  // EcdsaSecp256k1VerificationKey2019
    pub controller:     String,  // the DID string
    pub public_key_hex: String,  // compressed 33-byte pubkey
}

impl VerificationMethod {
    pub fn secp256k1(did: &Did, key_index: u32, pk: &PublicKey) -> Self {
        VerificationMethod {
            id:             did.key_ref(key_index),
            type_:          "EcdsaSecp256k1VerificationKey2019".to_string(),
            controller:     did.to_string(),
            public_key_hex: hex::encode(pk.serialize()),
        }
    }

    pub fn pubkey_bytes(&self) -> Vec<u8> {
        hex::decode(&self.public_key_hex).unwrap_or_default()
    }
}

// ─── Service Endpoint ─────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Service {
    pub id:               String,  // did:chain:xxx#service-1
    pub type_:            String,  // LinkedDomains | DIDCommMessaging | ...
    pub service_endpoint: String,
}

// ─── DID Document ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct DidDocument {
    pub id:                  String,
    pub controller:          Vec<String>,
    pub verification_method: Vec<VerificationMethod>,
    pub authentication:      Vec<String>,   // key IDs
    pub assertion_method:    Vec<String>,   // key IDs
    pub key_agreement:       Vec<String>,   // key IDs
    pub service:             Vec<Service>,
    pub deactivated:         bool,
    pub created:             u64,  // block height
    pub updated:             u64,
}

impl DidDocument {
    pub fn new(did: &Did, block: u64, vm: VerificationMethod) -> Self {
        let key_id = vm.id.clone();
        DidDocument {
            id:                  did.to_string(),
            controller:          vec![did.to_string()],
            verification_method: vec![vm],
            authentication:      vec![key_id.clone()],
            assertion_method:    vec![key_id.clone()],
            key_agreement:       vec![key_id],
            service:             Vec::new(),
            deactivated:         false,
            created:             block,
            updated:             block,
        }
    }

    pub fn first_pubkey_bytes(&self) -> Option<Vec<u8>> {
        self.verification_method.first().map(|vm| vm.pubkey_bytes())
    }

    pub fn print(&self) {
        println!("  DID Document:");
        println!("    id:         {}", self.id);
        println!("    controller: {:?}", self.controller);
        for vm in &self.verification_method {
            println!("    key:        {} [{}]  pk={}...", vm.id, vm.type_, &vm.public_key_hex[..8]);
        }
        for svc in &self.service {
            println!("    service:    {} [{}]  → {}", svc.id, svc.type_, svc.service_endpoint);
        }
        println!("    auth keys:  {:?}", self.authentication);
        println!("    deactivated: {}", self.deactivated);
        println!("    created@{} updated@{}", self.created, self.updated);
    }
}

// ─── DID Registry ─────────────────────────────────────────────────────────────

/// On-chain registry: stores DID Documents, enforces controller-only updates
pub struct DidRegistry {
    pub block:  u64,
    documents:  HashMap<String, DidDocument>,
    pub events: Vec<String>,
}

impl DidRegistry {
    pub fn new() -> Self {
        DidRegistry { block: 1, documents: HashMap::new(), events: Vec::new() }
    }

    pub fn advance(&mut self, n: u64) {
        self.block += n;
    }

    fn log(&mut self, msg: &str) {
        self.events.push(format!("[block {}] {}", self.block, msg));
    }

    /// Register a new DID from a secp256k1 public key
    pub fn create(&mut self, sk: &SecretKey) -> (Did, DidDocument) {
        let secp = Secp256k1::new();
        let pk = PublicKey::from_secret_key(&secp, sk);
        let pk_bytes = pk.serialize();
        let did = Did::from_pubkey(&pk_bytes);
        let vm = VerificationMethod::secp256k1(&did, 1, &pk);
        let doc = DidDocument::new(&did, self.block, vm);
        self.documents.insert(did.to_string(), doc.clone());
        self.log(&format!("DIDCreate: {}", did.to_string()));
        (did, doc)
    }

    /// Resolve a DID → DID Document (returns None if not found or deactivated)
    pub fn resolve(&self, did_str: &str) -> Option<&DidDocument> {
        let doc = self.documents.get(did_str)?;
        if doc.deactivated { None } else { Some(doc) }
    }

    /// Resolve including deactivated (for audit)
    pub fn resolve_any(&self, did_str: &str) -> Option<&DidDocument> {
        self.documents.get(did_str)
    }

    /// Add a service endpoint (controller only — verified by signature)
    pub fn add_service(&mut self, did_str: &str, sk: &SecretKey, svc: Service) -> Result<(), String> {
        self.verify_controller(did_str, sk)?;
        let doc = self.documents.get_mut(did_str).unwrap();
        if doc.deactivated {
            return Err("DID is deactivated".to_string());
        }
        doc.service.push(svc.clone());
        doc.updated = self.block;
        self.log(&format!("DIDUpdate(service): {} +{}", did_str, svc.type_));
        Ok(())
    }

    /// Add a second verification key (rotation)
    pub fn add_key(&mut self, did_str: &str, sk: &SecretKey, new_sk: &SecretKey) -> Result<(), String> {
        self.verify_controller(did_str, sk)?;
        let secp = Secp256k1::new();
        let new_pk = PublicKey::from_secret_key(&secp, new_sk);
        let doc = self.documents.get(did_str).unwrap();
        let did = Did::parse(did_str).unwrap();
        let key_n = doc.verification_method.len() as u32 + 1;
        let vm = VerificationMethod::secp256k1(&did, key_n, &new_pk);
        let key_id = vm.id.clone();
        let doc = self.documents.get_mut(did_str).unwrap();
        doc.verification_method.push(vm);
        doc.authentication.push(key_id.clone());
        doc.assertion_method.push(key_id);
        doc.updated = self.block;
        self.log(&format!("DIDUpdate(addKey): {} added key-{}", did_str, key_n));
        Ok(())
    }

    /// Deactivate a DID (irreversible — DID Document becomes invalid)
    pub fn deactivate(&mut self, did_str: &str, sk: &SecretKey) -> Result<(), String> {
        self.verify_controller(did_str, sk)?;
        let doc = self.documents.get_mut(did_str).unwrap();
        doc.deactivated = true;
        doc.updated = self.block;
        self.log(&format!("DIDDeactivate: {}", did_str));
        Ok(())
    }

    /// Verify that sk is the controller of the DID (matches first key)
    fn verify_controller(&self, did_str: &str, sk: &SecretKey) -> Result<(), String> {
        let doc = self.documents.get(did_str).ok_or("DID not found")?;
        let secp = Secp256k1::new();
        let pk = PublicKey::from_secret_key(&secp, sk);
        let expected_hex = hex::encode(pk.serialize());
        let matches = doc.verification_method.iter().any(|vm| vm.public_key_hex == expected_hex);
        if matches { Ok(()) } else { Err("Not a controller of this DID".to_string()) }
    }

    pub fn print_events_since(&self, from: usize) {
        for e in &self.events[from..] {
            println!("  {}", e);
        }
    }
}

// ─── Verifiable Credentials ───────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct CredentialProof {
    pub type_:               String,  // EcdsaSecp256k1Signature2019
    pub created:             u64,
    pub verification_method: String,  // issuer_did#key-1
    pub proof_value:         Vec<u8>, // compact ECDSA signature
}

#[derive(Clone, Debug)]
pub struct VerifiableCredential {
    pub context:         Vec<String>,
    pub id:              String,
    pub type_:           Vec<String>,
    pub issuer:          String,   // issuer DID
    pub issuance_date:   u64,      // block height
    pub expiry_date:     Option<u64>,
    pub subject_id:      String,   // subject DID
    pub claims:          HashMap<String, String>,
    pub proof:           Option<CredentialProof>,
}

impl VerifiableCredential {
    /// Canonical bytes to sign: H(issuer ‖ subject ‖ claims_sorted ‖ issuance_date)
    pub fn signing_bytes(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"vc_signing_v1");
        h.update(self.issuer.as_bytes());
        h.update(self.subject_id.as_bytes());
        let mut keys: Vec<_> = self.claims.keys().collect();
        keys.sort();
        for k in keys {
            h.update(k.as_bytes());
            h.update(b"=");
            h.update(self.claims[k].as_bytes());
            h.update(b";");
        }
        h.update(&self.issuance_date.to_le_bytes());
        if let Some(exp) = self.expiry_date {
            h.update(&exp.to_le_bytes());
        }
        for t in &self.type_ {
            h.update(t.as_bytes());
        }
        *h.finalize().as_bytes()
    }
}

/// Issue a Verifiable Credential
pub fn issue_credential(
    registry: &DidRegistry,
    issuer_did: &str,
    issuer_sk:  &SecretKey,
    subject_did: &str,
    vc_type:    &str,
    claims:     HashMap<String, String>,
    expiry:     Option<u64>,
) -> Result<VerifiableCredential, String> {
    // Resolve issuer DID
    registry.resolve(issuer_did).ok_or("Issuer DID not found")?;

    let block = registry.block;
    let vc_id = {
        let mut h = blake3::Hasher::new();
        h.update(b"vc_id");
        h.update(issuer_did.as_bytes());
        h.update(subject_did.as_bytes());
        h.update(&block.to_le_bytes());
        format!("urn:uuid:{}", hex::encode(&h.finalize().as_bytes()[..8]))
    };

    let mut vc = VerifiableCredential {
        context:       vec![
            "https://www.w3.org/2018/credentials/v1".to_string(),
            "https://www.w3.org/2018/credentials/examples/v1".to_string(),
        ],
        id:            vc_id,
        type_:         vec!["VerifiableCredential".to_string(), vc_type.to_string()],
        issuer:        issuer_did.to_string(),
        issuance_date: block,
        expiry_date:   expiry,
        subject_id:    subject_did.to_string(),
        claims,
        proof:         None,
    };

    // Sign
    let msg_hash = vc.signing_bytes();
    let secp = Secp256k1::new();
    let msg  = Message::from_slice(&msg_hash).map_err(|e| e.to_string())?;
    let sig  = secp.sign_ecdsa(&msg, issuer_sk).serialize_compact().to_vec();

    let vm_id = format!("{}#key-1", issuer_did);
    vc.proof = Some(CredentialProof {
        type_:               "EcdsaSecp256k1Signature2019".to_string(),
        created:             block,
        verification_method: vm_id,
        proof_value:         sig,
    });

    Ok(vc)
}

/// Verify a Verifiable Credential against the DID registry
pub fn verify_credential(registry: &DidRegistry, vc: &VerifiableCredential) -> bool {
    let proof = match &vc.proof {
        Some(p) => p,
        None    => return false,
    };

    // Resolve issuer
    let doc = match registry.resolve(&vc.issuer) {
        Some(d) => d,
        None    => return false,
    };

    // Check expiry
    if let Some(exp) = vc.expiry_date {
        if registry.block > exp {
            return false;  // expired
        }
    }

    // Find the verification method referenced in proof
    let vm = doc.verification_method.iter()
        .find(|vm| vm.id == proof.verification_method);
    let vm = match vm {
        Some(v) => v,
        None    => return false,
    };

    // Verify signature
    let pk_bytes = match hex::decode(&vm.public_key_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let secp = Secp256k1::new();
    let pk = match PublicKey::from_slice(&pk_bytes) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let msg_hash = vc.signing_bytes();
    let msg = match Message::from_slice(&msg_hash) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let sig_bytes: [u8; 64] = match proof.proof_value.as_slice().try_into() {
        Ok(b) => b,
        Err(_) => return false,
    };
    let sig = match secp256k1::ecdsa::Signature::from_compact(&sig_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };
    secp.verify_ecdsa(&msg, &sig, &pk).is_ok()
}

// ─── DID Authentication ───────────────────────────────────────────────────────

/// Challenge issued by a verifier to prove DID control
#[derive(Clone, Debug)]
pub struct AuthChallenge {
    pub nonce:      [u8; 32],
    pub domain:     String,
    pub issued_at:  u64,
    pub expires_at: u64,
}

impl AuthChallenge {
    pub fn new(block: u64, domain: &str) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"auth_challenge");
        h.update(&block.to_le_bytes());
        h.update(domain.as_bytes());
        let nonce = *h.finalize().as_bytes();
        AuthChallenge {
            nonce,
            domain:     domain.to_string(),
            issued_at:  block,
            expires_at: block + 10,
        }
    }

    /// Bytes that must be signed: H(nonce ‖ domain ‖ issued_at)
    pub fn signing_bytes(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"did_auth_v1");
        h.update(&self.nonce);
        h.update(self.domain.as_bytes());
        h.update(&self.issued_at.to_le_bytes());
        *h.finalize().as_bytes()
    }
}

#[derive(Clone, Debug)]
pub struct AuthProof {
    pub did:                 String,
    pub verification_method: String,
    pub signature:           Vec<u8>,
}

/// Subject signs the challenge to prove control of their DID
pub fn sign_challenge(did: &Did, sk: &SecretKey, challenge: &AuthChallenge) -> AuthProof {
    let secp = Secp256k1::new();
    let msg_hash = challenge.signing_bytes();
    let msg = Message::from_slice(&msg_hash).expect("32 bytes");
    let sig = secp.sign_ecdsa(&msg, sk).serialize_compact().to_vec();
    AuthProof {
        did:                 did.to_string(),
        verification_method: did.key_ref(1),
        signature:           sig,
    }
}

/// Verifier checks the proof against the DID registry
pub fn verify_auth(
    registry:  &DidRegistry,
    challenge: &AuthChallenge,
    proof:     &AuthProof,
) -> bool {
    // Check expiry
    if registry.block > challenge.expires_at {
        return false;
    }

    // Resolve DID
    let doc = match registry.resolve(&proof.did) {
        Some(d) => d,
        None    => return false,
    };

    // Find verification method
    let vm = doc.verification_method.iter()
        .find(|vm| vm.id == proof.verification_method);
    let vm = match vm {
        Some(v) => v,
        None    => return false,
    };

    // Verify signature
    let pk_bytes = match hex::decode(&vm.public_key_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let secp = Secp256k1::new();
    let pk = match PublicKey::from_slice(&pk_bytes) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let msg_hash = challenge.signing_bytes();
    let msg = match Message::from_slice(&msg_hash) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let sig_bytes: [u8; 64] = match proof.signature.as_slice().try_into() {
        Ok(b) => b,
        Err(_) => return false,
    };
    let sig = match secp256k1::ecdsa::Signature::from_compact(&sig_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };
    secp.verify_ecdsa(&msg, &sig, &pk).is_ok()
}

// ─── Key derivation helper ────────────────────────────────────────────────────

/// Derive a deterministic secret key from a seed label
pub fn derive_sk(label: &[u8], seed: &[u8]) -> SecretKey {
    let out = blake3::hash(&[label, seed].concat());
    SecretKey::from_slice(out.as_bytes()).expect("valid 32-byte key")
}
