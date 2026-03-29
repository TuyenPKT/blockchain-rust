//! pkt_script.rs — v23.1: P2PKH script verification for PKT full-node
//!
//! PKT uses legacy P2PKH (same opcode layout as Bitcoin) but with blake3
//! instead of SHA256d for the sighash preimage double-hash.
//!
//! Sighash algorithm (SIGHASH_ALL legacy):
//!   preimage = version || inputs* || outputs || locktime || sighash_type(4LE)
//!   (* signing input has scriptPubKey inline; other inputs have empty scriptSig)
//!   sighash  = blake3(blake3(preimage))
//!
//! scriptSig layout (P2PKH):
//!   <push_len><DER_sig + sighash_byte><push_len><compressed_pubkey>

#![allow(dead_code)]

use crate::pkt_utxo_sync::{WireTx, WireTxOut};

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptError {
    /// scriptSig too short to parse
    ScriptSigTooShort,
    /// scriptSig has unexpected format (not push-sig push-pubkey)
    UnexpectedScriptSigFormat,
    /// DER signature is malformed
    BadDerSignature,
    /// Public key is not a valid secp256k1 point
    BadPublicKey,
    /// Signature failed ECDSA verification
    SignatureInvalid,
    /// scriptPubKey is not a recognized P2PKH pattern
    UnsupportedScriptType,
    /// input_index out of range
    InputIndexOutOfRange,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScriptSigTooShort           => write!(f, "scriptSig too short"),
            Self::UnexpectedScriptSigFormat   => write!(f, "unexpected scriptSig format"),
            Self::BadDerSignature             => write!(f, "malformed DER signature"),
            Self::BadPublicKey                => write!(f, "invalid public key"),
            Self::SignatureInvalid            => write!(f, "ECDSA signature invalid"),
            Self::UnsupportedScriptType       => write!(f, "unsupported script type"),
            Self::InputIndexOutOfRange        => write!(f, "input index out of range"),
        }
    }
}

// ── P2PKH pattern detection ───────────────────────────────────────────────────

/// Returns `Some(hash160)` if `script` matches standard P2PKH:
/// `OP_DUP(76) OP_HASH160(a9) PUSH20(14) <20 bytes> OP_EQUALVERIFY(88) OP_CHECKSIG(ac)`
pub fn parse_p2pkh_script(script: &[u8]) -> Option<[u8; 20]> {
    if script.len() != 25 { return None; }
    if script[0] != 0x76 || script[1] != 0xa9 || script[2] != 0x14
        || script[23] != 0x88 || script[24] != 0xac
    {
        return None;
    }
    let mut hash = [0u8; 20];
    hash.copy_from_slice(&script[3..23]);
    Some(hash)
}

// ── scriptSig parsing ─────────────────────────────────────────────────────────

/// Parse a P2PKH scriptSig: `<push_sig><push_pubkey>`.
/// Returns `(der_sig_without_hashtype, pubkey_bytes)`.
pub fn parse_p2pkh_scriptsig(script_sig: &[u8]) -> Result<(Vec<u8>, Vec<u8>), ScriptError> {
    if script_sig.len() < 2 {
        return Err(ScriptError::ScriptSigTooShort);
    }
    let sig_push_len = script_sig[0] as usize;
    if script_sig.len() < 1 + sig_push_len + 1 {
        return Err(ScriptError::ScriptSigTooShort);
    }
    // sig bytes include the trailing sighash_type byte — strip it
    let sig_with_hashtype = &script_sig[1..1 + sig_push_len];
    if sig_with_hashtype.is_empty() {
        return Err(ScriptError::UnexpectedScriptSigFormat);
    }
    let der_sig = sig_with_hashtype[..sig_with_hashtype.len() - 1].to_vec();

    let pk_offset    = 1 + sig_push_len;
    let pk_push_len  = script_sig[pk_offset] as usize;
    if script_sig.len() < pk_offset + 1 + pk_push_len {
        return Err(ScriptError::ScriptSigTooShort);
    }
    let pubkey = script_sig[pk_offset + 1..pk_offset + 1 + pk_push_len].to_vec();
    Ok((der_sig, pubkey))
}

// ── Sighash preimage (legacy P2PKH) ──────────────────────────────────────────

fn write_varint(buf: &mut Vec<u8>, n: u64) {
    if n < 0xfd {
        buf.push(n as u8);
    } else if n <= 0xffff {
        buf.push(0xfd);
        buf.extend_from_slice(&(n as u16).to_le_bytes());
    } else if n <= 0xffff_ffff {
        buf.push(0xfe);
        buf.extend_from_slice(&(n as u32).to_le_bytes());
    } else {
        buf.push(0xff);
        buf.extend_from_slice(&n.to_le_bytes());
    }
}

/// Build the legacy sighash preimage for input `input_index` with SIGHASH_ALL.
///
/// `subscript` = the scriptPubKey of the UTXO being spent (raw bytes).
pub fn build_sighash_preimage(tx: &WireTx, input_index: usize, subscript: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();

    // version
    buf.extend_from_slice(&tx.version.to_le_bytes());

    // inputs
    write_varint(&mut buf, tx.inputs.len() as u64);
    for (i, inp) in tx.inputs.iter().enumerate() {
        buf.extend_from_slice(&inp.prev_txid);
        buf.extend_from_slice(&inp.prev_vout.to_le_bytes());
        if i == input_index {
            write_varint(&mut buf, subscript.len() as u64);
            buf.extend_from_slice(subscript);
        } else {
            write_varint(&mut buf, 0); // empty scriptSig
        }
        buf.extend_from_slice(&inp.sequence.to_le_bytes());
    }

    // outputs
    write_varint(&mut buf, tx.outputs.len() as u64);
    for out in &tx.outputs {
        buf.extend_from_slice(&out.value.to_le_bytes());
        write_varint(&mut buf, out.script_pubkey.len() as u64);
        buf.extend_from_slice(&out.script_pubkey);
    }

    // locktime
    buf.extend_from_slice(&tx.locktime.to_le_bytes());

    // sighash type (SIGHASH_ALL = 1, 4 bytes LE)
    buf.extend_from_slice(&1u32.to_le_bytes());

    buf
}

/// PKT double-hash: blake3(blake3(data)).
pub fn pkt_double_hash(data: &[u8]) -> [u8; 32] {
    let first = blake3::hash(data);
    *blake3::hash(first.as_bytes()).as_bytes()
}

// ── Core verification ─────────────────────────────────────────────────────────

/// Verify a single P2PKH input `input_index` of `tx`.
///
/// `utxo_script_pubkey` = the raw scriptPubKey bytes of the UTXO being spent.
///
/// Returns `Ok(())` on success, `Err(ScriptError)` on failure.
/// Non-P2PKH scripts return `Ok(())` (not verified, treated as valid — v23.1 scope).
pub fn verify_p2pkh_input(
    tx:               &WireTx,
    input_index:      usize,
    utxo_script_pubkey: &[u8],
) -> Result<(), ScriptError> {
    use secp256k1::{Secp256k1, PublicKey, Message, ecdsa::Signature};

    if input_index >= tx.inputs.len() {
        return Err(ScriptError::InputIndexOutOfRange);
    }

    // Only handle P2PKH — skip everything else (OP_RETURN, unknown, etc.)
    let hash160 = match parse_p2pkh_script(utxo_script_pubkey) {
        Some(h) => h,
        None    => return Ok(()), // unsupported script — skip
    };

    let script_sig = &tx.inputs[input_index].script_sig;
    let (der_sig, pubkey_bytes) = parse_p2pkh_scriptsig(script_sig)?;

    // 1. Verify pubkey hash matches scriptPubKey
    let b3_pk    = blake3::hash(&pubkey_bytes);
    let ripe_pk  = {
        use ripemd::Digest;
        let mut h = ripemd::Ripemd160::new();
        h.update(b3_pk.as_bytes());
        h.finalize()
    };
    if ripe_pk.as_slice() != hash160 {
        return Err(ScriptError::BadPublicKey);
    }

    // 2. Compute sighash
    let preimage = build_sighash_preimage(tx, input_index, utxo_script_pubkey);
    let sighash  = pkt_double_hash(&preimage);

    // 3. ECDSA verify
    let secp   = Secp256k1::verification_only();
    let pubkey = PublicKey::from_slice(&pubkey_bytes)
        .map_err(|_| ScriptError::BadPublicKey)?;
    let msg    = Message::from_slice(&sighash)
        .map_err(|_| ScriptError::BadDerSignature)?;
    let sig    = Signature::from_der(&der_sig)
        .map_err(|_| ScriptError::BadDerSignature)?;
    secp.verify_ecdsa(&msg, &sig, &pubkey)
        .map_err(|_| ScriptError::SignatureInvalid)
}

/// Verify all non-coinbase inputs of `tx`.
///
/// `utxo_lookup` returns the `WireTxOut` for a given `(txid, vout)` — caller
/// provides this from the UTXO DB.
///
/// Returns the index and error of the first failing input, or `Ok(())`.
pub fn verify_tx_scripts<F>(
    tx:           &WireTx,
    utxo_lookup:  F,
) -> Result<(), (usize, ScriptError)>
where
    F: Fn(&[u8; 32], u32) -> Option<WireTxOut>,
{
    if tx.is_coinbase() {
        return Ok(());
    }
    for (i, inp) in tx.inputs.iter().enumerate() {
        let utxo = match utxo_lookup(&inp.prev_txid, inp.prev_vout) {
            Some(u) => u,
            None    => continue, // UTXO not found — already caught by v23.0
        };
        verify_p2pkh_input(tx, i, &utxo.script_pubkey)
            .map_err(|e| (i, e))?;
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Secp256k1, SecretKey, Message};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_privkey(seed: u8) -> SecretKey {
        let mut bytes = [0u8; 32];
        bytes[31] = seed;
        SecretKey::from_slice(&bytes).unwrap()
    }

    fn pubkey_bytes(sk: &SecretKey) -> Vec<u8> {
        use secp256k1::PublicKey;
        let secp = Secp256k1::new();
        PublicKey::from_secret_key(&secp, sk).serialize().to_vec()
    }

    fn p2pkh_hash160(pk: &[u8]) -> [u8; 20] {
        use ripemd::Digest;
        let b3 = blake3::hash(pk);
        let mut h = ripemd::Ripemd160::new();
        h.update(b3.as_bytes());
        let r = h.finalize();
        let mut out = [0u8; 20];
        out.copy_from_slice(&r);
        out
    }

    fn p2pkh_script(hash160: &[u8; 20]) -> Vec<u8> {
        let mut s = vec![0x76u8, 0xa9, 0x14];
        s.extend_from_slice(hash160);
        s.push(0x88);
        s.push(0xac);
        s
    }

    fn sign_input(sk: &SecretKey, tx: &WireTx, input_index: usize, script_pubkey: &[u8]) -> Vec<u8> {
        let secp     = Secp256k1::new();
        let preimage = build_sighash_preimage(tx, input_index, script_pubkey);
        let sighash  = pkt_double_hash(&preimage);
        let msg      = Message::from_slice(&sighash).unwrap();
        let sig      = secp.sign_ecdsa(&msg, sk);
        let der      = sig.serialize_der();
        // DER + SIGHASH_ALL byte
        let mut sig_bytes = der.to_vec();
        sig_bytes.push(0x01);
        sig_bytes
    }

    fn build_scriptsig(sk: &SecretKey, tx: &WireTx, input_index: usize, script_pubkey: &[u8]) -> Vec<u8> {
        let pk_bytes = pubkey_bytes(sk);
        let sig      = sign_input(sk, tx, input_index, script_pubkey);
        let mut ss   = Vec::new();
        ss.push(sig.len() as u8);
        ss.extend_from_slice(&sig);
        ss.push(pk_bytes.len() as u8);
        ss.extend_from_slice(&pk_bytes);
        ss
    }

    fn simple_tx(prev_txid: [u8; 32], prev_vout: u32, script_sig: Vec<u8>) -> WireTx {
        use crate::pkt_utxo_sync::WireTxIn;
        WireTx {
            version:  1,
            inputs:   vec![WireTxIn { prev_txid, prev_vout, script_sig, sequence: 0xffff_ffff }],
            outputs:  vec![WireTxOut { value: 900, script_pubkey: vec![0x51] }],
            locktime: 0,
        }
    }

    // ── parse_p2pkh_script ────────────────────────────────────────────────────

    #[test]
    fn parse_p2pkh_valid() {
        let hash = [0xabu8; 20];
        let script = p2pkh_script(&hash);
        assert_eq!(parse_p2pkh_script(&script), Some(hash));
    }

    #[test]
    fn parse_p2pkh_wrong_length() {
        assert_eq!(parse_p2pkh_script(&[0u8; 24]), None);
        assert_eq!(parse_p2pkh_script(&[0u8; 26]), None);
    }

    #[test]
    fn parse_p2pkh_bad_opcodes() {
        let mut script = vec![0x76u8, 0xa9, 0x14];
        script.extend_from_slice(&[0u8; 20]);
        script.push(0x88);
        script.push(0xad); // wrong last byte
        assert_eq!(parse_p2pkh_script(&script), None);
    }

    // ── parse_p2pkh_scriptsig ─────────────────────────────────────────────────

    #[test]
    fn parse_scriptsig_roundtrip() {
        let sk   = make_privkey(42);
        let prev = [0x01u8; 32];
        let spk  = p2pkh_script(&p2pkh_hash160(&pubkey_bytes(&sk)));
        let tx   = simple_tx(prev, 0, vec![]);
        let ss   = build_scriptsig(&sk, &tx, 0, &spk);
        let (der, pk) = parse_p2pkh_scriptsig(&ss).unwrap();
        assert!(!der.is_empty());
        assert_eq!(pk, pubkey_bytes(&sk));
    }

    #[test]
    fn parse_scriptsig_too_short() {
        assert!(matches!(parse_p2pkh_scriptsig(&[]), Err(ScriptError::ScriptSigTooShort)));
        assert!(matches!(parse_p2pkh_scriptsig(&[3, 0, 0]), Err(ScriptError::ScriptSigTooShort)));
    }

    // ── build_sighash_preimage ────────────────────────────────────────────────

    #[test]
    fn sighash_preimage_deterministic() {
        let tx  = simple_tx([0u8; 32], 0, vec![]);
        let spk = vec![0x76u8, 0xa9, 0x14, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0x88, 0xac];
        let p1  = build_sighash_preimage(&tx, 0, &spk);
        let p2  = build_sighash_preimage(&tx, 0, &spk);
        assert_eq!(p1, p2);
    }

    #[test]
    fn sighash_preimage_different_inputs() {
        let tx  = simple_tx([0u8; 32], 0, vec![]);
        let spk = vec![0x51u8];
        // index 0 == signing input, index 999 doesn't exist but preimage should differ
        let p1  = build_sighash_preimage(&tx, 0, &spk);
        // Same tx, different dummy subscript
        let spk2 = vec![0x52u8];
        let p2   = build_sighash_preimage(&tx, 0, &spk2);
        assert_ne!(p1, p2);
    }

    // ── verify_p2pkh_input ────────────────────────────────────────────────────

    #[test]
    fn verify_valid_p2pkh_input() {
        let sk   = make_privkey(7);
        let hash = p2pkh_hash160(&pubkey_bytes(&sk));
        let spk  = p2pkh_script(&hash);
        let prev = [0x02u8; 32];

        // Build tx with empty scriptSig first to compute sighash
        let mut tx = simple_tx(prev, 0, vec![]);
        let ss = build_scriptsig(&sk, &tx, 0, &spk);
        tx.inputs[0].script_sig = ss;

        assert!(verify_p2pkh_input(&tx, 0, &spk).is_ok());
    }

    #[test]
    fn verify_wrong_key_fails() {
        let sk1  = make_privkey(1);
        let sk2  = make_privkey(2);
        let hash = p2pkh_hash160(&pubkey_bytes(&sk1)); // locked to sk1
        let spk  = p2pkh_script(&hash);
        let prev = [0x03u8; 32];

        // Sign with sk2 but lock to sk1's hash → pubkey hash mismatch
        let mut tx = simple_tx(prev, 0, vec![]);
        let ss = build_scriptsig(&sk2, &tx, 0, &spk);
        tx.inputs[0].script_sig = ss;

        assert!(matches!(
            verify_p2pkh_input(&tx, 0, &spk),
            Err(ScriptError::BadPublicKey)
        ));
    }

    #[test]
    fn verify_tampered_output_fails() {
        let sk   = make_privkey(5);
        let hash = p2pkh_hash160(&pubkey_bytes(&sk));
        let spk  = p2pkh_script(&hash);
        let prev = [0x04u8; 32];

        let mut tx = simple_tx(prev, 0, vec![]);
        let ss = build_scriptsig(&sk, &tx, 0, &spk);
        tx.inputs[0].script_sig = ss;

        // Tamper with output value after signing
        tx.outputs[0].value = 1_000_000;

        assert!(matches!(
            verify_p2pkh_input(&tx, 0, &spk),
            Err(ScriptError::SignatureInvalid)
        ));
    }

    #[test]
    fn verify_non_p2pkh_script_is_skipped() {
        let prev = [0x05u8; 32];
        let tx   = simple_tx(prev, 0, vec![]);
        // OP_1 script — not P2PKH, should pass (unsupported = skip)
        assert!(verify_p2pkh_input(&tx, 0, &[0x51u8]).is_ok());
    }

    #[test]
    fn verify_coinbase_is_skipped() {
        use crate::pkt_utxo_sync::WireTxIn;
        let tx = WireTx {
            version:  1,
            inputs:   vec![WireTxIn {
                prev_txid: [0u8; 32], prev_vout: 0xffff_ffff,
                script_sig: vec![0xab], sequence: 0xffff_ffff,
            }],
            outputs:  vec![WireTxOut { value: 4096, script_pubkey: vec![0x51] }],
            locktime: 0,
        };
        // coinbase — verify_tx_scripts should skip entirely
        let result = verify_tx_scripts(&tx, |_, _| None);
        assert!(result.is_ok());
    }

    // ── verify_tx_scripts ─────────────────────────────────────────────────────

    #[test]
    fn verify_tx_scripts_valid() {
        let sk   = make_privkey(9);
        let hash = p2pkh_hash160(&pubkey_bytes(&sk));
        let spk  = p2pkh_script(&hash);
        let prev = [0x06u8; 32];

        let mut tx = simple_tx(prev, 0, vec![]);
        let ss = build_scriptsig(&sk, &tx, 0, &spk);
        tx.inputs[0].script_sig = ss;

        let spk_clone = spk.clone();
        let result = verify_tx_scripts(&tx, move |txid, vout| {
            if *txid == prev && vout == 0 {
                Some(WireTxOut { value: 1000, script_pubkey: spk_clone.clone() })
            } else {
                None
            }
        });
        assert!(result.is_ok());
    }

    #[test]
    fn verify_tx_scripts_bad_sig_returns_index() {
        let sk   = make_privkey(10);
        let hash = p2pkh_hash160(&pubkey_bytes(&sk));
        let spk  = p2pkh_script(&hash);
        let prev = [0x07u8; 32];

        let mut tx = simple_tx(prev, 0, vec![]);
        let ss = build_scriptsig(&sk, &tx, 0, &spk);
        tx.inputs[0].script_sig = ss;

        // Tamper after signing
        tx.outputs[0].value = 999_999;

        let spk_clone = spk.clone();
        let result = verify_tx_scripts(&tx, move |txid, vout| {
            if *txid == prev && vout == 0 {
                Some(WireTxOut { value: 1000, script_pubkey: spk_clone.clone() })
            } else {
                None
            }
        });
        assert!(matches!(result, Err((0, ScriptError::SignatureInvalid))));
    }
}
