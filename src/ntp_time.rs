/// SNTP client với cache — query NTP mỗi 60 giây, dùng Instant monotonic
/// để nội suy chính xác giữa các lần query. Không cần crate thêm.
///
/// NTP epoch = 1900-01-01 · Unix epoch = 1970-01-01 · Offset = 2_208_988_800s

use std::net::UdpSocket;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const NTP_EPOCH_OFFSET: i64  = 2_208_988_800;
const TIMEOUT_MS:       u64  = 3_000;   // 3s timeout mỗi server
const REFRESH_SECS:     u64  = 60;      // query NTP mỗi 60 giây

static NTP_SERVERS: &[&str] = &[
    "pool.ntp.org:123",
    "time.google.com:123",
    "time.cloudflare.com:123",
    "time.aws.com:123",
    "time.facebook.com:123",
    "time.windows.com:123",
    "vn.pool.ntp.org:123",
    "time.apple.com:123",
];

struct NtpCache {
    ntp_ts:     i64,    // Unix timestamp tại thời điểm query NTP
    queried_at: Instant, // monotonic clock tại thời điểm đó
}

static CACHE: Mutex<Option<NtpCache>> = Mutex::new(None);

/// Trả về Unix timestamp (seconds) hiện tại.
/// - Lần đầu hoặc cache > 60s: query NTP, cập nhật cache.
/// - Giữa các lần query: nội suy bằng Instant (monotonic, không bị drift).
/// - Nếu tất cả NTP fail: fallback OS clock.
pub fn now_ts() -> i64 {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());

    let needs_refresh = match &*cache {
        None      => true,
        Some(c)   => c.queried_at.elapsed().as_secs() >= REFRESH_SECS,
    };

    if needs_refresh {
        if let Some(ts) = fetch_ntp() {
            *cache = Some(NtpCache { ntp_ts: ts, queried_at: Instant::now() });
        } else if cache.is_none() {
            // Lần đầu + NTP fail → OS clock, vẫn cache để tránh retry liên tục
            let os_ts = os_now();
            eprintln!("[ntp] ⚠ tất cả NTP servers fail → OS clock ({})", os_ts);
            *cache = Some(NtpCache { ntp_ts: os_ts, queried_at: Instant::now() });
        }
        // Nếu đã có cache cũ nhưng refresh fail → giữ cache cũ, nội suy tiếp
    }

    match &*cache {
        Some(c)  => c.ntp_ts + c.queried_at.elapsed().as_secs() as i64,
        None     => os_now(),
    }
}

/// Thử từng NTP server, trả về ts đầu tiên hợp lệ.
fn fetch_ntp() -> Option<i64> {
    for server in NTP_SERVERS {
        match query_sntp(server) {
            Ok(ts) => {
                println!("[ntp] ✅ {} → {}", server, ts);
                return Some(ts);
            }
            Err(e) => eprintln!("[ntp] ✗ {}: {}", server, e),
        }
    }
    None
}

fn query_sntp(server: &str) -> Result<i64, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("bind: {}", e))?;
    socket.set_read_timeout(Some(Duration::from_millis(TIMEOUT_MS)))
        .map_err(|e| format!("timeout set: {}", e))?;

    // SNTP request: 48 bytes, LI=0 VN=4 Mode=3 (client) = 0x23
    let mut req = [0u8; 48];
    req[0] = 0b00_100_011;
    socket.send_to(&req, server)
        .map_err(|e| format!("send: {}", e))?;

    let mut resp = [0u8; 48];
    socket.recv_from(&mut resp)
        .map_err(|e| format!("recv: {}", e))?;

    // Transmit Timestamp: bytes 40–43 = NTP seconds (big-endian)
    let ntp_secs = u32::from_be_bytes(
        resp[40..44].try_into().map_err(|_| "slice err".to_string())?
    ) as i64;

    if ntp_secs == 0 {
        return Err("ntp_secs=0".to_string());
    }
    Ok(ntp_secs - NTP_EPOCH_OFFSET)
}

fn os_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntp_timestamp_reasonable() {
        let ts = now_ts();
        // 2024-01-01 = 1_704_067_200 · 2030-01-01 = 1_893_456_000
        assert!(ts > 1_704_067_200, "timestamp quá cũ: {}", ts);
        assert!(ts < 1_893_456_000, "timestamp quá xa tương lai: {}", ts);
    }

    #[test]
    fn interpolation_is_monotonic() {
        let t1 = now_ts();
        std::thread::sleep(Duration::from_millis(1100));
        let t2 = now_ts();
        assert!(t2 >= t1 + 1, "t2={} phải >= t1+1={}", t2, t1 + 1);
    }

    #[test]
    fn ntp_servers_reachable() {
        let ok = NTP_SERVERS.iter().any(|s| query_sntp(s).is_ok());
        assert!(ok, "Không server NTP nào phản hồi");
    }

    #[test]
    fn consecutive_calls_consistent() {
        // Hai lần gọi liên tiếp phải cho kết quả nhất quán (không nhảy)
        let t1 = now_ts();
        let t2 = now_ts();
        assert!((t2 - t1).abs() <= 1, "t1={} t2={} chênh lệch quá 1s", t1, t2);
    }
}
