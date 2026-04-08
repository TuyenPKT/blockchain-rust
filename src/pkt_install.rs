#![allow(dead_code)]
//! v24.0 — Node Onboarding
//!
//! Sinh `install.sh` để join OCEIF testnet/mainnet chỉ với 1 lệnh:
//!
//! ```bash
//! curl -sSL https://install.oceif.com | sh
//! # hoặc
//! blockchain-rust install-node [--mainnet] [--user <unix-user>]
//! ```
//!
//! install.sh sẽ:
//!   1. Detect OS (Linux x86_64 / ARM64 / macOS)
//!   2. Download binary release mới nhất từ GitHub
//!   3. Đặt vào /usr/local/bin/blockchain-rust
//!   4. Tạo user `oceif` (nếu chưa có)
//!   5. Tạo ~/.pkt/config.toml với network mặc định
//!   6. Cài systemd service (Linux) hoặc launchd plist (macOS)
//!   7. Enable + start service

use std::fmt::Write as _;

// ── Constants ─────────────────────────────────────────────────────────────────

const GITHUB_REPO:    &str = "TuyenPKT/blockchain-rust";
const DEFAULT_USER:   &str = "oceif";
const BINARY_NAME:    &str = "blockchain-rust";
const SERVICE_NAME:   &str = "oceif-node";
const DATA_DIR:       &str = "/var/lib/oceif";

// ── Config ────────────────────────────────────────────────────────────────────

pub struct InstallConfig {
    pub mainnet:   bool,
    pub unix_user: String,
}

impl Default for InstallConfig {
    fn default() -> Self {
        InstallConfig {
            mainnet:   false,
            unix_user: DEFAULT_USER.to_string(),
        }
    }
}

impl InstallConfig {
    pub fn from_args(args: &[String]) -> Self {
        let mainnet   = args.iter().any(|a| a == "--mainnet");
        let unix_user = args.windows(2)
            .find(|w| w[0] == "--user")
            .map(|w| w[1].clone())
            .unwrap_or_else(|| DEFAULT_USER.to_string());
        InstallConfig { mainnet, unix_user }
    }

    pub fn network_name(&self) -> &str {
        if self.mainnet { "mainnet" } else { "testnet" }
    }

    pub fn peer(&self) -> &str {
        if self.mainnet {
            "seed.oceif.com:64764"
        } else {
            "seed.testnet.oceif.com:8333"
        }
    }

    pub fn web_port(&self) -> u16 {
        if self.mainnet { 8081 } else { 8082 }
    }
}

// ── install.sh generator ──────────────────────────────────────────────────────

/// Sinh nội dung install.sh
pub fn generate_install_sh(cfg: &InstallConfig) -> String {
    let mut s = String::new();
    let network  = cfg.network_name();
    let peer     = cfg.peer();
    let web_port = cfg.web_port();
    let user     = &cfg.unix_user;

    let _ = write!(s, r#"#!/usr/bin/env sh
# OCEIF Node Installer — v24.0
# Usage: curl -sSL https://install.oceif.com | sh
# Or:    curl -sSL https://install.oceif.com | sh -s -- --mainnet
#
# Supported: Linux x86_64, Linux ARM64, macOS x86_64, macOS ARM64
set -e

REPO="{repo}"
BINARY="{binary}"
SERVICE="{service}"
NETWORK="{network}"
PEER="{peer}"
WEB_PORT="{web_port}"
INSTALL_DIR="/usr/local/bin"
DATA_DIR="{data_dir}"
USER="{user}"

# ── Detect OS & arch ──────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
      aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
      armv7l)  TARGET="armv7-unknown-linux-gnueabihf" ;;
      *) echo "Unsupported arch: $ARCH"; exit 1 ;;
    esac
    ;;
  Darwin)
    TARGET="universal-apple-darwin"
    ;;
  *)
    echo "Unsupported OS: $OS. Download manually from https://github.com/$REPO/releases"
    exit 1
    ;;
esac

echo "[install] OS=$OS ARCH=$ARCH TARGET=$TARGET"

# ── Check dependencies ────────────────────────────────────────────────────────
need_cmd() {{ command -v "$1" >/dev/null 2>&1 || {{ echo "Required: $1"; exit 1; }}; }}
need_cmd curl
need_cmd tar

# ── Download latest release ───────────────────────────────────────────────────
LATEST=$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\(.*\)".*/\1/')

if [ -z "$LATEST" ]; then
  echo "[install] Could not fetch latest version. Check https://github.com/$REPO/releases"
  exit 1
fi

echo "[install] Latest version: $LATEST"

ARCHIVE="${{BINARY}}-${{TARGET}}.tar.gz"
URL="https://github.com/$REPO/releases/download/$LATEST/$ARCHIVE"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "[install] Downloading $URL ..."
curl -sSL "$URL" -o "$TMP/$ARCHIVE"
tar -xzf "$TMP/$ARCHIVE" -C "$TMP"

# ── Install binary ────────────────────────────────────────────────────────────
if [ "$(id -u)" -eq 0 ]; then
  install -m 755 "$TMP/$BINARY" "$INSTALL_DIR/$BINARY"
  echo "[install] Binary installed to $INSTALL_DIR/$BINARY"
else
  echo "[install] Not root — installing to ~/.local/bin"
  mkdir -p "$HOME/.local/bin"
  install -m 755 "$TMP/$BINARY" "$HOME/.local/bin/$BINARY"
  INSTALL_DIR="$HOME/.local/bin"
  echo "[install] Make sure $HOME/.local/bin is in your PATH"
fi

# ── Create data dir & config ──────────────────────────────────────────────────
mkdir -p "$DATA_DIR"

CONFIG_DIR="$HOME/.pkt"
mkdir -p "$CONFIG_DIR"

if [ ! -f "$CONFIG_DIR/config.toml" ]; then
  cat > "$CONFIG_DIR/config.toml" << TOML
# OCEIF Node config — generated by install.sh
network  = "$NETWORK"
peer     = "$PEER"
web_port = $WEB_PORT
data_dir = "$DATA_DIR"
TOML
  echo "[install] Config written to $CONFIG_DIR/config.toml"
fi

# ── systemd (Linux root) ──────────────────────────────────────────────────────
if [ "$OS" = "Linux" ] && [ "$(id -u)" -eq 0 ] && command -v systemctl >/dev/null 2>&1; then
  # Create user if not exists
  if ! id "$USER" >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin "$USER"
    echo "[install] Created system user: $USER"
  fi
  chown -R "$USER:$USER" "$DATA_DIR"

  cat > "/etc/systemd/system/$SERVICE.service" << SERVICE
[Unit]
Description=OCEIF Node ($NETWORK)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$USER
WorkingDirectory=$DATA_DIR
ExecStart=$INSTALL_DIR/$BINARY fullnode $WEB_PORT $PEER
Restart=on-failure
RestartSec=10s
Environment=RUST_LOG=info
LimitNOFILE=65536
ProtectSystem=strict
ReadWritePaths=$DATA_DIR /home/$USER
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
SERVICE

  systemctl daemon-reload
  systemctl enable --now "$SERVICE"
  echo "[install] Service enabled: $SERVICE"
  echo "[install] Status: systemctl status $SERVICE"

# ── launchd (macOS) ───────────────────────────────────────────────────────────
elif [ "$OS" = "Darwin" ]; then
  PLIST="$HOME/Library/LaunchAgents/com.oceif.node.plist"
  cat > "$PLIST" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>             <string>com.oceif.node</string>
  <key>ProgramArguments</key>
  <array>
    <string>$INSTALL_DIR/$BINARY</string>
    <string>fullnode</string>
    <string>$WEB_PORT</string>
    <string>$PEER</string>
  </array>
  <key>RunAtLoad</key>         <true/>
  <key>KeepAlive</key>         <true/>
  <key>StandardOutPath</key>   <string>$HOME/.pkt/node.log</string>
  <key>StandardErrorPath</key> <string>$HOME/.pkt/node.err</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>RUST_LOG</key> <string>info</string>
  </dict>
</dict>
</plist>
PLIST
  launchctl load "$PLIST"
  echo "[install] launchd agent loaded: com.oceif.node"
fi

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════════╗"
echo "║  OCEIF Node installed successfully!                  ║"
echo "╟──────────────────────────────────────────────────────╢"
echo "║  Network : $NETWORK"
echo "║  Peer    : $PEER"
echo "║  Web UI  : http://localhost:$WEB_PORT"
echo "║  Config  : $CONFIG_DIR/config.toml"
echo "╟──────────────────────────────────────────────────────╢"
echo "║  Check sync:                                         ║"
echo "║    curl http://localhost:$WEB_PORT/api/testnet/sync-status"
echo "╚══════════════════════════════════════════════════════╝"
"#,
        repo     = GITHUB_REPO,
        binary   = BINARY_NAME,
        service  = SERVICE_NAME,
        network  = network,
        peer     = peer,
        web_port = web_port,
        data_dir = DATA_DIR,
        user     = user,
    );
    s
}

// ── config.toml template ──────────────────────────────────────────────────────

/// Sinh nội dung ~/.pkt/config.toml mặc định
pub fn generate_config_toml(cfg: &InstallConfig) -> String {
    format!(
        r#"# OCEIF Node config
# Tạo bởi: blockchain-rust install-node
network  = "{network}"
peer     = "{peer}"
web_port = {web_port}
data_dir = "{data_dir}"
rust_log = "info"
"#,
        network  = cfg.network_name(),
        peer     = cfg.peer(),
        web_port = cfg.web_port(),
        data_dir = DATA_DIR,
    )
}

// ── CLI entry point ───────────────────────────────────────────────────────────

/// `blockchain-rust install-node [--mainnet] [--user <u>] [--print-sh|--print-config]`
pub fn cmd_install_node(args: &[String]) {
    let cfg = InstallConfig::from_args(args);

    let print_sh     = args.iter().any(|a| a == "--print-sh");
    let print_config = args.iter().any(|a| a == "--print-config");

    if print_sh {
        print!("{}", generate_install_sh(&cfg));
        return;
    }

    if print_config {
        print!("{}", generate_config_toml(&cfg));
        return;
    }

    // Default: print onboarding guide
    let network  = cfg.network_name();
    let peer     = cfg.peer();
    let web_port = cfg.web_port();

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  OCEIF Node Onboarding — v24.0                               ║");
    println!("╟──────────────────────────────────────────────────────────────╢");
    println!("║  Network : {network:<50}║");
    println!("║  Peer    : {peer:<50}║");
    println!("║  Web UI  : http://localhost:{web_port:<44}║");
    println!("╟──────────────────────────────────────────────────────────────╢");
    println!("║                                                              ║");
    println!("║  Option 1 — Install script (Linux/macOS):                   ║");
    println!("║    curl -sSL https://install.oceif.com | sh                 ║");
    println!("║                                                              ║");
    println!("║  Option 2 — cargo install:                                  ║");
    println!("║    cargo install --git https://github.com/{GITHUB_REPO}     ║");
    println!("║    blockchain-rust fullnode {web_port} {peer}                      ║");
    println!("║                                                              ║");
    println!("║  Option 3 — Build from source:                              ║");
    println!("║    git clone https://github.com/{GITHUB_REPO}               ║");
    println!("║    cd blockchain-rust && cargo build --release               ║");
    println!("║    ./target/release/blockchain-rust fullnode {web_port} {peer}     ║");
    println!("║                                                              ║");
    println!("╟──────────────────────────────────────────────────────────────╢");
    println!("║  Print install.sh : blockchain-rust install-node --print-sh  ║");
    println!("║  Print config.toml: blockchain-rust install-node --print-config ║");
    println!("║  Mainnet          : blockchain-rust install-node --mainnet    ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn testnet_cfg() -> InstallConfig {
        InstallConfig::default()
    }

    fn mainnet_cfg() -> InstallConfig {
        InstallConfig { mainnet: true, unix_user: "oceif".to_string() }
    }

    // ── InstallConfig ─────────────────────────────────────────────────────────

    #[test]
    fn testnet_defaults() {
        let cfg = testnet_cfg();
        assert_eq!(cfg.network_name(), "testnet");
        assert_eq!(cfg.peer(), "seed.testnet.oceif.com:8333");
        assert_eq!(cfg.web_port(), 8082);
    }

    #[test]
    fn mainnet_config() {
        let cfg = mainnet_cfg();
        assert_eq!(cfg.network_name(), "mainnet");
        assert_eq!(cfg.peer(), "seed.oceif.com:64764");
        assert_eq!(cfg.web_port(), 8081);
    }

    #[test]
    fn from_args_mainnet_flag() {
        let args = vec!["--mainnet".to_string()];
        let cfg = InstallConfig::from_args(&args);
        assert!(cfg.mainnet);
    }

    #[test]
    fn from_args_user_flag() {
        let args = vec!["--user".to_string(), "pktsync".to_string()];
        let cfg = InstallConfig::from_args(&args);
        assert_eq!(cfg.unix_user, "pktsync");
    }

    #[test]
    fn from_args_no_flags() {
        let cfg = InstallConfig::from_args(&[]);
        assert!(!cfg.mainnet);
        assert_eq!(cfg.unix_user, DEFAULT_USER);
    }

    // ── generate_install_sh ───────────────────────────────────────────────────

    #[test]
    fn install_sh_contains_shebang() {
        let sh = generate_install_sh(&testnet_cfg());
        assert!(sh.starts_with("#!/usr/bin/env sh"));
    }

    #[test]
    fn install_sh_testnet_peer() {
        let sh = generate_install_sh(&testnet_cfg());
        assert!(sh.contains("seed.testnet.oceif.com:8333"));
    }

    #[test]
    fn install_sh_mainnet_peer() {
        let sh = generate_install_sh(&mainnet_cfg());
        assert!(sh.contains("seed.oceif.com:64764"));
    }

    #[test]
    fn install_sh_contains_systemd_block() {
        let sh = generate_install_sh(&testnet_cfg());
        assert!(sh.contains("[Unit]"));
        assert!(sh.contains("[Service]"));
        assert!(sh.contains("[Install]"));
    }

    #[test]
    fn install_sh_contains_launchd_block() {
        let sh = generate_install_sh(&testnet_cfg());
        assert!(sh.contains("com.oceif.node"));
        assert!(sh.contains("launchctl load"));
    }

    #[test]
    fn install_sh_contains_github_repo() {
        let sh = generate_install_sh(&testnet_cfg());
        assert!(sh.contains(GITHUB_REPO));
    }

    #[test]
    fn install_sh_set_e() {
        let sh = generate_install_sh(&testnet_cfg());
        assert!(sh.contains("set -e"));
    }

    #[test]
    fn install_sh_arch_detection() {
        let sh = generate_install_sh(&testnet_cfg());
        assert!(sh.contains("x86_64-unknown-linux-gnu"));
        assert!(sh.contains("aarch64-unknown-linux-gnu"));
        assert!(sh.contains("universal-apple-darwin"));
    }

    #[test]
    fn install_sh_no_root_fallback() {
        let sh = generate_install_sh(&testnet_cfg());
        assert!(sh.contains("~/.local/bin"));
    }

    // ── generate_config_toml ──────────────────────────────────────────────────

    #[test]
    fn config_toml_testnet() {
        let toml = generate_config_toml(&testnet_cfg());
        assert!(toml.contains("network  = \"testnet\""));
        assert!(toml.contains("web_port = 8082"));
    }

    #[test]
    fn config_toml_mainnet() {
        let toml = generate_config_toml(&mainnet_cfg());
        assert!(toml.contains("network  = \"mainnet\""));
        assert!(toml.contains("web_port = 8081"));
    }

    #[test]
    fn config_toml_valid_toml_syntax() {
        // Basic TOML validity: no unmatched quotes, has = signs
        let toml = generate_config_toml(&testnet_cfg());
        assert!(toml.contains(" = "));
        let quote_count = toml.chars().filter(|&c| c == '"').count();
        assert_eq!(quote_count % 2, 0, "unmatched quotes in config toml");
    }
}
