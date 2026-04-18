#![allow(dead_code)]
//! SSRF guard — validate callback URLs trước khi server gửi outbound request.
//!
//! Từ chối các URL trỏ đến:
//!   - loopback: 127.0.0.0/8, ::1, localhost
//!   - private: 10/8, 172.16/12, 192.168/16
//!   - link-local: 169.254/16 (AWS metadata, GCP metadata)
//!   - unspecified: 0.0.0.0, ::
//!   - IPv4-mapped IPv6 private (::ffff:10.x.x.x, v.v.)

/// Kiểm tra URL hợp lệ để dùng làm callback endpoint.
/// Trả về Err với message nếu URL bị từ chối.
pub fn validate_callback_url(url: &str) -> Result<(), &'static str> {
    let after_scheme = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        rest
    } else {
        return Err("callback_url must use http or https scheme");
    };

    // Extract host — trước '/' '?' '#'
    let host_part = after_scheme
        .split(|c| c == '/' || c == '?' || c == '#')
        .next()
        .unwrap_or(after_scheme);

    // Tách port: IPv6 literal "[::1]:80" vs "host:80"
    let host = if host_part.starts_with('[') {
        // IPv6: lấy phần trong [...]
        host_part
            .trim_start_matches('[')
            .split(']')
            .next()
            .unwrap_or("")
    } else {
        // IPv4 / hostname: bỏ ":port"
        host_part.split(':').next().unwrap_or(host_part)
    };

    if host.is_empty() {
        return Err("callback_url has empty host");
    }

    // Block localhost variants
    let host_lower = host.to_lowercase();
    if host_lower == "localhost"
        || host_lower.ends_with(".localhost")
        || host_lower == "ip6-localhost"
        || host_lower == "ip6-loopback"
    {
        return Err("callback_url must not point to localhost");
    }

    // IPv4 literal
    if let Ok(v4) = host.parse::<std::net::Ipv4Addr>() {
        if is_blocked_v4(v4) {
            return Err("callback_url must not point to a private/internal address");
        }
        return Ok(());
    }

    // IPv6 literal
    if let Ok(v6) = host.parse::<std::net::Ipv6Addr>() {
        if v6.is_loopback() || v6.is_unspecified() {
            return Err("callback_url must not point to a private/internal address");
        }
        // IPv4-mapped: ::ffff:10.x.x.x
        if let Some(v4) = to_ipv4_mapped(v6) {
            if is_blocked_v4(v4) {
                return Err("callback_url must not point to a private/internal address");
            }
        }
    }

    Ok(())
}

fn is_blocked_v4(addr: std::net::Ipv4Addr) -> bool {
    addr.is_loopback()        // 127.0.0.0/8
        || addr.is_private()  // 10/8, 172.16/12, 192.168/16
        || addr.is_link_local() // 169.254/16 — AWS/GCP metadata
        || addr.is_unspecified() // 0.0.0.0
        || addr.is_broadcast()   // 255.255.255.255
}

// Ipv6Addr::to_ipv4_mapped() ổn định từ Rust 1.63 — dùng trực tiếp
fn to_ipv4_mapped(v6: std::net::Ipv6Addr) -> Option<std::net::Ipv4Addr> {
    v6.to_ipv4_mapped()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok_public_https() {
        assert!(validate_callback_url("https://example.com/hook").is_ok());
    }

    #[test]
    fn test_ok_public_http() {
        assert!(validate_callback_url("http://example.com/hook").is_ok());
    }

    #[test]
    fn test_reject_no_scheme() {
        assert!(validate_callback_url("example.com/hook").is_err());
    }

    #[test]
    fn test_reject_localhost() {
        assert!(validate_callback_url("http://localhost/hook").is_err());
        assert!(validate_callback_url("https://localhost:8080/x").is_err());
    }

    #[test]
    fn test_reject_loopback_ipv4() {
        assert!(validate_callback_url("http://127.0.0.1/hook").is_err());
        assert!(validate_callback_url("http://127.1.2.3/hook").is_err());
    }

    #[test]
    fn test_reject_private_ipv4() {
        assert!(validate_callback_url("http://10.0.0.1/hook").is_err());
        assert!(validate_callback_url("http://192.168.1.1/hook").is_err());
        assert!(validate_callback_url("http://172.16.0.1/hook").is_err());
    }

    #[test]
    fn test_reject_link_local_ipv4() {
        assert!(validate_callback_url("http://169.254.169.254/latest/meta-data/").is_err());
    }

    #[test]
    fn test_reject_loopback_ipv6() {
        assert!(validate_callback_url("http://[::1]/hook").is_err());
    }

    #[test]
    fn test_reject_ipv4_mapped_private() {
        assert!(validate_callback_url("http://[::ffff:10.0.0.1]/hook").is_err());
    }

    #[test]
    fn test_reject_unspecified() {
        assert!(validate_callback_url("http://0.0.0.0/hook").is_err());
    }
}
