#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

: "${RUSTUP_TOOLCHAIN:=1.91.1}"
: "${EXCHANGE_SYMBOL_SOURCE:=binance}"
: "${QUANT_CORE_DATABASE_URL:=postgres://postgres:postgres123@localhost:5432/quant_core}"
: "${EXCHANGE_LISTING_SIGNAL_SUBMIT:=0}"

if command -v rustup >/dev/null 2>&1; then
    : "${RUSTC:="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc)"}"
fi

export RUSTUP_TOOLCHAIN
export RUSTC
export EXCHANGE_SYMBOL_SOURCE
export QUANT_CORE_DATABASE_URL
export EXCHANGE_LISTING_SIGNAL_SUBMIT

echo "==> syncing exchange symbols"
echo "    source: ${EXCHANGE_SYMBOL_SOURCE}"
echo "    quant_core: ${QUANT_CORE_DATABASE_URL}"
echo "    submit listing signal: ${EXCHANGE_LISTING_SIGNAL_SUBMIT}"

cargo run --bin sync_exchange_symbols
