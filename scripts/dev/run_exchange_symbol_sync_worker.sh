#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

: "${RUSTUP_TOOLCHAIN:=1.91.1}"
: "${QUANT_CORE_DATABASE_URL:=postgres://postgres:postgres123@localhost:5432/quant_core}"
: "${EXCHANGE_SYMBOL_SOURCES:=binance okx bitget gate kucoin}"
: "${EXCHANGE_SYMBOL_SYNC_INTERVAL_SECS:=60}"
: "${EXCHANGE_LISTING_SIGNAL_SUBMIT:=0}"

if command -v rustup >/dev/null 2>&1; then
    : "${RUSTC:="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc)"}"
fi

export RUSTUP_TOOLCHAIN
export RUSTC
export QUANT_CORE_DATABASE_URL
export EXCHANGE_SYMBOL_SOURCES
export EXCHANGE_SYMBOL_SYNC_INTERVAL_SECS
export EXCHANGE_LISTING_SIGNAL_SUBMIT
export IS_RUN_EXCHANGE_SYMBOL_SYNC_WORKER=true
export EXCHANGE_SYMBOL_SYNC_WORKER_ONLY=true

echo "==> starting exchange symbol sync worker"
echo "    sources: ${EXCHANGE_SYMBOL_SOURCES}"
echo "    interval_secs: ${EXCHANGE_SYMBOL_SYNC_INTERVAL_SECS}"
echo "    quant_core: ${QUANT_CORE_DATABASE_URL}"
echo "    submit listing signal: ${EXCHANGE_LISTING_SIGNAL_SUBMIT}"

cargo run --bin rust_quant
