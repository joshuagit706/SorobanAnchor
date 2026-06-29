#!/bin/bash
# Fully isolated reproducible build verification for SorobanAnchor WASM output.
# Two back-to-back clean builds in separate temp directories with distinct
# CARGO_HOME/RUSTUP_HOME paths; compares SHA-256 digests of the output WASM.
# Exits 0 on match, 1 on mismatch.

set -e

WASM_TARGET=wasm32-unknown-unknown
WASM_REL_PATH=target/${WASM_TARGET}/release/anchorkit.wasm

# ── Toolchain verification ────────────────────────────────────────────────────
TOOLCHAIN_FILE=""
for f in rust-toolchain.toml rust-toolchain; do
    if [ -f "$f" ]; then TOOLCHAIN_FILE="$f"; break; fi
done

if [ -n "$TOOLCHAIN_FILE" ]; then
    CHANNEL=$(grep -m1 'channel' "$TOOLCHAIN_FILE" 2>/dev/null | cut -d'"' -f2 || true)
    if [ -n "$CHANNEL" ]; then
        echo "=== Toolchain: $CHANNEL (from $TOOLCHAIN_FILE) ==="
        if ! rustup toolchain list 2>/dev/null | grep -q "$CHANNEL"; then
            echo "ERROR: Rust toolchain '$CHANNEL' not installed. Run: rustup toolchain install $CHANNEL"
            exit 1
        fi
    fi
fi

# ── Temp directories ──────────────────────────────────────────────────────────
BUILD_DIR_A=$(mktemp -d)
BUILD_DIR_B=$(mktemp -d)

cleanup() {
    rm -rf "$BUILD_DIR_A" "$BUILD_DIR_B"
}
trap cleanup EXIT

echo "=== Reproducible Build Verification ==="
echo "Build A: $BUILD_DIR_A"
echo "Build B: $BUILD_DIR_B"

# ── Helper: run one isolated build ───────────────────────────────────────────
run_build() {
    local DIR="$1"
    local LABEL="$2"
    echo ""
    echo "--- $LABEL ---"
    rsync -a --exclude target --exclude .git . "$DIR/"
    (
        export CARGO_HOME="$DIR/.cargo"
        export RUSTUP_HOME="$DIR/.rustup"
        cd "$DIR"
        cargo build --release --target "$WASM_TARGET" --no-default-features --features wasm 2>&1
    )
    sha256sum "$DIR/$WASM_REL_PATH" | awk '{print $1}'
}

HASH_A=$(run_build "$BUILD_DIR_A" "Build A")
echo "Build A hash: $HASH_A"

HASH_B=$(run_build "$BUILD_DIR_B" "Build B")
echo "Build B hash: $HASH_B"

# ── Compare raw WASM ──────────────────────────────────────────────────────────
echo ""
echo "=== Raw WASM comparison ==="
if [ "$HASH_A" = "$HASH_B" ]; then
    echo "✅ PASS: raw WASM builds are reproducible"
    echo "   Hash: $HASH_A"
    RAW_PASS=0
else
    echo "❌ FAIL: raw WASM builds differ"
    echo "   Build A: $HASH_A"
    echo "   Build B: $HASH_B"
    RAW_PASS=1
fi

# ── Optional wasm-opt comparison ─────────────────────────────────────────────
OPT_PASS=0
if command -v wasm-opt >/dev/null 2>&1; then
    echo ""
    echo "=== Optimised WASM comparison (wasm-opt -Oz) ==="
    OPT_A="$BUILD_DIR_A/opt.wasm"
    OPT_B="$BUILD_DIR_B/opt.wasm"
    wasm-opt -Oz "$BUILD_DIR_A/$WASM_REL_PATH" -o "$OPT_A"
    wasm-opt -Oz "$BUILD_DIR_B/$WASM_REL_PATH" -o "$OPT_B"
    OPT_HASH_A=$(sha256sum "$OPT_A" | awk '{print $1}')
    OPT_HASH_B=$(sha256sum "$OPT_B" | awk '{print $1}')
    if [ "$OPT_HASH_A" = "$OPT_HASH_B" ]; then
        echo "✅ PASS: optimised WASM builds are reproducible"
        echo "   Hash: $OPT_HASH_A"
    else
        echo "❌ FAIL: optimised WASM builds differ"
        echo "   Build A: $OPT_HASH_A"
        echo "   Build B: $OPT_HASH_B"
        OPT_PASS=1
    fi
fi

exit $((RAW_PASS | OPT_PASS))
