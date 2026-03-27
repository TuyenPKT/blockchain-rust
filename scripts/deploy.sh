#!/usr/bin/env bash
# scripts/deploy.sh — Deploy lên VPS oceif.com
#
# Hai mode:
#   --source  (default): rsync source → build release trên server
#   --binary:            cross-compile local → scp binary → restart
#
# Config qua env vars:
#   PKT_SERVER    (default: oceif.com)
#   PKT_USER      (default: tuyenpkt)
#   PKT_REMOTE    (default: ~/blockchain-rust)
#
# Ví dụ:
#   bash scripts/deploy.sh
#   bash scripts/deploy.sh --binary
#   PKT_USER=root bash scripts/deploy.sh

set -euo pipefail

SERVER="${PKT_SERVER:-oceif.com}"
USER="${PKT_USER:-tuyenpkt}"
REMOTE="${PKT_REMOTE:-~/blockchain-rust}"
BINARY="blockchain-rust"
TARGET="x86_64-unknown-linux-musl"
LOCAL_BIN="target/${TARGET}/release/${BINARY}"

MODE="source"
for arg in "$@"; do
    case "${arg}" in
        --binary) MODE="binary" ;;
        --source) MODE="source" ;;
        *)
            echo "Usage: $0 [--source | --binary]"
            echo "  --source  rsync source + build on server (default)"
            echo "  --binary  cross-compile locally + scp binary"
            exit 1
            ;;
    esac
done

echo "==> Deploy mode: ${MODE}  |  ${USER}@${SERVER}:${REMOTE}"
echo ""

# ── Mode: source ──────────────────────────────────────────────────────────────
if [[ "${MODE}" == "source" ]]; then

    echo "[1/3] Tạo thư mục trên server…"
    ssh "${USER}@${SERVER}" "mkdir -p ${REMOTE}"

    echo "[2/3] rsync source (exclude target/, .git/)…"
    rsync -avz --progress \
        --exclude='target/' \
        --exclude='.git/' \
        --exclude='*.DS_Store' \
        ./ "${USER}@${SERVER}:${REMOTE}/"

    echo "[3/3] Build release + restart services trên server…"
    ssh "${USER}@${SERVER}" bash <<REMOTE
set -e
cd ${REMOTE}

# Cài Rust nếu chưa có
if ! command -v cargo &>/dev/null; then
    echo "  → Cài Rust…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
    source "\$HOME/.cargo/env"
fi

echo "  → cargo build --release…"
cargo build --release 2>&1 | tail -5

SIZE=\$(du -sh target/release/${BINARY} | cut -f1)
echo "  ✅ Built: target/release/${BINARY} (\${SIZE})"

# Restart services nếu tồn tại
for svc in pkt-sync blockchain-api; do
    if systemctl is-enabled "\${svc}" &>/dev/null 2>&1; then
        sudo systemctl restart "\${svc}" && echo "  ✅ restarted \${svc}"
    fi
done
REMOTE

# ── Mode: binary ──────────────────────────────────────────────────────────────
elif [[ "${MODE}" == "binary" ]]; then

    echo "[1/3] Cross-compile static Linux binary…"
    bash scripts/build-linux.sh

    echo ""
    echo "[2/3] scp binary lên server…"
    ssh "${USER}@${SERVER}" "mkdir -p ${REMOTE}/target/release"
    scp "${LOCAL_BIN}" "${USER}@${SERVER}:${REMOTE}/target/release/${BINARY}"

    echo "[3/3] Restart services…"
    ssh "${USER}@${SERVER}" bash <<REMOTE
set -e
chmod +x ${REMOTE}/target/release/${BINARY}
SIZE=\$(du -sh ${REMOTE}/target/release/${BINARY} | cut -f1)
echo "  ✅ Binary: ${REMOTE}/target/release/${BINARY} (\${SIZE})"

for svc in pkt-sync blockchain-api; do
    if systemctl is-enabled "\${svc}" &>/dev/null 2>&1; then
        sudo systemctl restart "\${svc}" && echo "  ✅ restarted \${svc}"
    fi
done
REMOTE
fi

echo ""
echo "✅  Deploy hoàn tất → ${USER}@${SERVER}:${REMOTE}"
