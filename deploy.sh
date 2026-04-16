#!/bin/bash
# deploy.sh — rsync source → build trên VPS → lưu binary → xóa target/
# Chạy từ máy local: bash deploy.sh

set -e

SERVER="180.93.1.235"
USER="tuyenpkt"
REMOTE_DIR="~/blockchain-rust"
BINARY_DIR="~/bin"

echo "=== [1/3] Sync source lên server (bỏ qua target/) ==="
ssh "$USER@$SERVER" "mkdir -p $REMOTE_DIR $BINARY_DIR"
rsync -az --exclude target/ --exclude .git/ \
    ./ "$USER@$SERVER:$REMOTE_DIR/"

echo "=== [2/3] Build release trên server ==="
ssh "$USER@$SERVER" bash << 'REMOTE'
set -e
cd ~/blockchain-rust
source "$HOME/.cargo/env" 2>/dev/null || true

cargo build --release 2>&1 | tail -5

# Lưu binary ra ngoài target/
cp target/release/blockchain-rust ~/bin/blockchain-rust
echo "✅ Binary: $(ls -lh ~/bin/blockchain-rust)"

# Xóa target/ ngay sau khi lấy binary
rm -rf target/
echo "✅ Đã xóa target/ — disk freed"
df -h / | tail -1
REMOTE

echo "=== [3/3] Restart services ==="
ssh "$USER@$SERVER" bash << 'REMOTE'
set -e
# Cập nhật ExecStart trong service files nếu cần
sudo systemctl daemon-reload
sudo systemctl restart blockchain-node.service pkt-fullnode.service 2>/dev/null || true
sudo systemctl status blockchain-node.service pkt-fullnode.service --no-pager -l | tail -10
REMOTE

echo ""
echo "✅ Deploy xong. Binary tại ~/bin/blockchain-rust trên VPS."
