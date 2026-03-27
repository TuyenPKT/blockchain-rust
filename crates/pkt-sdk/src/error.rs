//! Error types cho pkt-sdk.

use std::fmt;

/// Lỗi khi làm việc với PKT API hoặc parse data.
#[derive(Debug, Clone, PartialEq)]
pub enum PktError {
    /// Lỗi HTTP khi gọi API (status code, message).
    Api(String),
    /// Lỗi parse JSON hoặc hex.
    Parse(String),
    /// Lỗi kết nối mạng.
    Network(String),
    /// Không tìm thấy resource (404).
    NotFound(String),
}

impl fmt::Display for PktError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(m)      => write!(f, "API error: {}", m),
            Self::Parse(m)    => write!(f, "parse error: {}", m),
            Self::Network(m)  => write!(f, "network error: {}", m),
            Self::NotFound(m) => write!(f, "not found: {}", m),
        }
    }
}

impl std::error::Error for PktError {}

/// Result type cho pkt-sdk operations.
pub type PktResult<T> = Result<T, PktError>;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_api_error() {
        let e = PktError::Api("503".to_string());
        assert_eq!(e.to_string(), "API error: 503");
    }

    #[test]
    fn test_display_not_found() {
        let e = PktError::NotFound("block #999".to_string());
        assert!(e.to_string().contains("not found"));
    }

    #[test]
    fn test_pkt_result_ok() {
        let r: PktResult<u64> = Ok(42);
        assert_eq!(r.unwrap(), 42);
    }

    #[test]
    fn test_pkt_result_err() {
        let r: PktResult<u64> = Err(PktError::Parse("bad hex".to_string()));
        assert!(r.is_err());
    }
}
