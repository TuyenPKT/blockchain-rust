/// SNTP client — query external time servers, fallback to OS clock.
/// Không dùng crate thêm: chỉ std::net::UdpSocket + 48-byte SNTP packet.
///
/// NTP epoch = 1900-01-01. Unix epoch = 1970-01-01.
/// Offset = 70 years = 2_208_988_800 seconds.

use std::net::UdpSocket;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const NTP_EPOCH_OFFSET: i64 = 2_208_988_800;
const TIMEOUT_MS: u64 = 3_000;

static NTP_SERVERS: &[&str] = &[
    "pool.ntp.org:123",
    "time.cloudflare.com:123",
    "time.google.com:123",
];

/// Trả về Unix timestamp (seconds) lấy từ NTP server đầu tiên phản hồi.
/// Nếu tất cả fail → fallback OS clock (chrono::Utc::now).
pub fn now_ts() -> i64 {
    for server in NTP_SERVERS {
        if let Ok(ts) = query_sntp(server) {
            return ts;
        }
    }
    // Fallback: OS clock
    eprintln!("[ntp] ⚠ tất cả NTP servers không phản hồi → dùng OS clock");
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn query_sntp(server: &str) -> Result<i64, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("bind: {}", e))?;
    socket.set_read_timeout(Some(Duration::from_millis(TIMEOUT_MS)))
        .map_err(|e| format!("timeout: {}", e))?;

    // SNTP request: 48 bytes, LI=0 VN=4 Mode=3 (client)
    let mut req = [0u8; 48];
    req[0] = 0b00_100_011; // 0x23

    socket.send_to(&req, server)
        .map_err(|e| format!("send {}: {}", server, e))?;

    let mut resp = [0u8; 48];
    socket.recv_from(&mut resp)
        .map_err(|e| format!("recv {}: {}", server, e))?;

    // Transmit Timestamp: bytes 40–43 = seconds (big-endian, NTP epoch)
    let ntp_secs = u32::from_be_bytes(
        resp[40..44].try_into().map_err(|_| "parse".to_string())?
    ) as i64;

    if ntp_secs == 0 {
        return Err(format!("{}: ntp_secs=0 (server không trả timestamp)", server));
    }

    let unix_ts = ntp_secs - NTP_EPOCH_OFFSET;
    Ok(unix_ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntp_timestamp_reasonable() {
        let ts = now_ts();
        // 2024-01-01 = 1704067200, 2030-01-01 = 1893456000
        assert!(ts > 1_704_067_200, "timestamp quá cũ: {}", ts);
        assert!(ts < 1_893_456_000, "timestamp quá xa tương lai: {}", ts);
    }

    #[test]
    fn ntp_servers_reachable() {
        // Ít nhất 1 server phải trả được ts hợp lệ
        let ok = NTP_SERVERS.iter().any(|s| query_sntp(s).is_ok());
        assert!(ok, "Không server NTP nào phản hồi");
    }
}
