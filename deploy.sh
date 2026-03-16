#!/bin/bash
# deploy.sh — build release + copy lên server oceif.com
# Chạy từ máy local: bash deploy.sh

set -e

SERVER="oceif.com"
USER="tuyenpkt"      # đổi nếu dùng user khác
REMOTE_DIR="~/blockchain-rust"
BINARY="blockchain-rust"

echo "=== [1/3] Build release binary (macOS → Linux cross-compile) ==="
# Nếu chưa có target linux: rustup target add x86_64-unknown-linux-musl
# Hoặc build thẳng trên server (xem bên dưới)

echo ""
echo "⚠️  Khuyến nghị: build trực tiếp trên server để tránh cross-compile"
echo "   Chạy:  ssh $USER@$SERVER 'bash -s' < deploy_server.sh"
echo ""

echo "=== [2/3] Copy source lên server ==="
ssh "$USER@$SERVER" "mkdir -p $REMOTE_DIR"
rsync -avz --exclude target --exclude .git \
    ./ "$USER@$SERVER:$REMOTE_DIR/"

echo ""
echo "=== [3/3] Build + start trên server ==="
ssh "$USER@$SERVER" << 'REMOTE'
cd ~/blockchain-rust

# Cài Rust nếu chưa có
if ! command -v cargo &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Build release
cargo build --release
echo "✅ Build xong: $(ls -lh target/release/blockchain-rust)"

# Setup systemd
cp ~/blockchain-rust/blockchain-node.service /etc/systemd/system/
cp ~/blockchain-rust/blockchain-api.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable blockchain-node blockchain-api
systemctl restart blockchain-node blockchain-api
echo "✅ Services started"
systemctl status blockchain-node blockchain-api --no-pager
REMOTE
