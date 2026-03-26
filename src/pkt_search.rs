#![allow(dead_code)]
//! v18.2 — Search Pro
//!
//! Detect loại query và trả kết quả tìm kiếm thống nhất.
//!
//! ## Query types (ưu tiên từ trên xuống)
//!   Height  — toàn chữ số, e.g. "12345"
//!   Txid    — 8–64 ký tự hex, e.g. "deadbeef..."
//!   Address — bắt đầu bằng 'p', 25–40 ký tự Base58, e.g. "pXXX..."
//!   Label   — text tự do → fuzzy match tên label
//!   Unknown — không khớp loại nào
//!
//! ## API
//!   GET /api/testnet/search?q=<query>
//!   Response: {"query": "...", "results": [{type, label, value, meta}]}

use crate::pkt_labels::LabelDb;

// ── Query kind ────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum QueryKind {
    Height(u64),
    Txid(String),    // lowercase hex, 8–64 chars
    Address(String), // Base58Check PKT address
    Label(String),   // free-text fuzzy label search
    Unknown,
}

/// Phân tích query string, trả về loại phù hợp nhất.
pub fn detect_kind(q: &str) -> QueryKind {
    let q = q.trim();
    if q.is_empty() {
        return QueryKind::Unknown;
    }

    // Toàn chữ số → block height
    if !q.is_empty() && q.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(n) = q.parse::<u64>() {
            return QueryKind::Height(n);
        }
    }

    // 8–64 ký tự hex → txid (partial hoặc full)
    if q.len() >= 8 && q.len() <= 64 && q.chars().all(|c| c.is_ascii_hexdigit()) {
        return QueryKind::Txid(q.to_lowercase());
    }

    // Bắt đầu bằng 'p', toàn Base58, 20–40 chars → địa chỉ PKT
    const B58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    if q.starts_with('p') && q.len() >= 20 && q.len() <= 40 && q.chars().all(|c| B58.contains(c)) {
        return QueryKind::Address(q.to_string());
    }

    // Còn lại → tìm theo label
    if q.len() >= 2 {
        return QueryKind::Label(q.to_lowercase());
    }

    QueryKind::Unknown
}

// ── Label search helper ───────────────────────────────────────────────────────

/// Tìm địa chỉ / script theo label text (substring, case-insensitive).
/// Trả về Vec<(address_or_script, label, category, verified)>
pub fn search_labels(text: &str, ldb: Option<&LabelDb>) -> Vec<(String, String, String, bool)> {
    let needle = text.to_lowercase();
    let mut out: Vec<(String, String, String, bool)> = Vec::new();

    // 1. Presets
    // We re-use the preset slice via a dedicated function exposed from pkt_labels
    for (addr, label, cat, verified) in super::pkt_labels::PRESETS {
        if label.to_lowercase().contains(&needle)
            || cat.to_lowercase().contains(&needle)
        {
            out.push((addr.to_string(), label.to_string(), cat.to_string(), *verified));
        }
    }

    // 2. LabelDb
    if let Some(db) = ldb {
        for (key, entry) in db.list_all() {
            if entry.label.to_lowercase().contains(&needle)
                || entry.category.to_lowercase().contains(&needle)
            {
                // Avoid duplicate if already in presets (by address prefix)
                if !out.iter().any(|(k, _, _, _)| key.starts_with(k.as_str())) {
                    out.push((key, entry.label, entry.category, entry.verified));
                }
            }
        }
    }

    out.truncate(10);
    out
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── detect_kind ───────────────────────────────────────────────────────────

    #[test]
    fn test_height_simple() {
        assert_eq!(detect_kind("12345"), QueryKind::Height(12345));
    }

    #[test]
    fn test_height_zero() {
        assert_eq!(detect_kind("0"), QueryKind::Height(0));
    }

    #[test]
    fn test_height_large() {
        assert_eq!(detect_kind("999999"), QueryKind::Height(999_999));
    }

    #[test]
    fn test_txid_full_64() {
        let txid = "a".repeat(64);
        assert_eq!(detect_kind(&txid), QueryKind::Txid("a".repeat(64)));
    }

    #[test]
    fn test_txid_partial_8() {
        assert_eq!(detect_kind("deadbeef"), QueryKind::Txid("deadbeef".to_string()));
    }

    #[test]
    fn test_txid_uppercase_normalised() {
        assert_eq!(detect_kind("DEADBEEF"), QueryKind::Txid("deadbeef".to_string()));
    }

    #[test]
    fn test_txid_mixed_case() {
        assert_eq!(detect_kind("DeAdBeEf"), QueryKind::Txid("deadbeef".to_string()));
    }

    #[test]
    fn test_txid_too_short_not_txid() {
        // 7 hex chars → Label (too short for txid threshold)
        let r = detect_kind("abcdef1");
        assert_ne!(r, QueryKind::Txid("abcdef1".to_string()));
    }

    #[test]
    fn test_address_basic() {
        // Valid Base58 (no I, O, 0, l chars)
        assert_eq!(
            detect_kind("pSEHPyBkABCDEFGHJKLMNP"),
            QueryKind::Address("pSEHPyBkABCDEFGHJKLMNP".to_string())
        );
    }

    #[test]
    fn test_address_min_length() {
        // 20-char valid Base58 address starting with 'p'
        // 'A' is valid Base58, 20 chars total
        let addr = "p".to_string() + &"A".repeat(19);
        assert_eq!(detect_kind(&addr), QueryKind::Address(addr));
    }

    #[test]
    fn test_address_too_short_is_label() {
        // "pABC" is 4 chars, too short for address → falls through to Label
        assert_eq!(detect_kind("pABC"), QueryKind::Label("pabc".to_string()));
    }

    #[test]
    fn test_label_plain_text() {
        assert_eq!(detect_kind("miner"), QueryKind::Label("miner".to_string()));
    }

    #[test]
    fn test_label_uppercase_normalised() {
        assert_eq!(detect_kind("PKT Steward"), QueryKind::Label("pkt steward".to_string()));
    }

    #[test]
    fn test_unknown_empty() {
        assert_eq!(detect_kind(""), QueryKind::Unknown);
    }

    #[test]
    fn test_unknown_single_char() {
        assert_eq!(detect_kind("x"), QueryKind::Unknown);
    }

    #[test]
    fn test_whitespace_trimmed_height() {
        assert_eq!(detect_kind("  42  "), QueryKind::Height(42));
    }

    #[test]
    fn test_height_beats_hex_when_decimal_only() {
        // "12345" is all digits → Height (not Txid even though digits are valid hex)
        assert_eq!(detect_kind("12345"), QueryKind::Height(12345));
    }

    #[test]
    fn test_txid_65_chars_is_label() {
        // 65-char hex → too long for txid → Label
        let q = "a".repeat(65);
        assert_eq!(detect_kind(&q), QueryKind::Label("a".repeat(65)));
    }

    // ── search_labels ─────────────────────────────────────────────────────────

    #[test]
    fn test_search_labels_preset_burn() {
        let results = search_labels("burn", None);
        assert!(!results.is_empty());
        assert!(results.iter().any(|(_, _, cat, _)| cat == "burn"));
    }

    #[test]
    fn test_search_labels_preset_miner() {
        let results = search_labels("miner", None);
        assert!(!results.is_empty());
        assert!(results.iter().any(|(_, _, cat, _)| cat == "miner"));
    }

    #[test]
    fn test_search_labels_no_match() {
        let results = search_labels("zzznomatch999", None);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_labels_case_insensitive() {
        let lower = search_labels("burn", None);
        let upper = search_labels("BURN", None);
        assert_eq!(lower.len(), upper.len());
    }

    #[test]
    fn test_search_labels_max_10() {
        // Even if many match, cap at 10
        let results = search_labels("p", None); // 'p' matches all preset addresses
        assert!(results.len() <= 10);
    }

    #[test]
    fn test_search_labels_with_db() {
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());
        let _g = LOCK.lock().unwrap();
        let db = LabelDb::open_temp().unwrap();
        db.set_label("custom_addr", "Custom Mining Pool", "miner", false).unwrap();
        let results = search_labels("custom", Some(&db));
        assert!(results.iter().any(|(k, _, _, _)| k == "custom_addr"));
    }
}
