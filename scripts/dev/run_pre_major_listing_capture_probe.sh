#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

: "${RUSTUP_TOOLCHAIN:=1.91.1}"
: "${PRE_MAJOR_LISTING_CAPTURE_INPUT:?set PRE_MAJOR_LISTING_CAPTURE_INPUT to a capture request JSON file}"
: "${PRE_MAJOR_LISTING_CAPTURE_EXCHANGES:=bitget,bybit,gate}"

cd "${REPO_ROOT}"

ARGS=(
    --input "${PRE_MAJOR_LISTING_CAPTURE_INPUT}"
    --exchanges "${PRE_MAJOR_LISTING_CAPTURE_EXCHANGES}"
)

if [[ -n "${PRE_MAJOR_LISTING_ORDERBOOK_FIXTURE:-}" ]]; then
    ARGS+=(--orderbook-fixture "${PRE_MAJOR_LISTING_ORDERBOOK_FIXTURE}")
fi

if [[ -n "${PRE_MAJOR_LISTING_CAPTURE_OUTPUT:-}" ]]; then
    cargo run --bin pre_major_listing_capture_probe -- "${ARGS[@]}" \
        > "${PRE_MAJOR_LISTING_CAPTURE_OUTPUT}"
    echo "probe seed written: ${PRE_MAJOR_LISTING_CAPTURE_OUTPUT}"
else
    cargo run --bin pre_major_listing_capture_probe -- "${ARGS[@]}"
fi

echo "live trading: disabled"
