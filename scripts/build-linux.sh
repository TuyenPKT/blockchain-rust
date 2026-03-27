#!/usr/bin/env bash
# scripts/build-linux.sh — Cross-compile static Linux x86_64 binary (musl)
#
# Ưu tiên sử dụng theo thứ tự:
#   1. `cross`  (cargo install cross)  — Docker-based, không cần musl-cross
#   2. native musl target + musl-cross linker (brew install musl-cross)
#
# Sau khi build xong:
#   target/x86_64-unknown-linux-musl/release/blockchain-rust  ← static binary
#
# Cài đặt nhanh:
#   cargo install cross --locked   # chạy một lần
#   # Hoặc:
#   rustup target add x86_64-unknown-linux-musl
#   brew install FiloSottile/musl-cross/musl-cross

set -euo pipefail

TARGET="x86_64-unknown-linux-musl"
BINARY="blockchain-rust"
OUT="target/${TARGET}/release/${BINARY}"

echo "==> Build Linux static binary (${TARGET})"

# ── Option 1: cross (Docker) ──────────────────────────────────────────────────
if command -v cross &>/dev/null; then
    echo "    Dùng: cross (Docker-based)"
    cross build --release --target "${TARGET}"

# ── Option 2: native target + musl-cross linker ───────────────────────────────
elif rustup target list --installed 2>/dev/null | grep -q "${TARGET}"; then
    echo "    Dùng: native rustup target"

    if [[ "$(uname)" == "Darwin" ]]; then
        MUSL_GCC="x86_64-linux-musl-gcc"
        if ! command -v "${MUSL_GCC}" &>/dev/null; then
            echo ""
            echo "❌  musl-cross không tìm thấy. Cài:"
            echo "    brew install FiloSottile/musl-cross/musl-cross"
            echo ""
            echo "    Hoặc dùng 'cross' (không cần musl-cross):"
            echo "    cargo install cross --locked"
            exit 1
        fi
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="${MUSL_GCC}" \
            cargo build --release --target "${TARGET}"
    else
        # Linux host: target đã có sẵn linker
        cargo build --release --target "${TARGET}"
    fi

# ── Không tìm thấy cách build ─────────────────────────────────────────────────
else
    echo ""
    echo "❌  Chưa cài đặt công cụ cross-compile. Chạy một trong hai:"
    echo ""
    echo "    Option A — cross (đơn giản hơn, cần Docker):"
    echo "    cargo install cross --locked"
    echo ""
    echo "    Option B — native musl target (không cần Docker):"
    echo "    rustup target add ${TARGET}"
    echo "    brew install FiloSottile/musl-cross/musl-cross"
    exit 1
fi

# ── Kết quả ───────────────────────────────────────────────────────────────────
if [[ ! -f "${OUT}" ]]; then
    echo "❌  Build thất bại: ${OUT} không tồn tại"
    exit 1
fi

SIZE=$(du -sh "${OUT}" | cut -f1)
SHA=$(shasum -a 256 "${OUT}" | awk '{print $1}')
echo ""
echo "✅  Built: ${OUT}"
echo "    Size:   ${SIZE}"
echo "    SHA256: ${SHA}"
