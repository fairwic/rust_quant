#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

: "${RUSTUP_TOOLCHAIN:=1.91.1}"
: "${PRE_MAJOR_LISTING_PROBE_INPUT:?set PRE_MAJOR_LISTING_PROBE_INPUT to a probe-seeds JSON file}"

cd "${REPO_ROOT}"

if [[ -n "${PRE_MAJOR_LISTING_SAMPLES_OUTPUT:-}" ]]; then
    cargo run --bin pre_major_listing_paper_samples_from_probe -- \
        --input "${PRE_MAJOR_LISTING_PROBE_INPUT}" \
        > "${PRE_MAJOR_LISTING_SAMPLES_OUTPUT}"
    echo "paper samples written: ${PRE_MAJOR_LISTING_SAMPLES_OUTPUT}"
else
    cargo run --bin pre_major_listing_paper_samples_from_probe -- \
        --input "${PRE_MAJOR_LISTING_PROBE_INPUT}"
fi

echo "live trading: disabled"
