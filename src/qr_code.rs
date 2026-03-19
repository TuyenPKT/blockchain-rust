#![allow(dead_code)]
//! v14.3 — QR Code
//!
//! Hiển thị QR code trong terminal bằng Unicode block characters.
//! Hai chế độ render:
//!   - Full block  (2 chars/module): `██` dark, `  ` light — rõ trên mọi terminal
//!   - Half block  (1 char/module):  `▀▄█ ` — compact, terminal phải hỗ trợ Unicode
//!
//! Hỗ trợ BIP21 payment URI cho PKT:
//!   `pkt:ADDRESS?amount=X&label=Y`
//!
//! Dùng crate `qrcode = "0.14"` — pure Rust, không cần libpng / libqrencode.

use qrcode::{EcLevel, QrCode};
use qrcode::render::unicode;

// ── Error correction level ───────────────────────────────────────────────────

/// Mức sửa lỗi QR — ảnh hưởng kích thước và khả năng đọc khi bị che
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrLevel {
    /// Low — 7% có thể khôi phục (QR nhỏ nhất)
    Low,
    /// Medium — 15% (mặc định, cân bằng tốt)
    Medium,
    /// High — 30% (tốt nhất cho in ấn / môi trường nhiễu)
    High,
}

impl QrLevel {
    fn to_ec(self) -> EcLevel {
        match self {
            QrLevel::Low    => EcLevel::L,
            QrLevel::Medium => EcLevel::M,
            QrLevel::High   => EcLevel::Q,
        }
    }
}

impl Default for QrLevel {
    fn default() -> Self { QrLevel::Medium }
}

// ── Render modes ─────────────────────────────────────────────────────────────

/// Render QR dạng half-block (compact) — 1 char = 1 module ngang, 2 modules dọc
/// Terminal cần hỗ trợ Unicode block elements (UTF-8).
pub fn render_compact(data: &str, level: QrLevel) -> Result<String, String> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), level.to_ec())
        .map_err(|e| e.to_string())?;
    let s = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Dark)
        .light_color(unicode::Dense1x2::Light)
        .quiet_zone(true)
        .build();
    Ok(s)
}

/// Render QR dạng full-block (2 chars/module) — tương thích tất cả terminal
/// Mỗi module = `██` (dark) hoặc `  ` (light) → QR to hơn nhưng luôn hiển thị đúng.
pub fn render_full(data: &str, level: QrLevel) -> Result<String, String> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), level.to_ec())
        .map_err(|e| e.to_string())?;

    let width   = code.width();
    let pixels  = code.to_colors();
    let mut out = String::new();

    // Quiet zone trên (2 dòng)
    let margin = "    ";
    for _ in 0..2 {
        out.push_str(margin);
        for _ in 0..width { out.push_str("  "); }
        out.push_str("  \n");
    }

    for row in 0..width {
        out.push_str(margin);
        out.push_str("  "); // quiet zone trái
        for col in 0..width {
            match pixels[row * width + col] {
                qrcode::Color::Dark  => out.push_str("██"),
                qrcode::Color::Light => out.push_str("  "),
            }
        }
        out.push_str("  \n"); // quiet zone phải
    }

    // Quiet zone dưới
    for _ in 0..2 {
        out.push_str(margin);
        for _ in 0..width { out.push_str("  "); }
        out.push_str("  \n");
    }

    Ok(out)
}

// ── Payment URI ───────────────────────────────────────────────────────────────

/// Tạo BIP21-style payment URI cho PKT
///
/// Format: `pkt:ADDRESS` hoặc `pkt:ADDRESS?amount=X&label=Y`
pub fn payment_uri(address: &str, amount_pkt: Option<f64>, label: Option<&str>) -> String {
    let mut uri = format!("pkt:{}", address);
    let mut params: Vec<String> = Vec::new();

    if let Some(amt) = amount_pkt {
        if amt > 0.0 {
            params.push(format!("amount={:.8}", amt));
        }
    }
    if let Some(lbl) = label {
        if !lbl.is_empty() {
            params.push(format!("label={}", percent_encode(lbl)));
        }
    }

    if !params.is_empty() {
        uri.push('?');
        uri.push_str(&params.join("&"));
    }
    uri
}

/// Percent-encode đơn giản cho label (chỉ encode space và ký tự đặc biệt thường gặp)
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' '  => out.push_str("%20"),
            '&'  => out.push_str("%26"),
            '='  => out.push_str("%3D"),
            '?'  => out.push_str("%3F"),
            '#'  => out.push_str("%23"),
            _    => out.push(c),
        }
    }
    out
}

// ── QrResult ─────────────────────────────────────────────────────────────────

/// Kết quả render QR — dùng để test và hiển thị
#[derive(Debug)]
pub struct QrResult {
    /// Nội dung được encode
    pub data:     String,
    /// QR string để in ra terminal
    pub qr_str:   String,
    /// Số modules (chiều rộng, không tính quiet zone)
    pub width:    usize,
    /// Payment URI (nếu là địa chỉ + amount)
    pub uri:      Option<String>,
}

impl QrResult {
    pub fn print(&self) {
        if let Some(ref uri) = self.uri {
            println!("\n  {}", uri);
        } else {
            println!("\n  {}", self.data);
        }
        println!("{}", self.qr_str);
        println!("  {} × {} modules", self.width, self.width);
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Tạo QR cho địa chỉ PKT (compact mode)
pub fn address_qr(address: &str, level: QrLevel) -> Result<QrResult, String> {
    let qr_str = render_compact(address, level)?;
    let width  = qr_width(address, level)?;
    Ok(QrResult {
        data: address.to_string(),
        qr_str,
        width,
        uri: None,
    })
}

/// Tạo QR cho payment URI (address + optional amount + label)
pub fn payment_qr(
    address:    &str,
    amount_pkt: Option<f64>,
    label:      Option<&str>,
    level:      QrLevel,
) -> Result<QrResult, String> {
    let uri    = payment_uri(address, amount_pkt, label);
    let qr_str = render_compact(&uri, level)?;
    let width  = qr_width(&uri, level)?;
    Ok(QrResult {
        data:   uri.clone(),
        qr_str,
        width,
        uri:    Some(uri),
    })
}

/// Lấy width (số modules) của QR cho một chuỗi
pub fn qr_width(data: &str, level: QrLevel) -> Result<usize, String> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), level.to_ec())
        .map_err(|e| e.to_string())?;
    Ok(code.width())
}

/// `cargo run -- qr <address> [amount]` — in QR ra terminal
pub fn cmd_qr(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: cargo run -- qr <pkt-address> [amount_pkt] [label]");
        eprintln!("Example: cargo run -- qr pkt1q... 10.5 \"donation\"");
        return;
    }

    let address = &args[0];
    let amount  = args.get(1).and_then(|s| s.parse::<f64>().ok());
    let label   = args.get(2).map(|s| s.as_str());

    let result = if amount.is_some() || label.is_some() {
        payment_qr(address, amount, label, QrLevel::Medium)
    } else {
        address_qr(address, QrLevel::Medium)
    };

    match result {
        Ok(r)  => r.print(),
        Err(e) => eprintln!("QR error: {}", e),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const ADDR: &str = "pkt1qtest000000000000000000000000000000000";

    // ── render_compact ────────────────────────────────────────────────────

    #[test]
    fn test_render_compact_not_empty() {
        let r = render_compact(ADDR, QrLevel::Medium).unwrap();
        assert!(!r.is_empty());
    }

    #[test]
    fn test_render_compact_contains_newlines() {
        let r = render_compact(ADDR, QrLevel::Medium).unwrap();
        assert!(r.contains('\n'));
    }

    #[test]
    fn test_render_compact_different_data_different_qr() {
        let r1 = render_compact("pkt1qaaaaa", QrLevel::Medium).unwrap();
        let r2 = render_compact("pkt1qbbbbb", QrLevel::Medium).unwrap();
        assert_ne!(r1, r2);
    }

    #[test]
    fn test_render_compact_low_level() {
        let r = render_compact(ADDR, QrLevel::Low).unwrap();
        assert!(!r.is_empty());
    }

    #[test]
    fn test_render_compact_high_level() {
        let r = render_compact(ADDR, QrLevel::High).unwrap();
        assert!(!r.is_empty());
    }

    #[test]
    fn test_render_compact_short_string() {
        let r = render_compact("pkt", QrLevel::Low).unwrap();
        assert!(!r.is_empty());
    }

    // ── render_full ───────────────────────────────────────────────────────

    #[test]
    fn test_render_full_not_empty() {
        let r = render_full(ADDR, QrLevel::Medium).unwrap();
        assert!(!r.is_empty());
    }

    #[test]
    fn test_render_full_contains_block_chars() {
        let r = render_full(ADDR, QrLevel::Medium).unwrap();
        assert!(r.contains('█'), "full render phải có ký tự ██");
    }

    #[test]
    fn test_render_full_has_multiple_lines() {
        let r = render_full(ADDR, QrLevel::Medium).unwrap();
        let lines: Vec<&str> = r.lines().collect();
        assert!(lines.len() > 10, "QR phải có ít nhất 10 dòng");
    }

    #[test]
    fn test_render_full_lines_even_width() {
        // Mỗi module = 2 chars → tổng width là chẵn
        let r = render_full(ADDR, QrLevel::Medium).unwrap();
        for line in r.lines() {
            // Đếm theo char width (not bytes vì có ██ là 3 bytes UTF-8)
            let chars: usize = line.chars().count();
            // Quiet zone margin + 2 * width + 2 = tổng chẵn
            assert_eq!(chars % 2, 0, "line width phải chẵn: '{}'", line);
        }
    }

    // ── payment_uri ───────────────────────────────────────────────────────

    #[test]
    fn test_payment_uri_address_only() {
        let uri = payment_uri(ADDR, None, None);
        assert_eq!(uri, format!("pkt:{}", ADDR));
    }

    #[test]
    fn test_payment_uri_with_amount() {
        let uri = payment_uri(ADDR, Some(10.5), None);
        assert!(uri.contains("amount=10.50000000"));
        assert!(uri.starts_with("pkt:"));
    }

    #[test]
    fn test_payment_uri_with_label() {
        let uri = payment_uri(ADDR, None, Some("donation"));
        assert!(uri.contains("label=donation"));
    }

    #[test]
    fn test_payment_uri_with_all_params() {
        let uri = payment_uri(ADDR, Some(5.0), Some("test payment"));
        assert!(uri.contains("amount="));
        assert!(uri.contains("label=test%20payment"));
        assert!(uri.contains('?'));
        assert!(uri.contains('&'));
    }

    #[test]
    fn test_payment_uri_zero_amount_excluded() {
        let uri = payment_uri(ADDR, Some(0.0), None);
        assert!(!uri.contains("amount"), "amount=0 không được xuất hiện");
    }

    #[test]
    fn test_payment_uri_empty_label_excluded() {
        let uri = payment_uri(ADDR, None, Some(""));
        assert!(!uri.contains("label"), "label='' không được xuất hiện");
    }

    // ── percent_encode ────────────────────────────────────────────────────

    #[test]
    fn test_percent_encode_space() {
        assert_eq!(percent_encode("hello world"), "hello%20world");
    }

    #[test]
    fn test_percent_encode_ampersand() {
        assert_eq!(percent_encode("a&b"), "a%26b");
    }

    #[test]
    fn test_percent_encode_no_special_chars() {
        assert_eq!(percent_encode("donation"), "donation");
    }

    #[test]
    fn test_percent_encode_hash() {
        assert_eq!(percent_encode("tx#1"), "tx%231");
    }

    // ── qr_width ──────────────────────────────────────────────────────────

    #[test]
    fn test_qr_width_positive() {
        let w = qr_width(ADDR, QrLevel::Medium).unwrap();
        assert!(w > 0);
    }

    #[test]
    fn test_qr_width_longer_data_larger_or_equal() {
        let w1 = qr_width("pkt", QrLevel::Medium).unwrap();
        let w2 = qr_width(ADDR, QrLevel::Medium).unwrap();
        assert!(w2 >= w1, "địa chỉ dài hơn → QR lớn hơn hoặc bằng");
    }

    #[test]
    fn test_qr_width_multiple_of_module_spec() {
        // QR version N có width = 17 + 4*N, tất cả đều lẻ ≥ 21
        let w = qr_width(ADDR, QrLevel::Medium).unwrap();
        assert!(w >= 21);
        assert_eq!((w - 17) % 4, 0, "width phải = 17 + 4*N");
    }

    // ── address_qr ────────────────────────────────────────────────────────

    #[test]
    fn test_address_qr_ok() {
        let r = address_qr(ADDR, QrLevel::Medium).unwrap();
        assert_eq!(r.data, ADDR);
        assert!(r.uri.is_none());
        assert!(!r.qr_str.is_empty());
        assert!(r.width > 0);
    }

    #[test]
    fn test_address_qr_no_uri_field() {
        let r = address_qr(ADDR, QrLevel::Low).unwrap();
        assert!(r.uri.is_none());
    }

    // ── payment_qr ────────────────────────────────────────────────────────

    #[test]
    fn test_payment_qr_has_uri() {
        let r = payment_qr(ADDR, Some(1.0), Some("test"), QrLevel::Medium).unwrap();
        assert!(r.uri.is_some());
        let uri = r.uri.unwrap();
        assert!(uri.starts_with("pkt:"));
        assert!(uri.contains("amount="));
    }

    #[test]
    fn test_payment_qr_no_amount_ok() {
        let r = payment_qr(ADDR, None, None, QrLevel::Medium).unwrap();
        assert!(r.uri.is_some());
        let uri = r.uri.unwrap();
        assert!(!uri.contains("amount"));
    }

    // ── QrLevel ───────────────────────────────────────────────────────────

    #[test]
    fn test_qr_level_default_is_medium() {
        assert_eq!(QrLevel::default(), QrLevel::Medium);
    }

    #[test]
    fn test_high_ec_larger_or_equal_qr() {
        let wl = qr_width(ADDR, QrLevel::Low).unwrap();
        let wh = qr_width(ADDR, QrLevel::High).unwrap();
        assert!(wh >= wl, "High EC → QR lớn hơn hoặc bằng Low EC");
    }
}
