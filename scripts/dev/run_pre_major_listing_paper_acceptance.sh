#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

: "${RUSTUP_TOOLCHAIN:=1.91.1}"
: "${PRE_MAJOR_LISTING_MIN_TRADE_SAMPLES:=30}"
: "${PRE_MAJOR_LISTING_MIN_WIN_RATE_PCT:=60}"

if [[ -z "${PRE_MAJOR_LISTING_PAPER_INPUT:-}" ]]; then
    echo "PRE_MAJOR_LISTING_PAPER_INPUT is required" >&2
    echo "Expected a JSON file containing ListingCatchupPaperSample[] or {\"samples\": [...], \"criteria\": {...}}." >&2
    exit 2
fi

if command -v rustup >/dev/null 2>&1; then
    : "${RUSTC:="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc)"}"
fi

export RUSTUP_TOOLCHAIN
export RUSTC
export PRE_MAJOR_LISTING_PAPER_INPUT
export PRE_MAJOR_LISTING_MIN_TRADE_SAMPLES
export PRE_MAJOR_LISTING_MIN_WIN_RATE_PCT

echo "==> running pre-major-listing paper acceptance"
echo "    input: ${PRE_MAJOR_LISTING_PAPER_INPUT}"
echo "    min trade samples: ${PRE_MAJOR_LISTING_MIN_TRADE_SAMPLES}"
echo "    min win rate pct: ${PRE_MAJOR_LISTING_MIN_WIN_RATE_PCT}"
echo "    live trading: disabled"

cargo run --bin pre_major_listing_paper_acceptance -- \
    --input "${PRE_MAJOR_LISTING_PAPER_INPUT}" \
    --min-trade-samples "${PRE_MAJOR_LISTING_MIN_TRADE_SAMPLES}" \
    --min-win-rate-pct "${PRE_MAJOR_LISTING_MIN_WIN_RATE_PCT}"
