//! Conversion helpers — paklets ↔ PKT, hash formatting, address parsing.

use crate::PAKLETS_PER_PKT;

// ── Paklets ↔ PKT ─────────────────────────────────────────────────────────────

/// Chuyển paklets (u64) → PKT (f64).
///
/// ```
/// use pkt_sdk::paklets_to_pkt;
/// assert_eq!(paklets_to_pkt(1_073_741_824), 1.0);
/// assert_eq!(paklets_to_pkt(0), 0.0);
/// ```
pub fn paklets_to_pkt(paklets: u64) -> f64 {
    paklets as f64 / PAKLETS_PER_PKT as f64
}

/// Chuyển PKT (f64) → paklets (u64), làm tròn xuống.
///
/// ```
/// use pkt_sdk::pkt_to_paklets;
/// assert_eq!(pkt_to_paklets(1.0), 1_073_741_824);
/// ```
pub fn pkt_to_paklets(pkt: f64) -> u64 {
    (pkt * PAKLETS_PER_PKT as f64) as u64
}

// ── Hash formatting ────────────────────────────────────────────────────────────

/// Rút gọn hash hex thành `"abcd1234…ef567890"`.
///
/// ```
/// use pkt_sdk::short_hash;
/// let h = "abcd1234567890ef1234567890abcdef1234567890abcdef1234567890abcdef";
/// assert_eq!(short_hash(h), "abcd123456…90abcdef");
/// ```
pub fn short_hash(hash: &str) -> String {
    if hash.len() > 18 {
        format!("{}…{}", &hash[..10], &hash[hash.len() - 8..])
    } else {
        hash.to_string()
    }
}

/// Rút gọn address thành `"abcd1234…ef5678"`.
pub fn short_addr(addr: &str) -> String {
    if addr.len() > 14 {
        format!("{}…{}", &addr[..8], &addr[addr.len() - 6..])
    } else {
        addr.to_string()
    }
}

// ── Timestamp ─────────────────────────────────────────────────────────────────

/// Số giây kể từ timestamp unix đến bây giờ.
/// Trả về None nếu timestamp = 0 hoặc trong tương lai.
pub fn secs_ago(timestamp: u64) -> Option<u64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    if timestamp == 0 || timestamp > now { return None; }
    Some(now - timestamp)
}

/// Format số giây thành chuỗi dễ đọc: "5s ago", "3m ago", "2h ago".
pub fn ago(secs: u64) -> String {
    if secs < 60   { return format!("{}s ago", secs); }
    if secs < 3600 { return format!("{}m ago", secs / 60); }
    format!("{}h ago", secs / 3600)
}

// ── Hashrate formatting ────────────────────────────────────────────────────────

/// Format hashrate thành chuỗi dễ đọc: "1.23 TH/s", "456 MH/s".
pub fn fmt_hashrate(h: f64) -> String {
    if h >= 1e15 { return format!("{:.2} PH/s", h / 1e15); }
    if h >= 1e12 { return format!("{:.2} TH/s", h / 1e12); }
    if h >= 1e9  { return format!("{:.2} GH/s", h / 1e9);  }
    if h >= 1e6  { return format!("{:.2} MH/s", h / 1e6);  }
    if h >= 1e3  { return format!("{:.2} KH/s", h / 1e3);  }
    format!("{:.0} H/s", h)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paklets_to_pkt_one() {
        assert_eq!(paklets_to_pkt(1_073_741_824), 1.0);
    }

    #[test]
    fn test_paklets_to_pkt_zero() {
        assert_eq!(paklets_to_pkt(0), 0.0);
    }

    #[test]
    fn test_pkt_to_paklets_roundtrip() {
        assert_eq!(pkt_to_paklets(1.0), 1_073_741_824);
        assert_eq!(pkt_to_paklets(2.0), 2_147_483_648);
    }

    #[test]
    fn test_short_hash_long() {
        let h = "abcd1234567890ef1234567890abcdef1234567890abcdef1234567890abcdef";
        let s = short_hash(h);
        assert!(s.contains('…'));
        assert_eq!(s.chars().count(), 19); // 10 + '…' + 8
    }

    #[test]
    fn test_short_hash_short() {
        assert_eq!(short_hash("abc"), "abc");
    }

    #[test]
    fn test_short_addr() {
        let a = "pkt1q4f3abc123def456abc";
        let s = short_addr(a);
        assert!(s.contains('…'));
    }

    #[test]
    fn test_ago_seconds() {
        assert_eq!(ago(5),    "5s ago");
        assert_eq!(ago(59),   "59s ago");
    }

    #[test]
    fn test_ago_minutes() {
        assert_eq!(ago(60),   "1m ago");
        assert_eq!(ago(3599), "59m ago");
    }

    #[test]
    fn test_ago_hours() {
        assert_eq!(ago(3600), "1h ago");
        assert_eq!(ago(7200), "2h ago");
    }

    #[test]
    fn test_fmt_hashrate_th() {
        let s = fmt_hashrate(1.5e12);
        assert!(s.contains("TH/s"));
    }

    #[test]
    fn test_fmt_hashrate_mh() {
        let s = fmt_hashrate(500_000.0);
        assert!(s.contains("KH/s"));
    }
}
